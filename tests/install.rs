// tests/install.rs
use assert_cmd::Command;
use std::fs;

fn cmd(home: &std::path::Path) -> Command {
    let mut c = Command::cargo_bin("vlt").unwrap();
    c.env("HOME", home).env("VLT_HOME", home.join(".vlt")).env("VLT_FAKE_MODELS", "1").env_remove("XDG_CONFIG_HOME");
    c
}

#[test]
fn install_is_idempotent_and_uninstall_keeps_data() {
    let d = tempfile::tempdir().unwrap();
    let home = d.path();
    let vault = home.join("my vault");
    for _ in 0..2 {
        let out = cmd(home).args(["install", "--vault"]).arg(&vault).args(["--target", "claude", "--yes"]).output().unwrap();
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    }
    assert!(vault.is_dir(), "missing vault is created with --yes");
    let claude: serde_json::Value = serde_json::from_str(&fs::read_to_string(home.join(".claude.json")).unwrap()).unwrap();
    assert_eq!(claude["mcpServers"]["vlt"]["args"], serde_json::json!(["mcp"]));
    assert!(home.join(".claude/skills/vlt/SKILL.md").is_file());
    let cfg = vlt::config::Config::load(&home.join(".vlt/config.toml")).unwrap();
    assert_eq!(cfg.vault, vault);
    assert!(home.join(".vlt/index/vlt.db").is_file());

    let out = cmd(home).args(["uninstall", "--yes"]).output().unwrap();
    assert!(out.status.success());
    let claude: serde_json::Value = serde_json::from_str(&fs::read_to_string(home.join(".claude.json")).unwrap()).unwrap();
    assert!(claude["mcpServers"].get("vlt").is_none());
    assert!(!home.join(".claude/skills/vlt").exists());
    assert!(home.join(".vlt/config.toml").is_file(), "--yes keeps vlt home");
    assert!(vault.is_dir(), "vault untouched");
}

#[test]
fn yes_without_target_is_an_error() {
    let d = tempfile::tempdir().unwrap();
    let out = cmd(d.path()).args(["install", "--vault"]).arg(d.path().join("v")).arg("--yes").output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("--target"));
    assert!(!d.path().join("v").exists(), "no vault created before targets are known");
    assert!(!d.path().join(".vlt").exists(), "no config written");
    assert!(!d.path().join(".claude.json").exists());
}

#[test]
fn unsupported_target_is_rejected_by_clap() {
    let d = tempfile::tempdir().unwrap();
    let out = cmd(d.path()).args(["install", "--target", "cursor", "--yes"]).output().unwrap();
    assert_eq!(out.status.code(), Some(2));
}

fn install(home: &std::path::Path, args: &[&str]) -> std::process::Output {
    cmd(home).arg("install").args(args).output().unwrap()
}

#[test]
fn changing_the_vault_rewrites_only_the_vault_key() {
    let d = tempfile::tempdir().unwrap();
    let home = d.path();
    let (a, b) = (home.join("a"), home.join("b"));
    assert!(install(home, &["--vault", a.to_str().unwrap(), "--all", "--yes"]).status.success());
    let cfg_path = home.join(".vlt/config.toml");
    let edited = fs::read_to_string(&cfg_path).unwrap().replace("exclude = []", "exclude = [\"t/**\"] # mine");
    fs::write(&cfg_path, &edited).unwrap();

    let out = install(home, &["--all", "--yes"]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(fs::read_to_string(&cfg_path).unwrap(), edited, "--yes without --vault reuses the configured vault");

    let out = install(home, &["--vault", b.to_str().unwrap(), "--all", "--yes"]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let text = fs::read_to_string(&cfg_path).unwrap();
    assert!(text.contains("exclude = [\"t/**\"] # mine"), "{text}");
    assert_eq!(vlt::config::Config::load(&cfg_path).unwrap().vault, b);
    // fails until the index is rebuilt for the new vault
    fs::write(b.join("zebra.md"), "# Zebra\n\nStripes and savanna.\n").unwrap();
    fs::write(&cfg_path, fs::read_to_string(&cfg_path).unwrap().replace("min_score = 0.02 ", "min_score = 0 ")).unwrap();
    let out = cmd(home).args(["search", "zebra stripes", "--json"]).output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let paths: Vec<&str> = v["results"].as_array().unwrap().iter().map(|r| r["document"]["path"].as_str().unwrap()).collect();
    assert!(paths.contains(&"zebra.md"), "{paths:?}");
}

#[test]
fn yes_without_vault_and_without_config_is_an_error() {
    let d = tempfile::tempdir().unwrap();
    let out = install(d.path(), &["--all", "--yes"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("--vault"));
    assert!(!d.path().join(".claude.json").exists());
}

#[test]
fn vault_that_is_a_file_is_an_error() {
    let d = tempfile::tempdir().unwrap();
    let f = d.path().join("f");
    fs::write(&f, "x").unwrap();
    let out = install(d.path(), &["--vault", f.to_str().unwrap(), "--all", "--yes"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("not a directory"));
    assert!(!d.path().join(".claude.json").exists());
}

#[test]
fn tilde_vault_is_expanded_against_home() {
    let d = tempfile::tempdir().unwrap();
    let out = install(d.path(), &["--vault", "~/notes", "--all", "--yes"]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert!(d.path().join("notes").is_dir());
    let cfg = vlt::config::Config::load(&d.path().join(".vlt/config.toml")).unwrap();
    assert_eq!(cfg.vault, d.path().join("notes"));
}

#[test]
fn broken_config_fails_before_touching_anything() {
    let d = tempfile::tempdir().unwrap();
    fs::create_dir_all(d.path().join(".vlt")).unwrap();
    fs::write(d.path().join(".vlt/config.toml"), "vault = \"/v\"\nbogus = 1\n").unwrap();
    let out = install(d.path(), &["--vault", d.path().join("v").to_str().unwrap(), "--all", "--yes"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(!d.path().join(".claude.json").exists());
    assert!(!d.path().join("v").exists());
}

#[test]
fn uninstall_keeps_other_mcp_servers() {
    let d = tempfile::tempdir().unwrap();
    let home = d.path();
    fs::write(home.join(".claude.json"), r#"{"theme":"dark","mcpServers":{"other":{"command":"x"}}}"#).unwrap();
    assert!(install(home, &["--vault", home.join("v").to_str().unwrap(), "--target", "claude", "--target", "claude", "--yes"]).status.success());
    let out = cmd(home).args(["uninstall", "--yes"]).output().unwrap();
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_str(&fs::read_to_string(home.join(".claude.json")).unwrap()).unwrap();
    assert_eq!(v, serde_json::json!({"theme":"dark","mcpServers":{"other":{"command":"x"}}}));
}

#[test]
fn malformed_model_id_in_config_fails_before_creating_the_vault() {
    let d = tempfile::tempdir().unwrap();
    let home = d.path();
    fs::create_dir_all(home.join(".vlt")).unwrap();
    let text = vlt::config::Config::default_toml(&home.join("old")).replace("Qwen/Qwen3-Embedding-0.6B", "nope");
    fs::write(home.join(".vlt/config.toml"), text).unwrap();
    let vault = home.join("new vault");
    let out = install(home, &["--vault", vault.to_str().unwrap(), "--yes", "--target", "claude"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("invalid model id `nope`"), "{}", String::from_utf8_lossy(&out.stderr));
    assert!(!vault.exists(), "no vault created");
    assert!(!home.join(".claude.json").exists(), "no agent configured");
}

#[test]
fn install_while_locked_says_the_index_may_be_stale() {
    let d = tempfile::tempdir().unwrap();
    let home = d.path();
    let _lock = vlt::indexer::scanner::IndexLock::try_acquire(&home.join(".vlt/index")).unwrap().unwrap();
    let out = install(home, &["--vault", home.join("v").to_str().unwrap(), "--yes", "--target", "claude"]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("Another process is reindexing; skipped.\n"), "{text}");
    assert!(text.contains("may be stale") && text.contains("vlt reindex"), "{text}");
}

#[test]
fn uninstall_yes_keeps_vlt_home_that_contains_the_vault() {
    let d = tempfile::tempdir().unwrap();
    let home = d.path();
    let vault = home.join(".vlt/vault");
    assert!(install(home, &["--vault", vault.to_str().unwrap(), "--yes", "--target", "claude"]).status.success());
    let out = cmd(home).args(["uninstall", "--yes"]).output().unwrap();
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("because it contains the vault"), "{text}");
    assert!(vault.is_dir());
}

#[test]
fn uninstall_yes_keeps_vlt_home_when_the_config_cannot_be_read() {
    let d = tempfile::tempdir().unwrap();
    let home = d.path();
    fs::create_dir_all(home.join(".vlt")).unwrap();
    fs::write(home.join(".vlt/config.toml"), "vault = \"/v\"\nbogus = 1\n").unwrap();
    let out = cmd(home).args(["uninstall", "--yes"]).output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("because config.toml could not be read, so the vault location is unknown"), "{text}");
    assert_eq!(text.matches("Kept ").count(), 1, "the decision ends the uninstall before the delete step: {text}");
    assert!(home.join(".vlt/config.toml").is_file());
}

fn with_stdout_closed(home: &std::path::Path, args: &[&str]) -> std::process::Output {
    let mut child = std::process::Command::new(assert_cmd::cargo::cargo_bin("vlt"))
        .args(args)
        .env("HOME", home)
        .env("VLT_HOME", home.join(".vlt"))
        .env("VLT_FAKE_MODELS", "1")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    drop(child.stdout.take());
    child.wait_with_output().unwrap()
}

#[test]
fn install_and_uninstall_finish_quietly_with_exit_0_when_stdout_is_closed() {
    let d = tempfile::tempdir().unwrap();
    let home = d.path();
    let vault = home.join("vault");
    let out = with_stdout_closed(home, &["install", "--vault", vault.to_str().unwrap(), "--target", "claude", "--yes"]);
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(!err.contains("panicked"), "{err}");
    assert_eq!(out.status.code(), Some(0), "{err}");
    assert!(home.join(".vlt/index/vlt.db").is_file(), "the install still ran to the end");

    let out = with_stdout_closed(home, &["uninstall", "--yes"]);
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(!err.contains("panicked"), "{err}");
    assert_eq!(out.status.code(), Some(0), "{err}");
    assert!(!home.join(".claude/skills/vlt").exists(), "the uninstall still ran to the end");
}

fn json_of(path: &std::path::Path) -> serde_json::Value {
    serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
}

#[test]
fn all_installs_and_uninstalls_claude_opencode_and_codex() {
    let d = tempfile::tempdir().unwrap();
    let home = d.path();
    let out = install(home, &["--vault", home.join("v").to_str().unwrap(), "--all", "--yes"]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(json_of(&home.join(".claude.json"))["mcpServers"]["vlt"]["args"], serde_json::json!(["mcp"]));
    let opencode = json_of(&home.join(".config/opencode/opencode.json"));
    assert_eq!(opencode["mcp"]["vlt"]["type"], "local");
    assert_eq!(opencode["mcp"]["vlt"]["command"][1], "mcp");
    let codex = fs::read_to_string(home.join(".codex/config.toml")).unwrap();
    assert!(codex.contains("[mcp_servers.vlt]") && codex.contains("args = [\"mcp\"]"), "{codex}");
    for skill in [".claude/skills/vlt", ".config/opencode/skills/vlt", ".codex/skills/vlt"] {
        assert!(home.join(skill).join("SKILL.md").is_file(), "{skill}");
    }

    let out = cmd(home).args(["uninstall", "--yes"]).output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert!(json_of(&home.join(".config/opencode/opencode.json"))["mcp"].get("vlt").is_none());
    assert!(!fs::read_to_string(home.join(".codex/config.toml")).unwrap().contains("vlt"));
    for skill in [".claude/skills/vlt", ".config/opencode/skills/vlt", ".codex/skills/vlt"] {
        assert!(!home.join(skill).exists(), "{skill}");
    }
}

#[test]
fn a_single_target_leaves_the_other_agents_alone() {
    let d = tempfile::tempdir().unwrap();
    let home = d.path();
    let out = install(home, &["--vault", home.join("v").to_str().unwrap(), "--target", "codex", "--yes"]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert!(home.join(".codex/config.toml").is_file());
    assert!(!home.join(".claude.json").exists() && !home.join(".config/opencode").exists());
}

#[test]
fn opencode_follows_xdg_config_home() {
    let d = tempfile::tempdir().unwrap();
    let home = d.path();
    let xdg = home.join("xdg");
    let out = cmd(home)
        .env("XDG_CONFIG_HOME", &xdg)
        .args(["install", "--vault"]).arg(home.join("v")).args(["--target", "opencode", "--yes"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert!(xdg.join("opencode/opencode.json").is_file() && xdg.join("opencode/skills/vlt/SKILL.md").is_file());
    assert!(!home.join(".config").exists());
}

#[test]
fn opencode_config_with_comments_is_left_alone_and_the_install_goes_on() {
    let d = tempfile::tempdir().unwrap();
    let home = d.path();
    let dir = home.join(".config/opencode");
    fs::create_dir_all(&dir).unwrap();
    let original = "{\n  // mine\n  \"theme\": \"dark\"\n}\n";
    fs::write(dir.join("opencode.jsonc"), original).unwrap();
    let out = install(home, &["--vault", home.join("v").to_str().unwrap(), "--all", "--yes"]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("by hand") && text.contains("\"vlt\""), "{text}");
    assert_eq!(fs::read_to_string(dir.join("opencode.jsonc")).unwrap(), original);
    assert!(dir.join("skills/vlt/SKILL.md").is_file(), "the Skill is still installed");
    assert!(home.join(".codex/config.toml").is_file() && home.join(".claude.json").is_file(), "the other agents are configured");
    assert!(home.join(".vlt/index/vlt.db").is_file(), "the install reached the index");

    let out = cmd(home).args(["uninstall", "--yes"]).output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert!(String::from_utf8_lossy(&out.stdout).contains("by hand"));
    assert_eq!(fs::read_to_string(dir.join("opencode.jsonc")).unwrap(), original);
}

#[test]
fn install_announces_the_index_and_the_model_load_before_the_progress_line() {
    let d = tempfile::tempdir().unwrap();
    let home = d.path();
    let vault = home.join("v");
    fs::create_dir_all(&vault).unwrap();
    fs::write(vault.join("a.md"), "# A\n\nAlpha.\n").unwrap();
    let out = install(home, &["--vault", vault.to_str().unwrap(), "--yes", "--target", "claude"]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let err = String::from_utf8_lossy(&out.stderr);
    let start = err.find("Indexing 1 note (1 KB) on the CPU").expect(&err);
    let load = err.find("Loading the embedding model…\n").expect(&err);
    let line = err.find("\r\x1b[KIndexing 1/1 note · 1/1 KB (100%)").expect(&err);
    assert!(start < load && load < line, "{err:?}");
    assert!(String::from_utf8_lossy(&out.stdout).contains("✓ Vault indexed: added 1"));
}
