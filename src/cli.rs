use std::io::Write;
use std::time::{Duration, Instant};

use clap::{Parser, Subcommand};
use serde_json::Value;

use crate::app::App;
use crate::audit::{AuditResponse, Section};
use crate::documents::{DocumentResponse, FoldersResponse, PropertiesResponse};
use crate::indexer::Progress;
use crate::installer::models::{missing, pull_showing_progress};
use crate::installer::{InstallOptions, Target};
use crate::ml::models::Models;
use crate::paths;
use crate::related::RelatedResponse;
use crate::search::filter::parse_cli_where;
use crate::search::{SearchRequest, SearchResponse};
use crate::{Result, VltError};

#[derive(Parser)]
#[command(name = "vlt", version, about = "Semantic search over an Obsidian vault")]
struct Cli {
    #[command(subcommand)]
    cmd: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Set up vlt: vault, models, MCP server and Skill for each agent, first index
    Install {
        /// Path to the Obsidian vault (created after confirmation if missing)
        #[arg(long)]
        vault: Option<std::path::PathBuf>,
        /// Agent to configure (repeatable)
        #[arg(long = "target", value_enum)]
        targets: Vec<Target>,
        /// Configure every supported agent
        #[arg(long)]
        all: bool,
        /// Do not ask; answer yes to every confirmation
        #[arg(long)]
        yes: bool,
    },
    /// Remove the MCP server and Skill from every agent
    Uninstall {
        /// Do not ask; keeps ~/.vlt/
        #[arg(long)]
        yes: bool,
    },
    /// Hybrid search over the vault
    Search {
        /// What to look for: the key terms of the topic, in English
        query: String,
        /// Property filter `<key><op><value>`, op one of =, >, >=, <, <= (repeatable)
        #[arg(long = "where")]
        where_: Vec<String>,
        /// Only documents whose vault-relative path starts with this prefix (e.g. `projects/`)
        #[arg(long)]
        path_prefix: Option<String>,
        /// Maximum number of results
        #[arg(long)]
        limit: Option<usize>,
        /// Print the same JSON the MCP tool returns
        #[arg(long)]
        json: bool,
    },
    /// Show a document with its properties, content, links and backlinks
    Get {
        /// Vault-relative path of the document (e.g. `projects/mcp_rust.md`)
        path: String,
        /// Print the same JSON the MCP tool returns (adds sections, links and backlinks)
        #[arg(long)]
        json: bool,
    },
    /// List frontmatter properties with their types and top values
    Properties {
        /// Only documents matching `<key><op><value>`, op one of =, >, >=, <, <= (repeatable)
        #[arg(long = "where")]
        where_: Vec<String>,
        /// Only documents whose vault-relative path starts with this prefix
        #[arg(long)]
        path_prefix: Option<String>,
        /// Only this property, with every value instead of the top ones
        #[arg(long)]
        key: Option<String>,
        /// Print the same JSON the MCP tool returns
        #[arg(long)]
        json: bool,
    },
    /// List the vault's folders with their notes and most used keys
    Folders {
        /// Only folders whose vault-relative path starts with this prefix
        #[arg(long)]
        path_prefix: Option<String>,
        /// Print the same JSON the MCP tool returns
        #[arg(long)]
        json: bool,
    },
    /// Report spelling variants, rare keys, folder gaps, broken links and unlinked notes
    Audit {
        /// Only notes whose vault-relative path starts with this prefix
        #[arg(long)]
        path_prefix: Option<String>,
        /// Print the same JSON the MCP tool returns
        #[arg(long)]
        json: bool,
    },
    /// Show the notes related to a note, the tags they share and the sections that mention it unlinked
    Related {
        /// Vault-relative path of the note
        path: String,
        /// Maximum number of related notes (default 10)
        #[arg(long)]
        limit: Option<usize>,
        /// Print the same JSON the MCP tool returns
        #[arg(long)]
        json: bool,
    },
    /// Incremental reindex (or a full rebuild with --force); downloads missing models first
    Reindex {
        /// Rebuild the whole index instead of updating it
        #[arg(long)]
        force: bool,
    },
    /// Run the MCP server on stdio
    Mcp,
}

/// Parses args, dispatches, prints `error: <msg>` on stderr and returns 1 on error.
pub fn run() -> i32 {
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(e) => {
            let _ = e.print();
            return e.exit_code();
        }
    };
    match dispatch(cli.cmd) {
        Ok(()) => 0,
        Err(e) if is_stdout_closed(&e) => 0,
        Err(e) => {
            eprintln!("error: {e}");
            1
        }
    }
}

/// Marks a write to stdout that failed because the reader closed the pipe.
#[derive(Debug)]
struct StdoutClosed;

impl std::fmt::Display for StdoutClosed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("stdout was closed")
    }
}

impl std::error::Error for StdoutClosed {}

/// True for the error `write_out` returns when the reader closed stdout.
pub(crate) fn is_stdout_closed(e: &VltError) -> bool {
    matches!(e, VltError::Io(e) if e.get_ref().is_some_and(|r| r.is::<StdoutClosed>()))
}

/// Writes to stdout; a closed pipe becomes a `StdoutClosed` error so `run` exits 0 quietly.
pub(crate) fn write_out(args: std::fmt::Arguments) -> Result<()> {
    let mut stdout = std::io::stdout().lock();
    match stdout.write_fmt(args).and_then(|()| stdout.flush()) {
        Err(e) if e.kind() == std::io::ErrorKind::BrokenPipe => {
            Err(std::io::Error::new(std::io::ErrorKind::BrokenPipe, StdoutClosed).into())
        }
        r => Ok(r?),
    }
}

fn dispatch(cmd: Command) -> Result<()> {
    match cmd {
        Command::Install { vault, targets, all, yes } => {
            crate::installer::install(InstallOptions { vault, targets, all, yes })
        }
        Command::Uninstall { yes } => crate::installer::uninstall(yes),
        Command::Search { query, where_, path_prefix, limit, json } => {
            let where_ = if where_.is_empty() { None } else { Some(parse_cli_where(&where_)?) };
            let app = App::load()?;
            let req = SearchRequest { query, where_, path_prefix, limit, min_score: None };
            let resp = app.search(&req)?;
            if json {
                print_json(&resp)
            } else {
                write_out(format_args!("{}", render_search(&app, &resp, app.cfg.search.min_score)))
            }
        }
        Command::Get { path, json } => {
            let app = App::load()?;
            let doc = app.get_document(&path)?;
            if json {
                print_json(&doc)
            } else {
                write_out(format_args!("{}", render_document(&app, &doc)))
            }
        }
        Command::Properties { where_, path_prefix, key, json } => {
            let where_ = if where_.is_empty() { None } else { Some(parse_cli_where(&where_)?) };
            let app = App::load()?;
            let resp = app.list_properties(where_.as_ref(), path_prefix.as_deref(), key.as_deref())?;
            if json {
                print_json(&resp)
            } else {
                write_out(format_args!("{}", render_properties(&resp)))
            }
        }
        Command::Folders { path_prefix, json } => {
            let app = App::load()?;
            let resp = app.list_folders(path_prefix.as_deref())?;
            if json {
                print_json(&resp)
            } else {
                write_out(format_args!("{}", render_folders(&resp)))
            }
        }
        Command::Audit { path_prefix, json } => {
            let app = App::load()?;
            let resp = app.audit(path_prefix.as_deref())?;
            if json {
                print_json(&resp)
            } else {
                write_out(format_args!("{}", render_audit(&resp)))
            }
        }
        Command::Related { path, limit, json } => {
            let app = App::load()?;
            let resp = app.related(&path, limit)?;
            if json {
                print_json(&resp)
            } else {
                write_out(format_args!("{}", render_related(&resp)))
            }
        }
        Command::Reindex { force } => {
            let app = App::load()?;
            pull_missing_models(&app)?;
            let mut progress = IndexProgress::new();
            let out = app.reindex(force, &mut |p| progress.show(p))?;
            progress.finish();
            if out.reindexed {
                let s = &out.stats;
                write_out(format_args!("added {}, updated {}, removed {}, unchanged {}\n", s.added, s.updated, s.removed, s.unchanged))?;
            } else {
                write_out(format_args!("Another process is reindexing; skipped.\n"))?;
            }
            Ok(())
        }
        Command::Mcp => crate::mcp::serve(),
    }
}

fn print_json<T: serde::Serialize>(payload: &T) -> Result<()> {
    let text = serde_json::to_string_pretty(payload).map_err(|e| VltError::Internal(e.to_string()))?;
    write_out(format_args!("{text}\n"))
}

fn indent(text: &str) -> String {
    text.lines()
        .map(|l| if l.is_empty() { "\n".to_string() } else { format!("    {l}\n") })
        .collect()
}

fn render_search(app: &App, resp: &SearchResponse, min_score: f64) -> String {
    if resp.results.is_empty() {
        return format!("No results above min_score {min_score}.\n");
    }
    let blocks: Vec<String> = resp
        .results
        .iter()
        .map(|r| {
            format!(
                "{:.3}  {}\n     {}  lines {}-{}\n     {}\n\n{}",
                r.score,
                r.chunk_id,
                app.cfg.vault.join(&r.document.path).display(),
                r.lines[0],
                r.lines[1],
                r.heading_path.join(" > "),
                indent(&r.content),
            )
        })
        .collect();
    blocks.join("\n")
}

fn render_document(app: &App, doc: &DocumentResponse) -> String {
    let mut out = format!("{}\n{}\n", doc.title, app.cfg.vault.join(&doc.path).display());
    for (key, value) in &doc.properties {
        out.push_str(&format!("{key}: {value}\n"));
    }
    out.push('\n');
    out.push_str(&doc.content);
    if !doc.content.ends_with('\n') {
        out.push('\n');
    }
    out
}

fn render_properties(resp: &PropertiesResponse) -> String {
    let header = ["KEY".to_string(), "TYPE".to_string(), "DOCS".to_string(), "TOP VALUES".to_string()];
    let mut rows = vec![header];
    let item = |(v, n): &(Value, usize)| match v {
        Value::String(s) => format!("{s} ({n})"),
        other => format!("{other} ({n})"),
    };
    for p in &resp.properties {
        let top: Vec<String> = p.top_values.iter().map(item).collect();
        rows.push([p.key.clone(), p.kind.clone(), p.documents.to_string(), top.join(", ")]);
    }
    let width = |i: usize| rows.iter().map(|r| r[i].chars().count()).max().unwrap_or(0);
    let widths = [width(0), width(1), width(2)];
    let mut out = String::new();
    for (i, r) in rows.iter().enumerate() {
        out.push_str(&format!("{:w0$}  {:w1$}  {:w2$}  {}\n", r[0], r[1], r[2], r[3], w0 = widths[0], w1 = widths[1], w2 = widths[2]));
        // `rows[0]` is the header; `resp.properties[i - 1]` owns row `i`.
        if let Some(p) = i.checked_sub(1).map(|j| &resp.properties[j]).filter(|p| !p.variants.is_empty()) {
            let groups: Vec<String> = p.variants.iter().map(|g| g.iter().map(item).collect::<Vec<_>>().join(" / ")).collect();
            out.push_str(&format!("  variants: {}\n", groups.join("; ")));
        }
    }
    out
}

fn render_folders(resp: &FoldersResponse) -> String {
    let mut rows = vec![["FOLDER".to_string(), "DOCS".to_string(), "TOTAL".to_string(), "TOP KEYS".to_string()]];
    for f in &resp.folders {
        let keys: Vec<String> = f.top_keys.iter().map(|(k, n)| format!("{k} ({n})")).collect();
        let path = if f.path.is_empty() { "/".to_string() } else { f.path.clone() };
        rows.push([path, f.documents.to_string(), f.total.to_string(), keys.join(", ")]);
    }
    let width = |i: usize| rows.iter().map(|r| r[i].chars().count()).max().unwrap_or(0);
    let widths = [width(0), width(1), width(2)];
    let mut out = String::new();
    for r in &rows {
        out.push_str(&format!("{:w0$}  {:w1$}  {:w2$}  {}\n", r[0], r[1], r[2], r[3], w0 = widths[0], w1 = widths[1], w2 = widths[2]));
    }
    out
}

fn render_audit(resp: &AuditResponse) -> String {
    let value = |(v, n): &(Value, usize)| match v {
        Value::String(s) => format!("{s} ({n})"),
        other => format!("{other} ({n})"),
    };
    let mut out = String::new();
    fn section<T>(out: &mut String, name: &str, s: &Section<T>, line: impl Fn(&T) -> String) {
        if s.total == 0 {
            return;
        }
        if s.items.len() < s.total {
            out.push_str(&format!("{name} ({} of {})\n", s.items.len(), s.total));
        } else {
            out.push_str(&format!("{name} ({})\n", s.total));
        }
        for item in &s.items {
            out.push_str(&format!("  {}\n", line(item)));
        }
    }
    section(&mut out, "value variants", &resp.value_variants, |v| {
        let spellings: Vec<String> = v.spellings.iter().map(value).collect();
        format!("{}: {}", v.key, spellings.join(" / "))
    });
    section(&mut out, "rare keys", &resp.rare_keys, |r| format!("{}  {}", r.key, r.path));
    section(&mut out, "key variants", &resp.key_variants, |k| {
        let keys: Vec<String> = k.keys.iter().map(|(key, n)| format!("{key} ({n})")).collect();
        keys.join(" / ")
    });
    section(&mut out, "folder gaps", &resp.folder_gaps, |g| {
        let folder = if g.folder.is_empty() { "/" } else { &g.folder };
        format!("{folder}  {} {}  {}/{}  missing: {}", g.kind, g.name, g.present, g.notes, g.missing.join(", "))
    });
    section(&mut out, "broken links", &resp.broken_links, |l| format!("{}:{}  {}  {}", l.path, l.line, l.raw, l.status));
    section(&mut out, "orphans", &resp.orphans, |n| n.path.clone());
    section(&mut out, "no outgoing links", &resp.no_outgoing, |n| n.path.clone());
    if out.is_empty() {
        out.push_str("nothing to report\n");
    }
    out
}

fn render_related(resp: &RelatedResponse) -> String {
    let heading = |id: &str, path: &str| id.strip_prefix(path).and_then(|r| r.strip_prefix('#')).unwrap_or(id).to_string();
    let mut rows = vec![["NOTE".to_string(), "SCORE".to_string(), "LINKED".to_string(), "SECTIONS".to_string()]];
    for n in &resp.notes {
        rows.push([n.path.clone(), format!("{:.2}", n.score), n.linked.to_string(), format!("{} → {}", heading(&n.from, &resp.path), heading(&n.to, &n.path))]);
    }
    let width = |i: usize| rows.iter().map(|r| r[i].chars().count()).max().unwrap_or(0);
    let widths = [width(0), width(1), width(2)];
    let mut out = String::new();
    for r in &rows {
        out.push_str(&format!("{:w0$}  {:w1$}  {:w2$}  {}\n", r[0], r[1], r[2], r[3], w0 = widths[0], w1 = widths[1], w2 = widths[2]));
    }
    if !resp.suggested_tags.is_empty() {
        let tags: Vec<String> = resp.suggested_tags.iter().map(|(t, n)| format!("{t} ({n})")).collect();
        out.push_str(&format!("suggested tags: {}\n", tags.join(", ")));
    }
    if !resp.mentions.is_empty() {
        out.push_str("mentions:\n");
        for m in &resp.mentions {
            out.push_str(&format!("  {}:{}  {}\n", m.path, m.line, m.name));
        }
    }
    out
}

fn notes(n: usize) -> &'static str {
    if n == 1 {
        "note"
    } else {
        "notes"
    }
}

fn size_text(bytes: u64) -> String {
    let kb = bytes as f64 / 1024.0;
    if kb < 1024.0 {
        format!("{:.0} KB", kb.max(1.0))
    } else {
        format!("{:.1} MB", kb / 1024.0)
    }
}

/// `done` of `total`, both in the unit `size_text` picks for `total`.
fn sizes_text(done: u64, total: u64) -> String {
    let kb = |b: u64| if b == 0 { 0.0 } else { (b as f64 / 1024.0).max(1.0) };
    if kb(total) < 1024.0 {
        format!("{:.0}/{:.0} KB", kb(done), kb(total))
    } else {
        format!("{:.1}/{:.1} MB", done as f64 / 1_048_576.0, total as f64 / 1_048_576.0)
    }
}

fn time_left(secs: u64) -> String {
    if secs < 60 {
        format!("~{secs} s left")
    } else {
        format!("~{} min left", secs.div_ceil(60))
    }
}

/// The stderr text of a reindex `Progress`; `elapsed` is the time since `Progress::Started`.
fn progress_text(p: Progress, elapsed: Duration) -> String {
    match p {
        Progress::Started { files, bytes } => format!(
            "Indexing {files} {} ({}) on the CPU; a large vault can take several minutes\n",
            notes(files),
            size_text(bytes)
        ),
        Progress::LoadingModel => "Loading the embedding model…\n".into(),
        Progress::Indexed { files_done, files_total, bytes_done, bytes_total } => {
            let pct = (bytes_done * 100).checked_div(bytes_total).unwrap_or(100);
            let mut line = format!(
                "\r\x1b[KIndexing {files_done}/{files_total} {} · {} ({pct}%)",
                notes(files_total),
                sizes_text(bytes_done, bytes_total)
            );
            if bytes_done > 0 && bytes_done < bytes_total {
                let left = elapsed.as_secs_f64() * (bytes_total - bytes_done) as f64 / bytes_done as f64;
                line.push_str(&format!(" · {}", time_left(left.round() as u64)));
            }
            line
        }
    }
}

/// Shows reindex progress on stderr; `finish` ends the line rewritten in place.
pub(crate) struct IndexProgress {
    started: Instant,
    line_open: bool,
}

impl IndexProgress {
    pub(crate) fn new() -> Self {
        IndexProgress { started: Instant::now(), line_open: false }
    }

    pub(crate) fn show(&mut self, p: Progress) {
        if let Progress::Started { .. } = p {
            self.started = Instant::now();
        }
        self.line_open = matches!(p, Progress::Indexed { .. });
        eprint!("{}", progress_text(p, self.started.elapsed()));
        let _ = std::io::stderr().flush();
    }

    pub(crate) fn finish(&self) {
        if self.line_open {
            eprintln!();
        }
    }
}

/// Downloads the configured models that are not on disk yet; progress goes to stderr.
fn pull_missing_models(app: &App) -> Result<()> {
    if Models::fake_from_env() {
        return Ok(());
    }
    let dir = paths::models_dir(&app.home);
    for model in app.models.refs() {
        if missing(&dir, &model.id, &model.revision) {
            pull_showing_progress(&dir, &model.id, &model.revision)?;
        }
    }
    app.models.check()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn indexed(files_done: usize, bytes_done: u64) -> Progress {
        Progress::Indexed { files_done, files_total: 58, bytes_done, bytes_total: 4_000_000 }
    }

    #[test]
    fn the_start_and_the_model_load_each_get_their_own_line() {
        let t = progress_text(Progress::Started { files: 58, bytes: 953_000 }, Duration::ZERO);
        assert_eq!(t, "Indexing 58 notes (931 KB) on the CPU; a large vault can take several minutes\n");
        let t = progress_text(Progress::Started { files: 1, bytes: 3_000_000 }, Duration::ZERO);
        assert_eq!(t, "Indexing 1 note (2.9 MB) on the CPU; a large vault can take several minutes\n");
        assert_eq!(progress_text(Progress::LoadingModel, Duration::ZERO), "Loading the embedding model…\n");
    }

    #[test]
    fn the_indexed_line_clears_itself_and_shows_the_bytes_embedded_their_share_and_the_time_left() {
        assert_eq!(progress_text(indexed(0, 0), Duration::from_secs(5)), "\r\x1b[KIndexing 0/58 notes · 0.0/3.8 MB (0%)");
        assert_eq!(progress_text(indexed(12, 1_000_000), Duration::from_secs(60)), "\r\x1b[KIndexing 12/58 notes · 1.0/3.8 MB (25%) · ~3 min left");
        assert_eq!(progress_text(indexed(50, 3_600_000), Duration::from_secs(270)), "\r\x1b[KIndexing 50/58 notes · 3.4/3.8 MB (90%) · ~30 s left");
        assert_eq!(progress_text(indexed(58, 4_000_000), Duration::from_secs(300)), "\r\x1b[KIndexing 58/58 notes · 3.8/3.8 MB (100%)");
    }

    #[test]
    fn under_a_megabyte_both_byte_counts_are_in_kilobytes() {
        let kb = |files_done, bytes_done, bytes_total| progress_text(Progress::Indexed { files_done, files_total: 58, bytes_done, bytes_total }, Duration::from_secs(60));
        assert_eq!(kb(0, 0, 954_368), "\r\x1b[KIndexing 0/58 notes · 0/932 KB (0%)");
        assert_eq!(kb(0, 429_056, 954_368), "\r\x1b[KIndexing 0/58 notes · 419/932 KB (44%) · ~2 min left");
        assert_eq!(kb(1, 12, 12), "\r\x1b[KIndexing 1/58 notes · 1/1 KB (100%)", "a few bytes still show as 1 KB, as the total does");
    }
}
