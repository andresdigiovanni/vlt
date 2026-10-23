//! Retrieval metrics for the eval bench; definitions in docs/specs/2026-10-20-rag-eval.md.
use serde::Deserialize;
use std::collections::HashSet;

pub const CATEGORIES: [&str; 4] = ["lexical", "paraphrase", "specific", "unanswerable"];

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Evidence {
    pub path: String,
    pub evidence: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Query {
    pub id: String,
    pub query: String,
    /// The same question as a short key-term query, the way the Skill asks agents to search.
    pub terms: String,
    pub category: String,
    pub relevant: Vec<Evidence>,
}

/// A ranked result: (document path, chunk content).
pub type Hit<'a> = (&'a str, &'a str);

pub fn norm(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// One query per non-blank line; ids unique, categories known, `relevant` empty exactly for `unanswerable`.
pub fn parse_queries(text: &str) -> Result<Vec<Query>, String> {
    let mut ids = HashSet::new();
    let mut out = Vec::new();
    for (i, line) in text.lines().enumerate().filter(|(_, l)| !l.trim().is_empty()) {
        let q: Query = serde_json::from_str(line).map_err(|e| format!("line {}: {e}", i + 1))?;
        if !ids.insert(q.id.clone()) {
            return Err(format!("line {}: duplicate id {}", i + 1, q.id));
        }
        if !CATEGORIES.contains(&q.category.as_str()) {
            return Err(format!("{}: unknown category {}", q.id, q.category));
        }
        if (q.category == "unanswerable") != q.relevant.is_empty() {
            return Err(format!("{}: relevant must be empty exactly when the category is unanswerable", q.id));
        }
        out.push(q);
    }
    Ok(out)
}

pub fn matches(hit: Hit, r: &Evidence) -> bool {
    hit.0 == r.path && norm(hit.1).contains(&norm(&r.evidence))
}

/// Indices into `rel` of every evidence `hit` matches.
pub fn matched_evidence(hit: Hit, rel: &[Evidence]) -> Vec<usize> {
    (0..rel.len()).filter(|&j| matches(hit, &rel[j])).collect()
}

/// Ids of the queries with an evidence for which `found` is false.
pub fn missing_evidence(queries: &[Query], found: impl Fn(&Evidence) -> bool) -> Vec<String> {
    queries.iter().filter(|q| !q.relevant.iter().all(&found)).map(|q| q.id.clone()).collect()
}

pub fn recall_at(hits: &[Hit], rel: &[Evidence], k: usize) -> f64 {
    if rel.is_empty() {
        return 0.0;
    }
    rel.iter().filter(|r| hits.iter().take(k).any(|h| matches(*h, r))).count() as f64 / rel.len() as f64
}

pub fn rr_at(hits: &[Hit], rel: &[Evidence], k: usize) -> f64 {
    hits.iter().take(k).position(|h| rel.iter().any(|r| matches(*h, r))).map_or(0.0, |i| 1.0 / (i + 1) as f64)
}

pub fn hit_at_1(hits: &[Hit], rel: &[Evidence]) -> f64 {
    rr_at(hits, rel, 1)
}

/// Binary gains: a hit gains 1 when it matches an evidence no earlier hit matched.
pub fn ndcg_at(hits: &[Hit], rel: &[Evidence], k: usize) -> f64 {
    let discount = |i: usize| 1.0 / ((i + 2) as f64).log2();
    let mut seen = vec![false; rel.len()];
    let mut dcg = 0.0;
    for (i, h) in hits.iter().take(k).enumerate() {
        let m = matched_evidence(*h, rel);
        if m.iter().any(|&j| !seen[j]) {
            dcg += discount(i);
        }
        for j in m {
            seen[j] = true;
        }
    }
    let idcg: f64 = (0..k.min(rel.len())).map(discount).sum();
    if idcg == 0.0 { 0.0 } else { dcg / idcg }
}

/// Tokens of the first `k` hits' content, as `count` measures them.
pub fn tokens_at(hits: &[Hit], k: usize, count: impl Fn(&str) -> usize) -> f64 {
    hits.iter().take(k).map(|h| count(h.1)).sum::<usize>() as f64
}

pub fn mean(v: &[f64]) -> f64 {
    if v.is_empty() { 0.0 } else { v.iter().sum::<f64>() / v.len() as f64 }
}

pub fn percentile(v: &[f64], p: f64) -> f64 {
    if v.is_empty() {
        return 0.0;
    }
    let mut v = v.to_vec();
    v.sort_by(f64::total_cmp);
    v[((v.len() as f64 - 1.0) * p).round() as usize]
}

fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// 95% interval of the mean of `diffs` (latest − baseline per query), by resampling with replacement.
pub fn paired_bootstrap(diffs: &[f64], resamples: usize, seed: u64) -> (f64, f64) {
    if diffs.is_empty() {
        return (0.0, 0.0);
    }
    let mut state = seed;
    let n = diffs.len();
    let means: Vec<f64> = (0..resamples)
        .map(|_| (0..n).map(|_| diffs[(splitmix64(&mut state) % n as u64) as usize]).sum::<f64>() / n as f64)
        .collect();
    (percentile(&means, 0.025), percentile(&means, 0.975))
}
