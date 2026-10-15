use std::path::Path;

use candle_core::{Device, Module, Tensor};
use candle_nn::{linear, Linear};
use candle_transformers::models::bert::{BertModel, Config as BertConfig};
use candle_transformers::models::debertav2::{Config as DebertaConfig, DebertaV2SeqClassificationModel};
use candle_transformers::models::xlm_roberta::{Config as XlmConfig, XLMRobertaForSequenceClassification};
use tokenizers::{pad_encodings, Encoding, Tokenizer, TruncationParams, TruncationStrategy};

use crate::ml::embeddings::{batch_tensors, in_length_batches, load_config, load_tokenizer, load_weights, model_err};
use crate::ml::profile::{RerankerArch, RerankerProfile};
use crate::ml::Reranker;
use crate::Result;

const BATCH: usize = 16;
/// At most this many padded tokens per forward pass, bounding activation memory.
const BATCH_TOKENS: usize = 1024;

enum Arch {
    Bert { bert: BertModel, pooler: Linear, classifier: Linear },
    XlmRoberta(XLMRobertaForSequenceClassification),
    /// `type_vocab_size` 0 models take no token type ids. Boxed: its config makes it far larger than the others.
    DebertaV2 { model: Box<DebertaV2SeqClassificationModel>, token_types: bool },
}

pub struct CrossEncoder {
    arch: Arch,
    tokenizer: Tokenizer,
    /// LongestFirst truncation, for pairs whose query leaves no room for the document.
    fallback: Tokenizer,
    device: Device,
}

impl CrossEncoder {
    pub fn load(dir: &Path, profile: &RerankerProfile) -> Result<CrossEncoder> {
        let device = Device::Cpu;
        let vb = load_weights(&profile.weights, profile.precision.dtype(), &device)?;
        let arch = match profile.arch {
            RerankerArch::Bert => {
                let cfg: BertConfig = load_config(dir)?;
                let bert = BertModel::load(vb.pp("bert"), &cfg).map_err(model_err)?;
                let pooler = linear(cfg.hidden_size, cfg.hidden_size, vb.pp("bert.pooler.dense")).map_err(model_err)?;
                let classifier = linear(cfg.hidden_size, 1, vb.pp("classifier")).map_err(model_err)?;
                Arch::Bert { bert, pooler, classifier }
            }
            RerankerArch::XlmRoberta => {
                let cfg: XlmConfig = load_config(dir)?;
                Arch::XlmRoberta(XLMRobertaForSequenceClassification::new(1, &cfg, vb).map_err(model_err)?)
            }
            RerankerArch::DebertaV2 => {
                let cfg: DebertaConfig = load_config(dir)?;
                let token_types = cfg.type_vocab_size > 0;
                // The pooler and classifier sit at the root, beside `deberta`.
                let model = DebertaV2SeqClassificationModel::load(vb.pp("deberta"), &cfg, None).map_err(model_err)?;
                Arch::DebertaV2 { model: Box::new(model), token_types }
            }
        };
        // The query is never cut: only the document is truncated to fit.
        let trunc = TruncationParams { max_length: profile.max_tokens, strategy: TruncationStrategy::OnlySecond, ..Default::default() };
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
            Arch::DebertaV2 { model, token_types } => model.forward(&ids, token_types.then_some(types), Some(mask)),
        };
        logits
            .and_then(|t: Tensor| t.squeeze(1))
            .and_then(|t| t.to_dtype(candle_core::DType::F32))
            .and_then(|t| t.to_vec1::<f32>())
            .map_err(model_err)
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
        use crate::ml::profile::MAX_TOKENS;
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
