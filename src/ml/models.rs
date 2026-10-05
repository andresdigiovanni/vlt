use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use crate::config::ModelsConfig;
use crate::installer::models::missing;
use crate::ml::embeddings::BertEmbedder;
use crate::ml::fake::{FakeEmbedder, FakeReranker, FAKE_DIM, FAKE_MODEL_ID};
use crate::ml::registry::{lookup, model_dir, ModelKind, ModelSpec};
use crate::ml::reranker::CrossEncoder;
use crate::ml::{Embedder, Reranker};
use crate::storage::db::IndexMeta;
use crate::{Result, VltError};

struct State {
    embedder: Option<Arc<dyn Embedder>>,
    reranker: Option<Arc<dyn Reranker>>,
    last_used: Instant,
}

pub struct Models {
    embedding: &'static ModelSpec,
    reranker: &'static ModelSpec,
    models_dir: PathBuf,
    fake: bool,
    state: Mutex<State>,
}

fn spec_of(id: &str, ok: fn(ModelKind) -> bool) -> Result<&'static ModelSpec> {
    lookup(id)
        .filter(|s| ok(s.kind))
        .ok_or_else(|| VltError::Config(format!("unsupported model: {id}")))
}

impl Models {
    pub fn new(cfg: &ModelsConfig, models_dir: &Path, fake: bool) -> Result<Models> {
        let embedding = spec_of(&cfg.embedding, |k| matches!(k, ModelKind::Embedding { .. }))?;
        let reranker = spec_of(&cfg.reranker, |k| {
            matches!(k, ModelKind::BertCrossEncoder | ModelKind::XlmRobertaCrossEncoder)
        })?;
        Ok(Models {
            embedding,
            reranker,
            models_dir: models_dir.to_path_buf(),
            fake,
            state: Mutex::new(State { embedder: None, reranker: None, last_used: Instant::now() }),
        })
    }

    pub fn fake_from_env() -> bool {
        std::env::var("VLT_FAKE_MODELS").as_deref() == Ok("1")
    }

    pub fn index_meta(&self, vault: &Path) -> IndexMeta {
        let (model_id, dim) = match (self.fake, self.embedding.kind) {
            (true, _) => (FAKE_MODEL_ID.to_string(), FAKE_DIM),
            (false, ModelKind::Embedding { dim }) => (self.embedding.id.to_string(), dim),
            (false, _) => unreachable!("validated in Models::new"),
        };
        IndexMeta { model_id, dim, vault: vault.display().to_string() }
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn dir_of(&self, spec: &ModelSpec) -> Result<PathBuf> {
        if missing(&self.models_dir, spec) {
            return Err(VltError::Model(format!("model {} is not downloaded; run `vlt reindex`", spec.id)));
        }
        Ok(model_dir(&self.models_dir, spec.id))
    }

    pub fn embedder(&self) -> Result<Arc<dyn Embedder>> {
        let mut st = self.state();
        st.last_used = Instant::now();
        if let Some(e) = &st.embedder {
            return Ok(e.clone());
        }
        // The state lock stays held while a model loads (here and in `reranker()`), so concurrent
        // callers and `unload_if_idle` wait instead of loading the same model twice.
        let e: Arc<dyn Embedder> = if self.fake {
            Arc::new(FakeEmbedder)
        } else {
            Arc::new(BertEmbedder::load(&self.dir_of(self.embedding)?)?)
        };
        st.embedder = Some(e.clone());
        Ok(e)
    }

    pub fn reranker(&self) -> Result<Arc<dyn Reranker>> {
        let mut st = self.state();
        st.last_used = Instant::now();
        if let Some(r) = &st.reranker {
            return Ok(r.clone());
        }
        let r: Arc<dyn Reranker> = if self.fake {
            Arc::new(FakeReranker)
        } else {
            Arc::new(CrossEncoder::load(&self.dir_of(self.reranker)?, self.reranker.kind)?)
        };
        st.reranker = Some(r.clone());
        Ok(r)
    }

    pub fn unload_if_idle(&self, idle: Duration) -> bool {
        let mut st = self.state();
        if st.embedder.is_none() && st.reranker.is_none() {
            return false;
        }
        if st.last_used.elapsed() < idle {
            return false;
        }
        st.embedder = None;
        st.reranker = None;
        true
    }

    pub fn is_loaded(&self) -> bool {
        let st = self.state();
        st.embedder.is_some() || st.reranker.is_some()
    }

    pub fn specs(&self) -> [&'static ModelSpec; 2] {
        [self.embedding, self.reranker]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ModelsConfig;
    use std::path::Path;

    #[test]
    fn fake_models_load_lazily_and_unload() {
        let m = Models::new(&ModelsConfig::default(), Path::new("/nonexistent"), true).unwrap();
        assert!(!m.is_loaded());
        assert_eq!(m.index_meta(Path::new("/v")), IndexMeta { model_id: "fake".into(), dim: 384, vault: "/v".into() });
        m.embedder().unwrap();
        m.reranker().unwrap();
        assert!(m.is_loaded());
        assert!(!m.unload_if_idle(Duration::from_secs(3600)));
        assert!(m.unload_if_idle(Duration::ZERO));
        assert!(!m.is_loaded());
    }

    #[test]
    fn real_models_report_missing_files_and_meta() {
        let d = tempfile::tempdir().unwrap();
        let m = Models::new(&ModelsConfig::default(), d.path(), false).unwrap();
        assert_eq!(m.index_meta(Path::new("/v")).model_id, "BAAI/bge-small-en-v1.5");
        assert_eq!(m.index_meta(Path::new("/v")).dim, 384);
        let err = m.embedder().err().unwrap();
        assert!(err.to_string().contains("vlt reindex"), "{err}");
    }

    #[test]
    fn unsupported_or_misplaced_models_are_rejected() {
        let cfg = ModelsConfig { embedding: "other/model".into(), ..Default::default() };
        assert!(matches!(Models::new(&cfg, Path::new("/m"), false), Err(VltError::Config(_))));
        let cfg = ModelsConfig { reranker: "BAAI/bge-small-en-v1.5".into(), ..Default::default() };
        assert!(matches!(Models::new(&cfg, Path::new("/m"), false), Err(VltError::Config(_))));
    }

    #[test]
    fn models_load_once_and_every_use_refreshes_idle_clock() {
        let m = Models::new(&ModelsConfig::default(), Path::new("/nonexistent"), true).unwrap();
        let e = m.embedder().unwrap();
        assert!(m.is_loaded());
        assert!(Arc::ptr_eq(&e, &m.embedder().unwrap()));
        let r = m.reranker().unwrap();
        assert!(Arc::ptr_eq(&r, &m.reranker().unwrap()));

        let idle = Duration::from_millis(200);
        std::thread::sleep(Duration::from_millis(300));
        m.embedder().unwrap();
        assert!(!m.unload_if_idle(idle), "embedder() refreshes last use");
        std::thread::sleep(Duration::from_millis(300));
        m.reranker().unwrap();
        assert!(!m.unload_if_idle(idle), "reranker() refreshes last use");
        std::thread::sleep(Duration::from_millis(300));
        assert!(m.unload_if_idle(idle));
    }
}

