# Eval corpus

`eval/corpus/` holds 45 English Wikipedia articles on drug development and clinical research, fetched and converted to Markdown by `scripts/fetch-eval-corpus.py` (`uv run --with markdownify --with beautifulsoup4 scripts/fetch-eval-corpus.py`, from the repo root). Each article is pinned to the revision recorded in its frontmatter. The committed files, not a re-fetch, are the reference: re-fetching changes the text and can invalidate the labels. The articles were then linked to each other the way the Skill links notes: 638 wikilinks such as `[[food-and-drug-administration|FDA]]`, wrapped around words already in the text (one per target per section, none inside an evidence sentence). The script does not produce them; removing the link syntax gives back its output.

Article text © Wikipedia contributors, licensed under CC BY-SA 4.0 (https://creativecommons.org/licenses/by-sa/4.0/); each file links its source and revision. The text was converted to Markdown, stripped of references, infoboxes and navigation, and given wikilinks between the articles.

## Running

`make eval` (`cargo bench --bench eval`) indexes `eval/corpus` in a temporary vlt home, runs every query of `eval/queries.jsonl` through the search pipeline twice, as asked (`query`) and as its key terms (`terms`, the short query the Skill asks agents to send), prints the summary and per-category tables of each, then the config, and writes `target/eval/latest.json`. It uses the default models from the models dir of the vlt home, or the Hugging Face repos named by `VLT_BENCH_EMBEDDING` / `VLT_BENCH_RERANKER`, downloading any that is missing. `VLT_BENCH_SEARCH` takes the body of a `[search]` table to try other search settings without editing the defaults, e.g. `VLT_BENCH_SEARCH='rerank_top = 20' make eval`; the bench always requests 5 results. `VLT_FAKE_MODELS=1` runs on the fake models: a wiring check only, its numbers mean nothing.

## Updating the baseline

When `eval/baseline.json` exists, the bench prints each metric's change against it, with a paired-bootstrap 95% CI for Recall@5 and MRR@5. After a change passes the acceptance rule in `docs/specs/2026-10-20-rag-eval.md`, commit its run as the new baseline: `cp target/eval/latest.json eval/baseline.json`.
