use std::path::PathBuf;

use globset::GlobSet;
use rusqlite::OptionalExtension;

use crate::config::Config;
use crate::documents::{self, DocumentResponse, PropertiesResponse};
use crate::indexer::scanner::{build_exclude, IndexLock};
use crate::indexer::{self, ReindexStats};
use crate::ml::models::Models;
use crate::paths;
use crate::search::{self, SearchRequest, SearchResponse};
use crate::storage::db::{self, Db, SCHEMA_VERSION};
use crate::write::{self, Loaded, WriteResponse};
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
    pub fn reindex(&self, force: bool, progress: &mut dyn FnMut(usize, usize)) -> Result<ReindexOutcome> {
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
        let stats = indexer::reindex(db.conn_mut(), &self.cfg.vault, &self.exclude, &source, &mut |_, _| {})?;
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
                indexer::reindex(db.conn_mut(), &self.cfg.vault, &self.exclude, &source, &mut |_, _| {})?;
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

    /// Reads the document, applies `change` to its LF text, writes it atomically and reindexes.
    fn write_document(&self, path: &str, change: impl FnOnce(&str) -> Result<String>) -> Result<WriteResponse> {
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
        let _guard = self.write_lock.lock().unwrap_or_else(|e| e.into_inner());
        let loaded = Loaded::read(&abs)?;
        let updated = change(&loaded.lf)?;
        let diff = write::diff_hunk(&loaded.lf, &updated);
        write::atomic_replace(&abs, &loaded.bytes, &loaded.render(&updated))?;
        let reindexed = matches!(self.reindex_incremental(), Ok(o) if o.reindexed);
        Ok(WriteResponse { ok: true, path: path.to_string(), diff, reindexed })
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

    pub fn list_properties(&self, path_prefix: Option<&str>, key: Option<&str>) -> Result<PropertiesResponse> {
        let db = self.open_for_read()?;
        documents::list_properties(db.conn(), path_prefix, key)
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
