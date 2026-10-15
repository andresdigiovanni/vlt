//! `get_document` and `list_properties` (PRD 7.1).

use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::path::{Component, Path, PathBuf};

use rusqlite::Connection;
use serde_json::{Map, Value};

use crate::error::{Result, VltError};
use crate::indexer::parser::{parse, LinkKind};
use crate::links::{Backlink, LinkIndex, Resolution};

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
    pub resolved: Option<String>,
    /// `ok`, `broken` or `ambiguous`.
    pub status: &'static str,
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
        "SELECT chunk_id, heading_path_json, start_line, end_line FROM chunks WHERE path = ?1 ORDER BY start_line, id",
    )?;
    let rows = stmt.query_map([path], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?, r.get::<_, i64>(3)?))
    })?;
    let mut sections = Vec::new();
    for row in rows {
        let (chunk_id, hp, start, end) = row?;
        let heading_path = serde_json::from_str(&hp).map_err(internal)?;
        sections.push(Section { chunk_id, heading_path, lines: [start as usize, end as usize] });
    }

    let index = LinkIndex::load(conn)?;
    let mut stmt =
        conn.prepare("SELECT target, heading, raw, line, kind FROM links WHERE src_path = ?1 ORDER BY line, rowid")?;
    let rows = stmt.query_map([path], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, Option<String>>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, i64>(3)?,
            r.get::<_, String>(4)?,
        ))
    })?;
    let mut links = Vec::new();
    for row in rows {
        let (target, heading, raw, line, kind) = row?;
        let kind = LinkKind::from_str(&kind).ok_or_else(|| internal(format!("link kind: {kind}")))?;
        let (resolved, status) = match index.resolve(path, kind, &target, heading.as_deref()) {
            Resolution::Ok(p) => (Some(p), "ok"),
            Resolution::Broken => (None, "broken"),
            Resolution::Ambiguous(_) => (None, "ambiguous"),
        };
        links.push(LinkOut { raw, line: line as usize, resolved, status });
    }
    let backlinks = index.backlinks(conn, path)?;

    Ok(DocumentResponse {
        path: path.to_string(),
        title: doc.title,
        properties: doc.properties.unwrap_or_default(),
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

pub fn list_properties(conn: &Connection, path_prefix: Option<&str>) -> Result<PropertiesResponse> {
    let mut stmt = conn.prepare("SELECT path, properties_json FROM files WHERE properties_json IS NOT NULL")?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
    let mut stats: HashMap<String, KeyStats> = HashMap::new();
    for row in rows {
        let (path, json) = row?;
        if path_prefix.is_some_and(|p| !path.starts_with(p)) {
            continue;
        }
        let props: Map<String, Value> = serde_json::from_str(&json).map_err(internal)?;
        for (key, value) in props {
            let s = stats.entry(key).or_default();
            s.documents += 1;
            if let Some(t) = value_type(&value) {
                s.types.insert(t);
            }
            match &value {
                Value::Array(items) => items.iter().for_each(|v| s.count(v)),
                v => s.count(v),
            }
        }
    }
    let mut properties: Vec<PropertyInfo> = stats
        .into_iter()
        .map(|(key, s)| {
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
            let top_values = values.into_iter().take(TOP_VALUES).map(|(_, vn)| vn).collect();
            PropertyInfo { key, kind, documents: s.documents, top_values }
        })
        .collect();
    properties.sort_by(|a, b| b.documents.cmp(&a.documents).then_with(|| a.key.cmp(&b.key)));
    Ok(PropertiesResponse { properties })
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
        let r = list_properties(db.conn(), None).unwrap();
        let get = |k: &str| r.properties.iter().find(|p| p.key == k).unwrap();
        let keys: Vec<_> = r.properties.iter().map(|p| p.key.as_str()).collect();
        assert_eq!(keys, vec!["tags", "status", "date", "empty", "title"]);
        assert_eq!((get("tags").kind.as_str(), get("tags").documents), ("mixed", 3), "list + scalar string");
        assert_eq!(get("tags").top_values, vec![(json!("rust"), 3), (json!("mcp"), 1)]);
        assert_eq!(get("status").kind, "mixed");
        assert_eq!(get("date").kind, "date");
        assert_eq!((get("empty").kind.as_str(), get("empty").documents, get("empty").top_values.len()), ("null", 1, 0));
        let scoped = list_properties(db.conn(), Some("p/")).unwrap();
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
    fn list_properties_value_rules() {
        let mut files: Vec<(String, String)> = (0..12).map(|i| (format!("n{i:02}.md"), format!("---\nk: v{i:02}\n---\n"))).collect();
        files[0].1 = "---\nk: [v00, zz]\nn:\nd1: 2026-09-30x\nd2: '2026-09-30T10:00'\ns: '3'\n---\n".into();
        files[1].1 = "---\nk: [v01, zz]\nn: hello\ns: 3\n---\n".into();
        let (_d, db) = index(&files);
        let r = list_properties(db.conn(), None).unwrap();
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
}
