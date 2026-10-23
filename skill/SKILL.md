---
name: vlt
description: Searches and enriches the user's own markdown notes (their vault) through the vlt MCP tools. Use when the user asks what they know or wrote about something ("what do I know about X", "check my notes on X"); when the task depends on something only their notes would hold (one of their projects, a past decision, a personal convention); or when they ask to process the note X, add properties to it, or link it with other notes. Not for general knowledge (language syntax, public APIs), and not to create, move or delete notes.
---

# vlt

The vault is the user's folder of markdown notes: their projects, decisions and conventions. vlt indexes it and serves it through MCP tools.

- **Touch the vault only through the vlt tools**, never with native file or shell tools: vlt keeps paths inside the vault, preserves the frontmatter format and reindexes after every write.
- **Write only what the user picked.** `edit` and `update_properties` run only for items the user chose from your numbered list ("apply 1, 3"). Even when the request names the exact change, show it as a one-item list and wait.
- **vlt never creates, moves or deletes notes.** For something new, propose the content and a path; the user creates the file and can then ask you to process it.

You never need `reindex` in these flows: every read tool picks up outside changes first, and every write reindexes.

## Which flow

| The user... | Flow |
|---|---|
| asks what they know or wrote, or your answer depends on what their notes say | Answering |
| names a note and asks to process it | Enriching, all groups |
| names a note and asks for properties only | Enriching, group a only |
| names a note and asks to link it | Enriching, groups b–d |
| asks for one specific property or link change | Enriching, that change only (skip the searches it does not need) |
| asks to create, move or delete a note | None: propose content and a path |

Never start Enriching on your own. While answering, if you notice a missing property, a missing link or an outdated note, say so in one line and offer to process it.

## Answering

1. `search` with a short English query of key terms ("sqlite concurrency wal"), not the whole question. Name things by the term a note would use, not by a description of them: "declaration of helsinki", not "the 1964 ethics code for doctors". The notes and models are in English; answer in the user's language.
2. When the user names a property, value or folder ("my in-progress projects", "notes tagged rust"), call `properties` first and filter with the keys and values it returns, never guessed ones:
   - `where` scalar: `{"status": "in_progress"}`; on a list property: `{"tags": "rust"}` (contains), `{"tags": ["rust", "mcp"]}` (contains all);
   - dates and numbers: `{"date": {"gte": "2026-01-01", "lt": "2026-07-01"}}` (`gt`, `gte`, `lt`, `lte`);
   - several keys combine with AND; a folder goes in `path_prefix`, e.g. `"projects/"`.
3. `get` the document only when the returned section is not enough.
4. Cite the `chunk_id` of each section you use (`projects/mcp_rust.md#Concurrency`) and keep what the vault says apart from what you add.

A section can be on the topic without answering the question: read its `content` before you use it. No results, or none that answers, means nothing relevant: reformulate once (the technical term, a synonym, a broader term), then say the vault has nothing on it. Never lower `min_score` to force results.

Answering never writes. If notes look outdated or contradict each other, propose the fix as a numbered list.

## Enriching a note

1. `get(X)`: content, properties, links with their `status`, backlinks.
2. Properties (group a): `properties({})` for the vault's vocabulary and `properties({"path_prefix": "<folder of X>/"})` for what sibling notes use.
3. Links (groups b, c): `search` once per topic X covers; skip hits from X itself. `document.properties` on the hits also shows real values.
4. Propose ONE list, numbered across groups, in the user's language:
   - **a. Properties of X**: current → proposed.
   - **b. Links from X**: a sentence in X names a topic another note documents.
   - **c. Links to X**: a sentence in another note names what X documents; this is how X gets backlinks.
   - **d. Repairs**: links in X with `status` `broken` or `ambiguous`, and their fix. Include them whenever links are in scope.

   Each item is one change the user can accept alone: the document, the exact change (sentence before → after, quoted as written) and one line of why. Say in one line when a group is empty.
5. Wait for the pick ("apply 1, 3", "all", "1 but with tag rust"), then apply exactly those items and check each response's `diff`.
6. `get(X)` again: every link you wrote has `status` `ok`, every document you linked from is in `backlinks`. Report what was applied in a short list. A link still not `ok`: report it with its candidate paths instead of retrying.

### Properties

- Reuse the keys and spellings `properties` returns (`in_progress`, not `in progress`) and their type: lists stay lists, dates are ISO 8601 strings, numbers stay numbers.
- A key no note uses yet is marked as new in the list, with its reason.
- `set` only the keys that change. It replaces the whole value: to add a tag, send the current tags plus the new one.
- Values are strings, numbers, booleans or lists of those. Remove a key with `unset`, never with `null`.

### Links

vlt searches by section: a section is retrieved and read without the rest of its note, so a link only helps the section that contains it. Hence:

- **Put each link in the prose, in the sentence that discusses the topic.** Never add a "Related notes" or "See also" list, not even when asked to "add links".
- **Link per section.** Each section that discusses the topic gets its own link; never two links to the same target in one section.
- **Change as little as possible.** Wrap words that already name the topic: `[[projects/mcp_rust|the MCP server]]`. If no sentence takes the link naturally, skip it and say so; never add a sentence or section just to hold a link.
- **Format**: the vault-relative path without `.md`, `[[projects/mcp_rust]]`; for a section, `[[projects/mcp_rust#Concurrency]]`, using the heading text from `heading_path`, never a `chunk_id` suffix like ` [2/3]` or ` (2)`. Inserted text is in English; never put a link inside code.
- **Skip links that already exist** in that section, and check the other direction too: X already links to T when `get(X).links` has an entry whose `resolved` is T's path, and T links to X when T is in `get(X).backlinks`. That only tells you the note links somewhere; a section that mentions the topic without a link still gets one.
- **Repairs**: `search` for the intended note and propose the full-path form. If you cannot tell which note was meant, ask in the list.

### Editing

- `get` each document right before editing it, and copy `old_text` literally from its `content`, with just enough context to be unique. Never from memory or from a `search` snippet.
- `edit` changes the body only; properties go through `update_properties`.
- One `edit` per insertion; two insertions in the same sentence go in one `edit`.

## Errors

Errors arrive as `{"error": {"code": ..., "message": ...}}`. Retry at most once.

- `index_rebuild_required`: the index was built for another model or vault, or by an older vlt. Ask the user to run `vlt reindex --force` in a terminal, then retry.
- `document_not_found`: the path is not indexed (mistyped, or excluded in `~/.vlt/config.toml`). Use a `document.path` from `search`; never guess paths. If the user named the path, tell them.
- `path_outside_vault`: use the vault-relative paths that `search` and `get` return.
- `invalid_arguments`: fix the call as the message says.
- `not_found`: `old_text` is not in the body. `get` the document again and copy it from the new `content`.
- `ambiguous`: `old_text` appears more than once; widen it until it is unique.
- `file_changed`: the file changed during the write and nothing was saved. `get` it again and rebuild the change.
- `invalid_frontmatter`: the YAML does not parse. Ask the user to fix it in their editor; never repair it with `edit`.
- `internal`: if the message says the index is being built, retry shortly; otherwise report it and do not retry.

## Example

The user says "process the note projects/vlt_design.md".

1. `get({"path": "projects/vlt_design.md"})`: `tags: [rust]`, no links, no backlinks. The body says "Concurrency follows the same approach as the MCP server."
2. `properties` shows project notes use `status` (`in_progress`, `done`, `idea`) and `tags` (`rust`, `mcp`, `search`).
3. `search({"query": "mcp server concurrency"})` and `search({"query": "local semantic search engine"})` return `projects/mcp_rust.md#Concurrency` and `areas/search.md#Engines` ("I compared engines for local semantic search before building vlt.").
4. The proposal:

   > 1. `projects/vlt_design.md`: `tags` `[rust]` → `[rust, search]` (`search` is already used in `projects/`).
   > 2. `projects/vlt_design.md`: `status` (none) → `in_progress` (the value project notes use for active work).
   > 3. Link in `projects/vlt_design.md`: "the same approach as the MCP server." → "the same approach as [[projects/mcp_rust#Concurrency|the MCP server]]."
   > 4. Link in `areas/search.md`: "before building vlt." → "before building [[projects/vlt_design|vlt]]."
   >
   > No sentence in `projects/mcp_rust.md` mentions vlt, so no link back from it. No broken or ambiguous links.

5. The user answers "apply 1, 2 and 4". Get `areas/search.md` to copy `old_text` from its `content`, then:

   ```json
   update_properties({"path": "projects/vlt_design.md", "set": {"tags": ["rust", "search"], "status": "in_progress"}})
   edit({"path": "areas/search.md",
         "old_text": "semantic search before building vlt.",
         "new_text": "semantic search before building [[projects/vlt_design|vlt]]."})
   ```

6. `get({"path": "projects/vlt_design.md"})`: `backlinks` now lists `areas/search.md`. Report: `tags` and `status` updated, `areas/search.md` links to the note, item 3 not applied.
