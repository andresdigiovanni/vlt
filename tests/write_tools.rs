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

#[test]
fn create_writes_a_new_note_with_its_folders_and_reindexes() {
    let (dir, app) = app_with(&[("n.md", b"x\n")]);
    let content = "---\ntags: [new]\n---\n# Fresh\n\nA new note.\n";
    let r = app.create("ideas/2026/fresh.md", content).unwrap();
    assert!(r.ok && r.reindexed);
    assert_eq!(r.path, "ideas/2026/fresh.md");
    assert_eq!(r.diff, "@@ -1,0 +1,6 @@\n+---\n+tags: [new]\n+---\n+# Fresh\n+\n+A new note.");
    assert_eq!(std::fs::read_to_string(dir.path().join("vault/ideas/2026/fresh.md")).unwrap(), content);
    let doc = app.get_document("ideas/2026/fresh.md").unwrap();
    assert_eq!((doc.title.as_str(), doc.tags.clone()), ("Fresh", vec!["new".to_string()]), "the index has the note");
    let left: Vec<_> = std::fs::read_dir(dir.path().join("vault/ideas/2026")).unwrap().map(|e| e.unwrap().file_name()).collect();
    assert_eq!(left, vec!["fresh.md"], "no temp file remains");
}

#[test]
fn create_never_replaces_a_file_or_a_folder() {
    let (dir, app) = app_with(&[("n.md", b"old\n")]);
    std::fs::create_dir(dir.path().join("vault/dir.md")).unwrap();
    assert_eq!(app.create("n.md", "new\n").unwrap_err().code(), "already_exists");
    assert_eq!(app.create("dir.md", "new\n").unwrap_err().code(), "already_exists");
    assert_eq!(std::fs::read_to_string(dir.path().join("vault/n.md")).unwrap(), "old\n");
}

#[test]
fn create_rejects_bad_paths_and_bad_frontmatter_without_writing() {
    let dir = tempfile::tempdir().unwrap();
    let vault = dir.path().join("vault");
    std::fs::create_dir_all(vault.join("real")).unwrap();
    std::os::unix::fs::symlink(vault.join("real"), vault.join("link")).unwrap();
    let home = dir.path().join("home");
    std::fs::create_dir_all(&home).unwrap();
    let mut cfg = Config::parse(&Config::default_toml(&vault)).unwrap();
    cfg.exclude = vec!["templates/**".into()];
    let app = App::new(home, cfg, true).unwrap();
    let code = |p: &str, c: &str| app.create(p, c).unwrap_err().code();
    assert_eq!(code("../x.md", "x\n"), "path_outside_vault");
    assert_eq!(code("link/x.md", "x\n"), "path_outside_vault");
    assert_eq!(code("x.txt", "x\n"), "invalid_arguments");
    assert_eq!(code("templates/t.md", "x\n"), "invalid_arguments", "vlt could not read an excluded note back");
    assert_eq!(code("bad.md", "---\n- not a mapping\n---\nx\n"), "invalid_frontmatter");
    for p in ["templates", "bad.md", "x.txt", "real/x.md"] {
        assert!(!vault.join(p).exists(), "{p} was written");
    }
}

#[test]
fn create_ignores_a_planted_temp_name_and_never_writes_through_a_symlink() {
    let (dir, app) = app_with(&[("victim.md", b"keep\n")]);
    let v = dir.path().join("vault");
    std::os::unix::fs::symlink(v.join("victim.md"), v.join(".new.md.vlt-tmp")).unwrap();
    std::os::unix::fs::symlink(v.join("victim.md"), v.join("alias.md")).unwrap();
    std::os::unix::fs::symlink(dir.path().join("outside.md"), v.join("dangling.md")).unwrap();
    app.create("new.md", "fresh\n").unwrap();
    assert_eq!(std::fs::read_to_string(v.join("new.md")).unwrap(), "fresh\n");
    assert!(!std::fs::symlink_metadata(v.join("new.md")).unwrap().file_type().is_symlink());
    assert!(app.create("alias.md", "x\n").is_err());
    assert!(app.create("dangling.md", "x\n").is_err());
    assert_eq!(std::fs::read_to_string(v.join("victim.md")).unwrap(), "keep\n");
    assert!(!dir.path().join("outside.md").exists());
}

#[test]
fn concurrent_creates_of_one_path_keep_the_winners_content() {
    for round in 0..20 {
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path().join("vault");
        std::fs::create_dir_all(&vault).unwrap();
        let app = |home: &str| {
            let home = dir.path().join(home);
            std::fs::create_dir_all(&home).unwrap();
            App::new(home, Config::parse(&Config::default_toml(&vault)).unwrap(), true).unwrap()
        };
        let (a, b) = (app("h1"), app("h2"));
        let path = format!("r{round}.md");
        let (ra, rb) = std::thread::scope(|s| {
            let ha = s.spawn(|| a.create(&path, "from a\n"));
            let hb = s.spawn(|| b.create(&path, "from b\n"));
            (ha.join().unwrap(), hb.join().unwrap())
        });
        let on_disk = std::fs::read_to_string(vault.join(&path)).unwrap();
        match (ra, rb) {
            (Ok(_), Err(e)) => assert_eq!((on_disk.as_str(), e.code()), ("from a\n", "already_exists")),
            (Err(e), Ok(_)) => assert_eq!((on_disk.as_str(), e.code()), ("from b\n", "already_exists")),
            other => panic!("exactly one create must win: {other:?}"),
        }
    }
}

#[test]
fn create_checks_hidden_and_excluded_paths_on_the_folder_it_really_writes_to() {
    let dir = tempfile::tempdir().unwrap();
    let vault = dir.path().join("vault");
    std::fs::create_dir_all(vault.join("templates")).unwrap();
    std::fs::create_dir_all(vault.join("ideas")).unwrap();
    std::fs::write(vault.join("n.md"), "x\n").unwrap();
    let home = dir.path().join("home");
    std::fs::create_dir_all(&home).unwrap();
    let mut cfg = Config::parse(&Config::default_toml(&vault)).unwrap();
    cfg.exclude = vec!["templates/**".into()];
    let app = App::new(home, cfg, true).unwrap();
    let code = |p: &str| app.create(p, "x\n").unwrap_err().code();
    assert_eq!(code(".obsidian/x.md"), "invalid_arguments", "hidden folders are not indexed");
    assert_eq!(code("sub/.hidden.md"), "invalid_arguments");
    assert_eq!(code("n.md/x.md"), "invalid_arguments", "a folder in the path is a file");
    assert!(!vault.join(".obsidian").exists() && !vault.join("sub").exists());
    if vault.join("TEMPLATES").exists() {
        // A case-insensitive filesystem: `Templates/` is the excluded `templates/`, and `Ideas/` is `ideas/`.
        assert_eq!(code("Templates/t.md"), "invalid_arguments");
        assert!(!vault.join("templates/t.md").exists());
        let r = app.create("Ideas/x.md", "# X\n").unwrap();
        assert_eq!(r.path, "ideas/x.md", "the path as stored, which get finds");
        app.get_document(&r.path).unwrap();
    }
}

#[test]
fn create_checks_the_frontmatter_after_a_bom_and_takes_long_file_names() {
    let (dir, app) = app_with(&[]);
    assert_eq!(app.create("b.md", "\u{feff}---\n- a list\n---\nx\n").unwrap_err().code(), "invalid_frontmatter");
    let long = format!("{}.md", "a".repeat(247));
    app.create(&long, "x\n").unwrap();
    assert!(dir.path().join("vault").join(&long).exists());
}
