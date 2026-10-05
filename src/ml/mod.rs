use crate::Result;

pub mod embeddings;
pub mod fake;
pub mod models;
pub mod registry;
pub mod reranker;

pub const QUERY_PREFIX: &str = "Represent this sentence for searching relevant passages: ";

pub trait Embedder: Send + Sync {
    /// L2-normalized vectors, one per input text.
    fn embed(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>>;
    /// Token count including special tokens.
    fn count_tokens(&self, text: &str) -> usize;
}

pub trait Reranker: Send + Sync {
    /// Sigmoid scores in [0, 1], one per document.
    fn score(&self, query: &str, docs: &[&str]) -> Result<Vec<f32>>;
}
