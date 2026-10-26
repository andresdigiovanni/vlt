# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed

- `vlt install` and `vlt reindex` no longer look stuck while they index: they say how many notes and how much text they will embed, say when the embedding model is loading, and the progress line now advances after each pass of the embedding model (not every 16 notes), with the text embedded so far (`419/932 KB (44%)`) and an estimate of the time left.
- The model download progress line clears itself on each redraw, so no stray characters from a longer previous line remain (`1136.4/1136.4 MB0.0/0.0 MB`).

## [0.3.0] - 2026-10-26

### Added

- Tags written in the body (`#tag`, `#nested/tag`) count as tags, as in Obsidian, and `get` lists every tag of a note, frontmatter and body, in a new `tags` field.
- `get` lists the `candidates` of an ambiguous link: the paths it could mean, so the agent can propose the fix.
- `properties` takes a `key` (`vlt properties --key tags`) and lists that property alone with every value, not only the 10 most frequent: the agent can check whether a tag already exists before proposing a new one.
- Links in properties (`up: "[[note]]"`, quoted as Obsidian requires) count as links and backlinks, as in Obsidian; `get` marks each with the `property` it is in.
- `properties` takes `where` (`vlt properties --where type=project`), with the syntax of `search`: it counts only the notes it selects, so the agent sees which keys notes of one kind use.
- `properties` lists each key's `variants`: spellings of one value that differ only in case or in `-`, `_` and spaces (`Rust` and `rust`, `machine-learning` and `machine_learning`), most used first; `vlt properties` prints them under the key.
- `folders` (MCP tool and `vlt folders`) lists the vault's folders: per folder the notes directly in it, the total with its subfolders and its 5 most used keys, so the agent sees where a new note belongs.
- `related` (MCP tool and `vlt related <path>`) lists the notes closest to a note by meaning, section by section, with the links that already exist; the tags two or more of the closest of them carry and the note lacks; and the sections of other notes that name it (title, alias or file name) without a link. The Skill starts from it when linking a note.
- `audit` (MCP tool and `vlt audit`) reports what only shows across notes: spelling variants of values and keys with the notes to change, keys one note has, keys and tags on most but not all of a folder's notes (with the notes missing them), broken and ambiguous links, notes nobody links and notes linking nowhere. The Skill has an Auditing flow that turns it into one list of fixes.
- `create` (MCP tool) writes a new note the user approved: the whole file at a vault-relative `.md` path, creating missing folders. It never overwrites (`already_exists`, a new error code) and rejects excluded paths. The Skill's Drafting flow now ends with `create` instead of asking you to save the file.

### Changed

- The `tags` filter and `properties` count body `#tags` too: `tags=rust` finds a note that writes `#rust` only in its text, and `tags=#rust` is the same filter.
- A frontmatter `tags` string is split on commas and spaces (`tags: rust, mcp` is two tags) and loses a leading `#`; each piece is cut at the first character a tag cannot hold, as in the body (`"#mcp"` → `mcp`); all-digit tags (`2024`) are not tags, as in Obsidian; `properties` reports `tags` as a list counted once per note.
- A `tags` filter also matches nested tags, as Obsidian does: `tags=project` (or `tags=project/`) finds notes tagged `project/vlt` or `project/vlt/cli`, not `projects`. Other properties still match exact values.
- The reranker gets the top half of `rerank_top` from each search branch (vectors take the odd slot), in Reciprocal Rank Fusion order, instead of the RRF top: a section only the semantic branch finds no longer loses its place to sections both branches rank low. On the eval set (`make eval`), Recall@5 rises from 0.826 to 0.878, and from 0.489 to 0.644 on paraphrased questions.
- `min_score` defaults to 0.02 (was 0.05): on the eval set the default reranker scores some correct answers between 0.02 and 0.05, and no threshold separates relevant from unrelated sections well, so 0.05 cost answers for little filtering: Recall@5 rises from 0.878 to 0.907, while the questions the eval set cannot answer that come back empty drop from 2 of 15 to none. Existing configs keep their value.
- Notes are indexed in chunks of at most 256 tokens (was 512), and a search result is the whole section a matching chunk belongs to, once per section, scored as its best chunk: smaller chunks rank the sentence that answers, the section keeps its context. On the eval set Recall@5 rises from 0.907 to 0.944, and from 0.722 to 0.833 on paraphrased questions; results carry ~25% more text. The index format changes: run `vlt reindex` once to rebuild it.
- The Skill tells the agent to search by the term a note would use rather than a description of it, and to check that a section answers the question before using it; the MCP `search` tool's `query` parameter asks for key terms too. On the eval set, the questions rewritten as key-term queries the way the Skill asks reach Recall@5 0.985 (0.944 as asked), and 0.956 on paraphrased questions (0.833).
- The Skill has a flow for drafting a new note: search for a note that already covers it, use the folder's properties and file names, write headings that stand alone, then offer links to the saved note. When enriching, it also searches the note's title and `aliases` for links to it, proposes the keys most sibling notes have, prefers the most used spelling of a value, and keeps the list to about 10 items.
- `get` lists each section once, with the `chunk_id` and lines `search` returns for it, instead of one entry per chunk of a long section (`#Heading [1/3]`, `[2/3]`…).

### Fixed

- A model download whose connection stops sending data fails after 60 seconds instead of hanging forever.
- A search reads the index as one snapshot: a reindex that another vlt commits mid-search no longer turns a result into a bare chunk or mixes the old and new versions of a section.

## [0.2.0] - 2026-10-15

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
