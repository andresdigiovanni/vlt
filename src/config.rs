use crate::error::{Result, VltError};
use crate::ml::profile::{Pooling, Precision};
use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub vault: PathBuf,
    #[serde(default)]
    pub exclude: Vec<String>,
    #[serde(default)]
    pub models: ModelsConfig,
    #[serde(default)]
    pub search: SearchConfig,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct ModelsConfig {
    /// Hugging Face repo id of the embedding model.
    pub embedding: String,
    /// Hugging Face repo id of the reranker (a cross-encoder).
    pub reranker: String,
    pub idle_unload_secs: u64,
    pub embedding_options: EmbeddingOptions,
    pub reranker_options: RerankerOptions,
}

impl Default for ModelsConfig {
    fn default() -> Self {
        ModelsConfig {
            embedding: "Qwen/Qwen3-Embedding-0.6B".into(),
            reranker: "mixedbread-ai/mxbai-rerank-base-v1".into(),
            idle_unload_secs: 300,
            embedding_options: EmbeddingOptions::default(),
            reranker_options: RerankerOptions::default(),
        }
    }
}

/// Overrides for what the embedding model's repo declares; unset means "as the repo says".
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct EmbeddingOptions {
    /// Branch, tag or commit to download (default: `main`).
    pub revision: Option<String>,
    pub pooling: Option<Pooling>,
    /// Prepended to search queries.
    pub query_prefix: Option<String>,
    /// Prepended to indexed text.
    pub document_prefix: Option<String>,
    /// Float type to compute in (default: the one the weights are stored in).
    pub precision: Option<Precision>,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct RerankerOptions {
    /// Branch, tag or commit to download (default: `main`).
    pub revision: Option<String>,
    /// Float type to compute in (default: the one the weights are stored in).
    pub precision: Option<Precision>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct SearchConfig {
    pub k: usize,
    pub rrf_k: usize,
    pub rerank_top: usize,
    pub limit: usize,
    pub min_score: f64,
}

impl Default for SearchConfig {
    fn default() -> Self {
        SearchConfig { k: 50, rrf_k: 60, rerank_top: 10, limit: 5, min_score: 0.05 }
    }
}

impl Config {
    pub fn parse(text: &str) -> Result<Config> {
        toml_edit::de::from_str(text).map_err(|e| VltError::Config(e.to_string()))
    }

    pub fn load(path: &Path) -> Result<Config> {
        let text = std::fs::read_to_string(path)?;
        Config::parse(&text)
    }

    pub fn default_toml(vault: &Path) -> String {
        let vault_value = toml_edit::value(vault.display().to_string());
        format!(
            r##"vault = {vault_value}

# Paths relative to the vault that are not indexed (glob syntax; `**` crosses subdirectories).
# Example: exclude = ["templates/**", "archive/old/**"]
exclude = []

[models]
# Any Hugging Face repo of a supported architecture (see the README); changing the embedding
# model requires `vlt reindex`, which downloads it and rebuilds the index.
embedding = "Qwen/Qwen3-Embedding-0.6B"             # 1024 dimensions, ~1.2 GB
reranker  = "mixedbread-ai/mxbai-rerank-base-v1"    # ~350 MB
idle_unload_secs = 300                               # unload the models after 5 min without use

# Optional overrides of what the model repos declare:
# [models.embedding_options]
# revision = "main"              # branch, tag or commit to download
# pooling = "cls"                # cls, mean or last
# query_prefix = "query: "       # prepended to search queries
# document_prefix = "passage: "  # prepended to indexed text
# precision = "f32"              # f32 or f16 (default: f16 for 16-bit weights, else f32)
# [models.reranker_options]
# revision = "main"
# precision = "f32"

[search]
k = 50            # candidates per branch (BM25 and vectors)
rrf_k = 60        # Reciprocal Rank Fusion constant
rerank_top = 10   # candidates passed to the reranker; each one costs reranking time
limit = 5         # results returned by default
min_score = 0.05  # reranker threshold [0, 1]; depends on the reranker, calibrate with real documents
"##
        )
    }

    pub fn set_vault_in(text: &str, vault: &Path) -> Result<String> {
        let mut doc = text
            .parse::<toml_edit::DocumentMut>()
            .map_err(|e| VltError::Config(e.to_string()))?;
        doc["vault"] = toml_edit::value(vault.display().to_string());
        Ok(doc.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    #[test]
    fn defaults_fill_missing_sections() {
        let cfg = Config::parse("vault = \"/v\"\n").unwrap();
        assert_eq!(cfg.vault, PathBuf::from("/v"));
        assert!(cfg.exclude.is_empty());
        assert_eq!(cfg.models.embedding, "Qwen/Qwen3-Embedding-0.6B");
        assert_eq!(cfg.models.reranker, "mixedbread-ai/mxbai-rerank-base-v1");
        assert_eq!(cfg.models.idle_unload_secs, 300);
        let s = &cfg.search;
        assert_eq!((s.k, s.rrf_k, s.rerank_top, s.limit), (50, 60, 10, 5));
        assert!((s.min_score - 0.05).abs() < 1e-12);
    }

    #[test]
    fn partial_section_keeps_other_defaults() {
        let cfg = Config::parse("vault = \"/v\"\n[search]\nlimit = 9\n").unwrap();
        assert_eq!(cfg.search.limit, 9);
        assert_eq!(cfg.search.k, 50);
    }

    #[test]
    fn unknown_keys_are_rejected() {
        assert!(matches!(Config::parse("vault = \"/v\"\nfoo = 1\n"), Err(VltError::Config(_))));
        assert!(matches!(Config::parse("vault = \"/v\"\n[search]\nkk = 1\n"), Err(VltError::Config(_))));
    }

    #[test]
    fn model_options_parse_and_reject_unknown_keys() {
        let text = "vault = \"/v\"\n[models]\nembedding = \"BAAI/bge-large-en-v1.5\"\n\
                    [models.embedding_options]\nrevision = \"v1\"\npooling = \"last\"\nquery_prefix = \"q: \"\n\
                    [models.reranker_options]\nrevision = \"abc\"\n";
        let m = Config::parse(text).unwrap().models;
        assert_eq!(m.embedding, "BAAI/bge-large-en-v1.5");
        assert_eq!(m.reranker, "mixedbread-ai/mxbai-rerank-base-v1", "the default fills the missing key");
        let e = &m.embedding_options;
        assert_eq!((e.revision.as_deref(), e.pooling, e.query_prefix.as_deref(), e.document_prefix.as_deref()), (Some("v1"), Some(Pooling::Last), Some("q: "), None));
        assert_eq!(m.reranker_options.revision.as_deref(), Some("abc"));
        assert!(Config::parse("vault = \"/v\"\n[models.embedding_options]\npooling = \"max\"\n").is_err());
        assert!(Config::parse("vault = \"/v\"\n[models.reranker_options]\npooling = \"cls\"\n").is_err());
    }

    #[test]
    fn missing_vault_is_rejected() {
        assert!(matches!(Config::parse("exclude = []\n"), Err(VltError::Config(_))));
    }

    #[test]
    fn default_file_parses_back() {
        let text = Config::default_toml(Path::new("/my vault"));
        assert!(text.contains("exclude = []"));
        assert!(text.contains('#'), "default file documents its keys with comments");
        let cfg = Config::parse(&text).unwrap();
        assert_eq!(cfg.vault, PathBuf::from("/my vault"));
        assert_eq!(cfg.search, SearchConfig::default());
        assert_eq!(cfg.models, ModelsConfig::default());
    }

    #[test]
    fn set_vault_preserves_the_rest_of_the_file() {
        let text = "# mine\nvault = \"/a\"\nexclude = [\"t/**\"] # keep\n";
        let out = Config::set_vault_in(text, Path::new("/b")).unwrap();
        assert!(out.contains("# mine"));
        assert!(out.contains("\"t/**\"] # keep"));
        assert_eq!(Config::parse(&out).unwrap().vault, PathBuf::from("/b"));
    }
}
