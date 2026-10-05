use serde_json::json;
use vlt::app::App;
use vlt::config::Config;

/// A vault with the given files, an `App` on fake models, and the temp dir that owns them.
fn app_with(files: &[(&str, &[u8])]) -> (tempfile::TempDir, App) {
    let dir = tempfile::tempdir().unwrap();
    let vault = dir.path().join("vault");
    for (rel, bytes) in files {
        let p = vault.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, bytes).unwrap();
    }
    std::fs::create_dir_all(&vault).unwrap();
    let home = dir.path().join("home");
    std::fs::create_dir_all(&home).unwrap();
    let cfg = Config::parse(&Config::default_toml(&vault)).unwrap();
    (dir, App::new(home, cfg, true).unwrap())
}

#[test]
fn edit_writes_the_file_reindexes_and_returns_the_diff() {
    let (dir, app) = app_with(&[("db.md", b"# Storage\n\nWAL mode.\n")]);
    let r = app.edit("db.md", "WAL mode.", "WAL mode. See [[notes]].").unwrap();
    assert!(r.ok && r.reindexed);
    assert_eq!(r.path, "db.md");
    assert_eq!(r.diff, "@@ -3,1 +3,1 @@\n-WAL mode.\n+WAL mode. See [[notes]].");
    assert_eq!(std::fs::read_to_string(dir.path().join("vault/db.md")).unwrap(), "# Storage\n\nWAL mode. See [[notes]].\n");
    let doc = app.get_document("db.md").unwrap();
    assert!(doc.content.contains("See [[notes]]"), "the index has the new content");
    assert_eq!(doc.links.len(), 1);
}

#[test]
fn edit_keeps_crlf() {
    let (dir, app) = app_with(&[("n.md", b"one\r\ntwo\r\n")]);
    app.edit("n.md", "one\ntwo", "one\nTWO").unwrap();
    assert_eq!(std::fs::read(dir.path().join("vault/n.md")).unwrap(), b"one\r\nTWO\r\n");
}

#[test]
fn edit_rejects_bad_targets_without_touching_anything() {
    let (dir, app) = app_with(&[("n.md", b"hello\n"), ("n.txt", b"hello\n")]);
    std::os::unix::fs::symlink(dir.path().join("vault/n.md"), dir.path().join("vault/link.md")).unwrap();
    let code = |p: &str| app.edit(p, "hello", "bye").unwrap_err().code();
    assert_eq!(code("../x.md"), "path_outside_vault");
    assert_eq!(code("/etc/passwd"), "path_outside_vault");
    assert_eq!(code("link.md"), "path_outside_vault");
    assert_eq!(code("missing.md"), "document_not_found");
    assert_eq!(code("n.txt"), "invalid_arguments");
    assert_eq!(code("."), "invalid_arguments");
    assert_eq!(code("n.md/x.md"), "document_not_found");
    std::fs::create_dir(dir.path().join("vault/dir.md")).unwrap();
    assert_eq!(code("dir.md"), "invalid_arguments");
    assert_eq!(std::fs::read_to_string(dir.path().join("vault/n.md")).unwrap(), "hello\n");
}

#[test]
fn update_properties_writes_reindexes_and_the_filters_see_it() {
    let (dir, app) = app_with(&[("n.md", b"---\r\nstatus: idea\r\n---\r\n# N\r\n\r\nBody.\r\n")]);
    let set = json!({"status": "done", "tags": ["x"]});
    let r = app.update_properties("n.md", set.as_object().unwrap().clone(), vec![]).unwrap();
    assert!(r.ok && r.reindexed);
    assert_eq!(std::fs::read(dir.path().join("vault/n.md")).unwrap(),
               b"---\r\nstatus: done\r\ntags:\r\n  - x\r\n---\r\n# N\r\n\r\nBody.\r\n");
    assert_eq!(r.diff, "@@ -2,1 +2,3 @@\n-status: idea\n+status: done\n+tags:\n+  - x");
    let doc = app.get_document("n.md").unwrap();
    assert_eq!(doc.properties["status"], json!("done"));
    assert_eq!(doc.properties["tags"], json!(["x"]));
}

#[test]
fn update_properties_leaves_the_file_alone_on_error() {
    let (dir, app) = app_with(&[("bad.md", b"---\na: [\n---\nB\n")]);
    let set = json!({"b": 1});
    let err = app.update_properties("bad.md", set.as_object().unwrap().clone(), vec![]).unwrap_err();
    assert_eq!(err.code(), "invalid_frontmatter");
    assert_eq!(std::fs::read_to_string(dir.path().join("vault/bad.md")).unwrap(), "---\na: [\n---\nB\n");
}

#[test]
fn update_properties_unsets_keys() {
    let (dir, app) = app_with(&[("n.md", b"---\nstatus: idea\ntags: [a]\n---\nB\n")]);
    let r = app.update_properties("n.md", serde_json::Map::new(), vec!["tags".into()]).unwrap();
    assert_eq!(r.diff, "@@ -3,1 +3,0 @@\n-tags: [a]");
    assert_eq!(std::fs::read_to_string(dir.path().join("vault/n.md")).unwrap(), "---\nstatus: idea\n---\nB\n");
    assert!(!app.get_document("n.md").unwrap().properties.contains_key("tags"));
}

#[test]
fn update_properties_on_a_bom_file_keeps_the_bom_and_one_frontmatter_block() {
    let (dir, app) = app_with(&[("n.md", "\u{feff}---\ntags: [a]\n---\nbody\n".as_bytes())]);
    let set = json!({"tags": ["a", "b"]});
    app.update_properties("n.md", set.as_object().unwrap().clone(), vec![]).unwrap();
    let text = std::fs::read_to_string(dir.path().join("vault/n.md")).unwrap();
    assert!(text.starts_with("\u{feff}---\n"), "BOM kept at the start: {text:?}");
    assert_eq!(text.matches("---\n").count(), 2, "one frontmatter block: {text:?}");
    assert_eq!(app.get_document("n.md").unwrap().properties["tags"], json!(["a", "b"]));
}

#[test]
fn edit_on_a_bom_file_does_not_match_inside_the_frontmatter() {
    let (_dir, app) = app_with(&[("n.md", "\u{feff}---\nstatus: idea\n---\nbody\n".as_bytes())]);
    assert_eq!(app.edit("n.md", "status: idea", "status: done").unwrap_err().code(), "not_found");
}

#[test]
fn concurrent_edits_on_one_file_all_land() {
    let body: String = (0..8).map(|i| format!("P{i}\n")).collect();
    let (dir, app) = app_with(&[("n.md", body.as_bytes())]);
    std::thread::scope(|s| {
        let handles: Vec<_> = (0..8)
            .map(|i| {
                let app = &app;
                s.spawn(move || app.edit("n.md", &format!("P{i}"), &format!("Q{i}")))
            })
            .collect();
        for h in handles {
            h.join().unwrap().unwrap();
        }
    });
    let text = std::fs::read_to_string(dir.path().join("vault/n.md")).unwrap();
    for i in 0..8 {
        assert!(text.contains(&format!("Q{i}")), "Q{i} missing: {text:?}");
    }
    assert!(!text.contains('P'), "a placeholder survived: {text:?}");
}

#[test]
fn edit_accepts_an_uppercase_md_extension() {
    let (dir, app) = app_with(&[("Notes/Foo.MD", b"hello\n")]);
    app.edit("Notes/Foo.MD", "hello", "bye").unwrap();
    assert_eq!(std::fs::read_to_string(dir.path().join("vault/Notes/Foo.MD")).unwrap(), "bye\n");
}

#[test]
fn edit_keeps_the_endings_of_untouched_lines_in_a_mixed_file() {
    let (dir, app) = app_with(&[("m.md", b"one\r\ntwo\nthree\r\nfour\n")]);
    app.edit("m.md", "three", "THREE").unwrap();
    assert_eq!(std::fs::read(dir.path().join("vault/m.md")).unwrap(), b"one\r\ntwo\nTHREE\r\nfour\n");
}
