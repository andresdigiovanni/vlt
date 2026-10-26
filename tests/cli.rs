// tests/cli.rs
use assert_cmd::Command;
use std::{fs, path::Path};
use tempfile::TempDir;

struct Env { dir: TempDir }
impl Env {
    fn new() -> Env {
        let dir = tempfile::tempdir().unwrap();
        let v = dir.path().join("vault");
        fs::create_dir_all(v.join("projects")).unwrap();
        fs::write(v.join("projects/db.md"), "---\ntags: [rust, db]\n---\n# Storage\n\n## Concurrency\nSQLite WAL mode allows concurrent readers.\n").unwrap();
        fs::write(v.join("notes.md"), "---\ntags: [cooking]\n---\n# Pasta\n\nBoil water. See [[db]].\n").unwrap();
        let e = Env { dir };
        e.write_config(&v);
        e
    }
    fn vault(&self) -> std::path::PathBuf { self.dir.path().join("vault") }
    fn write_config(&self, vault: &Path) {
        let home = self.dir.path().join("home");
        fs::create_dir_all(&home).unwrap();
        fs::write(home.join("config.toml"), vlt::config::Config::default_toml(vault)).unwrap();
    }
    fn min_score(&self, score: &str) {
        let path = self.dir.path().join("home/config.toml");
        let text = fs::read_to_string(&path).unwrap().replace("min_score = 0.02 ", &format!("min_score = {score} "));
        fs::write(path, text).unwrap();
    }
    fn cmd(&self) -> Command {
        let mut c = Command::cargo_bin("vlt").unwrap();
        c.env("VLT_HOME", self.dir.path().join("home")).env("VLT_FAKE_MODELS", "1").env("HOME", self.dir.path());
        c
    }
    fn json(&self, args: &[&str]) -> serde_json::Value {
        let out = self.cmd().args(args).output().unwrap();
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
        serde_json::from_slice(&out.stdout).unwrap()
    }
}

#[test]
fn reindex_reports_stats() {
    let e = Env::new();
    let out = e.cmd().arg("reindex").output().unwrap();
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains("added 2, updated 0, removed 0, unchanged 0"));
    let out = e.cmd().arg("reindex").output().unwrap();
    assert!(String::from_utf8_lossy(&out.stdout).contains("unchanged 2"));
}

#[test]
fn search_json_matches_payload_and_indexes_on_first_read() {
    let e = Env::new();
    let v = e.json(&["search", "sqlite concurrent readers", "--json"]);
    assert_eq!(v["results"][0]["chunk_id"], "projects/db.md#Concurrency");
    assert!(v["results"][0]["document"].get("abs_path").is_none());
    e.min_score("0");
    let v = e.json(&["search", "water sqlite", "--where", "tags=cooking", "--json"]);
    assert!(v["results"].as_array().unwrap().iter().all(|r| r["document"]["path"] == "notes.md"));
}

#[test]
fn search_human_output_shows_absolute_path() {
    let e = Env::new();
    let out = e.cmd().args(["search", "sqlite concurrent readers"]).output().unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains(&e.vault().join("projects/db.md").display().to_string()), "{text}");
    assert!(text.contains("Storage > Concurrency"));
}

#[test]
fn get_and_properties() {
    let e = Env::new();
    let v = e.json(&["get", "projects/db.md", "--json"]);
    assert_eq!(v["title"], "Storage");
    assert_eq!(v["backlinks"][0]["path"], "notes.md");
    let v = e.json(&["properties", "--json"]);
    assert_eq!(v["properties"][0]["key"], "tags");
    assert_eq!(e.json(&["properties", "--key", "nope", "--json"])["properties"], serde_json::json!([]), "--key limits the inventory to that key");
    let out = e.cmd().args(["get", "nope.md"]).output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).starts_with("error: document not found"));
}

#[test]
fn properties_with_a_key_lists_only_that_key_with_every_value() {
    let e = Env::new();
    fs::write(e.vault().join("todo.md"), "---\nstatus: done\n---\n# Todo\n").unwrap();
    let v = e.json(&["properties", "--key", "tags", "--json"]);
    assert_eq!(v["properties"].as_array().unwrap().len(), 1, "{v}");
    assert_eq!(v["properties"][0]["key"], "tags");
    assert_eq!(v["properties"][0]["top_values"], serde_json::json!([["cooking", 1], ["db", 1], ["rust", 1]]));
}

#[test]
fn properties_where_counts_only_the_selected_notes() {
    let e = Env::new();
    let v = e.json(&["properties", "--where", "tags=rust", "--json"]);
    assert_eq!(v["properties"][0]["top_values"], serde_json::json!([["db", 1], ["rust", 1]]), "{v}");
}

#[test]
fn folders_lists_each_folder_with_its_notes_and_top_keys() {
    let e = Env::new();
    fs::write(e.vault().join("projects/extra.md"), "---\nstatus: x\n---\nx\n").unwrap();
    let out = e.cmd().arg("folders").output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "FOLDER     DOCS  TOTAL  TOP KEYS\n/          1     3      tags (1)\nprojects/  2     2      status (1), tags (1)\n"
    );
    let v = e.json(&["folders", "--path-prefix", "projects/", "--json"]);
    assert_eq!(v, serde_json::json!({"folders": [{"path": "projects/", "documents": 2, "total": 2, "top_keys": [["status", 1], ["tags", 1]]}]}));
}

#[test]
fn audit_text_lists_each_section_with_its_lines() {
    let e = Env::new();
    let v = e.vault();
    fs::write(v.join("a.md"), "---\nRepo-Role: x\n---\nSee [[gone]] and [[db]].\n").unwrap();
    fs::write(v.join("b.md"), "---\nrepo_role: y\n---\nx\n").unwrap();
    fs::write(v.join("c.md"), "---\ntags: [Rust]\n---\nc\n").unwrap();
    fs::create_dir_all(v.join("g")).unwrap();
    for i in 1..=5 {
        let fm = if i < 5 { "---\nstatus: done\n---\n" } else { "" };
        fs::write(v.join(format!("g/{i}.md")), format!("{fm}g{i}\n")).unwrap();
    }
    let out = e.cmd().arg("audit").output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let text = String::from_utf8_lossy(&out.stdout);
    for block in [
        "value variants (1)\n  tags: Rust (1) / rust (1)\n",
        "rare keys (2)\n  Repo-Role  a.md\n  repo_role  b.md\n",
        "key variants (1)\n  Repo-Role (1) / repo_role (1)\n",
        "folder gaps (1)\n  g/  key status  4/5  missing: g/5.md\n",
        "broken links (1)\n  a.md:4  [[gone]]  broken\n",
        "orphans (",
        "no outgoing links (",
    ] {
        assert!(text.contains(block), "{block:?} not in\n{text}");
    }
}

#[test]
fn audit_caps_sections_and_says_when_there_is_nothing_to_report() {
    let e = Env::new();
    let v = e.vault();
    fs::write(v.join("projects/db.md"), "See [[notes]].\n").unwrap();
    fs::write(v.join("notes.md"), "See [[db]].\n").unwrap();
    let out = e.cmd().arg("audit").output().unwrap();
    assert_eq!(String::from_utf8_lossy(&out.stdout), "nothing to report\n");
    for i in 0..60 {
        fs::write(v.join(format!("o{i:02}.md")), "x\n").unwrap();
    }
    let out = e.cmd().arg("audit").output().unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("orphans (50 of 60)\n  o00.md\n"), "{text}");
    let json = e.json(&["audit", "--json"]);
    assert_eq!((json["orphans"]["total"].as_u64(), json["orphans"]["items"].as_array().unwrap().len()), (Some(60), 50));
}

#[test]
fn audit_text_shows_the_root_folder_as_a_slash_and_lists_every_note_line() {
    let e = Env::new();
    for i in 1..=4 {
        fs::write(e.vault().join(format!("r{i}.md")), "---\ntype: x\n---\nr\n").unwrap();
    }
    let out = e.cmd().arg("audit").output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let text = String::from_utf8_lossy(&out.stdout);
    for block in [
        "folder gaps (1)\n  /  key type  4/5  missing: notes.md\n",
        "orphans (5)\n  notes.md\n  r1.md\n  r2.md\n  r3.md\n  r4.md\n",
        "no outgoing links (5)\n  projects/db.md\n  r1.md\n  r2.md\n  r3.md\n  r4.md\n",
    ] {
        assert!(text.contains(block), "{block:?} not in\n{text}");
    }
}

#[test]
fn related_lists_the_closest_notes_with_their_link_state() {
    let e = Env::new();
    let v = e.json(&["related", "projects/db.md", "--json"]);
    assert_eq!(v["path"], "projects/db.md");
    assert_eq!(v["notes"][0]["path"], "notes.md");
    assert_eq!(v["notes"][0]["linked"], "from", "notes.md links to db");
    assert_eq!(v["mentions"], serde_json::json!([]));
    let out = e.cmd().args(["related", "projects/db.md", "--limit", "1"]).output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let text = String::from_utf8_lossy(&out.stdout);
    let mut lines = text.lines();
    assert!(lines.next().unwrap().starts_with("NOTE      SCORE  LINKED  SECTIONS"), "{text}");
    let row = lines.next().unwrap();
    assert!(row.starts_with("notes.md  ") && row.contains("  from    ") && row.contains(" → Pasta"), "{text}");
    assert_eq!(lines.next(), None, "no suggested tags or mentions here");
    let bad = e.cmd().args(["related", "projects/db.md", "--limit", "0"]).output().unwrap();
    assert!(!bad.status.success());
}

#[test]
fn related_text_lists_suggested_tags_mentions_and_paths_with_a_hash() {
    let e = Env::new();
    fs::write(e.vault().join("a.md"), "---\ntags: [sql]\n---\nStorage notes about sqlite wal.\n").unwrap();
    fs::write(e.vault().join("b.md"), "---\ntags: [sql]\n---\nMore storage on sqlite readers.\n").unwrap();
    fs::create_dir_all(e.vault().join("C#")).unwrap();
    fs::write(e.vault().join("C#/basics.md"), "# Types\nSQLite WAL mode allows concurrent readers.\n").unwrap();
    let out = e.cmd().args(["related", "projects/db.md"]).output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("suggested tags: sql (2)\n"), "{text}");
    assert!(text.contains("mentions:\n  a.md:4  Storage\n  b.md:4  Storage\n"), "{text}");
    let row = text.lines().find(|l| l.starts_with("C#/basics.md")).expect("C#/basics.md is related");
    assert!(row.ends_with("Concurrency → Types"), "{row}");
    let bad = e.cmd().args(["related", "projects/db.md", "--limit", "0"]).output().unwrap();
    assert_eq!(bad.status.code(), Some(1));
    assert_eq!(String::from_utf8_lossy(&bad.stderr), "error: limit must be at least 1\n");
}

#[test]
fn vault_change_requires_rebuild_until_reindex() {
    let e = Env::new();
    e.json(&["search", "sqlite", "--json"]);
    let other = e.dir.path().join("vault2");
    fs::create_dir_all(&other).unwrap();
    fs::write(other.join("z.md"), "# Zebra\n\nStripes.\n").unwrap();
    e.write_config(&other);
    let out = e.cmd().args(["search", "zebra"]).output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("vlt reindex --force"));
    assert!(e.cmd().arg("reindex").output().unwrap().status.success());
    let v = e.json(&["search", "zebra stripes", "--json"]);
    assert_eq!(v["results"][0]["document"]["path"], "z.md");
}

#[test]
fn missing_config_points_to_install() {
    let e = Env::new();
    fs::remove_file(e.dir.path().join("home/config.toml")).unwrap();
    let out = e.cmd().args(["search", "x"]).output().unwrap();
    assert!(String::from_utf8_lossy(&out.stderr).contains("vlt install"));
}

fn hold_lock(e: &Env) -> vlt::indexer::scanner::IndexLock {
    vlt::indexer::scanner::IndexLock::try_acquire(&e.dir.path().join("home/index")).unwrap().unwrap()
}

#[test]
fn reindex_skips_while_another_process_holds_the_lock() {
    let e = Env::new();
    let _lock = hold_lock(&e);
    let out = e.cmd().arg("reindex").output().unwrap();
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains("Another process is reindexing; skipped."));
}

#[test]
fn read_without_index_while_locked_is_an_error() {
    let e = Env::new();
    let _lock = hold_lock(&e);
    let out = e.cmd().args(["search", "sqlite"]).output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("being built by another process"));
}

#[test]
fn read_while_locked_uses_current_index_without_reindexing() {
    let e = Env::new();
    e.json(&["search", "sqlite", "--json"]);
    fs::write(e.vault().join("zebra.md"), "# Zebra\n\nStripes.\n").unwrap();
    let lock = hold_lock(&e);
    let out = e.cmd().args(["get", "zebra.md"]).output().unwrap();
    assert!(String::from_utf8_lossy(&out.stderr).contains("document not found"));
    drop(lock);
    let v = e.json(&["get", "zebra.md", "--json"]);
    assert_eq!(v["title"], "Zebra");
}

#[test]
fn reindex_force_rebuilds_everything() {
    let e = Env::new();
    e.cmd().arg("reindex").output().unwrap();
    let out = e.cmd().args(["reindex", "--force"]).output().unwrap();
    assert!(String::from_utf8_lossy(&out.stdout).contains("added 2, updated 0, removed 0, unchanged 0"));
}

#[test]
fn usage_errors_exit_with_2() {
    let e = Env::new();
    assert_eq!(e.cmd().arg("search").output().unwrap().status.code(), Some(2));
}

fn edit_meta(e: &Env, sql: &str) {
    let conn = rusqlite::Connection::open(e.dir.path().join("home/index/vlt.db")).unwrap();
    conn.execute_batch(sql).unwrap();
}

#[test]
fn schema_version_change_requires_rebuild_and_reindex_rebuilds() {
    let e = Env::new();
    e.cmd().arg("reindex").output().unwrap();
    edit_meta(&e, "UPDATE meta SET value = '0' WHERE key = 'schema_version'");
    let out = e.cmd().args(["search", "sqlite"]).output().unwrap();
    assert!(String::from_utf8_lossy(&out.stderr).contains("vlt reindex --force"));
    let out = e.cmd().arg("reindex").output().unwrap();
    assert!(String::from_utf8_lossy(&out.stdout).contains("added 2, updated 0, removed 0, unchanged 0"));
    assert!(e.cmd().args(["search", "sqlite"]).output().unwrap().status.success());
}

#[test]
fn incomplete_index_meta_is_rebuilt_by_reindex() {
    let e = Env::new();
    e.cmd().arg("reindex").output().unwrap();
    edit_meta(&e, "DELETE FROM meta WHERE key = 'model_id'");
    let out = e.cmd().arg("reindex").output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert!(String::from_utf8_lossy(&out.stdout).contains("added 2, updated 0, removed 0, unchanged 0"));
}

#[test]
fn removed_flags_and_commands_are_usage_errors() {
    let e = Env::new();
    for args in [vec!["search", "x", "--where-json", "{}"], vec!["search", "x", "--min-score", "0"], vec!["models", "pull"]] {
        let out = e.cmd().args(&args).output().unwrap();
        assert_eq!(out.status.code(), Some(2), "{args:?}");
    }
}

#[test]
fn read_while_locked_still_checks_the_index_matches_the_config() {
    let e = Env::new();
    e.json(&["search", "sqlite", "--json"]);
    let other = e.dir.path().join("vault2");
    fs::create_dir_all(&other).unwrap();
    e.write_config(&other);
    let _lock = hold_lock(&e);
    let out = e.cmd().args(["search", "sqlite"]).output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("vlt reindex --force"));
}

#[test]
fn search_human_output_exact_format() {
    let e = Env::new();
    e.min_score("0");
    let out = e.cmd().args(["search", "sqlite water"]).output().unwrap();
    assert!(out.status.success());
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(text.ends_with('\n') && !text.ends_with("\n\n"), "{text:?}");
    let lines: Vec<&str> = text.lines().collect();
    // two results: 6 lines each (header, path, heading, blank, content) + 1 blank separator = 5 + 1 + 5
    assert_eq!(lines.len(), 11, "{text:?}");
    assert_eq!(lines[5], "", "blank separator between results: {text:?}");
    let abs = |rel: &str| e.vault().join(rel).display().to_string();
    let mut seen = vec![];
    for block in [&lines[0..5], &lines[6..11]] {
        let (score, id) = block[0].split_once("  ").unwrap();
        let (int, frac) = score.split_once('.').unwrap();
        assert!(int.chars().all(|c| c.is_ascii_digit()) && frac.len() == 3 && frac.chars().all(|c| c.is_ascii_digit()), "{score:?}");
        let (path, heading, content, range) = match id {
            "projects/db.md#Concurrency" => ("projects/db.md", "Storage > Concurrency", "SQLite WAL mode allows concurrent readers.", "6-7"),
            "notes.md#Pasta" => ("notes.md", "Pasta", "Boil water. See [[db]].", "4-6"),
            other => panic!("unexpected chunk {other}"),
        };
        assert_eq!(block[1], format!("     {}  lines {range}", abs(path)));
        assert_eq!(block[2], format!("     {heading}"));
        assert_eq!(block[3], "");
        assert_eq!(block[4], format!("    {content}"));
        seen.push(id.to_string());
    }
    seen.sort();
    assert_eq!(seen, ["notes.md#Pasta", "projects/db.md#Concurrency"]);
}

#[test]
fn search_human_output_without_results() {
    let e = Env::new();
    e.min_score("0.5");
    let out = e.cmd().args(["search", "sqlite", "--where", "tags=nothing"]).output().unwrap();
    assert!(out.status.success());
    assert_eq!(String::from_utf8_lossy(&out.stdout), "No results above min_score 0.5.\n");
}

#[test]
fn get_human_output_exact_format() {
    let e = Env::new();
    let out = e.cmd().args(["get", "projects/db.md"]).output().unwrap();
    assert!(out.status.success());
    let expected = format!(
        "Storage\n{}\ntags: [\"rust\",\"db\"]\n\n# Storage\n\n## Concurrency\nSQLite WAL mode allows concurrent readers.\n",
        e.vault().join("projects/db.md").display()
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout), expected);
}

#[test]
fn properties_human_output_exact_format() {
    let e = Env::new();
    let out = e.cmd().arg("properties").output().unwrap();
    assert!(out.status.success());
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "KEY   TYPE  DOCS  TOP VALUES\ntags  list  2     cooking (1), db (1), rust (1)\n"
    );
}

#[test]
fn properties_text_output_lists_variants_under_their_key() {
    let e = Env::new();
    fs::write(e.vault().join("extra.md"), "Uses #Rust and #DB.\n").unwrap();
    let out = e.cmd().arg("properties").output().unwrap();
    assert!(out.status.success());
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "KEY   TYPE  DOCS  TOP VALUES\ntags  list  3     DB (1), Rust (1), cooking (1), db (1), rust (1)\n  variants: DB (1) / db (1); Rust (1) / rust (1)\n"
    );
}

#[test]
fn reindex_progress_goes_to_stderr_and_summary_to_stdout() {
    let e = Env::new();
    let out = e.cmd().arg("reindex").output().unwrap();
    assert!(out.status.success());
    assert_eq!(String::from_utf8_lossy(&out.stdout), "added 2, updated 0, removed 0, unchanged 0\n");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.starts_with("Indexing 2 notes ("), "{err:?}");
    assert!(err.contains("Loading the embedding model…\n"), "{err:?}");
    assert!(err.contains("\r\x1b[KIndexing 2/2 notes · ") && err.contains(" KB (100%)"), "{err:?}");
    assert!(err.ends_with('\n'));
    assert!(!String::from_utf8_lossy(&out.stdout).contains("Indexing"));
}

#[test]
fn closed_stdout_pipe_stops_quietly_with_exit_0() {
    let e = Env::new();
    e.min_score("0");
    let mut child = std::process::Command::new(assert_cmd::cargo::cargo_bin("vlt"))
        .args(["search", "sqlite water"])
        .env("VLT_HOME", e.dir.path().join("home"))
        .env("VLT_FAKE_MODELS", "1")
        .env("HOME", e.dir.path())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    drop(child.stdout.take());
    let out = child.wait_with_output().unwrap();
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(!err.contains("panicked"), "{err}");
    assert_eq!(out.status.code(), Some(0), "{err}");
}

#[test]
fn read_while_locked_on_a_db_without_meta_is_an_error_and_writes_nothing() {
    let e = Env::new();
    let db = e.dir.path().join("home/index/vlt.db");
    fs::create_dir_all(db.parent().unwrap()).unwrap();
    rusqlite::Connection::open(&db).unwrap().execute_batch("CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT)").unwrap();
    let _lock = hold_lock(&e);
    let out = e.cmd().args(["search", "sqlite"]).output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("being built by another process"), "{}", String::from_utf8_lossy(&out.stderr));
    let rows: i64 = rusqlite::Connection::open(&db).unwrap().query_row("SELECT count(*) FROM meta", [], |r| r.get(0)).unwrap();
    assert_eq!(rows, 0, "a read under someone else's lock writes no meta");
}

#[test]
fn read_while_locked_on_incomplete_meta_still_requires_a_rebuild() {
    let e = Env::new();
    e.cmd().arg("reindex").output().unwrap();
    edit_meta(&e, "DELETE FROM meta WHERE key = 'model_id'");
    let _lock = hold_lock(&e);
    let out = e.cmd().args(["search", "sqlite"]).output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("vlt reindex --force"), "{}", String::from_utf8_lossy(&out.stderr));
}

/// The description clap prints for `entry` (e.g. `<QUERY>`, `--limit <LIMIT>`): the rest of
/// its line, or the more-indented line below it when the entry column is wide.
fn help_description(help: &str, entry: &str) -> String {
    let lines: Vec<&str> = help.lines().collect();
    let i = lines
        .iter()
        .position(|l| l.trim_start().starts_with(entry))
        .unwrap_or_else(|| panic!("no entry {entry} in help:\n{help}"));
    let same_line = lines[i].trim_start()[entry.len()..].trim();
    if !same_line.is_empty() {
        return same_line.to_string();
    }
    let indent = |l: &str| l.len() - l.trim_start().len();
    match lines.get(i + 1) {
        Some(next) if !next.trim().is_empty() && indent(next) > indent(lines[i]) => next.trim().to_string(),
        _ => String::new(),
    }
}

#[test]
fn every_argument_is_documented_in_help() {
    let cases: [(&str, &[&str]); 4] = [
        ("search", &["<QUERY>", "--where <WHERE>", "--path-prefix <PATH_PREFIX>", "--limit <LIMIT>", "--json"]),
        ("get", &["<PATH>", "--json"]),
        ("properties", &["--path-prefix <PATH_PREFIX>", "--key <KEY>", "--json"]),
        ("reindex", &["--force"]),
    ];
    for (cmd, entries) in cases {
        let out = Command::cargo_bin("vlt").unwrap().args([cmd, "--help"]).output().unwrap();
        assert!(out.status.success());
        let help = String::from_utf8(out.stdout).unwrap();
        for entry in entries {
            assert!(!help_description(&help, entry).is_empty(), "`vlt {cmd} --help` has no description for {entry}\n{help}");
        }
    }
}

#[test]
fn get_help_summary_mentions_content() {
    let out = Command::cargo_bin("vlt").unwrap().args(["--help"]).output().unwrap();
    let help = String::from_utf8(out.stdout).unwrap();
    assert!(help.contains("Show a document with its properties, content, links and backlinks"), "{help}");
}
