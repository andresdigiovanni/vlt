use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;

use sha2::{Digest, Sha256};
use ureq::tls::{RootCerts, TlsConfig};

use crate::ml::registry::{model_dir, ModelSpec};
use crate::{Result, VltError};

pub const HF_BASE: &str = "https://huggingface.co";

/// True when any file of `spec` is absent.
pub fn missing(models_dir: &Path, spec: &ModelSpec) -> bool {
    let dir = model_dir(models_dir, spec.id);
    spec.files.iter().any(|(name, _)| !dir.join(name).is_file())
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

fn new_agent() -> ureq::Agent {
    // OS trust store, so TLS-inspecting corporate proxies are trusted like in a browser.
    let tls = TlsConfig::builder().root_certs(RootCerts::PlatformVerifier).build();
    ureq::Agent::config_builder().tls_config(tls).build().new_agent()
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

/// Downloads and verifies every file of `spec` not already present with the pinned hash.
/// Returns true if anything was downloaded.
pub fn pull(models_dir: &Path, spec: &ModelSpec, base_url: &str, progress: &mut dyn FnMut(&str, u64, Option<u64>)) -> Result<bool> {
    let dir = model_dir(models_dir, spec.id);
    std::fs::create_dir_all(&dir)?;
    let mut downloaded = false;
    let mut agent = None;
    for (name, sha) in spec.files {
        let path = dir.join(name);
        if path.is_file() && sha256_file(&path)? == *sha {
            continue;
        }
        let part = dir.join(format!("{name}.part"));
        let url = format!("{base_url}/{}/resolve/{}/{name}", spec.id, spec.revision);
        let got = match download(agent.get_or_insert_with(new_agent), &url, &part, name, progress) {
            Ok(got) => got,
            Err(e) => {
                let _ = std::fs::remove_file(&part);
                return Err(e);
            }
        };
        if got != *sha {
            std::fs::remove_file(&part)?;
            return Err(VltError::Model(format!("checksum mismatch for {}/{name}", spec.id)));
        }
        std::fs::rename(&part, &path)?;
        downloaded = true;
    }
    Ok(downloaded)
}

fn progress_text(id: &str, name: &str, done: u64, total: Option<u64>) -> String {
    let mb = |b: u64| b as f64 / 1_048_576.0;
    match total {
        Some(t) => format!("\r{id} {name}: {:.1}/{:.1} MB", mb(done), mb(t)),
        None => format!("\r{id} {name}: {:.1} MB", mb(done)),
    }
}

/// `pull` from Hugging Face with a progress line on stderr, ended by a newline once shown.
pub(crate) fn pull_showing_progress(models_dir: &Path, spec: &ModelSpec) -> Result<bool> {
    let mut shown = false;
    let downloaded = pull(models_dir, spec, HF_BASE, &mut |name, done, total| {
        eprint!("{}", progress_text(spec.id, name, done, total));
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
    use crate::ml::registry::{ModelKind, ModelSpec};
    use std::io::{Read, Write};
    use std::net::TcpListener;

    /// Serves `body` for every GET, `hits` times.
    fn serve(body: &'static [u8], hits: usize) -> String {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = l.local_addr().unwrap();
        std::thread::spawn(move || {
            for s in l.incoming().take(hits) {
                let mut s = s.unwrap();
                let mut buf = [0u8; 4096];
                let _ = s.read(&mut buf);
                write!(s, "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len()).unwrap();
                s.write_all(body).unwrap();
            }
        });
        format!("http://{addr}")
    }
    // sha256("hello")
    const HELLO: &str = "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824";
    static GOOD: ModelSpec = ModelSpec { id: "t/good", revision: "r", kind: ModelKind::BertCrossEncoder, files: &[("a.json", HELLO)] };
    static BAD: ModelSpec = ModelSpec { id: "t/bad", revision: "r", kind: ModelKind::BertCrossEncoder,
        files: &[("a.json", "0000000000000000000000000000000000000000000000000000000000000000")] };

    #[test]
    fn pull_downloads_verifies_and_skips_present_files() {
        let d = tempfile::tempdir().unwrap();
        assert!(missing(d.path(), &GOOD));
        let base = serve(b"hello", 1);
        assert!(pull(d.path(), &GOOD, &base, &mut |_, _, _| {}).unwrap());
        let f = d.path().join("t--good/a.json");
        assert_eq!(std::fs::read(&f).unwrap(), b"hello");
        assert!(!missing(d.path(), &GOOD));
        assert!(!pull(d.path(), &GOOD, "http://127.0.0.1:9", &mut |_, _, _| {}).unwrap(), "no network when present");
    }

    #[test]
    fn checksum_mismatch_leaves_nothing() {
        let d = tempfile::tempdir().unwrap();
        let base = serve(b"hello", 1);
        let err = pull(d.path(), &BAD, &base, &mut |_, _, _| {}).unwrap_err();
        assert!(err.to_string().contains("checksum mismatch"), "{err}");
        let dir = d.path().join("t--bad");
        assert!(!dir.join("a.json").exists() && !dir.join("a.json.part").exists());
    }

    #[test]
    fn corrupt_present_file_is_downloaded_again() {
        let d = tempfile::tempdir().unwrap();
        let dir = d.path().join("t--good");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.json"), b"junk").unwrap();
        let base = serve(b"hello", 1);
        assert!(pull(d.path(), &GOOD, &base, &mut |_, _, _| {}).unwrap());
        assert_eq!(std::fs::read(dir.join("a.json")).unwrap(), b"hello");
    }

    #[test]
    fn progress_text_shows_megabytes_with_and_without_a_total() {
        assert_eq!(progress_text("o/m", "a.bin", 1_572_864, Some(3_145_728)), "\ro/m a.bin: 1.5/3.0 MB");
        assert_eq!(progress_text("o/m", "a.bin", 524_288, None), "\ro/m a.bin: 0.5 MB");
    }

    #[test]
    fn pull_downloads_every_missing_file_of_a_spec() {
        static TWO: ModelSpec = ModelSpec { id: "t/both", revision: "r", kind: ModelKind::BertCrossEncoder,
            files: &[("a.json", HELLO), ("b.json", HELLO)] };
        let d = tempfile::tempdir().unwrap();
        let base = serve(b"hello", 2);
        assert!(pull(d.path(), &TWO, &base, &mut |_, _, _| {}).unwrap());
        assert!(!missing(d.path(), &TWO));
    }

    #[test]
    fn missing_when_any_file_is_absent() {
        static TWO: ModelSpec = ModelSpec { id: "t/two", revision: "r", kind: ModelKind::BertCrossEncoder,
            files: &[("a.json", HELLO), ("b.json", HELLO)] };
        let d = tempfile::tempdir().unwrap();
        let dir = d.path().join("t--two");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.json"), b"hello").unwrap();
        assert!(missing(d.path(), &TWO));
        std::fs::write(dir.join("b.json"), b"hello").unwrap();
        assert!(!missing(d.path(), &TWO));
    }
}

