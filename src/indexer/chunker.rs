//! Section-based chunking of parsed documents.

use std::collections::HashSet;

use crate::indexer::parser::{fence_open, is_fence_close, Heading, ParsedDoc};

#[derive(Debug, Clone, PartialEq)]
pub struct Chunk {
    pub chunk_id: String,
    pub heading_path: Vec<String>,
    pub start_line: usize,
    pub end_line: usize,
    pub content: String,
    pub embed_text: String,
}

pub fn embed_prefix(title: &str, heading_path: &[String]) -> String {
    let rest = match heading_path.split_first() {
        Some((first, rest)) if first == title => rest,
        _ => heading_path,
    };
    std::iter::once(title)
        .chain(rest.iter().map(String::as_str))
        .collect::<Vec<_>>()
        .join(" > ")
}

fn is_blank(line: &str) -> bool {
    line.trim().is_empty()
}

/// Returns `base` when free, else the first free `"{base} ({n})"` for n = 2, 3, ...
fn unique_id(base: String, taken: &mut HashSet<String>) -> String {
    let mut id = base.clone();
    let mut n = 2;
    while taken.contains(&id) {
        id = format!("{base} ({n})");
        n += 1;
    }
    taken.insert(id.clone());
    id
}

/// Blocks of `lines[start..end]` as `(start, end)` index ranges: runs of non-blank
/// lines, where a fenced code block (blank lines included) stays whole.
fn blocks(lines: &[&str], start: usize, end: usize) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut block_start: Option<usize> = None;
    let mut fence: Option<(u8, usize)> = None;
    for (i, line) in lines.iter().enumerate().take(end).skip(start) {
        if let Some((ch, len)) = fence {
            if is_fence_close(line, ch, len) {
                fence = None;
            }
            continue;
        }
        if is_blank(line) {
            if let Some(s) = block_start.take() {
                out.push((s, i));
            }
            continue;
        }
        block_start.get_or_insert(i);
        fence = fence_open(line);
    }
    if let Some(s) = block_start {
        out.push((s, end));
    }
    out
}

pub fn chunk(
    path: &str,
    doc: &ParsedDoc,
    max_tokens: usize,
    count_tokens: &dyn Fn(&str) -> usize,
) -> Vec<Chunk> {
    let lines: Vec<&str> = doc.body.split('\n').collect();
    let base_line = doc.body_start_line;

    // (heading, first body line index, end index exclusive)
    let mut sections: Vec<(Option<&Heading>, usize, usize)> = Vec::new();
    let intro_end = doc
        .headings
        .first()
        .map_or(lines.len(), |h| h.line - base_line);
    sections.push((None, 0, intro_end));
    for (k, h) in doc.headings.iter().enumerate() {
        let end = doc
            .headings
            .get(k + 1)
            .map_or(lines.len(), |n| n.line - base_line);
        sections.push((Some(h), h.line - base_line + 1, end));
    }

    let mut taken = HashSet::new();
    let mut chunks = Vec::new();
    for (heading, mut s, mut e) in sections {
        while s < e && is_blank(lines[s]) {
            s += 1;
        }
        while e > s && is_blank(lines[e - 1]) {
            e -= 1;
        }
        if s == e {
            continue;
        }
        let heading_path = heading.map_or_else(Vec::new, |h| h.heading_path.clone());
        let base_id = unique_id(
            format!("{path}#{}", heading.map_or("", |h| h.text.as_str())),
            &mut taken,
        );
        let prefix = embed_prefix(&doc.title, &heading_path);
        let first_start = heading.map_or(base_line + s, |h| h.line);
        let embed_of = |from: usize, to: usize| format!("{prefix}\n\n{}", lines[from..to].join("\n"));

        if count_tokens(&embed_of(s, e)) <= max_tokens {
            chunks.push(Chunk {
                chunk_id: base_id,
                heading_path,
                start_line: first_start,
                end_line: base_line + e - 1,
                content: lines[s..e].join("\n"),
                embed_text: embed_of(s, e),
            });
            continue;
        }

        let mut parts: Vec<(usize, usize)> = Vec::new();
        for (bs, be) in blocks(&lines, s, e) {
            match parts.last_mut() {
                Some((ps, pe)) if count_tokens(&embed_of(*ps, be)) <= max_tokens => *pe = be,
                _ => parts.push((bs, be)),
            }
        }
        let n = parts.len();
        for (i, (ps, pe)) in parts.into_iter().enumerate() {
            let chunk_id = if n == 1 {
                base_id.clone()
            } else {
                unique_id(format!("{base_id} [{}/{n}]", i + 1), &mut taken)
            };
            chunks.push(Chunk {
                chunk_id,
                heading_path: heading_path.clone(),
                start_line: if i == 0 { first_start } else { base_line + ps },
                end_line: base_line + pe - 1,
                content: lines[ps..pe].join("\n"),
                embed_text: embed_of(ps, pe),
            });
        }
    }
    chunks
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::indexer::parser::parse;

    fn words(s: &str) -> usize { s.split_whitespace().count() }
    fn run(text: &str, max: usize) -> Vec<Chunk> { chunk("dir/doc.md", &parse(text, "doc"), max, &words) }
    fn ids(c: &[Chunk]) -> Vec<&str> { c.iter().map(|c| c.chunk_id.as_str()).collect() }

    #[test]
    fn intro_and_sections() {
        let c = run("Intro line\n\n# Doc\n\nFirst para\n## Sub\nSub text\n", 1000);
        assert_eq!(ids(&c), vec!["dir/doc.md#", "dir/doc.md#Doc", "dir/doc.md#Sub"]);
        assert_eq!((c[0].content.as_str(), c[0].start_line, c[0].end_line), ("Intro line", 1, 1));
        assert!(c[0].heading_path.is_empty());
        assert_eq!(c[0].embed_text, "Doc\n\nIntro line");
        assert_eq!((c[1].content.as_str(), c[1].start_line, c[1].end_line), ("First para", 3, 5));
        assert_eq!(c[2].heading_path, vec!["Doc".to_string(), "Sub".to_string()]);
        assert_eq!((c[2].start_line, c[2].end_line), (6, 7));
        assert_eq!(c[2].embed_text, "Doc > Sub\n\nSub text");
    }

    #[test]
    fn prefix_keeps_first_heading_when_it_differs_from_title() {
        assert_eq!(embed_prefix("T", &["A".into(), "B".into()]), "T > A > B");
        assert_eq!(embed_prefix("T", &["T".into(), "B".into()]), "T > B");
        assert_eq!(embed_prefix("T", &[]), "T");
    }

    #[test]
    fn empty_sections_produce_no_chunk() {
        assert_eq!(ids(&run("# A\n## B\ntext\n", 1000)), vec!["dir/doc.md#B"]);
        assert!(run("# A\n\n\n# C\n", 1000).is_empty());
    }

    #[test]
    fn duplicate_headings_get_unique_suffixes() {
        let c = run("# N (2)\na\n# N\nb\n# N\nc\n", 1000);
        assert_eq!(ids(&c), vec!["dir/doc.md#N (2)", "dir/doc.md#N", "dir/doc.md#N (3)"]);
    }

    #[test]
    fn long_section_splits_by_block_without_breaking_code() {
        let text = "# T\na b c\n\n```\nx y\n\nz w\n```\n\nd e f\n";
        let c = run(text, 5);
        assert_eq!(ids(&c), vec!["dir/doc.md#T [1/3]", "dir/doc.md#T [2/3]", "dir/doc.md#T [3/3]"]);
        assert_eq!((c[0].content.as_str(), c[0].start_line, c[0].end_line), ("a b c", 1, 2));
        assert_eq!((c[1].content.as_str(), c[1].start_line, c[1].end_line), ("```\nx y\n\nz w\n```", 4, 8));
        assert_eq!((c[2].content.as_str(), c[2].start_line, c[2].end_line), ("d e f", 10, 10));
        assert!(c.iter().all(|x| x.heading_path == vec!["T".to_string()]));
    }

    #[test]
    fn frontmatter_offsets_lines_and_is_not_chunked() {
        let c = run("---\na: 1\n---\n# H\ntext\n", 1000);
        assert_eq!(c.len(), 1);
        assert_eq!((c[0].start_line, c[0].end_line), (4, 5));
        assert!(!c[0].embed_text.contains("a: 1"));
    }
    #[test]
    fn first_duplicate_gets_suffix_two() {
        let c = run("# N\na\n# N\nb\n", 1000);
        assert_eq!(ids(&c), vec!["dir/doc.md#N", "dir/doc.md#N (2)"]);
    }

    #[test]
    fn token_limit_is_inclusive() {
        assert_eq!(ids(&run("# T\na b\n", 3)), vec!["dir/doc.md#T"], "3 tokens fit a limit of 3");
        assert_eq!(ids(&run("# T\na b\n", 2)), vec!["dir/doc.md#T"], "a one-part split keeps the plain id");
        let c = run("# T\na\n\nb\n\nc\n\nd\n", 3);
        assert_eq!(c.iter().map(|x| x.content.as_str()).collect::<Vec<_>>(), vec!["a\n\nb", "c\n\nd"]);
        assert_eq!((c[0].start_line, c[0].end_line, c[1].start_line, c[1].end_line), (1, 4, 6, 8));
    }

    #[test]
    fn part_ids_never_collide_with_later_headings() {
        let only_letters = |s: &str| s.split_whitespace().filter(|w| ["a", "b", "x"].contains(w)).count();
        let doc = parse("# T\na\n\nb\n# T [1/2]\nx\n", "doc");
        let c = chunk("dir/doc.md", &doc, 1, &only_letters);
        assert_eq!(ids(&c), vec!["dir/doc.md#T [1/2]", "dir/doc.md#T [2/2]", "dir/doc.md#T [1/2] (2)"]);
    }

    #[test]
    fn intro_starts_at_its_first_non_blank_line() {
        let c = run("\n\nIntro\n# H\nx\n", 1000);
        assert_eq!(ids(&c), vec!["dir/doc.md#", "dir/doc.md#H"]);
        assert_eq!(c[0].start_line, 3);
    }

    #[test]
    fn whitespace_only_lines_count_as_blank() {
        assert_eq!(ids(&run("# A\n  \t \n# B\nx", 1000)), vec!["dir/doc.md#B"]);
    }

    #[test]
    fn only_sections_with_a_chunk_reserve_ids() {
        assert_eq!(ids(&run("# N\n\n# N\na\n", 1000)), vec!["dir/doc.md#N"]);
    }
}
