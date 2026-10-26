---
name: vlt
description: Searches and enriches the user's own markdown notes (their vault) through the vlt MCP tools. Use when the user asks what they know or wrote about something ("what do I know about X", "check my notes on X"); when the task depends on something only their notes would hold (one of their projects, a past decision, a personal convention); when they ask to process the note X, add properties to it, or link it with other notes; when they ask you to draft a new note for the vault; or when they ask you to audit the vault or one of its folders. Not for general knowledge (language syntax, public APIs), and not to move or delete notes.
---

# vlt

The vault is the user's folder of markdown notes: their projects, decisions and conventions. vlt indexes it and serves it through MCP tools.

- **Touch the vault only through the vlt tools**, never with native file or shell tools: vlt keeps paths inside the vault, preserves the frontmatter format and reindexes after every write.
- **Write only what the user picked.** `edit` and `update_properties` run only for items the user chose from your numbered list ("apply 1, 3"); `create` only for a draft the user approved. Even when the request names the exact change, show it as a one-item list and wait.
- **vlt creates a note only through `create`**, for a draft the user approved (Drafting a new note); it never overwrites, moves or deletes notes.

You never need `reindex` in these flows: every read tool picks up outside changes first, and every write reindexes.

## Which flow

| The user... | Flow |
|---|---|
| asks what they know or wrote, or your answer depends on what their notes say | Answering |
| names a note and asks to process it | Enriching, all groups |
| names a note and asks for properties only | Enriching, group a only |
| names a note and asks to link it | Enriching, groups b–d |
| asks for one specific property or link change | Enriching, that change only (skip the searches it does not need) |
| asks to create a note, or to write something up as a note | Drafting a new note |
| asks to audit, check or clean up the vault, or a folder of it | Auditing the vault |
| asks to move or delete a note | None: say vlt cannot, and what the user would do in their editor |

Never start Enriching on your own. While answering, if you notice a missing property, a missing link or an outdated note, say so in one line and offer to process it.

## Answering

1. `search` with a short English query of key terms ("sqlite concurrency wal"), not the whole question. Name things by the term a note would use, not by a description of them: "declaration of helsinki", not "the 1964 ethics code for doctors". The notes and models are in English; answer in the user's language.
2. When the user names a property, value or folder ("my in-progress projects", "notes tagged rust"), call `properties` first and filter with the keys and values it returns, never guessed ones:
   - `where` scalar: `{"status": "in_progress"}`; on a list property: `{"tags": "rust"}` (contains), `{"tags": ["rust", "mcp"]}` (contains all); a tag also matches its nested tags: `{"tags": "project"}` finds `project/vlt`, and `tags` also matches `#tags` written in the body;
   - dates and numbers: `{"date": {"gte": "2026-01-01", "lt": "2026-07-01"}}` (`gt`, `gte`, `lt`, `lte`);
   - several keys combine with AND; a folder goes in `path_prefix`, e.g. `"projects/"`.
3. `get` the document only when the returned section is not enough.
4. Cite the `chunk_id` of each section you use (`projects/mcp_rust.md#Concurrency`) and keep what the vault says apart from what you add.

A section can be on the topic without answering the question: read its `content` before you use it. No results, or none that answers, means nothing relevant: reformulate once (the technical term, a synonym, a broader term), then say the vault has nothing on it. Never lower `min_score` to force results.

Answering never writes. If notes look outdated or contradict each other, propose the fix as a numbered list.

## Drafting a new note

You draft the note; once the user approves it, `create` writes it.

1. `search` the topic first. When a note already covers it, propose extending that note instead, as Enriching items, and say which note and why.
2. Pick the folder where the closest hits live (`folders` lists every folder with its notes and most used keys, to check it), then `properties({"path_prefix": "<folder>"})`, the folder's path as `folders` writes it, ending in `/` (no `path_prefix` for the vault root): give the note the keys most of the folder's notes have, with their spellings, types and values (Properties below).
3. Name the file like its siblings in that folder (case, separators, language), as the hits' `document.path` show.
4. Write the body in English with ATX headings. Each section is retrieved without the rest of the note: give each heading the name of its topic ("Retry policy", not "Details") and name the subject in its first sentence instead of "it". Put links to the notes your searches returned in the prose, per Links below.
5. Show the path and the whole file, frontmatter included, in one code block, and wait.
6. On approval, `create({"path": "<path>", "content": "<file>"})` with exactly that path and file. On `already_exists`, say so and propose another path, or offer to process the note that is there.
7. Then `get` the `path` `create` returned (it may differ in case from the one you sent) and offer Enriching groups c and d: a new note has no backlinks yet, and links to it are what lead other sections to it.

## Auditing the vault

`audit` lists what only shows across notes. Fix it the way Enriching does: one numbered list, applied only as picked.

1. `audit`; for one folder, `{"path_prefix": "<folder>"}` with the folder's path as `folders` writes it, ending in `/` (no `path_prefix` for the vault root).
2. One numbered list, in the user's language, grouped:
   - **a. Spellings**: for each `value_variants` group, change that value to the first spelling in each note that uses another (the rest of a list stays as it is); for each `key_variants` group, rename the other keys to the first (`update_properties` with `set` and `unset`). One item per group, naming the notes it changes (its `notes`).
   - **b. Folder gaps**: one item per missing note: add the key with a value its siblings use, or the tag. Every write follows Properties: `set` replaces the whole value, so send the note's current list with the one spelling changed or the tag added, and keep each value's type.
   - **c. Links**: `broken_links` repairs as in Enriching; for `orphans` and `no_outgoing`, offer to process the note (Enriching groups b–d) instead of editing it here.

   Mention `rare_keys` in one line: they are often a typo of another key; propose a fix only when one is clear. Keep the list to about 10 items; when more remain, say how many and offer them. A section whose `total` is above its items has more than the 50 listed: offer to audit a folder at a time.
3. Apply exactly the picked items (`get` each note first, per Editing; values per Properties), then `audit` again and report what changed.

## Enriching a note

1. `get(X)`: content, properties, links with their `status`, backlinks.
2. Properties (group a): `properties({})` for the vault's vocabulary and `properties({"path_prefix": "<folder of X>/"})` for what sibling notes use. Propose the keys X lacks that most siblings have: a key whose `documents` is close to that of the folder's most used key. When the folder mixes kinds of notes, `properties({"where": {"type": "<X's type>"}})` (or another key X shares with its peers) gives the keys of notes like X.
3. Links (groups b, c): `related({"path": X})` lists the notes closest to X, each with the section of X it matches (`from`: a group b link goes there) and its own section (`to`: a group c link goes there), and `linked` (`to`, `from`, `both`, `none`) for the links that already exist. Its `mentions` are sections of other notes that name X (title, alias or file name) without a link: group c. `search` only a topic of X that `related` does not cover; skip hits from X itself. `document.properties` on the hits also shows real values.
4. Propose ONE list, numbered across groups, in the user's language:
   - **a. Properties of X**: current → proposed.
   - **b. Links from X**: a sentence in X names a topic another note documents.
   - **c. Links to X**: a sentence in another note names what X documents; this is how X gets backlinks.
   - **d. Repairs**: links in X with `status` `broken` or `ambiguous`, and their fix. Include them whenever links are in scope.

   Each item is one change the user can accept alone: the document, the exact change (sentence before → after, quoted as written) and one line of why. Say in one line when a group is empty. Order each group strongest first and keep groups a–c to about 10 items in all; when more remain, say how many and offer them. Repairs never count toward that limit.
5. Wait for the pick ("apply 1, 3", "all", "1 but with tag rust"), then apply exactly those items and check each response's `diff`.
6. `get(X)` again: every link you wrote has `status` `ok`, every document you linked from is in `backlinks`. Report what was applied in a short list. A link still not `ok`: report it with its candidate paths instead of retrying.

### Properties

- Reuse the keys and spellings `properties` returns (`in_progress`, not `in progress`) and their type: lists stay lists, dates are ISO 8601 strings, numbers stay numbers.
- A key's `variants` lists spellings of one value that differ only in case or separators (`Rust`/`rust`, `machine-learning`/`machine_learning`), most used first: use the first and mention the others in one line. Other near-duplicates (`ml` and `machine-learning`) are yours to spot: use the one with the most documents.
- `properties` shows each key's 10 most frequent values. Before calling a value new, `properties({"key": "tags"})` lists every value of that key: reuse an existing one, or the closest spelling.
- A key or value no note uses yet is marked as new in the list, with its reason.
- `get(X).tags` lists every tag X has, frontmatter and body `#tags`: a tag in it is already there, even when only in the body; never propose it again.
- `related(X).suggested_tags` are tags that at least two of the notes closest to X carry: prefer them to a new tag.
- `set` only the keys that change. It replaces the whole value: to add a tag, send X's `properties.tags` (the frontmatter) plus the new one, never `get(X).tags`, which would copy the body's tags into the frontmatter.
- Values are strings, numbers, booleans or lists of those. Remove a key with `unset`, never with `null`.

### Links

vlt searches by section: a section is retrieved and read without the rest of its note, so a link only helps the section that contains it. Hence:

- **Put each link in the prose, in the sentence that discusses the topic.** Never add a "Related notes" or "See also" list, not even when asked to "add links".
- **Link per section.** Each section that discusses the topic gets its own link; never two links to the same target in one section.
- **Change as little as possible.** Wrap words that already name the topic: `[[projects/mcp_rust|the MCP server]]`. If no sentence takes the link naturally, skip it and say so; never add a sentence or section just to hold a link.
- **Format**: the vault-relative path without `.md`, `[[projects/mcp_rust]]`; for a section, `[[projects/mcp_rust#Concurrency]]`, using the heading text from `heading_path`, never a `chunk_id` suffix like ` [2/3]` or ` (2)`. Inserted text is in English; never put a link inside code.
- **Skip links that already exist** in that section, and check the other direction too: X already links to T when `get(X).links` has an entry whose `resolved` is T's path, and T links to X when T is in `get(X).backlinks`. That only tells you the note links somewhere; a section that mentions the topic without a link still gets one.
- **Repairs**: for an `ambiguous` link, its `candidates` are the notes it could mean; for a `broken` one, `search` for the intended note. Propose the full-path form. If you cannot tell which note was meant, ask in the list. A link with `property` is in the frontmatter: fix it with `update_properties`, setting that key to its value with the fixed link (a list keeps its other items; vlt quotes the `[[...]]` string), never with `edit`.

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
- `invalid_frontmatter`: the YAML does not parse. Ask the user to fix it in their editor; never repair it with `edit`. For a draft sent to `create`, fix its frontmatter and show it again.
- `already_exists`: a note or folder is already at that path, and nothing was written. Propose another path, or offer to process the note that is there.
- `internal`: if the message says the index is being built, retry shortly; otherwise report it and do not retry.

## Example

The user says "process the note projects/vlt_design.md".

1. `get({"path": "projects/vlt_design.md"})`: `tags: [rust]`, `aliases: [vlt]`, no links, no backlinks. The body says "Concurrency follows the same approach as the MCP server."
2. `properties` shows project notes use `status` (`in_progress`, `done`, `idea`) and `tags` (`rust`, `mcp`, `search`).
3. `related({"path": "projects/vlt_design.md"})`: its first note is `projects/mcp_rust.md` (`from` `projects/vlt_design.md#Concurrency`, `to` `projects/mcp_rust.md#Concurrency`, `linked` `none`); `suggested_tags` is `[["search", 2]]`; `mentions` has `areas/search.md#Engines`, line 12, name `vlt` ("I compared engines for local semantic search before building vlt.").
4. The proposal:

   > 1. `projects/vlt_design.md`: `tags` `[rust]` → `[rust, search]` (two of the closest notes carry it: `suggested_tags`).
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
