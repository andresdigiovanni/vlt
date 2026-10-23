// benches/eval/main.rs — run: make eval (cargo bench --bench eval)
// Retrieval quality of `vlt search` on eval/corpus with the queries in eval/queries.jsonl, each run as asked and
// as its key terms, compared with eval/baseline.json; spec: docs/specs/2026-10-20-rag-eval.md. Honours VLT_BENCH_EMBEDDING,
// VLT_BENCH_RERANKER, VLT_BENCH_SEARCH (the body of a [search] table) and VLT_FAKE_MODELS=1 (smoke run).
mod metrics;

use metrics::*;
use serde_json::{json, Map, Value};
use std::collections::{BTreeSet, HashMap};
use std::path::Path;
use std::time::Instant;
use vlt::app::App;
use vlt::config::{Config, SearchConfig};
use vlt::installer::models::{missing, pull, HF_BASE};
use vlt::ml::models::Models;
use vlt::ml::{Embedder, Reranker};
use vlt::search::{search_traced, SearchRequest, TracedChunk};
use vlt::storage::db::Db;

const K: usize = 5;
const SUMMARY: [&str; 13] = ["recall@5", "mrr@5", "ndcg@5", "hit@1", "tokens@5", "recall_bm25", "recall_vector",
    "recall_fused", "recall_reranked@5", "threshold_loss", "empty_rate", "latency_mean_ms", "latency_p95_ms"];
const ANSWERABLE: [&str; 10] = ["recall@5", "mrr@5", "ndcg@5", "hit@1", "tokens@5", "recall_bm25", "recall_vector",
    "recall_fused", "recall_reranked@5", "threshold_loss"];
/// The metrics compared with a paired bootstrap.
const PAIRED: [&str; 2] = ["recall@5", "mrr@5"];

fn hits(chunks: &[TracedChunk]) -> Vec<Hit<'_>> {
    chunks.iter().map(|c| (c.path.as_str(), c.content.as_str())).collect()
}

fn fail(msg: String) -> ! {
    eprintln!("{msg}");
    std::process::exit(1);
}

fn num(v: &Value, key: &str) -> Option<f64> {
    v.get(key).and_then(Value::as_f64)
}

fn fmt(key: &str, v: f64) -> String {
    if key.starts_with("latency") || key.starts_with("tokens") { format!("{v:.1}") } else { format!("{v:.3}") }
}

fn fmt_delta(key: &str, d: f64) -> String {
    if key.starts_with("latency") || key.starts_with("tokens") { format!("{d:+.1}") } else { format!("{d:+.3}") }
}

/// Mean of `key` over the rows whose category satisfies `keep`.
fn mean_of(rows: &[Value], key: &str, keep: impl Fn(&str) -> bool) -> f64 {
    let v: Vec<f64> = rows.iter().filter(|r| keep(r["category"].as_str().unwrap_or(""))).filter_map(|r| num(r, key)).collect();
    mean(&v)
}

/// Per paired metric, the 95% CI of latest − baseline over the answerable queries; `None` for a stale baseline.
fn compare(rows: &[Value], baseline: &Value) -> Option<HashMap<&'static str, (f64, f64)>> {
    let base_rows: HashMap<&str, &Value> = baseline["queries"]
        .as_array()?
        .iter()
        .filter_map(|r| Some((r["id"].as_str()?, r)))
        .collect();
    let ids: BTreeSet<&str> = rows.iter().filter_map(|r| r["id"].as_str()).collect();
    if ids != base_rows.keys().copied().collect::<BTreeSet<_>>() {
        return None;
    }
    let mut out = HashMap::new();
    for key in PAIRED {
        let mut diffs = Vec::new();
        for r in rows.iter().filter(|r| r["category"] != "unanswerable") {
            diffs.push(num(r, key)? - num(base_rows[r["id"].as_str()?], key)?);
        }
        out.insert(key, paired_bootstrap(&diffs, 10_000, 42));
    }
    Some(out)
}

/// Runs every query with the text `text` picks and returns `{"summary", "by_category", "queries"}`.
fn run_set(queries: &[Query], text: fn(&Query) -> &str, db: &Db, search: &SearchConfig, embedder: &dyn Embedder, reranker: &dyn Reranker) -> Value {
    let mut rows = Vec::new();
    let mut times = Vec::new();
    for q in queries {
        let req = SearchRequest { query: text(q).to_string(), limit: Some(K), ..Default::default() };
        let t = Instant::now();
        let (resp, trace) = search_traced(db.conn(), search, embedder, reranker, &req)
            .unwrap_or_else(|e| fail(format!("{}: {e}", q.id)));
        times.push(t.elapsed().as_secs_f64() * 1000.0);
        let final_hits: Vec<Hit> = resp.results.iter().map(|h| (h.document.path.as_str(), h.content.as_str())).collect();
        let mut row = Map::new();
        row.insert("id".into(), json!(q.id));
        row.insert("category".into(), json!(q.category));
        row.insert("results".into(), json!(resp.results.iter().map(|h| &h.chunk_id).collect::<Vec<_>>()));
        let scored: Vec<Value> = trace.reranked.iter().take(K)
            .map(|(c, s)| json!({"chunk_id": c.chunk_id, "score": s, "evidence": matched_evidence((&c.path, &c.content), &q.relevant)}))
            .collect();
        row.insert("reranked".into(), json!(scored));
        if q.relevant.is_empty() {
            row.insert("empty".into(), json!(resp.results.is_empty() as u8 as f64));
        } else {
            let rel = &q.relevant;
            let reranked: Vec<TracedChunk> = trace.reranked.iter().take(K).map(|(c, _)| c.clone()).collect();
            let recall = recall_at(&final_hits, rel, K);
            let recall_reranked = recall_at(&hits(&reranked), rel, K);
            row.insert("recall@5".into(), json!(recall));
            row.insert("mrr@5".into(), json!(rr_at(&final_hits, rel, K)));
            row.insert("ndcg@5".into(), json!(ndcg_at(&final_hits, rel, K)));
            row.insert("hit@1".into(), json!(hit_at_1(&final_hits, rel)));
            row.insert("tokens@5".into(), json!(tokens_at(&final_hits, K, |t| embedder.count_tokens(t))));
            row.insert("recall_bm25".into(), json!(recall_at(&hits(&trace.bm25), rel, usize::MAX)));
            row.insert("recall_vector".into(), json!(recall_at(&hits(&trace.vector), rel, usize::MAX)));
            row.insert("recall_fused".into(), json!(recall_at(&hits(&trace.fused), rel, usize::MAX)));
            row.insert("recall_reranked@5".into(), json!(recall_reranked));
            row.insert("threshold_loss".into(), json!((recall < recall_reranked) as u8 as f64));
        }
        rows.push(Value::Object(row));
    }

    let answerable = |c: &str| c != "unanswerable";
    let mut summary = Map::new();
    for key in ANSWERABLE {
        summary.insert(key.into(), json!(mean_of(&rows, key, answerable)));
    }
    summary.insert("empty_rate".into(), json!(mean_of(&rows, "empty", |c| c == "unanswerable")));
    summary.insert("latency_mean_ms".into(), json!(mean(&times)));
    summary.insert("latency_p95_ms".into(), json!(percentile(&times, 0.95)));
    let mut by_category = Map::new();
    for cat in CATEGORIES {
        let is = |c: &str| c == cat;
        let n = rows.iter().filter(|r| r["category"] == cat).count();
        let m = if cat == "unanswerable" {
            json!({"n": n, "empty_rate": mean_of(&rows, "empty", is)})
        } else {
            json!({"n": n, "recall@5": mean_of(&rows, "recall@5", is), "mrr@5": mean_of(&rows, "mrr@5", is)})
        };
        by_category.insert(cat.into(), m);
    }
    json!({"summary": summary, "by_category": by_category, "queries": rows})
}

/// Prints the summary and per-category tables of one set, compared with the baseline's same set when its ids match.
fn print_set(latest: &Value, baseline: Option<&Value>) {
    let comparison = baseline.filter(|b| !b.is_null()).and_then(|b| match compare(latest["queries"].as_array().unwrap(), b) {
        Some(ci) => Some((b, ci)),
        None => {
            println!("stale baseline: ids differ (re-run and copy target/eval/latest.json to eval/baseline.json)");
            None
        }
    });

    println!();
    println!("| metric | latest | baseline | Δ | 95% CI |");
    println!("|---|---|---|---|---|");
    for key in SUMMARY {
        let v = latest["summary"][key].as_f64().unwrap();
        let (base, delta, ci) = match &comparison {
            Some((b, ci)) => match num(&b["summary"], key) {
                Some(bv) => (
                    fmt(key, bv),
                    fmt_delta(key, v - bv),
                    ci.get(key).map(|(lo, hi)| format!("[{lo:+.3}, {hi:+.3}]")).unwrap_or_default(),
                ),
                None => Default::default(),
            },
            None => Default::default(),
        };
        println!("| {key} | {} | {base} | {delta} | {ci} |", fmt(key, v));
    }

    println!();
    println!("| category | n | recall@5 | Δ | mrr@5 | Δ | empty_rate |");
    println!("|---|---|---|---|---|---|---|");
    for cat in CATEGORIES {
        let c = &latest["by_category"][cat];
        let cell = |key: &str| num(c, key).map(|v| fmt(key, v)).unwrap_or_default();
        let delta = |key: &str| match &comparison {
            Some((b, _)) => num(c, key).zip(num(&b["by_category"][cat], key)).map(|(v, bv)| fmt_delta(key, v - bv)).unwrap_or_default(),
            None => String::new(),
        };
        println!(
            "| {cat} | {} | {} | {} | {} | {} | {} |",
            c["n"],
            cell("recall@5"),
            delta("recall@5"),
            cell("mrr@5"),
            delta("mrr@5"),
            cell("empty_rate")
        );
    }
}

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let text = std::fs::read_to_string(root.join("eval/queries.jsonl")).unwrap_or_else(|e| fail(format!("eval/queries.jsonl: {e}")));
    let queries = parse_queries(&text).unwrap_or_else(|e| fail(format!("eval/queries.jsonl: {e}")));

    let corpus = root.join("eval/corpus");
    let mut cfg = Config::parse(&Config::default_toml(&corpus)).unwrap_or_else(|e| fail(format!("config: {e}")));
    if let Ok(id) = std::env::var("VLT_BENCH_EMBEDDING") {
        cfg.models.embedding = id;
    }
    if let Ok(id) = std::env::var("VLT_BENCH_RERANKER") {
        cfg.models.reranker = id;
    }
    if let Ok(body) = std::env::var("VLT_BENCH_SEARCH") {
        cfg.search = toml_edit::de::from_str::<SearchConfig>(&body).unwrap_or_else(|e| fail(format!("VLT_BENCH_SEARCH: {e}")));
    }

    let real_home = vlt::paths::vlt_home().unwrap_or_else(|e| fail(format!("vlt home: {e}")));
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    std::fs::create_dir_all(home.join("index")).unwrap();
    std::os::unix::fs::symlink(vlt::paths::models_dir(&real_home), home.join("models")).unwrap();
    let fake = Models::fake_from_env();
    let app = App::new(home.clone(), cfg.clone(), fake).unwrap_or_else(|e| fail(format!("{e}")));
    if !fake {
        for m in app.models.refs() {
            if missing(&home.join("models"), &m.id, &m.revision) {
                println!("downloading {}", m.id);
                pull(&home.join("models"), &m.id, &m.revision, HF_BASE, &mut |_, _, _| {}).unwrap_or_else(|e| fail(format!("{}: {e}", m.id)));
            }
        }
    }
    let t = Instant::now();
    app.reindex(true, &mut |_, _| {}).unwrap_or_else(|e| fail(format!("reindex: {e}")));
    let indexing = t.elapsed();

    let meta = app.models.index_meta(&app.cfg.vault).unwrap_or_else(|e| fail(format!("{e}")));
    let db = Db::open(&app.db_path(), &meta).unwrap_or_else(|e| fail(format!("{e}")));
    let chunks: Vec<(String, String)> = {
        let mut stmt = db.conn().prepare("SELECT path, content FROM chunks").unwrap();
        stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?))).unwrap().collect::<Result<_, _>>().unwrap()
    };
    println!("indexed {} chunks in {indexing:.1?}", chunks.len());
    let bad = missing_evidence(&queries, |r| chunks.iter().any(|(p, c)| matches((p.as_str(), c.as_str()), r)));
    if !bad.is_empty() {
        fail(format!("evidence not inside one chunk: {bad:?}"));
    }

    let embedder = app.models.embedder().unwrap_or_else(|e| fail(format!("{e}")));
    let reranker = app.models.reranker().unwrap_or_else(|e| fail(format!("{e}")));
    let run = |text: fn(&Query) -> &str| run_set(&queries, text, &db, &cfg.search, embedder.as_ref(), reranker.as_ref());
    let asked = run(|q| q.query.as_str());
    let terms = run(|q| q.terms.as_str());
    let s = &cfg.search;
    let latest = json!({
        "config": {
            "search": {"k": s.k, "rrf_k": s.rrf_k, "rerank_top": s.rerank_top, "min_score": s.min_score},
            "embedding": cfg.models.embedding,
            "reranker": cfg.models.reranker,
            "fake_models": fake,
        },
        "summary": asked["summary"],
        "by_category": asked["by_category"],
        "queries": asked["queries"],
        "terms": terms,
    });
    let out_dir = root.join("target/eval");
    std::fs::create_dir_all(&out_dir).unwrap();
    let out = out_dir.join("latest.json");
    std::fs::write(&out, serde_json::to_string_pretty(&latest).unwrap()).unwrap();
    println!("wrote {}", out.display());

    let baseline_path = root.join("eval/baseline.json");
    let baseline: Option<Value> = baseline_path.exists().then(|| {
        let text = std::fs::read_to_string(&baseline_path).unwrap_or_else(|e| fail(format!("eval/baseline.json: {e}")));
        serde_json::from_str(&text).unwrap_or_else(|e| fail(format!("eval/baseline.json: {e}")))
    });
    println!();
    println!("## Questions as asked");
    print_set(&latest, baseline.as_ref());
    println!();
    println!("## Key terms");
    print_set(&latest["terms"], baseline.as_ref().map(|b| &b["terms"]));

    println!();
    println!(
        "embedding {}, reranker {}, k = {}, rrf_k = {}, rerank_top = {}, min_score = {}{}",
        cfg.models.embedding,
        cfg.models.reranker,
        s.k,
        s.rrf_k,
        s.rerank_top,
        s.min_score,
        if fake { " (fake models)" } else { "" }
    );
}
