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

fn read(dir: &tempfile::TempDir, rel: &str) -> String {
    std::fs::read_to_string(dir.path().join("vault").join(rel)).unwrap()
}

#[test]
fn move_renames_and_rewrites_the_links_it_would_break() {
    let (dir, app) = app_with(&[
        ("p/old.md", b"# Old\n\n[up](../top.md)\n"),
        ("top.md", b"---\r\nup: \"[[old]]\"\r\n---\r\nSee [[old]] and [x](p/old.md).\r\n"),
    ]);
    let r = app.move_note("p/old.md", "q/new.md").unwrap();
    assert!(r.ok && r.reindexed);
    assert_eq!((r.path.as_str(), r.from.as_str()), ("q/new.md", "p/old.md"));
    let paths: Vec<&str> = r.updated.iter().map(|u| u.path.as_str()).collect();
    assert_eq!(paths, vec!["top.md"], "../top.md still resolves from q/");
    assert!(!dir.path().join("vault/p/old.md").exists());
    assert_eq!(read(&dir, "top.md"), "---\r\nup: \"[[new]]\"\r\n---\r\nSee [[new]] and [x](q/new.md).\r\n");
    let doc = app.get_document("q/new.md").unwrap();
    assert_eq!(doc.backlinks.len(), 3);
    assert!(app.get_document("p/old.md").is_err());
}

#[test]
fn move_rewrites_the_moved_notes_own_relative_links() {
    let (dir, app) = app_with(&[("a/x.md", b"[up](../top.md)\n"), ("top.md", b"t\n")]);
    let r = app.move_note("a/x.md", "b/c/x.md").unwrap();
    assert_eq!(r.updated.len(), 1);
    assert_eq!(r.updated[0].path, "b/c/x.md");
    assert_eq!(r.updated[0].diff, "@@ -1,1 +1,1 @@\n-[up](../top.md)\n+[up](../../top.md)");
    assert_eq!(read(&dir, "b/c/x.md"), "[up](../../top.md)\n");
}

#[test]
fn move_allows_a_case_only_rename() {
    let (dir, app) = app_with(&[("note.md", b"x\n"), ("n.md", b"[[note]]\n")]);
    let r = app.move_note("note.md", "Note.md").unwrap();
    assert_eq!(r.path, "Note.md");
    let names: Vec<String> = std::fs::read_dir(dir.path().join("vault")).unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    assert!(names.contains(&"Note.md".to_string()) && !names.contains(&"note.md".to_string()));
    assert!(r.updated.is_empty(), "[[note]] still resolves: names compare case-insensitively");
}

#[test]
fn move_allows_a_case_only_rename_of_a_non_ascii_name() {
    use unicode_normalization::UnicodeNormalization;
    let (dir, app) = app_with(&[("émile.md", b"x\n")]);
    let r = app.move_note("émile.md", "Émile.md").unwrap();
    assert_eq!(r.path, "Émile.md");
    let names: Vec<String> = std::fs::read_dir(dir.path().join("vault")).unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().nfc().collect()).collect();
    assert!(names.contains(&"Émile.md".to_string()) && !names.contains(&"émile.md".to_string()), "{names:?}");
}

#[test]
fn move_rejects_bad_paths_without_touching_anything() {
    let (dir, app) = app_with(&[("a.md", b"a\n"), ("b.md", b"b\n"), ("f", b"file\n")]);
    let code = |p: &str, n: &str| app.move_note(p, n).unwrap_err().code();
    assert_eq!(code("missing.md", "z.md"), "document_not_found");
    assert_eq!(code("a.md", "b.md"), "already_exists");
    assert_eq!(code("a.md", "a.md"), "invalid_arguments");
    assert_eq!(code("a.md", "z.txt"), "invalid_arguments");
    assert_eq!(code("a.md", ".hidden/z.md"), "invalid_arguments");
    assert_eq!(code("a.md", "f/z.md"), "invalid_arguments");
    assert_eq!(code("a.md", "../z.md"), "path_outside_vault");
    std::fs::create_dir_all(dir.path().join("vault/.h")).unwrap();
    std::fs::write(dir.path().join("vault/.h/x.md"), "x\n").unwrap();
    assert_eq!(code(".h/x.md", "z.md"), "invalid_arguments");
    assert_eq!((read(&dir, "a.md"), read(&dir, "b.md")), ("a\n".to_string(), "b\n".to_string()));
}

#[test]
fn move_refuses_names_that_links_cannot_point_to_and_touches_nothing() {
    let (dir, app) = app_with(&[("a.md", b"a\n"), ("n.md", b"[[a]] [l](a.md)\n")]);
    for name in ["C# tips.md", "x|y.md", "a[1].md", "a]1.md", "^x.md", "f#/z.md"] {
        let e = app.move_note("a.md", name).unwrap_err();
        assert_eq!(e.code(), "invalid_arguments", "{name}");
        assert!(e.to_string().contains("Obsidian"), "{name}: {e}");
    }
    assert_eq!(read(&dir, "a.md"), "a\n");
    assert_eq!(read(&dir, "n.md"), "[[a]] [l](a.md)\n");
    assert!(!dir.path().join("vault/f#").exists(), "no folder created");
}

#[test]
fn move_to_a_name_with_spaces_and_parentheses_keeps_the_markdown_link_resolving() {
    let (_dir, app) = app_with(&[("a.md", b"a\n"), ("n.md", b"[l](a.md)\n")]);
    app.move_note("a.md", "b (1).md").unwrap();
    assert_eq!(app.get_document("b (1).md").unwrap().backlinks.len(), 1);
}

#[test]
fn move_does_not_rewrite_a_link_twice_when_one_rewrite_produces_another_links_text() {
    let (dir, app) = app_with(&[("y.md", b"y\n"), ("p/y.md", b"py\n"), ("p/n.md", b"[a](../y.md) [a](y.md)\n")]);
    app.move_note("p/n.md", "n.md").unwrap();
    assert_eq!(read(&dir, "n.md"), "[a](y.md) [a](p/y.md)\n");
}

#[test]
fn move_reports_a_note_it_could_not_write_and_still_rewrites_the_others() {
    use std::os::unix::fs::PermissionsExt;
    let (dir, app) = app_with(&[("old.md", b"x\n"), ("ro/n.md", b"[[old]]\n"), ("z.md", b"[[old]]\n")]);
    let ro = dir.path().join("vault/ro");
    std::fs::set_permissions(&ro, std::fs::Permissions::from_mode(0o555)).unwrap();
    let result = app.move_note("old.md", "new.md");
    std::fs::set_permissions(&ro, std::fs::Permissions::from_mode(0o755)).unwrap();
    let e = result.unwrap_err();
    assert_eq!(e.code(), "file_changed");
    assert!(e.to_string().contains("ro/n.md"), "{e}");
    assert_eq!(read(&dir, "z.md"), "[[new]]\n");
    assert!(dir.path().join("vault/new.md").exists());
}

#[test]
fn move_onto_a_hard_link_of_the_note_is_already_exists() {
    let (dir, app) = app_with(&[("a.md", b"a\n")]);
    std::fs::hard_link(dir.path().join("vault/a.md"), dir.path().join("vault/b.md")).unwrap();
    assert_eq!(app.move_note("a.md", "b.md").unwrap_err().code(), "already_exists");
}

#[test]
fn move_reports_notes_that_changed_since_they_were_indexed() {
    let (dir, app) = app_with(&[("old.md", b"x\n"), ("n.md", b"[[old]]\n")]);
    app.get_document("n.md").unwrap();
    let mut db = vlt::storage::db::Db::open_unchecked(&app.db_path()).unwrap();
    db.conn_mut().execute("UPDATE links SET line = 7 WHERE src_path = 'n.md'", []).unwrap();
    drop(db);
    let e = app.move_note("old.md", "new.md").unwrap_err();
    assert_eq!(e.code(), "file_changed");
    assert!(e.to_string().contains("n.md"));
    assert!(dir.path().join("vault/new.md").exists(), "the move itself stays done");
    assert_eq!(read(&dir, "n.md"), "[[old]]\n");
}

#[test]
fn delete_moves_a_note_to_the_trash_and_reports_its_backlinks() {
    let (dir, app) = app_with(&[("p/a.md", b"a\n"), ("n.md", b"x\n[[a]]\n"), (".trash/p/a.md", b"older\n")]);
    let r = app.delete("p/a.md").unwrap();
    assert!(r.ok && r.reindexed);
    assert_eq!(r.trashed_to.as_deref(), Some(".trash/p/a 1.md"));
    assert_eq!(r.broken_links.len(), 1);
    assert_eq!((r.broken_links[0].path.as_str(), r.broken_links[0].line, r.broken_links[0].raw.as_str()), ("n.md", 2, "[[a]]"));
    assert!(!dir.path().join("vault/p/a.md").exists());
    assert_eq!(read(&dir, ".trash/p/a 1.md"), "a\n");
    assert_eq!(read(&dir, ".trash/p/a.md"), "older\n");
    assert_eq!(app.get_document("p/a.md").unwrap_err().code(), "document_not_found");
}

#[test]
fn delete_removes_a_folder_tree_with_only_ds_store() {
    let (dir, app) = app_with(&[("keep.md", b"k\n"), ("old/.DS_Store", b""), ("old/sub/.DS_Store", b"")]);
    std::fs::create_dir_all(dir.path().join("vault/old/sub/deeper")).unwrap();
    let r = app.delete("old").unwrap();
    assert_eq!((r.trashed_to, r.broken_links.len()), (None, 0));
    assert!(!dir.path().join("vault/old").exists());
}

#[test]
fn delete_refuses_a_folder_with_files_and_bad_paths() {
    let (dir, app) = app_with(&[("f/sub/n.md", b"n\n"), ("f/.DS_Store", b""), ("x.txt", b"x\n"), ("h/.hidden.md", b"h\n")]);
    let e = app.delete("f").unwrap_err();
    assert_eq!(e.code(), "invalid_arguments");
    assert!(e.to_string().contains("f/sub/n.md"));
    assert!(dir.path().join("vault/f/.DS_Store").exists(), "nothing removed");
    let code = |p: &str| app.delete(p).unwrap_err().code();
    assert_eq!(code("missing.md"), "document_not_found");
    assert_eq!(code("x.txt"), "invalid_arguments");
    assert_eq!(code(""), "invalid_arguments");
    assert_eq!(code("."), "invalid_arguments");
    assert_eq!(code("h/.hidden.md"), "invalid_arguments");
    assert_eq!(code(".trash"), "invalid_arguments", "hidden is checked before existence");
    assert_eq!(code("h/.missing.md"), "invalid_arguments");
    assert_eq!(code("../x.md"), "path_outside_vault");
}

#[test]
fn delete_never_moves_a_note_through_a_symlinked_trash() {
    for link in [".trash", ".trash/p"] {
        let (dir, app) = app_with(&[("p/a.md", b"a\n")]);
        let (vault, outside) = (dir.path().join("vault"), dir.path().join("outside"));
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::create_dir_all(vault.join(link).parent().unwrap()).unwrap();
        std::os::unix::fs::symlink(&outside, vault.join(link)).unwrap();
        assert_eq!(app.delete("p/a.md").unwrap_err().code(), "path_outside_vault", "{link}");
        assert_eq!(read(&dir, "p/a.md"), "a\n", "{link}: the note stays");
        assert_eq!(std::fs::read_dir(&outside).unwrap().count(), 0, "{link}: nothing lands outside");
    }
}

#[test]
fn delete_checks_the_exclusion_on_the_notes_real_path() {
    let dir = tempfile::tempdir().unwrap();
    let vault = dir.path().join("vault");
    std::fs::create_dir_all(vault.join("Private")).unwrap();
    std::fs::write(vault.join("Private/x.md"), "x\n").unwrap();
    let home = dir.path().join("home");
    std::fs::create_dir_all(&home).unwrap();
    let mut cfg = Config::parse(&Config::default_toml(&vault)).unwrap();
    cfg.exclude = vec!["Private/**".into()];
    let app = App::new(home, cfg, true).unwrap();
    let err = app.delete("private/x.md").unwrap_err();
    if vault.join("private/x.md").exists() {
        assert_eq!(err.code(), "invalid_arguments", "a case-insensitive filesystem: `private/` is the excluded `Private/`");
    }
    assert!(vault.join("Private/x.md").exists(), "the excluded note stays");
    assert!(!vault.join(".trash").exists());
}
