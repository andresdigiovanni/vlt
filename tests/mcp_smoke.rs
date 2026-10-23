use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

fn rpc(stdin: &mut impl Write, id: u64, method: &str, params: Value) {
    writeln!(stdin, "{}", json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params})).unwrap();
}
fn read_id(out: &mut impl BufRead, id: u64) -> Value {
    loop {
        let mut line = String::new();
        assert!(out.read_line(&mut line).unwrap() > 0, "server closed stdout");
        let v: Value = serde_json::from_str(&line).unwrap();
        if v["id"] == json!(id) {
            return v;
        }
    }
}

#[test]
fn mcp_lists_and_calls_tools() {
    let dir = tempfile::tempdir().unwrap();
    let vault = dir.path().join("vault");
    std::fs::create_dir_all(&vault).unwrap();
    std::fs::write(vault.join("db.md"), "---\ntags: [db]\n---\n# Storage\n\n## Concurrency\nSQLite WAL mode allows concurrent readers.\n").unwrap();
    let home = dir.path().join("home");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::write(home.join("config.toml"), vlt::config::Config::default_toml(&vault)).unwrap();

    let mut child = Command::new(assert_cmd::cargo::cargo_bin("vlt"))
        .arg("mcp")
        .env("VLT_HOME", &home)
        .env("VLT_FAKE_MODELS", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut out = BufReader::new(child.stdout.take().unwrap());

    rpc(&mut stdin, 1, "initialize", json!({"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "t", "version": "0"}}));
    assert_eq!(read_id(&mut out, 1)["result"]["serverInfo"]["name"], "vlt");
    writeln!(stdin, "{}", json!({"jsonrpc": "2.0", "method": "notifications/initialized"})).unwrap();

    rpc(&mut stdin, 2, "tools/list", json!({}));
    let tools = read_id(&mut out, 2)["result"]["tools"].as_array().unwrap().clone();
    let schema = |name: &str| tools.iter().find(|t| t["name"] == name).unwrap()["inputSchema"].clone();
    assert_eq!(schema("search")["required"], json!(["query"]));
    assert!(schema("search")["properties"]["where"].is_object());
    assert_eq!(schema("search")["properties"]["limit"]["minimum"], json!(1), "limit 0 is rejected, so the schema must not advertise it");
    assert_eq!(schema("get")["required"], json!(["path"]));
    assert!(schema("properties")["properties"]["path_prefix"].is_object());
    assert!(schema("properties")["properties"]["key"].is_object());
    let mut names: Vec<String> = tools
        .iter()
        .map(|t| t["name"].as_str().unwrap().to_string())
        .collect();
    names.sort();
    assert_eq!(names, vec!["edit", "get", "properties", "reindex", "search", "update_properties"]);
    assert_eq!(schema("edit")["required"], json!(["path", "old_text", "new_text"]));
    assert_eq!(schema("update_properties")["required"], json!(["path"]));
    assert!(schema("update_properties")["properties"]["unset"].is_object());

    rpc(&mut stdin, 3, "tools/call", json!({"name": "search", "arguments": {"query": "sqlite concurrent readers", "where": "{\"tags\": \"db\"}"}}));
    let r = read_id(&mut out, 3);
    assert_eq!(r["result"]["structuredContent"]["results"][0]["chunk_id"], "db.md#Concurrency");

    rpc(&mut stdin, 4, "tools/call", json!({"name": "get", "arguments": {"path": "missing.md"}}));
    let r = read_id(&mut out, 4);
    assert_eq!(r["result"]["isError"], true);
    assert_eq!(r["result"]["structuredContent"]["error"]["code"], "document_not_found");

    rpc(&mut stdin, 5, "tools/call", json!({"name": "reindex", "arguments": {}}));
    let r = read_id(&mut out, 5);
    assert_eq!(r["result"]["structuredContent"]["reindexed"], true);
    assert_eq!(r["result"]["structuredContent"]["unchanged"], 1);

    let invalid = [
        ("search", json!({"query": "a", "limit": -1})),
        ("search", json!({"query": "a", "limit": 0})),
        ("search", json!({})),
        ("search", json!({"query": "a", "min_score": "x"})),
        ("get", json!({})),
        ("properties", json!({"path_prefix": 5})),
    ];
    for (i, (name, arguments)) in invalid.into_iter().enumerate() {
        let id = 10 + i as u64;
        rpc(&mut stdin, id, "tools/call", json!({"name": name, "arguments": arguments.clone()}));
        let r = read_id(&mut out, id);
        assert_eq!(r["result"]["isError"], true, "{name} {arguments}: {r}");
        assert_eq!(r["result"]["structuredContent"]["error"]["code"], "invalid_arguments", "{name} {arguments}: {r}");
        let text: Value = serde_json::from_str(r["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(text, r["result"]["structuredContent"], "the text block carries the same envelope");
    }

    drop(stdin);
    assert!(child.wait().unwrap().success(), "the server exits 0 when stdin closes");
}

#[test]
fn mcp_reports_missing_config_before_serving() {
    let dir = tempfile::tempdir().unwrap();
    let out = Command::new(assert_cmd::cargo::cargo_bin("vlt"))
        .arg("mcp")
        .env("VLT_HOME", dir.path())
        .env("VLT_FAKE_MODELS", "1")
        .stdin(Stdio::piped())
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(out.stdout.is_empty(), "nothing is written to the transport");
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(stderr.starts_with("error: ") && stderr.contains("vlt is not configured"), "{stderr}");
}

struct Server {
    child: std::process::Child,
    stdin: std::process::ChildStdin,
    out: BufReader<std::process::ChildStdout>,
    next_id: u64,
}

impl Server {
    fn start(home: &std::path::Path) -> Server {
        let mut child = Command::new(assert_cmd::cargo::cargo_bin("vlt"))
            .arg("mcp")
            .env("VLT_HOME", home)
            .env("VLT_FAKE_MODELS", "1")
            .env("HOME", home)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let mut stdin = child.stdin.take().unwrap();
        let mut out = BufReader::new(child.stdout.take().unwrap());
        rpc(&mut stdin, 1, "initialize", json!({"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "t", "version": "0"}}));
        read_id(&mut out, 1);
        writeln!(stdin, "{}", json!({"jsonrpc": "2.0", "method": "notifications/initialized"})).unwrap();
        Server { child, stdin, out, next_id: 2 }
    }

    fn call(&mut self, name: &str, arguments: Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        rpc(&mut self.stdin, id, "tools/call", json!({"name": name, "arguments": arguments}));
        read_id(&mut self.out, id)["result"].clone()
    }

    fn stop(self) {
        let Server { mut child, stdin, .. } = self;
        drop(stdin);
        assert!(child.wait().unwrap().success());
    }
}

fn vault_and_home(dir: &std::path::Path, vault_name: &str) -> (std::path::PathBuf, std::path::PathBuf) {
    let vault = dir.join(vault_name);
    std::fs::create_dir_all(&vault).unwrap();
    std::fs::write(vault.join("db.md"), "---\ntags: [db]\n---\n# Storage\n\nSee [[notes]].\n\n## Concurrency\nSQLite WAL mode allows concurrent readers.\n").unwrap();
    std::fs::write(vault.join("notes.md"), "---\ntags: [cooking]\ndate: 2026-01-02\n---\n# Pasta\n\nBoil water. See [[db]].\n").unwrap();
    let home = dir.join("home");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::write(home.join("config.toml"), vlt::config::Config::default_toml(&vault)).unwrap();
    (vault, home)
}

#[test]
fn mcp_reindex_builds_a_brand_new_index() {
    let dir = tempfile::tempdir().unwrap();
    let (_vault, home) = vault_and_home(dir.path(), "vault");
    let mut s = Server::start(&home);
    let r = s.call("reindex", json!({}));
    assert_ne!(r["isError"], true, "{r}");
    assert_eq!(r["structuredContent"]["reindexed"], true, "{r}");
    assert_eq!(r["structuredContent"]["added"], 2, "{r}");
    s.stop();
}

#[test]
fn mcp_reindex_refuses_to_rebuild_when_the_vault_changed() {
    let dir = tempfile::tempdir().unwrap();
    let (_vault, home) = vault_and_home(dir.path(), "vault");
    let mut s = Server::start(&home);
    assert_eq!(s.call("reindex", json!({}))["structuredContent"]["added"], 2);
    s.stop();

    let other = dir.path().join("other");
    std::fs::create_dir_all(&other).unwrap();
    std::fs::write(other.join("x.md"), "# X\n\nBody.\n").unwrap();
    std::fs::write(home.join("config.toml"), vlt::config::Config::default_toml(&other)).unwrap();
    let mut s = Server::start(&home);
    let r = s.call("reindex", json!({}));
    assert_eq!(r["isError"], true, "{r}");
    assert_eq!(r["structuredContent"]["error"]["code"], "index_rebuild_required", "{r}");
    let message = r["structuredContent"]["error"]["message"].as_str().unwrap();
    assert!(message.contains("vlt reindex --force"), "{message}");
    s.stop();

    let out = Command::new(assert_cmd::cargo::cargo_bin("vlt"))
        .arg("reindex")
        .env("VLT_HOME", &home)
        .env("VLT_FAKE_MODELS", "1")
        .env("HOME", &home)
        .output()
        .unwrap();
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("added 1, updated 0, removed 0, unchanged 0"), "the old index was left for `vlt reindex` to rebuild: {text}");
}

#[test]
fn cli_json_is_the_mcp_text_block_byte_for_byte() {
    let dir = tempfile::tempdir().unwrap();
    let (_vault, home) = vault_and_home(dir.path(), "vault");
    let cli = |args: &[&str]| -> String {
        let out = Command::new(assert_cmd::cargo::cargo_bin("vlt"))
            .args(args)
            .env("VLT_HOME", &home)
            .env("VLT_FAKE_MODELS", "1")
            .env("HOME", &home)
            .output()
            .unwrap();
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
        String::from_utf8(out.stdout).unwrap()
    };
    let cases = [
        (vec!["search", "sqlite concurrent readers", "--json"], "search", json!({"query": "sqlite concurrent readers"})),
        (vec!["get", "db.md", "--json"], "get", json!({"path": "db.md"})),
        (vec!["properties", "--json"], "properties", json!({})),
    ];
    let expected: Vec<String> = cases.iter().map(|(args, _, _)| cli(args)).collect();
    let mut s = Server::start(&home);
    for ((args, tool, arguments), cli_text) in cases.iter().zip(expected) {
        let r = s.call(tool, arguments.clone());
        assert_ne!(r["isError"], true, "{r}");
        let text = r["content"][0]["text"].as_str().unwrap();
        assert_eq!(format!("{text}\n"), cli_text, "{args:?} vs {tool}");
    }
    s.stop();
}

#[test]
fn mcp_enrichment_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    let (vault, home) = vault_and_home(dir.path(), "vault");
    let mut s = Server::start(&home);

    let r = s.call("update_properties", json!({"path": "db.md", "set": {"status": "in_progress"}, "unset": ["tags"]}));
    assert_ne!(r["isError"], true, "{r}");
    assert_eq!(r["structuredContent"]["ok"], true);
    assert_eq!(r["structuredContent"]["reindexed"], true);
    assert_eq!(r["structuredContent"]["diff"], "@@ -2,1 +2,1 @@\n-tags: [db]\n+status: in_progress");

    let stringified = s.call("update_properties", json!({"path": "db.md", "set": "{\"owner\": \"ana\"}"}));
    assert_ne!(stringified["isError"], true, "{stringified}");

    let r = s.call("edit", json!({"path": "notes.md", "old_text": "Boil water.", "new_text": "Boil water. See [[db]] for storage."}));
    assert_ne!(r["isError"], true, "{r}");
    let r = s.call("get", json!({"path": "db.md"}));
    assert_eq!(r["structuredContent"]["properties"], json!({"status": "in_progress", "owner": "ana"}));
    // The fixture's `See [[db]].` was already one backlink; the edit adds a second on the same line.
    let backlinks = r["structuredContent"]["backlinks"].as_array().unwrap();
    assert_eq!(backlinks.len(), 2, "{backlinks:?}");
    assert!(backlinks.iter().all(|b| b["path"] == "notes.md" && b["raw"] == "[[db]]"), "{backlinks:?}");

    let r = s.call("edit", json!({"path": "notes.md", "old_text": "nowhere", "new_text": "x"}));
    assert_eq!(r["isError"], true);
    assert_eq!(r["structuredContent"]["error"]["code"], "not_found");
    let r = s.call("update_properties", json!({"path": "db.md"}));
    assert_eq!(r["structuredContent"]["error"]["code"], "invalid_arguments");
    let r = s.call("update_properties", json!({"path": "db.md", "set": "not json"}));
    assert_eq!(r["structuredContent"]["error"]["code"], "invalid_arguments");
    let r = s.call("edit", json!({"path": "../x.md", "old_text": "a", "new_text": "b"}));
    assert_eq!(r["structuredContent"]["error"]["code"], "path_outside_vault");
    s.stop();

    let notes = std::fs::read_to_string(vault.join("notes.md")).unwrap();
    assert!(notes.contains("Boil water. See [[db]] for storage."));
}
