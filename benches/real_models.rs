// benches/real_models.rs — run: cargo bench --bench real_models (needs the models from `vlt install` under vlt home)
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::PathBuf;
use vlt::ml::embeddings::BertEmbedder;
use vlt::ml::registry::{model_dir, ModelKind};
use vlt::ml::reranker::CrossEncoder;
use vlt::ml::{Embedder, Reranker};

fn dir(id: &str) -> PathBuf {
    model_dir(&vlt::paths::models_dir(&vlt::paths::vlt_home().unwrap()), id)
}

fn embedder() -> BertEmbedder {
    BertEmbedder::load(&dir("BAAI/bge-small-en-v1.5")).unwrap()
}

fn cross_encoder() -> CrossEncoder {
    CrossEncoder::load(&dir("cross-encoder/ms-marco-MiniLM-L-6-v2"), ModelKind::BertCrossEncoder).unwrap()
}

fn real_embedder_ranks_related_text_higher() {
    let e = embedder();
    let v = e.embed(&["SQLite WAL mode allows concurrent readers", "database concurrency", "banana bread recipe"]).unwrap();
    let dot = |a: &[f32], b: &[f32]| a.iter().zip(b).map(|(x, y)| x * y).sum::<f32>();
    assert_eq!(v[0].len(), 384);
    assert!((dot(&v[0], &v[0]) - 1.0).abs() < 1e-4);
    assert!(dot(&v[0], &v[1]) > dot(&v[0], &v[2]));
}

fn padding_does_not_change_embeddings() {
    let e = embedder();
    let alone = e.embed(&["short text"]).unwrap();
    let padded = e.embed(&["short text", &"a much longer text that forces padding ".repeat(20)]).unwrap();
    assert!(alone[0].iter().zip(&padded[0]).all(|(a, b)| (a - b).abs() < 1e-4));
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
    assert!(s[0] > 0.5, "{s:?}");
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
    let checks: &[(&str, fn())] = &[
        ("real_embedder_ranks_related_text_higher", real_embedder_ranks_related_text_higher),
        ("padding_does_not_change_embeddings", padding_does_not_change_embeddings),
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
