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

        let mut headings: HashMap<String, Vec<Vec<String>>> = HashMap::new();
        let mut stmt = conn.prepare("SELECT path, heading_path_json FROM headings")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        for row in rows {
            let (path, json) = row?;
            let hp: Vec<String> =
                serde_json::from_str(&json).map_err(|e| VltError::Internal(format!("heading_path_json: {e}")))?;
            headings.entry(path).or_default().push(hp.iter().map(|h| normalize(h)).collect());
        }
        Ok(LinkIndex::from_parts(paths, excluded, headings))
    }

    fn from_parts(
        mut paths: Vec<String>,
        excluded: HashSet<String>,
        headings: HashMap<String, Vec<Vec<String>>>,
    ) -> LinkIndex {
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
        LinkIndex { candidates, by_name, by_full, excluded, headings }
    }

    /// This index with the note at `from` moved to `to`: its candidate path and headings follow it.
    pub fn with_move(&self, from: &str, to: &str) -> LinkIndex {
        let paths = self
            .candidates
            .iter()
            .map(|c| if c.path == from { to.to_string() } else { c.path.clone() })
            .collect();
        let mut headings = self.headings.clone();
        if let Some(h) = headings.remove(from) {
            headings.insert(to.to_string(), h);
        }
        LinkIndex::from_parts(paths, self.excluded.clone(), headings)
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

/// One row of the `links` table.
#[derive(Debug, Clone, PartialEq)]
pub struct StoredLink {
    pub src_path: String,
    pub kind: LinkKind,
    pub target: String,
    pub heading: Option<String>,
    pub raw: String,
    pub line: usize,
}

/// Every row of `links`.
pub fn load_links(conn: &Connection) -> Result<Vec<StoredLink>> {
    let mut stmt = conn.prepare(
        "SELECT src_path, target, heading, raw, line, kind FROM links ORDER BY src_path, line, rowid",
    )?;
    let rows = stmt.query_map([], |r| {
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
        let (src_path, target, heading, raw, line, kind) = row?;
        let kind = LinkKind::from_str(&kind).ok_or_else(|| VltError::Internal(format!("link kind: {kind}")))?;
        out.push(StoredLink { src_path, kind, target, heading, raw, line: line as usize });
    }
    Ok(out)
}

/// In `src` (its path after the move), every `raw` on `line` becomes `new_raw`.
#[derive(Debug, Clone, PartialEq)]
pub struct Rewrite {
    pub src: String,
    pub line: usize,
    pub raw: String,
    pub new_raw: String,
}

/// The links that resolved before moving `from` to `to` and would not resolve to the same note
/// after it, each with the text that does. Sorted by (src, line); one entry per (src, line, raw).
pub fn rewrites_for_move(before: &LinkIndex, links: &[StoredLink], from: &str, to: &str) -> Vec<Rewrite> {
    let after = before.with_move(from, to);
    let mut out: Vec<Rewrite> = Vec::new();
    for l in links {
        let Resolution::Ok(p) = before.resolve(&l.src_path, l.kind, &l.target, l.heading.as_deref()) else {
            continue;
        };
        let src = if l.src_path == from { to } else { l.src_path.as_str() };
        let want = if p == from { to } else { p.as_str() };
        if after.resolve(src, l.kind, &l.target, l.heading.as_deref()) == Resolution::Ok(want.to_string()) {
            continue;
        }
        let new_raw = match l.kind {
            LinkKind::Wiki | LinkKind::Embed => wiki_raw(&after, src, l, want),
            LinkKind::Markdown => markdown_raw(src, l, want),
        };
        let Some(new_raw) = new_raw else { continue };
        let r = Rewrite { src: src.to_string(), line: l.line, raw: l.raw.clone(), new_raw };
        if !out.contains(&r) {
            out.push(r);
        }
    }
    out.sort_by(|a, b| (&a.src, a.line).cmp(&(&b.src, b.line)));
    out
}

/// `l.raw` with its target replaced by the shortest form that resolves to `want` from `src`.
fn wiki_raw(after: &LinkIndex, src: &str, l: &StoredLink, want: &str) -> Option<String> {
    let key = strip_md(want);
    let new_target = if l.target.contains('/') {
        key.to_string()
    } else {
        let parts: Vec<&str> = key.split('/').collect();
        (0..parts.len())
            .rev()
            .map(|i| parts[i..].join("/"))
            .find(|suffix| after.resolve(src, l.kind, suffix, None) == Resolution::Ok(want.to_string()))
            .unwrap_or_else(|| key.to_string())
    };
    let keeps_md = l.target.len() >= 3
        && l.target.is_char_boundary(l.target.len() - 3)
        && l.target[l.target.len() - 3..].eq_ignore_ascii_case(".md");
    let new_target = if keeps_md { format!("{new_target}.md") } else { new_target };
    let open = if l.raw.starts_with("![[") { 3 } else { 2 };
    let at = open + l.raw.get(open..)?.find(&l.target)?;
    Some(format!("{}{}{}", &l.raw[..at], new_target, &l.raw[at + l.target.len()..]))
}

/// `l.raw` with the path of its destination replaced by the one that reaches `want` from `src`.
fn markdown_raw(src: &str, l: &StoredLink, want: &str) -> Option<String> {
    let (start, end, angled) = markdown_dest_span(&l.raw)?;
    let path = if l.target.starts_with('/') { format!("/{want}") } else { relative_path(src, want) };
    let path = if angled {
        path.replace('%', "%25").replace('>', "%3E")
    } else {
        path.replace('%', "%25").replace(' ', "%20").replace('(', "%28").replace(')', "%29").replace('<', "%3C").replace('>', "%3E")
    };
    Some(format!("{}{}{}", &l.raw[..start], path, &l.raw[end..]))
}

/// Byte range of the destination's path part (before any `#`) in a markdown link's `raw`, and
/// whether the destination is written in `<...>`.
fn markdown_dest_span(raw: &str) -> Option<(usize, usize, bool)> {
    let b = raw.as_bytes();
    let mut i = usize::from(b.first() == Some(&b'!'));
    if b.get(i) != Some(&b'[') {
        return None;
    }
    let mut depth = 0usize;
    loop {
        match b.get(i)? {
            b'[' => depth += 1,
            b']' => {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
            _ => {}
        }
        i += 1;
    }
    i += 1;
    if b.get(i) != Some(&b'(') {
        return None;
    }
    i += 1;
    while b.get(i) == Some(&b' ') {
        i += 1;
    }
    if b.get(i) == Some(&b'<') {
        let start = i + 1;
        let end = start + b[start..].iter().position(|&c| c == b'#' || c == b'>')?;
        return Some((start, end, true));
    }
    let start = i;
    let mut open = 0usize;
    while let Some(&c) = b.get(i) {
        match c {
            b'#' | b' ' | b'\t' => break,
            b'(' => open += 1,
            b')' if open == 0 => break,
            b')' => open -= 1,
            _ => {}
        }
        i += 1;
    }
    Some((start, i, false))
}

/// The path from `src`'s folder to `target`: the common leading folders dropped, one `..` per
/// folder of `src` left, then the rest of `target`.
fn relative_path(src: &str, target: &str) -> String {
    let mut src_dirs: Vec<&str> = src.split('/').collect();
    src_dirs.pop();
    let target_parts: Vec<&str> = target.split('/').collect();
    let target_dirs = &target_parts[..target_parts.len() - 1];
    let common = src_dirs.iter().zip(target_dirs).take_while(|(a, b)| a == b).count();
    let mut parts: Vec<&str> = vec![".."; src_dirs.len() - common];
    parts.extend(&target_parts[common..]);
    parts.join("/")
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
        let mut db = Db::open(&d.path().join("vlt.db"), &IndexMeta { model_id: "fake".into(), dim: 384, fingerprint: "fake".into(), vault: "v".into() }).unwrap();
        let src = || -> crate::Result<Arc<dyn crate::ml::Embedder>> { Ok(Arc::new(FakeEmbedder)) };
        reindex(db.conn_mut(), &v, &build_exclude(&["templates/**".into()]).unwrap(), &src, &mut |_| {}).unwrap();
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
        let mut db = Db::open(&d.path().join("vlt.db"), &IndexMeta { model_id: "fake".into(), dim: 384, fingerprint: "fake".into(), vault: "v".into() }).unwrap();
        let src = || -> crate::Result<Arc<dyn crate::ml::Embedder>> { Ok(Arc::new(FakeEmbedder)) };
        reindex(db.conn_mut(), &v, &build_exclude(&[]).unwrap(), &src, &mut |_| {}).unwrap();
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

    fn rewrites(files: &[(&str, &str)], from: &str, to: &str) -> Vec<(String, usize, String, String)> {
        let (_d, db) = index(files);
        let li = LinkIndex::load(db.conn()).unwrap();
        rewrites_for_move(&li, &load_links(db.conn()).unwrap(), from, to)
            .into_iter()
            .map(|r| (r.src, r.line, r.raw, r.new_raw))
            .collect()
    }

    fn rw(src: &str, line: usize, raw: &str, new_raw: &str) -> (String, usize, String, String) {
        (src.into(), line, raw.into(), new_raw.into())
    }

    #[test]
    fn a_rename_rewrites_every_link_kind_in_its_own_style() {
        let got = rewrites(&[
            ("p/old.md", "# Intro\n"),
            ("n.md", "---\nup: \"[[old]]\"\n---\n[[old]] ![[old#Intro|see]] [[p/old]] [[old.md]]\n[a](p/old.md#Intro) [b](</p/old.md>)\n"),
        ], "p/old.md", "p/New Name.md");
        assert_eq!(got, vec![
            rw("n.md", 2, "[[old]]", "[[New Name]]"),
            rw("n.md", 4, "[[old]]", "[[New Name]]"),
            rw("n.md", 4, "![[old#Intro|see]]", "![[New Name#Intro|see]]"),
            rw("n.md", 4, "[[p/old]]", "[[p/New Name]]"),
            rw("n.md", 4, "[[old.md]]", "[[New Name.md]]"),
            rw("n.md", 5, "[a](p/old.md#Intro)", "[a](p/New%20Name.md#Intro)"),
            rw("n.md", 5, "[b](</p/old.md>)", "[b](</p/New Name.md>)"),
        ]);
    }

    #[test]
    fn a_move_keeps_name_links_and_fixes_paths_and_its_own_relative_links() {
        let got = rewrites(&[
            ("a/x.md", "[up](../top.md) [[top]]\n"),
            ("top.md", "[[x]] [[a/x]] [x](a/x.md)\n"),
        ], "a/x.md", "b/c/x.md");
        assert_eq!(got, vec![
            rw("b/c/x.md", 1, "[up](../top.md)", "[up](../../top.md)"),
            rw("top.md", 1, "[[a/x]]", "[[b/c/x]]"),
            rw("top.md", 1, "[x](a/x.md)", "[x](b/c/x.md)"),
        ], "[[x]] and [[top]] still resolve");
    }

    #[test]
    fn a_rename_onto_a_taken_name_disambiguates_the_other_notes_links() {
        let got = rewrites(&[
            ("a/y.md", "x\n"),
            ("b/x.md", "x\n"),
            ("n.md", "[[y]] [[x]]\n"),
        ], "b/x.md", "b/y.md");
        assert_eq!(got, vec![
            rw("n.md", 1, "[[y]]", "[[a/y]]"),
            rw("n.md", 1, "[[x]]", "[[b/y]]"),
        ]);
    }

    #[test]
    fn broken_ambiguous_and_self_links_are_left_alone() {
        let got = rewrites(&[
            ("a/r.md", "x\n"), ("b/r.md", "x\n"),
            ("old.md", "[[#H]] [[nope]]\n# H\n"),
            ("n.md", "[[r]] [[nope]] [[old#Missing]]\n"),
        ], "old.md", "new.md");
        assert_eq!(got, vec![]);
    }

    #[test]
    fn a_backslash_in_a_markdown_label_does_not_hide_the_destination() {
        let got = rewrites(&[("a/old.md", "x\n"), ("n.md", "[a\\](a/old.md)\n")], "a/old.md", "b/x.md");
        assert_eq!(got, vec![rw("n.md", 1, "[a\\](a/old.md)", "[a\\](b/x.md)")]);
    }

    #[test]
    fn a_rename_encodes_what_would_end_a_markdown_destination_and_it_still_resolves() {
        let files = [("p/a.md", "# H\n"), ("n.md", "[x](p/a.md) [y](<p/a.md>) [z](p/a.md#H)\n")];
        let got = rewrites(&files, "p/a.md", "p/b (1).md");
        assert_eq!(got, vec![
            rw("n.md", 1, "[x](p/a.md)", "[x](p/b%20%281%29.md)"),
            rw("n.md", 1, "[y](<p/a.md>)", "[y](<p/b (1).md>)"),
            rw("n.md", 1, "[z](p/a.md#H)", "[z](p/b%20%281%29.md#H)"),
        ]);
        let (_d, db) = index(&files);
        let after = LinkIndex::load(db.conn()).unwrap().with_move("p/a.md", "p/b (1).md");
        for (_, _, _, new_raw) in &got {
            let doc = crate::indexer::parser::parse(&format!("{new_raw}\n"), "n");
            let l = &doc.links[0];
            assert_eq!(l.target, "p/b (1).md", "{new_raw}");
            assert_eq!(after.resolve("n.md", Markdown, &l.target, None), Resolution::Ok("p/b (1).md".into()), "{new_raw}");
        }
    }

    #[test]
    fn a_rename_encodes_percent_and_angle_brackets_in_a_markdown_destination() {
        let got = rewrites(&[("a.md", "x\n"), ("n.md", "[x](a.md) [y](<a.md>)\n")], "a.md", "50% <b>.md");
        assert_eq!(got, vec![
            rw("n.md", 1, "[x](a.md)", "[x](50%25%20%3Cb%3E.md)"),
            rw("n.md", 1, "[y](<a.md>)", "[y](<50%25 <b%3E.md>)"),
        ]);
    }

    #[test]
    fn with_move_carries_the_headings() {
        let (_d, db) = index(&[("a.md", "# H\n")]);
        let li = LinkIndex::load(db.conn()).unwrap().with_move("a.md", "z/b.md");
        assert_eq!(li.resolve("n.md", Wiki, "b", Some("H")), Resolution::Ok("z/b.md".into()));
        assert_eq!(li.resolve("n.md", Wiki, "a", None), Resolution::Broken);
    }
}
