use crate::Result;

pub mod embeddings;
pub mod fake;
pub mod models;
pub mod profile;
pub mod reranker;

pub trait Embedder: Send + Sync {
    /// L2-normalized vectors, one per indexed text.
    fn embed(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>>;
    /// `embed`, calling `on_batch` with the indices of the texts each forward pass embedded.
    /// By default all texts at once, after `embed`.
    fn embed_with_progress(&self, texts: &[&str], on_batch: &mut dyn FnMut(&[usize])) -> Result<Vec<Vec<f32>>> {
        let vectors = self.embed(texts)?;
        on_batch(&(0..texts.len()).collect::<Vec<_>>());
        Ok(vectors)
    }
    /// L2-normalized vector of a search query, with the model's query prompt applied.
    fn embed_query(&self, query: &str) -> Result<Vec<f32>> {
        self.embed(&[query])?
            .pop()
            .ok_or_else(|| crate::VltError::Internal("the embedder returned no vector".into()))
    }
    /// Token count including special tokens.
    fn count_tokens(&self, text: &str) -> usize;
}

pub trait Reranker: Send + Sync {
    /// Sigmoid scores in [0, 1], one per document.
    fn score(&self, query: &str, docs: &[&str]) -> Result<Vec<f32>>;
}
