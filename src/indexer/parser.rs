//! Markdown parsing: frontmatter, ATX headings and links.

use serde_json::{Map, Value};
use crate::write::frontmatter::key_of;
use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkKind {
    Wiki,
    Embed,
    Markdown,
}

impl LinkKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            LinkKind::Wiki => "wiki",
            LinkKind::Embed => "embed",
            LinkKind::Markdown => "markdown",
        }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<LinkKind> {
        match s {
            "wiki" => Some(LinkKind::Wiki),
            "embed" => Some(LinkKind::Embed),
            "markdown" => Some(LinkKind::Markdown),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Heading {
    pub level: u8,
    pub text: String,
    pub heading_path: Vec<String>,
    pub line: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RawLink {
    pub kind: LinkKind,
    /// As written (wiki: without `#`/`|`; markdown: percent-decoded path, without `#fragment`).
    pub target: String,
    /// Text after the first `#`; the markdown fragment is percent-decoded.
    pub heading: Option<String>,
    /// Exact source text of the link.
    pub raw: String,
    pub line: usize,
    /// The top-level frontmatter key the link is in; `None` for a link in the body.
    pub property: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedDoc {
    pub properties: Option<Map<String, Value>>,
    pub frontmatter_error: Option<String>,
    pub title: String,
    /// Text after the frontmatter, CRLF normalized to LF.
    pub body: String,
    /// File line (1-based) where `body` starts.
    pub body_start_line: usize,
    pub headings: Vec<Heading>,
    pub links: Vec<RawLink>,
    /// Frontmatter `tags`, then body `#tags`, deduplicated case-insensitively (first spelling kept).
    pub tags: Vec<String>,
}

pub fn parse(text: &str, file_stem: &str) -> ParsedDoc {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let text = text.replace("\r\n", "\n");

    let (properties, frontmatter_error, body, body_start_line) = split_frontmatter(&text);

    let mut headings: Vec<Heading> = Vec::new();
    let mut links = Vec::new();
    if let Some(p) = &properties {
        frontmatter_links(&text, p, body_start_line, &mut links);
    }
    let mut body_tags = Vec::new();
    let mut stack: Vec<(u8, String)> = Vec::new();
    let mut fence: Option<(u8, usize)> = None;

    for (i, line) in body.split('\n').enumerate() {
        let line_no = body_start_line + i;
        if let Some((ch, len)) = fence {
            if is_fence_close(line, ch, len) {
                fence = None;
            }
            continue;
        }
        if let Some(open) = fence_open(line) {
            fence = Some(open);
            continue;
        }
        if let Some((level, text)) = atx_heading(line) {
            while stack.last().is_some_and(|(l, _)| *l >= level) {
                stack.pop();
            }
            stack.push((level, text.clone()));
            headings.push(Heading {
                level,
                text,
                heading_path: stack.iter().map(|(_, t)| t.clone()).collect(),
                line: line_no,
            });
        }
        scan_links(line, line_no, &mut links);
        scan_tags(line, &mut body_tags);
    }

    let title = properties
        .as_ref()
        .and_then(|p| p.get("title"))
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .or_else(|| {
            headings
                .iter()
                .find(|h| h.level == 1)
                .map(|h| h.text.clone())
        })
        .unwrap_or_else(|| file_stem.to_string());

    let tags = effective_tags(frontmatter_tags(properties.as_ref()), body_tags);

    ParsedDoc {
        properties,
        frontmatter_error,
        title,
        body,
        body_start_line,
        headings,
        links,
        tags,
    }
}

/// Obsidian's frontmatter `tags`: a string or a list of strings, each split on commas and whitespace,
/// each piece cut like a body tag (leading quotes and one `#` dropped, cut at the first non-tag character,
/// a trailing `/` trimmed); non-string values and empty or all-digit pieces are not tags.
fn frontmatter_tags(properties: Option<&Map<String, Value>>) -> Vec<String> {
    let values = match properties.and_then(|p| p.get("tags")) {
        Some(Value::Array(items)) => items.as_slice(),
        Some(v) => std::slice::from_ref(v),
        None => &[],
    };
    values
        .iter()
        .filter_map(Value::as_str)
        .flat_map(|s| s.split(|c: char| c == ',' || c.is_whitespace()))
        .map(|t| {
            let t = t.trim_start_matches(['"', '\'']);
            leading_tag(t.strip_prefix('#').unwrap_or(t))
        })
        .filter(|t| is_tag(t))
        .map(str::to_string)
        .collect()
}

fn is_tag(t: &str) -> bool {
    !t.is_empty() && !t.chars().all(|c| c.is_ascii_digit())
}

/// Letters, digits, `_`, `-`, `/`, and any non-ASCII character but whitespace (emoji, symbols).
fn is_tag_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '_' | '-' | '/') || (!c.is_ascii() && !c.is_whitespace())
}

/// The tag at the start of `s` (after its `#`): the run of tag characters, a trailing `/` trimmed.
fn leading_tag(s: &str) -> &str {
    let len: usize = s.chars().take_while(|&c| is_tag_char(c)).map(char::len_utf8).sum();
    s[..len].trim_end_matches('/')
}

/// The `#tags` of one body line, as Obsidian reads them: a `#` at the line start or after whitespace,
/// outside code spans; a trailing `/` is trimmed.
fn scan_tags(line: &str, out: &mut Vec<String>) {
    if !line.contains('#') {
        return;
    }
    let b = line.as_bytes();
    let mut i = 0;
    let mut after_space = true;
    while i < b.len() {
        match b[i] {
            b'`' => {
                let run = b[i..].iter().take_while(|&&c| c == b'`').count();
                i = skip_code_span(b, i, run);
                after_space = false;
            }
            b'#' if after_space => {
                let rest = &line[i + 1..];
                let len: usize = rest.chars().take_while(|&c| is_tag_char(c)).map(char::len_utf8).sum();
                let tag = leading_tag(rest);
                if is_tag(tag) {
                    out.push(tag.to_string());
                }
                i += 1 + len;
                after_space = false;
            }
            _ => {
                let c = line[i..].chars().next().expect("i is on a char boundary");
                after_space = c.is_whitespace();
                i += c.len_utf8();
            }
        }
    }
}

fn effective_tags(frontmatter: Vec<String>, body: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    frontmatter.into_iter().chain(body).filter(|t| seen.insert(t.to_lowercase())).collect()
}

pub(crate) type Frontmatter = (Option<Map<String, Value>>, Option<String>, String, usize);

pub(crate) fn split_frontmatter(text: &str) -> Frontmatter {
    let mut lines = text.split_inclusive('\n');
    let no_frontmatter = || (None, None, text.to_string(), 1);
    match lines.next() {
        Some(first) if first.strip_suffix('\n').unwrap_or(first) == "---" => {}
        _ => return no_frontmatter(),
    }
    let yaml_start = 4; // "---\n"
    let mut offset = yaml_start;
    for (i, line) in lines.enumerate() {
        let content = line.strip_suffix('\n').unwrap_or(line);
        if content == "---" || content == "..." {
            let yaml = &text[yaml_start..offset];
            let body = text[offset + line.len()..].to_string();
            // Line 1 is the opener; this closer is line i + 2; the body starts on the next line.
            let body_start_line = i + 3;
            let (properties, error) = match serde_yaml_ng::from_str::<Value>(yaml) {
                Ok(Value::Object(map)) => (Some(map), None),
                Ok(Value::Null) => (Some(Map::new()), None),
                Ok(_) => (None, Some("frontmatter is not a mapping".to_string())),
                Err(e) => (None, Some(e.to_string())),
            };
            return (properties, error, body, body_start_line);
        }
        offset += line.len();
    }
    no_frontmatter()
}

/// Strips up to three leading spaces; `None` when the line is indented further.
fn strip_indent(line: &str) -> Option<&str> {
    let n = line.len() - line.trim_start_matches(' ').len();
    (n <= 3).then(|| &line[n..])
}

pub(crate) fn fence_open(line: &str) -> Option<(u8, usize)> {
    let rest = strip_indent(line)?;
    let ch = *rest.as_bytes().first()?;
    if ch != b'`' && ch != b'~' {
        return None;
    }
    let len = rest.len() - rest.trim_start_matches(ch as char).len();
    if len < 3 {
        return None;
    }
    if ch == b'`' && rest[len..].contains('`') {
        return None;
    }
    Some((ch, len))
}

pub(crate) fn is_fence_close(line: &str, ch: u8, open_len: usize) -> bool {
    let Some(rest) = strip_indent(line) else {
        return false;
    };
    let after = rest.trim_start_matches(ch as char);
    let len = rest.len() - after.len();
    len >= open_len && after.trim().is_empty()
}

fn atx_heading(line: &str) -> Option<(u8, String)> {
    let rest = strip_indent(line)?;
    let hashes = rest.len() - rest.trim_start_matches('#').len();
    if !(1..=6).contains(&hashes) {
        return None;
    }
    let content = rest[hashes..].strip_prefix([' ', '\t'])?;
    let mut text = content.trim_end();
    let without = text.trim_end_matches('#');
    if without.len() < text.len() && (without.is_empty() || without.ends_with([' ', '\t'])) {
        text = without;
    }
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    Some((hashes as u8, text.to_string()))
}

/// Bracket matches of one line, computed in a single pass so each `[` is resolved in constant time.
struct Brackets {
    /// For each `[` byte, the index of its matching `]`, nested brackets counted.
    close: Vec<Option<usize>>,
    /// Start of the last `]]` in the line.
    last_wiki_close: Option<usize>,
}

impl Brackets {
    fn new(line: &str) -> Brackets {
        let b = line.as_bytes();
        let mut close = vec![None; b.len()];
        let mut open = Vec::new();
        for (j, &c) in b.iter().enumerate() {
            match c {
                b'[' => open.push(j),
                b']' => {
                    if let Some(o) = open.pop() {
                        close[o] = Some(j);
                    }
                }
                _ => {}
            }
        }
        Brackets { close, last_wiki_close: line.rfind("]]") }
    }
}

/// Links in property values (Obsidian 1.4+), each with the top-level key whose entry holds its line.
/// Obsidian reads a link only in a quoted string, so a link counts only when a string of its key's value holds it.
fn frontmatter_links(text: &str, properties: &Map<String, Value>, body_start_line: usize, out: &mut Vec<RawLink>) {
    let mut key: Option<String> = None;
    // The YAML is file lines 2..=body_start_line - 2; line index i is file line i + 1.
    for (i, line) in text.split('\n').enumerate().take(body_start_line - 2).skip(1) {
        if let Some((k, _)) = key_of(line) {
            key = Some(k);
        }
        if line.trim_start().starts_with('#') {
            continue;
        }
        let Some((k, value)) = key.as_ref().and_then(|k| properties.get(k).map(|v| (k, v))) else {
            continue;
        };
        let mut found = Vec::new();
        scan_links(line, i + 1, &mut found);
        out.extend(
            found.into_iter().filter(|l| holds_text(value, &l.raw)).map(|l| RawLink { property: Some(k.clone()), ..l }),
        );
    }
}

/// Whether a string in `value`, or in a list at any depth, contains `raw`.
fn holds_text(value: &Value, raw: &str) -> bool {
    match value {
        Value::String(s) => s.contains(raw),
        Value::Array(items) => items.iter().any(|v| holds_text(v, raw)),
        _ => false,
    }
}

/// `line` with its links (`[[…]]`, `![[…]]`, `[…](…)`) and code spans replaced by spaces, byte for byte:
/// the prose a mention can be in.
pub(crate) fn prose(line: &str) -> String {
    if !line.contains(['[', '`']) {
        return line.to_string();
    }
    let brackets = Brackets::new(line);
    let b = line.as_bytes();
    let mut out = b.to_vec();
    let mut scratch = Vec::new();
    let mut i = 0;
    while i < b.len() {
        // (end, matched): a scanner that found nothing returns its failure index.
        let (end, matched) = match b[i] {
            b'`' => {
                let run = b[i..].iter().take_while(|&&c| c == b'`').count();
                let end = skip_code_span(b, i, run);
                (end, end != i + run)
            }
            b'!' if line[i..].starts_with("![[") => {
                let end = wiki_link(line, &brackets, i, 3, LinkKind::Embed, 0, &mut scratch);
                (end, end != i + 1)
            }
            b'[' if line[i..].starts_with("[[") => {
                let end = wiki_link(line, &brackets, i, 2, LinkKind::Wiki, 0, &mut scratch);
                (end, end != i + 1)
            }
            b'!' if line[i..].starts_with("![") => {
                let end = markdown_link(line, &brackets, i, i + 1, 0, &mut scratch);
                (end, end != i + 2)
            }
            b'[' => {
                let end = markdown_link(line, &brackets, i, i, 0, &mut scratch);
                (end, end != i + 1)
            }
            _ => (i + 1, false),
        };
        if matched {
            out[i..end].fill(b' ');
        }
        i = end;
    }
    String::from_utf8(out).expect("only whole ASCII-delimited spans are blanked")
}

fn scan_links(line: &str, line_no: usize, out: &mut Vec<RawLink>) {
    if !line.contains('[') {
        return;
    }
    let brackets = Brackets::new(line);
    let br = &brackets;
    let b = line.as_bytes();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'`' => {
                let run = b[i..].iter().take_while(|&&c| c == b'`').count();
                i = skip_code_span(b, i, run);
            }
            b'!' if line[i..].starts_with("![[") => {
                i = wiki_link(line, br, i, 3, LinkKind::Embed, line_no, out)
            }
            b'[' if line[i..].starts_with("[[") => {
                i = wiki_link(line, br, i, 2, LinkKind::Wiki, line_no, out)
            }
            b'!' if line[i..].starts_with("![") => i = markdown_link(line, br, i, i + 1, line_no, out),
            b'[' => i = markdown_link(line, br, i, i, line_no, out),
            _ => i += 1,
        }
    }
}

/// Returns the index after the code span opened by `run` backticks at `start`,
/// or after the opening run when no closing run of the same length exists.
fn skip_code_span(b: &[u8], start: usize, run: usize) -> usize {
    let mut j = start + run;
    while j < b.len() {
        if b[j] == b'`' {
            let n = b[j..].iter().take_while(|&&c| c == b'`').count();
            if n == run {
                return j + n;
            }
            j += n;
        } else {
            j += 1;
        }
    }
    start + run
}

fn wiki_link(
    line: &str,
    br: &Brackets,
    start: usize,
    open: usize,
    kind: LinkKind,
    line_no: usize,
    out: &mut Vec<RawLink>,
) -> usize {
    let inner_start = start + open;
    if br.last_wiki_close.is_none_or(|c| c < inner_start) {
        return start + 1;
    }
    let Some(rel) = line[inner_start..].find("]]") else {
        return start + 1;
    };
    let end = inner_start + rel + 2;
    let inner = &line[inner_start..inner_start + rel];
    let inner = inner
        .split_once('|')
        .map_or(inner, |(t, _)| t.strip_suffix('\\').unwrap_or(t));
    let (target, heading) = match inner.split_once('#') {
        Some((t, h)) => (t.trim(), Some(h.trim())),
        None => (inner.trim(), None),
    };
    let heading = heading.filter(|h| !h.is_empty() && !h.starts_with('^'));
    if (target.is_empty() && heading.is_none()) || has_non_md_extension(target) {
        return end;
    }
    out.push(RawLink {
        kind,
        target: target.to_string(),
        heading: heading.map(str::to_string),
        raw: line[start..end].to_string(),
        line: line_no,
        property: None,
    });
    end
}

fn has_non_md_extension(target: &str) -> bool {
    let segment = target.rsplit('/').next().unwrap_or(target);
    let Some((_, ext)) = segment.rsplit_once('.') else {
        return false;
    };
    (1..=10).contains(&ext.len())
        && ext.bytes().all(|c| c.is_ascii_alphanumeric())
        && !ext.eq_ignore_ascii_case("md")
        && !ext.eq_ignore_ascii_case("markdown")
}

/// `start` is the first byte of the link (`!` or `[`), `label` the `[`.
fn markdown_link(
    line: &str,
    br: &Brackets,
    start: usize,
    label: usize,
    line_no: usize,
    out: &mut Vec<RawLink>,
) -> usize {
    let fail = label + 1;
    let b = line.as_bytes();
    // Label: matching `]`, nested brackets allowed.
    let Some(label_end) = br.close[label] else {
        return fail;
    };
    if b.get(label_end + 1) != Some(&b'(') {
        return fail;
    }
    let mut k = label_end + 2;
    while b.get(k) == Some(&b' ') {
        k += 1;
    }
    let dest;
    if b.get(k) == Some(&b'<') {
        let Some(rel) = line[k + 1..].find('>') else {
            return fail;
        };
        dest = &line[k + 1..k + 1 + rel];
        k += rel + 2;
    } else {
        let d_start = k;
        let mut parens = 0usize;
        while let Some(&c) = b.get(k) {
            match c {
                b' ' | b'\t' => break,
                b'(' => parens += 1,
                b')' if parens == 0 => break,
                b')' => parens -= 1,
                _ => {}
            }
            k += 1;
        }
        dest = &line[d_start..k];
    }
    while b.get(k) == Some(&b' ') {
        k += 1;
    }
    if let Some(&q) = b.get(k).filter(|&&c| c == b'"' || c == b'\'') {
        let Some(rel) = line[k + 1..].find(q as char) else {
            return fail;
        };
        k += rel + 2;
        while b.get(k) == Some(&b' ') {
            k += 1;
        }
    }
    if b.get(k) != Some(&b')') {
        return fail;
    }
    let end = k + 1;
    if !has_scheme(dest) {
        let (path, frag) = match dest.split_once('#') {
            Some((p, f)) => (p, Some(f)),
            None => (dest, None),
        };
        let path = percent_decode(path);
        if path.to_ascii_lowercase().ends_with(".md") {
            out.push(RawLink {
                kind: LinkKind::Markdown,
                target: path,
                heading: frag
                    .filter(|f| !f.is_empty() && !f.starts_with('^'))
                    .map(percent_decode),
                raw: line[start..end].to_string(),
                line: line_no,
                property: None,
            });
        }
    }
    end
}

fn has_scheme(dest: &str) -> bool {
    let mut chars = dest.char_indices();
    if !chars.next().is_some_and(|(_, c)| c.is_ascii_alphabetic()) {
        return false;
    }
    for (_, c) in chars {
        if c == ':' {
            return true;
        }
        if !(c.is_ascii_alphanumeric() || matches!(c, '+' | '.' | '-')) {
            return false;
        }
    }
    false
}

/// Decodes `%XX` escapes as UTF-8 bytes; a malformed escape is kept verbatim,
/// and a result that is not valid UTF-8 leaves the input unchanged.
fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let (Some(h), Some(l)) = (hex(b[i + 1]), hex(b[i + 2])) {
                out.push(h * 16 + l);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8(out).unwrap_or_else(|_| s.to_string())
}

fn hex(c: u8) -> Option<u8> {
    (c as char).to_digit(16).map(|d| d as u8)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn s(v: &[&str]) -> Vec<String> { v.iter().map(|x| x.to_string()).collect() }

    #[test]
    fn frontmatter_title_and_body_start() {
        let d = parse("---\ntitle: My Note\ntags: [rust, mcp]\ndate: 2026-09-30\nat: 2026-09-30T10:00:00\nn: 1.5\n---\n# H\n\nBody\n", "note");
        let p = d.properties.as_ref().unwrap();
        assert_eq!(p["title"], json!("My Note"));
        assert_eq!(p["tags"], json!(["rust", "mcp"]));
        assert_eq!(p["date"], json!("2026-09-30"));
        assert_eq!(p["at"], json!("2026-09-30T10:00:00"));
        assert_eq!(p["n"], json!(1.5));
        assert_eq!(d.title, "My Note");
        assert_eq!(d.body_start_line, 8);
        assert_eq!(d.body, "# H\n\nBody\n");
        assert!(d.frontmatter_error.is_none());
    }

    #[test]
    fn invalid_frontmatter_keeps_body() {
        let d = parse("---\na: b: c\n---\nText\n", "x");
        assert!(d.properties.is_none());
        assert!(d.frontmatter_error.is_some());
        assert_eq!(d.body, "Text\n");
        assert_eq!(d.body_start_line, 4);
        assert_eq!(d.title, "x");
    }

    #[test]
    fn non_mapping_frontmatter_is_invalid() {
        let d = parse("---\n- a\n---\nText\n", "x");
        assert!(d.properties.is_none());
        assert!(d.frontmatter_error.is_some());
    }

    #[test]
    fn unclosed_frontmatter_is_body() {
        let text = "---\na: 1\nText\n";
        let d = parse(text, "x");
        assert!(d.properties.is_none() && d.frontmatter_error.is_none());
        assert_eq!(d.body, text);
        assert_eq!(d.body_start_line, 1);
    }

    #[test]
    fn crlf_and_bom_are_normalized() {
        let d = parse("\u{feff}---\r\na: 1\r\n---\r\n# T\r\nx\r\n", "x");
        assert_eq!(d.properties.unwrap()["a"], json!(1));
        assert_eq!(d.body, "# T\nx\n");
        assert_eq!(d.headings[0].line, 4);
    }

    #[test]
    fn title_falls_back_to_h1_then_stem() {
        assert_eq!(parse("intro\n## Sub\n# First H1\n# Second\n", "stem").title, "First H1");
        assert_eq!(parse("no headings\n", "stem").title, "stem");
        assert_eq!(parse("---\ntitle: 3\n---\n# H1\n", "stem").title, "H1", "non-string title property is ignored");
    }

    #[test]
    fn headings_with_paths_and_lines() {
        let d = parse("# A\n## B\ntext\n### C\n## D\n# E\n", "x");
        let got: Vec<_> = d.headings.iter().map(|h| (h.level, h.text.clone(), h.heading_path.clone(), h.line)).collect();
        assert_eq!(got, vec![
            (1, "A".to_string(), s(&["A"]), 1),
            (2, "B".to_string(), s(&["A", "B"]), 2),
            (3, "C".to_string(), s(&["A", "B", "C"]), 4),
            (2, "D".to_string(), s(&["A", "D"]), 5),
            (1, "E".to_string(), s(&["E"]), 6),
        ]);
    }

    #[test]
    fn code_fences_and_closing_hashes() {
        let d = parse("```\n# not\n[[nolink]]\n```\n## Real ##\n#nospace\n~~~~\n# no\n~~~~\n", "x");
        assert_eq!(d.headings.len(), 1);
        assert_eq!((d.headings[0].text.as_str(), d.headings[0].line), ("Real", 5));
        assert!(d.links.is_empty());
    }

    #[test]
    fn links_all_forms() {
        let text = "---\na: 1\n---\nSee [[mcp_rust]], [[projects/mcp_rust#Concurrency|text]], ![[diagram]] and [x](../tools/sq%20lite.md#WAL%20mode).\nIgnore [y](https://e.com/a.md), [z](img.png), `[[code]]`, ![[pic.png]] but keep [[v1.2 notes]], [[#Local]] and [[n#A#B]].\n";
        let d = parse(text, "n");
        let got: Vec<_> = d.links.iter().map(|l| (l.kind, l.target.clone(), l.heading.clone(), l.raw.clone(), l.line)).collect();
        assert_eq!(got, vec![
            (LinkKind::Wiki, "mcp_rust".into(), None, "[[mcp_rust]]".into(), 4),
            (LinkKind::Wiki, "projects/mcp_rust".into(), Some("Concurrency".into()), "[[projects/mcp_rust#Concurrency|text]]".into(), 4),
            (LinkKind::Embed, "diagram".into(), None, "![[diagram]]".into(), 4),
            (LinkKind::Markdown, "../tools/sq lite.md".into(), Some("WAL mode".into()), "[x](../tools/sq%20lite.md#WAL%20mode)".into(), 4),
            (LinkKind::Wiki, "v1.2 notes".into(), None, "[[v1.2 notes]]".into(), 5),
            (LinkKind::Wiki, "".into(), Some("Local".into()), "[[#Local]]".into(), 5),
            (LinkKind::Wiki, "n".into(), Some("A#B".into()), "[[n#A#B]]".into(), 5),
        ]);
    }

    #[test]
    fn markdown_link_with_angle_brackets_and_title() {
        let d = parse("[a](<my note.md> \"t\")\n", "x");
        assert_eq!(d.links[0].target, "my note.md");
        assert_eq!(d.links[0].heading, None);
    }

    #[test]
    fn fence_closes_only_with_same_char_and_enough_length() {
        let d = parse("~~~~\n~~~\n```\n# a\n~~~~~\n# b\n", "x");
        let got: Vec<_> = d.headings.iter().map(|h| h.text.as_str()).collect();
        assert_eq!(got, vec!["b"], "~~~ and ``` must not close a ~~~~ fence; ~~~~~ must");
        let d = parse("```\n~~~~\n# a\n````\n# b\n", "x");
        assert_eq!(d.headings.iter().map(|h| h.text.as_str()).collect::<Vec<_>>(), vec!["b"], "~~~~ must not close a ``` fence");
    }

    #[test]
    fn tab_after_hashes_opens_a_heading() {
        let d = parse("#\tHeading\n##\tSub ##\n", "x");
        let got: Vec<_> = d.headings.iter().map(|h| (h.level, h.text.as_str())).collect();
        assert_eq!(got, vec![(1, "Heading"), (2, "Sub")]);
    }

    #[test]
    fn inline_triple_backticks_and_deep_indent_do_not_open_fences_or_headings() {
        let d = parse("```a``` text\n    # indented\n   # three\n", "x");
        let got: Vec<_> = d.headings.iter().map(|h| h.text.as_str()).collect();
        assert_eq!(got, vec!["three"]);
    }

    #[test]
    fn dotted_frontmatter_closer() {
        let d = parse("---\na: 1\n...\nText\n", "x");
        assert_eq!(d.properties.unwrap()["a"], json!(1));
        assert_eq!(d.body, "Text\n");
        assert_eq!(d.body_start_line, 4);
    }

    #[test]
    fn wikilink_extension_rule() {
        let d = parse("[[a/Note.MD]] [[x.markdown]] [[y.a b]] ![[doc.pdf]] [[z.PNG]] [[dir.v2/plain]]\n", "x");
        let got: Vec<_> = d.links.iter().map(|l| l.target.as_str()).collect();
        assert_eq!(got, vec!["a/Note.MD", "x.markdown", "y.a b", "dir.v2/plain"]);
    }

    #[test]
    fn empty_frontmatter_is_empty_mapping() {
        for text in ["---\n---\nBody\n", "---\n\n---\nBody\n", "---\n# c\n---\nBody\n"] {
            let d = parse(text, "x");
            assert_eq!(d.properties, Some(Map::new()), "{text:?}");
            assert!(d.frontmatter_error.is_none(), "{text:?}");
            assert_eq!(d.body, "Body\n");
        }
        let d = parse("---\n42\n---\nBody\n", "x");
        assert!(d.properties.is_none() && d.frontmatter_error.is_some(), "scalar is not a mapping");
    }

    #[test]
    fn table_escaped_alias_separator() {
        let d = parse("| [[note\\|alias]] | [[n#h\\|al]] |\n", "x");
        let got: Vec<_> = d.links.iter().map(|l| (l.target.clone(), l.heading.clone(), l.raw.clone())).collect();
        assert_eq!(got, vec![
            ("note".to_string(), None, "[[note\\|alias]]".to_string()),
            ("n".to_string(), Some("h".to_string()), "[[n#h\\|al]]".to_string()),
        ]);
    }

    #[test]
    fn block_references_are_not_headings() {
        let d = parse("[[note#^block1]] [a](other.md#^b2)\n", "x");
        let got: Vec<_> = d.links.iter().map(|l| (l.target.clone(), l.heading.clone())).collect();
        assert_eq!(got, vec![
            ("note".to_string(), None),
            ("other.md".to_string(), None),
        ]);
    }

    #[test]
    fn wikilink_long_extensions_are_ignored() {
        let d = parse("[[x.canvas]] ![[d.excalidraw]] [[k.md]] [[v1.2 notes]] [[e.abcdefghijk]]\n", "x");
        let got: Vec<_> = d.links.iter().map(|l| l.target.as_str()).collect();
        assert_eq!(got, vec!["k.md", "v1.2 notes", "e.abcdefghijk"]);
    }

    #[test]
    fn markdown_link_case_insensitive_md_and_percent_decoding() {
        let d = parse("[a](Caf%C3%A9.MD#%zz%4) [b](bad%FF.md)\n", "x");
        let got: Vec<_> = d.links.iter().map(|l| (l.target.clone(), l.heading.clone())).collect();
        assert_eq!(got, vec![
            ("Café.MD".to_string(), Some("%zz%4".to_string())),
            ("bad%FF.md".to_string(), None),
        ]);
    }

    #[test]
    fn a_line_of_open_brackets_parses_in_linear_time() {
        let line = "[".repeat(100_000);
        let start = std::time::Instant::now();
        let d = parse(&line, "x");
        let elapsed = start.elapsed();
        assert!(d.links.is_empty());
        assert!(elapsed < std::time::Duration::from_secs(1), "100,000 `[` took {elapsed:?}");
    }

    #[test]
    fn unbalanced_and_nested_brackets_keep_their_links() {
        let d = parse("[[a] [b](b.md) ]] [x [y](y.md) [[c]] ] [[d]] [z] [w]](w.md) [p `]` q](p.md) [r `x](r.md)` [[e]\n", "x");
        let got: Vec<_> = d.links.iter().map(|l| (l.kind, l.raw.as_str())).collect();
        assert_eq!(got, vec![
            (LinkKind::Wiki, "[[a] [b](b.md) ]]"),
            (LinkKind::Markdown, "[y](y.md)"),
            (LinkKind::Wiki, "[[c]]"),
            (LinkKind::Wiki, "[[d]]"),
            (LinkKind::Markdown, "[r `x](r.md)"),
        ]);
    }

    #[test]
    fn body_tags_follow_obsidian_rules() {
        let d = parse("#start and #nested/tag/ mid#no (#paren) #1984 #y1984 #café #🦀 `#code` [[n#heading]] https://e.com/#anchor\n# Heading #inheading\n```\n#fenced\n```\n#tail.\n", "x");
        assert_eq!(d.tags, vec!["start", "nested/tag", "y1984", "café", "🦀", "inheading", "tail"]);
    }

    #[test]
    fn frontmatter_tags_take_every_obsidian_shape() {
        let tags = |fm: &str| parse(&format!("---\n{fm}\n---\n"), "x").tags;
        assert_eq!(tags("tags: [rust, '#mcp', 2024, 'a b']"), vec!["rust", "mcp", "a", "b"]);
        assert_eq!(tags("tags: rust, mcp"), vec!["rust", "mcp"]);
        assert_eq!(tags("tags: '#rust #mcp'"), vec!["rust", "mcp"]);
        assert_eq!(tags("tags:\n  - rust\n  - nested/x"), vec!["rust", "nested/x"]);
        assert!(tags("tag: rust").is_empty(), "the singular key is not read");
        assert!(tags("tags:").is_empty());
    }

    #[test]
    fn frontmatter_tag_pieces_are_cut_like_body_tags() {
        let tags = |fm: &str| parse(&format!("---\n{fm}\n---\n"), "x").tags;
        assert_eq!(tags(r##"tags: rust, "#mcp""##), vec!["rust", "mcp"], "a plain YAML string keeps its quotes");
        assert_eq!(tags(r##"tags: ["'a'", 'b.c', 'd/', '""', 'café🦀']"##), vec!["a", "b", "d", "café🦀"]);
    }

    #[test]
    fn effective_tags_put_frontmatter_first_and_drop_case_duplicates() {
        let d = parse("---\ntags: [Rust, mcp]\n---\nUses #rust and #search, #MCP, #search.\n", "x");
        assert_eq!(d.tags, vec!["Rust", "mcp", "search"]);
    }

    #[test]
    fn frontmatter_links_are_read_from_quoted_values_with_their_key() {
        let d = parse("---\nup: \"[[parent#Part]]\"\nrelated:\n  - \"[[a]]\"\n  # \"[[commented]]\"\n  - '[b](../b.md)'\nbare: [[not-a-link]]\nn: 3\n---\nBody [[c]].\n", "x");
        let links: Vec<_> = d.links.iter().map(|l| (l.raw.as_str(), l.line, l.property.as_deref(), l.heading.as_deref())).collect();
        assert_eq!(links, vec![
            ("[[parent#Part]]", 2, Some("up"), Some("Part")),
            ("[[a]]", 4, Some("related"), None),
            ("[b](../b.md)", 6, Some("related"), None),
            ("[[c]]", 10, None, None),
        ], "an unquoted [[x]] is a nested YAML list, not a link; comment lines are skipped");
    }

    #[test]
    fn a_frontmatter_that_does_not_parse_as_a_mapping_has_no_links() {
        for text in ["---\n- \"[[a]]\"\n---\nx\n", "---\nup: \"[[a]]\"\nb: c: d\n---\nx\n"] {
            let d = parse(text, "x");
            assert!(d.frontmatter_error.is_some(), "{text:?}");
            assert!(d.links.is_empty(), "{text:?}");
        }
    }

    #[test]
    fn prose_blanks_links_and_code_spans_and_keeps_the_rest() {
        let words = |line: &str| {
            let p = prose(line);
            assert_eq!(p.len(), line.len(), "byte length kept: {line:?}");
            p.split_whitespace().map(str::to_string).collect::<Vec<_>>()
        };
        assert_eq!(words("See [[a|the MCP server]] and ![[b]] or [x](y.md) and `code` here."), vec!["See", "and", "or", "and", "here."]);
        assert_eq!(words("[x](https://e.com) keeps nothing of the link"), vec!["keeps", "nothing", "of", "the", "link"]);
        assert_eq!(prose("unclosed [[ and ` stay"), "unclosed [[ and ` stay");
        assert_eq!(words("café [[ñ]] ok"), vec!["café", "ok"], "UTF-8 stays valid");
    }
}
