use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelKind {
    Embedding { dim: usize },
    BertCrossEncoder,
    XlmRobertaCrossEncoder,
}

#[derive(Debug)]
pub struct ModelSpec {
    pub id: &'static str,
    pub revision: &'static str,
    pub kind: ModelKind,
    /// (file name, sha256)
    pub files: &'static [(&'static str, &'static str)],
}

pub static MODELS: &[ModelSpec] = &[
    ModelSpec {
        id: "BAAI/bge-small-en-v1.5",
        revision: "5c38ec7c405ec4b44b94cc5a9bb96e735b38267a",
        kind: ModelKind::Embedding { dim: 384 },
        files: &[
            ("model.safetensors", "3c9f31665447c8911517620762200d2245a2518d6e7208acc78cd9db317e21ad"),
            ("config.json", "094f8e891b932f2000c92cfc663bac4c62069f5d8af5b5278c4306aef3084750"),
            ("tokenizer.json", "d241a60d5e8f04cc1b2b3e9ef7a4921b27bf526d9f6050ab90f9267a1f9e5c66"),
        ],
    },
    ModelSpec {
        id: "cross-encoder/ms-marco-MiniLM-L-6-v2",
        revision: "233902d25c440f23af6f7d6e94d2946bac0bee0a",
        kind: ModelKind::BertCrossEncoder,
        files: &[
            ("model.safetensors", "821d1aa69520101d6e0737f78a042ae25b19e5cb9160701909d10434f4aeb0ae"),
            ("config.json", "380e02c93f431831be65d99a4e7e5f67c133985bf2e77d9d4eba46847190bacc"),
            ("tokenizer.json", "d241a60d5e8f04cc1b2b3e9ef7a4921b27bf526d9f6050ab90f9267a1f9e5c66"),
        ],
    },
    ModelSpec {
        id: "BAAI/bge-reranker-base",
        revision: "2cfc18c9415c912f9d8155881c133215df768a70",
        kind: ModelKind::XlmRobertaCrossEncoder,
        files: &[
            ("model.safetensors", "ced967c45fd1902eb92716c9ceeca7c95a936770ea9db611f5a841b926e33fbd"),
            ("config.json", "289adf7ada1eb6b4afa7589a48a032d45a076cf2e46dcdb3b4cabc33be14f708"),
            ("tokenizer.json", "9eb652ac4e40cc093272bbbe0f55d521cf67570060227109b5cdc20945a4489e"),
        ],
    },
];

pub fn lookup(id: &str) -> Option<&'static ModelSpec> {
    MODELS.iter().find(|m| m.id == id)
}

pub fn model_dir(models_dir: &Path, id: &str) -> PathBuf {
    models_dir.join(id.replace('/', "--"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn registry_has_the_three_pinned_models() {
        let e = lookup("BAAI/bge-small-en-v1.5").unwrap();
        assert_eq!(e.kind, ModelKind::Embedding { dim: 384 });
        assert_eq!(lookup("cross-encoder/ms-marco-MiniLM-L-6-v2").unwrap().kind, ModelKind::BertCrossEncoder);
        assert_eq!(lookup("BAAI/bge-reranker-base").unwrap().kind, ModelKind::XlmRobertaCrossEncoder);
        assert!(lookup("other/model").is_none());
        for m in MODELS {
            assert_eq!(m.revision.len(), 40);
            let names: Vec<_> = m.files.iter().map(|f| f.0).collect();
            assert_eq!(names, vec!["model.safetensors", "config.json", "tokenizer.json"]);
            assert!(m.files.iter().all(|f| f.1.len() == 64 && f.1.chars().all(|c| c.is_ascii_hexdigit())));
        }
        assert_eq!(e.files[0].1, "3c9f31665447c8911517620762200d2245a2518d6e7208acc78cd9db317e21ad");
    }
    #[test]
    fn model_dir_naming() {
        assert_eq!(model_dir(Path::new("/m"), "BAAI/bge-small-en-v1.5"), PathBuf::from("/m/BAAI--bge-small-en-v1.5"));
    }
}
