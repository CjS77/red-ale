//! The red-ale binary: the JSON-RPC server, the MCP server, and a command-line front end for the same methods.

#![forbid(unsafe_code)]

mod params;

use std::io::{BufRead, Write};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::{Arc, Mutex};

use anyhow::{Context as _, Result, bail};
use clap::{Args, Parser, Subcommand};
use jsonrpsee::core::client::ClientT;
use jsonrpsee::core::params::ObjectParams;
use jsonrpsee::http_client::HttpClientBuilder;
use red_ale_rpc::{Dispatcher, RpcError};
use red_ale_service::Settings;
use red_ale_service::config::ConfigStore;
use red_ale_service::files::Retention;
use serde_json::{Value, json};

/// red-ale: GnuCash books, driven by JSON-RPC.
#[derive(Debug, Parser)]
#[command(version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Serve the JSON-RPC API over HTTP.
    Serve(ServeArgs),
    /// Serve every method as an MCP tool over standard input and output, for AI agents.
    Mcp(McpArgs),
    /// Call a method on a running server.
    Call(CallArgs),
    /// Open a book, run methods on it in this process, save if anything changed, and close it.
    Exec(ExecArgs),
    /// List every method.
    Methods,
    /// Print the API's schema document.
    Schema,
}

#[derive(Debug, Args)]
struct SettingsArgs {
    /// Keep backups and old journals this many days (0 deletes them at every save).
    #[arg(long, default_value_t = 30, conflicts_with = "keep_backups_forever")]
    retention_days: u32,
    /// Never delete backups or old journals.
    #[arg(long)]
    keep_backups_forever: bool,
    /// red-ale's configuration file (JSON), holding settings such as the accounting period. Without it they last
    /// only as long as the process.
    #[arg(long)]
    config: Option<PathBuf>,
}

impl SettingsArgs {
    fn settings(&self) -> Settings {
        let retention = match (self.keep_backups_forever, self.retention_days) {
            (true, _) => Retention::Forever,
            (false, 0) => Retention::Never,
            (false, days) => Retention::Days(days),
        };
        let config = self.config.as_ref().map_or_else(ConfigStore::in_memory, ConfigStore::at);
        Settings { retention, config, ..Settings::default() }
    }
}

#[derive(Debug, Args)]
struct ServeArgs {
    /// Address to listen on. Keep it on localhost: the API has no authentication.
    #[arg(long, default_value = "127.0.0.1:7878")]
    listen: SocketAddr,
    /// Open this book at start.
    #[arg(long)]
    book: Option<String>,
    /// Open the book read-only.
    #[arg(long, requires = "book")]
    read_only: bool,
    #[command(flatten)]
    settings: SettingsArgs,
}

#[derive(Debug, Args)]
struct McpArgs {
    /// Open this book at start.
    #[arg(long)]
    book: Option<String>,
    /// Open the book read-only.
    #[arg(long, requires = "book")]
    read_only: bool,
    #[command(flatten)]
    settings: SettingsArgs,
}

#[derive(Debug, Args)]
struct CallArgs {
    /// The server's URL.
    #[arg(long, default_value = "http://127.0.0.1:7878")]
    url: String,
    /// The method, such as `account.list`.
    method: String,
    /// Params: one JSON object, or `key=value` (a string) and `key:=json` pairs.
    params: Vec<String>,
    #[command(flatten)]
    input: InputArgs,
    #[command(flatten)]
    output: OutputArgs,
}

#[derive(Debug, Args)]
struct InputArgs {
    /// Read this file here, on the caller's side, into the `text` param, as `report.importConfigurations` and
    /// `report.importStyleSheets` take GnuCash's `saved-reports-2.8` and `stylesheets-2.0`. The server never reads a
    /// path a caller names.
    #[arg(long, value_name = "FILE")]
    file: Option<PathBuf>,
}

impl InputArgs {
    /// `params` with the file's text as `text`, when a file is given.
    fn fill(&self, mut params: Value) -> Result<Value> {
        let Some(path) = &self.file else { return Ok(params) };
        let text = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        let object = params.as_object_mut().context("--file needs params as an object")?;
        if object.contains_key("text") {
            bail!("--file fills the text param; do not give text as well");
        }
        object.insert("text".to_owned(), Value::String(text));
        Ok(params)
    }
}

#[derive(Debug, Args)]
struct OutputArgs {
    /// Write the result's `html` (from `report.export`) or `csv` (from `export.csvTree` and `export.csvTransactions`)
    /// to this file, here rather than on the server, and print what was written instead of the result: `{title, path,
    /// bytes}` for a report, the result's other fields with `path` and `bytes` for a CSV export.
    #[arg(long, value_name = "FILE")]
    output: Option<PathBuf>,
}

impl OutputArgs {
    /// Print a result, or with `--output` write its `html` or `csv` to the file and print what was written.
    fn emit(&self, result: &Value) -> Result<()> {
        let Some(path) = &self.output else { return print_json(result) };
        let (field, text) = ["html", "csv"]
            .into_iter()
            .find_map(|f| result.get(f).and_then(Value::as_str).map(|t| (f, t)))
            .context("--output needs a result with an html or csv field, such as report.export's or export.csvTree's")?;
        std::fs::write(path, text).with_context(|| format!("writing {}", path.display()))?;
        let (path, bytes) = (json!(path.display().to_string()), json!(text.len()));
        if field == "html" {
            let title = result.get("title").cloned().unwrap_or(Value::Null);
            return print_json(&json!({ "title": title, "path": path, "bytes": bytes }));
        }
        let mut summary = result.as_object().cloned().unwrap_or_default();
        summary.remove(field);
        summary.insert("path".to_owned(), path);
        summary.insert("bytes".to_owned(), bytes);
        print_json(&Value::Object(summary))
    }
}

#[derive(Debug, Args)]
struct ExecArgs {
    /// The book: a path or `xml://` URI. Without it the methods run with no book open, as the Configuration methods
    /// can.
    #[arg(long)]
    book: Option<String>,
    /// Create the book instead of opening it.
    #[arg(long, conflicts_with = "read_only", requires = "book")]
    create: bool,
    /// Open read-only.
    #[arg(long, requires = "book")]
    read_only: bool,
    /// Do not save, even if methods changed the book.
    #[arg(long)]
    no_save: bool,
    /// Run the methods in this file (`-` for standard input): one per line, either `method {json}` or
    /// `{"method": ..., "params": ...}`. Blank lines and lines starting with `#` are skipped. The first failure stops
    /// the script and nothing is saved.
    #[arg(long, conflicts_with = "method")]
    script: Option<PathBuf>,
    /// The method, such as `account.create`.
    #[arg(required_unless_present = "script")]
    method: Option<String>,
    /// Params: one JSON object, or `key=value` (a string) and `key:=json` pairs.
    params: Vec<String>,
    #[command(flatten)]
    input: InputArgs,
    #[command(flatten)]
    output: OutputArgs,
    #[command(flatten)]
    settings: SettingsArgs,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let outcome = match cli.command {
        Command::Serve(args) => serve(&args),
        Command::Mcp(args) => mcp(&args),
        Command::Call(args) => call(&args),
        Command::Exec(args) => exec(&args),
        Command::Methods => methods().map(|()| None),
        Command::Schema => print_json(&Dispatcher::new(Settings::default()).schema()).map(|()| None),
    };
    match outcome {
        Ok(None) => ExitCode::SUCCESS,
        Ok(Some(e)) => {
            eprintln!("{}", serde_json::to_string_pretty(&json!({ "error": e })).unwrap_or_else(|_| e.to_string()));
            ExitCode::from(2)
        }
        // A reader that stops early, as `red-ale schema | head` does, is not a failure. Every command closes its book
        // before printing, so nothing is lost.
        Err(e) if is_broken_pipe(&e) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("red-ale: {e:#}");
            ExitCode::from(1)
        }
    }
}

fn print_json(v: &Value) -> Result<()> {
    let mut out = std::io::stdout().lock();
    serde_json::to_writer_pretty(&mut out, v)?;
    writeln!(out)?;
    Ok(())
}

fn is_broken_pipe(e: &anyhow::Error) -> bool {
    e.chain().any(|cause| {
        let io = cause.downcast_ref::<std::io::Error>().map(std::io::Error::kind);
        let json = cause.downcast_ref::<serde_json::Error>().and_then(serde_json::Error::io_error_kind);
        io.or(json) == Some(std::io::ErrorKind::BrokenPipe)
    })
}

fn methods() -> Result<()> {
    let dispatcher = Dispatcher::new(Settings::default());
    let mut out = std::io::stdout().lock();
    dispatcher
        .methods()
        .try_for_each(|m| writeln!(out, "{:<32} {:<12} {}", m.name, format!("{:?}", m.effect).to_lowercase(), m.summary))?;
    Ok(())
}

/// A dispatcher for a server, with `book` opened in it if given. Reports go to standard error, never standard output.
fn server_dispatcher(settings: &SettingsArgs, book: Option<&str>, read_only: bool) -> Result<Arc<Mutex<Dispatcher>>> {
    let mut dispatcher = Dispatcher::new(settings.settings());
    if let Some(book) = book {
        let info = dispatcher.call("book.open", json!({ "path": book, "readOnly": read_only })).map_err(|e| anyhow::anyhow!("{e}"))?;
        eprintln!("red-ale: opened {}", info["path"].as_str().unwrap_or_default());
    }
    Ok(Arc::new(Mutex::new(dispatcher)))
}

/// Run the server until interrupted. An RPC error is never returned from here, so `Ok(None)` on success.
fn serve(args: &ServeArgs) -> Result<Option<RpcError>> {
    let config = args.settings.settings().config;
    let limits = (config.jsonrpc_max_request_bytes()?, config.jsonrpc_max_response_bytes()?);
    let dispatcher = server_dispatcher(&args.settings, args.book.as_deref(), args.read_only)?;
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let (addr, handle) = red_ale_rpc::server::serve(dispatcher.clone(), args.listen, limits).await?;
        eprintln!("red-ale: serving JSON-RPC on http://{addr}");
        shutdown_signal().await?;
        handle.stop().ok();
        handle.stopped().await;
        anyhow::Ok(())
    })?;
    shut_down(&dispatcher);
    Ok(None)
}

/// Serve MCP on standard input and output until the client closes standard input or a signal arrives, then close
/// the book as `serve` does. Standard output carries the protocol only.
fn mcp(args: &McpArgs) -> Result<Option<RpcError>> {
    let max_request_bytes = args.settings.settings().config.mcp_max_request_bytes()?;
    let dispatcher = server_dispatcher(&args.settings, args.book.as_deref(), args.read_only)?;
    let runtime = tokio::runtime::Runtime::new()?;
    let outcome = runtime.block_on(async {
        tokio::select! {
            served = red_ale_mcp::serve_stdio(dispatcher.clone(), max_request_bytes) => served.map_err(anyhow::Error::from),
            signalled = shutdown_signal() => signalled,
        }
    });
    shut_down(&dispatcher);
    // Reading standard input blocks a thread that a plain drop of the runtime would wait for.
    runtime.shutdown_background();
    outcome.map(|()| None)
}

/// Wait for Ctrl-C (SIGINT) or SIGTERM, so a service manager stopping the server also releases the book's lock.
async fn shutdown_signal() -> Result<()> {
    let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).context("installing the SIGTERM handler")?;
    tokio::select! {
        r = tokio::signal::ctrl_c() => r.context("waiting for Ctrl-C")?,
        _ = term.recv() => {}
    }
    Ok(())
}

/// Close the open book, if any. Unsaved changes are not saved without being asked; the journal still holds them.
fn shut_down(dispatcher: &Mutex<Dispatcher>) {
    let mut d = dispatcher.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let Ok(info) = d.call("book.info", Value::Null) else { return };
    if info["dirty"].as_bool() == Some(true) {
        let journal = info["journal"].as_str().unwrap_or("the journal");
        eprintln!("red-ale: unsaved changes were not saved; they are recorded in {journal}");
    }
    if let Err(e) = d.call("book.close", json!({ "discard": true })) {
        eprintln!("red-ale: closing the book failed: {e}");
    }
}

fn call(args: &CallArgs) -> Result<Option<RpcError>> {
    let params = args.input.fill(params::parse(&args.params)?)?;
    let runtime = tokio::runtime::Runtime::new()?;
    let result: Result<Value, jsonrpsee::core::ClientError> = runtime.block_on(async {
        // The server's own limit bounds what it sends (`jsonRpc.maxResponseBytes`), so the client takes any size.
        let client = HttpClientBuilder::default().max_response_size(u32::MAX).build(&args.url)?;
        let mut object = ObjectParams::new();
        params.as_object().into_iter().flatten().try_for_each(|(k, v)| object.insert(k, v))?;
        client.request(&args.method, object).await
    });
    match result {
        Ok(v) => args.output.emit(&v).map(|()| None),
        Err(jsonrpsee::core::ClientError::Call(e)) => {
            let data = e.data().and_then(|d| serde_json::from_str(d.get()).ok());
            Ok(Some(RpcError { code: e.code(), message: e.message().to_owned(), data }))
        }
        Err(e) => Err(anyhow::anyhow!("calling {}: {e}", args.url)),
    }
}

fn exec(args: &ExecArgs) -> Result<Option<RpcError>> {
    if args.script.is_some() && args.output.output.is_some() {
        bail!("--output writes one method's result; it does not combine with --script");
    }
    if args.script.is_some() && args.input.file.is_some() {
        bail!("--file fills one method's text param; it does not combine with --script");
    }
    let calls = match (&args.script, &args.method) {
        (Some(script), _) => read_script(script)?,
        (None, Some(method)) => vec![(method.clone(), args.input.fill(params::parse(&args.params)?)?)],
        (None, None) => bail!("give a method or --script"),
    };
    let mut d = Dispatcher::new(args.settings.settings());
    if let Err(e) = open_book(&mut d, args) {
        return Ok(Some(e));
    }
    let outcome = run_calls(&mut d, calls);
    let finish = match &outcome {
        _ if !d.context().has_session() => Ok(Value::Null),
        Ok(_) if !args.no_save => d.call("book.close", json!({ "save": true })),
        _ => d.call("book.close", json!({ "discard": true })),
    };
    match (outcome, finish) {
        (Err(e), _) | (Ok(_), Err(e)) => Ok(Some(e)),
        (Ok(results), Ok(_)) if args.script.is_some() => print_json(&Value::Array(results)).map(|()| None),
        (Ok(results), Ok(_)) => args.output.emit(&results.into_iter().next().unwrap_or(Value::Null)).map(|()| None),
    }
}

/// Open or create `--book`, if given.
fn open_book(d: &mut Dispatcher, args: &ExecArgs) -> Result<(), RpcError> {
    let opened = match &args.book {
        None => return Ok(()),
        Some(book) if args.create => d.call("book.create", json!({ "path": book })),
        Some(book) => d.call("book.open", json!({ "path": book, "readOnly": args.read_only })),
    };
    opened.map(drop)
}

fn run_calls(d: &mut Dispatcher, calls: Vec<(String, Value)>) -> Result<Vec<Value>, RpcError> {
    calls.into_iter().map(|(method, params)| d.call(&method, params)).collect()
}

fn read_script(path: &Path) -> Result<Vec<(String, Value)>> {
    let reader: Box<dyn BufRead> = if path == Path::new("-") {
        Box::new(std::io::stdin().lock())
    } else {
        Box::new(std::io::BufReader::new(std::fs::File::open(path).with_context(|| format!("opening {}", path.display()))?))
    };
    reader
        .lines()
        .enumerate()
        .map(|(i, line)| line.map(|l| (i + 1, l)))
        .filter(|r| r.as_ref().map_or(true, |(_, l)| !l.trim().is_empty() && !l.trim_start().starts_with('#')))
        .map(|r| {
            let (number, line) = r?;
            params::script_line(&line).with_context(|| format!("{}:{number}", path.display()))
        })
        .collect()
}
