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
}

#[derive(Debug, Args)]
struct ExecArgs {
    /// The book: a path or `xml://` URI.
    #[arg(long)]
    book: String,
    /// Create the book instead of opening it.
    #[arg(long, conflicts_with = "read_only")]
    create: bool,
    /// Open read-only.
    #[arg(long)]
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
    settings: SettingsArgs,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let outcome = match cli.command {
        Command::Serve(args) => serve(&args),
        Command::Mcp(args) => mcp(&args),
        Command::Call(args) => call(&args),
        Command::Exec(args) => exec(&args),
        Command::Methods => {
            methods();
            Ok(None)
        }
        Command::Schema => print_json(&Dispatcher::new(Settings::default()).schema()).map(|()| None),
    };
    match outcome {
        Ok(None) => ExitCode::SUCCESS,
        Ok(Some(e)) => {
            eprintln!("{}", serde_json::to_string_pretty(&json!({ "error": e })).unwrap_or_else(|_| e.to_string()));
            ExitCode::from(2)
        }
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

fn methods() {
    let dispatcher = Dispatcher::new(Settings::default());
    dispatcher.methods().for_each(|m| println!("{:<32} {:<12} {}", m.name, format!("{:?}", m.effect).to_lowercase(), m.summary));
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
    let dispatcher = server_dispatcher(&args.settings, args.book.as_deref(), args.read_only)?;
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let (addr, handle) = red_ale_rpc::server::serve(dispatcher.clone(), args.listen).await?;
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
    let dispatcher = server_dispatcher(&args.settings, args.book.as_deref(), args.read_only)?;
    let runtime = tokio::runtime::Runtime::new()?;
    let outcome = runtime.block_on(async {
        tokio::select! {
            served = red_ale_mcp::serve_stdio(dispatcher.clone()) => served.map_err(anyhow::Error::from),
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
    let params = params::parse(&args.params)?;
    let runtime = tokio::runtime::Runtime::new()?;
    let result: Result<Value, jsonrpsee::core::ClientError> = runtime.block_on(async {
        let client = HttpClientBuilder::default().build(&args.url)?;
        let mut object = ObjectParams::new();
        params.as_object().into_iter().flatten().try_for_each(|(k, v)| object.insert(k, v))?;
        client.request(&args.method, object).await
    });
    match result {
        Ok(v) => print_json(&v).map(|()| None),
        Err(jsonrpsee::core::ClientError::Call(e)) => {
            let data = e.data().and_then(|d| serde_json::from_str(d.get()).ok());
            Ok(Some(RpcError { code: e.code(), message: e.message().to_owned(), data }))
        }
        Err(e) => Err(anyhow::anyhow!("calling {}: {e}", args.url)),
    }
}

fn exec(args: &ExecArgs) -> Result<Option<RpcError>> {
    let calls = match (&args.script, &args.method) {
        (Some(script), _) => read_script(script)?,
        (None, Some(method)) => vec![(method.clone(), params::parse(&args.params)?)],
        (None, None) => bail!("give a method or --script"),
    };
    let mut d = Dispatcher::new(args.settings.settings());
    let opened = if args.create {
        d.call("book.create", json!({ "path": args.book }))
    } else {
        d.call("book.open", json!({ "path": args.book, "readOnly": args.read_only }))
    };
    if let Err(e) = opened {
        return Ok(Some(e));
    }
    let outcome = run_calls(&mut d, calls);
    let finish = match &outcome {
        Ok(_) if !args.no_save => d.call("book.close", json!({ "save": true })),
        _ => d.call("book.close", json!({ "discard": true })),
    };
    match (outcome, finish) {
        (Err(e), _) | (Ok(_), Err(e)) => Ok(Some(e)),
        (Ok(results), Ok(_)) => {
            let output = if args.script.is_some() { Value::Array(results) } else { results.into_iter().next().unwrap_or(Value::Null) };
            print_json(&output).map(|()| None)
        }
    }
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
