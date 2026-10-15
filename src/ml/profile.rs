//! What vlt needs to know about a model, read from the files its Hugging Face repo ships:
//! `config.json` (architecture, sizes) and the sentence-transformers files (pooling, prompts,
//! sequence length). Any repo of a supported architecture works; nothing lists models by name.

use std::io::Read;
use std::path::{Path, PathBuf};

use candle_core::DType;
use serde::Deserialize;

use crate::config::{EmbeddingOptions, RerankerOptions};
use crate::installer::models::{read_lock, Lock};
use crate::{Result, VltError};

/// The branch, tag or commit downloaded when the config names none.
pub const DEFAULT_REVISION: &str = "main";
/// Longest input, in tokens, an embedder or reranker is given; chunks are cut to this size.
pub const MAX_TOKENS: usize = 512;
/// Query instruction of the English BGE models, which their repos do not declare.
const BGE_EN_QUERY_PREFIX: &str = "Represent this sentence for searching relevant passages: ";
/// The embedding fingerprint of every index built by vlt 0.1.0, which stored none: 0.1.0 only
/// supported `BAAI/bge-small-en-v1.5` at this commit.
pub const LEGACY_FINGERPRINT: &str =
    "5c38ec7c405ec4b44b94cc5a9bb96e735b38267a pooling=cls document_prefix=\"\" max_tokens=512 precision=f32";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Pooling {
    /// The first token's vector (BERT-style `[CLS]`).
    Cls,
    /// The mean of the token vectors, padding excluded.
    Mean,
    /// The last token's vector (decoder models, whose last token has seen the whole text).
    Last,
}

impl Pooling {
    fn name(self) -> &'static str {
        match self {
            Pooling::Cls => "cls",
            Pooling::Mean => "mean",
            Pooling::Last => "last",
        }
    }
}

/// The float type a model computes in. Candle has no BF16 matrix product on CPU, so BF16
/// weights run as F16: as small, more precise, with less range.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Precision {
    F32,
    F16,
}

impl Precision {
    pub fn dtype(self) -> DType {
        match self {
            Precision::F32 => DType::F32,
            Precision::F16 => DType::F16,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Precision::F32 => "f32",
            Precision::F16 => "f16",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmbeddingArch {
    Bert,
    XlmRoberta,
    Qwen3,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RerankerArch {
    Bert,
    XlmRoberta,
    DebertaV2,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EmbeddingProfile {
    pub arch: EmbeddingArch,
    pub dim: usize,
    pub pooling: Pooling,
    pub query_prefix: String,
    pub document_prefix: String,
    pub max_tokens: usize,
    /// The downloaded commit.
    pub commit: String,
    /// The `.safetensors` files holding the weights.
    pub weights: Vec<PathBuf>,
    pub precision: Precision,
}

impl EmbeddingProfile {
    /// Everything that shapes the stored vectors besides the model id and dimension; an index
    /// built under another fingerprint must be rebuilt. The query prefix is left out: it only
    /// affects queries, never stored vectors.
    pub fn fingerprint(&self) -> String {
        format!(
            "{} pooling={} document_prefix={:?} max_tokens={} precision={}",
            self.commit,
            self.pooling.name(),
            self.document_prefix,
            self.max_tokens,
            self.precision.name()
        )
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RerankerProfile {
    pub arch: RerankerArch,
    pub max_tokens: usize,
    pub weights: Vec<PathBuf>,
    pub precision: Precision,
}

pub fn model_dir(models_dir: &Path, id: &str) -> PathBuf {
    models_dir.join(id.replace('/', "--"))
}

/// A Hugging Face repo id, `<owner>/<name>`; it names a directory under the models dir.
pub fn validate_model_id(id: &str) -> Result<()> {
    let part_ok = |p: &str| {
        !p.is_empty()
            && p.len() <= 96
            && !p.starts_with(['.', '-'])
            && p.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    };
    match id.split_once('/') {
        Some((owner, name)) if part_ok(owner) && part_ok(name) => Ok(()),
        _ => Err(VltError::Config(format!(
            "invalid model id `{id}`: expected `<owner>/<name>` as on Hugging Face, e.g. `BAAI/bge-small-en-v1.5`"
        ))),
    }
}

/// A branch, tag or commit of a Hugging Face repo, such as `main`, `v1.0` or `refs/pr/3`.
pub fn validate_revision(revision: &str) -> Result<()> {
    let ok = !revision.is_empty()
        && revision.len() <= 128
        && !revision.starts_with('/')
        && !revision.split('/').any(|s| s.is_empty() || s == "." || s == "..")
        && revision.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '/'));
    if ok {
        Ok(())
    } else {
        Err(VltError::Config(format!("invalid model revision `{revision}`: expected a branch, tag or commit")))
    }
}

#[derive(Deserialize)]
struct HfConfig {
    model_type: Option<String>,
    #[serde(default)]
    architectures: Vec<String>,
    hidden_size: Option<usize>,
    max_position_embeddings: Option<usize>,
    pad_token_id: Option<usize>,
    id2label: Option<serde_json::Map<String, serde_json::Value>>,
    num_labels: Option<usize>,
}

impl HfConfig {
    fn model_type(&self) -> &str {
        self.model_type.as_deref().unwrap_or("unknown")
    }

    fn is_classifier(&self) -> bool {
        self.architectures.iter().any(|a| a.ends_with("ForSequenceClassification"))
    }

    /// Longest sequence the position embeddings cover, capped at `MAX_TOKENS`. RoBERTa-style
    /// models start positions after the pad id, which costs them `pad_token_id + 1` positions.
    fn max_tokens(&self) -> usize {
        let positions = self.max_position_embeddings.unwrap_or(MAX_TOKENS);
        let usable = match self.model_type() {
            "xlm-roberta" => positions.saturating_sub(self.pad_token_id.unwrap_or(1) + 1),
            _ => positions,
        };
        usable.min(MAX_TOKENS)
    }
}

#[derive(Deserialize)]
struct StModule {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    path: String,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct StPooling {
    pooling_mode_cls_token: bool,
    pooling_mode_mean_tokens: bool,
    pooling_mode_max_tokens: bool,
    pooling_mode_mean_sqrt_len_tokens: bool,
    pooling_mode_weightedmean_tokens: bool,
    pooling_mode_lasttoken: bool,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct StConfig {
    prompts: std::collections::HashMap<String, String>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct StBertConfig {
    max_seq_length: Option<usize>,
}

/// A downloaded model: its directory and lock. Optional files are read only when the lock
/// lists them, so a file left over from another revision is never mistaken for this one's.
struct Downloaded<'a> {
    id: &'a str,
    dir: PathBuf,
    lock: Lock,
}

impl<'a> Downloaded<'a> {
    fn open(dir: &Path, id: &'a str) -> Result<Downloaded<'a>> {
        let lock = read_lock(dir)
            .ok_or_else(|| VltError::Model(format!("model {id} is not downloaded; run `vlt reindex`")))?;
        Ok(Downloaded { id, dir: dir.to_path_buf(), lock })
    }

    fn json<T: serde::de::DeserializeOwned>(&self, name: &str) -> Result<Option<T>> {
        if !self.lock.files.contains_key(name) {
            return Ok(None);
        }
        let path = self.dir.join(name);
        let text = std::fs::read_to_string(&path)
            .map_err(|e| VltError::Model(format!("cannot read {}: {e}; run `vlt reindex`", path.display())))?;
        serde_json::from_str(&text).map(Some).map_err(|e| VltError::Model(format!("invalid {}: {e}", path.display())))
    }

    fn config(&self) -> Result<HfConfig> {
        self.json("config.json")?
            .ok_or_else(|| VltError::Model(format!("model {} has no config.json", self.id)))
    }

    fn weights(&self) -> Vec<PathBuf> {
        self.lock.files.keys().filter(|f| f.ends_with(".safetensors")).map(|f| self.dir.join(f)).collect()
    }

    /// `chosen`, else the precision the weights are stored in: half precision halves the
    /// memory, and converting it to F32 would not make it more accurate.
    fn precision(&self, chosen: Option<Precision>) -> Result<Precision> {
        match chosen {
            Some(p) => Ok(p),
            None => stored_precision(&self.weights()),
        }
    }

    /// Pooling declared by the sentence-transformers modules, if any. A module vlt cannot
    /// apply (a dense projection, say) is an error: skipping it would give wrong vectors.
    fn declared_pooling(&self) -> Result<Option<Pooling>> {
        let Some(modules) = self.json::<Vec<StModule>>("modules.json")? else {
            return Ok(None);
        };
        let mut pooling = None;
        for m in &modules {
            match m.kind.rsplit('.').next().unwrap_or_default() {
                "Transformer" | "Normalize" => {}
                "Pooling" => {
                    let p: StPooling = self.json(&format!("{}/config.json", m.path))?.unwrap_or_default();
                    let unsupported = p.pooling_mode_max_tokens || p.pooling_mode_mean_sqrt_len_tokens || p.pooling_mode_weightedmean_tokens;
                    let modes = [(p.pooling_mode_cls_token, Pooling::Cls), (p.pooling_mode_mean_tokens, Pooling::Mean), (p.pooling_mode_lasttoken, Pooling::Last)];
                    let mut on = modes.iter().filter(|(set, _)| *set).map(|(_, mode)| *mode);
                    pooling = match (on.next(), on.next(), unsupported) {
                        (Some(mode), None, false) => Some(mode),
                        _ => {
                            return Err(VltError::Model(format!(
                                "model {} uses a pooling vlt does not support; set `pooling` (cls, mean or last) under [models.embedding_options]",
                                self.id
                            )))
                        }
                    };
                }
                other => {
                    return Err(VltError::Model(format!(
                        "model {} has a `{other}` module, which vlt does not support",
                        self.id
                    )))
                }
            }
        }
        Ok(pooling)
    }
}

/// F16 when every float tensor of the `.safetensors` files is stored in 16 bits (F16 or BF16),
/// F32 otherwise. Reads only the headers.
fn stored_precision(files: &[PathBuf]) -> Result<Precision> {
    let mut found = std::collections::BTreeSet::new();
    for path in files {
        let invalid = |e: &dyn std::fmt::Display| VltError::Model(format!("invalid weights {}: {e}; run `vlt reindex`", path.display()));
        let mut f = std::fs::File::open(path).map_err(|e| invalid(&e))?;
        let mut len = [0u8; 8];
        f.read_exact(&mut len).map_err(|e| invalid(&e))?;
        let len = u64::from_le_bytes(len);
        if len > 100 << 20 {
            return Err(invalid(&format!("header of {len} bytes")));
        }
        let mut header = vec![0u8; len as usize];
        f.read_exact(&mut header).map_err(|e| invalid(&e))?;
        let header: serde_json::Map<String, serde_json::Value> = serde_json::from_slice(&header).map_err(|e| invalid(&e))?;
        found.extend(header.values().filter_map(|t| t.get("dtype")?.as_str()).filter(|d| d.starts_with(['F', 'B'])).map(String::from));
    }
    Ok(match found.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        ["BF16"] | ["F16"] => Precision::F16,
        _ => Precision::F32,
    })
}

fn is_bge_en(id: &str) -> bool {
    id.starts_with("BAAI/bge-") && (id.ends_with("-en-v1.5") || id.ends_with("-en"))
}

/// The profile of the embedding model `id` downloaded in `dir`, with `opts` overriding what
/// the repo declares.
pub fn embedding_profile(dir: &Path, id: &str, opts: &EmbeddingOptions) -> Result<EmbeddingProfile> {
    let model = Downloaded::open(dir, id)?;
    let cfg = model.config()?;
    if cfg.is_classifier() {
        return Err(VltError::Config(format!(
            "{id} is a reranker (cross-encoder), not an embedding model; set it as `reranker` in [models]"
        )));
    }
    let arch = match cfg.model_type() {
        "bert" => EmbeddingArch::Bert,
        "xlm-roberta" => EmbeddingArch::XlmRoberta,
        "qwen3" => EmbeddingArch::Qwen3,
        other => {
            return Err(VltError::Config(format!(
                "{id} is a `{other}` model; vlt supports embedding models of type bert, xlm-roberta and qwen3"
            )))
        }
    };
    let dim = cfg.hidden_size.ok_or_else(|| VltError::Model(format!("model {id}: config.json has no hidden_size")))?;
    let default_pooling = if arch == EmbeddingArch::Qwen3 { Pooling::Last } else { Pooling::Mean };
    let pooling = match opts.pooling {
        Some(p) => p,
        None => model.declared_pooling()?.unwrap_or(default_pooling),
    };
    let st: StConfig = model.json("config_sentence_transformers.json")?.unwrap_or_default();
    let query_prefix = opts
        .query_prefix
        .clone()
        .or_else(|| st.prompts.get("query").cloned())
        .unwrap_or_else(|| if is_bge_en(id) { BGE_EN_QUERY_PREFIX.into() } else { String::new() });
    let document_prefix = opts
        .document_prefix
        .clone()
        .or_else(|| st.prompts.get("document").or_else(|| st.prompts.get("passage")).cloned())
        .unwrap_or_default();
    let sbert: StBertConfig = model.json("sentence_bert_config.json")?.unwrap_or_default();
    let max_tokens = sbert.max_seq_length.unwrap_or(MAX_TOKENS).min(cfg.max_tokens());
    Ok(EmbeddingProfile {
        arch,
        dim,
        pooling,
        query_prefix,
        document_prefix,
        max_tokens,
        commit: model.lock.commit.clone(),
        precision: model.precision(opts.precision)?,
        weights: model.weights(),
    })
}

/// The profile of the reranker `id` downloaded in `dir`: a cross-encoder with one output.
pub fn reranker_profile(dir: &Path, id: &str, opts: &RerankerOptions) -> Result<RerankerProfile> {
    let model = Downloaded::open(dir, id)?;
    let cfg = model.config()?;
    if !cfg.is_classifier() {
        return Err(VltError::Config(format!(
            "{id} is not a reranker (cross-encoder): its architecture is {:?}; an embedding model goes in `embedding`",
            cfg.architectures
        )));
    }
    let arch = match cfg.model_type() {
        "bert" => RerankerArch::Bert,
        "xlm-roberta" => RerankerArch::XlmRoberta,
        "deberta-v2" => RerankerArch::DebertaV2,
        other => {
            return Err(VltError::Config(format!(
                "{id} is a `{other}` cross-encoder; vlt supports rerankers of type bert, xlm-roberta and deberta-v2"
            )))
        }
    };
    // Transformers defaults to 2 labels when the config names none.
    let labels = cfg.id2label.as_ref().map(|m| m.len()).or(cfg.num_labels).unwrap_or(2);
    if labels != 1 {
        return Err(VltError::Config(format!(
            "{id} has {labels} output labels; a reranker must output a single relevance score"
        )));
    }
    // Candle's DeBERTa builds its masks in F32, which F16 activations cannot be added to.
    let precision = match (arch, opts.precision) {
        (RerankerArch::DebertaV2, Some(Precision::F16)) => {
            return Err(VltError::Config(format!("{id} is a deberta-v2 model, which vlt runs only in f32; remove `precision`")))
        }
        (RerankerArch::DebertaV2, _) => Precision::F32,
        (_, chosen) => model.precision(chosen)?,
    };
    Ok(RerankerProfile { arch, max_tokens: cfg.max_tokens(), precision, weights: model.weights() })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::installer::models::write_lock;
    use std::collections::BTreeMap;

    /// Writes `files` into `dir` with a lock listing them (and a weights file) at `commit`.
    pub(crate) fn fake_download(dir: &Path, commit: &str, files: &[(&str, &str)]) {
        fake_download_as(dir, commit, files, "F32");
    }

    /// `fake_download` with weights whose header declares `dtype` tensors.
    pub(crate) fn fake_download_as(dir: &Path, commit: &str, files: &[(&str, &str)], dtype: &str) {
        let mut locked = BTreeMap::new();
        for (name, body) in files {
            let path = dir.join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, body).unwrap();
            locked.insert(name.to_string(), "0".repeat(64));
        }
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join("model.safetensors"), safetensors_header(&[dtype, "I64"])).unwrap();
        locked.insert("model.safetensors".into(), "0".repeat(64));
        write_lock(dir, &Lock { revision: "main".into(), commit: commit.into(), files: locked }).unwrap();
    }

    /// A safetensors file of zero-sized tensors, one per dtype.
    fn safetensors_header(dtypes: &[&str]) -> Vec<u8> {
        let mut header = serde_json::Map::new();
        header.insert("__metadata__".into(), serde_json::json!({"format": "pt"}));
        for (i, d) in dtypes.iter().enumerate() {
            header.insert(format!("t{i}"), serde_json::json!({"dtype": d, "shape": [0], "data_offsets": [0, 0]}));
        }
        let header = serde_json::to_vec(&header).unwrap();
        [(header.len() as u64).to_le_bytes().to_vec(), header].concat()
    }

    const BGE_SMALL_COMMIT: &str = "5c38ec7c405ec4b44b94cc5a9bb96e735b38267a";
    const ST_MODULES: &str = r#"[{"idx":0,"name":"0","path":"","type":"sentence_transformers.models.Transformer"},
        {"idx":1,"name":"1","path":"1_Pooling","type":"sentence_transformers.models.Pooling"},
        {"idx":2,"name":"2","path":"2_Normalize","type":"sentence_transformers.models.Normalize"}]"#;

    /// The files vlt reads from `BAAI/bge-small-en-v1.5`, as published at the commit 0.1.0 pinned.
    pub(crate) fn bge_small(dir: &Path) {
        fake_download(dir, BGE_SMALL_COMMIT, &[
            ("config.json", r#"{"architectures":["BertModel"],"model_type":"bert","hidden_size":384,"max_position_embeddings":512,"pad_token_id":0,"id2label":{"0":"LABEL_0"}}"#),
            ("modules.json", ST_MODULES),
            ("1_Pooling/config.json", r#"{"word_embedding_dimension":384,"pooling_mode_cls_token":true,"pooling_mode_mean_tokens":false,"pooling_mode_max_tokens":false,"pooling_mode_mean_sqrt_len_tokens":false}"#),
            ("config_sentence_transformers.json", r#"{"__version__":{"sentence_transformers":"2.2.2"}}"#),
            ("sentence_bert_config.json", r#"{"max_seq_length":512,"do_lower_case":true}"#),
        ]);
    }

    /// The files vlt reads from `Qwen/Qwen3-Embedding-0.6B`.
    pub(crate) fn qwen3(dir: &Path) {
        fake_download_as(dir, "97b0c614be4d77ee51c0cef4e5f07c00f9eb65b3", &[
            ("config.json", r#"{"architectures":["Qwen3ForCausalLM"],"model_type":"qwen3","hidden_size":1024,"max_position_embeddings":32768}"#),
            ("modules.json", ST_MODULES),
            ("1_Pooling/config.json", r#"{"word_embedding_dimension":1024,"pooling_mode_cls_token":false,"pooling_mode_mean_tokens":false,"pooling_mode_lasttoken":true,"include_prompt":true}"#),
            ("config_sentence_transformers.json", r#"{"prompts":{"query":"Instruct: Given a web search query, retrieve relevant passages that answer the query\nQuery:","document":""},"default_prompt_name":null}"#),
        ], "BF16");
    }

    /// The config of `mixedbread-ai/mxbai-rerank-base-v1`, the parts vlt reads.
    pub(crate) fn mxbai_rerank_base(dir: &Path) {
        fake_download_as(dir, "800f24c113213a187e65bde9db00c15a2bb12738", &[
            ("config.json", r#"{"architectures":["DebertaV2ForSequenceClassification"],"model_type":"deberta-v2","hidden_size":768,"max_position_embeddings":512,"pad_token_id":0,"id2label":{"0":"LABEL_0"},"type_vocab_size":0}"#),
        ], "F16");
    }

    #[test]
    fn bge_small_profile_matches_what_vlt_0_1_0_built() {
        let d = tempfile::tempdir().unwrap();
        bge_small(d.path());
        let p = embedding_profile(d.path(), "BAAI/bge-small-en-v1.5", &EmbeddingOptions::default()).unwrap();
        assert_eq!((p.arch, p.dim, p.pooling, p.max_tokens), (EmbeddingArch::Bert, 384, Pooling::Cls, 512));
        assert_eq!(p.query_prefix, BGE_EN_QUERY_PREFIX, "BGE's instruction is not in its repo");
        assert_eq!(p.document_prefix, "");
        assert_eq!(p.fingerprint(), LEGACY_FINGERPRINT, "0.1.0 indexes must keep matching");
        assert_eq!(p.weights, vec![d.path().join("model.safetensors")]);
    }

    #[test]
    fn qwen3_profile_reads_last_token_pooling_and_prompts_from_the_repo() {
        let d = tempfile::tempdir().unwrap();
        qwen3(d.path());
        let p = embedding_profile(d.path(), "Qwen/Qwen3-Embedding-0.6B", &EmbeddingOptions::default()).unwrap();
        assert_eq!((p.arch, p.dim, p.pooling, p.max_tokens), (EmbeddingArch::Qwen3, 1024, Pooling::Last, 512));
        assert!(p.query_prefix.starts_with("Instruct: ") && p.query_prefix.ends_with("\nQuery:"), "{:?}", p.query_prefix);
        assert_eq!(p.document_prefix, "");
    }

    #[test]
    fn models_compute_in_the_precision_their_weights_are_stored_in() {
        let d = tempfile::tempdir().unwrap();
        qwen3(d.path());
        let p = embedding_profile(d.path(), "Qwen/Qwen3-Embedding-0.6B", &EmbeddingOptions::default()).unwrap();
        assert_eq!(p.precision, Precision::F16, "BF16 weights run as F16");
        let f32 = EmbeddingOptions { precision: Some(Precision::F32), ..Default::default() };
        let forced = embedding_profile(d.path(), "Qwen/Qwen3-Embedding-0.6B", &f32).unwrap();
        assert_eq!(forced.precision, Precision::F32);
        assert_ne!(p.fingerprint(), forced.fingerprint(), "the precision changes the stored vectors");
        let d = tempfile::tempdir().unwrap();
        mxbai_rerank_base(d.path());
        assert_eq!(reranker_profile(d.path(), "o/m", &RerankerOptions::default()).unwrap().precision, Precision::F32, "deberta-v2 runs in F32");
        let f16 = RerankerOptions { precision: Some(Precision::F16), ..Default::default() };
        assert!(reranker_profile(d.path(), "o/m", &f16).unwrap_err().to_string().contains("only in f32"));
        let d = tempfile::tempdir().unwrap();
        fake_download_as(d.path(), "c", &[("config.json", r#"{"model_type":"bert","architectures":["BertForSequenceClassification"],"id2label":{"0":"L"}}"#)], "F16");
        assert_eq!(reranker_profile(d.path(), "o/m", &RerankerOptions::default()).unwrap().precision, Precision::F16);
        let f32 = RerankerOptions { precision: Some(Precision::F32), ..Default::default() };
        assert_eq!(reranker_profile(d.path(), "o/m", &f32).unwrap().precision, Precision::F32);
    }

    #[test]
    fn mixed_or_full_precision_weights_compute_in_f32() {
        let d = tempfile::tempdir().unwrap();
        let path = |name: &str| d.path().join(name);
        for (dtypes, want) in [(vec!["F16", "BF16"], Precision::F32), (vec!["F32", "I64"], Precision::F32), (vec!["BF16", "I64", "U8"], Precision::F16), (vec!["F16"], Precision::F16)] {
            std::fs::write(path("w.safetensors"), safetensors_header(&dtypes)).unwrap();
            assert_eq!(stored_precision(&[path("w.safetensors")]).unwrap(), want, "{dtypes:?}");
        }
        std::fs::write(path("a.safetensors"), safetensors_header(&["F16"])).unwrap();
        std::fs::write(path("b.safetensors"), safetensors_header(&["BF16"])).unwrap();
        assert_eq!(stored_precision(&[path("a.safetensors"), path("b.safetensors")]).unwrap(), Precision::F32, "shards disagree");
        std::fs::write(path("c.safetensors"), safetensors_header(&["F32"])).unwrap();
        assert_eq!(stored_precision(&[path("a.safetensors"), path("c.safetensors")]).unwrap(), Precision::F32);
        std::fs::write(path("bad.safetensors"), b"xx").unwrap();
        assert!(stored_precision(&[path("bad.safetensors")]).unwrap_err().to_string().contains("invalid weights"));
    }

    #[test]
    fn options_override_the_repo() {
        let d = tempfile::tempdir().unwrap();
        bge_small(d.path());
        let opts = EmbeddingOptions {
            pooling: Some(Pooling::Mean),
            query_prefix: Some("q: ".into()),
            document_prefix: Some("d: ".into()),
            ..Default::default()
        };
        let p = embedding_profile(d.path(), "BAAI/bge-small-en-v1.5", &opts).unwrap();
        assert_eq!((p.pooling, p.query_prefix.as_str(), p.document_prefix.as_str()), (Pooling::Mean, "q: ", "d: "));
        assert_ne!(p.fingerprint(), LEGACY_FINGERPRINT, "pooling and document prefix change the stored vectors");
        let only_query = EmbeddingOptions { query_prefix: Some("q: ".into()), ..Default::default() };
        let p = embedding_profile(d.path(), "BAAI/bge-small-en-v1.5", &only_query).unwrap();
        assert_eq!(p.fingerprint(), LEGACY_FINGERPRINT, "the query prefix does not");
    }

    #[test]
    fn repos_without_sentence_transformers_files_get_architecture_defaults() {
        let d = tempfile::tempdir().unwrap();
        fake_download(d.path(), "c", &[("config.json", r#"{"model_type":"xlm-roberta","hidden_size":768,"max_position_embeddings":514,"pad_token_id":1}"#)]);
        let p = embedding_profile(d.path(), "o/m", &EmbeddingOptions::default()).unwrap();
        assert_eq!((p.arch, p.pooling, p.max_tokens, p.query_prefix.as_str()), (EmbeddingArch::XlmRoberta, Pooling::Mean, 512, ""));
    }

    #[test]
    fn files_the_lock_does_not_list_are_ignored() {
        let d = tempfile::tempdir().unwrap();
        fake_download(d.path(), "c", &[("config.json", r#"{"model_type":"bert","hidden_size":8}"#)]);
        std::fs::write(d.path().join("config_sentence_transformers.json"), r#"{"prompts":{"query":"stale: "}}"#).unwrap();
        let p = embedding_profile(d.path(), "o/m", &EmbeddingOptions::default()).unwrap();
        assert_eq!(p.query_prefix, "");
    }

    #[test]
    fn unsupported_embedding_models_are_rejected_with_the_reason() {
        let d = tempfile::tempdir().unwrap();
        let err = |config: &str, extra: &[(&str, &str)]| {
            let d = tempfile::tempdir().unwrap();
            let files: Vec<(&str, &str)> = [("config.json", config)].into_iter().chain(extra.iter().copied()).collect();
            fake_download(d.path(), "c", &files);
            embedding_profile(d.path(), "o/m", &EmbeddingOptions::default()).unwrap_err().to_string()
        };
        assert!(err(r#"{"model_type":"gemma3","hidden_size":8}"#, &[]).contains("`gemma3` model"));
        assert!(err(r#"{"model_type":"bert","architectures":["BertForSequenceClassification"]}"#, &[]).contains("is a reranker"));
        let dense = r#"[{"path":"","type":"sentence_transformers.models.Transformer"},{"path":"2_Dense","type":"sentence_transformers.models.Dense"}]"#;
        assert!(err(r#"{"model_type":"bert","hidden_size":8}"#, &[("modules.json", dense)]).contains("`Dense` module"));
        let max = r#"[{"path":"1_Pooling","type":"sentence_transformers.models.Pooling"}]"#;
        let msg = err(r#"{"model_type":"bert","hidden_size":8}"#, &[("modules.json", max), ("1_Pooling/config.json", r#"{"pooling_mode_max_tokens":true}"#)]);
        assert!(msg.contains("pooling vlt does not support"), "{msg}");
        let msg = embedding_profile(d.path(), "o/m", &EmbeddingOptions::default()).unwrap_err().to_string();
        assert!(msg.contains("not downloaded"), "{msg}");
    }

    #[test]
    fn reranker_profiles_come_from_the_architecture() {
        let profile = |config: &str| {
            let d = tempfile::tempdir().unwrap();
            fake_download(d.path(), "c", &[("config.json", config)]);
            reranker_profile(d.path(), "o/m", &RerankerOptions::default()).map(|p| (p.arch, p.max_tokens)).map_err(|e| e.to_string())
        };
        let one = r#""id2label":{"0":"LABEL_0"}"#;
        assert_eq!(profile(&format!(r#"{{"model_type":"bert","architectures":["BertForSequenceClassification"],"max_position_embeddings":512,{one}}}"#)), Ok((RerankerArch::Bert, 512)));
        assert_eq!(profile(&format!(r#"{{"model_type":"xlm-roberta","architectures":["XLMRobertaForSequenceClassification"],"max_position_embeddings":514,"pad_token_id":1,{one}}}"#)), Ok((RerankerArch::XlmRoberta, 512)));
        assert_eq!(profile(&format!(r#"{{"model_type":"deberta-v2","architectures":["DebertaV2ForSequenceClassification"],"max_position_embeddings":512,{one}}}"#)), Ok((RerankerArch::DebertaV2, 512)));
        assert!(profile(r#"{"model_type":"bert","architectures":["BertModel"]}"#).unwrap_err().contains("not a reranker"));
        assert!(profile(r#"{"model_type":"bert","architectures":["BertForSequenceClassification"]}"#).unwrap_err().contains("2 output labels"));
        assert!(profile(&format!(r#"{{"model_type":"electra","architectures":["ElectraForSequenceClassification"],{one}}}"#)).unwrap_err().contains("`electra` cross-encoder"));
    }

    #[test]
    fn model_ids_and_revisions_are_validated() {
        for ok in ["BAAI/bge-small-en-v1.5", "Qwen/Qwen3-Embedding-0.6B", "o_1/m.x"] {
            assert!(validate_model_id(ok).is_ok(), "{ok}");
        }
        for bad in ["", "nope", "a/b/c", "../x", "a/..", "a/.hidden", "a/b c", "/a", "a/"] {
            assert!(validate_model_id(bad).is_err(), "{bad}");
        }
        for ok in ["main", "v1.0", "refs/pr/3", "5c38ec7c405ec4b44b94cc5a9bb96e735b38267a"] {
            assert!(validate_revision(ok).is_ok(), "{ok}");
        }
        for bad in ["", "/main", "a/../b", "a//b", "a b", "x?y"] {
            assert!(validate_revision(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn model_dir_naming() {
        assert_eq!(model_dir(Path::new("/m"), "BAAI/bge-small-en-v1.5"), PathBuf::from("/m/BAAI--bge-small-en-v1.5"));
    }
}
