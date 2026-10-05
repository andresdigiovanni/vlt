use std::path::Path;

use candle_core::{Device, Module, Tensor};
use candle_nn::{linear, Linear};
use candle_transformers::models::bert::{BertModel, Config as BertConfig};
use candle_transformers::models::xlm_roberta::{Config as XlmConfig, XLMRobertaForSequenceClassification};
use tokenizers::{pad_encodings, Encoding, Tokenizer, TruncationParams, TruncationStrategy};

use crate::ml::embeddings::{batch_tensors, in_length_batches, load_config, load_tokenizer, load_weights, model_err};
use crate::ml::registry::ModelKind;
use crate::ml::Reranker;
use crate::{Result, VltError};

const MAX_TOKENS: usize = 512;
const BATCH: usize = 16;
/// At most this many padded tokens per forward pass, bounding activation memory.
const BATCH_TOKENS: usize = 1024;

enum Arch {
    Bert { bert: BertModel, pooler: Linear, classifier: Linear },
    XlmRoberta(XLMRobertaForSequenceClassification),
}

pub struct CrossEncoder {
    arch: Arch,
    tokenizer: Tokenizer,
    /// LongestFirst truncation, for pairs whose query leaves no room for the document.
    fallback: Tokenizer,
    device: Device,
}

impl CrossEncoder {
    pub fn load(dir: &Path, kind: ModelKind) -> Result<CrossEncoder> {
        let device = Device::Cpu;
        let arch = match kind {
            ModelKind::BertCrossEncoder => {
                let cfg: BertConfig = load_config(dir)?;
                let vb = load_weights(dir, &device)?;
                let bert = BertModel::load(vb.pp("bert"), &cfg).map_err(model_err)?;
                let pooler = linear(cfg.hidden_size, cfg.hidden_size, vb.pp("bert.pooler.dense")).map_err(model_err)?;
                let classifier = linear(cfg.hidden_size, 1, vb.pp("classifier")).map_err(model_err)?;
                Arch::Bert { bert, pooler, classifier }
            }
            ModelKind::XlmRobertaCrossEncoder => {
                let cfg: XlmConfig = load_config(dir)?;
                let vb = load_weights(dir, &device)?;
                Arch::XlmRoberta(XLMRobertaForSequenceClassification::new(1, &cfg, vb).map_err(model_err)?)
            }
            ModelKind::Embedding { .. } => {
                return Err(VltError::Model(format!("{} is not a cross-encoder", dir.display())))
            }
        };
        // The query is never cut: only the document is truncated to fit.
        let trunc = TruncationParams { max_length: MAX_TOKENS, strategy: TruncationStrategy::OnlySecond, ..Default::default() };
        let (tokenizer, _) = load_tokenizer(dir, trunc.clone())?;
        let mut fallback = tokenizer.clone();
        fallback
            .with_truncation(Some(TruncationParams { strategy: TruncationStrategy::LongestFirst, ..trunc }))
            .map_err(model_err)?;
        Ok(CrossEncoder { arch, tokenizer, fallback, device })
    }

    fn encode(&self, query: &str, docs: &[&str]) -> Result<Vec<Encoding>> {
        let pairs: Vec<(String, String)> = docs.iter().map(|d| (query.to_string(), d.to_string())).collect();
        if let Ok(encodings) = self.tokenizer.encode_batch(pairs.clone(), true) {
            return Ok(encodings);
        }
        // Some pair cannot be fitted by cutting only the document: cut that pair LongestFirst.
        let mut encodings = pairs
            .into_iter()
            .map(|p| self.tokenizer.encode(p.clone(), true).or_else(|_| self.fallback.encode(p, true)))
            .collect::<tokenizers::Result<Vec<_>>>()
            .map_err(model_err)?;
        if let Some(padding) = self.tokenizer.get_padding() {
            pad_encodings(&mut encodings, padding).map_err(model_err)?;
        }
        Ok(encodings)
    }

    fn logits(&self, query: &str, docs: &[&str]) -> Result<Vec<f32>> {
        let encodings = self.encode(query, docs)?;
        let (ids, types, mask) = batch_tensors(&encodings, &self.device)?;
        let logits = match &self.arch {
            Arch::Bert { bert, pooler, classifier } => {
                let hidden = bert.forward(&ids, &types, Some(&mask)).map_err(model_err)?;
                hidden
                    .narrow(1, 0, 1)
                    .and_then(|t| t.squeeze(1))
                    .and_then(|cls| pooler.forward(&cls))
                    .and_then(|t| t.tanh())
                    .and_then(|t| classifier.forward(&t))
            }
            Arch::XlmRoberta(model) => model.forward(&ids, &mask, &types),
        };
        logits.and_then(|t: Tensor| t.squeeze(1)).and_then(|t| t.to_vec1::<f32>()).map_err(model_err)
    }
}

fn sigmoid(x: f32) -> f32 {
    1.0 / (1.0 + (-x).exp())
}

impl Reranker for CrossEncoder {
    fn score(&self, query: &str, docs: &[&str]) -> Result<Vec<f32>> {
        // Pair lengths after truncation, so batches of similar length share little padding.
        let lens: Vec<usize> = self
            .encode(query, docs)?
            .iter()
            .map(|e| e.get_attention_mask().iter().filter(|&&m| m == 1).count())
            .collect();
        in_length_batches(&lens, BATCH, BATCH_TOKENS, |batch| {
            let batch_docs: Vec<&str> = batch.iter().map(|&i| docs[i]).collect();
            Ok(self.logits(query, &batch_docs)?.into_iter().map(sigmoid).collect())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rerank_forwards_hold_at_most_batch_pairs_and_batch_tokens_padded_tokens() {
        use crate::ml::embeddings::tests::uniform_sizes;
        assert_eq!((BATCH, BATCH_TOKENS), (16, 1024), "caps measured to keep peak RSS at or below main: re-measure before changing");
        const { assert!(BATCH_TOKENS / 10 > BATCH && BATCH_TOKENS / MAX_TOKENS < BATCH, "the cases below hit each cap") };
        for len in [10, MAX_TOKENS, BATCH_TOKENS / BATCH + 1] {
            let n = 2 * BATCH + 3;
            let mut sizes = Vec::new();
            let out = in_length_batches(&vec![len; n], BATCH, BATCH_TOKENS, |b| {
                sizes.push(b.len());
                Ok(b.to_vec())
            })
            .unwrap();
            assert_eq!(sizes, uniform_sizes(n, len, BATCH, BATCH_TOKENS), "len {len}");
            assert_eq!(out, (0..n).collect::<Vec<_>>());
        }
    }
}
