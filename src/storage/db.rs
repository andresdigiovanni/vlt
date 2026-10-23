use std::collections::HashMap;
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection, OptionalExtension};

use crate::error::{Result, VltError};
use crate::indexer::parser::{Heading, RawLink};
use crate::ml::profile::LEGACY_FINGERPRINT;

pub const SCHEMA_VERSION: &str = "2";

#[derive(Debug, Clone, PartialEq)]
pub struct IndexMeta {
    pub model_id: String,
    pub dim: usize,
    /// Everything else that shapes the stored vectors (commit, pooling, document prefix…).
    pub fingerprint: String,
    pub vault: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FileState {
    pub mtime_ns: i64,
    pub size: i64,
    pub sha256: String,
}

pub struct FileRow {
    pub path: String,
    pub mtime_ns: i64,
    pub size: i64,
    pub sha256: String,
    pub title: String,
    pub properties: Option<serde_json::Map<String, serde_json::Value>>,
    pub frontmatter_error: Option<String>,
}

pub struct ChunkRow {
    pub chunk_id: String,
    pub section_id: String,
    pub heading_path: Vec<String>,
    pub start_line: usize,
    pub end_line: usize,
    pub content: String,
    pub fts_text: String,
    pub embedding: Vec<f32>,
}

const TABLES: [&str; 8] = [
    "meta",
    "files",
    "excluded_files",
    "chunks",
    "headings",
    "links",
    "chunks_fts",
    "chunk_vectors",
];

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS files (
    path TEXT PRIMARY KEY,
    mtime_ns INTEGER NOT NULL,
    size INTEGER NOT NULL,
    sha256 TEXT NOT NULL,
    indexed_at INTEGER NOT NULL,
    title TEXT NOT NULL,
    properties_json TEXT NULL,
    frontmatter_error TEXT NULL
);
CREATE TABLE IF NOT EXISTS excluded_files (path TEXT PRIMARY KEY);
CREATE TABLE IF NOT EXISTS chunks (
    id INTEGER PRIMARY KEY,
    chunk_id TEXT NOT NULL,
    section_id TEXT NOT NULL,
    path TEXT NOT NULL,
    heading_path_json TEXT NOT NULL,
    start_line INTEGER NOT NULL,
    end_line INTEGER NOT NULL,
    content TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS chunks_path ON chunks(path);
CREATE INDEX IF NOT EXISTS chunks_chunk_id ON chunks(chunk_id);
CREATE TABLE IF NOT EXISTS headings (
    path TEXT NOT NULL,
    level INTEGER NOT NULL,
    text TEXT NOT NULL,
    heading_path_json TEXT NOT NULL,
    line INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS headings_path ON headings(path);
CREATE TABLE IF NOT EXISTS links (
    src_path TEXT NOT NULL,
    target TEXT NOT NULL,
    heading TEXT NULL,
    raw TEXT NOT NULL,
    line INTEGER NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('wiki', 'embed', 'markdown'))
);
CREATE INDEX IF NOT EXISTS links_src_path ON links(src_path);
CREATE VIRTUAL TABLE IF NOT EXISTS chunks_fts USING fts5(text, tokenize='porter unicode61');
CREATE TABLE IF NOT EXISTS chunk_vectors (chunk_rowid INTEGER PRIMARY KEY, embedding BLOB NOT NULL);
";

pub fn register_sqlite_vec() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| unsafe {
        #[allow(clippy::missing_transmute_annotations)]
        rusqlite::ffi::sqlite3_auto_extension(Some(std::mem::transmute(
            sqlite_vec::sqlite3_vec_init as *const (),
        )));
    });
}

pub struct Db {
    conn: Connection,
}

impl Db {
    pub fn open(path: &Path, expected: &IndexMeta) -> Result<Db> {
        let mut db = Db::connect(path)?;
        if meta_is_empty(&db.conn)? {
            let tx = db.conn.transaction()?;
            tx.execute_batch(SCHEMA)?;
            write_meta(&tx, expected)?;
            tx.commit()?;
        } else {
            check_meta(&db.conn, expected)?;
        }
        Ok(db)
    }

    pub fn open_unchecked(path: &Path) -> Result<Db> {
        let db = Db::connect(path)?;
        db.conn.execute_batch(SCHEMA)?;
        Ok(db)
    }

    pub fn exists(path: &Path) -> bool {
        path.is_file()
    }

    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    pub fn conn_mut(&mut self) -> &mut Connection {
        &mut self.conn
    }

    fn connect(path: &Path) -> Result<Db> {
        register_sqlite_vec();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        conn.busy_timeout(Duration::from_millis(5000))?;
        conn.query_row("PRAGMA journal_mode=WAL", [], |_| Ok(()))?;
        conn.execute_batch("PRAGMA foreign_keys=OFF")?;
        Ok(Db { conn })
    }
}

fn meta_is_empty(conn: &Connection) -> Result<bool> {
    let has_table: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'meta')",
        [],
        |r| r.get(0),
    )?;
    if !has_table {
        return Ok(true);
    }
    let rows: i64 = conn.query_row("SELECT count(*) FROM meta", [], |r| r.get(0))?;
    Ok(rows == 0)
}

fn meta_value(conn: &Connection, key: &str) -> Result<Option<String>> {
    Ok(conn
        .query_row("SELECT value FROM meta WHERE key = ?1", [key], |r| r.get(0))
        .optional()?)
}

fn mismatch(key: &str, found: Option<&str>, expected: &str) -> VltError {
    match found {
        Some(found) => VltError::IndexRebuildRequired(format!(
            "{key} differs: index has `{found}`, configuration has `{expected}`"
        )),
        None => VltError::IndexRebuildRequired(format!("{key} is missing from the index")),
    }
}

/// The stored fingerprint; indexes from vlt 0.1.0 have none, and could only have been built
/// with what `LEGACY_FINGERPRINT` describes.
fn fingerprint_value(conn: &Connection) -> Result<String> {
    Ok(meta_value(conn, "fingerprint")?.unwrap_or_else(|| LEGACY_FINGERPRINT.to_string()))
}

fn check_meta(conn: &Connection, expected: &IndexMeta) -> Result<()> {
    let dim = expected.dim.to_string();
    let wanted = [
        ("schema_version", SCHEMA_VERSION),
        ("model_id", expected.model_id.as_str()),
        ("dim", dim.as_str()),
        ("vault", expected.vault.as_str()),
    ];
    for (key, want) in wanted {
        let found = meta_value(conn, key)?;
        if found.as_deref() != Some(want) {
            return Err(mismatch(key, found.as_deref(), want));
        }
    }
    let found = fingerprint_value(conn)?;
    if found != expected.fingerprint {
        return Err(mismatch("fingerprint", Some(&found), &expected.fingerprint));
    }
    Ok(())
}

fn write_meta(conn: &Connection, meta: &IndexMeta) -> Result<()> {
    let dim = meta.dim.to_string();
    let rows = [
        ("schema_version", SCHEMA_VERSION),
        ("model_id", meta.model_id.as_str()),
        ("dim", dim.as_str()),
        ("fingerprint", meta.fingerprint.as_str()),
        ("vault", meta.vault.as_str()),
    ];
    for (key, value) in rows {
        conn.execute(
            "INSERT OR REPLACE INTO meta (key, value) VALUES (?1, ?2)",
            params![key, value],
        )?;
    }
    Ok(())
}

pub fn read_meta(conn: &Connection) -> Result<Option<IndexMeta>> {
    if meta_is_empty(conn)? {
        return Ok(None);
    }
    let get = |key: &str| -> Result<String> {
        meta_value(conn, key)?.ok_or_else(|| mismatch(key, None, ""))
    };
    let dim_text = get("dim")?;
    let dim = dim_text.parse::<usize>().map_err(|_| {
        VltError::IndexRebuildRequired(format!("dim is not a number in the index: `{dim_text}`"))
    })?;
    Ok(Some(IndexMeta { model_id: get("model_id")?, dim, fingerprint: fingerprint_value(conn)?, vault: get("vault")? }))
}

pub fn reset_schema(conn: &Connection, meta: &IndexMeta) -> Result<()> {
    for table in TABLES {
        conn.execute_batch(&format!("DROP TABLE IF EXISTS {table}"))?;
    }
    conn.execute_batch(SCHEMA)?;
    write_meta(conn, meta)
}

pub fn file_states(conn: &Connection) -> Result<HashMap<String, FileState>> {
    let mut stmt = conn.prepare("SELECT path, mtime_ns, size, sha256 FROM files")?;
    let rows = stmt.query_map([], |r| {
        Ok((r.get::<_, String>(0)?, FileState { mtime_ns: r.get(1)?, size: r.get(2)?, sha256: r.get(3)? }))
    })?;
    let mut states = HashMap::new();
    for row in rows {
        let (path, state) = row?;
        states.insert(path, state);
    }
    Ok(states)
}

fn to_json<T: serde::Serialize>(value: &T) -> Result<String> {
    serde_json::to_string(value).map_err(|e| VltError::Internal(e.to_string()))
}

pub fn put_file(
    conn: &Connection,
    file: &FileRow,
    chunks: &[ChunkRow],
    headings: &[Heading],
    links: &[RawLink],
) -> Result<()> {
    delete_file(conn, &file.path)?;
    let indexed_at = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
    let properties_json = file.properties.as_ref().map(to_json).transpose()?;
    conn.execute(
        "INSERT INTO files (path, mtime_ns, size, sha256, indexed_at, title, properties_json, frontmatter_error)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            file.path,
            file.mtime_ns,
            file.size,
            file.sha256,
            indexed_at,
            file.title,
            properties_json,
            file.frontmatter_error
        ],
    )?;
    let mut insert_chunk = conn.prepare(
        "INSERT INTO chunks (chunk_id, section_id, path, heading_path_json, start_line, end_line, content)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
    )?;
    let mut insert_fts = conn.prepare("INSERT INTO chunks_fts (rowid, text) VALUES (?1, ?2)")?;
    let mut insert_vec = conn.prepare("INSERT INTO chunk_vectors (chunk_rowid, embedding) VALUES (?1, ?2)")?;
    for chunk in chunks {
        insert_chunk.execute(params![
            chunk.chunk_id,
            chunk.section_id,
            file.path,
            to_json(&chunk.heading_path)?,
            chunk.start_line as i64,
            chunk.end_line as i64,
            chunk.content
        ])?;
        let rowid = conn.last_insert_rowid();
        insert_fts.execute(params![rowid, chunk.fts_text])?;
        insert_vec.execute(params![rowid, f32_blob(&chunk.embedding)])?;
    }
    let mut insert_heading = conn.prepare(
        "INSERT INTO headings (path, level, text, heading_path_json, line) VALUES (?1, ?2, ?3, ?4, ?5)",
    )?;
    for h in headings {
        insert_heading.execute(params![file.path, h.level, h.text, to_json(&h.heading_path)?, h.line as i64])?;
    }
    let mut insert_link = conn.prepare(
        "INSERT INTO links (src_path, target, heading, raw, line, kind) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
    )?;
    for l in links {
        insert_link.execute(params![file.path, l.target, l.heading, l.raw, l.line as i64, l.kind.as_str()])?;
    }
    Ok(())
}

pub fn delete_file(conn: &Connection, path: &str) -> Result<()> {
    conn.execute(
        "DELETE FROM chunk_vectors WHERE chunk_rowid IN (SELECT id FROM chunks WHERE path = ?1)",
        [path],
    )?;
    conn.execute(
        "DELETE FROM chunks_fts WHERE rowid IN (SELECT id FROM chunks WHERE path = ?1)",
        [path],
    )?;
    conn.execute("DELETE FROM chunks WHERE path = ?1", [path])?;
    conn.execute("DELETE FROM headings WHERE path = ?1", [path])?;
    conn.execute("DELETE FROM links WHERE src_path = ?1", [path])?;
    conn.execute("DELETE FROM files WHERE path = ?1", [path])?;
    Ok(())
}

pub fn set_mtime(conn: &Connection, path: &str, mtime_ns: i64) -> Result<()> {
    conn.execute("UPDATE files SET mtime_ns = ?1 WHERE path = ?2", params![mtime_ns, path])?;
    Ok(())
}

pub fn set_excluded(conn: &Connection, paths: &[String]) -> Result<()> {
    conn.execute("DELETE FROM excluded_files", [])?;
    let mut insert = conn.prepare("INSERT OR IGNORE INTO excluded_files (path) VALUES (?1)")?;
    for p in paths {
        insert.execute([p])?;
    }
    Ok(())
}

pub fn f32_blob(v: &[f32]) -> Vec<u8> {
    v.iter().flat_map(|x| x.to_le_bytes()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::indexer::parser::{Heading, LinkKind, RawLink};
    use tempfile::tempdir;

    fn meta() -> IndexMeta { IndexMeta { model_id: "fake".into(), dim: 2, fingerprint: "fake".into(), vault: "/v".into() } }
    fn file(path: &str) -> FileRow {
        FileRow { path: path.into(), mtime_ns: 1, size: 2, sha256: "h".into(), title: "T".into(),
                  properties: Some(serde_json::json!({"tags": ["a"]}).as_object().unwrap().clone()), frontmatter_error: None }
    }
    fn chunk(id: &str, text: &str, v: [f32; 2]) -> ChunkRow {
        ChunkRow { chunk_id: id.into(), section_id: id.into(), heading_path: vec!["H".into()], start_line: 1, end_line: 2,
                   content: text.into(), fts_text: text.into(), embedding: v.to_vec() }
    }
    fn count(c: &Connection, table: &str) -> i64 { c.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0)).unwrap() }

    #[test]
    fn open_creates_and_checks_meta() {
        let d = tempdir().unwrap();
        let p = d.path().join("vlt.db");
        assert!(!Db::exists(&p));
        Db::open(&p, &meta()).unwrap();
        assert!(Db::exists(&p));
        Db::open(&p, &meta()).unwrap();
        let mut other = meta(); other.vault = "/w".into();
        assert!(matches!(Db::open(&p, &other), Err(VltError::IndexRebuildRequired(_))));
        let mut other = meta(); other.dim = 3;
        assert!(matches!(Db::open(&p, &other), Err(VltError::IndexRebuildRequired(_))));
        assert_eq!(read_meta(Db::open_unchecked(&p).unwrap().conn()).unwrap(), Some(meta()));
    }

    #[test]
    fn meta_mismatch_names_the_key() {
        let d = tempdir().unwrap();
        let p = d.path().join("vlt.db");
        Db::open(&p, &meta()).unwrap();
        let msg = |m: &IndexMeta| match Db::open(&p, m) {
            Err(VltError::IndexRebuildRequired(s)) => s,
            other => panic!("expected IndexRebuildRequired, got {:?}", other.map(|_| ())),
        };
        let mut m = meta(); m.model_id = "real".into();
        assert!(msg(&m).starts_with("model_id"), "{}", msg(&m));
        let mut m = meta(); m.dim = 3;
        assert!(msg(&m).starts_with("dim"), "{}", msg(&m));
        let mut m = meta(); m.vault = "/w".into();
        assert!(msg(&m).starts_with("vault"), "{}", msg(&m));
        let mut m = meta(); m.fingerprint = "other".into();
        assert!(msg(&m).starts_with("fingerprint"), "{}", msg(&m));
        Db::open_unchecked(&p).unwrap().conn().execute("UPDATE meta SET value = '0' WHERE key = 'schema_version'", []).unwrap();
        assert!(msg(&meta()).starts_with("schema_version"), "{}", msg(&meta()));
    }

    #[test]
    fn an_index_without_a_fingerprint_was_built_by_0_1_0() {
        let d = tempdir().unwrap();
        let p = d.path().join("vlt.db");
        Db::open(&p, &meta()).unwrap();
        Db::open_unchecked(&p).unwrap().conn().execute("DELETE FROM meta WHERE key = 'fingerprint'", []).unwrap();
        let legacy = IndexMeta { fingerprint: LEGACY_FINGERPRINT.into(), ..meta() };
        Db::open(&p, &legacy).unwrap();
        assert_eq!(read_meta(Db::open_unchecked(&p).unwrap().conn()).unwrap(), Some(legacy));
        assert!(matches!(Db::open(&p, &meta()), Err(VltError::IndexRebuildRequired(_))));
    }

    #[test]
    fn put_file_replaces_rows_and_indexes_fts_and_vectors() {
        let d = tempdir().unwrap();
        let db = Db::open(&d.path().join("vlt.db"), &meta()).unwrap();
        let c = db.conn();
        let h = vec![Heading { level: 1, text: "H".into(), heading_path: vec!["H".into()], line: 1 }];
        let l = vec![RawLink { kind: LinkKind::Wiki, target: "b".into(), heading: None, raw: "[[b]]".into(), line: 2 }];
        put_file(c, &file("a.md"), &[chunk("a.md#H", "sqlite wal mode", [1.0, 0.0]), chunk("a.md#I", "other", [0.0, 1.0])], &h, &l).unwrap();
        assert_eq!((count(c, "chunks"), count(c, "chunk_vectors"), count(c, "headings"), count(c, "links")), (2, 2, 1, 1));
        let hit: String = c.query_row(
            "SELECT ch.chunk_id FROM chunks_fts f JOIN chunks ch ON ch.id = f.rowid WHERE chunks_fts MATCH '\"modes\"'", [], |r| r.get(0)).unwrap();
        assert_eq!(hit, "a.md#H", "porter stemming matches modes/mode");
        let dist: f64 = c.query_row(
            "SELECT vec_distance_cosine(v.embedding, ?1) FROM chunk_vectors v JOIN chunks ch ON ch.id = v.chunk_rowid WHERE ch.chunk_id = 'a.md#H'",
            [f32_blob(&[1.0, 0.0])], |r| r.get(0)).unwrap();
        assert!(dist.abs() < 1e-6);

        put_file(c, &file("a.md"), &[chunk("a.md#H", "new", [1.0, 0.0])], &[], &[]).unwrap();
        assert_eq!((count(c, "chunks"), count(c, "chunks_fts"), count(c, "chunk_vectors"), count(c, "headings"), count(c, "links")), (1, 1, 1, 0, 0));
        delete_file(c, "a.md").unwrap();
        assert_eq!((count(c, "files"), count(c, "chunks"), count(c, "chunks_fts"), count(c, "chunk_vectors")), (0, 0, 0, 0));
    }

    #[test]
    fn put_file_stores_every_column() {
        let d = tempdir().unwrap();
        let db = Db::open(&d.path().join("vlt.db"), &meta()).unwrap();
        let c = db.conn();
        // A stray FTS row makes FTS auto-rowids diverge from chunks.id: the insert must set rowid explicitly.
        c.execute("INSERT INTO chunks_fts (rowid, text) VALUES (100, 'stray')", []).unwrap();
        let mut f = file("a.md");
        f.frontmatter_error = Some("bad yaml".into());
        let mut ch = chunk("a.md#H", "body", [1.0, 0.0]);
        ch.fts_text = "T > H body".into();
        let h = vec![Heading { level: 2, text: "H".into(), heading_path: vec!["T".into(), "H".into()], line: 3 }];
        let l = vec![RawLink { kind: LinkKind::Embed, target: "img.png".into(), heading: Some("x".into()), raw: "![[img.png#x]]".into(), line: 4 }];
        put_file(c, &f, &[ch], &h, &l).unwrap();

        let row: (String, Option<String>, Option<String>) = c.query_row(
            "SELECT title, properties_json, frontmatter_error FROM files WHERE path = 'a.md'", [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).unwrap();
        assert_eq!(row, ("T".into(), Some(r#"{"tags":["a"]}"#.into()), Some("bad yaml".into())));
        let row: (String, String, i64, i64, String, String) = c.query_row(
            "SELECT ch.path, ch.heading_path_json, ch.start_line, ch.end_line, ch.content, f.text
             FROM chunks ch JOIN chunks_fts f ON f.rowid = ch.id WHERE ch.chunk_id = 'a.md#H'", [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?))).unwrap();
        assert_eq!(row, ("a.md".into(), r#"["H"]"#.into(), 1, 2, "body".into(), "T > H body".into()));
        let row: (String, i64, String, String, i64) = c.query_row(
            "SELECT path, level, text, heading_path_json, line FROM headings", [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))).unwrap();
        assert_eq!(row, ("a.md".into(), 2, "H".into(), r#"["T","H"]"#.into(), 3));
        let row: (String, String, Option<String>, String, i64, String) = c.query_row(
            "SELECT src_path, target, heading, raw, line, kind FROM links", [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?))).unwrap();
        assert_eq!(row, ("a.md".into(), "img.png".into(), Some("x".into()), "![[img.png#x]]".into(), 4, "embed".into()));
        assert_eq!(LinkKind::from_str(&row.5), Some(LinkKind::Embed));
    }

    #[test]
    fn two_files_may_share_a_chunk_id() {
        let d = tempdir().unwrap();
        let db = Db::open(&d.path().join("vlt.db"), &meta()).unwrap();
        let c = db.conn();
        put_file(c, &file("x.md"), &[chunk("x.md#y.md#z", "one", [1.0, 0.0])], &[], &[]).unwrap();
        put_file(c, &file("x.md#y.md"), &[chunk("x.md#y.md#z", "two", [0.0, 1.0])], &[], &[]).unwrap();
        let paths: Vec<String> = c.prepare("SELECT path FROM chunks WHERE chunk_id = 'x.md#y.md#z' ORDER BY path").unwrap()
            .query_map([], |r| r.get(0)).unwrap().map(|r| r.unwrap()).collect();
        assert_eq!(paths, vec!["x.md".to_string(), "x.md#y.md".to_string()]);
    }

    #[test]
    fn busy_timeout_is_set_on_open() {
        let d = tempdir().unwrap();
        let db = Db::open(&d.path().join("vlt.db"), &meta()).unwrap();
        let ms: i64 = db.conn().query_row("PRAGMA busy_timeout", [], |r| r.get(0)).unwrap();
        assert_eq!(ms, 5000);
    }

    #[test]
    fn file_states_mtime_and_excluded() {
        let d = tempdir().unwrap();
        let db = Db::open(&d.path().join("vlt.db"), &meta()).unwrap();
        put_file(db.conn(), &file("a.md"), &[], &[], &[]).unwrap();
        set_mtime(db.conn(), "a.md", 99).unwrap();
        assert_eq!(file_states(db.conn()).unwrap()["a.md"], FileState { mtime_ns: 99, size: 2, sha256: "h".into() });
        set_excluded(db.conn(), &["t/x.md".into()]).unwrap();
        set_excluded(db.conn(), &["t/y.md".into()]).unwrap();
        let ex: Vec<String> = db.conn().prepare("SELECT path FROM excluded_files").unwrap()
            .query_map([], |r| r.get(0)).unwrap().map(|r| r.unwrap()).collect();
        assert_eq!(ex, vec!["t/y.md".to_string()]);
    }

    #[test]
    fn reset_schema_in_uncommitted_transaction_keeps_old_index() {
        let d = tempdir().unwrap();
        let mut db = Db::open(&d.path().join("vlt.db"), &meta()).unwrap();
        put_file(db.conn(), &file("a.md"), &[], &[], &[]).unwrap();
        {
            let tx = db.conn_mut().transaction().unwrap();
            let mut m = meta(); m.dim = 4;
            reset_schema(&tx, &m).unwrap();
            assert_eq!(count(&tx, "files"), 0);
            assert_eq!(read_meta(&tx).unwrap(), Some(m));
        } // dropped → rollback
        assert_eq!(count(db.conn(), "files"), 1);
        assert_eq!(read_meta(db.conn()).unwrap(), Some(meta()));
    }
}
