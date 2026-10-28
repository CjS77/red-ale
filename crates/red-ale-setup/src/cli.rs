//! The CLI adapters: Harnesses whose own command line adds and removes servers, which `setup` runs because the
//! Harness owns its format (`design/setup.md`, "Decided", item 1). Each reads its config only to plan, and never
//! writes it. Command lines are from `design/notes/mcp-harnesses.md`, cited by section.

use std::fmt;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::adapters::{DevinFile, plain};
use crate::file::{self, Location};
use crate::json::Dialect;
use crate::{Action, Applied, Change, Entry, Env, Error, Harness, NAME, Plan, Scope, Work};

/// One run of a Harness CLI.
#[derive(Debug)]
pub(crate) struct Invocation {
    program: PathBuf,
    args: Vec<String>,
    /// The project directory, for a CLI that takes its project from the working directory.
    dir: Option<PathBuf>,
}

impl fmt::Display for Invocation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let plain = |a: &str| !a.is_empty() && a.bytes().all(|b| b.is_ascii_alphanumeric() || b"-_./=:@%+,".contains(&b));
        let quoted = |a: &String| if plain(a) { a.clone() } else { format!("'{}'", a.replace('\'', r"'\''")) };
        let line: Vec<String> = std::iter::once(self.program.display().to_string()).chain(self.args.iter().map(quoted)).collect();
        match &self.dir {
            Some(dir) => write!(f, "(cd {} && {})", quoted(&dir.display().to_string()), line.join(" ")),
            None => write!(f, "{}", line.join(" ")),
        }
    }
}

/// A Harness set up through its own CLI.
pub(crate) trait CliAdapter {
    fn id(&self) -> &'static str;
    fn name(&self) -> &'static str;
    /// The CLI's command name.
    fn program(&self) -> &'static str;
    /// The CLI, if it is installed.
    fn binary(&self, env: &Env) -> Option<PathBuf> {
        env.which(self.program())
    }
    /// The file the CLI writes for `scope`, read only to plan.
    fn config(&self, env: &Env, scope: &Scope) -> Result<PathBuf, Error>;
    /// The red-ale entry the CLI wrote, if any.
    fn current(&self, env: &Env, scope: &Scope) -> Result<Option<Value>, Error> {
        json_entry(self.config(env, scope)?, &["mcpServers"])
    }
    /// The arguments that add the red-ale entry.
    fn add(&self, scope: &Scope, entry: &Entry) -> Vec<String>;
    /// The arguments that remove it.
    fn remove(&self, scope: &Scope) -> Vec<String>;
    fn restart(&self) -> Option<&'static str>;
}

/// A [`CliAdapter`] as a [`Harness`].
pub(crate) struct Cli<A>(pub(crate) A);

impl<A: CliAdapter> Harness for Cli<A> {
    fn id(&self) -> &'static str {
        self.0.id()
    }

    fn name(&self) -> &'static str {
        self.0.name()
    }

    fn detect(&self, env: &Env) -> bool {
        self.0.binary(env).is_some()
    }

    fn config(&self, env: &Env, scope: &Scope) -> Result<PathBuf, Error> {
        self.0.config(env, scope)
    }

    fn read_entry(&self, env: &Env, scope: &Scope) -> Result<Option<Value>, Error> {
        self.0.current(env, scope)
    }

    fn plan(&self, env: &Env, scope: &Scope, entry: &Entry) -> Plan {
        plan(&self.0, env, scope, &Change::Set(entry.clone()))
    }

    fn plan_removal(&self, env: &Env, scope: &Scope) -> Plan {
        plan(&self.0, env, scope, &Change::Remove)
    }

    /// Runs the commands in order and stops at the first that fails. An update is a removal then an addition, so if
    /// the addition fails the Harness is left with no red-ale entry, which the error says.
    fn apply(&self, env: &Env, plan: Plan) -> Result<Applied, Error> {
        let Plan { action, config, work, .. } = plan;
        if let Action::Skip(e) = action {
            return Err(e);
        }
        if let Work::Cli(commands) = &work {
            commands.iter().try_for_each(|c| {
                let message = |message| Error::Cli { command: c.to_string(), message };
                env.run(&c.program, &c.args, c.dir.as_deref()).map_err(message)
            })?;
        }
        Ok(Applied { action, config: config.unwrap_or_default(), backup: None })
    }

    fn restart(&self) -> Option<&'static str> {
        self.0.restart()
    }
}

fn plan(adapter: &impl CliAdapter, env: &Env, scope: &Scope, change: &Change) -> Plan {
    let config = adapter.config(env, scope).ok();
    match planned(adapter, env, scope, change) {
        Ok((action, commands)) => Plan {
            action,
            config,
            scope: scope.clone(),
            change: change.clone(),
            work: Work::Cli(commands),
        },
        Err(e) => Plan::skip(scope, change, config, e),
    }
}

/// The action and the commands that carry it out. Every CLI is told to remove before it adds, because some refuse an
/// existing name (Claude Code) and others replace it (Codex, Gemini), §"Cross-cutting findings", item 3.
fn planned(adapter: &impl CliAdapter, env: &Env, scope: &Scope, change: &Change) -> Result<(Action, Vec<Invocation>), Error> {
    let program = adapter.binary(env).ok_or(Error::NotOnPath(adapter.program()))?;
    let current = adapter.current(env, scope)?;
    let dir = match scope {
        Scope::User => None,
        Scope::Project(dir) => Some(dir.clone()),
    };
    let run = |args| Invocation { program: program.clone(), args, dir: dir.clone() };
    Ok(match (change, current) {
        (Change::Set(entry), None) => (Action::Add, vec![run(adapter.add(scope, entry))]),
        (Change::Set(entry), Some(old)) if runs(&old, entry) => (Action::Nothing, Vec::new()),
        (Change::Set(entry), Some(old)) => {
            let diff = file::entry_diff(&essentials(&old), &plain(entry));
            (Action::Update { diff }, vec![run(adapter.remove(scope)), run(adapter.add(scope, entry))])
        }
        (Change::Remove, None) => (Action::Nothing, Vec::new()),
        (Change::Remove, Some(_)) => (Action::Remove, vec![run(adapter.remove(scope))]),
    })
}

/// Whether an entry a CLI wrote runs `entry`: the same command and arguments, with no environment. Each CLI adds
/// fields of its own (Claude Code `type` and an empty `env`, Copilot CLI `tools`), so the entry is not compared whole.
fn runs(existing: &Value, entry: &Entry) -> bool {
    let args = existing.get("args").cloned().unwrap_or_else(|| json!([]));
    let no_env = existing.get("env").is_none_or(|e| e.as_object().is_some_and(serde_json::Map::is_empty));
    existing.get("command") == Some(&json!(entry.command)) && args == json!(entry.args) && no_env
}

/// The fields of an entry [`runs`] compares, so the diff shown for an update leaves out what the CLI adds itself.
fn essentials(existing: &Value) -> Value {
    let fields = ["command", "args", "env"].into_iter().filter_map(|key| {
        let value = existing.get(key).filter(|v| v.as_object().is_none_or(|o| !o.is_empty()))?;
        Some((key.to_owned(), value.clone()))
    });
    Value::Object(fields.collect())
}

fn json_entry(path: PathBuf, keys: &'static [&'static str]) -> Result<Option<Value>, Error> {
    file::read_entry(&Location { path, dialect: Dialect::Jsonc, keys, render: plain })
}

/// `user`, or `project` for `--project`.
fn scope_flag(scope: &Scope) -> String {
    match scope {
        Scope::User => "user".to_owned(),
        Scope::Project(_) => "project".to_owned(),
    }
}

/// `<name> -- <command> <args>…`, the tail most CLIs take.
fn server(entry: &Entry) -> Vec<String> {
    [NAME.to_owned(), "--".to_owned()].into_iter().chain(entry.command_line()).collect()
}

fn args(head: &[&str], tail: impl IntoIterator<Item = String>) -> Vec<String> {
    head.iter().map(|s| (*s).to_owned()).chain(tail).collect()
}

/// Claude Code: `claude mcp add -s user red-ale -- <abs> mcp` and `claude mcp remove -s user red-ale` (§1 "CLI").
/// The user entry is the top-level `mcpServers` of `~/.claude.json`, or of `$CLAUDE_CONFIG_DIR/.claude.json` (§1
/// "Locations"); Claude Code rewrites that file constantly, so it is only read (§1 "Quirks"). `-s project` writes the
/// project's `.mcp.json`.
pub(crate) struct ClaudeCode;

impl CliAdapter for ClaudeCode {
    fn id(&self) -> &'static str {
        "claude-code"
    }

    fn name(&self) -> &'static str {
        "Claude Code"
    }

    fn program(&self) -> &'static str {
        "claude"
    }

    /// On `PATH`, or where the native installer puts it (§1 "Detection").
    fn binary(&self, env: &Env) -> Option<PathBuf> {
        let native = || env.home().ok().map(|h| h.join(".local/bin/claude")).filter(|p| p.is_file());
        env.which("claude").or_else(native)
    }

    fn config(&self, env: &Env, scope: &Scope) -> Result<PathBuf, Error> {
        match scope {
            Scope::User => Ok(env.path_var("CLAUDE_CONFIG_DIR").map_or_else(|| env.home(), Ok)?.join(".claude.json")),
            Scope::Project(dir) => Ok(dir.join(".mcp.json")),
        }
    }

    fn add(&self, scope: &Scope, entry: &Entry) -> Vec<String> {
        args(&["mcp", "add", "-s", &scope_flag(scope)], server(entry))
    }

    fn remove(&self, scope: &Scope) -> Vec<String> {
        args(&["mcp", "remove", "-s", &scope_flag(scope), NAME], [])
    }

    fn restart(&self) -> Option<&'static str> {
        Some("start a new Claude Code session")
    }
}

/// Codex: `codex mcp add red-ale -- <abs> mcp` and `codex mcp remove red-ale`, which write only the user's
/// `$CODEX_HOME/config.toml` (default `~/.codex/`), `[mcp_servers.red-ale]`; there is no scope flag, so no project
/// entry (§2 "Locations", "CLI").
pub(crate) struct Codex;

impl CliAdapter for Codex {
    fn id(&self) -> &'static str {
        "codex"
    }

    fn name(&self) -> &'static str {
        "Codex"
    }

    fn program(&self) -> &'static str {
        "codex"
    }

    fn config(&self, env: &Env, scope: &Scope) -> Result<PathBuf, Error> {
        let Scope::User = scope else { return Err(Error::NoProjectScope("Codex")) };
        Ok(env.path_var("CODEX_HOME").map_or_else(|| env.home().map(|h| h.join(".codex")), Ok)?.join("config.toml"))
    }

    fn current(&self, env: &Env, scope: &Scope) -> Result<Option<Value>, Error> {
        toml_entry(&self.config(env, scope)?)
    }

    fn add(&self, _scope: &Scope, entry: &Entry) -> Vec<String> {
        args(&["mcp", "add"], server(entry))
    }

    fn remove(&self, _scope: &Scope) -> Vec<String> {
        args(&["mcp", "remove", NAME], [])
    }

    fn restart(&self) -> Option<&'static str> {
        Some("restart Codex (the IDE extension: Restart extension)")
    }
}

/// `[mcp_servers.red-ale]` of a Codex config, read only.
fn toml_entry(path: &Path) -> Result<Option<Value>, Error> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(source) => return Err(Error::Read { path: path.to_path_buf(), source }),
    };
    let invalid = |message: String| Error::Invalid { path: path.to_path_buf(), format: "TOML", message };
    let table: toml::Table = text.parse().map_err(|e: toml::de::Error| invalid(e.to_string()))?;
    let servers = serde_json::to_value(table).map_err(|e| invalid(e.to_string()))?;
    Ok(servers.get("mcp_servers").and_then(|s| s.get(NAME)).cloned())
}

/// GitHub Copilot CLI: `copilot mcp add --tools '*' red-ale -- <abs> mcp` (`tools` is required) and
/// `copilot mcp remove red-ale`, user scope only, `$COPILOT_HOME/mcp-config.json` (default `~/.copilot/`) (§10
/// "Locations", "Entry", "CLI").
pub(crate) struct CopilotCli;

/// The marker in VS Code's `copilot` shim (§10 "Detection").
const SHIM_MARKER: &[u8] = b"VSCODE_COPILOT_RUST_SHIM_V1";

/// Whether a `copilot` is VS Code's shim rather than the CLI: it resolves into a `copilot-shim` directory, or it is
/// small (the shim is about 1 MB, the CLI about 150 MB) and holds the shim's marker (§10 "Detection").
fn is_shim(path: &Path) -> bool {
    let resolved = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let in_shim_dir = resolved.parent().and_then(Path::file_name).is_some_and(|d| d == "copilot-shim");
    let small = std::fs::metadata(&resolved).is_ok_and(|m| m.len() < 16 << 20);
    in_shim_dir || (small && std::fs::read(&resolved).is_ok_and(|b| b.windows(SHIM_MARKER.len()).any(|w| w == SHIM_MARKER)))
}

impl CliAdapter for CopilotCli {
    fn id(&self) -> &'static str {
        "copilot-cli"
    }

    fn name(&self) -> &'static str {
        "GitHub Copilot CLI"
    }

    fn program(&self) -> &'static str {
        "copilot"
    }

    /// The first `copilot` on `PATH` that is not VS Code's shim, else `~/.local/bin/copilot` (§10 "Detection").
    fn binary(&self, env: &Env) -> Option<PathBuf> {
        let local = || env.home().ok().map(|h| h.join(".local/bin/copilot")).filter(|p| p.is_file() && !is_shim(p));
        env.which_all("copilot").find(|p| !is_shim(p)).or_else(local)
    }

    fn config(&self, env: &Env, scope: &Scope) -> Result<PathBuf, Error> {
        let Scope::User = scope else { return Err(Error::NoProjectScope("GitHub Copilot CLI")) };
        Ok(env.path_var("COPILOT_HOME").map_or_else(|| env.home().map(|h| h.join(".copilot")), Ok)?.join("mcp-config.json"))
    }

    fn add(&self, _scope: &Scope, entry: &Entry) -> Vec<String> {
        args(&["mcp", "add", "--tools", "*"], server(entry))
    }

    fn remove(&self, _scope: &Scope) -> Vec<String> {
        args(&["mcp", "remove", NAME], [])
    }

    /// Servers added through the CLI are available without a restart (§10 "CLI").
    fn restart(&self) -> Option<&'static str> {
        None
    }
}

/// Gemini CLI: `gemini mcp add -s user red-ale <abs> mcp` (flags before the name, no `--`) and
/// `gemini mcp remove -s user red-ale`; `~/.gemini/settings.json`, under `$GEMINI_CLI_HOME` if set, or the project's
/// `.gemini/settings.json` with `-s project` (§4 "Locations", "CLI").
pub(crate) struct Gemini;

impl CliAdapter for Gemini {
    fn id(&self) -> &'static str {
        "gemini"
    }

    fn name(&self) -> &'static str {
        "Gemini CLI"
    }

    fn program(&self) -> &'static str {
        "gemini"
    }

    fn config(&self, env: &Env, scope: &Scope) -> Result<PathBuf, Error> {
        let home = match scope {
            Scope::User => env.path_var("GEMINI_CLI_HOME").map_or_else(|| env.home(), Ok)?,
            Scope::Project(dir) => dir.clone(),
        };
        Ok(home.join(".gemini/settings.json"))
    }

    fn add(&self, scope: &Scope, entry: &Entry) -> Vec<String> {
        args(&["mcp", "add", "-s", &scope_flag(scope), NAME], entry.command_line())
    }

    fn remove(&self, scope: &Scope) -> Vec<String> {
        args(&["mcp", "remove", "-s", &scope_flag(scope), NAME], [])
    }

    fn restart(&self) -> Option<&'static str> {
        Some("restart Gemini CLI")
    }
}

/// Devin CLI: `devin mcp add -s user red-ale -- <abs> mcp` and `devin mcp remove -s user red-ale` (§8 "CLI"), which
/// write the same files as [`DevinFile`]; `-s project` writes `.devin/mcp_config.json`.
pub(crate) struct DevinCli;

impl CliAdapter for DevinCli {
    fn id(&self) -> &'static str {
        "devin"
    }

    fn name(&self) -> &'static str {
        "Devin"
    }

    fn program(&self) -> &'static str {
        "devin"
    }

    fn config(&self, env: &Env, scope: &Scope) -> Result<PathBuf, Error> {
        match scope {
            Scope::User => Ok(DevinFile::dir(env)?.join("mcp_config.json")),
            Scope::Project(dir) => Ok(dir.join(".devin/mcp_config.json")),
        }
    }

    fn add(&self, scope: &Scope, entry: &Entry) -> Vec<String> {
        args(&["mcp", "add", "-s", &scope_flag(scope)], server(entry))
    }

    fn remove(&self, scope: &Scope) -> Vec<String> {
        args(&["mcp", "remove", "-s", &scope_flag(scope), NAME], [])
    }

    fn restart(&self) -> Option<&'static str> {
        Some("restart Devin")
    }
}

/// Devin: its CLI when `devin` is on `PATH`, else its file (`design/setup.md`, "First release").
pub(crate) struct Devin;

impl Devin {
    fn form(env: &Env) -> &'static dyn Harness {
        if env.which("devin").is_some() { &Cli(DevinCli) } else { &DevinFile }
    }
}

impl Harness for Devin {
    fn id(&self) -> &'static str {
        "devin"
    }

    fn name(&self) -> &'static str {
        "Devin"
    }

    fn detect(&self, env: &Env) -> bool {
        Harness::detect(&DevinFile, env)
    }

    fn config(&self, env: &Env, scope: &Scope) -> Result<PathBuf, Error> {
        Self::form(env).config(env, scope)
    }

    fn read_entry(&self, env: &Env, scope: &Scope) -> Result<Option<Value>, Error> {
        Self::form(env).read_entry(env, scope)
    }

    fn plan(&self, env: &Env, scope: &Scope, entry: &Entry) -> Plan {
        Self::form(env).plan(env, scope, entry)
    }

    fn plan_removal(&self, env: &Env, scope: &Scope) -> Plan {
        Self::form(env).plan_removal(env, scope)
    }

    fn apply(&self, env: &Env, plan: Plan) -> Result<Applied, Error> {
        match plan.work {
            Work::Cli(_) => Cli(DevinCli).apply(env, plan),
            Work::File(_) | Work::Nothing => DevinFile.apply(env, plan),
        }
    }

    fn restart(&self) -> Option<&'static str> {
        Some("restart Devin")
    }
}
