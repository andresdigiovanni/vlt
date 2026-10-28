const SKILL: &str = include_str!("../skill/SKILL.md");

#[test]
fn the_skill_covers_every_tool_and_error_code() {
    for tool in ["search", "get", "properties", "folders", "related", "audit", "reindex", "edit", "update_properties", "create", "move", "delete"] {
        assert!(SKILL.contains(&format!("`{tool}`")), "SKILL.md does not document `{tool}`");
    }
    for code in ["index_rebuild_required", "document_not_found", "path_outside_vault", "invalid_arguments",
                 "not_found", "ambiguous", "file_changed", "invalid_frontmatter", "already_exists"] {
        assert!(SKILL.contains(&format!("`{code}`")), "SKILL.md does not explain `{code}`");
    }
}

#[test]
fn the_skill_states_when_to_use_it_and_does_not_claim_the_vault_is_read_only() {
    let (front, _) = SKILL[4..].split_once("\n---\n").unwrap();
    for trigger in ["add properties", "link", "process the note", "draft a new note", "audit the vault"] {
        assert!(front.contains(trigger), "description misses the trigger {trigger:?}");
    }
    assert!(!SKILL.contains("Read-only."), "the old read-only rule is gone");
    assert!(!SKILL.contains("no editing tools"), "the old 'no editing tools' rule is gone");
}

#[test]
fn the_skill_routes_moves_and_deletes_to_the_tools() {
    let (front, _) = SKILL[4..].split_once("\n---\n").unwrap();
    assert!(!front.contains("not to move or delete notes"));
    assert!(!SKILL.contains("say vlt cannot"));
    assert!(SKILL.contains(".trash/"));
}
