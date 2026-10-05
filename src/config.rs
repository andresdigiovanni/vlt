use crate::error::{Result, VltError};
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
    pub embedding: String,
    pub reranker: String,
    pub idle_unload_secs: u64,
}

impl Default for ModelsConfig {
    fn default() -> Self {
        ModelsConfig {
            embedding: "BAAI/bge-small-en-v1.5".into(),
            reranker: "cross-encoder/ms-marco-MiniLM-L-6-v2".into(),
            idle_unload_secs: 300,
        }
    }
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
        SearchConfig { k: 50, rrf_k: 60, rerank_top: 20, limit: 5, min_score: 0.3 }
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
embedding = "BAAI/bge-small-en-v1.5"                 # 384 dimensions, ~130 MB
reranker  = "cross-encoder/ms-marco-MiniLM-L-6-v2"   # ~90 MB
idle_unload_secs = 300                               # unload the models after 5 min without use

[search]
k = 50            # candidates per branch (BM25 and vectors)
rrf_k = 60        # Reciprocal Rank Fusion constant
rerank_top = 20   # candidates passed to the reranker
limit = 5         # results returned by default
min_score = 0.3   # reranker threshold [0, 1]; calibrate with real documents
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
        assert_eq!(cfg.models.embedding, "BAAI/bge-small-en-v1.5");
        assert_eq!(cfg.models.reranker, "cross-encoder/ms-marco-MiniLM-L-6-v2");
        assert_eq!(cfg.models.idle_unload_secs, 300);
        let s = &cfg.search;
        assert_eq!((s.k, s.rrf_k, s.rerank_top, s.limit), (50, 60, 20, 5));
        assert!((s.min_score - 0.3).abs() < 1e-12);
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
