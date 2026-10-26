# AGENTS.md

## Project

Rust CLI and stdio MCP server (`vlt`, package `vlt`): hybrid semantic search over a vault (a folder of markdown notes), backed by SQLite (FTS5 + sqlite-vec) and local Candle models. vlt writes to the vault only through the MCP tools `create` (new notes, never overwriting), `edit` and `update_properties`, when the agent calls them; everything else vlt owns lives under `~/.vlt/`.

## Validation

`make help` lists the task shortcuts (`make check` = lint + test, `make ci` adds the locked release build, `make bench` runs the benchmarks); each one wraps the cargo command below.

- `cargo test` — unit tests plus the fast integration tests (`tests/`); must pass before merging. It runs on deterministic fake models.
- `cargo clippy --all-targets -- -D warnings` — must be clean.
- `cargo build --release --locked` — CI releases build locked; keep `Cargo.lock` in sync when editing `Cargo.toml` (after a version bump: `cargo update -p vlt`).
- `cargo bench --bench real_models` — the real-model checks (prints PASS/FAIL per check, exits non-zero on a failure). They load the default models (`Qwen/Qwen3-Embedding-0.6B`, `mixedbread-ai/mxbai-rerank-base-v1`), or the Hugging Face repos named by `VLT_BENCH_EMBEDDING` / `VLT_BENCH_RERANKER`, from the models dir of the vlt home (`~/.vlt/models/`, or `$VLT_HOME/models/`), downloading any that is missing. Run them for every architecture a change touches (see README → Models).
- `cargo bench --bench eval` (`make eval`) — retrieval quality (Recall@5, MRR@5, stage recall…) on `eval/corpus` with `eval/queries.jsonl`, against `eval/baseline.json`; same models as `real_models`, plus `VLT_BENCH_SEARCH='<[search] keys>'` to try search settings. Writes `target/eval/latest.json`; a change that the spec's acceptance rule keeps commits that file as the new `eval/baseline.json`.
- `cargo bench --bench latency` — the PRD 10 latency benchmark (synthetic 5,000-document vault, ~17 min); needs the same models. It prints a table and asserts nothing.

## Release

Releases run via the manual `Release` workflow (`.github/workflows/release.yml`), never by hand-tagging:

1. Bump `version` in `Cargo.toml` + `cargo update -p vlt`; commit both.
2. Write release notes under `## [Unreleased]` in `CHANGELOG.md` (the workflow promotes them into `## [<version>]` and auto-commits, via `scripts/prepare-release.sh`).
3. Push, then run Actions → Release → Run workflow.

The workflow builds 4 platform binaries (macOS and Linux, x86_64 and aarch64), publishes them as GitHub Release assets with `SHA256SUMS` + provenance attestations, and is safe to re-run for the same version.
