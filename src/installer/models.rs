use std::collections::BTreeMap;
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;

use serde::{Deserialize, Serialize};
use sha1::Sha1;
use sha2::{Digest, Sha256};
use ureq::tls::{RootCerts, TlsConfig};
use ureq::unversioned::resolver::DefaultResolver;
use ureq::unversioned::transport::{Buffers, ConnectionDetails, Connector, DefaultConnector, NextTimeout, Transport};

use crate::ml::profile::model_dir;
use crate::{Result, VltError};

pub const HF_BASE: &str = "https://huggingface.co";
/// Written last by `pull`, so its presence means the download completed.
const LOCK_FILE: &str = "vlt-lock.json";
/// Repo files vlt reads besides the weights; the first two are required.
const METADATA: [&str; 5] =
    ["config.json", "tokenizer.json", "modules.json", "config_sentence_transformers.json", "sentence_bert_config.json"];
const WEIGHTS: &str = "model.safetensors";
const SHARD_INDEX: &str = "model.safetensors.index.json";
/// How long a download may go without sending or receiving a byte before it fails.
const IDLE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);

/// A completed download: the revision the config asked for, the commit it resolved to, and the
/// sha256 of every file. The model stays at that commit until the config names another revision.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Lock {
    pub revision: String,
    pub commit: String,
    pub files: BTreeMap<String, String>,
}

/// The lock in `dir`; None when absent or unreadable, which callers treat as not downloaded.
pub fn read_lock(dir: &Path) -> Option<Lock> {
    let text = std::fs::read_to_string(dir.join(LOCK_FILE)).ok()?;
    serde_json::from_str(&text).ok()
}

pub(crate) fn write_lock(dir: &Path, lock: &Lock) -> Result<()> {
    let tmp = dir.join(format!("{LOCK_FILE}.tmp"));
    let text = serde_json::to_string_pretty(lock).map_err(|e| VltError::Internal(e.to_string()))?;
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, dir.join(LOCK_FILE))?;
    Ok(())
}

/// True when `id` at `revision` is not fully downloaded: no lock for that revision, or a
/// locked file is absent.
pub fn missing(models_dir: &Path, id: &str, revision: &str) -> bool {
    let dir = model_dir(models_dir, id);
    match read_lock(&dir) {
        Some(lock) => lock.revision != revision || lock.files.keys().any(|f| !dir.join(f).is_file()),
        None => true,
    }
}

#[derive(Deserialize)]
struct RepoInfo {
    sha: String,
    siblings: Vec<Sibling>,
}

#[derive(Deserialize)]
struct Sibling {
    rfilename: String,
    #[serde(rename = "blobId")]
    blob_id: Option<String>,
    lfs: Option<Lfs>,
}

#[derive(Deserialize)]
struct Lfs {
    sha256: String,
}

/// The hash Hugging Face publishes for a file at a commit: sha256 for LFS files, the git blob
/// sha1 for the rest.
enum Expected<'a> {
    Sha256(&'a str),
    GitBlob(&'a str),
}

impl Sibling {
    fn expected(&self) -> Result<Expected<'_>> {
        match (&self.lfs, &self.blob_id) {
            (Some(lfs), _) => Ok(Expected::Sha256(&lfs.sha256)),
            (None, Some(blob)) => Ok(Expected::GitBlob(blob)),
            (None, None) => Err(VltError::Model(format!("Hugging Face published no hash for {}", self.rfilename))),
        }
    }
}

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn sha256_file(path: &Path) -> Result<String> {
    let mut f = File::open(path)?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 16];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(to_hex(&h.finalize()))
}

/// The id git gives the file's contents (`git hash-object`); only small, non-LFS files have one.
fn git_blob_sha1(path: &Path) -> Result<String> {
    let bytes = std::fs::read(path)?;
    let mut h = Sha1::new();
    h.update(format!("blob {}\0", bytes.len()));
    h.update(&bytes);
    Ok(to_hex(&h.finalize()))
}

/// The file's sha256 when it matches `expected`; `sha256` is the file's sha256 if already known.
fn verified(path: &Path, expected: &Expected, sha256: Option<String>) -> Result<Option<String>> {
    let sha256 = match sha256 {
        Some(s) => s,
        None => sha256_file(path)?,
    };
    let ok = match expected {
        Expected::Sha256(want) => sha256 == *want,
        Expected::GitBlob(want) => git_blob_sha1(path)? == *want,
    };
    Ok(ok.then_some(sha256))
}

/// ureq's transports wrapped so that no single read waits longer than `idle`; ureq's own
/// timeouts are total budgets, which a multi-GB download cannot have.
#[derive(Debug)]
struct IdleTimeout {
    inner: DefaultConnector,
    idle: std::time::Duration,
}

#[derive(Debug)]
struct IdleTransport {
    inner: Box<dyn Transport>,
    idle: std::time::Duration,
}

impl IdleTransport {
    fn capped(&self, timeout: NextTimeout) -> NextTimeout {
        NextTimeout { after: timeout.after.min(self.idle.into()), reason: timeout.reason }
    }
}

impl Connector<()> for IdleTimeout {
    type Out = IdleTransport;

    fn connect(&self, details: &ConnectionDetails, chained: Option<()>) -> std::result::Result<Option<IdleTransport>, ureq::Error> {
        Ok(self.inner.connect(details, chained)?.map(|inner| IdleTransport { inner, idle: self.idle }))
    }
}

impl Transport for IdleTransport {
    fn buffers(&mut self) -> &mut dyn Buffers {
        self.inner.buffers()
    }

    fn transmit_output(&mut self, amount: usize, timeout: NextTimeout) -> std::result::Result<(), ureq::Error> {
        self.inner.transmit_output(amount, timeout)
    }

    fn await_input(&mut self, timeout: NextTimeout) -> std::result::Result<bool, ureq::Error> {
        let timeout = self.capped(timeout);
        self.inner.await_input(timeout)
    }

    fn is_open(&mut self) -> bool {
        self.inner.is_open()
    }

    fn is_tls(&self) -> bool {
        self.inner.is_tls()
    }
}

fn new_agent(idle: std::time::Duration) -> ureq::Agent {
    // OS trust store, so TLS-inspecting corporate proxies are trusted like in a browser.
    let tls = TlsConfig::builder().root_certs(RootCerts::PlatformVerifier).build();
    // The connect budget covers the TCP connect, the TLS handshake and any proxy setup, which
    // run before the transport is wrapped.
    let config = ureq::Agent::config_builder().tls_config(tls).timeout_connect(Some(idle)).build();
    ureq::Agent::with_parts(config, IdleTimeout { inner: DefaultConnector::new(), idle }, DefaultResolver::default())
}

/// The commit `revision` of `id` points to and the files it holds.
fn repo_info(agent: &ureq::Agent, base_url: &str, id: &str, revision: &str) -> Result<RepoInfo> {
    let url = format!("{base_url}/api/models/{id}/revision/{}?blobs=true", revision.replace('/', "%2F"));
    let resp = agent.get(&url).call().map_err(|e| match e {
        ureq::Error::StatusCode(401 | 403 | 404) => VltError::Model(format!(
            "model {id} at revision `{revision}` was not found on Hugging Face (private and gated models are not supported); check [models] in config.toml"
        )),
        e => VltError::Model(format!("cannot reach Hugging Face for {id}: {e}")),
    })?;
    let text = resp.into_body().read_to_string().map_err(|e| VltError::Model(format!("cannot read the file list of {id}: {e}")))?;
    let info: RepoInfo =
        serde_json::from_str(&text).map_err(|e| VltError::Model(format!("unexpected file list for {id}: {e}")))?;
    if info.sha.len() != 40 || !info.sha.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(VltError::Model(format!("unexpected commit `{}` for {id}", info.sha)));
    }
    Ok(info)
}

/// `N_Pooling/config.json`, the sentence-transformers pooling settings.
fn is_pooling_config(name: &str) -> bool {
    name.split_once('/').is_some_and(|(dir, file)| {
        file == "config.json"
            && dir.split_once('_').is_some_and(|(n, rest)| rest == "Pooling" && !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
    })
}

/// The files of `info` vlt needs, except the shards a shard index lists.
fn select<'a>(info: &'a RepoInfo, id: &str) -> Result<Vec<&'a Sibling>> {
    let find = |name: &str| info.siblings.iter().find(|s| s.rfilename == name);
    let mut files = Vec::new();
    for name in &METADATA[..2] {
        files.push(find(name).ok_or_else(|| VltError::Model(format!("model {id} has no {name}, which vlt needs")))?);
    }
    files.extend(METADATA[2..].iter().filter_map(|name| find(name)));
    files.extend(info.siblings.iter().filter(|s| is_pooling_config(&s.rfilename)));
    let weights = find(WEIGHTS).or_else(|| find(SHARD_INDEX)).ok_or_else(|| {
        VltError::Model(format!("model {id} has no safetensors weights ({WEIGHTS}); vlt cannot load other weight formats"))
    })?;
    files.push(weights);
    Ok(files)
}

/// The shards the downloaded shard index lists: plain `.safetensors` names of the same commit.
fn shards<'a>(index: &Path, info: &'a RepoInfo, id: &str) -> Result<Vec<&'a Sibling>> {
    #[derive(Deserialize)]
    struct Index {
        weight_map: BTreeMap<String, String>,
    }
    let text = std::fs::read_to_string(index)?;
    let index: Index = serde_json::from_str(&text).map_err(|e| VltError::Model(format!("invalid {SHARD_INDEX} in {id}: {e}")))?;
    let mut names: Vec<&String> = index.weight_map.values().collect();
    names.sort();
    names.dedup();
    names
        .into_iter()
        .map(|name| {
            let plain = name.ends_with(".safetensors") && !name.contains(['/', '\\']) && !name.starts_with('.');
            info.siblings
                .iter()
                .find(|s| plain && s.rfilename == *name)
                .ok_or_else(|| VltError::Model(format!("{SHARD_INDEX} in {id} lists an unknown shard `{name}`")))
        })
        .collect()
}

/// Streams `url` into `part`, reporting progress; returns the sha256 of the body.
fn download(
    agent: &ureq::Agent,
    url: &str,
    part: &Path,
    name: &str,
    progress: &mut dyn FnMut(&str, u64, Option<u64>),
) -> Result<String> {
    let resp = agent.get(url).call().map_err(|e| VltError::Model(format!("download failed for {url}: {e}")))?;
    let body = resp.into_body();
    let total = body.content_length();
    let mut reader = body.into_reader();
    let mut out = File::create(part)?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 16];
    let mut done = 0u64;
    progress(name, 0, total);
    loop {
        let n = reader.read(&mut buf).map_err(|e| VltError::Model(format!("download failed for {url}: {e}")))?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
        out.write_all(&buf[..n])?;
        done += n as u64;
        progress(name, done, total);
    }
    out.sync_all()?;
    Ok(to_hex(&h.finalize()))
}

struct Fetcher<'a> {
    agent: ureq::Agent,
    base_url: &'a str,
    id: &'a str,
    commit: &'a str,
    dir: &'a Path,
}

impl Fetcher<'_> {
    /// Ensures `file` is in the model dir as published at the commit; returns its sha256 and
    /// whether it was downloaded. A present file that matches is kept.
    fn fetch(&self, file: &Sibling, progress: &mut dyn FnMut(&str, u64, Option<u64>)) -> Result<(String, bool)> {
        let name = &file.rfilename;
        let expected = file.expected()?;
        let path = self.dir.join(name);
        if path.is_file() {
            if let Some(sha256) = verified(&path, &expected, None)? {
                return Ok((sha256, false));
            }
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let part = self.dir.join(format!("{name}.part"));
        let url = format!("{}/{}/resolve/{}/{name}", self.base_url, self.id, self.commit);
        let checked = download(&self.agent, &url, &part, name, progress)
            .and_then(|sha256| verified(&part, &expected, Some(sha256)));
        match checked {
            Ok(Some(sha256)) => {
                std::fs::rename(&part, &path)?;
                Ok((sha256, true))
            }
            Ok(None) => {
                std::fs::remove_file(&part)?;
                Err(VltError::Model(format!("checksum mismatch for {}/{name}", self.id)))
            }
            Err(e) => {
                let _ = std::fs::remove_file(&part);
                Err(e)
            }
        }
    }
}

/// Downloads `id` at `revision` into the models dir, verifying every file against the hash
/// Hugging Face publishes for the resolved commit, and locks it. Files already present with
/// the right contents are kept. Returns true if anything was downloaded; does nothing (and
/// needs no network) when the model is already downloaded at that revision.
pub fn pull(
    models_dir: &Path,
    id: &str,
    revision: &str,
    base_url: &str,
    progress: &mut dyn FnMut(&str, u64, Option<u64>),
) -> Result<bool> {
    if !missing(models_dir, id, revision) {
        return Ok(false);
    }
    let dir = model_dir(models_dir, id);
    std::fs::create_dir_all(&dir)?;
    let agent = new_agent(IDLE_TIMEOUT);
    let info = repo_info(&agent, base_url, id, revision)?;
    let mut files = select(&info, id)?;
    // The files are about to change, so an old lock would vouch for files it no longer matches.
    match std::fs::remove_file(dir.join(LOCK_FILE)) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.into()),
        _ => {}
    }
    let fetcher = Fetcher { agent, base_url, id, commit: &info.sha, dir: &dir };
    let mut locked = BTreeMap::new();
    let mut downloaded = false;
    let mut next = 0;
    while let Some(file) = files.get(next).copied() {
        let (sha256, fetched) = fetcher.fetch(file, progress)?;
        locked.insert(file.rfilename.clone(), sha256);
        downloaded |= fetched;
        if file.rfilename == SHARD_INDEX {
            files.extend(shards(&dir.join(SHARD_INDEX), &info, id)?);
        }
        next += 1;
    }
    write_lock(&dir, &Lock { revision: revision.to_string(), commit: info.sha.clone(), files: locked })?;
    Ok(downloaded)
}

fn progress_text(id: &str, name: &str, done: u64, total: Option<u64>) -> String {
    let mb = |b: u64| b as f64 / 1_048_576.0;
    match total {
        Some(t) => format!("\r\x1b[K{id} {name}: {:.1}/{:.1} MB", mb(done), mb(t)),
        None => format!("\r\x1b[K{id} {name}: {:.1} MB", mb(done)),
    }
}

/// `pull` from Hugging Face with a progress line on stderr, ended by a newline once shown.
pub(crate) fn pull_showing_progress(models_dir: &Path, id: &str, revision: &str) -> Result<bool> {
    let mut shown = false;
    let downloaded = pull(models_dir, id, revision, HF_BASE, &mut |name, done, total| {
        eprint!("{}", progress_text(id, name, done, total));
        let _ = std::io::stderr().flush();
        shown = true;
    })?;
    if shown {
        eprintln!();
    }
    Ok(downloaded)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};

    const COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";
    // sha256("hello"), and the git blob ids of "hello" and `{"a":1}`
    const HELLO_SHA256: &str = "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824";
    const HELLO_BLOB: &str = "b6fc4c620b67d95f953a5c1c1230aaab5db5a1b0";
    const JSON_BLOB: &str = "daa5053ecf5f9a37b2de733d0751cc1ab53ac010";

    /// A fake Hugging Face: serves `routes` by request path (404 otherwise) and records every
    /// path requested.
    struct Hub {
        base: String,
        hits: Arc<Mutex<Vec<String>>>,
    }

    impl Hub {
        fn new(routes: Vec<(String, Vec<u8>)>) -> Hub {
            let l = TcpListener::bind("127.0.0.1:0").unwrap();
            let base = format!("http://{}", l.local_addr().unwrap());
            let hits = Arc::new(Mutex::new(Vec::new()));
            let seen = hits.clone();
            let routes: HashMap<String, Vec<u8>> = routes.into_iter().collect();
            std::thread::spawn(move || {
                for s in l.incoming() {
                    let mut s = s.unwrap();
                    let mut buf = [0u8; 4096];
                    let n = s.read(&mut buf).unwrap_or(0);
                    let req = String::from_utf8_lossy(&buf[..n]);
                    let path = req.split(' ').nth(1).unwrap_or_default().to_string();
                    seen.lock().unwrap().push(path.clone());
                    match routes.get(&path) {
                        Some(body) => {
                            write!(s, "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len()).unwrap();
                            s.write_all(body).unwrap();
                        }
                        None => write!(s, "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap(),
                    }
                }
            });
            Hub { base, hits }
        }

        /// A repo `id` at `COMMIT` for `revision`: `files` are (name, body, lfs) and are listed
        /// with their real hashes unless `hashes` overrides one.
        fn repo(id: &str, revision: &str, files: &[(&str, &[u8], bool)], hashes: &[(&str, &str)]) -> Hub {
            let d = tempfile::tempdir().unwrap();
            let siblings: Vec<serde_json::Value> = files
                .iter()
                .map(|(name, body, lfs)| {
                    let p = d.path().join("f");
                    std::fs::write(&p, body).unwrap();
                    let real = if *lfs { sha256_file(&p).unwrap() } else { git_blob_sha1(&p).unwrap() };
                    let hash = hashes.iter().find(|(n, _)| n == name).map(|(_, h)| h.to_string()).unwrap_or(real);
                    if *lfs {
                        serde_json::json!({"rfilename": name, "blobId": "x", "lfs": {"sha256": hash, "size": body.len()}})
                    } else {
                        serde_json::json!({"rfilename": name, "blobId": hash})
                    }
                })
                .collect();
            let info = serde_json::json!({"sha": COMMIT, "siblings": siblings});
            let mut routes = vec![(
                format!("/api/models/{id}/revision/{}?blobs=true", revision.replace('/', "%2F")),
                info.to_string().into_bytes(),
            )];
            routes.extend(files.iter().map(|(name, body, _)| (format!("/{id}/resolve/{COMMIT}/{name}"), body.to_vec())));
            Hub::new(routes)
        }

        fn downloads(&self) -> Vec<String> {
            self.hits.lock().unwrap().iter().filter(|p| p.contains("/resolve/")).cloned().collect()
        }
    }

    fn pull_from(models: &Path, id: &str, revision: &str, hub: &Hub) -> Result<bool> {
        pull(models, id, revision, &hub.base, &mut |_, _, _| {})
    }

    const BASIC: &[(&str, &[u8], bool)] = &[
        ("config.json", br#"{"a":1}"#, false),
        ("tokenizer.json", b"hello", false),
        ("model.safetensors", b"hello", true),
        ("README.md", b"hello", false),
        ("pytorch_model.bin", b"hello", true),
        ("onnx/model.onnx", b"hello", true),
        ("1_Pooling/config.json", b"hello", false),
        ("2_Dense/config.json", b"hello", false),
    ];

    #[test]
    fn pull_downloads_the_needed_files_verifies_them_and_locks_the_commit() {
        let d = tempfile::tempdir().unwrap();
        let hub = Hub::repo("o/m", "main", BASIC, &[]);
        assert!(missing(d.path(), "o/m", "main"));
        assert!(pull_from(d.path(), "o/m", "main", &hub).unwrap());
        let mut got = hub.downloads();
        got.sort();
        let want: Vec<String> = ["1_Pooling/config.json", "config.json", "model.safetensors", "tokenizer.json"]
            .iter()
            .map(|n| format!("/o/m/resolve/{COMMIT}/{n}"))
            .collect();
        assert_eq!(got, want, "README, other weight formats and unsupported modules are skipped");
        let dir = d.path().join("o--m");
        assert_eq!(std::fs::read(dir.join("1_Pooling/config.json")).unwrap(), b"hello");
        let lock = read_lock(&dir).unwrap();
        assert_eq!((lock.revision.as_str(), lock.commit.as_str()), ("main", COMMIT));
        assert_eq!(lock.files.keys().collect::<Vec<_>>(), ["1_Pooling/config.json", "config.json", "model.safetensors", "tokenizer.json"]);
        assert_eq!(lock.files["model.safetensors"], HELLO_SHA256);
        assert!(!missing(d.path(), "o/m", "main"));
        assert!(!pull(d.path(), "o/m", "main", "http://127.0.0.1:9", &mut |_, _, _| {}).unwrap(), "no network when locked");
    }

    #[test]
    fn checksum_mismatch_leaves_no_file_and_no_lock() {
        for bad in ["model.safetensors", "config.json"] {
            let d = tempfile::tempdir().unwrap();
            let hub = Hub::repo("o/m", "main", BASIC, &[(bad, "0000000000000000000000000000000000000000")]);
            let err = pull_from(d.path(), "o/m", "main", &hub).unwrap_err();
            assert!(err.to_string().contains(&format!("checksum mismatch for o/m/{bad}")), "{err}");
            let dir = d.path().join("o--m");
            assert!(!dir.join(bad).exists() && !dir.join(format!("{bad}.part")).exists());
            assert!(read_lock(&dir).is_none() && missing(d.path(), "o/m", "main"));
        }
    }

    #[test]
    fn present_files_are_kept_when_they_match_and_downloaded_again_when_not() {
        let d = tempfile::tempdir().unwrap();
        let dir = d.path().join("o--m");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("model.safetensors"), b"hello").unwrap();
        std::fs::write(dir.join("config.json"), b"junk").unwrap();
        let hub = Hub::repo("o/m", "main", BASIC, &[]);
        assert!(pull_from(d.path(), "o/m", "main", &hub).unwrap());
        assert!(!hub.downloads().iter().any(|p| p.ends_with("model.safetensors")), "a matching file is not downloaded again");
        assert_eq!(std::fs::read(dir.join("config.json")).unwrap(), br#"{"a":1}"#);
    }

    #[test]
    fn a_new_revision_is_pulled_again() {
        let d = tempfile::tempdir().unwrap();
        pull_from(d.path(), "o/m", "main", &Hub::repo("o/m", "main", BASIC, &[])).unwrap();
        assert!(missing(d.path(), "o/m", "refs/pr/3"));
        let hub = Hub::repo("o/m", "refs/pr/3", BASIC, &[]);
        assert!(!pull_from(d.path(), "o/m", "refs/pr/3", &hub).unwrap(), "same files: nothing downloaded");
        assert_eq!(read_lock(&d.path().join("o--m")).unwrap().revision, "refs/pr/3");
        assert!(!missing(d.path(), "o/m", "refs/pr/3"));
    }

    #[test]
    fn missing_when_a_locked_file_is_absent() {
        let d = tempfile::tempdir().unwrap();
        pull_from(d.path(), "o/m", "main", &Hub::repo("o/m", "main", BASIC, &[])).unwrap();
        std::fs::remove_file(d.path().join("o--m/tokenizer.json")).unwrap();
        assert!(missing(d.path(), "o/m", "main"));
    }

    #[test]
    fn sharded_weights_download_every_shard_of_the_index() {
        let index = br#"{"metadata":{},"weight_map":{"a":"model-1.safetensors","b":"model-2.safetensors","c":"model-1.safetensors"}}"#;
        let files: &[(&str, &[u8], bool)] = &[
            ("config.json", br#"{"a":1}"#, false),
            ("tokenizer.json", b"hello", false),
            ("model.safetensors.index.json", index, false),
            ("model-1.safetensors", b"one", true),
            ("model-2.safetensors", b"two", true),
        ];
        let d = tempfile::tempdir().unwrap();
        pull_from(d.path(), "o/m", "main", &Hub::repo("o/m", "main", files, &[])).unwrap();
        let lock = read_lock(&d.path().join("o--m")).unwrap();
        assert_eq!(
            lock.files.keys().collect::<Vec<_>>(),
            ["config.json", "model-1.safetensors", "model-2.safetensors", "model.safetensors.index.json", "tokenizer.json"]
        );
    }

    #[test]
    fn a_shard_index_cannot_point_outside_the_repo() {
        let index = br#"{"weight_map":{"a":"../evil.safetensors"}}"#;
        let files: &[(&str, &[u8], bool)] = &[
            ("config.json", b"{}", false),
            ("tokenizer.json", b"{}", false),
            ("model.safetensors.index.json", index, false),
            ("../evil.safetensors", b"x", true),
        ];
        let d = tempfile::tempdir().unwrap();
        let err = pull_from(d.path(), "o/m", "main", &Hub::repo("o/m", "main", files, &[])).unwrap_err();
        assert!(err.to_string().contains("unknown shard"), "{err}");
        assert!(!d.path().join("evil.safetensors").exists());
    }

    #[test]
    fn repos_vlt_cannot_load_fail_before_downloading() {
        let d = tempfile::tempdir().unwrap();
        let no_tokenizer: &[(&str, &[u8], bool)] = &[("config.json", b"{}", false), ("model.safetensors", b"x", true)];
        let hub = Hub::repo("o/m", "main", no_tokenizer, &[]);
        assert!(pull_from(d.path(), "o/m", "main", &hub).unwrap_err().to_string().contains("has no tokenizer.json"));
        let bin_only: &[(&str, &[u8], bool)] = &[("config.json", b"{}", false), ("tokenizer.json", b"{}", false), ("pytorch_model.bin", b"x", true)];
        let hub = Hub::repo("o/m", "main", bin_only, &[]);
        assert!(pull_from(d.path(), "o/m", "main", &hub).unwrap_err().to_string().contains("no safetensors weights"));
        assert!(hub.downloads().is_empty());
    }

    #[test]
    fn an_unknown_repo_says_so() {
        let d = tempfile::tempdir().unwrap();
        let err = pull_from(d.path(), "no/such", "main", &Hub::new(vec![])).unwrap_err().to_string();
        assert!(err.contains("no/such") && err.contains("not found on Hugging Face"), "{err}");
    }

    #[test]
    fn a_download_that_stops_sending_fails_instead_of_hanging() {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/m.bin", l.local_addr().unwrap());
        std::thread::spawn(move || {
            let (mut s, _) = l.accept().unwrap();
            let _ = s.read(&mut [0u8; 4096]);
            write!(s, "HTTP/1.1 200 OK\r\nContent-Length: 1000\r\n\r\nhello").unwrap();
            std::thread::sleep(std::time::Duration::from_secs(30));
        });
        let d = tempfile::tempdir().unwrap();
        let part = d.path().join("m.bin.part");
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let agent = new_agent(std::time::Duration::from_millis(300));
            let _ = tx.send(download(&agent, &url, &part, "m.bin", &mut |_, _, _| {}).map_err(|e| e.to_string()));
        });
        let got = rx.recv_timeout(std::time::Duration::from_secs(10)).expect("the download is still blocked on a silent server");
        let err = got.unwrap_err();
        assert!(err.contains("download failed") && err.contains("timeout"), "{err}");
    }

    #[test]
    fn a_server_that_never_answers_the_tls_handshake_fails_instead_of_hanging() {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("https://{}/m.bin", l.local_addr().unwrap());
        std::thread::spawn(move || {
            let (mut s, _) = l.accept().unwrap();
            let _ = s.read(&mut [0u8; 4096]);
            std::thread::sleep(std::time::Duration::from_secs(30));
        });
        let d = tempfile::tempdir().unwrap();
        let part = d.path().join("m.bin.part");
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let agent = new_agent(std::time::Duration::from_millis(300));
            let _ = tx.send(download(&agent, &url, &part, "m.bin", &mut |_, _, _| {}).map_err(|e| e.to_string()));
        });
        let got = rx.recv_timeout(std::time::Duration::from_secs(10)).expect("the download is still blocked in the TLS handshake");
        let err = got.unwrap_err();
        assert!(err.contains("download failed") && err.contains("timeout"), "{err}");
    }

    #[test]
    fn a_slow_download_that_keeps_sending_completes() {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/m.bin", l.local_addr().unwrap());
        std::thread::spawn(move || {
            let (mut s, _) = l.accept().unwrap();
            let _ = s.read(&mut [0u8; 4096]);
            write!(s, "HTTP/1.1 200 OK\r\nContent-Length: 5\r\nConnection: close\r\n\r\n").unwrap();
            for b in b"hello" {
                std::thread::sleep(std::time::Duration::from_millis(150));
                s.write_all(&[*b]).unwrap();
            }
        });
        let d = tempfile::tempdir().unwrap();
        let part = d.path().join("m.bin.part");
        let agent = new_agent(std::time::Duration::from_millis(300));
        assert_eq!(download(&agent, &url, &part, "m.bin", &mut |_, _, _| {}).unwrap(), HELLO_SHA256, "750 ms in all, no gap over 300 ms");
    }

    #[test]
    fn hashes_match_git_and_sha256() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("f");
        std::fs::write(&p, b"hello").unwrap();
        assert_eq!(git_blob_sha1(&p).unwrap(), HELLO_BLOB);
        assert_eq!(sha256_file(&p).unwrap(), HELLO_SHA256);
        std::fs::write(&p, br#"{"a":1}"#).unwrap();
        assert_eq!(git_blob_sha1(&p).unwrap(), JSON_BLOB);
    }

    #[test]
    fn pooling_configs_are_recognized() {
        assert!(is_pooling_config("1_Pooling/config.json") && is_pooling_config("12_Pooling/config.json"));
        for no in ["Pooling/config.json", "1_Pooling/other.json", "x_Pooling/config.json", "1_Dense/config.json", "1_Pooling/a/config.json"] {
            assert!(!is_pooling_config(no), "{no}");
        }
    }

    #[test]
    fn progress_text_clears_the_line_and_shows_megabytes_with_and_without_a_total() {
        assert_eq!(progress_text("o/m", "a.bin", 1_572_864, Some(3_145_728)), "\r\x1b[Ko/m a.bin: 1.5/3.0 MB");
        assert_eq!(progress_text("o/m", "a.bin", 524_288, None), "\r\x1b[Ko/m a.bin: 0.5 MB");
    }
}
