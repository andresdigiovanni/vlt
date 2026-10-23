use std::path::{Path, PathBuf};

use super::agentsetup::opencode_dir;
use super::Target;
use crate::Result;

pub const SKILL_MD: &str = include_str!("../../skill/SKILL.md");

/// Where the Skill of `target` lives, e.g. `~/.claude/skills/vlt/SKILL.md`.
pub fn skill_path(home: &Path, target: Target) -> PathBuf {
    match target {
        Target::Claude => home.join(".claude/skills/vlt/SKILL.md"),
        Target::Opencode => opencode_dir(home).join("skills/vlt/SKILL.md"),
        Target::Codex => home.join(".codex/skills/vlt/SKILL.md"),
    }
}

/// Writes the embedded Skill, overwriting any previous version.
pub fn install_skill(home: &Path, target: Target) -> Result<PathBuf> {
    let path = skill_path(home, target);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(&path, SKILL_MD)?;
    Ok(path)
}

/// Removes the Skill's `vlt/` directory; false when it was not installed.
pub fn uninstall_skill(home: &Path, target: Target) -> Result<bool> {
    let path = skill_path(home, target);
    match path.parent() {
        Some(dir) if dir.exists() => {
            std::fs::remove_dir_all(dir)?;
            Ok(true)
        }
        _ => Ok(false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn install_and_uninstall_skill() {
        let d = tempfile::tempdir().unwrap();
        let p = install_skill(d.path(), Target::Claude).unwrap();
        assert_eq!(p, d.path().join(".claude/skills/vlt/SKILL.md"));
        assert_eq!(std::fs::read_to_string(&p).unwrap(), SKILL_MD);
        assert!(SKILL_MD.starts_with("---\nname: vlt\n"));
        assert!(uninstall_skill(d.path(), Target::Claude).unwrap());
        assert!(!d.path().join(".claude/skills/vlt").exists());
        assert!(!uninstall_skill(d.path(), Target::Claude).unwrap());
    }

    #[test]
    fn skill_covers_the_query_contract() {
        let fm_end = SKILL_MD[4..].find("\n---\n").expect("frontmatter closes") + 4;
        let fm = &SKILL_MD[..fm_end];
        assert!(fm.contains("\ndescription: "));
        assert!(fm.contains("general knowledge"), "description must exclude general knowledge");
        for needle in ["search", "`get`", "`properties`", "reindex", "min_score", "where", "path_prefix",
                       "chunk_id", "English", "index_rebuild_required", "vlt reindex --force",
                       "document_not_found", "in_progress", "reformulate", "user's language",
                       "the term a note would use", "read its `content`", "older vlt"] {
            assert!(SKILL_MD.contains(needle), "SKILL.md must mention {needle}");
        }
        for forbidden in ["VLT_HOME", "VLT_FAKE_MODELS", "edit_document"] {
            assert!(!SKILL_MD.contains(forbidden), "SKILL.md must not mention {forbidden}");
        }
    }
}
