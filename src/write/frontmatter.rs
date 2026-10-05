//! Line-preserving frontmatter updates (`update_properties`).

use serde_json::{Map, Value};

use crate::error::{Result, VltError};
use crate::indexer::parser::split_frontmatter;

const RESERVED: [&str; 10] = ["yes", "no", "on", "off", "y", "n", "true", "false", "null", "~"];

fn nothing_to_change() -> VltError {
    VltError::InvalidArguments("nothing to change".into())
}

fn unreadable() -> VltError {
    VltError::Internal("refusing to write: the updated frontmatter does not read back as intended".into())
}

/// Sets and removes top-level keys of the LF text's frontmatter, keeping every untouched line as is.
pub(crate) fn update_frontmatter(lf: &str, set: &Map<String, Value>, unset: &[String]) -> Result<String> {
    validate(set, unset)?;
    let (props, err, body, _) = split_frontmatter(lf);
    if let Some(m) = err {
        return Err(VltError::InvalidFrontmatter(m));
    }
    let Some(before) = props else {
        let entries: String = set.iter().map(|(k, v)| render(k, v, None)).collect();
        if entries.is_empty() {
            return Err(nothing_to_change());
        }
        return check(format!("---\n{entries}---\n{lf}"), lf, &Map::new(), set, unset);
    };

    let head = &lf[..lf.len() - body.len()];
    let lines: Vec<&str> = head.split_inclusive('\n').collect();
    let (opener, closer, yaml) = (lines[0], lines[lines.len() - 1], &lines[1..lines.len() - 1]);

    let mut out = String::from(opener);
    let mut found: Vec<String> = Vec::new();
    for piece in pieces(yaml) {
        match piece {
            Piece::Gap(line) => out.push_str(line),
            Piece::Entry { key, lines } => {
                let verbatim = || lines.concat();
                if unset.contains(&key) {
                } else if let Some(value) = set.get(&key) {
                    if before.get(&key) == Some(value) {
                        out.push_str(&verbatim());
                    } else {
                        out.push_str(&render(&key, value, Some(&lines)));
                    }
                } else {
                    out.push_str(&verbatim());
                }
                if found.contains(&key) && (set.contains_key(&key) || unset.contains(&key)) {
                    return Err(unreadable());
                }
                found.push(key);
            }
        }
    }
    for (k, v) in set {
        if !found.contains(k) {
            // The parser keeps the last of duplicate keys, so a key the line scan missed must not be appended.
            if before.contains_key(k) {
                return Err(unreadable());
            }
            out.push_str(&render(k, v, None));
        }
    }
    if unset.iter().any(|k| before.contains_key(k) && !found.contains(k)) {
        return Err(unreadable());
    }
    out.push_str(closer);
    out.push_str(&body);
    check(out, lf, &before, set, unset)
}

/// Refuses a no-op, then re-parses `out` and requires exactly the intended properties.
fn check(out: String, lf: &str, before: &Map<String, Value>, set: &Map<String, Value>, unset: &[String]) -> Result<String> {
    if out == lf {
        return Err(nothing_to_change());
    }
    let mut want = before.clone();
    for k in unset {
        want.remove(k);
    }
    for (k, v) in set {
        want.insert(k.clone(), v.clone());
    }
    match split_frontmatter(&out) {
        (Some(p), None, ..) if p == want => Ok(out),
        _ => Err(unreadable()),
    }
}

fn validate(set: &Map<String, Value>, unset: &[String]) -> Result<()> {
    if set.is_empty() && unset.is_empty() {
        return Err(nothing_to_change());
    }
    for k in set.keys().chain(unset) {
        if !valid_key(k) {
            return Err(VltError::InvalidArguments(format!("invalid property key {k:?}")));
        }
    }
    if let Some(k) = unset.iter().find(|k| set.contains_key(*k)) {
        return Err(VltError::InvalidArguments(format!("{k:?} is both in set and in unset")));
    }
    let is_scalar = |v: &Value| matches!(v, Value::Bool(_) | Value::Number(_) | Value::String(_));
    for (k, v) in set {
        let ok = match v {
            Value::Array(items) => items.iter().all(is_scalar),
            v => is_scalar(v),
        };
        if !ok {
            return Err(VltError::InvalidArguments(format!(
                "{k:?}: values are a string, number, bool or list of those (remove a key with unset)"
            )));
        }
    }
    Ok(())
}

fn valid_key(k: &str) -> bool {
    let mut chars = k.chars();
    let Some(first) = chars.next() else { return false };
    (first.is_alphanumeric() || first == '_')
        && chars.all(|c| c.is_alphanumeric() || "_-. ".contains(c))
        && !k.ends_with(' ')
}

enum Piece<'a> {
    Gap(&'a str),
    Entry { key: String, lines: Vec<&'a str> },
}

/// Groups YAML lines into top-level entries (key line plus continuation) and verbatim gap lines.
fn pieces<'a>(yaml: &[&'a str]) -> Vec<Piece<'a>> {
    let mut out: Vec<Piece<'a>> = Vec::new();
    let mut current: Option<(String, Vec<&'a str>)> = None;
    let flush = |current: &mut Option<(String, Vec<&'a str>)>, out: &mut Vec<Piece<'a>>| {
        if let Some((key, mut lines)) = current.take() {
            let mut trailing = Vec::new();
            while lines.last().is_some_and(|l| l.trim().is_empty()) {
                trailing.push(lines.pop().unwrap());
            }
            out.push(Piece::Entry { key, lines });
            out.extend(trailing.into_iter().rev().map(Piece::Gap));
        }
    };
    for (i, &line) in yaml.iter().enumerate() {
        if let Some((key, _)) = key_of(line) {
            flush(&mut current, &mut out);
            current = Some((key, vec![line]));
        } else if line.starts_with('#') {
            match current.as_mut() {
                Some((_, lines)) if entry_continues(&yaml[i + 1..]) => lines.push(line),
                _ => {
                    flush(&mut current, &mut out);
                    out.push(Piece::Gap(line));
                }
            }
        } else if let Some((_, lines)) = current.as_mut() {
            lines.push(line);
        } else {
            out.push(Piece::Gap(line));
        }
    }
    flush(&mut current, &mut out);
    out
}

/// True when the first line after a column-0 comment that is not blank or a comment still belongs to
/// the entry above (an indented line or a block-list item).
fn entry_continues(rest: &[&str]) -> bool {
    rest.iter()
        .find(|l| !l.trim().is_empty() && !l.starts_with('#'))
        .is_some_and(|l| l.starts_with([' ', '\t', '-']))
}

/// The top-level key a line starts, and the text after its colon.
fn key_of(line: &str) -> Option<(String, &str)> {
    let first = line.chars().next()?;
    if first.is_whitespace() || first == '#' || first == '-' {
        return None;
    }
    if first == '"' || first == '\'' {
        let close = line[1..].find(first)? + 1;
        let rest = line[close + 1..].strip_prefix(':')?;
        return Some((line[1..close].to_string(), rest));
    }
    let bytes = line.as_bytes();
    let colon = (0..bytes.len()).find(|&i| {
        bytes[i] == b':' && matches!(bytes.get(i + 1), None | Some(b' ' | b'\t' | b'\r' | b'\n'))
    })?;
    Some((line[..colon].trim_end().to_string(), &line[colon + 1..]))
}

/// Whole lines for `key: value`; a list keeps the flow or block style of the `old` entry.
fn render(key: &str, value: &Value, old: Option<&[&str]>) -> String {
    let key = yaml_key(key);
    let Value::Array(items) = value else {
        return format!("{key}: {}\n", scalar(value));
    };
    if items.is_empty() {
        return format!("{key}: []\n");
    }
    let flow = old
        .and_then(|l| key_of(l[0]))
        .is_some_and(|(_, rest)| rest.trim().starts_with('['));
    if flow {
        let items: Vec<String> = items.iter().map(scalar).collect();
        return format!("{key}: [{}]\n", items.join(", "));
    }
    let indent = match old.and_then(|l| l[1..].iter().find(|l| !l.trim().is_empty() && !l.starts_with('#'))) {
        Some(second) if second.starts_with('-') => "",
        Some(second) => &second[..second.len() - second.trim_start().len()],
        None => "  ",
    };
    let mut out = format!("{key}:\n");
    for item in items {
        out.push_str(&format!("{indent}- {}\n", scalar(item)));
    }
    out
}

fn scalar(v: &Value) -> String {
    let Value::String(s) = v else { return v.to_string() };
    if is_plain(s) {
        s.clone()
    } else {
        serde_json::to_string(s).expect("a string serializes")
    }
}

/// A key is written bare only if YAML reads it as a string, never as a date, bool, null or number.
fn yaml_key(k: &str) -> String {
    if is_plain(k) && !is_iso_datetime(k) {
        k.to_string()
    } else {
        serde_json::to_string(k).expect("a string serializes")
    }
}

/// Whether `s` written unquoted reads back as the same string (an ISO date or date-time counts as one).
fn is_plain(s: &str) -> bool {
    let iso = is_iso_datetime(s);
    s.chars().all(|c| c.is_alphanumeric() || "_-. /".contains(c) || (iso && c == ':'))
        && s.chars().next().is_some_and(char::is_alphanumeric)
        && !s.ends_with(' ')
        && !RESERVED.contains(&s.to_lowercase().as_str())
        && (iso || !looks_numeric(s))
        && serde_yaml_ng::from_str::<Value>(s).ok() == Some(Value::String(s.to_string()))
}

/// What a YAML 1.1 or 1.2 reader may take for a number: `007`, `1_000`, `0x1F`, `1e3`, `+1`, `.5`.
fn looks_numeric(s: &str) -> bool {
    s.starts_with(|c: char| c.is_ascii_digit() || "+-.".contains(c))
        && s.chars().all(|c| c.is_ascii_hexdigit() || "_.+-xXoObB".contains(c))
}

/// `YYYY-MM-DD`, `YYYY-MM-DDTHH:MM` or `YYYY-MM-DDTHH:MM:SS`.
fn is_iso_datetime(s: &str) -> bool {
    let shape = |p: &str| {
        p.len() == s.len() && p.bytes().zip(s.bytes()).all(|(p, c)| if p == b'9' { c.is_ascii_digit() } else { p == c })
    };
    ["9999-99-99", "9999-99-99T99:99", "9999-99-99T99:99:99"].into_iter().any(shape)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn upd(text: &str, set: Value, unset: &[&str]) -> Result<String> {
        let unset: Vec<String> = unset.iter().map(|s| s.to_string()).collect();
        update_frontmatter(text, set.as_object().unwrap(), &unset)
    }

    #[test]
    fn a_changed_key_keeps_its_place_and_every_other_line() {
        let t = "---\n# who\nstatus: idea\ntags: [a]\ndate: 2026-01-01\n---\nBody\n";
        assert_eq!(upd(t, json!({"status": "done"}), &[]).unwrap(),
                   "---\n# who\nstatus: done\ntags: [a]\ndate: 2026-01-01\n---\nBody\n");
    }

    #[test]
    fn an_unchanged_value_keeps_its_original_line_even_with_a_comment() {
        let t = "---\nstatus: idea # keep\n---\nB\n";
        assert_eq!(upd(t, json!({"status": "idea", "owner": "ana"}), &[]).unwrap(),
                   "---\nstatus: idea # keep\nowner: ana\n---\nB\n");
    }

    #[test]
    fn new_keys_go_to_the_end_of_the_frontmatter() {
        let t = "---\na: 1\n---\nB\n";
        assert_eq!(upd(t, json!({"owner": "ana"}), &[]).unwrap(), "---\na: 1\nowner: ana\n---\nB\n");
    }

    #[test]
    fn unset_removes_the_key_with_its_block_list_but_not_the_next_comment() {
        let t = "---\ntags:\n  - a\n  - b\n# about status\nstatus: x\n---\nB\n";
        assert_eq!(upd(t, json!({}), &["tags"]).unwrap(), "---\n# about status\nstatus: x\n---\nB\n");
    }

    #[test]
    fn a_comment_inside_a_block_list_belongs_to_the_entry() {
        let t = "---\nt:\n  - a\n# c\n  - b\nz: 1\n---\nB\n";
        assert_eq!(upd(t, json!({}), &["t"]).unwrap(), "---\nz: 1\n---\nB\n");
        assert_eq!(upd(t, json!({"t": ["x"]}), &[]).unwrap(), "---\nt:\n  - x\nz: 1\n---\nB\n");
    }

    #[test]
    fn the_list_indent_ignores_a_comment_right_after_the_key() {
        let t = "---\nt:\n# c\n  - a\nz: 1\n---\nB\n";
        assert_eq!(upd(t, json!({"t": ["x"]}), &[]).unwrap(), "---\nt:\n  - x\nz: 1\n---\nB\n");
    }

    #[test]
    fn lists_keep_the_style_the_key_had_and_new_ones_are_blocks() {
        assert_eq!(upd("---\ntags: [a, b]\n---\n", json!({"tags": ["x", "y z"]}), &[]).unwrap(),
                   "---\ntags: [x, y z]\n---\n");
        assert_eq!(upd("---\ntags:\n  - a\n---\n", json!({"tags": ["x", "y"]}), &[]).unwrap(),
                   "---\ntags:\n  - x\n  - \"y\"\n---\n");
        assert_eq!(upd("---\na: 1\n---\n", json!({"aliases": ["q"]}), &[]).unwrap(),
                   "---\na: 1\naliases:\n  - q\n---\n");
        assert_eq!(upd("---\na: 1\n---\n", json!({"aliases": []}), &[]).unwrap(), "---\na: 1\naliases: []\n---\n");
    }

    #[test]
    fn scalars_are_serialized_so_they_read_back_with_their_type() {
        let cases = [
            (json!("2026-09-30"), "k: 2026-09-30"),
            (json!("2026-09-30T10:00:00"), "k: 2026-09-30T10:00:00"),
            (json!("yes"), "k: \"yes\""),
            (json!("true"), "k: \"true\""),
            (json!("null"), "k: \"null\""),
            (json!("~"), "k: \"~\""),
            (json!("1.0"), "k: \"1.0\""),
            (json!("a: b"), "k: \"a: b\""),
            (json!("two\nlines"), "k: \"two\\nlines\""),
            (json!("plain text"), "k: plain text"),
            (json!("007"), "k: \"007\""),
            (json!("1_000"), "k: \"1_000\""),
            (json!("+1"), "k: \"+1\""),
            (json!("-5"), "k: \"-5\""),
            (json!(".5"), "k: \".5\""),
            (json!("1st"), "k: 1st"),
            (json!(3), "k: 3"),
            (json!(true), "k: true"),
        ];
        for (value, line) in cases {
            let out = upd("---\nz: 1\n---\n", json!({"k": value}), &[]).unwrap();
            assert_eq!(out, format!("---\nz: 1\n{line}\n---\n"), "{value}");
        }
    }

    #[test]
    fn a_document_without_frontmatter_gets_one() {
        assert_eq!(upd("# Title\n", json!({"status": "idea"}), &[]).unwrap(), "---\nstatus: idea\n---\n# Title\n");
        assert_eq!(upd("---\n---\nB\n", json!({"a": 1}), &[]).unwrap(), "---\na: 1\n---\nB\n");
        assert_eq!(upd("# Title\n", json!({}), &["a"]).unwrap_err().code(), "invalid_arguments", "no empty frontmatter is added");
    }

    #[test]
    fn invalid_frontmatter_is_refused() {
        let err = upd("---\na: [\n---\nB\n", json!({"b": 1}), &[]).unwrap_err();
        assert_eq!(err.code(), "invalid_frontmatter");
    }

    #[test]
    fn bad_arguments_are_invalid_arguments() {
        let t = "---\na: 1\n---\n";
        let bad = [
            upd(t, json!({}), &[]),
            upd(t, json!({"a": null}), &[]),
            upd(t, json!({"a": {"x": 1}}), &[]),
            upd(t, json!({"a": [["x"]]}), &[]),
            upd(t, json!({"a": 2}), &["a"]),
            upd(t, json!({"a: b": 2}), &[]),
            upd(t, json!({"": 2}), &[]),
            upd(t, json!({}), &["missing"]),
            upd(t, json!({"a": 1}), &[]),
        ];
        for r in bad {
            assert_eq!(r.unwrap_err().code(), "invalid_arguments");
        }
    }

    // Inputs beyond the plan's list: each either works or is refused; never a corrupt frontmatter.

    #[test]
    fn a_flow_list_spanning_lines_is_replaced_whole() {
        let t = "---\ntags: [a,\n  b]\nz: 1\n---\n";
        assert_eq!(upd(t, json!({"tags": ["c"]}), &[]).unwrap(), "---\ntags: [c]\nz: 1\n---\n");
        assert_eq!(upd(t, json!({}), &["tags"]).unwrap(), "---\nz: 1\n---\n");
    }

    #[test]
    fn a_quoted_key_is_found() {
        let t = "---\n\"my key\": 1\nz: 1\n---\n";
        assert_eq!(upd(t, json!({"my key": 2}), &[]).unwrap(), "---\nmy key: 2\nz: 1\n---\n");
        assert_eq!(upd(t, json!({}), &["my key"]).unwrap(), "---\nz: 1\n---\n");
    }

    #[test]
    fn a_block_value_with_a_blank_line_inside_is_one_entry() {
        let t = "---\ndesc: |\n  one\n\n  two\n\nz: 1\n---\n";
        assert_eq!(upd(t, json!({"desc": "x"}), &[]).unwrap(), "---\ndesc: x\n\nz: 1\n---\n");
        assert_eq!(upd(t, json!({"z": 2}), &[]).unwrap(), "---\ndesc: |\n  one\n\n  two\n\nz: 2\n---\n");
    }

    #[test]
    fn a_dots_closer_and_a_closer_at_eof_are_kept() {
        assert_eq!(upd("---\na: 1\n...\nB\n", json!({"b": 2}), &[]).unwrap(), "---\na: 1\nb: 2\n...\nB\n");
        assert_eq!(upd("---\na: 1\n---", json!({"a": 2}), &[]).unwrap(), "---\na: 2\n---");
    }

    #[test]
    fn a_block_list_keeps_a_wider_indent() {
        assert_eq!(upd("---\ntags:\n    - a\n---\n", json!({"tags": ["b"]}), &[]).unwrap(), "---\ntags:\n    - b\n---\n");
    }

    #[test]
    fn a_block_list_at_the_key_column_keeps_its_indent() {
        let t = "---\ntags:\n- a\nz: 1\n---\n";
        assert_eq!(upd(t, json!({"tags": ["b", "c"]}), &[]).unwrap(), "---\ntags:\n- b\n- c\nz: 1\n---\n");
    }

    #[test]
    fn list_items_are_quoted_like_scalars() {
        assert_eq!(upd("---\nt: [a]\n---\n", json!({"t": ["a, b", "yes", 1]}), &[]).unwrap(),
                   "---\nt: [\"a, b\", \"yes\", 1]\n---\n");
        assert_eq!(upd("---\nz: 1\n---\n", json!({"t": ["no", "- x", "-x"]}), &[]).unwrap(),
                   "---\nz: 1\nt:\n  - \"no\"\n  - \"- x\"\n  - \"-x\"\n---\n");
    }

    #[test]
    fn a_key_the_line_scan_cannot_see_is_refused_not_duplicated() {
        // `? a` is an explicit YAML key that the line scan does not recognize: setting `a` would append a duplicate.
        let t = "---\n? a\n: 1\n---\nB\n";
        assert_eq!(upd(t, json!({"a": 2}), &[]).unwrap_err().code(), "internal");
        assert_eq!(upd(t, json!({}), &["a"]).unwrap_err().code(), "internal");
    }

    #[test]
    fn a_key_written_twice_is_not_touched() {
        let t = "---\na: 1\na: 2\nz: 1\n---\n";
        assert_eq!(upd(t, json!({"a": 3}), &[]).unwrap_err().code(), "internal");
        assert_eq!(upd(t, json!({}), &["a"]).unwrap_err().code(), "internal");
        assert_eq!(upd(t, json!({"z": 2}), &[]).unwrap(), "---\na: 1\na: 2\nz: 2\n---\n");
    }

    #[test]
    fn a_line_the_scan_misreads_is_refused_by_the_read_back() {
        // The escaped quote hides the key from the scan, so its line rides along with `z` and would be lost.
        let t = "---\nz: 1\n\"a\\\"b\": 2\n---\n";
        assert_eq!(upd(t, json!({"z": 5}), &[]).unwrap_err().code(), "internal");
    }

    #[test]
    fn a_key_yaml_would_read_as_another_type_is_written_quoted() {
        assert_eq!(upd("---\nz: 1\n---\n", json!({"true": "v"}), &[]).unwrap(), "---\nz: 1\n\"true\": v\n---\n");
        assert_eq!(upd("---\nz: 1\n---\n", json!({"1e3": "v"}), &[]).unwrap(), "---\nz: 1\n\"1e3\": v\n---\n");
        assert_eq!(upd("---\n\"yes\": 1\nz: 1\n---\n", json!({"yes": 2}), &[]).unwrap(),
                   "---\n\"yes\": 2\nz: 1\n---\n");
        assert_eq!(upd("---\nz: 1\n---\n", json!({"plain": "v"}), &[]).unwrap(), "---\nz: 1\nplain: v\n---\n");
    }

    #[test]
    fn a_key_with_a_trailing_space_is_invalid() {
        assert_eq!(upd("---\na: 1\n---\n", json!({"b ": 2}), &[]).unwrap_err().code(), "invalid_arguments");
    }
}
