//! `related`: the notes closest to a note, the tags they agree on, and where other notes name it without a link.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{Map, Value};

use crate::documents::resolve_in_vault;
use crate::error::{Result, VltError};
use crate::indexer::parser::{fence_open, is_fence_close, prose, LinkKind};
use crate::links::{LinkIndex, Resolution};

#[derive(Debug, serde::Serialize)]
pub struct RelatedResponse {
    pub path: String,
    pub notes: Vec<RelatedNote>,
    pub suggested_tags: Vec<(String, usize)>,
    pub mentions: Vec<Mention>,
}

#[derive(Debug, serde::Serialize)]
pub struct RelatedNote {
    pub path: String,
    pub title: String,
    pub score: f64,
    pub from: String,
    pub to: String,
    /// `to`, `from`, `both` or `none`.
    pub linked: &'static str,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Mention {
    pub path: String,
    pub chunk_id: String,
    pub line: usize,
    pub name: String,
}

fn internal(e: impl std::fmt::Display) -> VltError {
    VltError::Internal(e.to_string())
}

/// `k`: candidate chunks per chunk of `path` (the configured `[search] k`); `limit`: related notes returned.
pub fn related(conn: &Connection, vault: &Path, path: &str, k: usize, limit: usize) -> Result<RelatedResponse> {
    if limit < 1 {
        return Err(VltError::InvalidArguments("limit must be at least 1".into()));
    }
    resolve_in_vault(vault, path)?;
    let own_tags: Option<String> = conn
        .query_row("SELECT tags_json FROM files WHERE path = ?1", [path], |r| r.get(0))
        .optional()?;
    let Some(own_tags) = own_tags else { return Err(VltError::DocumentNotFound(path.to_string())) };
    let own_tags: Vec<String> = serde_json::from_str(&own_tags).map_err(internal)?;

    // Best (similarity, X's section, their section) per other note.
    let mut own = conn.prepare(
        "SELECT ch.section_id, v.embedding FROM chunks ch JOIN chunk_vectors v ON v.chunk_rowid = ch.id
         WHERE ch.path = ?1 ORDER BY ch.id",
    )?;
    let own: Vec<(String, Vec<u8>)> = own.query_map([path], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<std::result::Result<_, _>>()?;
    let mut nearest = conn.prepare(
        "SELECT ch.path, ch.section_id, vec_distance_cosine(v.embedding, ?1) AS d FROM chunk_vectors v
         JOIN chunks ch ON ch.id = v.chunk_rowid WHERE ch.path <> ?2 ORDER BY d, ch.id LIMIT ?3",
    )?;
    let mut best: HashMap<String, (f64, String, String)> = HashMap::new();
    for (from, embedding) in &own {
        let rows = nearest.query_map(params![embedding, path, k as i64], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, f64>(2)?)))?;
        for row in rows {
            let (other, to, distance) = row?;
            let score = 1.0 - distance;
            let entry = best.entry(other).or_insert((f64::NEG_INFINITY, String::new(), String::new()));
            if score > entry.0 {
                *entry = (score, from.clone(), to);
            }
        }
    }
    let mut ranked: Vec<(String, (f64, String, String))> = best.into_iter().collect();
    ranked.sort_by(|(pa, (sa, ..)), (pb, (sb, ..))| sb.total_cmp(sa).then_with(|| pa.cmp(pb)));
    ranked.truncate(limit);

    let index = LinkIndex::load(conn)?;
    let outgoing = outgoing_targets(conn, &index, path)?;
    let incoming: HashSet<String> = index.backlinks(conn, path)?.into_iter().map(|b| b.path).collect();
    let mut file = conn.prepare("SELECT title, tags_json FROM files WHERE path = ?1")?;
    let mut notes = Vec::with_capacity(ranked.len());
    for (other, (score, from, to)) in ranked {
        let (title, tags): (String, String) = file.query_row([&other], |r| Ok((r.get(0)?, r.get(1)?)))?;
        let linked = match (outgoing.contains(&other), incoming.contains(&other)) {
            (true, true) => "both",
            (true, false) => "to",
            (false, true) => "from",
            (false, false) => "none",
        };
        notes.push(RelatedNote { path: other, title, score, from, to, linked, tags: serde_json::from_str(&tags).map_err(internal)? });
    }
    // Only notes at least half as close as the closest vote, so weak neighbours of a small vault do not.
    let best = notes.first().map_or(0.0, |n| n.score);
    let voters: Vec<&RelatedNote> = notes.iter().filter(|n| best > 0.0 && n.score >= best / 2.0).collect();
    let suggested_tags = suggested_tags(&own_tags, &voters);
    let (title, properties): (String, Option<String>) =
        conn.query_row("SELECT title, properties_json FROM files WHERE path = ?1", [path], |r| Ok((r.get(0)?, r.get(1)?)))?;
    let properties: Option<Map<String, Value>> = match properties {
        Some(json) => serde_json::from_str(&json).map_err(internal)?,
        None => None,
    };
    let names = names(&title, properties.as_ref(), path);
    let mentions = mentions(conn, &index, path, &names)?;
    Ok(RelatedResponse { path: path.to_string(), notes, suggested_tags, mentions })
}

const MAX_MENTIONS: usize = 50;

/// X's title, its aliases (`aliases`: a string, or a list's strings) and its file name without `.md`:
/// case-insensitively unique, at least 3 characters with one alphanumeric.
fn names(title: &str, properties: Option<&Map<String, Value>>, path: &str) -> Vec<String> {
    let aliases: Vec<&str> = match properties.and_then(|p| p.get("aliases")) {
        Some(Value::String(s)) => vec![s.as_str()],
        Some(Value::Array(items)) => items.iter().filter_map(Value::as_str).collect(),
        _ => Vec::new(),
    };
    let stem = Path::new(path).file_stem().and_then(|s| s.to_str()).unwrap_or(path);
    let mut seen = HashSet::new();
    std::iter::once(title)
        .chain(aliases)
        .chain(std::iter::once(stem))
        .map(str::trim)
        .filter(|n| n.chars().count() >= 3 && n.chars().any(char::is_alphanumeric))
        .filter(|n| seen.insert(n.to_lowercase()))
        .map(str::to_string)
        .collect()
}

/// Sections of other notes whose prose names X without a link to it: the first occurrence per section.
fn mentions(conn: &Connection, index: &LinkIndex, path: &str, names: &[String]) -> Result<Vec<Mention>> {
    let mut candidates = conn.prepare(
        "SELECT ch.path, ch.section_id, ch.end_line, ch.content FROM chunks_fts JOIN chunks ch ON ch.id = chunks_fts.rowid
         WHERE chunks_fts MATCH ?1 AND ch.path <> ?2",
    )?;
    let mut found: HashMap<(String, String), Mention> = HashMap::new();
    for name in names {
        let phrase = format!("\"{}\"", name.replace('"', "\"\""));
        let rows = candidates.query_map(params![phrase, path], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?, r.get::<_, String>(3)?))
        })?;
        for row in rows {
            let (other, section, end_line, content) = row?;
            let lines: Vec<&str> = content.split('\n').collect();
            let Some(offset) = first_mention(&lines, name) else { continue };
            // The content's last line is `end_line`.
            let line = end_line as usize + 1 - lines.len() + offset;
            if found.get(&(other.clone(), section.clone())).is_some_and(|m| m.line <= line) {
                continue;
            }
            if section_links_to(conn, index, &other, &section, path)? {
                continue;
            }
            found.insert((other.clone(), section.clone()), Mention { path: other, chunk_id: section, line, name: name.clone() });
        }
    }
    let mut out: Vec<Mention> = found.into_values().collect();
    out.sort_by(|a, b| a.path.cmp(&b.path).then(a.line.cmp(&b.line)));
    out.truncate(MAX_MENTIONS);
    Ok(out)
}

/// Index of the first line, outside fenced code, whose prose holds `name` as a whole word.
fn first_mention(lines: &[&str], name: &str) -> Option<usize> {
    let name = name.to_lowercase();
    let mut fence: Option<(u8, usize)> = None;
    for (i, line) in lines.iter().enumerate() {
        if let Some((ch, len)) = fence {
            if is_fence_close(line, ch, len) {
                fence = None;
            }
            continue;
        }
        if let Some(open) = fence_open(line) {
            fence = Some(open);
            continue;
        }
        if has_word(&prose(line).to_lowercase(), &name) {
            return Some(i);
        }
    }
    None
}

/// `needle` in `hay` with no word character (alphanumeric or `_`) right before or after it.
fn has_word(hay: &str, needle: &str) -> bool {
    let word = |c: Option<char>| c.is_some_and(|c| c.is_alphanumeric() || c == '_');
    let mut from = 0;
    while let Some(i) = hay[from..].find(needle).map(|i| from + i) {
        if !word(hay[..i].chars().next_back()) && !word(hay[i + needle.len()..].chars().next()) {
            return true;
        }
        from = i + hay[i..].chars().next().map_or(1, char::len_utf8);
    }
    false
}

/// Whether a link within `section`'s lines of `src` resolves to `target`.
fn section_links_to(conn: &Connection, index: &LinkIndex, src: &str, section: &str, target: &str) -> Result<bool> {
    let (start, end): (Option<i64>, Option<i64>) = conn.query_row(
        "SELECT MIN(start_line), MAX(end_line) FROM chunks WHERE path = ?1 AND section_id = ?2",
        params![src, section],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let (Some(start), Some(end)) = (start, end) else { return Ok(false) };
    let mut stmt = conn.prepare("SELECT target, heading, kind FROM links WHERE src_path = ?1 AND line BETWEEN ?2 AND ?3")?;
    let rows = stmt.query_map(params![src, start, end], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?, r.get::<_, String>(2)?))
    })?;
    for row in rows {
        let (t, heading, kind) = row?;
        let kind = LinkKind::from_str(&kind).ok_or_else(|| internal(format!("link kind: {kind}")))?;
        if index.resolve(src, kind, &t, heading.as_deref()) == Resolution::Ok(target.to_string()) {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Paths the links of `path` resolve to.
fn outgoing_targets(conn: &Connection, index: &LinkIndex, path: &str) -> Result<HashSet<String>> {
    let mut stmt = conn.prepare("SELECT target, heading, kind FROM links WHERE src_path = ?1")?;
    let rows = stmt.query_map([path], |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?, r.get::<_, String>(2)?)))?;
    let mut out = HashSet::new();
    for row in rows {
        let (target, heading, kind) = row?;
        let kind = LinkKind::from_str(&kind).ok_or_else(|| internal(format!("link kind: {kind}")))?;
        if let Resolution::Ok(p) = index.resolve(path, kind, &target, heading.as_deref()) {
            out.insert(p);
        }
    }
    Ok(out)
}

/// Tags the voting `notes` carry and `own` lacks (case-insensitively), with 2 or more votes; each in the spelling
/// most notes use (a tie: string asc); votes desc, then tag asc.
fn suggested_tags(own: &[String], notes: &[&RelatedNote]) -> Vec<(String, usize)> {
    let own: HashSet<String> = own.iter().map(|t| t.to_lowercase()).collect();
    let mut votes: HashMap<String, (usize, HashMap<&str, usize>)> = HashMap::new();
    for note in notes {
        // Effective tags are already unique case-insensitively within a note.
        for tag in &note.tags {
            let key = tag.to_lowercase();
            if own.contains(&key) {
                continue;
            }
            let entry = votes.entry(key).or_default();
            entry.0 += 1;
            *entry.1.entry(tag.as_str()).or_default() += 1;
        }
    }
    let mut out: Vec<(String, usize)> = votes
        .into_values()
        .filter(|(n, _)| *n >= 2)
        .map(|(n, spellings)| {
            let spelling = spellings.into_iter().max_by(|(a, na), (b, nb)| na.cmp(nb).then_with(|| b.cmp(a))).map(|(s, _)| s.to_string());
            (spelling.unwrap_or_default(), n)
        })
        .collect();
    out.sort_by(|(a, na), (b, nb)| nb.cmp(na).then_with(|| a.cmp(b)));
    out
}


#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use crate::indexer::{reindex, scanner::build_exclude};
    use crate::ml::fake::FakeEmbedder;
    use crate::storage::db::{Db, IndexMeta};
    use std::{fs, sync::Arc};
    use tempfile::tempdir;

    fn vault(files: &[(&str, &str)]) -> (tempfile::TempDir, std::path::PathBuf, Db) {
        let d = tempdir().unwrap();
        let v = d.path().join("vault");
        for (p, c) in files { let p = v.join(p); fs::create_dir_all(p.parent().unwrap()).unwrap(); fs::write(p, c).unwrap(); }
        let mut db = Db::open(&d.path().join("vlt.db"), &IndexMeta { model_id: "fake".into(), dim: 384, fingerprint: "fake".into(), vault: "v".into() }).unwrap();
        let src = || -> crate::Result<Arc<dyn crate::ml::Embedder>> { Ok(Arc::new(FakeEmbedder)) };
        reindex(db.conn_mut(), &v, &build_exclude(&[]).unwrap(), &src, &mut |_| {}).unwrap();
        (d, v, db)
    }

    /// Fake-embedder similarity follows shared words: `near` shares 3 words with x's Alpha, `mid` 2 with Beta,
    /// `far` and `other` none.
    fn neighbours() -> (tempfile::TempDir, std::path::PathBuf, Db) {
        vault(&[
            ("x.md", "---\ntags: [rust]\n---\n# Alpha\napple banana cherry [[near]]\n# Beta\nzebra yak walrus [[mid]]\n"),
            ("near.md", "---\ntags: [Rust, Fruit, sweet]\nup: \"[[x]]\"\n---\n# Fruit\napple banana cherry date\n"),
            ("mid.md", "---\ntags: [fruit, animals]\n---\n# Zoo\nzebra yak lion tiger\n"),
            ("far.md", "---\ntags: [fruit]\n---\n# Other\nquantum physics lecture notes [[x]]\n"),
            ("other.md", "# Misc\nkettle teapot\n"),
            ("empty.md", ""),
        ])
    }

    #[test]
    fn related_ranks_other_notes_by_their_best_section_pair() {
        let (_d, v, db) = neighbours();
        let r = related(db.conn(), &v, "x.md", 50, 10).unwrap();
        let rows: Vec<_> = r.notes.iter().map(|n| n.path.as_str()).collect();
        assert_eq!(rows, vec!["near.md", "mid.md", "far.md", "other.md", ], "x itself and the chunkless empty.md are not listed; ties by path");
        assert_eq!((r.notes[0].from.as_str(), r.notes[0].to.as_str()), ("x.md#Alpha", "near.md#Fruit"));
        assert_eq!((r.notes[1].from.as_str(), r.notes[1].to.as_str()), ("x.md#Beta", "mid.md#Zoo"));
        assert!(r.notes[0].score > r.notes[1].score && r.notes[1].score > r.notes[2].score);
        assert_eq!(r.notes[0].title, "Fruit");
        assert_eq!(r.notes[0].tags, vec!["Rust", "Fruit", "sweet"]);
        assert_eq!(related(db.conn(), &v, "x.md", 50, 2).unwrap().notes.len(), 2, "limit");
    }

    #[test]
    fn related_marks_the_links_that_already_exist() {
        let (_d, v, db) = neighbours();
        let r = related(db.conn(), &v, "x.md", 50, 10).unwrap();
        let linked: Vec<_> = r.notes.iter().map(|n| (n.path.as_str(), n.linked)).collect();
        assert_eq!(linked, vec![("near.md", "both"), ("mid.md", "to"), ("far.md", "from"), ("other.md", "none")], "near links back through a property");
    }

    #[test]
    fn suggested_tags_are_those_two_or_more_related_notes_carry_and_x_lacks() {
        let (_d, v, db) = neighbours();
        let r = related(db.conn(), &v, "x.md", 50, 10).unwrap();
        assert_eq!(r.suggested_tags, vec![("Fruit".to_string(), 2)], "near and mid vote (far is under half the best score); `Rust` is x's `rust`; `sweet` and `animals` have one vote; a 1-1 spelling tie goes to the string-ascending first");
        assert_eq!(related(db.conn(), &v, "x.md", 50, 2).unwrap().suggested_tags, vec![("Fruit".to_string(), 2)], "votes come from the returned notes only: near and mid vote, far is under half the best score; a spelling tie goes to the string-ascending first");
    }

    #[test]
    fn notes_less_than_half_as_close_as_the_best_do_not_vote() {
        let (_d, v, db) = vault(&[
            ("x.md", "# X\napple banana cherry\n"),
            ("s1.md", "---\ntags: [t]\n---\n# S1\napple banana cherry\n"),
            ("w1.md", "---\ntags: [weak]\n---\n# W1\napple zzz yyy qqq www\n"),
            ("w2.md", "---\ntags: [weak]\n---\n# W2\napple vvv uuu ttt sss\n"),
        ]);
        let r = related(db.conn(), &v, "x.md", 50, 10).unwrap();
        assert_eq!(r.notes.len(), 3);
        assert!(r.notes[1].score < r.notes[0].score / 2.0, "the fixture's weak notes are under half the best");
        assert!(r.suggested_tags.is_empty(), "`weak` has two votes, but from notes under half the best score");
    }

    #[test]
    fn related_of_a_note_without_chunks_is_empty_and_errors_match_get() {
        let (_d, v, db) = neighbours();
        let r = related(db.conn(), &v, "empty.md", 50, 10).unwrap();
        assert!(r.notes.is_empty() && r.suggested_tags.is_empty());
        assert!(matches!(related(db.conn(), &v, "missing.md", 50, 10), Err(VltError::DocumentNotFound(_))));
        assert!(matches!(related(db.conn(), &v, "../x.md", 50, 10), Err(VltError::PathOutsideVault(_))));
        assert!(matches!(related(db.conn(), &v, "x.md", 50, 0), Err(VltError::InvalidArguments(_))));
    }

    #[test]
    fn suggested_tags_order_by_votes_then_tag() {
        let (_d, v, db) = vault(&[
            ("x.md", "# X\napple banana\n"),
            ("a.md", "---\ntags: [zeta, MID, beta]\n---\n# A\napple banana one\n"),
            ("b.md", "---\ntags: [zeta, MID, beta]\n---\n# B\napple banana two\n"),
            ("c.md", "---\ntags: [zeta, MID]\n---\n# C\napple banana three\n"),
        ]);
        let r = related(db.conn(), &v, "x.md", 50, 10).unwrap();
        assert_eq!(r.suggested_tags, vec![("MID".to_string(), 3), ("zeta".to_string(), 3), ("beta".to_string(), 2)]);
    }

    #[test]
    fn names_are_title_aliases_and_file_name_unique_and_at_least_three_characters() {
        let props = json!({"aliases": ["Rust MCP", "AI", "mcp server", "---"]});
        assert_eq!(names("MCP server", props.as_object(), "projects/mcp_rust.md"), vec!["MCP server", "Rust MCP", "mcp_rust"]);
        let one = json!({"aliases": "Solo alias"});
        assert_eq!(names("mcp_rust", one.as_object(), "mcp_rust.md"), vec!["mcp_rust", "Solo alias"]);
    }

    fn mentioned() -> (tempfile::TempDir, std::path::PathBuf, Db) {
        vault(&[
            ("projects/mcp_rust.md", "---\ntitle: MCP server\naliases: [Rust MCP, AI]\n---\n# MCP server\nbody\n"),
            ("a.md", "# A\nThe mcp server handles requests.\n\n# B\nSee [[mcp_rust|the MCP server]] and the MCP server again.\n\n# C\nUse `mcp_rust` or mcp_rustacean here.\n\n# D\n```\nMCP server\n```\nrust mcp is fine\n"),
            ("b.md", "Two mentions: MCP server here.\nAnd MCP server there.\n"),
            ("c.md", "[[other|MCP server]] only.\n"),
            ("d.md", "[MCP server](https://example.com) only.\n"),
        ])
    }

    #[test]
    fn mentions_are_whole_word_names_in_sections_that_do_not_link_yet() {
        let (_d, v, db) = mentioned();
        let m = related(db.conn(), &v, "projects/mcp_rust.md", 50, 10).unwrap().mentions;
        let rows: Vec<_> = m.iter().map(|m| (m.path.as_str(), m.chunk_id.as_str(), m.line, m.name.as_str())).collect();
        assert_eq!(rows, vec![
            ("a.md", "a.md#A", 2, "MCP server"),
            ("a.md", "a.md#D", 14, "Rust MCP"),
            ("b.md", "b.md#", 1, "MCP server"),
        ], "B links already; C has the name only in code or inside a longer word; D's fence is skipped; c.md and d.md have it only inside links; one per section");
    }

    #[test]
    fn mentions_stop_at_fifty() {
        let mut files = vec![("x.md".to_string(), "# MCP server\nbody\n".to_string())];
        files.extend((0..60).map(|i| (format!("n{i:02}.md"), "Uses the MCP server.\n".to_string())));
        let refs: Vec<(&str, &str)> = files.iter().map(|(p, c)| (p.as_str(), c.as_str())).collect();
        let (_d, v, db) = vault(&refs);
        let m = related(db.conn(), &v, "x.md", 50, 10).unwrap().mentions;
        assert_eq!((m.len(), m[0].path.as_str(), m[49].path.as_str()), (50, "n00.md", "n49.md"));
    }

    #[test]
    fn has_word_tries_every_occurrence_and_treats_underscore_as_a_word_character() {
        assert!(has_word("xha-ha-ha", "ha-ha"), "a rejected occurrence does not hide an overlapping one");
        assert!(has_word("ba-a-a", "a-a"));
        assert!(!has_word("mcp_rust_x", "mcp_rust"), "`_` after");
        assert!(!has_word("x_mcp_rust", "mcp_rust"), "`_` before");
        assert!(has_word("see mcp_rust.", "mcp_rust"));
    }

    #[test]
    fn mentions_skip_the_note_itself() {
        let (_d, v, db) = vault(&[("x.md", "# X note\nThe X note is here.\n"), ("y.md", "About the X note.\n")]);
        let m = related(db.conn(), &v, "x.md", 50, 10).unwrap().mentions;
        assert_eq!(m.iter().map(|m| m.path.as_str()).collect::<Vec<_>>(), vec!["y.md"]);
    }

    #[test]
    fn a_section_keeps_its_earliest_mention_across_names() {
        let (_d, v, db) = vault(&[
            ("x.md", "---\ntitle: MCP server\naliases: [Rust MCP]\n---\nbody\n"),
            ("n.md", "Rust MCP first.\nThen MCP server.\n"),
        ]);
        let m = related(db.conn(), &v, "x.md", 50, 10).unwrap().mentions;
        assert_eq!(m.iter().map(|m| (m.line, m.name.as_str())).collect::<Vec<_>>(), vec![(1, "Rust MCP")], "the title is tried first but its line is later");
    }
}
