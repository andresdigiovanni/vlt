// Unit tests of the eval bench's metrics (benches/eval/metrics.rs) and checks of the committed eval data.
#[path = "../benches/eval/metrics.rs"]
mod metrics;

use metrics::*;

fn ev(path: &str, evidence: &str) -> Evidence { Evidence { path: path.into(), evidence: evidence.into() } }

#[test]
fn norm_collapses_whitespace() {
    assert_eq!(norm("  a \n\t b  c "), "a b c");
}

#[test]
fn matching_needs_the_same_path_and_the_evidence_inside_the_content() {
    let r = ev("a.md", "the  quick\nfox");
    assert!(matches(("a.md", "Intro. The x; the quick fox ran."), &r), "whitespace-normalised containment");
    assert!(!matches(("b.md", "the quick fox"), &r), "other document");
    assert!(!matches(("a.md", "The Quick fox"), &r), "case-sensitive");
}

#[test]
fn tokens_at_sums_the_tokens_of_the_first_k_hits() {
    let hits: Vec<Hit> = vec![("a.md", "one two"), ("b.md", "three"), ("c.md", "four five six")];
    let words = |t: &str| t.split_whitespace().count();
    assert_eq!(tokens_at(&hits, 2, words), 3.0);
    assert_eq!(tokens_at(&hits, 5, words), 6.0, "fewer hits than k");
    assert_eq!(tokens_at(&[], 5, words), 0.0);
}

#[test]
fn ranking_metrics() {
    let rel = [ev("a.md", "alpha"), ev("b.md", "beta")];
    let hits: Vec<Hit> = vec![("c.md", "x"), ("a.md", "alpha"), ("a.md", "alpha again"), ("b.md", "beta")];
    assert_eq!(recall_at(&hits, &rel, 2), 0.5);
    assert_eq!(recall_at(&hits, &rel, 3), 0.5, "rank 4 is outside k = 3");
    assert_eq!(recall_at(&hits, &rel, 4), 1.0);
    assert_eq!(rr_at(&hits, &rel, 5), 0.5);
    assert_eq!(rr_at(&hits, &rel, 1), 0.0);
    assert_eq!(hit_at_1(&hits, &rel), 0.0);
    assert_eq!(hit_at_1(&hits[1..], &rel), 1.0);
    // gains 0,1,0,1 (the second alpha repeats a matched evidence): DCG = 1/log2(3) + 1/log2(5); IDCG = 1 + 1/log2(3)
    let want = (1.0 / 3f64.log2() + 1.0 / 5f64.log2()) / (1.0 + 1.0 / 3f64.log2());
    assert!((ndcg_at(&hits, &rel, 5) - want).abs() < 1e-12);
    let want3 = (1.0 / 3f64.log2()) / (1.0 + 1.0 / 3f64.log2());
    assert!((ndcg_at(&hits, &rel, 3) - want3).abs() < 1e-12, "rank 4 is outside k = 3");
    assert_eq!(ndcg_at(&hits, &[], 5), 0.0, "no evidence, no score");
    assert_eq!(ndcg_at(&[("a.md", "alpha"), ("b.md", "beta")], &rel, 5), 1.0);
    assert_eq!(recall_at(&hits, &rel, usize::MAX), 1.0);
    assert_eq!(ndcg_at(&hits, &rel, 0), 0.0);
    // one hit matching both evidences leaves nothing new for the next hit: gains 1,0
    let both = [ev("a.md", "alpha"), ev("a.md", "beta")];
    let want = 1.0 / (1.0 + 1.0 / 3f64.log2());
    assert!((ndcg_at(&[("a.md", "alpha beta"), ("a.md", "beta")], &both, 5) - want).abs() < 1e-12);
}

#[test]
fn no_hits_score_zero() {
    let rel = [ev("a.md", "alpha")];
    assert_eq!((recall_at(&[], &rel, 5), rr_at(&[], &rel, 5), hit_at_1(&[], &rel), ndcg_at(&[], &rel, 5)), (0.0, 0.0, 0.0, 0.0));
}

#[test]
fn parse_queries_accepts_valid_lines_and_rejects_bad_ones() {
    let ok = r#"{"id":"q1","query":"x","terms":"x t","category":"lexical","relevant":[{"path":"a.md","evidence":"e"}]}

{"id":"q2","query":"y","terms":"y t","category":"unanswerable","relevant":[]}"#;
    let parsed = parse_queries(ok).unwrap();
    assert_eq!(parsed.len(), 2, "blank lines are skipped");
    assert_eq!((parsed[0].query.as_str(), parsed[0].terms.as_str()), ("x", "x t"));
    for bad in [
        r#"{"id":"q1","query":"x","terms":"x t","category":"lexical","relevant":[]}"#,
        r#"{"id":"q1","query":"x","terms":"x t","category":"unanswerable","relevant":[{"path":"a.md","evidence":"e"}]}"#,
        r#"{"id":"q1","query":"x","terms":"x t","category":"other","relevant":[]}"#,
        r#"{"id":"q1","query":"x","category":"unanswerable","relevant":[]}"#,
        r#"{"id":"q1","query":"x","terms":"x t","category":"lexical","relevant":[{"path":"a.md","evidence":"e"}],"extra":1}"#,
        "{\"id\":\"q1\",\"query\":\"x\",\"terms\":\"x t\",\"category\":\"unanswerable\",\"relevant\":[]}\n{\"id\":\"q1\",\"query\":\"y\",\"terms\":\"y t\",\"category\":\"unanswerable\",\"relevant\":[]}",
    ] {
        assert!(parse_queries(bad).is_err(), "{bad}");
    }
}

#[test]
fn missing_evidence_lists_each_bad_query_once() {
    let qs = parse_queries(r#"{"id":"q1","query":"x","terms":"x t","category":"lexical","relevant":[{"path":"a.md","evidence":"ok"},{"path":"a.md","evidence":"bad"}]}
{"id":"q2","query":"y","terms":"y t","category":"lexical","relevant":[{"path":"a.md","evidence":"ok"}]}"#).unwrap();
    assert_eq!(missing_evidence(&qs, |r| r.evidence == "ok"), vec!["q1".to_string()]);
}

#[test]
fn mean_and_percentile() {
    assert_eq!(mean(&[]), 0.0);
    assert_eq!(percentile(&[], 0.5), 0.0);
    assert_eq!(mean(&[1.0, 2.0, 3.0]), 2.0);
    assert_eq!(percentile(&[3.0, 1.0, 2.0], 0.5), 2.0);
    assert_eq!(percentile(&[5.0, 1.0, 4.0, 2.0, 3.0], 0.95), 5.0);
}

#[test]
fn paired_bootstrap_is_deterministic_and_brackets_the_mean() {
    assert_eq!(paired_bootstrap(&[0.25; 40], 1000, 7), (0.25, 0.25), "0.25 sums exactly");
    let mixed: Vec<f64> = (0..100).map(|i| if i % 2 == 0 { 1.0 } else { -1.0 }).collect();
    let (lo, hi) = paired_bootstrap(&mixed, 10_000, 7);
    assert!(lo < 0.0 && hi > 0.0, "({lo}, {hi})");
    assert_eq!(paired_bootstrap(&mixed, 10_000, 7), (lo, hi), "same seed, same interval");
    let better: Vec<f64> = (0..100).map(|i| if i % 10 == 0 { 0.0 } else { 1.0 }).collect();
    assert!(paired_bootstrap(&better, 10_000, 7).0 > 0.0);
}

fn eval_dir() -> std::path::PathBuf { std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("eval") }

fn corpus() -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = std::fs::read_dir(eval_dir().join("corpus")).unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "md"))
        .map(|p| (p.file_name().unwrap().to_string_lossy().into_owned(), std::fs::read_to_string(&p).unwrap()))
        .collect();
    out.sort();
    out
}

/// True when `text` holds a citation marker such as `[12]`.
fn has_citation_marker(text: &str) -> bool {
    text.split('[').skip(1).any(|s| s.split_once(']').is_some_and(|(n, _)| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit())))
}

/// True for a bare bibliography entry such as `Macklin R. Future challenges ...: ...` (author, initials, title with a colon).
fn is_bibliography_line(l: &str) -> bool {
    let mut w = l.split(' ');
    let (Some(a), Some(b)) = (w.next(), w.next()) else { return false };
    l.chars().take(120).collect::<String>().contains(": ") && a.len() > 2 && a.chars().next().is_some_and(|c| c.is_uppercase()) && a.chars().all(|c| c.is_alphabetic())
        && (1..=2).contains(&b.trim_end_matches('.').len()) && b.ends_with('.') && b.chars().all(|c| c.is_uppercase() || c == '.')
}

#[test]
fn corpus_files_carry_provenance_and_no_wiki_markup() {
    let files = corpus();
    assert_eq!(files.len(), 45);
    for (name, text) in &files {
        let (front, body) = text.strip_prefix("---\n").and_then(|t| t.split_once("\n---\n")).unwrap_or_else(|| panic!("{name}: no frontmatter"));
        assert!(front.lines().any(|l| l.starts_with("title: ")), "{name}: title");
        assert!(front.lines().any(|l| l.starts_with("source: https://en.wikipedia.org/wiki/")), "{name}: source");
        assert!(front.lines().any(|l| l.strip_prefix("revision: ").is_some_and(|r| r.parse::<u64>().is_ok())), "{name}: revision");
        assert!(front.lines().any(|l| l == "license: CC BY-SA 4.0"), "{name}: license");
        assert!(body.trim_start().starts_with("# "), "{name}: body starts with the H1 title");
        assert!(body.matches("\n## ").count() >= 2, "{name}: keeps its sections");
        assert!(!has_citation_marker(body), "{name}: citation markers left");
        for (i, l) in body.lines().enumerate() {
            let h = l.to_lowercase();
            if l.starts_with("## ") || l.starts_with("### ") {
                assert!(!h.contains("external links"), "{name}:{}: dropped section kept: {l}", i + 1);
                assert!(h != "## resources", "{name}:{}: dropped section kept: {l}", i + 1);
            }
            assert!(!is_bibliography_line(l), "{name}:{}: bibliography line: {l}", i + 1);
            assert!(!l.contains(".\\* "), "{name}:{}: footnote asterisk leaked: {l}", i + 1);
            assert!(!l.contains("Wayback Machine") && !l.contains("Archived 20"), "{name}:{}: archive note: {l}", i + 1);
        }
        // a prose paragraph is one line: the HTML source's own line breaks must not survive
        let mut fenced = false;
        let lines: Vec<&str> = body.lines().filter(|l| { if l.starts_with("```") { fenced = !fenced; return true; } !fenced }).collect();
        for w in lines.windows(2) {
            let prose = |l: &str| l.chars().next().is_some_and(|c| c.is_alphabetic());
            assert!(!(prose(w[0]) && prose(w[1])), "{name}: paragraph broken across lines: {:?} / {:?}", w[0], w[1]);
        }
        for junk in ["[edit]", "## References", "## External links", "## See also", "## Further reading", "## Notes", "<div", "<span"] {
            assert!(!body.contains(junk), "{name}: contains {junk:?}");
        }
    }
}

#[test]
fn corpus_keeps_sections_whose_titles_only_mention_dropped_words() {
    let files = corpus();
    let text = |name: &str| files.iter().find(|(n, _)| n == name).unwrap_or_else(|| panic!("{name} missing")).1.clone();
    assert!(text("pharmacovigilance.md").lines().any(|l| l == "## Novel sources"));
    assert!(text("systematic-review.md").lines().any(|l| l == "### Searching for sources"));
}

#[test]
fn committed_queries_have_valid_labels_and_cover_the_corpus() {
    let queries = parse_queries(&std::fs::read_to_string(eval_dir().join("queries.jsonl")).unwrap()).unwrap();
    let texts: std::collections::HashMap<String, String> = corpus().into_iter().map(|(n, t)| (n, norm(&t))).collect();
    let bad = missing_evidence(&queries, |r| texts.get(&r.path).is_some_and(|t| t.contains(&norm(&r.evidence))));
    assert!(bad.is_empty(), "evidence not found verbatim in its document: {bad:?}");
    let count = |c: &str| queries.iter().filter(|q| q.category == c).count();
    for c in ["lexical", "paraphrase", "specific"] {
        assert!(count(c) >= 40, "{c}: {} queries", count(c));
    }
    assert!(count("unanswerable") >= 12, "unanswerable: {}", count("unanswerable"));
    let cited: std::collections::BTreeSet<&str> = queries.iter().flat_map(|q| q.relevant.iter().map(|r| r.path.as_str())).collect();
    let docs: std::collections::BTreeSet<&str> = texts.keys().map(String::as_str).collect();
    assert_eq!(cited, docs, "every document answers some query and every cited path exists");
}

#[test]
fn matched_evidence_lists_every_evidence_a_hit_contains() {
    let rel = [ev("a.md", "alpha"), ev("a.md", "beta"), ev("b.md", "alpha")];
    assert_eq!(matched_evidence(("a.md", "alpha and beta"), &rel), vec![0, 1]);
    assert_eq!(matched_evidence(("b.md", "alpha"), &rel), vec![2]);
    assert!(matched_evidence(("a.md", "gamma"), &rel).is_empty());
}
