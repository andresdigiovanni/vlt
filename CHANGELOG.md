# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Any Hugging Face model whose architecture vlt can run is accepted in `[models]`, without a built-in list: embedding models of type `bert`, `xlm-roberta` and `qwen3` (e.g. `BAAI/bge-large-en-v1.5`, `Qwen/Qwen3-Embedding-0.6B`) and `bert`, `xlm-roberta` and `deberta-v2` cross-encoders as rerankers (e.g. `mixedbread-ai/mxbai-rerank-large-v1`). Pooling, query and document prompts and maximum length come from the repo's sentence-transformers files.
- `[models.embedding_options]` (`revision`, `pooling`, `query_prefix`, `document_prefix`, `precision`) and `[models.reranker_options]` (`revision`, `precision`) override what a repo declares.
- Models compute in the precision their weights are stored in: 16-bit weights run in F16, halving their memory (BF16 weights run as F16, since Candle has no BF16 matrix product on CPU; `deberta-v2` rerankers stay in F32).
- Sharded safetensors weights (`model.safetensors.index.json`).

### Changed

- New default models: `Qwen/Qwen3-Embedding-0.6B` (multilingual, 1024 dimensions) and `mixedbread-ai/mxbai-rerank-base-v1`. They rank better but download ~1.5 GB, peak at ~2.3 GB of memory and rerank slower on a CPU (~5 s for 10 sections of ~290 tokens). Existing configs name their models and keep them; set the 0.1.0 defaults (`BAAI/bge-small-en-v1.5`, `cross-encoder/ms-marco-MiniLM-L-6-v2`) for fast, small searches.
- `rerank_top` defaults to 10 (was 20): reranking time grows in proportion to it.
- `min_score` defaults to 0.05 (was 0.3), calibrated for the new default reranker, which scores some correct but reworded matches low. Existing configs keep their value; with the 0.1.0 reranker, 0.3 remains a good choice.
- Model files are verified against the hashes Hugging Face publishes for the resolved commit, instead of built-in hashes, and the commit is locked in `vlt-lock.json` in the model's folder. Existing downloads of the default models are kept.
- The index also records the embedding model's commit, pooling, document prefix and maximum length; changing any of them asks for a rebuild. Indexes built by 0.1.0 keep working.
- A malformed model id is rejected when the config is loaded; a model of an unsupported architecture, or in the wrong role, is reported right after its download.

## [0.1.0] - 2026-10-05

### Added

- Indexing of an Obsidian vault into SQLite (FTS5 + vectors) with incremental reindexing.
- Hybrid search (BM25 + vectors, Reciprocal Rank Fusion) with local reranking, property filters, path prefix and score threshold.
- `get` (sections, links, backlinks) and `properties`.
- `vlt` CLI: `search`, `get`, `properties`, `reindex`, `mcp`, `install`, `uninstall`.
- Stdio MCP server exposing `search`, `get`, `properties` and `reindex`.
- `vlt install` / `vlt uninstall` for Claude Code, opencode and Codex (MCP server and Skill).
- Install script and release workflow for macOS and Linux (x86_64 and aarch64).
- MCP tools `edit` (replace one exact fragment of a document's body) and `update_properties` (add, change or remove frontmatter properties), the only tools that write to the vault; both write atomically and reindex.
- The Skill covers enriching a note: reusing the vault's properties and linking it with the existing notes in both directions, under the user's approval.

### Changed

- Refreshed MCP tool descriptions and `--help` texts for every command and argument.
