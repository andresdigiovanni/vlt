use std::path::{Path, PathBuf};

use super::Target;
use crate::{Result, VltError};

fn prompt_error(e: inquire::InquireError) -> VltError {
    VltError::Internal(format!("prompt failed: {e}"))
}

/// Asks for the vault path, proposing `default` when there is one.
pub fn ask_vault(default: Option<&Path>) -> Result<PathBuf> {
    let shown = default.map(|p| p.display().to_string());
    let mut text = inquire::Text::new("Path to your Obsidian vault:");
    if let Some(d) = shown.as_deref() {
        text = text.with_default(d);
    }
    let answer = text.prompt().map_err(prompt_error)?;
    let answer = answer.trim();
    if answer.is_empty() {
        return Err(VltError::InvalidArguments("a vault path is required".into()));
    }
    Ok(PathBuf::from(answer))
}

pub fn confirm(message: &str, default: bool) -> Result<bool> {
    inquire::Confirm::new(message).with_default(default).prompt().map_err(prompt_error)
}

/// Multi-select of the agents to configure; every supported agent is preselected.
pub fn select_targets() -> Result<Vec<Target>> {
    let all: &[(Target, &str)] = &[(Target::Claude, "Claude Code"), (Target::Opencode, "opencode"), (Target::Codex, "Codex")];
    let labels: Vec<&str> = all.iter().map(|(_, l)| *l).collect();
    let preselected: Vec<usize> = (0..all.len()).collect();
    let chosen = inquire::MultiSelect::new("Agents to configure:", labels)
        .with_default(&preselected)
        .prompt()
        .map_err(prompt_error)?;
    Ok(all.iter().filter(|(_, l)| chosen.contains(l)).map(|(t, _)| *t).collect())
}
