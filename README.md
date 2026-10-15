# vlt

> **Semantic search over your Obsidian vault, for your AI agent and for you.**

vlt turns a vault (a folder of markdown notes) into a knowledge base your AI agent can search. It indexes your notes locally and serves them through **MCP for agents** and the **`vlt` CLI for humans**: the agent finds the right sections of your notes, answers from them, and, when you ask, adds properties and links to them.

Everything runs on your machine: no daemon, no cloud, no API keys.

## Why?

Your notes hold what no model knows: your projects, your past decisions, your conventions. But an agent can't use them well:

- **It doesn't know they exist.** Unless you paste a note into the chat, the agent answers from general knowledge.
- **grep is not search.** "How do we handle retries?" won't match a note titled *Backoff policy*. The agent reads file after file, or misses the answer.
- **Notes rot.** Properties are inconsistent, related notes aren't linked, links break when notes move.

vlt fixes all three:

```text
"what did we decide about retries?"
        │
        ▼
   ┌──────────┐   BM25 (keywords) ──┐
   │  search  │                     ├─ fused (RRF) ─▶ reranked ─▶ top sections
   └──────────┘   vectors (meaning)─┘                              above a threshold
        │
        ▼
 answer that cites projects/api.md#Retry policy
```

- **Hybrid search**: keyword search (SQLite FTS5) and semantic search (sqlite-vec) over the sections of every note, fused with Reciprocal Rank Fusion and reranked by a local cross-encoder. Results below a relevance threshold are dropped, so "nothing relevant" is an answer, not five random hits.
- **Filters**: by frontmatter property (`status=in_progress`, `tags=rust`, `date>=2026-01-01`) and by folder.
- **Enrichment on request**: ask the agent to "process the note X" and it proposes properties that match your vault's vocabulary, links from and to related notes, and fixes for broken links. It writes only the changes you pick.
- **Local models**: embeddings and reranking run on your CPU with [Candle](https://github.com/huggingface/candle). Pick any Hugging Face model of a [supported architecture](#models). Models are downloaded once, checked against the hashes Hugging Face publishes, and locked to that commit; vlt works offline afterwards.

## Quick start

### 1. Install the binary

```bash
curl -fsSL https://raw.githubusercontent.com/andresdigiovanni/vlt/main/install.sh | bash
```

The script downloads the prebuilt binary for your platform (macOS or Linux, x86_64 or aarch64) from GitHub Releases, verifies its SHA256 checksum and installs it to `~/.local/bin/vlt`. Make sure `~/.local/bin` is on your `PATH`.

### 2. Set it up

```bash
vlt install
```

`vlt install`:

1. asks for the path of your vault (and offers to create it if it does not exist),
2. downloads the embedding and reranking models to `~/.vlt/models/` (~1.5 GB),
3. registers the MCP server and installs the vlt Skill for each agent you pick (Claude Code, opencode, Codex),
4. writes `~/.vlt/config.toml` with default values,
5. indexes the vault.

Restart your agent session afterwards.

### 3. Ask your agent

```text
What do I know about SQLite concurrency?
Which of my projects are in progress?
Process the note projects/vlt_design.md
```

The Skill teaches the agent when to search your notes and how to enrich them. You can also search from the terminal:

```bash
vlt search "sqlite concurrency"
```

## Install

### Non-interactive

```bash
vlt install --vault ~/notes --target claude --yes
```

`--target` is repeatable; `--all` configures every supported agent; `--yes` answers yes to every confirmation.

| Agent | `--target` | MCP server registered in | Skill installed in |
|---|---|---|---|
| Claude Code | `claude` | `~/.claude.json` | `~/.claude/skills/vlt/` |
| opencode | `opencode` | `~/.config/opencode/opencode.json` (or `.jsonc`; `$XDG_CONFIG_HOME` is honoured) | `~/.config/opencode/skills/vlt/` |
| Codex | `codex` | `~/.codex/config.toml` | `~/.codex/skills/vlt/` |

Installation configures two things per agent:

1. **MCP server**: gives the agent the vlt tools.
2. **Skill**: teaches the agent when to search your notes, how to filter, and how to propose changes.

If your opencode config has comments, vlt does not rewrite it: it prints the `mcp.vlt` block for you to add by hand and carries on with the rest. vlt does not change your agent's permissions: the agent asks the first time it uses each tool.

Running `vlt install` again is safe: it keeps your `config.toml` (updating only the vault if you pick a different one) and skips models already downloaded.

### A specific version

```bash
curl -fsSL https://raw.githubusercontent.com/andresdigiovanni/vlt/main/install.sh | bash -s -- --version vX.Y.Z
```

### Upgrade

Run the install script again; it replaces the binary in `~/.local/bin`. Your config, index and models are kept.

### Manual MCP configuration

If you prefer to register the MCP server yourself, the command is `vlt mcp` (stdio). `vlt install` writes the absolute path of the binary; use it if `vlt` is not on the `PATH` your agent sees.

Claude Code (`~/.claude.json`):

```json
{
  "mcpServers": {
    "vlt": { "type": "stdio", "command": "vlt", "args": ["mcp"] }
  }
}
```

opencode:

```json
{
  "mcp": {
    "vlt": { "type": "local", "command": ["vlt", "mcp"], "enabled": true }
  }
}
```

Codex (`~/.codex/config.toml`):

```toml
[mcp_servers.vlt]
command = "vlt"
args = ["mcp"]
```

Without `vlt install` you also need `~/.vlt/config.toml` (see [Configuration](#configuration)) and `vlt reindex`, which downloads the models and builds the index.

## Usage

### With your agent

The agent uses vlt in two flows:

- **Answering.** When you ask what you know or wrote about something, or when the task depends on your notes (one of your projects, a past decision, a convention), the agent searches the vault, reads what it needs and cites the sections it used (`projects/mcp_rust.md#Concurrency`).
- **Enriching.** When you ask it to process a note, add properties to it or link it, the agent proposes one numbered list: properties, links from the note, links to it from other notes, and repairs for broken links. You pick ("apply 1, 3"), and it writes only those.

### From the terminal

```bash
vlt search "retry policy"
vlt search "retry policy" --where tags=backend --where "date>=2026-01-01" --path-prefix projects/
vlt get projects/api.md
vlt properties
```

### Keeping the index fresh

You don't need to reindex by hand. Every search, `get` and `properties` call picks up the notes you added, changed or deleted since the last one, and every write by the agent reindexes the note it changed. `vlt reindex` exists for a full rebuild (`--force`) or to warm the index ahead of time.

## CLI reference

```
vlt install [--vault <path>] [--target claude|opencode|codex]... [--all] [--yes]
vlt uninstall [--yes]
vlt mcp
vlt search <query> [--where <expr>]... [--path-prefix <p>] [--limit <n>] [--json]
vlt get <path> [--json]
vlt properties [--path-prefix <p>] [--json]
vlt reindex [--force]
```

| Command | What it does |
|---|---|
| `install` | Sets up vlt: vault, models, MCP server and Skill for each selected agent, first index. |
| `uninstall` | Removes the MCP server and the Skill from every agent. Asks before deleting `~/.vlt/`; with `--yes` it keeps `~/.vlt/`. |
| `mcp` | Runs the MCP server on stdio (the agent starts it). |
| `search` | Hybrid search. `--where` filters by property (repeatable), `--path-prefix` restricts to a folder, `--limit` overrides the configured value. |
| `get` | Shows a document (path relative to the vault): its title, path, properties and content. `--json` adds its sections, outgoing links and backlinks. |
| `properties` | Lists the frontmatter properties with their inferred type, the number of documents using them and their most frequent values. |
| `reindex` | Incremental reindex; `--force` rebuilds the index from scratch. Downloads the configured models first if they are missing. |

`--json` prints the same payload the MCP tool returns.

`--where` takes `<key><op><value>` with `op` one of `=`, `>`, `>=`, `<`, `<=`. On a list property, `=` means "contains".

Errors are printed to stderr as `error: <message>` with exit code 1.

## MCP tools

The server is named `vlt`. It exposes six tools:

| Tool | Purpose |
|---|---|
| `search` | Hybrid search with property filters, reranking and a score threshold. Parameters: `query`, `where`, `path_prefix`, `limit`, `min_score`. |
| `get` | Full document (`path`) with properties, sections, outgoing links and backlinks. |
| `properties` | Inventory of the vault's properties (optional `path_prefix`). |
| `reindex` | Detects changes in the vault and reindexes them incrementally. |
| `edit` | Replaces one exact fragment of a document's body. Parameters: `path`, `old_text`, `new_text`. |
| `update_properties` | Adds, changes or removes frontmatter properties of a document, keeping the rest of the file as it is. Parameters: `path`, `set`, `unset`. |

### What vlt writes

`edit` and `update_properties` are the only tools that write to the vault, and they only change existing `.md` files inside it. The agent calls them for the changes you approve from the list it proposes. vlt never creates, moves or deletes documents, and the `vlt` CLI never edits them.

## Configuration

`~/.vlt/config.toml`, written by `vlt install`. Unknown keys are rejected; missing keys take their defaults.

```toml
vault = "/path/to/vault"

# Paths relative to the vault that are not indexed (glob syntax; `**` crosses subdirectories).
# Example: exclude = ["templates/**", "archive/old/**"]
exclude = []

[models]
embedding = "Qwen/Qwen3-Embedding-0.6B"             # 1024 dimensions, ~1.2 GB
reranker  = "mixedbread-ai/mxbai-rerank-base-v1"    # ~350 MB
idle_unload_secs = 300                               # unload the models after 5 min without use

# Optional overrides of what the model repos declare:
# [models.embedding_options]
# revision = "main"              # branch, tag or commit to download
# pooling = "cls"                # cls, mean or last
# query_prefix = "query: "       # prepended to search queries
# document_prefix = "passage: "  # prepended to indexed text
# precision = "f32"              # f32 or f16 (default: f16 for 16-bit weights, else f32)
# [models.reranker_options]
# revision = "main"
# precision = "f32"

[search]
k = 50            # candidates per branch (BM25 and vectors)
rrf_k = 60        # Reciprocal Rank Fusion constant
rerank_top = 10   # candidates passed to the reranker; each one costs reranking time
limit = 5         # results returned by default
min_score = 0.05  # reranker threshold [0, 1]; depends on the reranker, calibrate with real documents
```

Changing `exclude` does not require a rebuild: the next incremental reindex drops the newly excluded paths and indexes the ones that are no longer excluded.

Changing `vault` or a model makes the existing index unusable: searches then fail with a message asking for a rebuild. Run `vlt reindex` to rebuild it (it takes a while on a large vault).

The default embedding model is multilingual; the default reranker was trained on English, so notes in other languages are found but ranked less precisely.

### Models

`embedding` and `reranker` take any Hugging Face repo id (`<owner>/<name>`) whose architecture vlt can run:

| Role | Architectures (`model_type` in the repo's `config.json`) | Examples |
|---|---|---|
| `embedding` | `bert`, `xlm-roberta`, `qwen3` | `BAAI/bge-small-en-v1.5`, `BAAI/bge-large-en-v1.5`, `Qwen/Qwen3-Embedding-0.6B` |
| `reranker` | `bert`, `xlm-roberta`, `deberta-v2` cross-encoders with one output (`*ForSequenceClassification`) | `cross-encoder/ms-marco-MiniLM-L-6-v2`, `BAAI/bge-reranker-base`, `mixedbread-ai/mxbai-rerank-base-v1`, `mixedbread-ai/mxbai-rerank-large-v1` |

The repo must ship `config.json`, `tokenizer.json` and safetensors weights (`model.safetensors`, or shards listed in `model.safetensors.index.json`). vlt reads everything else from the repo's sentence-transformers files: the pooling (`1_Pooling/config.json`), the query and document prompts (`config_sentence_transformers.json`) and the maximum length (`sentence_bert_config.json`, capped at 512 tokens). Set a key under `[models.embedding_options]` when the repo does not declare it or you want another value. A repo without pooling settings gets mean pooling (last-token for `qwen3`); the English BGE models get their documented query instruction.

Models compute in the precision their weights are stored in: 16-bit weights (F16, or BF16, which Candle cannot multiply on a CPU and so runs as F16) use half the memory of F32 and run faster on CPUs with native F16. `deberta-v2` rerankers always run in F32, since Candle's implementation builds F32 masks. Set `precision = "f32"` to trade memory for the exact F32 result.

`vlt reindex` (and `vlt install`) downloads a model the first time it is configured, at the revision set in the options (`main` by default). vlt checks each file against the hash Hugging Face publishes for that commit and records the commit and hashes in `vlt-lock.json` in the model's folder. From then on the model stays at that commit, even if the repo changes; set another `revision` to move it. An unsupported model is reported right after its download, with the reason. Private and gated repos are not supported.

Model size sets speed and memory on a CPU, and the reranker dominates both: it reads every candidate in full. Measured on an Apple M-series CPU, reranking 10 sections of ~290 tokens (the default `rerank_top`) takes about 5 s with `mixedbread-ai/mxbai-rerank-base-v1` (the default), 15 s with `mixedbread-ai/mxbai-rerank-large-v1`, 2.3 s with `mixedbread-ai/mxbai-rerank-xsmall-v1` and 0.8 s with `cross-encoder/ms-marco-MiniLM-L-6-v2`; time grows in proportion to `rerank_top` and to section length. With the defaults vlt peaks at about 2.3 GB of memory; `BAAI/bge-small-en-v1.5` with `cross-encoder/ms-marco-MiniLM-L-6-v2` (the defaults of vlt 0.1.0) need about 350 MB.

## Where everything lives

```text
~/.vlt/
├── config.toml   # configuration
├── index/        # SQLite database: sections, FTS5 index, vectors, links
└── models/       # embedding and reranking models
```

Your vault stays exactly as it is: vlt keeps nothing inside it and only writes there through `edit` and `update_properties`, when you approve a change.

## Uninstall

```bash
vlt uninstall
```

This removes the MCP server and the Skill from every agent and asks whether to delete `~/.vlt/` (config, index and models). With `--yes`, `~/.vlt/` is kept.

Uninstalling never touches your vault: if it lives inside `~/.vlt/`, or if `config.toml` cannot be read so vlt cannot tell where it is, `~/.vlt/` is kept and vlt says so.

Finally, remove the binary:

```bash
rm ~/.local/bin/vlt
```
