use std::os::unix::fs::MetadataExt;
use std::path::PathBuf;

use globset::GlobSet;
use rusqlite::OptionalExtension;

use crate::audit::{self, AuditResponse};
use crate::config::Config;
use crate::documents::{self, DocumentResponse, FoldersResponse, PropertiesResponse};
use crate::indexer::parser::split_frontmatter;
use crate::indexer::scanner::{build_exclude, IndexLock};
use crate::indexer::{self, ReindexStats};
use crate::links;
use crate::ml::models::Models;
use crate::paths;
use crate::related::{self, RelatedResponse};
use crate::search::{self, SearchRequest, SearchResponse};
use crate::storage::db::{self, Db, SCHEMA_VERSION};
use crate::write::{self, FileDiff, Loaded, DeleteResponse, MoveResponse, WriteResponse};
use crate::{Result, VltError};

pub struct App {
    pub home: PathBuf,
    pub cfg: Config,
    pub models: Models,
    exclude: GlobSet,
    /// Serializes vault writes: each holds it for read, change check, replace and reindex.
    write_lock: std::sync::Mutex<()>,
}

#[derive(Debug, serde::Serialize)]
pub struct ReindexOutcome {
    #[serde(flatten)]
    pub stats: ReindexStats,
    pub reindexed: bool,
}

impl App {
    pub fn load() -> Result<App> {
        let home = paths::vlt_home()?;
        let path = paths::config_path(&home);
        if !path.is_file() {
            return Err(VltError::Config("vlt is not configured; run `vlt install`".into()));
        }
        let cfg = Config::load(&path)?;
        App::new(home, cfg, Models::fake_from_env())
    }

    pub fn new(home: PathBuf, cfg: Config, fake_models: bool) -> Result<App> {
        let models = Models::new(&cfg.models, &paths::models_dir(&home), fake_models)?;
        let exclude = build_exclude(&cfg.exclude)?;
        Ok(App { home, cfg, models, exclude, write_lock: std::sync::Mutex::new(()) })
    }

    pub fn db_path(&self) -> PathBuf {
        paths::index_dir(&self.home).join("vlt.db")
    }

    fn lock(&self) -> Result<Option<IndexLock>> {
        IndexLock::try_acquire(&paths::index_dir(&self.home))
    }

    /// Incremental reindex, or a full rebuild when `force` or the index does not match the configuration.
    pub fn reindex(&self, force: bool, progress: &mut dyn FnMut(indexer::Progress)) -> Result<ReindexOutcome> {
        let Some(_lock) = self.lock()? else {
            return Ok(ReindexOutcome { stats: ReindexStats::default(), reindexed: false });
        };
        let mut db = Db::open_unchecked(&self.db_path())?;
        let meta = self.models.index_meta(&self.cfg.vault)?;
        let source = || self.models.embedder();
        let stats = if force || !index_matches(&db, &meta)? {
            indexer::rebuild(db.conn_mut(), &meta, &self.cfg.vault, &self.exclude, &source, progress)?
        } else {
            indexer::reindex(db.conn_mut(), &self.cfg.vault, &self.exclude, &source, progress)?
        };
        Ok(ReindexOutcome { stats, reindexed: true })
    }

    /// Incremental reindex that never rebuilds: an index that does not match the configuration is
    /// `IndexRebuildRequired`, left for `vlt reindex` to rebuild. A brand-new index is built.
    pub(crate) fn reindex_incremental(&self) -> Result<ReindexOutcome> {
        let Some(_lock) = self.lock()? else {
            return Ok(ReindexOutcome { stats: ReindexStats::default(), reindexed: false });
        };
        let mut db = Db::open(&self.db_path(), &self.models.index_meta(&self.cfg.vault)?)?;
        let source = || self.models.embedder();
        let stats = indexer::reindex(db.conn_mut(), &self.cfg.vault, &self.exclude, &source, &mut |_| {})?;
        Ok(ReindexOutcome { stats, reindexed: true })
    }

    /// Opens the index for a read tool, running an incremental reindex first when the lock is free.
    fn open_for_read(&self) -> Result<Db> {
        let path = self.db_path();
        let meta = self.models.index_meta(&self.cfg.vault)?;
        match self.lock()? {
            Some(_lock) => {
                let mut db = Db::open(&path, &meta)?;
                let source = || self.models.embedder();
                indexer::reindex(db.conn_mut(), &self.cfg.vault, &self.exclude, &source, &mut |_| {})?;
                Ok(db)
            }
            None if Db::exists(&path) && has_meta(&path)? => Db::open(&path, &meta),
            None => Err(VltError::Internal(
                "the index is being built by another process; try again shortly".into(),
            )),
        }
    }

    pub fn search(&self, req: &SearchRequest) -> Result<SearchResponse> {
        let db = self.open_for_read()?;
        let embedder = self.models.embedder()?;
        let reranker = self.models.reranker()?;
        search::search(db.conn(), &self.cfg.search, embedder.as_ref(), reranker.as_ref(), req)
    }

    /// The absolute path of an existing `.md` regular file and its vault-relative path as stored on
    /// disk (real case, through `canonicalize`). Does not apply `check_readable`: `edit` keeps working
    /// on excluded notes as today.
    fn existing_note(&self, path: &str) -> Result<(PathBuf, String)> {
        let abs = documents::resolve_in_vault(&self.cfg.vault, path)?;
        if !abs.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("md")) {
            return Err(VltError::InvalidArguments(format!("{path} is not a Markdown (.md) document")));
        }
        match std::fs::symlink_metadata(&abs) {
            Ok(m) if m.is_file() => {}
            Ok(_) => return Err(VltError::InvalidArguments(format!("{path} is not a regular file"))),
            Err(e) if matches!(e.kind(), std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory) => {
                return Err(VltError::DocumentNotFound(path.to_string()))
            }
            Err(e) => return Err(e.into()),
        }
        let vault = std::fs::canonicalize(&self.cfg.vault)?;
        let real = std::fs::canonicalize(&abs)?;
        let rel = real.strip_prefix(&vault).map_err(|_| VltError::PathOutsideVault(path.to_string()))?;
        Ok((abs, rel.to_string_lossy().into_owned()))
    }

    /// Validates a path for a new note and creates its folders: the real absolute path and the
    /// stored vault-relative path. Errors as `create` reports them; does not check existence.
    fn new_note_path(&self, path: &str) -> Result<(PathBuf, String)> {
        let abs = documents::resolve_in_vault(&self.cfg.vault, path)?;
        let name = abs.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        if !abs.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("md")) {
            return Err(VltError::InvalidArguments(format!("{path} is not a Markdown (.md) document")));
        }
        let requested = abs.strip_prefix(&self.cfg.vault).map_err(|e| VltError::Internal(e.to_string()))?;
        self.check_readable(path, &requested.to_string_lossy())?;
        let parent = abs.parent().ok_or_else(|| VltError::InvalidArguments(format!("{path} has no folder")))?;
        std::fs::create_dir_all(parent).map_err(|e| match e.kind() {
            std::io::ErrorKind::AlreadyExists | std::io::ErrorKind::NotADirectory => {
                VltError::InvalidArguments(format!("a folder in {path} is a file"))
            }
            _ => e.into(),
        })?;
        // Where the folder really is: on a case-insensitive filesystem `Ideas/` may be `ideas/`, and a
        // folder swapped for a symlink since the checks above points elsewhere.
        let vault = std::fs::canonicalize(&self.cfg.vault)?;
        let real_parent = std::fs::canonicalize(parent)?;
        let real_dir = real_parent.strip_prefix(&vault).map_err(|_| VltError::PathOutsideVault(path.to_string()))?;
        let asked_dir = requested.parent().unwrap_or(requested);
        if links::normalize(&real_dir.to_string_lossy()) != links::normalize(&asked_dir.to_string_lossy()) {
            return Err(VltError::PathOutsideVault(path.to_string()));
        }
        let stored = real_dir.join(&name).to_string_lossy().into_owned();
        self.check_readable(path, &stored)?;
        Ok((real_parent.join(&name), stored))
    }

    /// Reads the document, applies `change` to its LF text, writes it atomically and reindexes.
    fn write_document(&self, path: &str, change: impl FnOnce(&str) -> Result<String>) -> Result<WriteResponse> {
        let (abs, _) = self.existing_note(path)?;
        let _guard = self.write_lock.lock().unwrap_or_else(|e| e.into_inner());
        let loaded = Loaded::read(&abs)?;
        let updated = change(&loaded.lf)?;
        let diff = write::diff_hunk(&loaded.lf, &updated);
        write::atomic_replace(&abs, &loaded.bytes, &loaded.render(&updated))?;
        let reindexed = matches!(self.reindex_incremental(), Ok(o) if o.reindexed);
        Ok(WriteResponse { ok: true, path: path.to_string(), diff, reindexed })
    }

    /// Writes a new note at `path`; never replaces a file or folder that is there.
    pub fn create(&self, path: &str, content: &str) -> Result<WriteResponse> {
        let lf = content.strip_prefix('\u{feff}').unwrap_or(content).replace("\r\n", "\n");
        if let (_, Some(error), _, _) = split_frontmatter(&lf) {
            return Err(VltError::InvalidFrontmatter(error));
        }
        let _guard = self.write_lock.lock().unwrap_or_else(|e| e.into_inner());
        let (real_abs, stored) = self.new_note_path(path)?;
        if std::fs::symlink_metadata(&real_abs).is_ok() {
            return Err(VltError::AlreadyExists(path.to_string()));
        }
        write::create_new(&real_abs, content.as_bytes()).map_err(|e| match e.kind() {
            std::io::ErrorKind::AlreadyExists => VltError::AlreadyExists(path.to_string()),
            _ => e.into(),
        })?;
        let reindexed = matches!(self.reindex_incremental(), Ok(o) if o.reindexed);
        Ok(WriteResponse { ok: true, path: stored, diff: write::diff_hunk("", &lf), reindexed })
    }

    /// Moves or renames a note and rewrites the links the move would break. The rename is done once
    /// the rewrites start: a note whose links could not be rewritten (changed meanwhile, unwritable) is
    /// reported in `FilesChanged`, and the move is not undone.
    pub fn move_note(&self, path: &str, new_path: &str) -> Result<MoveResponse> {
        let (from_abs, from) = self.existing_note(path)?;
        self.check_readable(path, &from)?;
        if let Some(c) = new_path.split('/').flat_map(str::chars).find(|c| matches!(c, '#' | '|' | '[' | ']' | '^')) {
            return Err(VltError::InvalidArguments(format!(
                "{new_path} contains `{c}`: Obsidian links cannot point to a name with # | [ ] or ^"
            )));
        }
        let _guard = self.write_lock.lock().unwrap_or_else(|e| e.into_inner());
        let (to_abs, to) = self.new_note_path(new_path)?;
        if to == from {
            return Err(VltError::InvalidArguments(format!("{new_path} is the note's current path")));
        }
        if let Ok(m) = std::fs::symlink_metadata(&to_abs) {
            let same = std::fs::metadata(&from_abs).is_ok_and(|f| (f.dev(), f.ino()) == (m.dev(), m.ino()));
            if !(same && links::normalize(&to) == links::normalize(&from)) {
                return Err(VltError::AlreadyExists(new_path.to_string()));
            }
        }
        let rewrites = {
            let db = self.open_for_read()?;
            links::rewrites_for_move(&links::LinkIndex::load(db.conn())?, &links::load_links(db.conn())?, &from, &to)
        };
        std::fs::rename(&from_abs, &to_abs)?;
        let (mut updated, mut changed) = (Vec::new(), Vec::new());
        let mut srcs: Vec<&str> = rewrites.iter().map(|r| r.src.as_str()).collect();
        srcs.dedup();
        for src in srcs {
            let edits: Vec<(usize, &str, &str)> = rewrites
                .iter()
                .filter(|r| r.src == src)
                .map(|r| (r.line, r.raw.as_str(), r.new_raw.as_str()))
                .collect();
            let abs = self.cfg.vault.join(src);
            let result = Loaded::read(&abs).and_then(|loaded| {
                let new = write::replace_on_lines(&loaded.lf, &edits).ok_or(VltError::FileChanged)?;
                write::atomic_replace(&abs, &loaded.bytes, &loaded.render(&new))?;
                Ok(write::diff_hunk(&loaded.lf, &new))
            });
            match result {
                Ok(diff) => updated.push(FileDiff { path: src.to_string(), diff }),
                Err(_) => changed.push(src.to_string()),
            }
        }
        let reindexed = matches!(self.reindex_incremental(), Ok(o) if o.reindexed);
        if !changed.is_empty() {
            return Err(VltError::FilesChanged(changed));
        }
        Ok(MoveResponse { ok: true, path: to, from, updated, reindexed })
    }

    /// Deletes a note by moving it to `.trash/`, or removes a folder holding only folders and
    /// `.DS_Store`. `broken_links` lists the links to the note that now point nowhere.
    pub fn delete(&self, path: &str) -> Result<DeleteResponse> {
        let abs = documents::resolve_in_vault(&self.cfg.vault, path)?;
        let rel = abs.strip_prefix(&self.cfg.vault).map_err(|e| VltError::Internal(e.to_string()))?.to_string_lossy().into_owned();
        if rel.is_empty() {
            return Err(VltError::InvalidArguments("the vault root cannot be deleted".into()));
        }
        self.check_readable(path, &rel)?;
        match std::fs::symlink_metadata(&abs) {
            Ok(m) if m.is_dir() => {
                let _guard = self.write_lock.lock().unwrap_or_else(|e| e.into_inner());
                write::remove_empty_tree(&abs, &rel)?;
                let reindexed = matches!(self.reindex_incremental(), Ok(o) if o.reindexed);
                return Ok(DeleteResponse { ok: true, path: rel, trashed_to: None, broken_links: Vec::new(), reindexed });
            }
            Err(e) if matches!(e.kind(), std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory) => {
                return Err(VltError::DocumentNotFound(path.to_string()))
            }
            _ => {}
        }
        let (abs, rel) = self.existing_note(path)?;
        self.check_readable(path, &rel)?;
        let _guard = self.write_lock.lock().unwrap_or_else(|e| e.into_inner());
        let broken_links = {
            let db = self.open_for_read()?;
            links::LinkIndex::load(db.conn())?.backlinks(db.conn(), &rel)?
        };
        let trashed = write::trash_path(&self.cfg.vault, &rel);
        let dest = documents::resolve_in_vault(&self.cfg.vault, &trashed)?;
        let dest_dir = dest.parent().expect("under .trash");
        std::fs::create_dir_all(dest_dir)?;
        let trash_root = std::fs::canonicalize(&self.cfg.vault)?.join(".trash");
        if !std::fs::canonicalize(dest_dir)?.starts_with(&trash_root) {
            return Err(VltError::PathOutsideVault(trashed));
        }
        std::fs::rename(&abs, &dest)?;
        let reindexed = matches!(self.reindex_incremental(), Ok(o) if o.reindexed);
        Ok(DeleteResponse { ok: true, path: rel, trashed_to: Some(trashed), broken_links, reindexed })
    }

    /// Rejects a vault-relative path the index would skip, so a created note can be read back.
    fn check_readable(&self, path: &str, rel: &str) -> Result<()> {
        if rel.split('/').any(|part| part.starts_with('.')) {
            return Err(VltError::InvalidArguments(format!("{path} is in a hidden file or folder, which vlt does not index")));
        }
        if self.exclude.is_match(rel) {
            return Err(VltError::InvalidArguments(format!(
                "{path} is excluded by the configuration (exclude), so vlt could not read it back"
            )));
        }
        Ok(())
    }

    pub fn edit(&self, path: &str, old_text: &str, new_text: &str) -> Result<WriteResponse> {
        self.write_document(path, |lf| write::edit_text(lf, old_text, new_text))
    }

    pub fn update_properties(
        &self,
        path: &str,
        set: serde_json::Map<String, serde_json::Value>,
        unset: Vec<String>,
    ) -> Result<WriteResponse> {
        self.write_document(path, |lf| write::frontmatter::update_frontmatter(lf, &set, &unset))
    }

    pub fn get_document(&self, path: &str) -> Result<DocumentResponse> {
        let db = self.open_for_read()?;
        documents::get_document(db.conn(), &self.cfg.vault, path)
    }

    pub fn list_properties(&self, where_: Option<&serde_json::Value>, path_prefix: Option<&str>, key: Option<&str>) -> Result<PropertiesResponse> {
        let db = self.open_for_read()?;
        documents::list_properties(db.conn(), where_, path_prefix, key)
    }

    pub fn list_folders(&self, path_prefix: Option<&str>) -> Result<FoldersResponse> {
        let db = self.open_for_read()?;
        documents::list_folders(db.conn(), path_prefix)
    }

    pub fn audit(&self, path_prefix: Option<&str>) -> Result<AuditResponse> {
        let db = self.open_for_read()?;
        audit::audit(db.conn(), path_prefix)
    }

    pub fn related(&self, path: &str, limit: Option<usize>) -> Result<RelatedResponse> {
        let db = self.open_for_read()?;
        related::related(db.conn(), &self.cfg.vault, path, self.cfg.search.k, limit.unwrap_or(10))
    }
}

/// True when the index at `path` has meta rows; reads only, so it never writes under another
/// process's lock. Incomplete meta counts as present: `Db::open` reports it as a rebuild.
/// A file that vanished before the open counts as absent and is not created.
fn has_meta(path: &std::path::Path) -> Result<bool> {
    let flags = rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX;
    let conn = match rusqlite::Connection::open_with_flags(path, flags) {
        Err(rusqlite::Error::SqliteFailure(e, _)) if e.code == rusqlite::ErrorCode::CannotOpen => return Ok(false),
        r => r?,
    };
    conn.busy_timeout(std::time::Duration::from_millis(5000))?;
    match db::read_meta(&conn) {
        Ok(found) => Ok(found.is_some()),
        Err(VltError::IndexRebuildRequired(_)) => Ok(true),
        Err(e) => Err(e),
    }
}

/// False for a brand-new index, a different schema version, or meta that differs from `meta`.
fn index_matches(db: &Db, meta: &db::IndexMeta) -> Result<bool> {
    let schema: Option<String> = db
        .conn()
        .query_row("SELECT value FROM meta WHERE key = 'schema_version'", [], |r| r.get(0))
        .optional()?;
    if schema.as_deref() != Some(SCHEMA_VERSION) {
        return Ok(false);
    }
    match db::read_meta(db.conn()) {
        Ok(found) => Ok(found.as_ref() == Some(meta)),
        Err(VltError::IndexRebuildRequired(_)) => Ok(false),
        Err(e) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn has_meta_on_a_vanished_index_creates_nothing_and_reads_as_absent() {
        let d = tempfile::tempdir().unwrap();
        let path = d.path().join("vlt.db");
        assert!(!has_meta(&path).unwrap(), "a vanished index has no meta, so the read path reports it as being built");
        assert!(!path.exists(), "the read-only open created the db file");
    }
}
