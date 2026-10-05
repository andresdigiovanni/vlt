pub mod agentsetup;
pub mod models;
pub mod prompts;
pub mod skill;

use std::io::Write;
use std::path::{Component, Path, PathBuf};

use clap::ValueEnum;

use crate::app::App;
use crate::cli::{is_stdout_closed, write_out};
use crate::config::Config;
use crate::ml::models::Models;
use crate::paths;
use crate::{Result, VltError};
use agentsetup::{
    claude_json, codex_file, configure_claude, configure_codex, configure_opencode, deconfigure_claude, deconfigure_codex,
    deconfigure_opencode, opencode_dir, opencode_file, SetupAction,
};
use models::{missing, pull_showing_progress};
use skill::{install_skill, uninstall_skill};

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Target {
    Claude,
    Opencode,
    Codex,
}

pub struct InstallOptions {
    pub vault: Option<PathBuf>,
    pub targets: Vec<Target>,
    pub all: bool,
    pub yes: bool,
}

/// Expands a leading `~` and makes `p` absolute against the current directory.
fn absolute(p: &Path, home: &Path) -> Result<PathBuf> {
    let mut comps = p.components();
    let expanded = match comps.next() {
        Some(Component::Normal(first)) if first == "~" => home.join(comps.as_path()),
        _ => p.to_path_buf(),
    };
    if expanded.is_absolute() {
        Ok(expanded)
    } else {
        Ok(std::env::current_dir()?.join(expanded))
    }
}

/// Settles the vault path, asking whether to create it when missing. Has no side effects:
/// returns the path and whether it must be created.
fn resolve_vault(opts: &InstallOptions, existing: Option<&Path>, home: &Path) -> Result<(PathBuf, bool)> {
    let raw = match (&opts.vault, opts.yes, existing) {
        (Some(v), _, _) => v.clone(),
        (None, true, Some(e)) => e.to_path_buf(),
        (None, true, None) => return Err(VltError::InvalidArguments("pass --vault <path>".into())),
        (None, false, _) => prompts::ask_vault(existing)?,
    };
    let vault = absolute(&raw, home)?;
    if !vault.exists() {
        let question = format!("The path '{}' does not exist. Create it?", vault.display());
        if !(opts.yes || prompts::confirm(&question, true)?) {
            return Err(VltError::InvalidArguments(format!("the vault path '{}' does not exist", vault.display())));
        }
        return Ok((vault, true));
    }
    if !vault.is_dir() {
        return Err(VltError::InvalidArguments(format!("the vault path '{}' is not a directory", vault.display())));
    }
    Ok((vault, false))
}

fn resolve_targets(opts: &InstallOptions) -> Result<Vec<Target>> {
    let targets = if opts.all {
        Target::value_variants().to_vec()
    } else if !opts.targets.is_empty() {
        opts.targets.clone()
    } else if opts.yes {
        return Err(VltError::InvalidArguments("pass --target or --all".into()));
    } else {
        prompts::select_targets()?
    };
    if targets.is_empty() {
        return Err(VltError::InvalidArguments("no agent selected".into()));
    }
    Ok(targets)
}

/// Prints installer progress; a closed stdout is ignored so the install or uninstall still runs to the end.
fn say(args: std::fmt::Arguments) -> Result<()> {
    match write_out(args) {
        Err(e) if is_stdout_closed(&e) => Ok(()),
        r => r,
    }
}

fn ensure_models(vlt_home: &Path, models: &Models) -> Result<()> {
    if Models::fake_from_env() {
        say(format_args!("✓ Models: fake models in use, nothing to download\n"))?;
        return Ok(());
    }
    let dir = paths::models_dir(vlt_home);
    for spec in models.specs() {
        if !missing(&dir, spec) {
            say(format_args!("✓ Model {} already downloaded\n", spec.id))?;
            continue;
        }
        pull_showing_progress(&dir, spec)?;
        say(format_args!("✓ Model {} downloaded\n", spec.id))?;
    }
    Ok(())
}

fn label(action: &SetupAction) -> &'static str {
    match action {
        SetupAction::Added => "added",
        SetupAction::Updated => "updated",
        SetupAction::Unchanged => "unchanged",
        SetupAction::Removed => "removed",
        SetupAction::NotPresent => "not present",
        SetupAction::NeedsManualEdit(_) => "needs a manual edit",
    }
}

/// The file that holds the MCP server entry of `target`.
fn mcp_config_path(target: Target, home: &Path) -> PathBuf {
    match target {
        Target::Claude => claude_json(home),
        Target::Opencode => opencode_file(&opencode_dir(home)),
        Target::Codex => codex_file(home),
    }
}

/// `path` as shown to the user: `~/…` when it lies under `home`.
fn shown(path: &Path, home: &Path) -> String {
    match path.strip_prefix(home) {
        Ok(rest) => format!("~/{}", rest.display()),
        Err(_) => path.display().to_string(),
    }
}

fn configure_target(target: Target, home: &Path, exe: &Path) -> Result<()> {
    let action = match target {
        Target::Claude => configure_claude(home, exe)?,
        Target::Opencode => configure_opencode(&opencode_dir(home), exe)?,
        Target::Codex => configure_codex(home, exe)?,
    };
    let config = shown(&mcp_config_path(target, home), home);
    match action {
        SetupAction::NeedsManualEdit(hint) => say(format_args!("! MCP server not registered in {config}: {hint}\n"))?,
        action => say(format_args!("✓ MCP server registered in {config} ({})\n", label(&action)))?,
    }
    let path = install_skill(home, target)?;
    say(format_args!("✓ Skill installed at {}\n", path.display()))?;
    Ok(())
}

fn write_config(config_path: &Path, existing: Option<(&str, &Config)>, vault: &Path) -> Result<()> {
    match existing {
        None => {
            if let Some(dir) = config_path.parent() {
                std::fs::create_dir_all(dir)?;
            }
            std::fs::write(config_path, Config::default_toml(vault))?;
            say(format_args!("✓ Config written to {}\n", config_path.display()))?;
        }
        Some((text, cfg)) if cfg.vault != vault => {
            std::fs::write(config_path, Config::set_vault_in(text, vault)?)?;
            say(format_args!("✓ Config updated with the new vault in {}\n", config_path.display()))?;
        }
        Some(_) => say(format_args!("✓ Config kept at {}\n", config_path.display()))?,
    }
    Ok(())
}

fn index() -> Result<()> {
    let app = App::load()?;
    let mut shown = false;
    let out = app.reindex(false, &mut |done, total| {
        if total > 0 {
            eprint!("\rIndexing {done}/{total}");
            let _ = std::io::stderr().flush();
            shown = true;
        }
    })?;
    if shown {
        eprintln!();
    }
    if out.reindexed {
        let s = &out.stats;
        say(format_args!("✓ Vault indexed: added {}, updated {}, removed {}, unchanged {}\n", s.added, s.updated, s.removed, s.unchanged))?;
    } else {
        say(format_args!("Another process is reindexing; skipped.\n"))?;
        say(format_args!("The index may be stale for this vault; run `vlt reindex` once the other process finishes.\n"))?;
    }
    Ok(())
}

/// `vlt install`: vault, agents, models, MCP entry + Skill per agent, config, first index (PRD 2).
pub fn install(opts: InstallOptions) -> Result<()> {
    let home = paths::home_dir()?;
    let vlt_home = paths::vlt_home()?;
    let config_path = paths::config_path(&vlt_home);
    let existing_text = if config_path.is_file() { Some(std::fs::read_to_string(&config_path)?) } else { None };
    let existing_cfg = existing_text.as_deref().map(Config::parse).transpose()?;
    let models_cfg = existing_cfg.as_ref().map(|c| c.models.clone()).unwrap_or_default();
    let models = Models::new(&models_cfg, &paths::models_dir(&vlt_home), Models::fake_from_env())?;

    let (vault, create_vault) = resolve_vault(&opts, existing_cfg.as_ref().map(|c| c.vault.as_path()), &home)?;
    let targets = resolve_targets(&opts)?;
    if create_vault {
        std::fs::create_dir_all(&vault)?;
        say(format_args!("✓ Vault created at {}\n", vault.display()))?;
    }
    ensure_models(&vlt_home, &models)?;

    let exe = std::env::current_exe()?.canonicalize()?;
    for target in targets {
        configure_target(target, &home, &exe)?;
    }
    write_config(&config_path, existing_text.as_deref().zip(existing_cfg.as_ref()), &vault)?;
    index()
}

/// True when `vault` lies inside `vlt_home`, so deleting vlt home would delete the vault.
/// Compares canonical paths, falling back to each path as given when it cannot be resolved,
/// then looks for vlt home itself (same device and inode) among the vault and its existing
/// ancestors, which catches spellings a path comparison misses, such as another case.
fn vault_inside(vlt_home: &Path, vault: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    let canonical = |p: &Path| p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
    if vault.starts_with(vlt_home) || canonical(vault).starts_with(canonical(vlt_home)) {
        return true;
    }
    let Ok(home) = std::fs::metadata(vlt_home) else { return false };
    vault
        .ancestors()
        .filter_map(|a| std::fs::metadata(a).ok())
        .any(|m| m.dev() == home.dev() && m.ino() == home.ino())
}

/// What `uninstall` may do with vlt home.
#[derive(Debug, PartialEq, Eq)]
enum HomeDecision {
    /// The configured vault lies inside vlt home.
    KeepVaultInside(PathBuf),
    /// config.toml exists but cannot be loaded, so the vault location is unknown.
    KeepConfigUnreadable,
    /// No vault known inside vlt home: deleting it is up to the user.
    Ask,
}

fn home_decision(vlt_home: &Path, config_path: &Path) -> HomeDecision {
    match std::fs::symlink_metadata(config_path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return HomeDecision::Ask,
        _ => {}
    }
    match Config::load(config_path) {
        Err(_) => HomeDecision::KeepConfigUnreadable,
        Ok(cfg) if vault_inside(vlt_home, &cfg.vault) => HomeDecision::KeepVaultInside(cfg.vault),
        Ok(_) => HomeDecision::Ask,
    }
}

/// `vlt uninstall`: removes the MCP entries and Skills; deletes vlt home only after an interactive yes.
pub fn uninstall(yes: bool) -> Result<()> {
    let home = paths::home_dir()?;
    let vlt_home = paths::vlt_home()?;
    for &target in Target::value_variants() {
        let action = match target {
            Target::Claude => deconfigure_claude(&home)?,
            Target::Opencode => deconfigure_opencode(&opencode_dir(&home))?,
            Target::Codex => deconfigure_codex(&home)?,
        };
        let config = shown(&mcp_config_path(target, &home), &home);
        match action {
            SetupAction::Removed => say(format_args!("✓ MCP server removed from {config}\n"))?,
            SetupAction::NeedsManualEdit(hint) => say(format_args!("! MCP server not removed from {config}: {hint}\n"))?,
            _ => say(format_args!("MCP server was not registered in {config}\n"))?,
        }
        let path = skill::skill_path(&home, target);
        let dir = path.parent().unwrap_or(&path).display().to_string();
        if uninstall_skill(&home, target)? {
            say(format_args!("✓ Skill removed from {dir}\n"))?;
        } else {
            say(format_args!("Skill was not installed at {dir}\n"))?;
        }
    }
    if !vlt_home.exists() {
        return Ok(());
    }
    match home_decision(&vlt_home, &paths::config_path(&vlt_home)) {
        HomeDecision::KeepVaultInside(vault) => {
            say(format_args!("Kept {} because it contains the vault {}\n", vlt_home.display(), vault.display()))?;
            return Ok(());
        }
        HomeDecision::KeepConfigUnreadable => {
            let home = vlt_home.display();
            say(format_args!("Kept {home} because config.toml could not be read, so the vault location is unknown.\n"))?;
            return Ok(());
        }
        HomeDecision::Ask => {}
    }
    let question = format!("Delete {} (config, index and models)?", vlt_home.display());
    if !yes && prompts::confirm(&question, false)? {
        std::fs::remove_dir_all(&vlt_home)?;
        say(format_args!("✓ Deleted {}\n", vlt_home.display()))?;
    } else {
        say(format_args!("Kept {}\n", vlt_home.display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absolute_expands_tilde_and_relative_paths() {
        let home = Path::new("/h");
        assert_eq!(absolute(Path::new("~"), home).unwrap(), PathBuf::from("/h"));
        assert_eq!(absolute(Path::new("~/my vault"), home).unwrap(), PathBuf::from("/h/my vault"));
        assert_eq!(absolute(Path::new("/abs/v"), home).unwrap(), PathBuf::from("/abs/v"));
        assert_eq!(absolute(Path::new("~x/v"), home).unwrap(), std::env::current_dir().unwrap().join("~x/v"));
        assert_eq!(absolute(Path::new("rel"), home).unwrap(), std::env::current_dir().unwrap().join("rel"));
    }

    #[test]
    fn vault_inside_vlt_home_is_detected() {
        let d = tempfile::tempdir().unwrap();
        let vlt_home = d.path().join(".vlt");
        std::fs::create_dir_all(vlt_home.join("vault")).unwrap();
        std::fs::create_dir_all(d.path().join("elsewhere")).unwrap();
        assert!(vault_inside(&vlt_home, &vlt_home.join("vault")));
        assert!(vault_inside(&vlt_home, &vlt_home), "the vault is vlt home itself");
        assert!(!vault_inside(&vlt_home, &d.path().join("elsewhere")));
        assert!(!vault_inside(&vlt_home, &d.path().join(".vlt-other")), "a sibling sharing the prefix");
    }

    #[test]
    fn vault_inside_vlt_home_follows_symlinks() {
        let d = tempfile::tempdir().unwrap();
        let vlt_home = d.path().join(".vlt");
        std::fs::create_dir_all(vlt_home.join("vault")).unwrap();
        std::os::unix::fs::symlink(vlt_home.join("vault"), d.path().join("link")).unwrap();
        assert!(vault_inside(&vlt_home, &d.path().join("link")), "the vault reached through a symlink");
        std::os::unix::fs::symlink(&vlt_home, d.path().join("home-link")).unwrap();
        assert!(vault_inside(&d.path().join("home-link"), &vlt_home.join("vault")), "vlt home reached through a symlink");
    }

    #[test]
    fn vault_inside_vlt_home_without_canonical_paths_compares_as_given() {
        let d = tempfile::tempdir().unwrap();
        let vlt_home = d.path().join(".vlt");
        std::fs::create_dir_all(&vlt_home).unwrap();
        assert!(vault_inside(&vlt_home, &vlt_home.join("missing")), "a vault that does not exist yet");
        assert!(vault_inside(Path::new("/nowhere/.vlt"), Path::new("/nowhere/.vlt/v")));
        assert!(!vault_inside(Path::new("/nowhere/.vlt"), Path::new("/nowhere/v")));
    }

    #[test]
    fn vault_inside_vlt_home_spelled_differently_is_detected() {
        let d = tempfile::tempdir().unwrap();
        let vlt_home = d.path().join(".vlt");
        std::fs::create_dir_all(vlt_home.join("v")).unwrap();
        if d.path().join(".VLT").exists() {
            eprintln!("variant: case-insensitive filesystem");
            assert!(vault_inside(&vlt_home, &d.path().join(".VLT/v")), "vlt home spelled with another case");
            assert!(vault_inside(&vlt_home, &d.path().join(".VLT/missing")), "a missing vault under vlt home spelled with another case");
        } else {
            eprintln!("variant: case-sensitive filesystem, symlink alias");
            std::os::unix::fs::symlink(&vlt_home, d.path().join("alias")).unwrap();
            assert!(vault_inside(&vlt_home, &d.path().join("alias/missing")), "a missing vault reached through an alias of vlt home");
        }
    }

    fn write_config(d: &Path, text: &str) -> PathBuf {
        let path = d.join(".vlt/config.toml");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, text).unwrap();
        path
    }

    #[test]
    fn home_decision_keeps_vlt_home_unless_the_vault_is_known_to_be_outside() {
        let d = tempfile::tempdir().unwrap();
        let vlt_home = d.path().join(".vlt");
        let missing = vlt_home.join("config.toml");
        assert_eq!(home_decision(&vlt_home, &missing), HomeDecision::Ask, "no config, no vault known");

        let broken = write_config(d.path(), "vault = \"/v\"\nbogus = 1\n");
        assert_eq!(home_decision(&vlt_home, &broken), HomeDecision::KeepConfigUnreadable);

        let inside = vlt_home.join("vault");
        let path = write_config(d.path(), &Config::default_toml(&inside));
        assert_eq!(home_decision(&vlt_home, &path), HomeDecision::KeepVaultInside(inside));

        let path = write_config(d.path(), &Config::default_toml(&d.path().join("elsewhere")));
        assert_eq!(home_decision(&vlt_home, &path), HomeDecision::Ask, "the vault is outside vlt home");
    }
}
