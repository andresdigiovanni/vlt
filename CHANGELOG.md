# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
