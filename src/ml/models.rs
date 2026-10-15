use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use crate::config::{EmbeddingOptions, ModelsConfig, RerankerOptions};
use crate::installer::models::missing;
use crate::ml::embeddings::TextEmbedder;
use crate::ml::fake::{FakeEmbedder, FakeReranker, FAKE_DIM, FAKE_MODEL_ID};
use crate::ml::profile::{
    embedding_profile, model_dir, reranker_profile, validate_model_id, validate_revision, EmbeddingProfile,
    RerankerProfile, DEFAULT_REVISION,
};
use crate::ml::reranker::CrossEncoder;
use crate::ml::{Embedder, Reranker};
use crate::storage::db::IndexMeta;
use crate::{Result, VltError};

/// A model as the config names it: a Hugging Face repo id and the revision to download.
#[derive(Debug, Clone, PartialEq)]
pub struct ModelRef {
    pub id: String,
    pub revision: String,
}

impl ModelRef {
    fn new(id: &str, revision: Option<&str>) -> Result<ModelRef> {
        validate_model_id(id)?;
        let revision = revision.unwrap_or(DEFAULT_REVISION);
        validate_revision(revision)?;
        Ok(ModelRef { id: id.to_string(), revision: revision.to_string() })
    }
}

struct State {
    embedder: Option<Arc<dyn Embedder>>,
    reranker: Option<Arc<dyn Reranker>>,
    last_used: Instant,
}

pub struct Models {
    embedding: ModelRef,
    reranker: ModelRef,
    embedding_options: EmbeddingOptions,
    reranker_options: RerankerOptions,
    models_dir: PathBuf,
    fake: bool,
    state: Mutex<State>,
}

impl Models {
    /// Checks only the shape of the ids and revisions: what a model is comes from its files,
    /// so `check` validates the models once they are downloaded.
    pub fn new(cfg: &ModelsConfig, models_dir: &Path, fake: bool) -> Result<Models> {
        Ok(Models {
            embedding: ModelRef::new(&cfg.embedding, cfg.embedding_options.revision.as_deref())?,
            reranker: ModelRef::new(&cfg.reranker, cfg.reranker_options.revision.as_deref())?,
            embedding_options: cfg.embedding_options.clone(),
            reranker_options: cfg.reranker_options.clone(),
            models_dir: models_dir.to_path_buf(),
            fake,
            state: Mutex::new(State { embedder: None, reranker: None, last_used: Instant::now() }),
        })
    }

    pub fn fake_from_env() -> bool {
        std::env::var("VLT_FAKE_MODELS").as_deref() == Ok("1")
    }

    /// What the index must have been built with; reads the embedding model's files, so it
    /// fails when the model is not downloaded.
    pub fn index_meta(&self, vault: &Path) -> Result<IndexMeta> {
        let vault = vault.display().to_string();
        if self.fake {
            return Ok(IndexMeta { model_id: FAKE_MODEL_ID.into(), dim: FAKE_DIM, fingerprint: FAKE_MODEL_ID.into(), vault });
        }
        let (_, profile) = self.embedding_profile()?;
        Ok(IndexMeta { model_id: self.embedding.id.clone(), dim: profile.dim, fingerprint: profile.fingerprint(), vault })
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn dir_of(&self, model: &ModelRef) -> Result<PathBuf> {
        if missing(&self.models_dir, &model.id, &model.revision) {
            return Err(VltError::Model(format!("model {} is not downloaded; run `vlt reindex`", model.id)));
        }
        Ok(model_dir(&self.models_dir, &model.id))
    }

    fn embedding_profile(&self) -> Result<(PathBuf, EmbeddingProfile)> {
        let dir = self.dir_of(&self.embedding)?;
        let profile = embedding_profile(&dir, &self.embedding.id, &self.embedding_options)?;
        Ok((dir, profile))
    }

    fn reranker_profile(&self) -> Result<(PathBuf, RerankerProfile)> {
        let dir = self.dir_of(&self.reranker)?;
        let profile = reranker_profile(&dir, &self.reranker.id, &self.reranker_options)?;
        Ok((dir, profile))
    }

    /// Fails unless both models are downloaded and of a kind vlt can run in their role, so a
    /// wrong id is reported right after the download rather than at the first search.
    pub fn check(&self) -> Result<()> {
        if !self.fake {
            self.embedding_profile()?;
            self.reranker_profile()?;
        }
        Ok(())
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
            let (dir, profile) = self.embedding_profile()?;
            Arc::new(TextEmbedder::load(&dir, &profile)?)
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
            let (dir, profile) = self.reranker_profile()?;
            Arc::new(CrossEncoder::load(&dir, &profile)?)
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

    /// The embedding model, then the reranker.
    pub fn refs(&self) -> [&ModelRef; 2] {
        [&self.embedding, &self.reranker]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ModelsConfig;
    use crate::ml::profile::tests::{bge_small, fake_download, mxbai_rerank_base, qwen3};
    use crate::ml::profile::LEGACY_FINGERPRINT;
    use std::path::Path;

    /// The 0.1.0 embedding model, whose fixture has a known fingerprint.
    fn bge_small_config() -> ModelsConfig {
        ModelsConfig { embedding: "BAAI/bge-small-en-v1.5".into(), ..Default::default() }
    }

    #[test]
    fn fake_models_load_lazily_and_unload() {
        let m = Models::new(&ModelsConfig::default(), Path::new("/nonexistent"), true).unwrap();
        assert!(!m.is_loaded());
        assert_eq!(
            m.index_meta(Path::new("/v")).unwrap(),
            IndexMeta { model_id: "fake".into(), dim: 384, fingerprint: "fake".into(), vault: "/v".into() }
        );
        m.check().unwrap();
        m.embedder().unwrap();
        m.reranker().unwrap();
        assert!(m.is_loaded());
        assert!(!m.unload_if_idle(Duration::from_secs(3600)));
        assert!(m.unload_if_idle(Duration::ZERO));
        assert!(!m.is_loaded());
    }

    #[test]
    fn real_models_need_their_files_for_meta_and_loading() {
        let d = tempfile::tempdir().unwrap();
        let m = Models::new(&bge_small_config(), d.path(), false).unwrap();
        for err in [m.index_meta(Path::new("/v")).err().unwrap(), m.embedder().err().unwrap(), m.check().err().unwrap()] {
            assert!(err.to_string().contains("vlt reindex"), "{err}");
        }
        bge_small(&d.path().join("BAAI--bge-small-en-v1.5"));
        let meta = m.index_meta(Path::new("/v")).unwrap();
        assert_eq!((meta.model_id.as_str(), meta.dim, meta.fingerprint.as_str()), ("BAAI/bge-small-en-v1.5", 384, LEGACY_FINGERPRINT));
    }

    #[test]
    fn a_new_revision_in_the_config_needs_a_download() {
        let d = tempfile::tempdir().unwrap();
        bge_small(&d.path().join("BAAI--bge-small-en-v1.5"));
        let mut cfg = bge_small_config();
        cfg.embedding_options.revision = Some("v2".into());
        let m = Models::new(&cfg, d.path(), false).unwrap();
        assert!(m.index_meta(Path::new("/v")).unwrap_err().to_string().contains("not downloaded"));
        assert_eq!(m.refs()[0], &ModelRef { id: "BAAI/bge-small-en-v1.5".into(), revision: "v2".into() });
        assert_eq!(m.refs()[1].revision, DEFAULT_REVISION);
    }

    #[test]
    fn malformed_ids_are_rejected_and_unknown_ones_wait_for_their_files() {
        let cfg = ModelsConfig { embedding: "nope".into(), ..Default::default() };
        assert!(matches!(Models::new(&cfg, Path::new("/m"), false), Err(VltError::Config(_))));
        let mut cfg = ModelsConfig::default();
        cfg.reranker_options.revision = Some("../x".into());
        assert!(matches!(Models::new(&cfg, Path::new("/m"), false), Err(VltError::Config(_))));
        let cfg = ModelsConfig { embedding: "any/model".into(), ..Default::default() };
        assert!(Models::new(&cfg, Path::new("/m"), false).is_ok());
    }

    #[test]
    fn check_reports_a_model_in_the_wrong_role() {
        let d = tempfile::tempdir().unwrap();
        bge_small(&d.path().join("BAAI--bge-small-en-v1.5"));
        let cfg = ModelsConfig { reranker: "BAAI/bge-small-en-v1.5".into(), ..bge_small_config() };
        let m = Models::new(&cfg, d.path(), false).unwrap();
        let err = m.check().unwrap_err().to_string();
        assert!(err.contains("not a reranker"), "{err}");
        let cross = r#"{"model_type":"bert","architectures":["BertForSequenceClassification"],"id2label":{"0":"L"}}"#;
        fake_download(&d.path().join("o--cross"), "c", &[("config.json", cross)]);
        let cfg = ModelsConfig { reranker: "o/cross".into(), ..bge_small_config() };
        Models::new(&cfg, d.path(), false).unwrap().check().unwrap();
    }

    #[test]
    fn the_default_models_are_supported() {
        let d = tempfile::tempdir().unwrap();
        qwen3(&d.path().join("Qwen--Qwen3-Embedding-0.6B"));
        mxbai_rerank_base(&d.path().join("mixedbread-ai--mxbai-rerank-base-v1"));
        let m = Models::new(&ModelsConfig::default(), d.path(), false).unwrap();
        m.check().unwrap();
        let meta = m.index_meta(Path::new("/v")).unwrap();
        assert_eq!((meta.model_id.as_str(), meta.dim), ("Qwen/Qwen3-Embedding-0.6B", 1024));
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
