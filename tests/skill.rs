const SKILL: &str = include_str!("../skill/SKILL.md");

#[test]
fn the_skill_covers_every_tool_and_error_code() {
    for tool in ["search", "get", "properties", "reindex", "edit", "update_properties"] {
        assert!(SKILL.contains(&format!("`{tool}`")), "SKILL.md does not document `{tool}`");
    }
    for code in ["index_rebuild_required", "document_not_found", "path_outside_vault", "invalid_arguments",
                 "not_found", "ambiguous", "file_changed", "invalid_frontmatter"] {
        assert!(SKILL.contains(&format!("`{code}`")), "SKILL.md does not explain `{code}`");
    }
}

#[test]
fn the_skill_states_when_to_use_it_and_does_not_claim_the_vault_is_read_only() {
    let (front, _) = SKILL[4..].split_once("\n---\n").unwrap();
    for trigger in ["add properties", "link", "process the note"] {
        assert!(front.contains(trigger), "description misses the trigger {trigger:?}");
    }
    assert!(!SKILL.contains("Read-only."), "the old read-only rule is gone");
    assert!(!SKILL.contains("no editing tools"), "the old 'no editing tools' rule is gone");
}
