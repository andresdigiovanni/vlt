//! Link resolution (PRD 5.2) and backlinks (PRD 5.3).

use std::collections::{HashMap, HashSet};

use rusqlite::Connection;
use unicode_normalization::UnicodeNormalization;

use crate::error::{Result, VltError};
use crate::indexer::parser::LinkKind;

#[derive(Debug, Clone, PartialEq)]
pub enum Resolution {
    Ok(String),
    Broken,
    Ambiguous(Vec<String>),
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Backlink {
    pub path: String,
    pub line: usize,
    pub raw: String,
}

struct Candidate {
    path: String,
    /// `normalize(path)` with `.md` removed.
    key: String,
}

/// All indexed and excluded paths, with the headings of the indexed ones.
pub struct LinkIndex {
    /// Sorted by path.
    candidates: Vec<Candidate>,
    /// Last component of `key` → candidate indices, ascending.
    by_name: HashMap<String, Vec<usize>>,
    /// `normalize(path)`, `.md` included → candidate indices, ascending.
    by_full: HashMap<String, Vec<usize>>,
    excluded: HashSet<String>,
    /// Normalized heading paths per indexed path.
    headings: HashMap<String, Vec<Vec<String>>>,
}

/// NFC + lowercase: the comparison form of paths, targets and headings.
pub fn normalize(s: &str) -> String {
    s.nfc().collect::<String>().to_lowercase()
}

fn strip_md(s: &str) -> &str {
    s.strip_suffix(".md").unwrap_or(s)
}

fn last_component(s: &str) -> &str {
    s.rsplit('/').next().unwrap_or(s)
}

impl LinkIndex {
    pub fn load(conn: &Connection) -> Result<LinkIndex> {
        let mut paths: Vec<String> = conn
            .prepare("SELECT path FROM files")?
            .query_map([], |r| r.get(0))?
            .collect::<std::result::Result<_, _>>()?;
        let excluded: HashSet<String> = conn
            .prepare("SELECT path FROM excluded_files")?
            .query_map([], |r| r.get(0))?
            .collect::<std::result::Result<_, _>>()?;
        paths.extend(excluded.iter().cloned());
        paths.sort();
        paths.dedup();
        let mut candidates = Vec::with_capacity(paths.len());
        let mut by_name: HashMap<String, Vec<usize>> = HashMap::new();
        let mut by_full: HashMap<String, Vec<usize>> = HashMap::new();
        for (i, path) in paths.into_iter().enumerate() {
            let full = normalize(&path);
            let key = strip_md(&full).to_string();
            by_name.entry(last_component(&key).to_string()).or_default().push(i);
            by_full.entry(full).or_default().push(i);
            candidates.push(Candidate { path, key });
        }

        let mut headings: HashMap<String, Vec<Vec<String>>> = HashMap::new();
        let mut stmt = conn.prepare("SELECT path, heading_path_json FROM headings")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        for row in rows {
            let (path, json) = row?;
            let hp: Vec<String> =
                serde_json::from_str(&json).map_err(|e| VltError::Internal(format!("heading_path_json: {e}")))?;
            headings.entry(path).or_default().push(hp.iter().map(|h| normalize(h)).collect());
        }
        Ok(LinkIndex { candidates, by_name, by_full, excluded, headings })
    }

    pub fn resolve(&self, src_path: &str, kind: LinkKind, target: &str, heading: Option<&str>) -> Resolution {
        let matches: Vec<String> = match kind {
            LinkKind::Wiki | LinkKind::Embed => {
                let t = normalize(target);
                let t = strip_md(&t);
                if t.is_empty() {
                    vec![src_path.to_string()]
                } else if t.contains('/') {
                    // A key equal to `t` or ending in `/{t}` has `t`'s last component as its name.
                    let suffix = format!("/{t}");
                    self.paths_where(self.by_name.get(last_component(t)), |c| c.key == t || c.key.ends_with(&suffix))
                } else {
                    self.paths_where(self.by_name.get(t), |_| true)
                }
            }
            LinkKind::Markdown => {
                let Some(joined) = join_relative(src_path, target) else {
                    return Resolution::Broken;
                };
                let full = normalize(&joined);
                self.paths_where(self.by_full.get(&full), |_| true)
            }
        };
        let target_path = match <[String; 1]>::try_from(matches) {
            Ok([only]) => only,
            Err(m) if m.is_empty() => return Resolution::Broken,
            Err(mut m) => {
                m.sort();
                return Resolution::Ambiguous(m);
            }
        };
        if self.heading_matches(&target_path, heading) {
            Resolution::Ok(target_path)
        } else {
            Resolution::Broken
        }
    }

    /// Paths of the candidates at `indices` that satisfy `keep`, in path order.
    fn paths_where(&self, indices: Option<&Vec<usize>>, keep: impl Fn(&Candidate) -> bool) -> Vec<String> {
        indices
            .into_iter()
            .flatten()
            .map(|&i| &self.candidates[i])
            .filter(|c| keep(c))
            .map(|c| c.path.clone())
            .collect()
    }

    fn heading_matches(&self, path: &str, heading: Option<&str>) -> bool {
        let Some(heading) = heading else {
            return true;
        };
        let parts: Vec<String> = heading.split('#').filter(|p| !p.is_empty()).map(normalize).collect();
        if parts.is_empty() || self.excluded.contains(path) {
            return true;
        }
        self.headings
            .get(path)
            .is_some_and(|hps| hps.iter().any(|hp| hp.ends_with(&parts)))
    }

    /// Links resolving to `path`, sorted by (path, line); self-links excluded.
    pub fn backlinks(&self, conn: &Connection, path: &str) -> Result<Vec<Backlink>> {
        let mut stmt = conn.prepare(
            "SELECT src_path, target, heading, raw, line, kind FROM links
             WHERE src_path <> ?1 ORDER BY src_path, line, rowid",
        )?;
        let rows = stmt.query_map([path], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, i64>(4)?,
                r.get::<_, String>(5)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (src, target, heading, raw, line, kind) = row?;
            let kind = LinkKind::from_str(&kind).ok_or_else(|| VltError::Internal(format!("link kind: {kind}")))?;
            if self.resolve(&src, kind, &target, heading.as_deref()) == Resolution::Ok(path.to_string()) {
                out.push(Backlink { path: src, line: line as usize, raw });
            }
        }
        Ok(out)
    }
}

/// Joins a markdown link target to the source's directory (the vault root when the
/// target starts with `/`), collapsing `.` and `..`.
/// `None` when the result would escape the vault root.
fn join_relative(src_path: &str, target: &str) -> Option<String> {
    let mut parts: Vec<&str> = src_path.split('/').collect();
    parts.pop();
    if target.starts_with('/') {
        parts.clear();
    }
    for part in target.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            p => parts.push(p),
        }
    }
    Some(parts.join("/"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::indexer::parser::LinkKind::*;
    use crate::indexer::{reindex, scanner::build_exclude};
    use crate::ml::fake::FakeEmbedder;
    use crate::storage::db::{Db, IndexMeta};
    use std::{fs, sync::Arc};
    use tempfile::tempdir;

    fn setup() -> (tempfile::TempDir, Db) {
        let d = tempdir().unwrap();
        let v = d.path().join("vault");
        let files = [
            ("projects/mcp_rust.md", "# MCP\n## Architecture\n### Concurrency\ntext\n"),
            ("a/readme.md", "x\n"), ("b/readme.md", "y\n"),
            ("Café.md", "z\n"),
            ("notes/weekly.md", "See [[mcp_rust#Concurrency]] and [[projects/mcp_rust]] and [m](../projects/mcp_rust.md).\n[[readme]] [[nope]] [[mcp_rust#Nope]] [[#Local]]\n"),
            ("templates/t.md", "---\n: bad\n---\n"),
        ];
        for (p, c) in files { let p = v.join(p); fs::create_dir_all(p.parent().unwrap()).unwrap(); fs::write(p, c).unwrap(); }
        let mut db = Db::open(&d.path().join("vlt.db"), &IndexMeta { model_id: "fake".into(), dim: 384, vault: "v".into() }).unwrap();
        let src = || -> crate::Result<Arc<dyn crate::ml::Embedder>> { Ok(Arc::new(FakeEmbedder)) };
        reindex(db.conn_mut(), &v, &build_exclude(&["templates/**".into()]).unwrap(), &src, &mut |_, _| {}).unwrap();
        (d, db)
    }

    #[test]
    fn wikilink_rules() {
        let (_d, db) = setup();
        let li = LinkIndex::load(db.conn()).unwrap();
        let ok = |p: &str| Resolution::Ok(p.to_string());
        let src = "notes/weekly.md";
        assert_eq!(li.resolve(src, Wiki, "mcp_rust", None), ok("projects/mcp_rust.md"));
        assert_eq!(li.resolve(src, Wiki, "MCP_RUST.md", None), ok("projects/mcp_rust.md"));
        assert_eq!(li.resolve(src, Wiki, "projects/mcp_rust", None), ok("projects/mcp_rust.md"));
        assert_eq!(li.resolve(src, Wiki, "rust", None), Resolution::Broken, "file name must match whole");
        assert_eq!(li.resolve(src, Wiki, "cafe\u{301}", None), ok("Café.md"), "NFD target matches NFC name");
        assert_eq!(li.resolve(src, Embed, "readme", None), Resolution::Ambiguous(vec!["a/readme.md".into(), "b/readme.md".into()]));
        assert_eq!(li.resolve(src, Wiki, "a/readme", None), ok("a/readme.md"));
        assert_eq!(li.resolve(src, Wiki, "t", None), ok("templates/t.md"), "excluded files exist for links");
        assert_eq!(li.resolve(src, Wiki, "t", Some("Anything")), ok("templates/t.md"));
    }

    #[test]
    fn headings_and_markdown_links() {
        let (_d, db) = setup();
        let li = LinkIndex::load(db.conn()).unwrap();
        let src = "notes/weekly.md";
        let ok = Resolution::Ok("projects/mcp_rust.md".into());
        assert_eq!(li.resolve(src, Wiki, "mcp_rust", Some("concurrency")), ok);
        assert_eq!(li.resolve(src, Wiki, "mcp_rust", Some("Architecture#Concurrency")), ok);
        assert_eq!(li.resolve(src, Wiki, "mcp_rust", Some("MCP#Concurrency")), Resolution::Broken, "sequence must be a contiguous tail");
        assert_eq!(li.resolve(src, Wiki, "mcp_rust", Some("Nope")), Resolution::Broken);
        assert_eq!(li.resolve(src, Markdown, "../projects/mcp_rust.md", None), ok);
        assert_eq!(li.resolve(src, Markdown, "../../outside.md", None), Resolution::Broken);
        assert_eq!(li.resolve(src, Markdown, "/projects/mcp_rust.md", None), ok, "leading `/` is the vault root");
        assert_eq!(li.resolve(src, Markdown, "/../projects/mcp_rust.md", None), Resolution::Broken, "`..` cannot leave the root");
        assert_eq!(li.resolve("projects/mcp_rust.md", Wiki, "", Some("Concurrency")), Resolution::Ok("projects/mcp_rust.md".into()));
    }

    #[test]
    fn backlinks_resolve_all_link_kinds() {
        let (_d, db) = setup();
        let li = LinkIndex::load(db.conn()).unwrap();
        let b = li.backlinks(db.conn(), "projects/mcp_rust.md").unwrap();
        let raws: Vec<_> = b.iter().map(|x| (x.path.as_str(), x.line, x.raw.as_str())).collect();
        assert_eq!(raws, vec![
            ("notes/weekly.md", 1, "[[mcp_rust#Concurrency]]"),
            ("notes/weekly.md", 1, "[[projects/mcp_rust]]"),
            ("notes/weekly.md", 1, "[m](../projects/mcp_rust.md)"),
        ], "broken heading link [[mcp_rust#Nope]] is not a backlink");
        assert!(li.backlinks(db.conn(), "a/readme.md").unwrap().is_empty(), "ambiguous links are nobody's backlink");
    }

    fn index(files: &[(&str, &str)]) -> (tempfile::TempDir, Db) {
        let d = tempdir().unwrap();
        let v = d.path().join("vault");
        for (p, c) in files { let p = v.join(p); fs::create_dir_all(p.parent().unwrap()).unwrap(); fs::write(p, c).unwrap(); }
        let mut db = Db::open(&d.path().join("vlt.db"), &IndexMeta { model_id: "fake".into(), dim: 384, vault: "v".into() }).unwrap();
        let src = || -> crate::Result<Arc<dyn crate::ml::Embedder>> { Ok(Arc::new(FakeEmbedder)) };
        reindex(db.conn_mut(), &v, &build_exclude(&[]).unwrap(), &src, &mut |_, _| {}).unwrap();
        (d, db)
    }

    #[test]
    fn path_suffixes_markdown_paths_and_backlink_order() {
        let (_d, db) = index(&[
            ("work/projects/mcp_rust.md", "# H\n"),
            ("top.md", "Self [[top#Intro]] and [[#Intro]].\n# Intro\n"),
            ("notes/weekly.md", "x\n"),
            ("a/src.md", "x\n\n[[top]]\n"),
            ("b/src.md", "[[top]]\n"),
        ]);
        let li = LinkIndex::load(db.conn()).unwrap();
        let src = "notes/weekly.md";
        let ok = |p: &str| Resolution::Ok(p.to_string());
        assert_eq!(li.resolve(src, Wiki, "projects/mcp_rust", None), ok("work/projects/mcp_rust.md"), "`/` target matches the path's end");
        assert_eq!(li.resolve(src, Wiki, "jects/mcp_rust", None), Resolution::Broken, "suffix match is by whole components");
        assert_eq!(li.resolve(src, Markdown, "../top.md", None), ok("top.md"));
        assert_eq!(li.resolve(src, Markdown, "../../top.md", None), Resolution::Broken, "`..` cannot leave the vault");
        assert_eq!(li.resolve(src, Markdown, "../top", None), Resolution::Broken, "markdown links need `.md`");
        assert_eq!(li.resolve("top.md", Wiki, "", Some("Intro")), ok("top.md"));
        let b: Vec<_> = li.backlinks(db.conn(), "top.md").unwrap().into_iter().map(|x| (x.path, x.line)).collect();
        assert_eq!(b, vec![("a/src.md".to_string(), 3), ("b/src.md".to_string(), 1)], "sorted by (path, line), self-links excluded");
    }
}
