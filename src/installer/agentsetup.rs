use std::path::{Path, PathBuf};

use serde_json::{json, Map, Value};

use crate::{Result, VltError};

const SERVER: &str = "vlt";

#[derive(Debug, PartialEq, Eq)]
pub enum SetupAction {
    Added,
    Updated,
    Unchanged,
    Removed,
    NotPresent,
    /// The config cannot be edited safely; carries what the user has to do by hand.
    NeedsManualEdit(String),
}

pub fn claude_json(home: &Path) -> PathBuf {
    home.join(".claude.json")
}

fn claude_entry(exe: &Path) -> Value {
    json!({ "type": "stdio", "command": exe.display().to_string(), "args": ["mcp"] })
}

fn invalid(path: &Path, why: &str) -> VltError {
    VltError::Internal(format!("{} {why}; fix it or add the vlt MCP server by hand", path.display()))
}

/// Reads `path` as a JSON object; `None` when the file does not exist.
fn read_root(path: &Path) -> Result<Option<Map<String, Value>>> {
    if !path.is_file() {
        return Ok(None);
    }
    let raw = std::fs::read_to_string(path)?;
    match serde_json::from_str(&raw) {
        Ok(Value::Object(o)) => Ok(Some(o)),
        Ok(_) => Err(invalid(path, "is not a JSON object")),
        Err(e) => Err(invalid(path, &format!("is not valid JSON ({e})"))),
    }
}

fn servers_of<'a>(path: &Path, root: &'a mut Map<String, Value>, key: &str) -> Result<&'a mut Map<String, Value>> {
    root.entry(key)
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or_else(|| invalid(path, &format!("has a `{key}` that is not an object")))
}

/// Creates `tmp` already restricted to `like`'s permissions (0600 when `like` is absent).
fn create_tmp(tmp: &Path, like: Option<&std::fs::Permissions>) -> std::io::Result<std::fs::File> {
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        opts.mode(like.map_or(0o600, |p| p.mode() & 0o777));
    }
    let file = opts.open(tmp)?;
    if let Some(p) = like {
        // The umask may have narrowed the mode at creation; restore the exact original.
        file.set_permissions(p.clone())?;
    }
    Ok(file)
}

/// Writes `root` to `path` through a per-process temp file + fsync + rename in the same
/// directory, keeping the original permissions. A symlinked `path` is resolved first so the
/// real file is replaced and the link survives.
fn write_atomic(path: &Path, root: Map<String, Value>) -> Result<()> {
    let text = serde_json::to_string_pretty(&Value::Object(root)).map_err(|e| VltError::Internal(e.to_string()))?;
    write_text_atomic(path, &text)
}

fn write_text_atomic(path: &Path, text: &str) -> Result<()> {
    let target = if path.exists() { path.canonicalize()? } else { path.to_path_buf() };
    let like = std::fs::metadata(&target).ok().map(|m| m.permissions());
    let name = target.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let tmp = target.with_file_name(format!(".{name}.vlt-tmp.{}", std::process::id()));
    let _ = std::fs::remove_file(&tmp); // stale leftover of a crashed run with the same pid
    let written = (|| {
        let mut file = create_tmp(&tmp, like.as_ref())?;
        std::io::Write::write_all(&mut file, text.as_bytes())?;
        file.sync_all()?;
        std::fs::rename(&tmp, &target)
    })();
    if written.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    Ok(written?)
}

/// Registers `mcpServers.vlt` in `~/.claude.json`, preserving every other key.
pub fn configure_claude(home: &Path, exe: &Path) -> Result<SetupAction> {
    let path = claude_json(home);
    let mut root = read_root(&path)?.unwrap_or_default();
    let entry = claude_entry(exe);
    let servers = servers_of(&path, &mut root, "mcpServers")?;
    let action = match servers.get(SERVER) {
        Some(found) if *found == entry => return Ok(SetupAction::Unchanged),
        Some(_) => SetupAction::Updated,
        None => SetupAction::Added,
    };
    servers.insert(SERVER.into(), entry);
    write_atomic(&path, root)?;
    Ok(action)
}

/// Removes `mcpServers.vlt` from `~/.claude.json`, preserving every other key.
pub fn deconfigure_claude(home: &Path) -> Result<SetupAction> {
    let path = claude_json(home);
    let Some(mut root) = read_root(&path)? else {
        return Ok(SetupAction::NotPresent);
    };
    let removed = match root.get_mut("mcpServers").and_then(Value::as_object_mut) {
        Some(servers) => servers.remove(SERVER).is_some(),
        None => false,
    };
    if !removed {
        return Ok(SetupAction::NotPresent);
    }
    write_atomic(&path, root)?;
    Ok(SetupAction::Removed)
}

/// The opencode config directory: `$XDG_CONFIG_HOME/opencode`, else `~/.config/opencode`.
pub fn opencode_dir(home: &Path) -> PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| home.join(".config"));
    base.join("opencode")
}

/// The opencode config file: `opencode.jsonc` when it exists, else `opencode.json`.
pub fn opencode_file(dir: &Path) -> PathBuf {
    let jsonc = dir.join("opencode.jsonc");
    if jsonc.is_file() {
        jsonc
    } else {
        dir.join("opencode.json")
    }
}

pub fn codex_file(home: &Path) -> PathBuf {
    home.join(".codex/config.toml")
}

fn opencode_entry(exe: &Path) -> Value {
    json!({ "type": "local", "command": [exe.display().to_string(), "mcp"], "enabled": true })
}

/// Parses an opencode config as plain JSON. A file that is not (comments, trailing commas, a
/// non-object root) cannot be rewritten without losing the user's text, so it is left to them.
fn parse_opencode(raw: &str) -> Option<Map<String, Value>> {
    match serde_json::from_str(raw) {
        Ok(Value::Object(o)) => Some(o),
        _ => None,
    }
}

const NOT_PLAIN_JSON: &str = "it is not plain JSON (comments, trailing commas?) and vlt does not rewrite it";

/// Registers `mcp.vlt` in the opencode config, preserving every other key.
pub fn configure_opencode(dir: &Path, exe: &Path) -> Result<SetupAction> {
    let path = opencode_file(dir);
    let entry = opencode_entry(exe);
    let mut root = if path.is_file() {
        match parse_opencode(&std::fs::read_to_string(&path)?) {
            Some(root) => root,
            None => {
                let block = serde_json::to_string_pretty(&entry).map_err(|e| VltError::Internal(e.to_string()))?;
                return Ok(SetupAction::NeedsManualEdit(format!("{NOT_PLAIN_JSON}; add this under \"mcp\" by hand:\n\"{SERVER}\": {block}")));
            }
        }
    } else {
        Map::new()
    };
    let servers = servers_of(&path, &mut root, "mcp")?;
    let action = match servers.get(SERVER) {
        Some(found) if *found == entry => return Ok(SetupAction::Unchanged),
        Some(_) => SetupAction::Updated,
        None => SetupAction::Added,
    };
    servers.insert(SERVER.into(), entry);
    std::fs::create_dir_all(dir)?;
    write_atomic(&path, root)?;
    Ok(action)
}

/// Removes `mcp.vlt` from the opencode config, preserving every other key.
pub fn deconfigure_opencode(dir: &Path) -> Result<SetupAction> {
    let path = opencode_file(dir);
    if !path.is_file() {
        return Ok(SetupAction::NotPresent);
    }
    let Some(mut root) = parse_opencode(&std::fs::read_to_string(&path)?) else {
        return Ok(SetupAction::NeedsManualEdit(format!("{NOT_PLAIN_JSON}; remove the \"{SERVER}\" entry under \"mcp\" by hand")));
    };
    let removed = match root.get_mut("mcp").and_then(Value::as_object_mut) {
        Some(servers) => servers.remove(SERVER).is_some(),
        None => false,
    };
    if !removed {
        return Ok(SetupAction::NotPresent);
    }
    write_atomic(&path, root)?;
    Ok(SetupAction::Removed)
}

fn read_codex(path: &Path) -> Result<toml_edit::DocumentMut> {
    let raw = if path.is_file() { std::fs::read_to_string(path)? } else { String::new() };
    raw.parse().map_err(|e| invalid(path, &format!("is not valid TOML ({e})")))
}

/// Registers `[mcp_servers.vlt]` in `~/.codex/config.toml`, preserving comments and every other key.
pub fn configure_codex(home: &Path, exe: &Path) -> Result<SetupAction> {
    let path = codex_file(home);
    let mut doc = read_codex(&path)?;
    if doc.get("mcp_servers").is_none() {
        let mut table = toml_edit::Table::new();
        table.set_implicit(true);
        doc.insert("mcp_servers", toml_edit::Item::Table(table));
    }
    let servers = doc["mcp_servers"]
        .as_table_like_mut()
        .ok_or_else(|| invalid(&path, "has an `mcp_servers` that is not a table"))?;
    let command = exe.display().to_string();
    let action = match servers.get("vlt") {
        None => SetupAction::Added,
        Some(found) => {
            let found = found.as_table_like().ok_or_else(|| invalid(&path, "has an `mcp_servers.vlt` that is not a table"))?;
            let same_command = found.get("command").and_then(|c| c.as_str()) == Some(command.as_str());
            let same_args = found
                .get("args")
                .and_then(|a| a.as_array())
                .is_some_and(|a| a.len() == 1 && a.get(0).and_then(|v| v.as_str()) == Some("mcp"));
            if same_command && same_args {
                return Ok(SetupAction::Unchanged);
            }
            SetupAction::Updated
        }
    };
    if servers.get("vlt").is_none() {
        servers.insert("vlt", toml_edit::Item::Table(toml_edit::Table::new()));
    }
    let entry = servers.get_mut("vlt").and_then(|e| e.as_table_like_mut()).expect("vlt entry is a table");
    entry.insert("command", toml_edit::value(command));
    entry.insert("args", toml_edit::value(toml_edit::Array::from_iter(["mcp"])));
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    write_text_atomic(&path, &doc.to_string())?;
    Ok(action)
}

/// Removes `[mcp_servers.vlt]` from `~/.codex/config.toml`, preserving comments and every other key.
pub fn deconfigure_codex(home: &Path) -> Result<SetupAction> {
    let path = codex_file(home);
    if !path.is_file() {
        return Ok(SetupAction::NotPresent);
    }
    let mut doc = read_codex(&path)?;
    let removed = match doc.get_mut("mcp_servers").and_then(|s| s.as_table_like_mut()) {
        Some(servers) => servers.remove("vlt").is_some(),
        None => false,
    };
    if !removed {
        return Ok(SetupAction::NotPresent);
    }
    write_text_atomic(&path, &doc.to_string())?;
    Ok(SetupAction::Removed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn read(h: &Path) -> serde_json::Value { serde_json::from_str(&std::fs::read_to_string(h.join(".claude.json")).unwrap()).unwrap() }

    #[test]
    fn configure_is_idempotent_and_preserves_other_keys() {
        let d = tempfile::tempdir().unwrap();
        let h = d.path();
        assert_eq!(configure_claude(h, Path::new("/bin/vlt")).unwrap(), SetupAction::Added);
        assert_eq!(read(h)["mcpServers"]["vlt"], json!({"type": "stdio", "command": "/bin/vlt", "args": ["mcp"]}));
        let mut v = read(h);
        v["theme"] = json!("dark");
        v["mcpServers"]["other"] = json!({"command": "x"});
        std::fs::write(h.join(".claude.json"), v.to_string()).unwrap();
        assert_eq!(configure_claude(h, Path::new("/bin/vlt")).unwrap(), SetupAction::Unchanged);
        assert_eq!(configure_claude(h, Path::new("/new/vlt")).unwrap(), SetupAction::Updated);
        let v = read(h);
        assert_eq!((v["theme"].clone(), v["mcpServers"]["other"]["command"].clone()), (json!("dark"), json!("x")));
        assert_eq!(v["mcpServers"]["vlt"]["command"], "/new/vlt");
        assert_eq!(deconfigure_claude(h).unwrap(), SetupAction::Removed);
        assert!(read(h)["mcpServers"].get("vlt").is_none());
        assert_eq!(read(h)["mcpServers"]["other"]["command"], "x");
        assert_eq!(deconfigure_claude(h).unwrap(), SetupAction::NotPresent);
    }

    #[test]
    fn invalid_files_are_an_error_and_left_untouched() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join(".claude.json");
        for original in ["{not json", "[1]", r#"{"mcpServers": 3}"#] {
            std::fs::write(&p, original).unwrap();
            assert!(configure_claude(d.path(), Path::new("/bin/vlt")).is_err(), "{original}");
            assert_eq!(std::fs::read_to_string(&p).unwrap(), original);
        }
        for original in ["{not json", "[1]"] {
            std::fs::write(&p, original).unwrap();
            assert!(deconfigure_claude(d.path()).is_err(), "{original}");
            assert_eq!(std::fs::read_to_string(&p).unwrap(), original);
        }
    }

    #[test]
    fn missing_file_or_entry_is_not_present_and_not_created() {
        let d = tempfile::tempdir().unwrap();
        assert_eq!(deconfigure_claude(d.path()).unwrap(), SetupAction::NotPresent);
        assert!(!d.path().join(".claude.json").exists());
    }

    #[cfg(unix)]
    #[test]
    fn rewrite_keeps_file_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join(".claude.json");
        std::fs::write(&p, "{}").unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o600)).unwrap();
        configure_claude(d.path(), Path::new("/bin/vlt")).unwrap();
        assert_eq!(std::fs::metadata(&p).unwrap().permissions().mode() & 0o777, 0o600);
        deconfigure_claude(d.path()).unwrap();
        assert_eq!(std::fs::metadata(&p).unwrap().permissions().mode() & 0o777, 0o600);
        assert!(!d.path().join(".claude.json.vlt-tmp").exists());
    }

    fn leftovers(dir: &Path) -> Vec<String> {
        std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|n| n.contains("vlt-tmp"))
            .collect()
    }

    #[cfg(unix)]
    #[test]
    fn new_file_is_private_and_no_temp_file_is_left() {
        use std::os::unix::fs::PermissionsExt;
        let d = tempfile::tempdir().unwrap();
        configure_claude(d.path(), Path::new("/bin/vlt")).unwrap();
        let p = d.path().join(".claude.json");
        assert_eq!(std::fs::metadata(&p).unwrap().permissions().mode() & 0o777, 0o600);
        assert!(leftovers(d.path()).is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn rewrite_keeps_a_mode_wider_than_the_umask() {
        use std::os::unix::fs::PermissionsExt;
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join(".claude.json");
        std::fs::write(&p, "{}").unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o666)).unwrap();
        configure_claude(d.path(), Path::new("/bin/vlt")).unwrap();
        assert_eq!(std::fs::metadata(&p).unwrap().permissions().mode() & 0o777, 0o666);
    }

    #[test]
    fn failed_rename_leaves_no_temp_file() {
        let d = tempfile::tempdir().unwrap();
        // A non-empty directory where the file should be: the rename onto it fails.
        std::fs::create_dir_all(d.path().join(".claude.json/x")).unwrap();
        assert!(configure_claude(d.path(), Path::new("/bin/vlt")).is_err());
        assert!(leftovers(d.path()).is_empty(), "{:?}", leftovers(d.path()));
    }

    #[test]
    fn creates_mcp_servers_when_the_key_is_missing() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join(".claude.json"), r#"{"theme":"dark"}"#).unwrap();
        assert_eq!(configure_claude(d.path(), Path::new("/bin/vlt")).unwrap(), SetupAction::Added);
        let v = read(d.path());
        assert_eq!(v["theme"], "dark");
        assert_eq!(v["mcpServers"]["vlt"]["command"], "/bin/vlt");
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_file_is_updated_through_the_link() {
        use std::os::unix::fs::PermissionsExt;
        let d = tempfile::tempdir().unwrap();
        let dots = d.path().join("dotfiles");
        std::fs::create_dir(&dots).unwrap();
        let real = dots.join("claude.json");
        std::fs::write(&real, r#"{"theme":"dark"}"#).unwrap();
        std::fs::set_permissions(&real, std::fs::Permissions::from_mode(0o640)).unwrap();
        let link = d.path().join(".claude.json");
        std::os::unix::fs::symlink(&real, &link).unwrap();

        configure_claude(d.path(), Path::new("/bin/vlt")).unwrap();
        assert!(std::fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
        let v: Value = serde_json::from_str(&std::fs::read_to_string(&real).unwrap()).unwrap();
        assert_eq!((v["theme"].clone(), v["mcpServers"]["vlt"]["command"].clone()), (json!("dark"), json!("/bin/vlt")));
        assert_eq!(std::fs::metadata(&real).unwrap().permissions().mode() & 0o777, 0o640);

        assert_eq!(deconfigure_claude(d.path()).unwrap(), SetupAction::Removed);
        assert!(std::fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
        assert!(read(d.path())["mcpServers"].get("vlt").is_none());
        assert!(leftovers(d.path()).is_empty() && leftovers(&dots).is_empty());
    }

    fn opencode_read(dir: &Path, file: &str) -> Value {
        serde_json::from_str(&std::fs::read_to_string(dir.join(file)).unwrap()).unwrap()
    }

    #[test]
    fn opencode_creates_the_config_and_is_idempotent() {
        let d = tempfile::tempdir().unwrap();
        let dir = d.path().join("opencode");
        assert_eq!(configure_opencode(&dir, Path::new("/bin/vlt")).unwrap(), SetupAction::Added);
        assert_eq!(
            opencode_read(&dir, "opencode.json")["mcp"]["vlt"],
            json!({"type": "local", "command": ["/bin/vlt", "mcp"], "enabled": true})
        );
        assert_eq!(configure_opencode(&dir, Path::new("/bin/vlt")).unwrap(), SetupAction::Unchanged);
        assert_eq!(configure_opencode(&dir, Path::new("/new/vlt")).unwrap(), SetupAction::Updated);
        assert_eq!(opencode_read(&dir, "opencode.json")["mcp"]["vlt"]["command"], json!(["/new/vlt", "mcp"]));
    }

    #[test]
    fn opencode_keeps_other_keys_and_prefers_an_existing_jsonc_file() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("opencode.jsonc"), r#"{"theme":"dark","mcp":{"other":{"type":"local"}}}"#).unwrap();
        assert_eq!(configure_opencode(d.path(), Path::new("/bin/vlt")).unwrap(), SetupAction::Added);
        assert!(!d.path().join("opencode.json").exists());
        let v = opencode_read(d.path(), "opencode.jsonc");
        assert_eq!((v["theme"].clone(), v["mcp"]["other"]["type"].clone()), (json!("dark"), json!("local")));
        assert_eq!(deconfigure_opencode(d.path()).unwrap(), SetupAction::Removed);
        let v = opencode_read(d.path(), "opencode.jsonc");
        assert!(v["mcp"].get("vlt").is_none());
        assert_eq!(v["mcp"]["other"]["type"], "local");
        assert_eq!(deconfigure_opencode(d.path()).unwrap(), SetupAction::NotPresent);
    }

    #[test]
    fn opencode_config_that_is_not_plain_json_is_left_untouched_with_a_manual_hint() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("opencode.jsonc");
        let original = "{\n  // my theme\n  \"theme\": \"dark\"\n}\n";
        std::fs::write(&p, original).unwrap();
        let SetupAction::NeedsManualEdit(hint) = configure_opencode(d.path(), Path::new("/bin/vlt")).unwrap() else {
            panic!("expected a manual edit");
        };
        assert!(hint.contains("\"vlt\"") && hint.contains("/bin/vlt"), "the hint carries the block to add: {hint}");
        assert!(matches!(deconfigure_opencode(d.path()).unwrap(), SetupAction::NeedsManualEdit(_)));
        assert_eq!(std::fs::read_to_string(&p).unwrap(), original);
    }

    #[test]
    fn opencode_deconfigure_without_a_config_creates_nothing() {
        let d = tempfile::tempdir().unwrap();
        let dir = d.path().join("opencode");
        assert_eq!(deconfigure_opencode(&dir).unwrap(), SetupAction::NotPresent);
        assert!(!dir.exists());
    }

    fn codex_read(home: &Path) -> toml_edit::DocumentMut {
        std::fs::read_to_string(home.join(".codex/config.toml")).unwrap().parse().unwrap()
    }

    #[test]
    fn codex_creates_the_config_and_is_idempotent() {
        let d = tempfile::tempdir().unwrap();
        assert_eq!(configure_codex(d.path(), Path::new("/bin/vlt")).unwrap(), SetupAction::Added);
        let doc = codex_read(d.path());
        assert_eq!(doc["mcp_servers"]["vlt"]["command"].as_str(), Some("/bin/vlt"));
        assert_eq!(doc["mcp_servers"]["vlt"]["args"].as_array().unwrap().iter().collect::<Vec<_>>().len(), 1);
        assert_eq!(doc["mcp_servers"]["vlt"]["args"][0].as_str(), Some("mcp"));
        assert_eq!(configure_codex(d.path(), Path::new("/bin/vlt")).unwrap(), SetupAction::Unchanged);
        assert_eq!(configure_codex(d.path(), Path::new("/new/vlt")).unwrap(), SetupAction::Updated);
        assert_eq!(codex_read(d.path())["mcp_servers"]["vlt"]["command"].as_str(), Some("/new/vlt"));
    }

    #[test]
    fn codex_keeps_comments_and_other_servers() {
        let d = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(d.path().join(".codex")).unwrap();
        let p = d.path().join(".codex/config.toml");
        std::fs::write(&p, "# my codex\nmodel = \"o3\"\n\n[mcp_servers.other]\ncommand = \"x\" # keep\n").unwrap();
        assert_eq!(configure_codex(d.path(), Path::new("/bin/vlt")).unwrap(), SetupAction::Added);
        let text = std::fs::read_to_string(&p).unwrap();
        assert!(text.contains("# my codex") && text.contains("command = \"x\" # keep") && text.contains("model = \"o3\""), "{text}");
        assert_eq!(deconfigure_codex(d.path()).unwrap(), SetupAction::Removed);
        let text = std::fs::read_to_string(&p).unwrap();
        assert!(!text.contains("vlt") && text.contains("[mcp_servers.other]") && text.contains("# keep"), "{text}");
        assert_eq!(deconfigure_codex(d.path()).unwrap(), SetupAction::NotPresent);
    }

    #[test]
    fn codex_invalid_toml_is_an_error_and_left_untouched() {
        let d = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(d.path().join(".codex")).unwrap();
        let p = d.path().join(".codex/config.toml");
        for original in ["not = = toml", "mcp_servers = 3\n"] {
            std::fs::write(&p, original).unwrap();
            assert!(configure_codex(d.path(), Path::new("/bin/vlt")).is_err(), "{original}");
            assert_eq!(std::fs::read_to_string(&p).unwrap(), original);
        }
        std::fs::write(&p, "not = = toml").unwrap();
        assert!(deconfigure_codex(d.path()).is_err());
    }

    #[test]
    fn codex_deconfigure_without_a_config_creates_nothing() {
        let d = tempfile::tempdir().unwrap();
        assert_eq!(deconfigure_codex(d.path()).unwrap(), SetupAction::NotPresent);
        assert!(!d.path().join(".codex").exists());
    }
}
