use std::path::{Path, PathBuf};

use candle_core::{DType, Device, Tensor};
use candle_nn::VarBuilder;
use candle_transformers::models::bert::{self, BertModel};
use candle_transformers::models::qwen3;
use candle_transformers::models::xlm_roberta::{self, XLMRobertaModel};
use tokenizers::{PaddingParams, Tokenizer, TruncationParams};

use crate::ml::profile::{EmbeddingArch, EmbeddingProfile, Pooling};
#[cfg(test)]
use crate::ml::profile::Precision;
use crate::ml::Embedder;
use crate::{Result, VltError};

/// At most this many texts per forward pass.
const BATCH: usize = 32;
/// At most this many padded tokens per forward pass, bounding activation memory.
const BATCH_TOKENS: usize = 768;

enum Backbone {
    Bert(BertModel),
    XlmRoberta(XLMRobertaModel),
    /// Causal: a token only attends to those before it, so right padding never reaches the
    /// text and needs no mask. Cloned for each forward, since the model keeps a KV cache that
    /// must start empty; the clone shares the weights.
    Qwen3(qwen3::Model),
}

pub struct TextEmbedder {
    backbone: Backbone,
    pooling: Pooling,
    query_prefix: String,
    document_prefix: String,
    /// Truncating, not padding: each batch is padded on the right to its own longest text.
    tokenizer: Tokenizer,
    padding: PaddingParams,
    counter: Tokenizer,
    device: Device,
}

pub(crate) fn model_err(e: impl std::fmt::Display) -> VltError {
    VltError::Model(e.to_string())
}

pub(crate) fn load_config<T: serde::de::DeserializeOwned>(dir: &Path) -> Result<T> {
    let path = dir.join("config.json");
    let invalid = |e: &dyn std::fmt::Display| VltError::Model(format!("invalid {}: {e}", path.display()));
    let text = std::fs::read_to_string(&path).map_err(|e| invalid(&e))?;
    serde_json::from_str(&text).map_err(|e| invalid(&e))
}

/// The weights converted to `dtype` as they load.
pub(crate) fn load_weights(files: &[PathBuf], dtype: DType, device: &Device) -> Result<VarBuilder<'static>> {
    if files.is_empty() {
        return Err(VltError::Model("the model has no weights; run `vlt reindex`".into()));
    }
    // SAFETY: the files are owned by vlt and not modified while mapped.
    unsafe { VarBuilder::from_mmaped_safetensors(files, dtype, device) }.map_err(model_err)
}

/// `vb` under `prefix` when the weights were saved from a task model (`bert.encoder…` rather
/// than `encoder…`); `probe` is a tensor the base model always has.
pub(crate) fn base_model(vb: VarBuilder<'static>, prefix: &str, probe: &str) -> VarBuilder<'static> {
    if !vb.contains_tensor(probe) && vb.pp(prefix).contains_tensor(probe) {
        vb.pp(prefix)
    } else {
        vb
    }
}

/// Batch-longest right padding with the tokenizer's own pad token.
fn pad_params(tokenizer: &Tokenizer) -> PaddingParams {
    if let Some(p) = tokenizer.get_padding() {
        return PaddingParams { pad_id: p.pad_id, pad_type_id: p.pad_type_id, pad_token: p.pad_token.clone(), ..Default::default() };
    }
    ["[PAD]", "<pad>"]
        .into_iter()
        .find_map(|t| tokenizer.token_to_id(t).map(|id| PaddingParams { pad_id: id, pad_token: t.into(), ..Default::default() }))
        .unwrap_or_default()
}

/// Returns (truncating + padding tokenizer, plain tokenizer).
pub(crate) fn load_tokenizer(dir: &Path, truncation: TruncationParams) -> Result<(Tokenizer, Tokenizer)> {
    let mut counter = Tokenizer::from_file(dir.join("tokenizer.json")).map_err(model_err)?;
    let padding = pad_params(&counter);
    counter.with_truncation(None).map_err(model_err)?;
    counter.with_padding(None);
    let mut tokenizer = counter.clone();
    tokenizer.with_truncation(Some(truncation)).map_err(model_err)?;
    tokenizer.with_padding(Some(padding));
    Ok((tokenizer, counter))
}

/// Indices of `lens` sorted by length and grouped so that a batch holds at most `max_batch`
/// items and, padded to its longest item, at most `max_tokens` tokens (a longer item goes alone).
pub(crate) fn length_batches(lens: &[usize], max_batch: usize, max_tokens: usize) -> Vec<Vec<usize>> {
    let mut order: Vec<usize> = (0..lens.len()).collect();
    order.sort_by_key(|&i| lens[i]);
    let mut batches: Vec<Vec<usize>> = Vec::new();
    for i in order {
        match batches.last_mut() {
            // ascending order: item i is the longest of its batch
            Some(b) if b.len() < max_batch && (b.len() + 1) * lens[i] <= max_tokens => b.push(i),
            _ => batches.push(vec![i]),
        }
    }
    batches
}

/// Runs `forward` over the `length_batches` of `lens` (it gets the batch's input indices and
/// returns one result per index) and returns the results in input order.
pub(crate) fn in_length_batches<T>(
    lens: &[usize],
    max_batch: usize,
    max_tokens: usize,
    mut forward: impl FnMut(&[usize]) -> Result<Vec<T>>,
) -> Result<Vec<T>> {
    let mut out: Vec<Option<T>> = (0..lens.len()).map(|_| None).collect();
    for batch in length_batches(lens, max_batch, max_tokens) {
        for (&i, r) in batch.iter().zip(forward(&batch)?) {
            out[i] = Some(r);
        }
    }
    Ok(out.into_iter().map(|r| r.expect("every input is in exactly one batch with one result")).collect())
}

/// (input_ids, type_ids, attention_mask), each of shape (batch, seq).
pub(crate) fn batch_tensors(encodings: &[tokenizers::Encoding], device: &Device) -> Result<(Tensor, Tensor, Tensor)> {
    let rows = |f: fn(&tokenizers::Encoding) -> &[u32]| -> Result<Tensor> {
        let rows = encodings
            .iter()
            .map(|e| Tensor::new(f(e), device))
            .collect::<candle_core::Result<Vec<_>>>()
            .map_err(model_err)?;
        Tensor::stack(&rows, 0).map_err(model_err)
    };
    Ok((
        rows(|e| e.get_ids())?,
        rows(|e| e.get_type_ids())?,
        rows(|e| e.get_attention_mask())?,
    ))
}

/// One vector per row of `hidden` (batch, seq, dim): `lens` real tokens each, padding after.
fn pool(pooling: Pooling, hidden: &Tensor, mask: &Tensor, lens: &[usize]) -> candle_core::Result<Tensor> {
    match pooling {
        Pooling::Cls => hidden.narrow(1, 0, 1)?.squeeze(1),
        Pooling::Mean => {
            let mask = mask.to_dtype(DType::F32)?.unsqueeze(2)?;
            let sum = hidden.broadcast_mul(&mask)?.sum(1)?;
            sum.broadcast_div(&mask.sum(1)?.clamp(1f32, f32::MAX)?)
        }
        Pooling::Last => {
            let rows = lens
                .iter()
                .enumerate()
                .map(|(i, &len)| hidden.get(i)?.get(len.saturating_sub(1)))
                .collect::<candle_core::Result<Vec<_>>>()?;
            Tensor::stack(&rows, 0)
        }
    }
}

fn l2_normalize(v: &Tensor) -> candle_core::Result<Tensor> {
    let norm = v.sqr()?.sum_keepdim(1)?.sqrt()?.clamp(1e-12f32, f32::MAX)?;
    v.broadcast_div(&norm)
}

impl TextEmbedder {
    pub fn load(dir: &Path, profile: &EmbeddingProfile) -> Result<TextEmbedder> {
        let device = Device::Cpu;
        let vb = load_weights(&profile.weights, profile.precision.dtype(), &device)?;
        let backbone = match profile.arch {
            EmbeddingArch::Bert => {
                let cfg: bert::Config = load_config(dir)?;
                let vb = base_model(vb, "bert", "embeddings.word_embeddings.weight");
                Backbone::Bert(BertModel::load(vb, &cfg).map_err(model_err)?)
            }
            EmbeddingArch::XlmRoberta => {
                let cfg: xlm_roberta::Config = load_config(dir)?;
                let vb = base_model(vb, "roberta", "embeddings.word_embeddings.weight");
                Backbone::XlmRoberta(XLMRobertaModel::new(&cfg, vb).map_err(model_err)?)
            }
            EmbeddingArch::Qwen3 => {
                let cfg: qwen3::Config = load_config(dir)?;
                // Candle names the tensors as the causal LM does (`model.layers…`); embedding
                // repos save the bare model (`layers…`).
                let vb = if vb.contains_tensor("model.embed_tokens.weight") {
                    vb
                } else {
                    vb.rename_f(|name| name.strip_prefix("model.").unwrap_or(name).to_string())
                };
                Backbone::Qwen3(qwen3::Model::new(&cfg, vb).map_err(model_err)?)
            }
        };
        let trunc = TruncationParams { max_length: profile.max_tokens, ..Default::default() };
        let (mut tokenizer, counter) = load_tokenizer(dir, trunc)?;
        let padding = tokenizer.get_padding().cloned().unwrap_or_default();
        tokenizer.with_padding(None);
        Ok(TextEmbedder {
            backbone,
            pooling: profile.pooling,
            query_prefix: profile.query_prefix.clone(),
            document_prefix: profile.document_prefix.clone(),
            tokenizer,
            padding,
            counter,
            device,
        })
    }

    fn embed_batch(&self, mut encodings: Vec<tokenizers::Encoding>) -> Result<Vec<Vec<f32>>> {
        let lens: Vec<usize> = encodings.iter().map(|e| e.len()).collect();
        tokenizers::pad_encodings(&mut encodings, &self.padding).map_err(model_err)?;
        let (ids, types, mask) = batch_tensors(&encodings, &self.device)?;
        let hidden = match &self.backbone {
            Backbone::Bert(m) => m.forward(&ids, &types, Some(&mask)),
            Backbone::XlmRoberta(m) => m.forward(&ids, &mask, &types, None, None, None),
            Backbone::Qwen3(m) => m.clone().forward(&ids, 0),
        };
        // Pooled and normalized in F32 whatever the model computes in.
        hidden
            .and_then(|h| h.to_dtype(DType::F32))
            .and_then(|h| pool(self.pooling, &h, &mask, &lens))
            .and_then(|v| l2_normalize(&v))
            .and_then(|v| v.to_vec2::<f32>())
            .map_err(model_err)
    }

    fn embed_raw(&self, texts: &[&str], on_batch: &mut dyn FnMut(&[usize])) -> Result<Vec<Vec<f32>>> {
        let encodings = self.tokenizer.encode_batch(texts.to_vec(), true).map_err(model_err)?;
        let lens: Vec<usize> = encodings.iter().map(|e| e.len()).collect();
        let mut encodings: Vec<Option<tokenizers::Encoding>> = encodings.into_iter().map(Some).collect();
        in_length_batches(&lens, BATCH, BATCH_TOKENS, |batch| {
            let vectors = self.embed_batch(batch.iter().filter_map(|&i| encodings[i].take()).collect())?;
            on_batch(batch);
            Ok(vectors)
        })
    }
}

impl Embedder for TextEmbedder {
    fn embed(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        self.embed_with_progress(texts, &mut |_| {})
    }

    fn embed_with_progress(&self, texts: &[&str], on_batch: &mut dyn FnMut(&[usize])) -> Result<Vec<Vec<f32>>> {
        if self.document_prefix.is_empty() {
            return self.embed_raw(texts, on_batch);
        }
        let prefixed: Vec<String> = texts.iter().map(|t| format!("{}{t}", self.document_prefix)).collect();
        self.embed_raw(&prefixed.iter().map(String::as_str).collect::<Vec<_>>(), on_batch)
    }

    fn embed_query(&self, query: &str) -> Result<Vec<f32>> {
        let text = format!("{}{query}", self.query_prefix);
        self.embed_raw(&[&text], &mut |_| {})?
            .pop()
            .ok_or_else(|| VltError::Internal("the embedder returned no vector".into()))
    }

    /// Counts the document prefix too, so a chunk cut to fit still fits once prefixed.
    fn count_tokens(&self, text: &str) -> usize {
        let text = format!("{}{text}", self.document_prefix);
        self.counter.encode(text, true).map(|e| e.len()).unwrap_or(0)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    fn word_level(vocab: &[(&str, u32)]) -> Tokenizer {
        let vocab = vocab.iter().map(|(t, i)| (t.to_string(), *i)).collect();
        let model = tokenizers::models::wordlevel::WordLevel::builder().vocab(vocab).unk_token("[UNK]".into()).build().unwrap();
        Tokenizer::new(model)
    }

    #[test]
    fn pad_params_use_the_tokenizers_own_pad_token() {
        let mut configured = word_level(&[("[UNK]", 0), ("[PAD]", 1)]);
        configured.with_padding(Some(PaddingParams { pad_id: 7, pad_type_id: 2, pad_token: "<p>".into(), ..Default::default() }));
        let p = pad_params(&configured);
        assert_eq!((p.pad_id, p.pad_type_id, p.pad_token.as_str()), (7, 2, "<p>"));
        assert_eq!(pad_params(&word_level(&[("[UNK]", 0), ("[PAD]", 3)])).pad_id, 3);
        let p = pad_params(&word_level(&[("<pad>", 1), ("[UNK]", 0)]));
        assert_eq!((p.pad_id, p.pad_token.as_str()), (1, "<pad>"));
        assert_eq!(pad_params(&word_level(&[("[UNK]", 5)])).pad_id, 0);
    }

    #[test]
    fn length_batches_sort_by_length_and_respect_both_limits() {
        assert_eq!(length_batches(&[5, 1, 3, 2], 2, 100), vec![vec![1, 3], vec![2, 0]]);
        assert_eq!(length_batches(&[10, 10, 10], 8, 25), vec![vec![0, 1], vec![2]]);
        assert_eq!(length_batches(&[10, 10, 10], 8, 30), vec![vec![0, 1, 2]], "the token cap is inclusive");
        assert_eq!(length_batches(&[600, 4], 8, 512), vec![vec![1], vec![0]], "an item over the cap goes alone");
        assert!(length_batches(&[], 8, 512).is_empty());
    }

    /// Runs the embedder's batching over `lens`, checking every forward against the embedder's caps.
    fn batched(lens: &[usize]) -> (Vec<usize>, Vec<Vec<usize>>) {
        let mut forwards = Vec::new();
        let out = in_length_batches(lens, BATCH, BATCH_TOKENS, |batch| {
            forwards.push(batch.to_vec());
            Ok(batch.to_vec())
        })
        .unwrap();
        (out, forwards)
    }

    #[test]
    fn batched_results_come_back_in_input_order() {
        let over_cap = BATCH_TOKENS + 1;
        let lens = [512, 3, over_cap, 1, 40, 512, 7, 200, 3, 90];
        let (out, forwards) = batched(&lens);
        assert_eq!(out, (0..lens.len()).collect::<Vec<_>>());
        assert!(forwards.len() > 1, "{forwards:?}");
        assert!(forwards.iter().any(|b| b == &vec![2]), "the over-cap text runs alone: {forwards:?}");
        let (out, forwards) = batched(&[]);
        assert!(out.is_empty() && forwards.is_empty());
    }

    /// Forward sizes for `n` texts of `len` tokens each under caps of `max_batch` texts and `max_tokens` padded tokens.
    pub(crate) fn uniform_sizes(n: usize, len: usize, max_batch: usize, max_tokens: usize) -> Vec<usize> {
        let per = max_batch.min(max_tokens / len).max(1);
        (0..n).step_by(per).map(|start| per.min(n - start)).collect()
    }

    #[test]
    fn forwards_hold_at_most_batch_texts_and_batch_tokens_padded_tokens() {
        let at_both_caps = BATCH_TOKENS / BATCH;
        assert_eq!((BATCH, BATCH_TOKENS), (32, 768), "caps measured to keep peak RSS at or below main: re-measure before changing");
        const { assert!(BATCH_TOKENS / 10 > BATCH && BATCH_TOKENS / 512 < BATCH, "the cases below hit each cap") };
        for len in [10, 512, at_both_caps, at_both_caps + 1] {
            let n = 3 * BATCH + 4;
            let (_, forwards) = batched(&vec![len; n]);
            assert_eq!(forwards.iter().map(|b| b.len()).collect::<Vec<_>>(), uniform_sizes(n, len, BATCH, BATCH_TOKENS), "len {len}");
        }
        assert_eq!(uniform_sizes(10, 10, 4, 1000), vec![4, 4, 2]);
        assert_eq!(uniform_sizes(5, 300, 4, 1000), vec![3, 2]);
    }

    #[test]
    fn forwards_are_sorted_by_length() {
        let lens: Vec<usize> = (0..70).map(|i| (i * 37) % 300 + 1).collect();
        let (_, forwards) = batched(&lens);
        let flat: Vec<usize> = forwards.concat().iter().map(|&i| lens[i]).collect();
        assert!(flat.windows(2).all(|w| w[0] <= w[1]), "{flat:?}");
        for b in &forwards {
            let longest = b.iter().map(|&i| lens[i]).max().unwrap();
            assert!(b.len() <= BATCH && b.len() * longest <= BATCH_TOKENS, "{b:?}");
        }
    }

    #[test]
    fn missing_config_or_weights_is_a_model_error() {
        let d = tempfile::tempdir().unwrap();
        assert!(matches!(load_config::<bert::Config>(d.path()), Err(VltError::Model(_))));
        let profile = EmbeddingProfile {
            arch: EmbeddingArch::Bert,
            dim: 8,
            pooling: Pooling::Cls,
            query_prefix: String::new(),
            document_prefix: String::new(),
            max_tokens: 512,
            commit: "c".into(),
            weights: vec![d.path().join("model.safetensors")],
            precision: Precision::F32,
        };
        assert!(matches!(TextEmbedder::load(d.path(), &profile), Err(VltError::Model(_))));
        let no_weights = EmbeddingProfile { weights: vec![], ..profile };
        assert!(matches!(TextEmbedder::load(d.path(), &no_weights), Err(VltError::Model(_))));
    }

    #[test]
    fn pooling_picks_the_right_tokens_and_ignores_padding() {
        // two rows of three tokens; the second row has one real token and two pads
        let hidden = Tensor::new(&[[[1f32, 0.], [3., 2.], [5., 4.]], [[7., 6.], [9., 9.], [9., 9.]]], &Device::Cpu).unwrap();
        let mask = Tensor::new(&[[1u32, 1, 1], [1, 0, 0]], &Device::Cpu).unwrap();
        let lens = [3, 1];
        let run = |p| pool(p, &hidden, &mask, &lens).unwrap().to_vec2::<f32>().unwrap();
        assert_eq!(run(Pooling::Cls), vec![vec![1., 0.], vec![7., 6.]]);
        assert_eq!(run(Pooling::Mean), vec![vec![3., 2.], vec![7., 6.]]);
        assert_eq!(run(Pooling::Last), vec![vec![5., 4.], vec![7., 6.]]);
        let unit = l2_normalize(&Tensor::new(&[[3f32, 4.]], &Device::Cpu).unwrap()).unwrap().to_vec2::<f32>().unwrap();
        assert_eq!(unit, vec![vec![0.6, 0.8]]);
    }
}
