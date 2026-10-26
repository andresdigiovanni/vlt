// benches/real_models.rs — run: cargo bench --bench real_models
// Checks the default models, or the Hugging Face repos named by VLT_BENCH_EMBEDDING and
// VLT_BENCH_RERANKER; any that is missing is downloaded into the vlt home's models dir.
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::Path;
use std::sync::{Arc, OnceLock};
use vlt::config::ModelsConfig;
use vlt::installer::models::{missing, pull, HF_BASE};
use vlt::ml::models::Models;
use vlt::ml::{Embedder, Reranker};

fn models() -> &'static Models {
    static MODELS: OnceLock<Models> = OnceLock::new();
    MODELS.get_or_init(|| {
        let mut cfg = ModelsConfig::default();
        if let Ok(id) = std::env::var("VLT_BENCH_EMBEDDING") {
            cfg.embedding = id;
        }
        if let Ok(id) = std::env::var("VLT_BENCH_RERANKER") {
            cfg.reranker = id;
        }
        let dir = vlt::paths::models_dir(&vlt::paths::vlt_home().unwrap());
        let models = Models::new(&cfg, &dir, false).unwrap();
        for m in models.refs() {
            if missing(&dir, &m.id, &m.revision) {
                println!("downloading {}", m.id);
                pull(&dir, &m.id, &m.revision, HF_BASE, &mut |_, _, _| {}).unwrap();
            }
        }
        models.check().unwrap();
        println!("embedding {}, reranker {}", cfg.embedding, cfg.reranker);
        models
    })
}

fn embedder() -> Arc<dyn Embedder> {
    models().embedder().unwrap()
}

fn cross_encoder() -> Arc<dyn Reranker> {
    models().reranker().unwrap()
}

fn real_embedder_ranks_related_text_higher() {
    let e = embedder();
    let v = e.embed(&["SQLite WAL mode allows concurrent readers", "database concurrency", "banana bread recipe"]).unwrap();
    let dot = |a: &[f32], b: &[f32]| a.iter().zip(b).map(|(x, y)| x * y).sum::<f32>();
    assert_eq!(v[0].len(), models().index_meta(Path::new("/v")).unwrap().dim);
    assert!((dot(&v[0], &v[0]) - 1.0).abs() < 1e-4);
    assert!(dot(&v[0], &v[1]) > dot(&v[0], &v[2]));
}

fn padding_does_not_change_embeddings() {
    let e = embedder();
    let alone = e.embed(&["short text"]).unwrap();
    let padded = e.embed(&["short text", &"a much longer text that forces padding ".repeat(20)]).unwrap();
    assert!(alone[0].iter().zip(&padded[0]).all(|(a, b)| (a - b).abs() < 1e-4));
}

fn progress_reports_each_forward_pass_once_with_the_same_vectors() {
    let e = embedder();
    let long = "SQLite in WAL mode lets many readers run concurrently with one writer. ".repeat(30);
    let mut texts: Vec<String> = (0..6).map(|i| format!("{i} {long}")).collect();
    texts.extend((0..10).map(|i| format!("short text {i}")));
    let refs: Vec<&str> = texts.iter().map(String::as_str).collect();
    let mut batches: Vec<Vec<usize>> = Vec::new();
    let v = e.embed_with_progress(&refs, &mut |b| batches.push(b.to_vec())).unwrap();
    assert!(batches.len() > 1, "{batches:?}");
    let mut reported = batches.concat();
    reported.sort();
    assert_eq!(reported, (0..refs.len()).collect::<Vec<_>>());
    let plain = e.embed(&refs).unwrap();
    assert!(v.iter().zip(&plain).all(|(a, b)| a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-5)));
}

fn queries_rank_their_answer_above_unrelated_text() {
    let e = embedder();
    let q = e.embed_query("how do readers and writers share a sqlite database").unwrap();
    let d = e.embed(&["SQLite in WAL mode lets many readers run concurrently with one writer.", "Bake the bread for 40 minutes."]).unwrap();
    let dot = |a: &[f32], b: &[f32]| a.iter().zip(b).map(|(x, y)| x * y).sum::<f32>();
    assert!(dot(&q, &d[0]) > dot(&q, &d[1]));
}

fn real_cross_encoder_scores_relevant_pair_higher() {
    let r = cross_encoder();
    let s = r.score("how does sqlite handle concurrent readers", &["SQLite in WAL mode lets many readers run concurrently with one writer.", "Bake the bread for 40 minutes."]).unwrap();
    assert!(s.iter().all(|x| (0.0..=1.0).contains(x)));
    assert!(s[0] > 0.5 && s[1] < 0.1, "{s:?}");
}

fn long_document_is_truncated_not_the_query() {
    let r = cross_encoder();
    let long = format!("SQLite in WAL mode lets many readers run concurrently with one writer. {}", "Bake the bread. ".repeat(2000));
    let s = r.score("how does sqlite handle concurrent readers", &[&long, "short"]).unwrap();
    assert_eq!(s.len(), 2);
    // A cut query would score near zero. How much the filler lowers the score depends on the
    // model: ms-marco-MiniLM stays above 0.5, mxbai-rerank-large-v1 gives 0.197 (as in PyTorch).
    assert!(s[0] > 0.1 && s[0] > 50.0 * s[1], "{s:?}");
}

fn query_too_long_for_only_second_still_scores() {
    let r = cross_encoder();
    for (q, docs) in [("sqlite ".repeat(600), vec!["short doc"]), ("sqlite ".repeat(505), vec!["a b c d e f g h i j k l"])] {
        let s = r.score(&q, &docs).unwrap();
        assert_eq!(s.len(), docs.len());
        assert!(s.iter().all(|x| (0.0..=1.0).contains(x)), "{s:?}");
    }
}

fn main() {
    models();
    let checks: &[(&str, fn())] = &[
        ("real_embedder_ranks_related_text_higher", real_embedder_ranks_related_text_higher),
        ("padding_does_not_change_embeddings", padding_does_not_change_embeddings),
        ("progress_reports_each_forward_pass_once_with_the_same_vectors", progress_reports_each_forward_pass_once_with_the_same_vectors),
        ("queries_rank_their_answer_above_unrelated_text", queries_rank_their_answer_above_unrelated_text),
        ("real_cross_encoder_scores_relevant_pair_higher", real_cross_encoder_scores_relevant_pair_higher),
        ("long_document_is_truncated_not_the_query", long_document_is_truncated_not_the_query),
        ("query_too_long_for_only_second_still_scores", query_too_long_for_only_second_still_scores),
    ];
    let mut failed = Vec::new();
    for (name, check) in checks {
        if catch_unwind(AssertUnwindSafe(check)).is_ok() {
            println!("PASS {name}");
        } else {
            println!("FAIL {name}");
            failed.push(*name);
        }
    }
    println!("{} passed, {} failed", checks.len() - failed.len(), failed.len());
    if !failed.is_empty() {
        eprintln!("failed: {}", failed.join(", "));
        std::process::exit(1);
    }
}
