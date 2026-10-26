//! `audit`: problems that live across notes — spellings, folder gaps, links — for the agent to fix.

use std::collections::{BTreeMap, HashMap, HashSet};

use rusqlite::Connection;
use serde_json::{Map, Value};

use crate::documents::{list_properties, variant_form};
use crate::error::{Result, VltError};
use crate::indexer::parser::LinkKind;
use crate::links::{LinkIndex, Resolution};

pub const MAX_ITEMS: usize = 50;

#[derive(Debug, serde::Serialize)]
pub struct Section<T> {
    pub total: usize,
    pub items: Vec<T>,
}

#[derive(Debug, serde::Serialize)]
pub struct AuditResponse {
    pub value_variants: Section<ValueVariant>,
    pub rare_keys: Section<RareKey>,
    pub key_variants: Section<KeyVariant>,
    pub folder_gaps: Section<FolderGap>,
    pub broken_links: Section<BrokenLink>,
    pub orphans: Section<NotePath>,
    pub no_outgoing: Section<NotePath>,
}

#[derive(Debug, PartialEq, serde::Serialize)]
pub struct ValueVariant {
    pub key: String,
    pub spellings: Vec<(Value, usize)>,
    /// Notes using a spelling other than the first, with it: the notes a fix changes.
    pub notes: Vec<(String, Value)>,
}

#[derive(Debug, PartialEq, serde::Serialize)]
pub struct RareKey {
    pub key: String,
    pub path: String,
}

#[derive(Debug, PartialEq, serde::Serialize)]
pub struct KeyVariant {
    pub keys: Vec<(String, usize)>,
    /// Notes using a key other than the first, with it: the notes a rename changes.
    pub notes: Vec<(String, String)>,
}

#[derive(Debug, PartialEq, serde::Serialize)]
pub struct FolderGap {
    pub folder: String,
    pub kind: &'static str,
    pub name: String,
    pub present: usize,
    pub notes: usize,
    pub missing: Vec<String>,
}

#[derive(Debug, PartialEq, serde::Serialize)]
pub struct BrokenLink {
    pub path: String,
    pub line: usize,
    pub raw: String,
    /// `broken` or `ambiguous`.
    pub status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub property: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub candidates: Vec<String>,
}

#[derive(Debug, PartialEq, serde::Serialize)]
pub struct NotePath {
    pub path: String,
}

fn internal(e: impl std::fmt::Display) -> VltError {
    VltError::Internal(e.to_string())
}

impl<T> Section<T> {
    fn of(mut items: Vec<T>) -> Section<T> {
        let total = items.len();
        items.truncate(MAX_ITEMS);
        Section { total, items }
    }
}

/// A note in scope: its keys other than `tags`, and its effective tags.
struct Note {
    path: String,
    keys: Vec<String>,
    values: Map<String, Value>,
    tags: Vec<String>,
}

pub fn audit(conn: &Connection, path_prefix: Option<&str>) -> Result<AuditResponse> {
    let notes = load_notes(conn, path_prefix)?;

    let mut properties = list_properties(conn, None, path_prefix, None)?.properties;
    properties.sort_by(|a, b| a.key.cmp(&b.key));
    let value_variants: Vec<ValueVariant> = properties
        .into_iter()
        .flat_map(|p| {
            let notes = &notes;
            let key = p.key;
            p.variants.into_iter().map(move |spellings| {
                let notes = value_variant_notes(notes, &key, &spellings);
                ValueVariant { key: key.clone(), spellings, notes }
            })
        })
        .collect();

    let by_key = notes_by_key(&notes);
    let rare_keys = by_key
        .iter()
        .filter(|(_, paths)| paths.len() == 1)
        .map(|(k, paths)| RareKey { key: k.clone(), path: paths[0].to_string() })
        .collect();

    let mut forms: BTreeMap<String, Vec<(String, usize)>> = BTreeMap::new();
    for (k, paths) in &by_key {
        forms.entry(variant_form(k)).or_default().push((k.clone(), paths.len()));
    }
    let mut key_variants: Vec<KeyVariant> = forms
        .into_values()
        .filter(|keys| keys.len() > 1)
        .map(|mut keys| {
            keys.sort_by(|(ka, na), (kb, nb)| nb.cmp(na).then_with(|| ka.cmp(kb)));
            let notes = key_variant_notes(&notes, &keys);
            KeyVariant { keys, notes }
        })
        .collect();
    key_variants.sort_by(|a, b| a.keys[0].0.cmp(&b.keys[0].0));

    let (broken, orphans, no_outgoing) = link_sections(conn, &notes)?;

    Ok(AuditResponse {
        value_variants: Section::of(value_variants),
        rare_keys: Section::of(rare_keys),
        key_variants: Section::of(key_variants),
        folder_gaps: Section::of(folder_gaps(&notes)),
        broken_links: Section::of(broken),
        orphans: Section::of(orphans),
        no_outgoing: Section::of(no_outgoing),
    })
}

/// The notes whose path starts with `path_prefix`, by path.
fn load_notes(conn: &Connection, path_prefix: Option<&str>) -> Result<Vec<Note>> {
    let mut stmt = conn.prepare("SELECT path, properties_json, tags_json FROM files ORDER BY path")?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?, r.get::<_, String>(2)?)))?;
    let mut notes = Vec::new();
    for row in rows {
        let (path, properties, tags) = row?;
        if path_prefix.is_some_and(|p| !path.starts_with(p)) {
            continue;
        }
        let props: Map<String, Value> = match properties {
            Some(json) => serde_json::from_str(&json).map_err(internal)?,
            None => Map::new(),
        };
        let keys = props.keys().filter(|k| *k != "tags").cloned().collect();
        let tags: Vec<String> = serde_json::from_str(&tags).map_err(internal)?;
        notes.push(Note { path, keys, values: props, tags });
    }
    Ok(notes)
}

/// The notes in scope holding a spelling other than the first, with it, by path then spelling.
fn value_variant_notes(notes: &[Note], key: &str, spellings: &[(Value, usize)]) -> Vec<(String, Value)> {
    let others: HashSet<&str> = spellings.iter().skip(1).filter_map(|(v, _)| v.as_str()).collect();
    let mut out = Vec::new();
    for n in notes {
        let held: Vec<&str> = if key == "tags" {
            n.tags.iter().map(String::as_str).collect()
        } else {
            match n.values.get(key) {
                Some(Value::String(s)) => vec![s.as_str()],
                Some(Value::Array(items)) => items.iter().filter_map(Value::as_str).collect(),
                _ => Vec::new(),
            }
        };
        let mut mine: Vec<&str> = held.into_iter().filter(|s| others.contains(s)).collect();
        mine.sort();
        mine.dedup();
        out.extend(mine.into_iter().map(|s| (n.path.clone(), Value::String(s.to_string()))));
    }
    out
}

/// The notes in scope using a key other than the first, with it, by path then key.
fn key_variant_notes(notes: &[Note], keys: &[(String, usize)]) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for n in notes {
        let mut mine: Vec<&str> = keys
            .iter()
            .skip(1)
            .map(|(k, _)| k.as_str())
            .filter(|k| if *k == "tags" { !n.tags.is_empty() } else { n.keys.iter().any(|nk| nk == k) })
            .collect();
        mine.sort();
        out.extend(mine.into_iter().map(|k| (n.path.clone(), k.to_string())));
    }
    out
}

/// Each key with the notes that have it (`tags`: the notes with an effective tag), key asc.
fn notes_by_key(notes: &[Note]) -> BTreeMap<String, Vec<&str>> {
    let mut out: BTreeMap<String, Vec<&str>> = BTreeMap::new();
    for n in notes {
        for k in &n.keys {
            out.entry(k.clone()).or_default().push(&n.path);
        }
        if !n.tags.is_empty() {
            out.entry("tags".to_string()).or_default().push(&n.path);
        }
    }
    out
}

/// Keys, then tags (case-insensitive, majority spelling), on ≥ 80% but not all of a folder's direct notes;
/// a folder needs 3 or more of them.
fn folder_gaps(notes: &[Note]) -> Vec<FolderGap> {
    let mut folders: BTreeMap<&str, Vec<&Note>> = BTreeMap::new();
    for n in notes {
        folders.entry(n.path.rfind('/').map_or("", |i| &n.path[..=i])).or_default().push(n);
    }
    let mut out = Vec::new();
    for (folder, members) in folders {
        let total = members.len();
        if total < 3 {
            continue;
        }
        let is_gap = |present: usize| present < total && present * 5 >= total * 4;
        let missing = |has: &HashSet<&str>| {
            members.iter().filter(|n| !has.contains(n.path.as_str())).map(|n| n.path.clone()).collect::<Vec<_>>()
        };
        let mut keys: BTreeMap<&str, HashSet<&str>> = BTreeMap::new();
        let mut tags: HashMap<String, (HashSet<&str>, HashMap<&str, usize>)> = HashMap::new();
        for n in &members {
            for k in &n.keys {
                keys.entry(k).or_default().insert(&n.path);
            }
            for t in &n.tags {
                let entry = tags.entry(t.to_lowercase()).or_default();
                entry.0.insert(&n.path);
                *entry.1.entry(t).or_default() += 1;
            }
        }
        for (k, has) in &keys {
            if is_gap(has.len()) {
                out.push(FolderGap {
                    folder: folder.to_string(),
                    kind: "key",
                    name: k.to_string(),
                    present: has.len(),
                    notes: total,
                    missing: missing(has),
                });
            }
        }
        let mut tag_gaps: Vec<FolderGap> = tags
            .into_values()
            .filter(|(has, _)| is_gap(has.len()))
            .map(|(has, spellings)| {
                let name = spellings
                    .into_iter()
                    .max_by(|(a, na), (b, nb)| na.cmp(nb).then_with(|| b.cmp(a)))
                    .map(|(s, _)| s.to_string());
                FolderGap {
                    folder: folder.to_string(),
                    kind: "tag",
                    name: name.unwrap_or_default(),
                    present: has.len(),
                    notes: total,
                    missing: missing(&has),
                }
            })
            .collect();
        tag_gaps.sort_by(|a, b| a.name.cmp(&b.name));
        out.extend(tag_gaps);
    }
    out
}

/// Broken and ambiguous links of the notes in scope, and those of them no other note links to or that link to
/// no other note; backlinks count from every indexed note.
fn link_sections(conn: &Connection, notes: &[Note]) -> Result<(Vec<BrokenLink>, Vec<NotePath>, Vec<NotePath>)> {
    let in_scope: HashSet<&str> = notes.iter().map(|n| n.path.as_str()).collect();
    let index = LinkIndex::load(conn)?;
    let mut stmt = conn.prepare("SELECT src_path, target, heading, raw, line, kind, property FROM links ORDER BY src_path, line, rowid")?;
    let rows = stmt.query_map([], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, Option<String>>(2)?, r.get::<_, String>(3)?,
            r.get::<_, i64>(4)?, r.get::<_, String>(5)?, r.get::<_, Option<String>>(6)?))
    })?;
    let mut linked: HashSet<String> = HashSet::new();
    let mut linking: HashSet<String> = HashSet::new();
    let mut broken = Vec::new();
    for row in rows {
        let (src, target, heading, raw, line, kind, property) = row?;
        let kind = LinkKind::from_str(&kind).ok_or_else(|| internal(format!("link kind: {kind}")))?;
        let (status, candidates) = match index.resolve(&src, kind, &target, heading.as_deref()) {
            Resolution::Ok(p) => {
                if p != src {
                    linked.insert(p);
                    linking.insert(src);
                }
                continue;
            }
            Resolution::Broken => ("broken", Vec::new()),
            Resolution::Ambiguous(c) => ("ambiguous", c),
        };
        if in_scope.contains(src.as_str()) {
            broken.push(BrokenLink { path: src, line: line as usize, raw, status, property, candidates });
        }
    }
    let without = |set: &HashSet<String>| notes.iter().filter(|n| !set.contains(&n.path)).map(|n| NotePath { path: n.path.clone() }).collect();
    Ok((broken, without(&linked), without(&linking)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::indexer::{reindex, scanner::build_exclude};
    use crate::ml::fake::FakeEmbedder;
    use crate::storage::db::{Db, IndexMeta};
    use serde_json::json;
    use std::{fs, sync::Arc};
    use tempfile::tempdir;

    fn vault(files: &[(&str, &str)]) -> (tempfile::TempDir, Db) {
        let d = tempdir().unwrap();
        let v = d.path().join("vault");
        for (p, c) in files { let p = v.join(p); fs::create_dir_all(p.parent().unwrap()).unwrap(); fs::write(p, c).unwrap(); }
        let mut db = Db::open(&d.path().join("vlt.db"), &IndexMeta { model_id: "fake".into(), dim: 384, fingerprint: "fake".into(), vault: "v".into() }).unwrap();
        let src = || -> crate::Result<Arc<dyn crate::ml::Embedder>> { Ok(Arc::new(FakeEmbedder)) };
        reindex(db.conn_mut(), &v, &build_exclude(&[]).unwrap(), &src, &mut |_| {}).unwrap();
        (d, db)
    }

    fn props_vault() -> (tempfile::TempDir, Db) {
        vault(&[
            ("p/a.md", "---\nstatus: done\ntags: [Rust, Infra]\nrepo_role: x\n---\na\n"),
            ("p/b.md", "---\nstatus: done\ntags: [rust, Infra]\nRepo-Role: y\n---\nb\n"),
            ("p/c.md", "---\nstatus: open\ntags: [rust, backup, Infra]\n---\nc\n"),
            ("p/d.md", "---\nstatus: open\ntags: [rust, backup, infra]\n---\nd\n"),
            ("p/e.md", "---\ntags: [RUST, backup]\nowner: me\n---\ne\n"),
            ("q/x.md", "---\nstatus: done\n---\nx\n"),
            ("q/y.md", "---\nstatus: done\n---\ny\n"),
        ])
    }

    #[test]
    fn value_variants_list_each_group_by_key() {
        let (_d, db) = props_vault();
        let r = audit(db.conn(), None).unwrap();
        assert_eq!(r.value_variants.total, 2);
        assert_eq!(r.value_variants.items, vec![
            ValueVariant { key: "tags".into(), spellings: vec![(json!("rust"), 3), (json!("RUST"), 1), (json!("Rust"), 1)],
                           notes: vec![("p/a.md".into(), json!("Rust")), ("p/e.md".into(), json!("RUST"))] },
            ValueVariant { key: "tags".into(), spellings: vec![(json!("Infra"), 3), (json!("infra"), 1)],
                           notes: vec![("p/d.md".into(), json!("infra"))] },
        ]);
    }

    #[test]
    fn rare_keys_and_key_variants() {
        let (_d, db) = props_vault();
        let r = audit(db.conn(), None).unwrap();
        let rk = |k: &str, p: &str| RareKey { key: k.into(), path: p.into() };
        assert_eq!(r.rare_keys.items, vec![rk("Repo-Role", "p/b.md"), rk("owner", "p/e.md"), rk("repo_role", "p/a.md")], "key asc, byte order");
        assert_eq!(r.key_variants.items, vec![KeyVariant { keys: vec![("Repo-Role".into(), 1), ("repo_role".into(), 1)], notes: vec![("p/a.md".into(), "repo_role".into())] }]);
    }

    #[test]
    fn variant_notes_cover_list_items_and_scalars() {
        let (_d, db) = vault(&[
            ("a.md", "---\nk: [x-y, X_Y]\n---\n"),
            ("b.md", "---\nk: [x-y]\n---\n"),
            ("c.md", "---\nk: x y\n---\n"),
        ]);
        let r = audit(db.conn(), None).unwrap();
        assert_eq!(r.value_variants.items, vec![ValueVariant {
            key: "k".into(),
            spellings: vec![(json!("x-y"), 2), (json!("X_Y"), 1), (json!("x y"), 1)],
            notes: vec![("a.md".into(), json!("X_Y")), ("c.md".into(), json!("x y"))],
        }]);
    }

    #[test]
    fn folder_gaps_are_keys_and_tags_on_four_fifths_but_not_all_of_a_folder() {
        let (_d, db) = props_vault();
        let r = audit(db.conn(), None).unwrap();
        let gap = |kind, name: &str| FolderGap { folder: "p/".into(), kind, name: name.into(), present: 4, notes: 5, missing: vec!["p/e.md".into()] };
        assert_eq!(r.folder_gaps.items, vec![gap("key", "status"), gap("tag", "Infra")],
                   "`backup` is on 3 of 5; `rust` is on all 5 in any case; `tags` itself is never a gap; q/ has 2 notes");
    }

    #[test]
    fn path_prefix_limits_every_property_section() {
        let (_d, db) = props_vault();
        let r = audit(db.conn(), Some("q/")).unwrap();
        assert_eq!((r.value_variants.total, r.rare_keys.total, r.key_variants.total, r.folder_gaps.total), (0, 0, 0, 0));
    }

    #[test]
    fn a_section_keeps_fifty_items_and_counts_them_all() {
        let files: Vec<(String, String)> = (0..60).map(|i| (format!("n{i:02}.md"), format!("---\nk{i:02}: 1\n---\n"))).collect();
        let refs: Vec<(&str, &str)> = files.iter().map(|(p, c)| (p.as_str(), c.as_str())).collect();
        let (_d, db) = vault(&refs);
        let r = audit(db.conn(), None).unwrap();
        assert_eq!((r.rare_keys.total, r.rare_keys.items.len(), r.rare_keys.items[0].key.as_str()), (60, 50, "k00"));
    }

    #[test]
    fn the_tags_key_is_never_a_folder_gap() {
        let (_d, db) = vault(&[
            ("g/1.md", "---\ntags: [a]\n---\n1\n"),
            ("g/2.md", "---\ntags: [b]\n---\n2\n"),
            ("g/3.md", "---\ntags: [c]\n---\n3\n"),
            ("g/4.md", "---\ntags: [d]\n---\n4\n"),
            ("g/5.md", "5\n"),
        ]);
        assert!(audit(db.conn(), None).unwrap().folder_gaps.items.is_empty(), "`tags` is on 4 of 5 notes, but only its values can be gaps");
    }

    #[test]
    fn a_tag_one_note_has_makes_tags_a_rare_key() {
        let (_d, db) = vault(&[("a.md", "Body #solo.\n"), ("b.md", "x\n")]);
        assert_eq!(audit(db.conn(), None).unwrap().rare_keys.items, vec![RareKey { key: "tags".into(), path: "a.md".into() }]);
    }

    #[test]
    fn a_tag_gap_is_named_by_its_most_used_spelling_and_a_tie_goes_to_the_first_string() {
        let (_d, db) = vault(&[
            ("g/1.md", "---\ntags: [Ops]\n---\n1\n"),
            ("g/2.md", "---\ntags: [ops]\n---\n2\n"),
            ("g/3.md", "---\ntags: [Ops]\n---\n3\n"),
            ("g/4.md", "---\ntags: [ops]\n---\n4\n"),
            ("g/5.md", "5\n"),
        ]);
        let gaps = audit(db.conn(), None).unwrap().folder_gaps.items;
        assert_eq!(gaps, vec![FolderGap { folder: "g/".into(), kind: "tag", name: "Ops".into(), present: 4, notes: 5, missing: vec!["g/5.md".into()] }]);
    }

    fn links_vault() -> (tempfile::TempDir, Db) {
        vault(&[
            ("a.md", "Links [[b]] and [[gone]] and [[dup]].\n"),
            ("b.md", "---\nup: \"[[nowhere]]\"\n---\nNo body links.\n"),
            ("c.md", "Self [[c]] only.\n"),
            ("x/dup.md", "x\n"),
            ("y/dup.md", "y\n"),
            ("z/in.md", "See [[a]].\n"),
        ])
    }

    #[test]
    fn broken_links_list_broken_and_ambiguous_links_with_their_details() {
        let (_d, db) = links_vault();
        let r = audit(db.conn(), None).unwrap();
        let rows: Vec<_> = r.broken_links.items.iter().map(|l| (l.path.as_str(), l.line, l.raw.as_str(), l.status, l.property.as_deref())).collect();
        assert_eq!(rows, vec![("a.md", 1, "[[gone]]", "broken", None), ("a.md", 1, "[[dup]]", "ambiguous", None), ("b.md", 2, "[[nowhere]]", "broken", Some("up"))]);
        assert_eq!(r.broken_links.items[1].candidates, vec!["x/dup.md", "y/dup.md"]);
    }

    #[test]
    fn orphans_and_notes_linking_nowhere() {
        let (_d, db) = links_vault();
        let r = audit(db.conn(), None).unwrap();
        let paths = |s: &Section<NotePath>| s.items.iter().map(|n| n.path.clone()).collect::<Vec<_>>();
        assert_eq!(paths(&r.orphans), vec!["c.md", "x/dup.md", "y/dup.md", "z/in.md"], "a self-link is not a backlink");
        assert_eq!(paths(&r.no_outgoing), vec!["b.md", "c.md", "x/dup.md", "y/dup.md"], "broken and self links lead nowhere");
    }

    #[test]
    fn path_prefix_limits_link_sections_but_backlinks_count_from_everywhere() {
        let (_d, db) = links_vault();
        let r = audit(db.conn(), Some("a")).unwrap();
        assert_eq!(r.broken_links.total, 2);
        assert!(r.orphans.items.is_empty(), "a.md is linked from z/in.md, outside the prefix");
        assert!(r.no_outgoing.items.is_empty());
    }

    #[test]
    fn a_link_to_an_excluded_note_counts_as_outgoing() {
        let d = tempdir().unwrap();
        let v = d.path().join("vault");
        for (p, c) in [("a.md", "See [[hidden]].\n"), ("ex/hidden.md", "x\n")] {
            let p = v.join(p);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, c).unwrap();
        }
        let mut db = Db::open(&d.path().join("vlt.db"), &IndexMeta { model_id: "fake".into(), dim: 384, fingerprint: "fake".into(), vault: "v".into() }).unwrap();
        let src = || -> crate::Result<Arc<dyn crate::ml::Embedder>> { Ok(Arc::new(FakeEmbedder)) };
        reindex(db.conn_mut(), &v, &build_exclude(&["ex/**".into()]).unwrap(), &src, &mut |_| {}).unwrap();
        let r = audit(db.conn(), None).unwrap();
        assert!(r.no_outgoing.items.is_empty(), "a.md links to an excluded note, which `get` reports ok");
        assert!(r.broken_links.items.is_empty());
    }
}
