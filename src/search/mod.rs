pub mod filter;

use std::collections::HashMap;

use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{Map, Value};

use crate::config::SearchConfig;
use crate::ml::{Embedder, Reranker};
use crate::storage::db::f32_blob;
use crate::{Result, VltError};
use filter::Filter;

#[derive(Debug, Clone, Default)]
pub struct SearchRequest {
    pub query: String,
    pub where_: Option<Value>,
    pub path_prefix: Option<String>,
    pub limit: Option<usize>,
    pub min_score: Option<f64>,
}

#[derive(Debug, serde::Serialize)]
pub struct SearchResponse {
    pub results: Vec<SearchHit>,
}

#[derive(Debug, serde::Serialize)]
pub struct SearchHit {
    pub chunk_id: String,
    pub score: f64,
    pub heading_path: Vec<String>,
    pub lines: [usize; 2],
    pub content: String,
    pub document: DocumentRef,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct DocumentRef {
    pub path: String,
    pub title: String,
    pub properties: Map<String, Value>,
}

/// Each whitespace-separated term quoted (inner `"` doubled) and joined with ` OR `.
pub fn fts_query(query: &str) -> Option<String> {
    let terms: Vec<String> = query
        .split(|c: char| c.is_whitespace() || c == '\0')
        .filter(|t| !t.is_empty())
        .map(|t| format!("\"{}\"", t.replace('"', "\"\"")))
        .collect();
    if terms.is_empty() {
        None
    } else {
        Some(terms.join(" OR "))
    }
}

/// Reciprocal Rank Fusion: score = Σ 1/(k + rank), rank 1-based; ties go to the lower id.
pub fn rrf(lists: &[Vec<i64>], k: usize) -> Vec<i64> {
    let mut scores: HashMap<i64, f64> = HashMap::new();
    for list in lists {
        for (i, id) in list.iter().enumerate() {
            *scores.entry(*id).or_insert(0.0) += 1.0 / (k + i + 1) as f64;
        }
    }
    let mut ranked: Vec<(i64, f64)> = scores.into_iter().collect();
    ranked.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
    ranked.into_iter().map(|(id, _)| id).collect()
}

/// The `n` ids to rerank: the top ⌈n/2⌉ of `vector` and top ⌊n/2⌋ of `bm25`, topped up from the RRF
/// ranking when they overlap or a branch is short, in RRF order. Plain RRF favours ids found by both
/// branches, so a branch's best id found by it alone could miss the reranker.
fn rerank_candidates(bm25: &[i64], vector: &[i64], rrf_k: usize, n: usize) -> Vec<i64> {
    let picked: std::collections::HashSet<i64> = vector.iter().take(n.div_ceil(2)).chain(bm25.iter().take(n / 2)).copied().collect();
    let mut fill = n.saturating_sub(picked.len());
    rrf(&[bm25.to_vec(), vector.to_vec()], rrf_k)
        .into_iter()
        .filter(|id| {
            if picked.contains(id) {
                return true;
            }
            let take = fill > 0;
            fill = fill.saturating_sub(1);
            take
        })
        .collect()
}

fn parse_properties(raw: Option<String>) -> Result<Option<Map<String, Value>>> {
    raw.map(|s| serde_json::from_str(&s).map_err(|e| VltError::Internal(format!("stored properties are not valid JSON: {e}"))))
        .transpose()
}

pub fn load_filtered_paths(conn: &Connection, filter: &Filter) -> Result<Vec<String>> {
    let mut stmt = conn.prepare("SELECT path, properties_json, tags_json FROM files ORDER BY path")?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?, r.get::<_, String>(2)?)))?;
    let mut out = Vec::new();
    for row in rows {
        let (path, raw, tags) = row?;
        let mut props = parse_properties(raw)?.unwrap_or_default();
        // A `tags` condition matches the effective tags: frontmatter and body `#tags`.
        let tags: Vec<Value> =
            serde_json::from_str(&tags).map_err(|e| VltError::Internal(format!("stored tags are not valid JSON: {e}")))?;
        if tags.is_empty() {
            props.remove("tags");
        } else {
            props.insert("tags".to_string(), Value::Array(tags));
        }
        if filter.matches(&path, Some(&props)) {
            out.push(path);
        }
    }
    Ok(out)
}

const FILTER_CLAUSE: &str = "SELECT path FROM temp.filter_paths";

fn fill_filter_paths(conn: &Connection, paths: &[String]) -> Result<()> {
    conn.execute_batch("CREATE TEMP TABLE IF NOT EXISTS filter_paths (path TEXT PRIMARY KEY); DELETE FROM temp.filter_paths;")?;
    let mut insert = conn.prepare("INSERT INTO temp.filter_paths (path) VALUES (?1)")?;
    for p in paths {
        insert.execute([p])?;
    }
    Ok(())
}

fn bm25_ids(conn: &Connection, query: &str, filtered: bool, k: usize) -> Result<Vec<i64>> {
    let Some(fts) = fts_query(query) else { return Ok(Vec::new()) };
    // Filtered: the FTS match drives the loop (CROSS JOIN fixes the order) and each hit's path is
    // checked; a `rowid IN (subquery)` constraint made FTS5 evaluate the MATCH once per filtered rowid.
    let sql = if filtered {
        format!(
            "SELECT chunks_fts.rowid FROM chunks_fts CROSS JOIN chunks ch ON ch.id = chunks_fts.rowid \
             WHERE chunks_fts MATCH ?1 AND ch.path IN ({FILTER_CLAUSE}) ORDER BY bm25(chunks_fts) LIMIT ?2"
        )
    } else {
        "SELECT rowid FROM chunks_fts WHERE chunks_fts MATCH ?1 ORDER BY bm25(chunks_fts) LIMIT ?2".to_string()
    };
    let mut stmt = conn.prepare(&sql)?;
    let ids = stmt.query_map(params![fts, k as i64], |r| r.get(0))?.collect::<rusqlite::Result<Vec<i64>>>()?;
    Ok(ids)
}

fn vector_ids(conn: &Connection, embedder: &dyn Embedder, query: &str, filtered: bool, k: usize) -> Result<Vec<i64>> {
    let vector = embedder.embed_query(query)?;
    let restrict = if filtered { format!(" WHERE ch.path IN ({FILTER_CLAUSE})") } else { String::new() };
    let sql = format!(
        "SELECT ch.id FROM chunk_vectors v JOIN chunks ch ON ch.id = v.chunk_rowid{restrict} ORDER BY vec_distance_cosine(v.embedding, ?1) LIMIT ?2"
    );
    let mut stmt = conn.prepare(&sql)?;
    let ids = stmt.query_map(params![f32_blob(&vector), k as i64], |r| r.get(0))?.collect::<rusqlite::Result<Vec<i64>>>()?;
    Ok(ids)
}

struct Candidate {
    hit: SearchHit,
    section_id: String,
    fts_text: String,
}

fn load_candidate(conn: &Connection, id: i64) -> Result<Option<Candidate>> {
    let row = conn
        .query_row(
            "SELECT ch.chunk_id, ch.path, ch.heading_path_json, ch.start_line, ch.end_line, ch.content, f.title, f.properties_json, fts.text, ch.section_id
             FROM chunks ch JOIN files f ON f.path = ch.path JOIN chunks_fts fts ON fts.rowid = ch.id
             WHERE ch.id = ?1",
            [id],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, i64>(3)?,
                    r.get::<_, i64>(4)?,
                    r.get::<_, String>(5)?,
                    r.get::<_, String>(6)?,
                    r.get::<_, Option<String>>(7)?,
                    r.get::<_, String>(8)?,
                    r.get::<_, String>(9)?,
                ))
            },
        )
        .optional()?;
    let Some((chunk_id, path, heading_json, start, end, content, title, props, fts_text, section_id)) = row else { return Ok(None) };
    let heading_path: Vec<String> = serde_json::from_str(&heading_json)
        .map_err(|e| VltError::Internal(format!("stored heading path is not valid JSON: {e}")))?;
    Ok(Some(Candidate {
        hit: SearchHit {
            chunk_id,
            score: 0.0,
            heading_path,
            lines: [start as usize, end as usize],
            content,
            document: DocumentRef { path, title, properties: parse_properties(props)?.unwrap_or_default() },
        },
        section_id,
        fts_text,
    }))
}

/// One chunk as a pipeline stage saw it.
#[derive(Debug, Clone, PartialEq)]
pub struct TracedChunk { pub chunk_id: String, pub path: String, pub content: String }

impl TracedChunk {
    fn of(hit: &SearchHit) -> Self {
        TracedChunk { chunk_id: hit.chunk_id.clone(), path: hit.document.path.clone(), content: hit.content.clone() }
    }
}

/// What each stage of a search produced.
#[derive(Debug, Default)]
pub struct SearchTrace {
    /// BM25 branch, best first, up to `cfg.k`.
    pub bm25: Vec<TracedChunk>,
    /// Vector branch, best first, up to `cfg.k`.
    pub vector: Vec<TracedChunk>,
    /// What the reranker received: RRF order, up to `cfg.rerank_top`.
    pub fused: Vec<TracedChunk>,
    /// `fused` scored by the reranker, best first, before `min_score` and `limit`.
    pub reranked: Vec<(TracedChunk, f64)>,
}

fn traced(conn: &Connection, ids: &[i64]) -> Result<Vec<TracedChunk>> {
    let mut out = Vec::new();
    for id in ids {
        if let Some(c) = load_candidate(conn, *id)? {
            out.push(TracedChunk::of(&c.hit));
        }
    }
    Ok(out)
}

/// Hybrid search (PRD 4): filters → BM25 + vectors → each branch's top, in RRF order → reranker → threshold → limit.
pub fn search(conn: &Connection, cfg: &SearchConfig, embedder: &dyn Embedder, reranker: &dyn Reranker, req: &SearchRequest) -> Result<SearchResponse> {
    run(conn, cfg, embedder, reranker, req, None)
}

/// `search` plus what each stage produced, for the eval bench.
pub fn search_traced(conn: &Connection, cfg: &SearchConfig, embedder: &dyn Embedder, reranker: &dyn Reranker, req: &SearchRequest) -> Result<(SearchResponse, SearchTrace)> {
    let mut trace = SearchTrace::default();
    let response = run(conn, cfg, embedder, reranker, req, Some(&mut trace))?;
    Ok((response, trace))
}

fn run(conn: &Connection, cfg: &SearchConfig, embedder: &dyn Embedder, reranker: &dyn Reranker, req: &SearchRequest, mut trace: Option<&mut SearchTrace>) -> Result<SearchResponse> {
    let empty = || Ok(SearchResponse { results: Vec::new() });
    if req.query.trim().is_empty() {
        return Err(VltError::InvalidArguments("query must not be empty".into()));
    }
    let limit = req.limit.unwrap_or(cfg.limit);
    if limit < 1 {
        return Err(VltError::InvalidArguments("limit must be at least 1".into()));
    }
    let min_score = req.min_score.unwrap_or(cfg.min_score);
    if !(0.0..=1.0).contains(&min_score) {
        return Err(VltError::InvalidArguments("min_score must be between 0 and 1".into()));
    }
    let filter = Filter::new(req.where_.as_ref(), req.path_prefix.as_deref())?;
    // One snapshot for every read: a reindex committed mid-search must not mix versions or drop a section.
    let _snapshot = conn.unchecked_transaction()?;
    let filtered = !filter.is_empty();
    if filtered {
        let paths = load_filtered_paths(conn, &filter)?;
        if paths.is_empty() {
            return empty();
        }
        fill_filter_paths(conn, &paths)?;
    }

    let bm25 = bm25_ids(conn, &req.query, filtered, cfg.k)?;
    let vector = vector_ids(conn, embedder, &req.query, filtered, cfg.k)?;
    if let Some(t) = trace.as_deref_mut() {
        t.bm25 = traced(conn, &bm25)?;
        t.vector = traced(conn, &vector)?;
    }
    let fused = rerank_candidates(&bm25, &vector, cfg.rrf_k, cfg.rerank_top);

    let mut candidates = Vec::new();
    for id in fused {
        if let Some(c) = load_candidate(conn, id)? {
            candidates.push(c);
        }
    }
    if candidates.is_empty() {
        return empty();
    }
    if let Some(t) = trace.as_deref_mut() {
        t.fused = candidates.iter().map(|c| TracedChunk::of(&c.hit)).collect();
    }
    let docs: Vec<&str> = candidates.iter().map(|c| c.fts_text.as_str()).collect();
    let scores = reranker.score(&req.query, &docs)?;
    if scores.len() != candidates.len() {
        return Err(VltError::Internal("the reranker returned a wrong number of scores".into()));
    }

    let mut scored: Vec<(f64, Candidate)> = candidates
        .into_iter()
        .zip(scores)
        .map(|(c, s)| (s as f64, c))
        .collect();
    scored.sort_by(|a, b| b.0.total_cmp(&a.0));
    if let Some(t) = trace {
        t.reranked = scored.iter().map(|(s, c)| (TracedChunk::of(&c.hit), *s)).collect();
    }
    let mut seen = std::collections::HashSet::new();
    let mut results = Vec::new();
    for (s, c) in scored.into_iter().filter(|(s, _)| *s >= min_score) {
        if results.len() == limit {
            break;
        }
        if seen.insert((c.hit.document.path.clone(), c.section_id.clone())) {
            let mut hit = whole_section(conn, c)?;
            hit.score = (s * 1000.0).round() / 1000.0;
            results.push(hit);
        }
    }
    Ok(SearchResponse { results })
}

/// The candidate's hit widened to its whole section: every chunk of it, joined by a blank line.
fn whole_section(conn: &Connection, c: Candidate) -> Result<SearchHit> {
    let mut stmt = conn.prepare("SELECT content, start_line, end_line FROM chunks WHERE path = ?1 AND section_id = ?2 ORDER BY start_line")?;
    let parts = stmt
        .query_map(params![c.hit.document.path, c.section_id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?, r.get::<_, i64>(2)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let (Some(first), Some(last)) = (parts.first(), parts.last()) else {
        return Err(VltError::Internal(format!("section {} has no chunks in the index", c.section_id)));
    };
    let lines = [first.1 as usize, last.2 as usize];
    let content = parts.iter().map(|p| p.0.as_str()).collect::<Vec<_>>().join("\n\n");
    Ok(SearchHit { chunk_id: c.section_id, lines, content, ..c.hit })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::SearchConfig;
    use crate::indexer::{reindex, scanner::build_exclude};
    use crate::ml::fake::{FakeEmbedder, FakeReranker};
    use crate::storage::db::{Db, IndexMeta};
    use serde_json::json;
    use std::collections::HashSet;
    use std::{fs, sync::Arc};
    use tempfile::tempdir;

    fn setup() -> (tempfile::TempDir, Db) {
        let d = tempdir().unwrap();
        let v = d.path().join("vault");
        fs::create_dir_all(v.join("projects")).unwrap();
        fs::write(v.join("projects/db.md"), "---\ntags: [rust, db]\nstatus: in_progress\n---\n# Storage\n\n## Concurrency\nSQLite WAL mode allows concurrent readers.\n\n## Backups\nCopy the file nightly.\n").unwrap();
        fs::write(v.join("notes.md"), "---\ntags: [cooking]\n---\n# Pasta\n\nBoil water with salt.\n").unwrap();
        let mut db = Db::open(&d.path().join("vlt.db"), &IndexMeta { model_id: "fake".into(), dim: 384, fingerprint: "fake".into(), vault: "v".into() }).unwrap();
        let src = || -> crate::Result<Arc<dyn crate::ml::Embedder>> { Ok(Arc::new(FakeEmbedder)) };
        reindex(db.conn_mut(), &v, &build_exclude(&[]).unwrap(), &src, &mut |_| {}).unwrap();
        (d, db)
    }
    fn run(db: &Db, req: SearchRequest) -> crate::Result<SearchResponse> {
        search(db.conn(), &SearchConfig::default(), &FakeEmbedder, &FakeReranker, &req)
    }
    fn q(s: &str) -> SearchRequest { SearchRequest { query: s.into(), ..Default::default() } }

    #[test]
    fn a_tags_filter_matches_body_tags_and_normalized_frontmatter_tags() {
        let d = tempdir().unwrap();
        let v = d.path().join("vault");
        fs::create_dir_all(&v).unwrap();
        for (p, c) in [
            ("body.md", "Notes on #rust/async.\n"),
            ("fm.md", "---\ntags: '#rust, mcp'\n---\nx\n"),
            ("none.md", "---\ntags: [go]\n---\n`#rust`\n"),
        ] {
            fs::write(v.join(p), c).unwrap();
        }
        let mut db = Db::open(&d.path().join("vlt.db"), &IndexMeta { model_id: "fake".into(), dim: 384, fingerprint: "fake".into(), vault: "v".into() }).unwrap();
        let src = || -> crate::Result<Arc<dyn crate::ml::Embedder>> { Ok(Arc::new(FakeEmbedder)) };
        reindex(db.conn_mut(), &v, &build_exclude(&[]).unwrap(), &src, &mut |_| {}).unwrap();
        let paths = |w: Value| load_filtered_paths(db.conn(), &Filter::new(Some(&w), None).unwrap()).unwrap();
        assert_eq!(paths(json!({"tags": "rust"})), vec!["body.md", "fm.md"], "nested body tag and a split frontmatter string");
        assert_eq!(paths(json!({"tags": "mcp"})), vec!["fm.md"]);
        assert_eq!(paths(json!({"tags": "go"})), vec!["none.md"], "a tag in code is not a tag");
    }

    #[test]
    fn fts_query_escapes_terms_and_joins_with_or() {
        assert_eq!(fts_query("wal \"mode\" c-d:*").unwrap(), r#""wal" OR """mode""" OR "c-d:*""#);
        assert_eq!(fts_query("   "), None);
    }

    #[test]
    fn fts_query_treats_nul_as_separator() {
        assert_eq!(fts_query("a\u{0000}b").unwrap(), r#""a" OR "b""#);
        assert_eq!(fts_query("\u{0000}"), None);
    }

    #[test]
    fn nul_in_query_does_not_break_fts() {
        let (_d, db) = setup();
        assert!(run(&db, q("sqlite\u{0000}readers")).is_ok());
    }

    #[test]
    fn rrf_fuses_ranks() {
        assert_eq!(rrf(&[vec![1, 2, 3], vec![3, 1]], 60), vec![1, 3, 2]);
        assert_eq!(rrf(&[vec![], vec![7]], 60), vec![7]);
    }

    #[test]
    fn finds_the_relevant_section_with_payload() {
        let (_d, db) = setup();
        let r = run(&db, q("sqlite concurrent readers")).unwrap();
        let top = &r.results[0];
        assert_eq!(top.chunk_id, "projects/db.md#Concurrency");
        assert_eq!(top.heading_path, vec!["Storage".to_string(), "Concurrency".to_string()]);
        assert_eq!(top.lines, [7, 8]);
        assert_eq!(top.content, "SQLite WAL mode allows concurrent readers.");
        assert_eq!(top.document.title, "Storage");
        assert_eq!(top.document.properties["status"], json!("in_progress"));
        assert!(top.score >= SearchConfig::default().min_score && top.score <= 1.0);
        let v = serde_json::to_value(&r).unwrap();
        assert_eq!(v["results"][0]["document"]["path"], json!("projects/db.md"));
    }

    #[test]
    fn special_characters_do_not_break_fts() {
        let (_d, db) = setup();
        assert!(run(&db, q("\"sqlite\" -wal: *readers* AND (")).is_ok());
    }

    #[test]
    fn threshold_limit_and_filters() {
        let (_d, db) = setup();
        assert!(run(&db, q("quantum chromodynamics")).unwrap().results.is_empty(), "nothing above min_score");
        let strict = SearchRequest { min_score: Some(1.0), ..q("sqlite nightly") };
        assert!(run(&db, strict).unwrap().results.is_empty());
        let one = SearchRequest { limit: Some(1), min_score: Some(0.0), ..q("water file") };
        assert_eq!(run(&db, one).unwrap().results.len(), 1);
        let filtered = SearchRequest { where_: Some(json!({"tags": "cooking"})), min_score: Some(0.0), ..q("sqlite water") };
        let r = run(&db, filtered).unwrap();
        assert!(!r.results.is_empty() && r.results.iter().all(|h| h.document.path == "notes.md"));
        let prefixed = SearchRequest { path_prefix: Some("projects/".into()), min_score: Some(0.0), ..q("water") };
        assert!(run(&db, prefixed).unwrap().results.iter().all(|h| h.document.path.starts_with("projects/")));
        let none = SearchRequest { where_: Some(json!({"tags": "nope"})), ..q("sqlite") };
        assert!(run(&db, none).unwrap().results.is_empty());
    }

    #[test]
    fn invalid_arguments() {
        let (_d, db) = setup();
        for req in [q("  "), SearchRequest { limit: Some(0), ..q("x") }, SearchRequest { min_score: Some(1.5), ..q("x") },
                    SearchRequest { where_: Some(json!([1])), ..q("x") }] {
            assert!(matches!(run(&db, req), Err(crate::VltError::InvalidArguments(_))));
        }
    }

    struct SpyReranker {
        scores: Vec<f32>,
        seen: std::sync::Mutex<Vec<(String, Vec<String>)>>,
    }
    impl SpyReranker {
        fn new(scores: Vec<f32>) -> Self { SpyReranker { scores, seen: Default::default() } }
        fn docs(&self) -> Vec<String> { self.seen.lock().unwrap().iter().flat_map(|(_, d)| d.clone()).collect() }
    }
    impl Reranker for SpyReranker {
        fn score(&self, query: &str, docs: &[&str]) -> crate::Result<Vec<f32>> {
            self.seen.lock().unwrap().push((query.to_string(), docs.iter().map(|d| d.to_string()).collect()));
            Ok((0..docs.len()).map(|i| self.scores.get(i).copied().unwrap_or(*self.scores.last().unwrap())).collect())
        }
    }

    struct FixedEmbedder {
        vector: Vec<f32>,
        seen: std::sync::Mutex<Vec<String>>,
    }
    impl Embedder for FixedEmbedder {
        fn embed(&self, texts: &[&str]) -> crate::Result<Vec<Vec<f32>>> {
            self.seen.lock().unwrap().extend(texts.iter().map(|t| t.to_string()));
            Ok(texts.iter().map(|_| self.vector.clone()).collect())
        }
        fn embed_query(&self, query: &str) -> crate::Result<Vec<f32>> {
            self.seen.lock().unwrap().push(format!("query:{query}"));
            Ok(self.vector.clone())
        }
        fn count_tokens(&self, text: &str) -> usize { text.split_whitespace().count() }
    }

    fn fts_text_of(db: &Db, chunk_id: &str) -> String {
        db.conn().query_row("SELECT f.text FROM chunks ch JOIN chunks_fts f ON f.rowid = ch.id WHERE ch.chunk_id = ?1", [chunk_id], |r| r.get(0)).unwrap()
    }
    fn chunk_count(db: &Db) -> usize {
        db.conn().query_row("SELECT count(*) FROM chunks", [], |r| r.get::<_, i64>(0)).unwrap() as usize
    }
    fn all(s: &str) -> SearchRequest { SearchRequest { limit: Some(100), min_score: Some(0.0), ..q(s) } }

    #[test]
    fn search_traced_returns_the_same_response_and_every_stage() {
        let (_d, db) = setup();
        let cfg = SearchConfig::default();
        let req = SearchRequest { min_score: Some(0.05), ..q("sqlite concurrent readers") };
        let plain = search(db.conn(), &cfg, &FakeEmbedder, &SpyReranker::new(vec![0.9, 0.01, 0.5]), &req).unwrap();
        let spy = SpyReranker::new(vec![0.9, 0.01, 0.5]);
        let (traced, trace) = search_traced(db.conn(), &cfg, &FakeEmbedder, &spy, &req).unwrap();
        assert_eq!(serde_json::to_value(&traced).unwrap(), serde_json::to_value(&plain).unwrap());
        assert_eq!(trace.bm25.iter().map(|c| c.chunk_id.as_str()).collect::<Vec<_>>(), vec!["projects/db.md#Concurrency"]);
        assert_eq!(trace.vector.len(), chunk_count(&db), "k = 50 exceeds the 3 chunks");
        let fused_text: Vec<String> = trace.fused.iter().map(|c| fts_text_of(&db, &c.chunk_id)).collect();
        assert_eq!(fused_text, spy.docs(), "fused is what the reranker received, in order");
        assert_eq!(trace.reranked.len(), trace.fused.len(), "reranked keeps what min_score drops");
        assert!(trace.reranked.windows(2).all(|w| w[0].1 >= w[1].1), "best first");
        assert!(trace.reranked.iter().any(|(_, s)| *s < 0.05));
        let c = trace.fused.iter().find(|c| c.chunk_id == "projects/db.md#Concurrency").unwrap();
        assert_eq!((c.path.as_str(), c.content.as_str()), ("projects/db.md", "SQLite WAL mode allows concurrent readers."));
    }

    #[test]
    fn rerank_candidates_keep_each_branch_top_in_rrf_order() {
        let bm25: Vec<i64> = (1..=12).collect();
        let vector: Vec<i64> = [20, 2, 3, 4, 5, 6, 7, 8, 9, 10, 30].to_vec();
        assert_eq!(rrf(&[bm25.clone(), vector.clone()], 60).into_iter().take(10).collect::<Vec<_>>(), vec![2, 3, 4, 5, 6, 7, 8, 9, 10, 1],
            "plain RRF drops the vector branch's best, found by vectors alone");
        assert_eq!(rerank_candidates(&bm25, &vector, 60, 10), vec![2, 3, 4, 5, 6, 7, 8, 9, 1, 20], "top 5 of each branch, filled and ordered by RRF");
        assert_eq!(rerank_candidates(&bm25, &vector, 60, 3), vec![2, 1, 20], "odd: the extra slot goes to the vector branch");
        assert_eq!(rerank_candidates(&bm25, &vector, 60, 1), vec![20]);
    }

    #[test]
    fn rerank_candidates_fill_from_rrf_when_a_branch_is_short() {
        assert_eq!(rerank_candidates(&[], &[7, 3, 9], 60, 10), vec![7, 3, 9]);
        assert_eq!(rerank_candidates(&[4], &[7, 3, 9, 5], 60, 3), vec![4, 7, 3]);
        assert_eq!(rerank_candidates(&[1, 2], &[1, 2], 60, 4), vec![1, 2]);
    }

    #[test]
    fn rrf_rank_is_one_based_and_ties_go_to_the_lower_id() {
        assert_eq!(rrf(&[vec![1, 2], vec![3, 2]], 1), vec![2, 1, 3]);
        assert_eq!(rrf(&[vec![9, 4], vec![2, 7]], 60), vec![2, 9, 4, 7]);
        let singles: Vec<Vec<i64>> = [5, 3, 8, 1, 7, 2, 6, 4].iter().map(|i| vec![*i]).collect();
        assert_eq!(rrf(&singles, 60), (1..=8).collect::<Vec<i64>>());
    }

    #[test]
    fn min_score_is_inclusive() {
        let (_d, db) = setup();
        let r = run(&db, SearchRequest { min_score: Some(0.5), ..q("sqlite nightly") }).unwrap();
        assert!(!r.results.is_empty());
        assert!(r.results.iter().all(|h| h.score == 0.5));
    }

    #[test]
    fn reranker_scores_fts_text_and_results_sort_by_score() {
        let (_d, db) = setup();
        let spy = SpyReranker::new(vec![0.1, 0.2, 0.3, 0.4, 0.5, 0.6]);
        let r = search(db.conn(), &SearchConfig::default(), &FakeEmbedder, &spy, &all("sqlite wal")).unwrap();
        let docs = spy.docs();
        assert_eq!(spy.seen.lock().unwrap()[0].0, "sqlite wal", "reranker gets the raw query");
        assert_eq!(docs.len(), chunk_count(&db));
        assert_eq!(r.results.len(), docs.len());
        let order: Vec<String> = r.results.iter().map(|h| fts_text_of(&db, &h.chunk_id)).collect();
        let reversed: Vec<String> = docs.iter().rev().cloned().collect();
        assert_eq!(order, reversed, "highest reranker score first");
        assert!(r.results[0].content != docs[docs.len() - 1], "reranker input is the fts text, not the content");
    }

    #[test]
    fn equal_scores_keep_rrf_order() {
        let (_d, db) = setup();
        let spy = SpyReranker::new(vec![0.5]);
        let r = search(db.conn(), &SearchConfig::default(), &FakeEmbedder, &spy, &all("sqlite wal")).unwrap();
        let order: Vec<String> = r.results.iter().map(|h| fts_text_of(&db, &h.chunk_id)).collect();
        assert_eq!(order, spy.docs());
    }

    #[test]
    fn rerank_top_truncates_before_reranking() {
        let (_d, db) = setup();
        let spy = SpyReranker::new(vec![0.5]);
        let cfg = SearchConfig { rerank_top: 1, ..SearchConfig::default() };
        let r = search(db.conn(), &cfg, &FakeEmbedder, &spy, &all("sqlite concurrent readers")).unwrap();
        assert_eq!(spy.docs().len(), 1);
        assert_eq!(r.results.len(), 1);
        assert_eq!(r.results[0].chunk_id, "projects/db.md#Concurrency");
    }

    #[test]
    fn score_is_rounded_to_three_decimals() {
        let (_d, db) = setup();
        let spy = SpyReranker::new(vec![0.12345]);
        let r = search(db.conn(), &SearchConfig::default(), &FakeEmbedder, &spy, &all("sqlite")).unwrap();
        assert!(r.results.iter().all(|h| h.score == 0.123), "{:?}", r.results.iter().map(|h| h.score).collect::<Vec<_>>());
    }

    #[test]
    fn both_branches_contribute_and_the_vector_branch_embeds_the_query_as_a_query() {
        let (_d, db) = setup();
        let pasta = fts_text_of(&db, "notes.md#Pasta");
        let backups = fts_text_of(&db, "projects/db.md#Backups");
        let embedder = FixedEmbedder { vector: FakeEmbedder.embed(&[&pasta]).unwrap().remove(0), seen: Default::default() };
        let spy = SpyReranker::new(vec![0.5]);
        let cfg = SearchConfig { k: 1, ..SearchConfig::default() };
        search(db.conn(), &cfg, &embedder, &spy, &all("copy nightly wal")).unwrap();
        let mut docs = spy.docs();
        docs.sort();
        let mut want = vec![backups, pasta];
        want.sort();
        assert_eq!(docs, want, "BM25 top-1 is Backups, vector top-1 is Pasta");
        assert_eq!(*embedder.seen.lock().unwrap(), vec!["query:copy nightly wal"], "the model's query prompt applies");
    }

    #[test]
    fn the_vector_branch_best_reaches_the_reranker_when_plain_rrf_would_drop_it() {
        let (_d, db) = setup();
        let pasta = fts_text_of(&db, "notes.md#Pasta");
        let embedder = FixedEmbedder { vector: FakeEmbedder.embed(&[&pasta]).unwrap().remove(0), seen: Default::default() };
        let spy = SpyReranker::new(vec![0.5]);
        let cfg = SearchConfig { rerank_top: 2, ..SearchConfig::default() };
        let (_, trace) = search_traced(db.conn(), &cfg, &embedder, &spy, &all("sqlite copy")).unwrap();
        assert_eq!(trace.vector[0].chunk_id, "notes.md#Pasta", "vectors alone rank Pasta first");
        assert_eq!(trace.bm25.len(), 2, "BM25 finds both db.md sections, which plain RRF ranks above Pasta");
        assert!(spy.docs().contains(&pasta));
    }

    /// 30 files under `a/` and `b/`, every third tagged `x`; several share identical text, so BM25 ties.
    fn bm25_vault() -> (tempfile::TempDir, Db) {
        let d = tempdir().unwrap();
        let v = d.path().join("vault");
        fs::create_dir_all(v.join("a")).unwrap();
        fs::create_dir_all(v.join("b")).unwrap();
        for i in 0..30 {
            let dir = if i % 2 == 0 { "a" } else { "b" };
            let tags = if i % 3 == 0 { "[x]" } else { "[y]" };
            let body = match i % 5 {
                0 | 1 => "apple banana cherry".to_string(),
                2 => "apple apple apple banana".to_string(),
                3 => format!("{} apple", "filler ".repeat(i)),
                _ => "cherry only here".to_string(),
            };
            fs::write(v.join(format!("{dir}/n{i:02}.md")), format!("---\ntags: {tags}\n---\n{body}\n")).unwrap();
        }
        let mut db = Db::open(&d.path().join("vlt.db"), &IndexMeta { model_id: "fake".into(), dim: 384, fingerprint: "fake".into(), vault: "v".into() }).unwrap();
        let src = || -> crate::Result<Arc<dyn crate::ml::Embedder>> { Ok(Arc::new(FakeEmbedder)) };
        reindex(db.conn_mut(), &v, &build_exclude(&[]).unwrap(), &src, &mut |_| {}).unwrap();
        (d, db)
    }

    fn path_of(db: &Db, id: i64) -> String {
        db.conn().query_row("SELECT path FROM chunks WHERE id = ?1", [id], |r| r.get(0)).unwrap()
    }

    #[test]
    fn filtered_bm25_equals_unfiltered_bm25_restricted_to_the_filter() {
        let (_d, db) = bm25_vault();
        let conn = db.conn();
        let mut stmt = conn.prepare("SELECT bm25(chunks_fts) FROM chunks_fts WHERE chunks_fts MATCH '\"apple\" OR \"banana\"'").unwrap();
        let scores: Vec<f64> = stmt.query_map([], |r| r.get(0)).unwrap().map(|r| r.unwrap()).collect();
        let distinct: HashSet<u64> = scores.iter().map(|s| s.to_bits()).collect();
        assert!(distinct.len() < scores.len(), "the vault has BM25 ties");
        let cases = [
            (None, Some("a/")),
            (Some(json!({"tags": "x"})), None),
            (Some(json!({"tags": "x"})), Some("b/")),
            (Some(json!({"tags": "y"})), Some("a/")),
        ];
        for (where_, prefix) in cases {
            let filter = Filter::new(where_.as_ref(), prefix).unwrap();
            let paths = load_filtered_paths(conn, &filter).unwrap();
            assert!(!paths.is_empty());
            fill_filter_paths(conn, &paths).unwrap();
            let everything = bm25_ids(conn, "apple banana", false, 1000).unwrap();
            for k in [1, 3, 5, 100] {
                let want: Vec<i64> = everything.iter().copied().filter(|id| paths.contains(&path_of(&db, *id))).take(k).collect();
                assert!(!want.is_empty());
                assert_eq!(bm25_ids(conn, "apple banana", true, k).unwrap(), want, "where {where_:?} prefix {prefix:?} k {k}");
            }
        }
    }

    #[test]
    fn filtered_bm25_with_an_empty_filter_set_finds_nothing() {
        let (_d, db) = bm25_vault();
        fill_filter_paths(db.conn(), &[]).unwrap();
        assert!(!bm25_ids(db.conn(), "apple", false, 10).unwrap().is_empty());
        assert!(bm25_ids(db.conn(), "apple", true, 10).unwrap().is_empty());
    }

    /// `big.md`: sections Topic (lines 3-8) and Other (lines 10-13), each paragraph too long to share a chunk.
    fn split_vault() -> (tempfile::TempDir, Db) {
        let d = tempdir().unwrap();
        let v = d.path().join("vault");
        fs::create_dir_all(&v).unwrap();
        let p = |tag: &str| format!("zebra {tag} {}", (0..300).map(|i| format!("w{i}")).collect::<Vec<_>>().join(" "));
        let text = format!("# Big\n\n## Topic\n{}\n\n{}\n\n{}\n\n## Other\n{}\n\n{}\n", p("one"), p("two"), p("three"), p("four"), p("five"));
        fs::write(v.join("big.md"), text).unwrap();
        let mut db = Db::open(&d.path().join("vlt.db"), &IndexMeta { model_id: "fake".into(), dim: 384, fingerprint: "fake".into(), vault: "v".into() }).unwrap();
        let src = || -> crate::Result<Arc<dyn crate::ml::Embedder>> { Ok(Arc::new(FakeEmbedder)) };
        reindex(db.conn_mut(), &v, &build_exclude(&[]).unwrap(), &src, &mut |_| {}).unwrap();
        assert_eq!(chunk_count(&db), 5, "every paragraph is its own chunk");
        (d, db)
    }

    #[test]
    fn a_hit_returns_its_whole_section_once() {
        let (_d, db) = split_vault();
        let r = run(&db, all("zebra")).unwrap();
        assert_eq!(r.results.iter().map(|h| h.chunk_id.as_str()).collect::<HashSet<_>>(), HashSet::from(["big.md#Topic", "big.md#Other"]));
        assert_eq!(r.results.len(), 2, "one result per section");
        let topic = r.results.iter().find(|h| h.chunk_id == "big.md#Topic").unwrap();
        let body: Vec<String> = fs::read_to_string(_d.path().join("vault/big.md")).unwrap().lines().map(String::from).collect();
        assert_eq!(topic.content, body[3..8].join("\n"));
        assert_eq!(topic.lines, [3, 8]);
        assert_eq!(topic.heading_path, vec!["Big".to_string(), "Topic".to_string()]);
    }

    #[test]
    fn limit_counts_sections_not_chunks() {
        let (_d, db) = split_vault();
        let r = run(&db, SearchRequest { limit: Some(2), ..all("zebra one two") }).unwrap();
        let ids: Vec<&str> = r.results.iter().map(|h| h.chunk_id.as_str()).collect();
        assert_eq!(ids, vec!["big.md#Topic", "big.md#Other"], "Topic's two best chunks fill one slot");
    }

    #[test]
    fn a_section_scores_as_its_best_chunk() {
        let (_d, db) = split_vault();
        let (r, trace) = search_traced(db.conn(), &SearchConfig::default(), &FakeEmbedder, &FakeReranker, &all("zebra three")).unwrap();
        let best = trace.reranked.iter().filter(|(c, _)| c.content.starts_with("zebra three")).map(|(_, s)| *s).fold(0.0, f64::max);
        assert!(trace.reranked.iter().any(|(c, s)| c.content.starts_with("zebra one") && *s < best), "the parts score differently");
        assert_eq!(r.results[0].chunk_id, "big.md#Topic");
        assert_eq!(r.results[0].score, (best * 1000.0).round() / 1000.0);
    }

    /// Deletes `big.md`'s chunks from another connection while it scores, as a concurrent reindex would.
    struct ReindexingReranker(std::path::PathBuf);
    impl Reranker for ReindexingReranker {
        fn score(&self, _query: &str, docs: &[&str]) -> crate::Result<Vec<f32>> {
            rusqlite::Connection::open(&self.0)?.execute("DELETE FROM chunks WHERE path = 'big.md'", [])?;
            Ok(vec![0.5; docs.len()])
        }
    }

    #[test]
    fn a_reindex_committed_mid_search_does_not_reach_its_results() {
        let (d, db) = split_vault();
        let r = search(db.conn(), &SearchConfig::default(), &FakeEmbedder, &ReindexingReranker(d.path().join("vlt.db")), &all("zebra")).unwrap();
        assert_eq!(chunk_count(&db), 0, "the other connection committed");
        let topic = r.results.iter().find(|h| h.document.path == "big.md" && h.heading_path.last().map(String::as_str) == Some("Topic")).unwrap();
        let body: Vec<String> = fs::read_to_string(d.path().join("vault/big.md")).unwrap().lines().map(String::from).collect();
        assert_eq!((topic.chunk_id.as_str(), topic.lines, topic.content.as_str()), ("big.md#Topic", [3, 8], body[3..8].join("\n").as_str()));
    }

    #[test]
    fn a_section_without_chunks_is_an_error_not_the_bare_chunk() {
        let (_d, db) = split_vault();
        let id: i64 = db.conn().query_row("SELECT id FROM chunks WHERE chunk_id = 'big.md#Topic [1/3]'", [], |r| r.get(0)).unwrap();
        let c = load_candidate(db.conn(), id).unwrap().unwrap();
        db.conn().execute("DELETE FROM chunks WHERE section_id = 'big.md#Topic'", []).unwrap();
        assert!(matches!(whole_section(db.conn(), c), Err(crate::VltError::Internal(_))));
    }
}
