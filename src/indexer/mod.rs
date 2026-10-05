pub mod chunker;
pub mod parser;
pub mod scanner;

use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;

use globset::GlobSet;
use rusqlite::Connection;

use crate::error::{Result, VltError};
use crate::ml::Embedder;
use crate::storage::db::{self, ChunkRow, FileRow, IndexMeta};
use scanner::{sha256_hex, Scan, ScannedFile};

pub const MAX_TOKENS: usize = 512;

#[derive(Debug, Default, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ReindexStats {
    pub added: usize,
    pub updated: usize,
    pub removed: usize,
    pub unchanged: usize,
}

pub type EmbedderSource<'a> = &'a dyn Fn() -> Result<Arc<dyn Embedder>>;

/// Calls the source at most once, and only when an embedder is first needed.
struct LazyEmbedder<'a> {
    source: EmbedderSource<'a>,
    loaded: Option<Arc<dyn Embedder>>,
}

impl<'a> LazyEmbedder<'a> {
    fn new(source: EmbedderSource<'a>) -> Self {
        LazyEmbedder { source, loaded: None }
    }

    fn get(&mut self) -> Result<Arc<dyn Embedder>> {
        if let Some(e) = &self.loaded {
            return Ok(e.clone());
        }
        let e = (self.source)()?;
        self.loaded = Some(e.clone());
        Ok(e)
    }
}

struct PreparedFile {
    file: FileRow,
    chunks: Vec<ChunkRow>,
    doc: parser::ParsedDoc,
}

fn file_stem(rel_path: &str) -> &str {
    let name = rel_path.rsplit('/').next().unwrap_or(rel_path);
    Path::new(name).file_stem().and_then(|s| s.to_str()).unwrap_or(name)
}

/// Parses, chunks and embeds several files with one embedder call; touches no database.
/// `dim`: the expected vector length, when known.
fn prepare_many(
    files: Vec<(&ScannedFile, Vec<u8>, String)>,
    embedder: &mut LazyEmbedder,
    dim: Option<usize>,
) -> Result<Vec<PreparedFile>> {
    if files.is_empty() {
        return Ok(Vec::new());
    }
    let model = embedder.get()?;
    let mut parsed = Vec::with_capacity(files.len());
    for (scanned, bytes, sha) in files {
        let text = String::from_utf8_lossy(&bytes);
        let doc = parser::parse(&text, file_stem(&scanned.rel_path));
        let chunks = chunker::chunk(&scanned.rel_path, &doc, MAX_TOKENS, &|t: &str| model.count_tokens(t));
        parsed.push((scanned, sha, doc, chunks));
    }
    let texts: Vec<&str> = parsed.iter().flat_map(|p| p.3.iter().map(|c| c.embed_text.as_str())).collect();
    let n = texts.len();
    let vectors = if texts.is_empty() { Vec::new() } else { model.embed(&texts)? };
    if vectors.len() != n {
        return Err(VltError::Internal(format!("embedder returned {} vectors for {n} chunks", vectors.len())));
    }
    if let Some(dim) = dim {
        if let Some(v) = vectors.iter().find(|v| v.len() != dim) {
            return Err(VltError::Internal(format!(
                "embedder returned a {}-dimensional vector, index expects {dim}",
                v.len()
            )));
        }
    }
    let mut vectors = vectors.into_iter();
    let mut out = Vec::with_capacity(parsed.len());
    for (scanned, sha, doc, chunks) in parsed {
        let chunks = chunks
            .into_iter()
            .zip(vectors.by_ref())
            .map(|(c, embedding)| ChunkRow {
                chunk_id: c.chunk_id,
                heading_path: c.heading_path,
                start_line: c.start_line,
                end_line: c.end_line,
                content: c.content,
                fts_text: c.embed_text,
                embedding,
            })
            .collect();
        let file = FileRow {
            path: scanned.rel_path.clone(),
            mtime_ns: scanned.mtime_ns,
            size: scanned.size as i64,
            sha256: sha,
            title: doc.title.clone(),
            properties: doc.properties.clone(),
            frontmatter_error: doc.frontmatter_error.clone(),
        };
        out.push(PreparedFile { file, chunks, doc });
    }
    Ok(out)
}

/// Files parsed and embedded together, so the embedder sees larger length-sorted batches.
const FILES_PER_EMBED: usize = 16;

fn write(conn: &Connection, p: &PreparedFile) -> Result<()> {
    db::put_file(conn, &p.file, &p.chunks, &p.doc.headings, &p.doc.links)
}

/// `None` when the file vanished after the scan (e.g. an editor's atomic save).
fn read_scanned(f: &ScannedFile) -> Result<Option<Vec<u8>>> {
    match std::fs::read(&f.abs_path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Incremental reindex (PRD 3.1): only new or changed files are parsed and embedded.
pub fn reindex(
    conn: &mut Connection,
    vault: &Path,
    exclude: &GlobSet,
    embedder: EmbedderSource,
    progress: &mut dyn FnMut(usize, usize),
) -> Result<ReindexStats> {
    let scan = scanner::scan(vault, exclude)?;
    reindex_scan(conn, &scan, embedder, progress)
}

fn reindex_scan(
    conn: &mut Connection,
    scan: &Scan,
    embedder: EmbedderSource,
    progress: &mut dyn FnMut(usize, usize),
) -> Result<ReindexStats> {
    let states = db::file_states(conn)?;
    let mut stats = ReindexStats::default();
    let mut pending = Vec::new();
    for f in &scan.files {
        let state = states.get(&f.rel_path);
        if let Some(s) = state {
            if s.mtime_ns == f.mtime_ns && s.size == f.size as i64 {
                stats.unchanged += 1;
                continue;
            }
        }
        let Some(bytes) = read_scanned(f)? else {
            continue;
        };
        let sha = sha256_hex(&bytes);
        if let Some(s) = state {
            if s.sha256 == sha {
                db::set_mtime(conn, &f.rel_path, f.mtime_ns)?;
                stats.unchanged += 1;
                continue;
            }
        }
        pending.push((f, bytes, sha, state.is_none()));
    }

    let dim = db::read_meta(conn)?.map(|m| m.dim);
    let mut model = LazyEmbedder::new(embedder);
    let total = pending.len();
    let mut done = 0;
    let mut pending = pending.into_iter().peekable();
    while pending.peek().is_some() {
        let group: Vec<_> = pending.by_ref().take(FILES_PER_EMBED).collect();
        let flags: Vec<bool> = group.iter().map(|g| g.3).collect();
        let prepared = prepare_many(group.into_iter().map(|(f, b, s, _)| (f, b, s)).collect(), &mut model, dim)?;
        for (p, is_new) in prepared.iter().zip(flags) {
            let tx = conn.transaction()?;
            write(&tx, p)?;
            tx.commit()?;
            if is_new {
                stats.added += 1;
            } else {
                stats.updated += 1;
            }
            done += 1;
            progress(done, total);
        }
    }

    let present: HashSet<&str> = scan.files.iter().map(|f| f.rel_path.as_str()).collect();
    let tx = conn.transaction()?;
    for path in states.keys().filter(|p| !present.contains(p.as_str())) {
        db::delete_file(&tx, path)?;
        stats.removed += 1;
    }
    db::set_excluded(&tx, &scan.excluded)?;
    tx.commit()?;
    Ok(stats)
}

/// Full rebuild in one transaction: readers keep the old snapshot until commit.
pub fn rebuild(
    conn: &mut Connection,
    meta: &IndexMeta,
    vault: &Path,
    exclude: &GlobSet,
    embedder: EmbedderSource,
    progress: &mut dyn FnMut(usize, usize),
) -> Result<ReindexStats> {
    let scan = scanner::scan(vault, exclude)?;
    rebuild_scan(conn, meta, &scan, embedder, progress)
}

fn rebuild_scan(
    conn: &mut Connection,
    meta: &IndexMeta,
    scan: &Scan,
    embedder: EmbedderSource,
    progress: &mut dyn FnMut(usize, usize),
) -> Result<ReindexStats> {
    let mut model = LazyEmbedder::new(embedder);
    let tx = conn.transaction()?;
    db::reset_schema(&tx, meta)?;
    let total = scan.files.len();
    let mut added = 0;
    let mut done = 0;
    for files in scan.files.chunks(FILES_PER_EMBED) {
        let mut group = Vec::with_capacity(files.len());
        for f in files {
            if let Some(bytes) = read_scanned(f)? {
                let sha = sha256_hex(&bytes);
                group.push((f, bytes, sha));
            }
        }
        for p in prepare_many(group, &mut model, Some(meta.dim))? {
            write(&tx, &p)?;
            added += 1;
        }
        for _ in files {
            done += 1;
            progress(done, total);
        }
    }
    db::set_excluded(&tx, &scan.excluded)?;
    tx.commit()?;
    Ok(ReindexStats { added, ..Default::default() })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ml::fake::FakeEmbedder;
    use crate::storage::db::{read_meta, Db, IndexMeta};
    use std::path::PathBuf;
    use std::sync::{atomic::{AtomicUsize, Ordering}, Arc};
    use std::{fs, time::Duration};
    use tempfile::tempdir;

    fn meta() -> IndexMeta { IndexMeta { model_id: "fake".into(), dim: 384, vault: "/v".into() } }
    fn no_progress() -> impl FnMut(usize, usize) { |_, _| {} }
    fn count(c: &Connection, sql: &str) -> i64 { c.query_row(sql, [], |r| r.get(0)).unwrap() }

    struct Env { _d: tempfile::TempDir, vault: PathBuf, db: Db, calls: Arc<AtomicUsize> }
    fn env() -> Env {
        let d = tempdir().unwrap();
        let vault = d.path().join("vault");
        fs::create_dir_all(vault.join("notes")).unwrap();
        fs::write(vault.join("notes/a.md"), "---\ntags: [db]\n---\n# SQLite\n\nWAL mode allows readers.\n").unwrap();
        fs::write(vault.join("b.md"), "# Pasta\n\nBoil water.\n").unwrap();
        let db = Db::open(&d.path().join("vlt.db"), &meta()).unwrap();
        Env { _d: d, vault, db, calls: Arc::new(AtomicUsize::new(0)) }
    }
    fn run(e: &mut Env, exclude: &[&str]) -> ReindexStats {
        let calls = e.calls.clone();
        let src = move || -> Result<Arc<dyn Embedder>> { calls.fetch_add(1, Ordering::SeqCst); Ok(Arc::new(FakeEmbedder)) };
        let ex = scanner::build_exclude(&exclude.iter().map(|s| s.to_string()).collect::<Vec<_>>()).unwrap();
        reindex(e.db.conn_mut(), &e.vault, &ex, &src, &mut no_progress()).unwrap()
    }

    #[test]
    fn incremental_lifecycle() {
        let mut e = env();
        assert_eq!(run(&mut e, &[]), ReindexStats { added: 2, updated: 0, removed: 0, unchanged: 0 });
        assert_eq!(count(e.db.conn(), "SELECT count(*) FROM chunks"), 2);
        assert_eq!(count(e.db.conn(), "SELECT count(*) FROM chunks_fts WHERE chunks_fts MATCH '\"readers\"'"), 1);

        let before = e.calls.load(Ordering::SeqCst);
        assert_eq!(run(&mut e, &[]), ReindexStats { unchanged: 2, ..Default::default() });
        assert_eq!(e.calls.load(Ordering::SeqCst), before, "no model load when nothing changed");

        // same content, new mtime → unchanged, no embedding
        let f = fs::File::options().write(true).open(e.vault.join("b.md")).unwrap();
        f.set_modified(std::time::SystemTime::now() + Duration::from_secs(5)).unwrap();
        assert_eq!(run(&mut e, &[]), ReindexStats { unchanged: 2, ..Default::default() });
        assert_eq!(e.calls.load(Ordering::SeqCst), before);

        fs::write(e.vault.join("b.md"), "# Pasta\n\nBoil salted water.\n## Sauce\nTomato.\n").unwrap();
        assert_eq!(run(&mut e, &[]), ReindexStats { updated: 1, unchanged: 1, ..Default::default() });
        assert_eq!(count(e.db.conn(), "SELECT count(*) FROM chunks WHERE path = 'b.md'"), 2);

        fs::remove_file(e.vault.join("b.md")).unwrap();
        assert_eq!(run(&mut e, &[]), ReindexStats { removed: 1, unchanged: 1, ..Default::default() });
        assert_eq!(count(e.db.conn(), "SELECT count(*) FROM chunks WHERE path = 'b.md'"), 0);
    }

    #[test]
    fn exclude_removes_and_records_paths() {
        let mut e = env();
        run(&mut e, &[]);
        assert_eq!(run(&mut e, &["notes/**"]), ReindexStats { removed: 1, unchanged: 1, ..Default::default() });
        assert_eq!(count(e.db.conn(), "SELECT count(*) FROM excluded_files WHERE path = 'notes/a.md'"), 1);
        assert_eq!(run(&mut e, &[]), ReindexStats { added: 1, unchanged: 1, ..Default::default() });
    }

    #[test]
    fn invalid_frontmatter_is_indexed_with_error() {
        let mut e = env();
        fs::write(e.vault.join("bad.md"), "---\na: b: c\n---\n# Bad\n\ntext\n").unwrap();
        run(&mut e, &[]);
        let err: Option<String> = e.db.conn().query_row("SELECT frontmatter_error FROM files WHERE path='bad.md'", [], |r| r.get(0)).unwrap();
        assert!(err.is_some());
        assert_eq!(count(e.db.conn(), "SELECT count(*) FROM chunks WHERE path='bad.md'"), 1);
    }

    #[test]
    fn rebuild_resets_meta_and_indexes_everything() {
        let mut e = env();
        run(&mut e, &[]);
        let mut m = meta(); m.vault = "/other".into();
        let src = || -> Result<Arc<dyn Embedder>> { Ok(Arc::new(FakeEmbedder)) };
        let stats = rebuild(e.db.conn_mut(), &m, &e.vault, &scanner::build_exclude(&[]).unwrap(), &src, &mut no_progress()).unwrap();
        assert_eq!(stats, ReindexStats { added: 2, ..Default::default() });
        assert_eq!(read_meta(e.db.conn()).unwrap(), Some(m));
        assert_eq!(count(e.db.conn(), "SELECT count(*) FROM chunks"), 2);
    }

    fn counting_source(calls: &Arc<AtomicUsize>) -> impl Fn() -> Result<Arc<dyn Embedder>> {
        let calls = calls.clone();
        move || -> Result<Arc<dyn Embedder>> { calls.fetch_add(1, Ordering::SeqCst); Ok(Arc::new(FakeEmbedder)) }
    }

    #[test]
    fn embedder_is_loaded_once_per_run_and_progress_counts_indexed_files() {
        let mut e = env();
        let src = counting_source(&e.calls);
        let ex = scanner::build_exclude(&[]).unwrap();
        let mut seen = Vec::new();
        reindex(e.db.conn_mut(), &e.vault, &ex, &src, &mut |d, t| seen.push((d, t))).unwrap();
        assert_eq!(e.calls.load(Ordering::SeqCst), 1);
        assert_eq!(seen, vec![(1, 2), (2, 2)]);
        seen.clear();
        reindex(e.db.conn_mut(), &e.vault, &ex, &src, &mut |d, t| seen.push((d, t))).unwrap();
        assert!(seen.is_empty());
        rebuild(e.db.conn_mut(), &meta(), &e.vault, &ex, &src, &mut |d, t| seen.push((d, t))).unwrap();
        assert_eq!(seen, vec![(1, 2), (2, 2)]);
        assert_eq!(e.calls.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn rebuild_of_empty_vault_does_not_load_the_embedder() {
        let mut e = env();
        fs::remove_dir_all(&e.vault).unwrap();
        fs::create_dir_all(&e.vault).unwrap();
        let src = counting_source(&e.calls);
        let stats = rebuild(e.db.conn_mut(), &meta(), &e.vault, &scanner::build_exclude(&[]).unwrap(), &src, &mut no_progress()).unwrap();
        assert_eq!(stats, ReindexStats::default());
        assert_eq!(e.calls.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn size_change_with_same_mtime_is_reindexed() {
        let mut e = env();
        run(&mut e, &[]);
        let path = e.vault.join("b.md");
        let mtime = fs::metadata(&path).unwrap().modified().unwrap();
        fs::write(&path, "# Pasta\n\nBoil much more water.\n").unwrap();
        fs::File::options().write(true).open(&path).unwrap().set_modified(mtime).unwrap();
        assert_eq!(run(&mut e, &[]), ReindexStats { updated: 1, unchanged: 1, ..Default::default() });
    }

    #[test]
    fn same_content_touch_records_new_mtime() {
        let mut e = env();
        run(&mut e, &[]);
        let path = e.vault.join("b.md");
        let later = std::time::SystemTime::now() + Duration::from_secs(5);
        fs::File::options().write(true).open(&path).unwrap().set_modified(later).unwrap();
        run(&mut e, &[]);
        let want = later.duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos() as i64;
        let got: i64 = e.db.conn().query_row("SELECT mtime_ns FROM files WHERE path='b.md'", [], |r| r.get(0)).unwrap();
        assert_eq!(got, want);
    }

    #[test]
    fn title_falls_back_to_stem_without_md_extension() {
        let mut e = env();
        fs::write(e.vault.join("Plain Note.MD"), "just text\n").unwrap();
        run(&mut e, &[]);
        let title: String = e.db.conn().query_row("SELECT title FROM files WHERE path='Plain Note.MD'", [], |r| r.get(0)).unwrap();
        assert_eq!(title, "Plain Note");
    }

    #[test]
    fn files_vanishing_between_scan_and_read_are_skipped() {
        let mut e = env();
        run(&mut e, &[]);
        fs::write(e.vault.join("b.md"), "# Pasta\n\nBoil much more salted water.\n").unwrap();
        fs::write(e.vault.join("c.md"), "# New\n\ntext\n").unwrap();
        let scan = scanner::scan(&e.vault, &scanner::build_exclude(&[]).unwrap()).unwrap();
        fs::remove_file(e.vault.join("b.md")).unwrap();
        fs::remove_file(e.vault.join("c.md")).unwrap();
        let src = counting_source(&e.calls);
        let stats = reindex_scan(e.db.conn_mut(), &scan, &src, &mut no_progress()).unwrap();
        assert_eq!(stats, ReindexStats { unchanged: 1, ..Default::default() });
        assert_eq!(count(e.db.conn(), "SELECT count(*) FROM chunks WHERE path = 'b.md'"), 1, "old rows kept");
        assert_eq!(count(e.db.conn(), "SELECT count(*) FROM files WHERE path = 'c.md'"), 0);
    }

    #[test]
    fn rebuild_skips_files_vanishing_after_scan() {
        let mut e = env();
        let scan = scanner::scan(&e.vault, &scanner::build_exclude(&[]).unwrap()).unwrap();
        fs::remove_file(e.vault.join("b.md")).unwrap();
        let src = counting_source(&e.calls);
        let stats = rebuild_scan(e.db.conn_mut(), &meta(), &scan, &src, &mut no_progress()).unwrap();
        assert_eq!(stats, ReindexStats { added: 1, ..Default::default() });
        assert_eq!(count(e.db.conn(), "SELECT count(*) FROM files"), 1);
    }

    #[test]
    fn files_with_colliding_chunk_ids_both_index() {
        let mut e = env();
        fs::write(e.vault.join("x.md"), "# y.md#z\n\none\n").unwrap();
        fs::write(e.vault.join("x.md#y.md"), "# z\n\ntwo\n").unwrap();
        assert_eq!(run(&mut e, &[]), ReindexStats { added: 4, ..Default::default() });
        assert_eq!(count(e.db.conn(), "SELECT count(*) FROM chunks WHERE chunk_id = 'x.md#y.md#z'"), 2);
    }

    /// Returns vectors one element shorter than the index dim.
    struct ShortEmbedder;
    impl Embedder for ShortEmbedder {
        fn embed(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
            Ok(texts.iter().map(|_| vec![0.0; meta().dim - 1]).collect())
        }
        fn count_tokens(&self, text: &str) -> usize { FakeEmbedder.count_tokens(text) }
    }

    #[test]
    fn wrong_vector_dim_is_an_internal_error() {
        let mut e = env();
        let src = || -> Result<Arc<dyn Embedder>> { Ok(Arc::new(ShortEmbedder)) };
        let ex = scanner::build_exclude(&[]).unwrap();
        let is_dim_error = |r: Result<ReindexStats>| matches!(r, Err(VltError::Internal(m)) if m.contains("383") && m.contains("384"));
        assert!(is_dim_error(reindex(e.db.conn_mut(), &e.vault, &ex, &src, &mut no_progress())));
        assert!(is_dim_error(rebuild(e.db.conn_mut(), &meta(), &e.vault, &ex, &src, &mut no_progress())));
        assert_eq!(count(e.db.conn(), "SELECT count(*) FROM chunks"), 0);
    }

    /// Records how many texts each `embed` call received.
    #[derive(Default)]
    struct RecordingEmbedder { calls: std::sync::Mutex<Vec<usize>> }
    impl Embedder for RecordingEmbedder {
        fn embed(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
            self.calls.lock().unwrap().push(texts.len());
            FakeEmbedder.embed(texts)
        }
        fn count_tokens(&self, text: &str) -> usize { FakeEmbedder.count_tokens(text) }
    }

    fn vault_of(files: &[(String, String)]) -> (tempfile::TempDir, PathBuf, Db) {
        let d = tempdir().unwrap();
        let vault = d.path().join("vault");
        fs::create_dir_all(&vault).unwrap();
        for (name, body) in files {
            fs::write(vault.join(name), body).unwrap();
        }
        let db = Db::open(&d.path().join("vlt.db"), &meta()).unwrap();
        (d, vault, db)
    }

    #[test]
    fn the_embedder_gets_up_to_16_files_per_call() {
        let files: Vec<(String, String)> = (0..33).map(|i| (format!("n{i:02}.md"), format!("# T{i}\n\nbody {i}\n"))).collect();
        let (_d, vault, mut db) = vault_of(&files);
        let ex = scanner::build_exclude(&[]).unwrap();
        let rec = Arc::new(RecordingEmbedder::default());
        let r = rec.clone();
        let src = move || -> Result<Arc<dyn Embedder>> { Ok(r.clone()) };
        let mut seen = Vec::new();
        reindex(db.conn_mut(), &vault, &ex, &src, &mut |d, _| seen.push(d)).unwrap();
        assert_eq!(*rec.calls.lock().unwrap(), vec![16, 16, 1], "one chunk per file");
        assert_eq!(seen, (1..=33).collect::<Vec<_>>());
        rec.calls.lock().unwrap().clear();
        seen.clear();
        rebuild(db.conn_mut(), &meta(), &vault, &ex, &src, &mut |d, _| seen.push(d)).unwrap();
        assert_eq!(*rec.calls.lock().unwrap(), vec![16, 16, 1]);
        assert_eq!(seen, (1..=33).collect::<Vec<_>>());
    }

    #[test]
    fn grouped_files_get_their_own_chunk_vectors() {
        // Different section counts per file, so a misaligned vector would land on another chunk.
        let files: Vec<(String, String)> = (0..20)
            .map(|i| {
                let sections: String = (0..=i % 4).map(|s| format!("## S{s}\nword{i} topic{s} {}\n", "x ".repeat(i))).collect();
                (format!("n{i:02}.md"), format!("# Doc {i}\n\n{sections}"))
            })
            .collect();
        let (_d, vault, mut db) = vault_of(&files);
        let ex = scanner::build_exclude(&[]).unwrap();
        let src = || -> Result<Arc<dyn Embedder>> { Ok(Arc::new(FakeEmbedder)) };
        let check = |c: &Connection| {
            let mut stmt = c.prepare("SELECT f.text, v.embedding FROM chunks ch JOIN chunks_fts f ON f.rowid = ch.id JOIN chunk_vectors v ON v.chunk_rowid = ch.id").unwrap();
            let rows: Vec<(String, Vec<u8>)> = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?))).unwrap().map(|r| r.unwrap()).collect();
            assert!(rows.len() > 40);
            for (text, blob) in rows {
                assert_eq!(blob, db::f32_blob(&FakeEmbedder.embed(&[&text]).unwrap()[0]), "{text}");
            }
        };
        reindex(db.conn_mut(), &vault, &ex, &src, &mut no_progress()).unwrap();
        check(db.conn());
        rebuild(db.conn_mut(), &meta(), &vault, &ex, &src, &mut no_progress()).unwrap();
        check(db.conn());
    }

    #[test]
    fn other_read_errors_still_abort() {
        let mut e = env();
        let scan = scanner::scan(&e.vault, &scanner::build_exclude(&[]).unwrap()).unwrap();
        fs::remove_file(e.vault.join("b.md")).unwrap();
        fs::create_dir(e.vault.join("b.md")).unwrap();
        let src = counting_source(&e.calls);
        assert!(matches!(reindex_scan(e.db.conn_mut(), &scan, &src, &mut no_progress()), Err(VltError::Io(_))));
        assert!(matches!(rebuild_scan(e.db.conn_mut(), &meta(), &scan, &src, &mut no_progress()), Err(VltError::Io(_))));
    }
}
