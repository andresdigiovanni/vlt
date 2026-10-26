//! `get_document` and `list_properties` (PRD 7.1).

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fs;
use std::path::{Component, Path, PathBuf};

use rusqlite::Connection;
use serde_json::{Map, Value};

use crate::error::{Result, VltError};
use crate::indexer::parser::{parse, LinkKind};
use crate::links::{Backlink, LinkIndex, Resolution};
use crate::search::{filter::Filter, load_filtered_paths};

/// Resolves a vault-relative path; rejects absolute paths, `..` above the root and symlinks.
pub fn resolve_in_vault(vault: &Path, rel: &str) -> Result<PathBuf> {
    let outside = || VltError::PathOutsideVault(rel.to_string());
    let mut parts: Vec<&std::ffi::OsStr> = Vec::new();
    for c in Path::new(rel).components() {
        match c {
            Component::Normal(p) => parts.push(p),
            Component::CurDir => {}
            Component::ParentDir => {
                parts.pop().ok_or_else(outside)?;
            }
            Component::RootDir | Component::Prefix(_) => return Err(outside()),
        }
    }
    let mut path = vault.to_path_buf();
    let mut exists = true;
    for p in parts {
        path.push(p);
        if exists {
            match fs::symlink_metadata(&path) {
                Ok(m) if m.file_type().is_symlink() => return Err(outside()),
                Ok(_) => {}
                Err(_) => exists = false,
            }
        }
    }
    Ok(path)
}

#[derive(Debug, serde::Serialize)]
pub struct DocumentResponse {
    pub path: String,
    pub title: String,
    pub properties: Map<String, Value>,
    pub tags: Vec<String>,
    pub content: String,
    pub sections: Vec<Section>,
    pub links: Vec<LinkOut>,
    pub backlinks: Vec<Backlink>,
}

#[derive(Debug, serde::Serialize)]
pub struct Section {
    pub chunk_id: String,
    pub heading_path: Vec<String>,
    pub lines: [usize; 2],
}

#[derive(Debug, serde::Serialize)]
pub struct LinkOut {
    pub raw: String,
    pub line: usize,
    /// The frontmatter key the link is in; omitted for a link in the body.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub property: Option<String>,
    pub resolved: Option<String>,
    /// `ok`, `broken` or `ambiguous`.
    pub status: &'static str,
    /// The paths an `ambiguous` link could mean; empty, and omitted, otherwise.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub candidates: Vec<String>,
}

fn internal(e: impl std::fmt::Display) -> VltError {
    VltError::Internal(e.to_string())
}

pub fn get_document(conn: &Connection, vault: &Path, path: &str) -> Result<DocumentResponse> {
    let abs = resolve_in_vault(vault, path)?;
    let not_found = || VltError::DocumentNotFound(path.to_string());
    let is_md = Path::new(path).extension().is_some_and(|e| e.eq_ignore_ascii_case("md"));
    if !is_md {
        return Err(not_found());
    }
    let indexed: i64 = conn.query_row("SELECT COUNT(*) FROM files WHERE path = ?1", [path], |r| r.get(0))?;
    if indexed == 0 {
        return Err(not_found());
    }
    let text = fs::read_to_string(&abs).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => not_found(),
        _ => e.into(),
    })?;
    let stem = abs.file_stem().and_then(|s| s.to_str()).unwrap_or(path);
    let doc = parse(&text, stem);

    let mut stmt = conn.prepare(
        "SELECT section_id, heading_path_json, MIN(start_line), MAX(end_line) FROM chunks WHERE path = ?1
         GROUP BY section_id ORDER BY MIN(start_line), MIN(id)",
    )?;
    let rows = stmt.query_map([path], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?, r.get::<_, i64>(3)?))
    })?;
    let mut sections = Vec::new();
    for row in rows {
        let (section_id, hp, start, end) = row?;
        let heading_path = serde_json::from_str(&hp).map_err(internal)?;
        sections.push(Section { chunk_id: section_id, heading_path, lines: [start as usize, end as usize] });
    }

    let index = LinkIndex::load(conn)?;
    let mut stmt =
        conn.prepare("SELECT target, heading, raw, line, kind, property FROM links WHERE src_path = ?1 ORDER BY line, rowid")?;
    let rows = stmt.query_map([path], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, Option<String>>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, i64>(3)?,
            r.get::<_, String>(4)?,
            r.get::<_, Option<String>>(5)?,
        ))
    })?;
    let mut links = Vec::new();
    for row in rows {
        let (target, heading, raw, line, kind, property) = row?;
        let kind = LinkKind::from_str(&kind).ok_or_else(|| internal(format!("link kind: {kind}")))?;
        let (resolved, status, candidates) = match index.resolve(path, kind, &target, heading.as_deref()) {
            Resolution::Ok(p) => (Some(p), "ok", Vec::new()),
            Resolution::Broken => (None, "broken", Vec::new()),
            Resolution::Ambiguous(c) => (None, "ambiguous", c),
        };
        links.push(LinkOut { raw, line: line as usize, property, resolved, status, candidates });
    }
    let backlinks = index.backlinks(conn, path)?;

    Ok(DocumentResponse {
        path: path.to_string(),
        title: doc.title,
        properties: doc.properties.unwrap_or_default(),
        tags: doc.tags,
        content: doc.body,
        sections,
        links,
        backlinks,
    })
}

#[derive(Debug, serde::Serialize)]
pub struct PropertiesResponse {
    pub properties: Vec<PropertyInfo>,
}

#[derive(Debug, serde::Serialize)]
pub struct PropertyInfo {
    pub key: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub documents: usize,
    pub top_values: Vec<(Value, usize)>,
    /// Spellings of one value that differ only in case or separators, most used first; omitted when none.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub variants: Vec<Vec<(Value, usize)>>,
}

const TOP_VALUES: usize = 10;

fn is_iso_date(s: &str) -> bool {
    let b = s.as_bytes();
    let digits = |r: std::ops::Range<usize>| b[r].iter().all(u8::is_ascii_digit);
    b.len() >= 10
        && digits(0..4)
        && b[4] == b'-'
        && digits(5..7)
        && b[7] == b'-'
        && digits(8..10)
        && (b.len() == 10 || b[10] == b'T')
}

fn value_type(v: &Value) -> Option<&'static str> {
    match v {
        Value::Null => None,
        Value::String(s) if is_iso_date(s) => Some("date"),
        Value::String(_) => Some("string"),
        Value::Number(_) => Some("number"),
        Value::Bool(_) => Some("bool"),
        Value::Array(_) => Some("list"),
        Value::Object(_) => Some("object"),
    }
}

fn string_form(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

#[derive(Default)]
struct KeyStats {
    documents: usize,
    types: BTreeSet<&'static str>,
    /// Keyed by the JSON encoding, so `"3"` and `3` stay distinct.
    counts: HashMap<String, (Value, usize)>,
}

impl KeyStats {
    fn count(&mut self, v: &Value) {
        if matches!(v, Value::String(_) | Value::Number(_) | Value::Bool(_)) {
            self.counts.entry(v.to_string()).or_insert_with(|| (v.clone(), 0)).1 += 1;
        }
    }
}

/// Lowercased, each run of `-`, `_` and whitespace as one `-`: spellings with the same form are variants.
pub(crate) fn variant_form(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_separator = false;
    for c in s.chars() {
        if c == '-' || c == '_' || c.is_whitespace() {
            if !in_separator {
                out.push('-');
            }
            in_separator = true;
        } else {
            out.extend(c.to_lowercase());
            in_separator = false;
        }
    }
    out
}

/// Groups of two or more string spellings that share a variant form, each in `values`' order
/// (that of `top_values`); groups by summed count desc, then first spelling asc.
fn variant_groups(values: &[(String, (Value, usize))]) -> Vec<Vec<(Value, usize)>> {
    let mut groups: Vec<Vec<(Value, usize)>> = Vec::new();
    let mut by_form: HashMap<String, usize> = HashMap::new();
    for (_, (v, n)) in values {
        let Value::String(s) = v else { continue };
        let i = *by_form.entry(variant_form(s)).or_insert_with(|| {
            groups.push(Vec::new());
            groups.len() - 1
        });
        groups[i].push((v.clone(), *n));
    }
    groups.retain(|g| g.len() > 1);
    let total = |g: &[(Value, usize)]| g.iter().map(|(_, n)| n).sum::<usize>();
    groups.sort_by(|a, b| total(b).cmp(&total(a)).then_with(|| string_form(&a[0].0).cmp(&string_form(&b[0].0))));
    groups
}

pub fn list_properties(conn: &Connection, where_: Option<&Value>, path_prefix: Option<&str>, key: Option<&str>) -> Result<PropertiesResponse> {
    // `where` and `path_prefix` select the documents counted, as in `search`.
    let filter = Filter::new(where_, path_prefix)?;
    let selected: Option<HashSet<String>> =
        if filter.is_empty() { None } else { Some(load_filtered_paths(conn, &filter)?.into_iter().collect()) };
    let mut stmt = conn.prepare("SELECT path, properties_json, tags_json FROM files")?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?, r.get::<_, String>(2)?)))?;
    let mut stats: HashMap<String, KeyStats> = HashMap::new();
    for row in rows {
        let (path, json, tags_json) = row?;
        if selected.as_ref().is_some_and(|s| !s.contains(&path)) {
            continue;
        }
        let props: Map<String, Value> = match json {
            Some(json) => serde_json::from_str(&json).map_err(internal)?,
            None => Map::new(),
        };
        for (k, value) in props {
            if k == "tags" || key.is_some_and(|key| key != k) {
                continue;
            }
            let s = stats.entry(k).or_default();
            s.documents += 1;
            if let Some(t) = value_type(&value) {
                s.types.insert(t);
            }
            match &value {
                Value::Array(items) => items.iter().for_each(|v| s.count(v)),
                v => s.count(v),
            }
        }
        // `tags` counts the effective tags: frontmatter and body `#tags`, once per document.
        let tags: Vec<Value> = serde_json::from_str(&tags_json).map_err(internal)?;
        if !tags.is_empty() && key.is_none_or(|key| key == "tags") {
            let s = stats.entry("tags".to_string()).or_default();
            s.documents += 1;
            s.types.insert("list");
            tags.iter().for_each(|t| s.count(t));
        }
    }
    let mut properties: Vec<PropertyInfo> = stats
        .into_iter()
        .map(|(k, s)| {
            let kind = match s.types.len() {
                0 => "null".to_string(),
                1 => s.types.iter().next().copied().unwrap_or_default().to_string(),
                _ => "mixed".to_string(),
            };
            // Count desc, then string form asc, then JSON encoding (the map key) asc.
            let mut values: Vec<(String, (Value, usize))> = s.counts.into_iter().collect();
            values.sort_by(|(ja, (a, na)), (jb, (b, nb))| {
                nb.cmp(na).then_with(|| string_form(a).cmp(&string_form(b))).then_with(|| ja.cmp(jb))
            });
            let variants = variant_groups(&values);
            // One key asked for by name gets every value.
            let top = if key.is_some() { values.len() } else { TOP_VALUES };
            let top_values = values.into_iter().take(top).map(|(_, vn)| vn).collect();
            PropertyInfo { key: k, kind, documents: s.documents, top_values, variants }
        })
        .collect();
    properties.sort_by(|a, b| b.documents.cmp(&a.documents).then_with(|| a.key.cmp(&b.key)));
    Ok(PropertiesResponse { properties })
}

#[derive(Debug, serde::Serialize)]
pub struct FoldersResponse {
    pub folders: Vec<FolderInfo>,
}

#[derive(Debug, serde::Serialize)]
pub struct FolderInfo {
    /// `""` for the vault root, else the folder with a trailing `/`, as `path_prefix` takes it.
    pub path: String,
    /// Notes directly in the folder.
    pub documents: usize,
    /// Notes in the folder and every folder below it.
    pub total: usize,
    /// The most used keys among the folder's own notes, as `[key, documents]`.
    pub top_keys: Vec<(String, usize)>,
}

const TOP_KEYS: usize = 5;

/// The folders `path` is in, outermost first: `""`, then each directory with a trailing `/`.
fn ancestors(path: &str) -> Vec<String> {
    let mut out = vec![String::new()];
    let mut end = 0;
    while let Some(i) = path[end..].find('/') {
        end += i + 1;
        out.push(path[..end].to_string());
    }
    out
}

pub fn list_folders(conn: &Connection, path_prefix: Option<&str>) -> Result<FoldersResponse> {
    #[derive(Default)]
    struct Folder {
        documents: usize,
        total: usize,
        keys: HashMap<String, usize>,
    }
    let mut stmt = conn.prepare("SELECT path, properties_json, tags_json FROM files")?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?, r.get::<_, String>(2)?)))?;
    let mut folders: BTreeMap<String, Folder> = BTreeMap::new();
    for row in rows {
        let (path, json, tags_json) = row?;
        let dirs = ancestors(&path);
        for dir in &dirs {
            folders.entry(dir.clone()).or_default().total += 1;
        }
        let own = folders.entry(dirs.last().cloned().unwrap_or_default()).or_default();
        own.documents += 1;
        let props: Map<String, Value> = match json {
            Some(json) => serde_json::from_str(&json).map_err(internal)?,
            None => Map::new(),
        };
        for k in props.keys().filter(|k| *k != "tags") {
            *own.keys.entry(k.clone()).or_default() += 1;
        }
        // `tags` counts a note with at least one effective tag, as `properties` does.
        let tags: Vec<Value> = serde_json::from_str(&tags_json).map_err(internal)?;
        if !tags.is_empty() {
            *own.keys.entry("tags".to_string()).or_default() += 1;
        }
    }
    let folders = folders
        .into_iter()
        .filter(|(path, _)| path_prefix.is_none_or(|p| path.starts_with(p)))
        .map(|(path, f)| {
            let mut top_keys: Vec<(String, usize)> = f.keys.into_iter().collect();
            top_keys.sort_by(|(ka, na), (kb, nb)| nb.cmp(na).then_with(|| ka.cmp(kb)));
            top_keys.truncate(TOP_KEYS);
            FolderInfo { path, documents: f.documents, total: f.total, top_keys }
        })
        .collect();
    Ok(FoldersResponse { folders })
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

    fn setup() -> (tempfile::TempDir, PathBuf, Db) {
        let d = tempdir().unwrap();
        let v = d.path().join("vault");
        let files = [
            ("p/a.md", "---\ntitle: Alpha\ntags: [rust, mcp]\nstatus: done\ndate: 2026-09-30\n---\n# A\n\nIntro [[b]] [[gone]].\n## Part\nMore.\n"),
            ("b.md", "---\ntags: rust\nstatus: 3\n---\nLinks to [[a#Part]].\n"),
            ("c.md", "---\ntags: [rust]\nempty:\n---\nNo links.\n"),
            ("ex/x.md", "x\n"),
        ];
        for (p, c) in files { let p = v.join(p); fs::create_dir_all(p.parent().unwrap()).unwrap(); fs::write(p, c).unwrap(); }
        let mut db = Db::open(&d.path().join("vlt.db"), &IndexMeta { model_id: "fake".into(), dim: 384, fingerprint: "fake".into(), vault: "v".into() }).unwrap();
        let src = || -> crate::Result<Arc<dyn crate::ml::Embedder>> { Ok(Arc::new(FakeEmbedder)) };
        reindex(db.conn_mut(), &v, &build_exclude(&["ex/**".into()]).unwrap(), &src, &mut |_, _| {}).unwrap();
        (d, v, db)
    }

    #[test]
    fn get_document_payload() {
        let (_d, v, db) = setup();
        let doc = get_document(db.conn(), &v, "p/a.md").unwrap();
        assert_eq!(doc.title, "Alpha");
        assert_eq!(doc.properties["tags"], json!(["rust", "mcp"]));
        assert_eq!(doc.content, "# A\n\nIntro [[b]] [[gone]].\n## Part\nMore.\n");
        let secs: Vec<_> = doc.sections.iter().map(|s| (s.chunk_id.as_str(), s.lines)).collect();
        assert_eq!(secs, vec![("p/a.md#A", [7, 9]), ("p/a.md#Part", [10, 11])]);
        let links: Vec<_> = doc.links.iter().map(|l| (l.raw.as_str(), l.line, l.resolved.as_deref(), l.status)).collect();
        assert_eq!(links, vec![("[[b]]", 9, Some("b.md"), "ok"), ("[[gone]]", 9, None, "broken")]);
        assert_eq!(doc.backlinks, vec![crate::links::Backlink { path: "b.md".into(), line: 5, raw: "[[a#Part]]".into() }]);
    }

    #[test]
    fn an_ambiguous_link_lists_its_candidates_and_other_links_do_not() {
        let d = tempdir().unwrap();
        let v = d.path().join("vault");
        for (p, c) in [("a/x.md", "a\n"), ("b/x.md", "b\n"), ("src.md", "[[x]] and [[a/x]].\n")] {
            let p = v.join(p); fs::create_dir_all(p.parent().unwrap()).unwrap(); fs::write(p, c).unwrap();
        }
        let mut db = Db::open(&d.path().join("vlt.db"), &IndexMeta { model_id: "fake".into(), dim: 384, fingerprint: "fake".into(), vault: "v".into() }).unwrap();
        let src = || -> crate::Result<Arc<dyn crate::ml::Embedder>> { Ok(Arc::new(FakeEmbedder)) };
        reindex(db.conn_mut(), &v, &build_exclude(&[]).unwrap(), &src, &mut |_, _| {}).unwrap();
        let links = serde_json::to_value(get_document(db.conn(), &v, "src.md").unwrap()).unwrap()["links"].clone();
        assert_eq!(links[0], json!({"raw": "[[x]]", "line": 1, "resolved": null, "status": "ambiguous", "candidates": ["a/x.md", "b/x.md"]}));
        assert_eq!(links[1], json!({"raw": "[[a/x]]", "line": 1, "resolved": "a/x.md", "status": "ok"}));
    }

    #[test]
    fn a_section_split_into_chunks_is_listed_once_with_its_whole_span() {
        let d = tempdir().unwrap();
        let v = d.path().join("vault");
        fs::create_dir_all(&v).unwrap();
        let para = (0..200).map(|i| format!("w{i}")).collect::<Vec<_>>().join(" ");
        fs::write(v.join("long.md"), format!("# Long\n{para}\n\n{para}\n\n{para}\n## Next\nshort\n")).unwrap();
        let mut db = Db::open(&d.path().join("vlt.db"), &IndexMeta { model_id: "fake".into(), dim: 384, fingerprint: "fake".into(), vault: "v".into() }).unwrap();
        let src = || -> crate::Result<Arc<dyn crate::ml::Embedder>> { Ok(Arc::new(FakeEmbedder)) };
        reindex(db.conn_mut(), &v, &build_exclude(&[]).unwrap(), &src, &mut |_, _| {}).unwrap();
        let doc = get_document(db.conn(), &v, "long.md").unwrap();
        let secs: Vec<_> = doc.sections.iter().map(|s| (s.chunk_id.as_str(), s.lines)).collect();
        assert_eq!(secs, vec![("long.md#Long", [1, 6]), ("long.md#Next", [7, 8])]);
    }

    #[test]
    fn get_document_errors() {
        let (_d, v, db) = setup();
        assert!(matches!(get_document(db.conn(), &v, "missing.md"), Err(VltError::DocumentNotFound(_))));
        assert!(matches!(get_document(db.conn(), &v, "p/a"), Err(VltError::DocumentNotFound(_))));
        assert!(matches!(get_document(db.conn(), &v, "ex/x.md"), Err(VltError::DocumentNotFound(_))), "excluded");
        assert!(matches!(get_document(db.conn(), &v, "../etc/passwd.md"), Err(VltError::PathOutsideVault(_))));
        assert!(matches!(get_document(db.conn(), &v, "/etc/passwd.md"), Err(VltError::PathOutsideVault(_))));
        std::os::unix::fs::symlink(v.join("b.md"), v.join("s.md")).unwrap();
        assert!(matches!(resolve_in_vault(&v, "s.md"), Err(VltError::PathOutsideVault(_))));
        assert_eq!(resolve_in_vault(&v, "p/../b.md").unwrap(), v.join("b.md"));
    }

    #[test]
    fn indexed_file_deleted_from_disk_is_not_found() {
        let (_d, v, db) = setup();
        fs::remove_file(v.join("c.md")).unwrap();
        assert!(matches!(get_document(db.conn(), &v, "c.md"), Err(VltError::DocumentNotFound(p)) if p == "c.md"));
    }

    #[test]
    fn list_properties_inventory() {
        let (_d, _v, db) = setup();
        let r = list_properties(db.conn(), None, None, None).unwrap();
        let get = |k: &str| r.properties.iter().find(|p| p.key == k).unwrap();
        let keys: Vec<_> = r.properties.iter().map(|p| p.key.as_str()).collect();
        assert_eq!(keys, vec!["tags", "status", "date", "empty", "title"]);
        assert_eq!((get("tags").kind.as_str(), get("tags").documents), ("list", 3), "a string value is read as a list of tags");
        assert_eq!(get("tags").top_values, vec![(json!("rust"), 3), (json!("mcp"), 1)]);
        assert_eq!(get("status").kind, "mixed");
        assert_eq!(get("date").kind, "date");
        assert_eq!((get("empty").kind.as_str(), get("empty").documents, get("empty").top_values.len()), ("null", 1, 0));
        let scoped = list_properties(db.conn(), None, Some("p/"), None).unwrap();
        assert_eq!(scoped.properties.iter().find(|p| p.key == "tags").unwrap().kind, "list");
    }

    fn index(files: &[(String, String)]) -> (tempfile::TempDir, Db) {
        let d = tempdir().unwrap();
        let v = d.path().join("vault");
        for (p, c) in files { let p = v.join(p); fs::create_dir_all(p.parent().unwrap()).unwrap(); fs::write(p, c).unwrap(); }
        let mut db = Db::open(&d.path().join("vlt.db"), &IndexMeta { model_id: "fake".into(), dim: 384, fingerprint: "fake".into(), vault: "v".into() }).unwrap();
        let src = || -> crate::Result<Arc<dyn crate::ml::Embedder>> { Ok(Arc::new(FakeEmbedder)) };
        reindex(db.conn_mut(), &v, &build_exclude(&[]).unwrap(), &src, &mut |_, _| {}).unwrap();
        (d, db)
    }

    #[test]
    fn list_properties_counts_body_tags_once_per_document() {
        let (_d, db) = index(&[
            ("a.md".into(), "---\ntags: [rust]\n---\nAlso #rust and #Search.\n".into()),
            ("b.md".into(), "No frontmatter, #search here.\n".into()),
            ("c.md".into(), "---\nstatus: x\n---\nNo tags.\n".into()),
        ]);
        let r = list_properties(db.conn(), None, None, Some("tags")).unwrap();
        let t = &r.properties[0];
        assert_eq!((t.key.as_str(), t.kind.as_str(), t.documents), ("tags", "list", 2));
        assert_eq!(t.top_values, vec![(json!("Search"), 1), (json!("rust"), 1), (json!("search"), 1)], "spellings stay distinct across notes");
    }

    #[test]
    fn get_lists_frontmatter_and_body_tags_and_keeps_properties_as_written() {
        let (d, db) = index(&[("a.md".into(), "---\ntags: rust, mcp\n---\nSee #search.\n".into())]);
        let doc = serde_json::to_value(get_document(db.conn(), &d.path().join("vault"), "a.md").unwrap()).unwrap();
        assert_eq!(doc["tags"], json!(["rust", "mcp", "search"]));
        assert_eq!(doc["properties"]["tags"], json!("rust, mcp"));
    }

    #[test]
    fn list_properties_value_rules() {
        let mut files: Vec<(String, String)> = (0..12).map(|i| (format!("n{i:02}.md"), format!("---\nk: v{i:02}\n---\n"))).collect();
        files[0].1 = "---\nk: [v00, zz]\nn:\nd1: 2026-09-30x\nd2: '2026-09-30T10:00'\ns: '3'\n---\n".into();
        files[1].1 = "---\nk: [v01, zz]\nn: hello\ns: 3\n---\n".into();
        let (_d, db) = index(&files);
        let r = list_properties(db.conn(), None, None, None).unwrap();
        let get = |k: &str| r.properties.iter().find(|p| p.key == k).unwrap();
        let mut expected = vec![(json!("zz"), 2)];
        expected.extend((0..9).map(|i| (json!(format!("v{i:02}")), 1)));
        assert_eq!(get("k").top_values, expected, "count desc, then value asc, at most 10");
        assert_eq!((get("n").kind.as_str(), get("n").documents), ("string", 2), "null is ignored for type");
        assert_eq!(get("d1").kind, "string");
        assert_eq!(get("d2").kind, "date");
        assert_eq!(get("s").kind, "mixed");
        let s_values = &get("s").top_values;
        assert_eq!(*s_values, vec![(json!("3"), 1), (json!(3), 1)], "\"3\" and 3 are distinct; ties on string form order by JSON encoding");
    }

    #[test]
    fn variants_group_spellings_that_differ_in_case_or_separators() {
        let (_d, db) = index(&[
            ("a.md".into(), "---\ntopic: [Machine Learning, rust]\nother: x\n---\n".into()),
            ("b.md".into(), "---\ntopic: [machine-learning, Rust]\n---\n".into()),
            ("c.md".into(), "---\ntopic: [machine_learning, project/vlt]\n---\n".into()),
            ("d.md".into(), "---\ntopic: [machine-learning, project-vlt, '3']\n---\n".into()),
            ("e.md".into(), "---\ntopic: [rust, 3]\n---\n".into()),
        ]);
        let r = list_properties(db.conn(), None, None, None).unwrap();
        let get = |k: &str| r.properties.iter().find(|p| p.key == k).unwrap();
        assert_eq!(get("topic").variants, vec![
            vec![(json!("machine-learning"), 2), (json!("Machine Learning"), 1), (json!("machine_learning"), 1)],
            vec![(json!("rust"), 2), (json!("Rust"), 1)],
        ], "groups by summed count; `/` is kept; numbers never form variants");
        assert!(serde_json::to_value(get("other")).unwrap().get("variants").is_none(), "omitted when a key has none");
    }

    #[test]
    fn variants_cover_every_value_and_tags_use_the_effective_tags() {
        let mut files: Vec<(String, String)> = (0..11)
            .flat_map(|i| [(format!("n{i:02}.md"), format!("---\nk: v{i:02}\n---\n")), (format!("m{i:02}.md"), format!("---\nk: v{i:02}\n---\n"))])
            .collect();
        files.push(("a.md".into(), "---\nk: Odd_One\n---\n".into()));
        files.push(("b.md".into(), "---\nk: odd one\ntags: [ml_ops]\n---\nSee #ML-ops.\n".into()));
        let (_d, db) = index(&files);
        let r = list_properties(db.conn(), None, None, None).unwrap();
        let get = |k: &str| r.properties.iter().find(|p| p.key == k).unwrap();
        assert_eq!(get("k").top_values.len(), 10);
        assert_eq!(get("k").variants, vec![vec![(json!("Odd_One"), 1), (json!("odd one"), 1)]], "found beyond the top 10");
        assert_eq!(get("tags").variants, vec![vec![(json!("ML-ops"), 1), (json!("ml_ops"), 1)]]);
    }

    #[test]
    fn variants_collapse_runs_of_separators() {
        let (_d, db) = index(&[
            ("a.md".into(), "---\nk: [b-x]\n---\n".into()),
            ("b.md".into(), "---\nk: ['B  _x']\n---\n".into()),
        ]);
        let r = list_properties(db.conn(), None, None, None).unwrap();
        assert_eq!(r.properties[0].variants, vec![vec![(json!("B  _x"), 1), (json!("b-x"), 1)]]);
    }

    #[test]
    fn variant_groups_with_the_same_total_order_by_first_spelling() {
        let (_d, db) = index(&[
            ("a.md".into(), "---\nk: [z, A]\n---\n".into()),
            ("b.md".into(), "---\nk: [z, A]\n---\n".into()),
            ("c.md".into(), "---\nk: [z, a]\n---\n".into()),
            ("d.md".into(), "---\nk: [Z, a]\n---\n".into()),
        ]);
        let r = list_properties(db.conn(), None, None, None).unwrap();
        assert_eq!(r.properties[0].variants, vec![
            vec![(json!("A"), 2), (json!("a"), 2)],
            vec![(json!("z"), 3), (json!("Z"), 1)],
        ], "both total 4: `A` sorts before `z` although `z` is the most used value");
    }

    #[test]
    fn list_properties_where_on_a_list_key_selects_notes_containing_the_value() {
        let (_d, db) = index(&[
            ("a.md".into(), "---\nauthors: [ana, bo]\nx: 1\n---\n".into()),
            ("b.md".into(), "---\nauthors: [bo]\ny: 1\n---\n".into()),
        ]);
        let keys: Vec<_> = list_properties(db.conn(), Some(&json!({"authors": "ana"})), None, None)
            .unwrap().properties.into_iter().map(|p| p.key).collect();
        assert_eq!(keys, vec!["authors", "x"]);
    }

    #[test]
    fn list_properties_for_one_key_returns_only_it_with_every_value() {
        let files: Vec<(String, String)> = (0..12).map(|i| (format!("d/n{i:02}.md"), format!("---\nk: v{i:02}\nother: x\n---\n"))).collect();
        let (_d, db) = index(&files);
        let r = list_properties(db.conn(), None, None, Some("k")).unwrap();
        assert_eq!(r.properties.iter().map(|p| p.key.as_str()).collect::<Vec<_>>(), vec!["k"]);
        assert_eq!(r.properties[0].top_values, (0..12).map(|i| (json!(format!("v{i:02}")), 1)).collect::<Vec<_>>());
        assert!(list_properties(db.conn(), None, None, Some("missing")).unwrap().properties.is_empty());
        assert!(list_properties(db.conn(), None, Some("elsewhere/"), Some("k")).unwrap().properties.is_empty(), "the prefix still applies");
    }

    #[test]
    fn list_properties_counts_only_the_documents_a_where_selects() {
        let (_d, db) = index(&[
            ("p/a.md".into(), "---\ntype: project\nstatus: done\nn: 3\n---\nUses #rust.\n".into()),
            ("p/b.md".into(), "---\ntype: project\nstatus: open\nn: 7\n---\nx\n".into()),
            ("q/c.md".into(), "---\ntype: project\nowner: me\n---\nx\n".into()),
            ("q/d.md".into(), "---\ntype: note\nowner: you\n---\nx\n".into()),
        ]);
        let keys = |w: Value, prefix: Option<&str>, key: Option<&str>| -> Vec<(String, usize)> {
            list_properties(db.conn(), Some(&w), prefix, key).unwrap().properties.into_iter().map(|p| (p.key, p.documents)).collect()
        };
        let s = |k: &str, n: usize| (k.to_string(), n);
        assert_eq!(keys(json!({"type": "project"}), None, None), vec![s("type", 3), s("n", 2), s("status", 2), s("owner", 1), s("tags", 1)]);
        assert_eq!(keys(json!({"tags": "rust"}), None, None), vec![s("n", 1), s("status", 1), s("tags", 1), s("type", 1)], "a body tag selects");
        assert_eq!(keys(json!({"n": {"gte": 5}}), None, None), vec![s("n", 1), s("status", 1), s("type", 1)]);
        assert_eq!(keys(json!({"type": "project"}), Some("q/"), None), vec![s("owner", 1), s("type", 1)], "where AND path_prefix");
        assert_eq!(keys(json!({"type": "project"}), None, Some("status")), vec![s("status", 2)]);
        assert!(keys(json!({"missing": "x"}), None, None).is_empty(), "a key no document has selects nothing");
    }

    #[test]
    fn list_properties_rejects_a_where_that_is_not_an_object() {
        let (_d, db) = index(&[("a.md".into(), "x\n".into())]);
        assert!(matches!(list_properties(db.conn(), Some(&json!(["a"])), None, None), Err(VltError::InvalidArguments(_))));
    }

    #[test]
    fn symlinked_directory_is_outside_vault() {
        let (_d, v, _db) = setup();
        std::os::unix::fs::symlink(v.join("p"), v.join("q")).unwrap();
        assert!(matches!(resolve_in_vault(&v, "q/a.md"), Err(VltError::PathOutsideVault(_))));
        assert_eq!(resolve_in_vault(&v, "new/dir/x.md").unwrap(), v.join("new/dir/x.md"), "missing paths are fine");
    }

    #[test]
    fn uppercase_md_extension_is_a_document() {
        let (_d, db) = index(&[("sub/B.MD".into(), "Body.\n".into())]);
        let v = _d.path().join("vault");
        let doc = get_document(db.conn(), &v, "sub/B.MD").unwrap();
        assert_eq!((doc.path.as_str(), doc.title.as_str()), ("sub/B.MD", "B"), "same path and title as the indexer");
    }

    #[test]
    fn a_frontmatter_link_has_its_property_and_counts_as_a_backlink() {
        let (d, db) = index(&[
            ("child.md".into(), "---\nup: \"[[parent]]\"\n---\nBody.\n".into()),
            ("parent.md".into(), "# Parent\n".into()),
        ]);
        let v = d.path().join("vault");
        let links = serde_json::to_value(get_document(db.conn(), &v, "child.md").unwrap()).unwrap()["links"].clone();
        assert_eq!(links, json!([{"raw": "[[parent]]", "line": 2, "property": "up", "resolved": "parent.md", "status": "ok"}]));
        assert_eq!(get_document(db.conn(), &v, "parent.md").unwrap().backlinks,
                   vec![crate::links::Backlink { path: "child.md".into(), line: 2, raw: "[[parent]]".into() }]);
    }

    #[test]
    fn list_folders_counts_direct_and_nested_notes_with_their_top_keys() {
        let d = tempdir().unwrap();
        let v = d.path().join("vault");
        let files = [
            ("root.md", "---\ntitle: R\n---\n"),
            ("a/x.md", "---\nstatus: done\ntype: p\n---\nSee #rust.\n"),
            ("a/y.md", "---\nstatus: open\nk1: 1\nk2: 1\nk3: 1\nk4: 1\n---\n"),
            ("a/b/c/z.md", "Note #idea.\n"),
            ("ex/skip.md", "---\nstatus: x\n---\n"),
        ];
        for (p, c) in files { let p = v.join(p); fs::create_dir_all(p.parent().unwrap()).unwrap(); fs::write(p, c).unwrap(); }
        let mut db = Db::open(&d.path().join("vlt.db"), &IndexMeta { model_id: "fake".into(), dim: 384, fingerprint: "fake".into(), vault: "v".into() }).unwrap();
        let src = || -> crate::Result<Arc<dyn crate::ml::Embedder>> { Ok(Arc::new(FakeEmbedder)) };
        reindex(db.conn_mut(), &v, &build_exclude(&["ex/**".into()]).unwrap(), &src, &mut |_, _| {}).unwrap();

        let r = list_folders(db.conn(), None).unwrap();
        let rows: Vec<_> = r.folders.iter().map(|f| (f.path.as_str(), f.documents, f.total)).collect();
        assert_eq!(rows, vec![("", 1, 4), ("a/", 2, 3), ("a/b/", 0, 1), ("a/b/c/", 1, 1)], "root is \"\"; excluded files make no folder");
        let k = |s: &str, n: usize| (s.to_string(), n);
        assert_eq!(r.folders[0].top_keys, vec![k("title", 1)]);
        assert_eq!(r.folders[1].top_keys, vec![k("status", 2), k("k1", 1), k("k2", 1), k("k3", 1), k("k4", 1)], "5 at most, documents desc then key asc");
        assert!(r.folders[2].top_keys.is_empty(), "a folder with only subfolders");
        assert_eq!(r.folders[3].top_keys, vec![k("tags", 1)], "a body tag counts as `tags`");
        let scoped: Vec<_> = list_folders(db.conn(), Some("a/b/")).unwrap().folders.into_iter().map(|f| f.path).collect();
        assert_eq!(scoped, vec!["a/b/", "a/b/c/"]);
    }
}
