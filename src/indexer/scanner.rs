//! Vault scanning, exclusion globs and the index lock.

use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use globset::{GlobBuilder, GlobSet, GlobSetBuilder};
use sha2::{Digest, Sha256};
use walkdir::WalkDir;

use crate::error::{Result, VltError};

pub struct ScannedFile {
    pub rel_path: String,
    pub abs_path: PathBuf,
    pub mtime_ns: i64,
    pub size: u64,
}

/// Both lists are sorted by `rel_path`.
pub struct Scan {
    pub files: Vec<ScannedFile>,
    pub excluded: Vec<String>,
}

/// Compiles `exclude` patterns; `*` stays within one path segment, `**` crosses them.
pub fn build_exclude(patterns: &[String]) -> Result<GlobSet> {
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        let glob = GlobBuilder::new(pattern)
            .literal_separator(true)
            .build()
            .map_err(|e| VltError::Config(format!("invalid exclude pattern `{pattern}`: {e}")))?;
        builder.add(glob);
    }
    builder
        .build()
        .map_err(|e| VltError::Config(format!("invalid exclude patterns: {e}")))
}

fn is_hidden(entry: &walkdir::DirEntry) -> bool {
    entry.depth() > 0 && entry.file_name().to_string_lossy().starts_with('.')
}

fn has_md_extension(path: &Path) -> bool {
    path.extension()
        .is_some_and(|ext| ext.to_string_lossy().eq_ignore_ascii_case("md"))
}

/// `None` when the relative path is not valid UTF-8.
fn rel_path(vault: &Path, path: &Path) -> Option<String> {
    let rel = path.strip_prefix(vault).unwrap_or(path);
    rel.components()
        .map(|c| c.as_os_str().to_str())
        .collect::<Option<Vec<_>>>()
        .map(|parts| parts.join("/"))
}

fn mtime_ns(meta: &std::fs::Metadata) -> Result<i64> {
    let modified = meta.modified()?;
    Ok(match modified.duration_since(UNIX_EPOCH) {
        Ok(d) => d.as_nanos() as i64,
        Err(e) => -(e.duration().as_nanos() as i64),
    })
}

/// `Ok(None)` when the path vanished mid-walk (e.g. an editor's atomic save).
fn skip_if_vanished<T>(res: std::result::Result<T, std::io::Error>) -> Result<Option<T>> {
    match res {
        Ok(v) => Ok(Some(v)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

pub fn scan(vault: &Path, exclude: &GlobSet) -> Result<Scan> {
    if !vault.is_dir() {
        return Err(VltError::Config(format!(
            "vault directory does not exist: {}",
            vault.display()
        )));
    }
    let mut files = Vec::new();
    let mut excluded = Vec::new();
    let walker = WalkDir::new(vault)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| !is_hidden(e) && !e.path_is_symlink());
    for entry in walker {
        let Some(entry) = skip_if_vanished(entry.map_err(std::io::Error::from))? else {
            continue;
        };
        if !entry.file_type().is_file() || !has_md_extension(entry.path()) {
            continue;
        }
        let Some(rel) = rel_path(vault, entry.path()) else {
            continue;
        };
        if exclude.is_match(&rel) {
            excluded.push(rel);
            continue;
        }
        let Some(meta) = skip_if_vanished(entry.metadata().map_err(std::io::Error::from))? else {
            continue;
        };
        files.push(ScannedFile {
            rel_path: rel,
            abs_path: entry.path().to_path_buf(),
            mtime_ns: mtime_ns(&meta)?,
            size: meta.len(),
        });
    }
    files.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));
    excluded.sort();
    Ok(Scan { files, excluded })
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

/// Exclusive reindex lock on `<index_dir>/vlt.lock`, released on drop.
pub struct IndexLock {
    _file: std::fs::File,
}

impl IndexLock {
    /// `Ok(None)` when another holder has the lock.
    pub fn try_acquire(index_dir: &Path) -> Result<Option<IndexLock>> {
        std::fs::create_dir_all(index_dir)?;
        let file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(index_dir.join("vlt.lock"))?;
        match fs4::FileExt::try_lock(&file) {
            Ok(()) => Ok(Some(IndexLock { _file: file })),
            Err(fs4::TryLockError::WouldBlock) => Ok(None),
            Err(fs4::TryLockError::Error(e)) => Err(e.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn scan_skips_dotfiles_symlinks_non_md_and_splits_excluded() {
        let d = tempdir().unwrap();
        let v = d.path();
        for p in ["a.md", "sub/b.MD", "templates/t.md", ".obsidian/x.md", "sub/.hidden.md", "img.png"] {
            let p = v.join(p);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(&p, "x").unwrap();
        }
        std::os::unix::fs::symlink(v.join("a.md"), v.join("link.md")).unwrap();
        std::os::unix::fs::symlink(v.join("sub"), v.join("sublink")).unwrap();
        let s = scan(v, &build_exclude(&["templates/**".into()]).unwrap()).unwrap();
        let files: Vec<_> = s.files.iter().map(|f| f.rel_path.as_str()).collect();
        assert_eq!(files, vec!["a.md", "sub/b.MD"]);
        assert_eq!(s.excluded, vec!["templates/t.md".to_string()]);
        assert_eq!(s.files[0].size, 1);
    }

    #[test]
    fn bad_glob_and_missing_vault_are_config_errors() {
        assert!(matches!(build_exclude(&["[".into()]), Err(VltError::Config(_))));
        assert!(matches!(scan(Path::new("/nonexistent/vlt"), &build_exclude(&[]).unwrap()), Err(VltError::Config(_))));
    }

    #[test]
    fn lock_is_exclusive_until_dropped() {
        let d = tempdir().unwrap();
        let first = IndexLock::try_acquire(d.path()).unwrap();
        assert!(first.is_some());
        assert!(IndexLock::try_acquire(d.path()).unwrap().is_none());
        drop(first);
        assert!(IndexLock::try_acquire(d.path()).unwrap().is_some());
    }

    #[test]
    fn directories_named_md_are_walked_not_listed() {
        let d = tempdir().unwrap();
        fs::create_dir_all(d.path().join("dir.md")).unwrap();
        fs::write(d.path().join("dir.md/n.md"), "x").unwrap();
        let s = scan(d.path(), &build_exclude(&[]).unwrap()).unwrap();
        let files: Vec<_> = s.files.iter().map(|f| f.rel_path.as_str()).collect();
        assert_eq!(files, vec!["dir.md/n.md"]);
    }

    #[test]
    fn single_star_stays_within_one_segment() {
        let d = tempdir().unwrap();
        for p in ["top.md", "sub/deep.md"] {
            let p = d.path().join(p);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(&p, "x").unwrap();
        }
        let s = scan(d.path(), &build_exclude(&["*.md".into()]).unwrap()).unwrap();
        assert_eq!(s.excluded, vec!["top.md".to_string()]);
        assert_eq!(s.files.len(), 1);
        assert_eq!(s.files[0].rel_path, "sub/deep.md");
    }

    #[test]
    fn vanished_paths_are_skipped_other_io_errors_abort() {
        use std::io::{Error, ErrorKind};
        assert!(matches!(skip_if_vanished(Ok::<_, Error>(7)), Ok(Some(7))));
        assert!(matches!(skip_if_vanished::<()>(Err(Error::from(ErrorKind::NotFound))), Ok(None)));
        assert!(matches!(
            skip_if_vanished::<()>(Err(Error::from(ErrorKind::PermissionDenied))),
            Err(VltError::Io(e)) if e.kind() == ErrorKind::PermissionDenied
        ));
    }

    #[test]
    fn non_utf8_relative_paths_are_rejected() {
        use std::os::unix::ffi::OsStrExt;
        let v = Path::new("/v");
        let bad = v.join(std::ffi::OsStr::from_bytes(b"sub/n\xffote.md"));
        assert_eq!(rel_path(v, &bad), None);
        assert_eq!(rel_path(v, &v.join("sub/ok.md")), Some("sub/ok.md".to_string()));
    }
}
