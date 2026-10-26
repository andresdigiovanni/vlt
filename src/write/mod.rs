//! Vault writes (PRD 7.2): body edits and frontmatter updates, atomic and change-checked.

use std::fs;
use std::io::Write;
use std::path::Path;

use crate::error::{Result, VltError};
use crate::indexer::parser::split_frontmatter;

pub(crate) mod frontmatter;

#[derive(Debug, serde::Serialize)]
pub struct WriteResponse {
    pub ok: bool,
    pub path: String,
    pub diff: String,
    pub reindexed: bool,
}

/// A file as read from disk, with LF-normalized text for editing.
#[derive(Debug)]
pub(crate) struct Loaded {
    pub bytes: Vec<u8>,
    pub lf: String,
    /// The text as stored (BOM stripped), kept to restore the endings of lines an edit leaves alone.
    raw: String,
    /// The ending new lines get: CRLF when the file uses it at least as often as LF.
    crlf: bool,
    /// The file mixes CRLF and LF lines.
    mixed: bool,
    bom: bool,
}

impl Loaded {
    pub(crate) fn read(path: &Path) -> Result<Loaded> {
        let bytes = fs::read(path)?;
        let text = std::str::from_utf8(&bytes)
            .map_err(|_| VltError::InvalidArguments(format!("{} is not valid UTF-8", path.display())))?;
        let (bom, text) = match text.strip_prefix('\u{feff}') {
            Some(rest) => (true, rest),
            None => (false, text),
        };
        let crlf_lines = text.matches("\r\n").count();
        let lf_lines = text.matches('\n').count() - crlf_lines;
        let (lf, raw) = (text.replace("\r\n", "\n"), text.to_string());
        Ok(Loaded {
            bytes,
            lf,
            raw,
            crlf: crlf_lines > 0 && crlf_lines >= lf_lines,
            mixed: crlf_lines > 0 && lf_lines > 0,
            bom,
        })
    }

    pub(crate) fn render(&self, lf: &str) -> Vec<u8> {
        let text = if self.mixed { self.render_mixed(lf) } else { self.with_ending(lf) };
        if self.bom { format!("\u{feff}{text}").into_bytes() } else { text.into_bytes() }
    }

    fn with_ending(&self, lf: &str) -> String {
        if self.crlf { lf.replace('\n', "\r\n") } else { lf.to_string() }
    }

    /// Lines the edit left alone at the start and end of the file keep their own ending; the lines in
    /// between take the file's dominant one.
    fn render_mixed(&self, lf: &str) -> String {
        let (old, new, raw): (Vec<&str>, Vec<&str>, Vec<&str>) = (
            self.lf.split_inclusive('\n').collect(),
            lf.split_inclusive('\n').collect(),
            self.raw.split_inclusive('\n').collect(),
        );
        let (head, tail) = shared_lines(&old, &new);
        let middle = self.with_ending(&new[head..new.len() - tail].concat());
        format!("{}{}{}", raw[..head].concat(), middle, raw[raw.len() - tail..].concat())
    }
}

/// How many leading and trailing lines (terminators included) two texts share, never overlapping.
fn shared_lines(o: &[&str], n: &[&str]) -> (usize, usize) {
    let mut head = 0;
    while head < o.len() && head < n.len() && o[head] == n[head] {
        head += 1;
    }
    let mut tail = 0;
    while tail < o.len() - head && tail < n.len() - head && o[o.len() - 1 - tail] == n[n.len() - 1 - tail] {
        tail += 1;
    }
    (head, tail)
}

/// One unified hunk without context: the lines between the common head and tail.
pub(crate) fn diff_hunk(old: &str, new: &str) -> String {
    let (o, n): (Vec<&str>, Vec<&str>) = (old.split_inclusive('\n').collect(), new.split_inclusive('\n').collect());
    let (head, tail) = shared_lines(&o, &n);
    let (removed, added) = (&o[head..o.len() - tail], &n[head..n.len() - tail]);
    let mut out = format!("@@ -{},{} +{},{} @@", head + 1, removed.len(), head + 1, added.len());
    for (sign, lines) in [('-', removed), ('+', added)] {
        for line in lines {
            out.push_str(&format!("\n{sign}{}", line.strip_suffix('\n').unwrap_or(line)));
            if !line.ends_with('\n') {
                out.push_str("\n\\ No newline at end of file");
            }
        }
    }
    out
}

/// Writes `new` over `path` through a hidden temp file, unless the file no longer holds `original`.
pub(crate) fn atomic_replace(path: &Path, original: &[u8], new: &[u8]) -> Result<()> {
    let name = path.file_name().ok_or_else(|| VltError::Internal("path has no file name".into()))?;
    let tmp = path.with_file_name(format!(".{}.vlt-tmp", name.to_string_lossy()));
    let perms = fs::metadata(path)?.permissions();
    let result = (|| -> Result<()> {
        fs::write(&tmp, new)?;
        fs::set_permissions(&tmp, perms)?;
        if fs::read(path)? != original {
            return Err(VltError::FileChanged);
        }
        fs::rename(&tmp, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

/// Writes `new` to a file that must not exist yet: the exclusive create never replaces a file and never
/// follows a symlink at `path`. A failed write removes the partial file.
pub(crate) fn create_new(path: &Path, new: &[u8]) -> std::io::Result<()> {
    let mut file = fs::OpenOptions::new().write(true).create_new(true).open(path)?;
    let written = file.write_all(new).and_then(|()| file.sync_all());
    if written.is_err() {
        let _ = fs::remove_file(path);
    }
    written
}

/// Replaces the single occurrence of `old` in the body (after the frontmatter) of LF text.
pub(crate) fn edit_text(lf: &str, old: &str, new: &str) -> Result<String> {
    let (old, new) = (old.replace("\r\n", "\n"), new.replace("\r\n", "\n"));
    if old.is_empty() {
        return Err(VltError::InvalidArguments("old_text must not be empty".into()));
    }
    if old == new {
        return Err(VltError::InvalidArguments("nothing to change: old_text equals new_text".into()));
    }
    let (_, _, body, _) = split_frontmatter(lf);
    let body_start = lf.len() - body.len();
    let first = body.find(&old).ok_or(VltError::NotFound)?;
    let (mut count, mut at) = (1, first);
    // Step one char past each hit so overlapping occurrences ("aa" in "aaa") count too.
    loop {
        let next = at + body[at..].chars().next().map_or(1, char::len_utf8);
        match body[next..].find(&old) {
            Some(i) => (at, count) = (next + i, count + 1),
            None => break,
        }
    }
    if count > 1 {
        return Err(VltError::Ambiguous(count));
    }
    let at = body_start + first;
    Ok(format!("{}{}{}", &lf[..at], new, &lf[at + old.len()..]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diff_of_one_changed_line() {
        let d = diff_hunk("a\nb\nc\n", "a\nB\nc\n");
        assert_eq!(d, "@@ -2,1 +2,1 @@\n-b\n+B");
    }

    #[test]
    fn diff_of_an_insertion_and_a_deletion() {
        assert_eq!(diff_hunk("a\nc\n", "a\nb\nc\n"), "@@ -2,0 +2,1 @@\n+b");
        assert_eq!(diff_hunk("a\nb\nc\n", "a\nc\n"), "@@ -2,1 +2,0 @@\n-b");
    }

    #[test]
    fn crlf_is_normalized_for_editing_and_restored_on_write() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("n.md");
        std::fs::write(&p, "one\r\ntwo\r\n").unwrap();
        let l = Loaded::read(&p).unwrap();
        assert_eq!(l.lf, "one\ntwo\n");
        assert_eq!(l.render("one\nTWO\n"), b"one\r\nTWO\r\n");
        std::fs::write(&p, "one\ntwo").unwrap();
        let l = Loaded::read(&p).unwrap();
        assert_eq!(l.render("one\nTWO"), b"one\nTWO", "LF files and a missing final newline are kept");
    }

    #[test]
    fn a_leading_bom_is_stripped_for_editing_and_restored_on_write() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("n.md");
        std::fs::write(&p, "\u{feff}---\r\na: 1\r\n---\r\nB\r\n").unwrap();
        let l = Loaded::read(&p).unwrap();
        assert_eq!(l.lf, "---\na: 1\n---\nB\n");
        assert_eq!(l.bytes, "\u{feff}---\r\na: 1\r\n---\r\nB\r\n".as_bytes(), "bytes stay raw");
        assert_eq!(l.render("---\na: 2\n---\nB\n"), "\u{feff}---\r\na: 2\r\n---\r\nB\r\n".as_bytes());
    }

    #[test]
    fn a_change_of_only_the_final_newline_is_shown() {
        assert_eq!(diff_hunk("a\n", "a"), "@@ -1,1 +1,1 @@\n-a\n+a\n\\ No newline at end of file");
        assert_eq!(diff_hunk("a", "a\n"), "@@ -1,1 +1,1 @@\n-a\n\\ No newline at end of file\n+a");
    }

    #[test]
    fn mixed_line_endings_survive_outside_the_changed_lines() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("n.md");
        std::fs::write(&p, "a\r\nb\nc\r\nd\n").unwrap();
        let l = Loaded::read(&p).unwrap();
        assert_eq!(l.render("a\nb\nC\nd\n"), b"a\r\nb\nC\r\nd\n", "a tie between the endings uses CRLF");
        std::fs::write(&p, "a\nb\nc\nd\r\n").unwrap();
        let l = Loaded::read(&p).unwrap();
        assert_eq!(l.render("a\nb\nC\nd\n"), b"a\nb\nC\nd\r\n", "the last line keeps its own ending");
        assert_eq!(l.render("x\na\nb\nc\nd\n"), b"x\na\nb\nc\nd\r\n", "an inserted line takes the dominant ending");
    }

    #[test]
    fn non_utf8_is_invalid_arguments() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("n.md");
        std::fs::write(&p, [0xff, 0xfe]).unwrap();
        assert_eq!(Loaded::read(&p).unwrap_err().code(), "invalid_arguments");
    }

    #[test]
    fn atomic_replace_swaps_the_file_and_leaves_no_temp() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("n.md");
        std::fs::write(&p, "old").unwrap();
        atomic_replace(&p, b"old", b"new").unwrap();
        assert_eq!(std::fs::read(&p).unwrap(), b"new");
        assert!(!dir.path().join(".n.md.vlt-tmp").exists());
    }

    #[test]
    fn atomic_replace_refuses_when_the_file_changed() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("n.md");
        std::fs::write(&p, "edited by the user").unwrap();
        let err = atomic_replace(&p, b"what the agent read", b"new").unwrap_err();
        assert_eq!(err.code(), "file_changed");
        assert_eq!(std::fs::read(&p).unwrap(), b"edited by the user");
        assert!(!dir.path().join(".n.md.vlt-tmp").exists(), "the temp file is removed");
    }

    #[cfg(unix)]
    #[test]
    fn atomic_replace_keeps_the_original_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("n.md");
        std::fs::write(&p, "old").unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o600)).unwrap();
        atomic_replace(&p, b"old", b"new").unwrap();
        assert_eq!(std::fs::metadata(&p).unwrap().permissions().mode() & 0o777, 0o600);
    }

    #[test]
    fn edit_replaces_one_exact_occurrence_in_the_body() {
        let t = "---\nstatus: a\n---\n# T\n\nWAL mode.\n";
        assert_eq!(
            edit_text(t, "WAL mode.", "WAL mode. See [[sqlite]].").unwrap(),
            "---\nstatus: a\n---\n# T\n\nWAL mode. See [[sqlite]].\n"
        );
    }

    #[test]
    fn edit_errors() {
        let t = "---\nstatus: a\n---\nfoo bar foo\n";
        assert_eq!(edit_text(t, "baz", "x").unwrap_err().code(), "not_found");
        let amb = edit_text(t, "foo", "x").unwrap_err();
        assert_eq!(amb.code(), "ambiguous");
        assert!(amb.to_string().contains("2 times"), "{amb}");
        assert_eq!(edit_text(t, "", "x").unwrap_err().code(), "invalid_arguments");
        assert_eq!(edit_text(t, "bar", "bar").unwrap_err().code(), "invalid_arguments");
        assert_eq!(edit_text(t, "status", "x").unwrap_err().code(), "not_found", "the frontmatter is not part of the body");
    }

    #[test]
    fn overlapping_occurrences_are_ambiguous() {
        let amb = edit_text("aaa\n", "aa", "x").unwrap_err();
        assert_eq!(amb.code(), "ambiguous");
        assert!(amb.to_string().contains("2 times"), "{amb}");
        let amb = edit_text("ééé\n", "éé", "x").unwrap_err();
        assert_eq!(amb.code(), "ambiguous");
        assert!(amb.to_string().contains("2 times"), "{amb}");
    }

    #[test]
    fn edit_matches_across_lines_whatever_the_line_endings_of_the_arguments() {
        let t = "a\nb\nc\n";
        assert_eq!(edit_text(t, "a\r\nb", "A\r\nB").unwrap(), "A\nB\nc\n");
    }
}
