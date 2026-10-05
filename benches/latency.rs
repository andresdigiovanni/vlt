// benches/latency.rs — run: cargo bench --bench latency (needs the models from `vlt install` under vlt home)
use std::time::{Duration, Instant};
use vlt::{app::App, config::Config, search::SearchRequest};

const DOCS: usize = 5_000;
const SECTIONS: usize = 10;
const WORDS: &[&str] = &[
    "sqlite", "rust", "vector", "index", "query", "latency", "cache", "thread", "schema", "backup", "network", "graph",
    "token", "model", "search", "memory", "disk", "file", "lock", "journal",
];

fn percentile(mut v: Vec<Duration>, p: f64) -> Duration {
    v.sort();
    v[((v.len() as f64 - 1.0) * p).round() as usize]
}

fn rss_mb() -> f64 {
    let out = std::process::Command::new("ps")
        .args(["-o", "rss=", "-p", &std::process::id().to_string()])
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).trim().parse::<f64>().unwrap_or(0.0) / 1024.0
}

fn main() {
    let real_home = vlt::paths::vlt_home().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let vault = tmp.path().join("vault");
    for d in 0..DOCS {
        let dir = vault.join(format!("d{:02}", d % 50));
        std::fs::create_dir_all(&dir).unwrap();
        let mut text = format!("---\ntags: [t{}]\n---\n# Doc {d}\n\n", d % 20);
        for s in 0..SECTIONS {
            text.push_str(&format!("## Section {s}\n"));
            for w in 0..60 {
                text.push_str(WORDS[(d * 7 + s * 13 + w * 3) % WORDS.len()]);
                text.push(' ');
            }
            text.push_str("\n\n");
        }
        std::fs::write(dir.join(format!("doc{d}.md")), text).unwrap();
    }
    let home = tmp.path().join("home");
    std::fs::create_dir_all(home.join("index")).unwrap();
    std::os::unix::fs::symlink(real_home.join("models"), home.join("models")).unwrap();
    let cfg = Config::parse(&Config::default_toml(&vault)).unwrap();
    let app = App::new(home.clone(), cfg, false).unwrap();

    let t = Instant::now();
    app.reindex(false, &mut |_, _| {}).unwrap();
    let initial = t.elapsed();
    let t = Instant::now();
    app.reindex(false, &mut |_, _| {}).unwrap();
    let noop = t.elapsed();

    let queries = ["sqlite lock journal", "vector index latency", "rust thread memory", "graph token model", "backup disk file"];
    app.search(&SearchRequest { query: "warm up".into(), ..Default::default() }).unwrap();
    let mut warm = Vec::new();
    for i in 0..50 {
        let t = Instant::now();
        app.search(&SearchRequest { query: queries[i % queries.len()].into(), ..Default::default() }).unwrap();
        warm.push(t.elapsed());
    }
    let rss = rss_mb();

    let cfg_path = home.join("config.toml");
    std::fs::write(&cfg_path, Config::default_toml(&vault)).unwrap();
    let t = Instant::now();
    let ok = std::process::Command::new(env!("CARGO_BIN_EXE_vlt"))
        .args(["search", "sqlite lock journal", "--json"])
        .env("VLT_HOME", &home)
        .output()
        .unwrap()
        .status
        .success();
    let cold = t.elapsed();
    assert!(ok);

    println!("| metric | measured | target |");
    println!("| initial indexing ({DOCS} docs, {} chunks) | {:.1?} | < 10 min |", DOCS * SECTIONS, initial);
    println!("| no-change reindex | {:.1?} | < 200 ms |", noop);
    println!(
        "| search p50 / p95 (warm) | {:.1?} / {:.1?} | p95 < 300 ms |",
        percentile(warm.clone(), 0.5),
        percentile(warm, 0.95)
    );
    println!("| cold CLI search | {:.1?} | < 3 s |", cold);
    println!("| RSS with models loaded | {rss:.0} MB | < 400 MB |");
}
