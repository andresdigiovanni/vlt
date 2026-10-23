use std::io::Write;

use clap::{Parser, Subcommand};
use serde_json::Value;

use crate::app::App;
use crate::documents::{DocumentResponse, PropertiesResponse};
use crate::installer::models::{missing, pull_showing_progress};
use crate::installer::{InstallOptions, Target};
use crate::ml::models::Models;
use crate::paths;
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
        Command::Properties { path_prefix, key, json } => {
            let app = App::load()?;
            let resp = app.list_properties(path_prefix.as_deref(), key.as_deref())?;
            if json {
                print_json(&resp)
            } else {
                write_out(format_args!("{}", render_properties(&resp)))
            }
        }
        Command::Reindex { force } => {
            let app = App::load()?;
            pull_missing_models(&app)?;
            let mut shown = false;
            let out = app.reindex(force, &mut |done, total| {
                if total > 0 {
                    eprint!("\rIndexing {done}/{total}");
                    let _ = std::io::stderr().flush();
                    shown = true;
                }
            })?;
            if shown {
                eprintln!();
            }
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
    for p in &resp.properties {
        let top: Vec<String> = p
            .top_values
            .iter()
            .map(|(v, n)| match v {
                Value::String(s) => format!("{s} ({n})"),
                other => format!("{other} ({n})"),
            })
            .collect();
        rows.push([p.key.clone(), p.kind.clone(), p.documents.to_string(), top.join(", ")]);
    }
    let width = |i: usize| rows.iter().map(|r| r[i].chars().count()).max().unwrap_or(0);
    let widths = [width(0), width(1), width(2)];
    rows.iter()
        .map(|r| {
            format!(
                "{:w0$}  {:w1$}  {:w2$}  {}\n",
                r[0],
                r[1],
                r[2],
                r[3],
                w0 = widths[0],
                w1 = widths[1],
                w2 = widths[2]
            )
        })
        .collect()
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
