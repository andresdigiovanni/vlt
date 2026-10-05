use std::collections::HashSet;

use crate::ml::{Embedder, Reranker, QUERY_PREFIX};
use crate::Result;

pub const FAKE_MODEL_ID: &str = "fake";
pub const FAKE_DIM: usize = 384;

pub struct FakeEmbedder;
pub struct FakeReranker;

/// Lowercase alphanumeric runs.
pub fn tokens(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_lowercase())
        .collect()
}

fn fnv1a(s: &str) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in s.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

impl Embedder for FakeEmbedder {
    fn embed(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        Ok(texts
            .iter()
            .map(|text| {
                let text = text.strip_prefix(QUERY_PREFIX).unwrap_or(text);
                let mut v = vec![0f32; FAKE_DIM];
                for t in tokens(text) {
                    v[(fnv1a(&t) % FAKE_DIM as u64) as usize] += 1.0;
                }
                let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
                if norm > 0.0 {
                    v.iter_mut().for_each(|x| *x /= norm);
                }
                v
            })
            .collect())
    }

    fn count_tokens(&self, text: &str) -> usize {
        text.split_whitespace().count()
    }
}

impl Reranker for FakeReranker {
    fn score(&self, query: &str, docs: &[&str]) -> Result<Vec<f32>> {
        let q: HashSet<String> = tokens(query).into_iter().collect();
        Ok(docs
            .iter()
            .map(|d| {
                if q.is_empty() {
                    return 0.0;
                }
                let d: HashSet<String> = tokens(d).into_iter().collect();
                q.intersection(&d).count() as f32 / q.len() as f32
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ml::{Embedder, Reranker, QUERY_PREFIX};
    fn dot(a: &[f32], b: &[f32]) -> f32 { a.iter().zip(b).map(|(x, y)| x * y).sum() }

    #[test]
    fn embedder_is_deterministic_normalized_and_bag_of_words() {
        let e = FakeEmbedder;
        let v = e.embed(&["SQLite WAL mode", "sqlite wal mode", "cooking pasta", ""]).unwrap();
        assert_eq!(v.len(), 4);
        assert_eq!(v[0].len(), FAKE_DIM);
        assert_eq!(v[0], v[1]);
        assert!((dot(&v[0], &v[0]) - 1.0).abs() < 1e-5);
        assert!(dot(&v[0], &v[1]) > dot(&v[0], &v[2]));
        assert!(v[3].iter().all(|x| *x == 0.0));
        let q = e.embed(&[&format!("{QUERY_PREFIX}sqlite wal mode")]).unwrap();
        assert_eq!(q[0], v[0], "query prefix is ignored");
        assert_eq!(e.count_tokens("a b  c"), 3);
    }

    #[test]
    fn reranker_scores_query_term_coverage() {
        let s = FakeReranker.score("sqlite wal", &["WAL mode in SQLite", "only sqlite", "nothing"]).unwrap();
        assert_eq!(s, vec![1.0, 0.5, 0.0]);
        assert_eq!(FakeReranker.score("!!", &["x"]).unwrap(), vec![0.0]);
    }

    #[test]
    fn reranker_counts_distinct_query_tokens() {
        let s = FakeReranker.score("sqlite sqlite wal", &["sqlite"]).unwrap();
        assert_eq!(s, vec![0.5]);
    }

    #[test]
    fn fnv1a_matches_reference_vectors() {
        assert_eq!(fnv1a(""), 0xcbf29ce484222325);
        assert_eq!(fnv1a("a"), 0xaf63dc4c8601ec8c);
        assert_eq!(fnv1a("foobar"), 0x85944171f73967e8);
    }
}
