//! The file adapters: Harnesses whose config `setup` edits itself. Paths, keys and entry shapes are from
//! `design/notes/mcp-harnesses.md`, cited by section; `design/setup.md`, "First release", chooses among them.

use std::cell::OnceCell;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::file::{self, Committed, Location};
use crate::json::Dialect;
use crate::{Action, Applied, Change, Entry, Env, Error, Harness, Os, Plan, Scope};

/// How many times a plan is made again when the Harness changed its config meanwhile (`design/setup.md`, "Not
/// changed underneath").
const REPLANS: usize = 3;

/// The adapter for `id`, if it is a file adapter.
pub(crate) fn by_id(id: &str) -> Option<Box<dyn Harness>> {
    let adapter: Box<dyn Harness> = match id {
        "opencode" => Box::new(Opencode::default()),
        "claude-desktop" => Box::new(ClaudeDesktop),
        "cursor" => Box::new(Cursor),
        "vscode" => Box::new(VsCode),
        "antigravity" => Box::new(Antigravity),
        // The CLI form, when `devin` is on `PATH`, wraps this one (`design/setup.md`, "First release").
        "devin" => Box::new(DevinFile),
        "zed" => Box::new(Zed),
        _ => return None,
    };
    Some(adapter)
}

/// A Harness whose config is a JSON or JSONC file that `setup` edits. Every such adapter is a [`Harness`].
pub(crate) trait FileAdapter {
    fn id(&self) -> &'static str;
    fn name(&self) -> &'static str;
    fn detect(&self, env: &Env) -> bool;
    fn location(&self, env: &Env, scope: &Scope) -> Result<Location, Error>;
    /// Why the file can't be written now, even though it parses.
    fn blocker(&self, _env: &Env) -> Option<Error> {
        None
    }
    fn restart(&self) -> Option<&'static str> {
        None
    }
}

impl<A: FileAdapter> Harness for A {
    fn id(&self) -> &'static str {
        FileAdapter::id(self)
    }

    fn name(&self) -> &'static str {
        FileAdapter::name(self)
    }

    fn detect(&self, env: &Env) -> bool {
        FileAdapter::detect(self, env)
    }

    fn config(&self, env: &Env, scope: &Scope) -> Result<PathBuf, Error> {
        self.location(env, scope).map(|l| l.path)
    }

    fn read_entry(&self, env: &Env, scope: &Scope) -> Result<Option<Value>, Error> {
        file::read_entry(&self.location(env, scope)?)
    }

    fn plan(&self, env: &Env, scope: &Scope, entry: &Entry) -> Plan {
        plan_file(self, env, scope, &Change::Set(entry.clone()))
    }

    fn plan_removal(&self, env: &Env, scope: &Scope) -> Plan {
        plan_file(self, env, scope, &Change::Remove)
    }

    fn apply(&self, env: &Env, plan: Plan) -> Result<Applied, Error> {
        apply_file(self, env, plan)
    }

    fn restart(&self) -> Option<&'static str> {
        FileAdapter::restart(self)
    }
}

fn plan_file(adapter: &impl FileAdapter, env: &Env, scope: &Scope, change: &Change) -> Plan {
    let location = match adapter.location(env, scope) {
        Ok(location) => location,
        Err(e) => return Plan::skip(scope, change, None, e),
    };
    let config = Some(location.path.clone());
    match file::plan(&location, change) {
        Err(e) => Plan::skip(scope, change, config, e),
        Ok((action, edit)) => match action.changes().then(|| adapter.blocker(env)).flatten() {
            Some(blocker) => Plan::skip(scope, change, config, blocker),
            None => Plan {
                action,
                config,
                scope: scope.clone(),
                change: change.clone(),
                edit: Some(edit),
            },
        },
    }
}

/// Apply a plan, planning again while the Harness changes its config underneath.
fn apply_file(adapter: &impl FileAdapter, env: &Env, mut plan: Plan) -> Result<Applied, Error> {
    for _ in 0..=REPLANS {
        let Plan { action, config, scope, change, edit } = plan;
        let config = config.unwrap_or_default();
        let (action, edit) = match (action, edit) {
            (Action::Skip(e), _) => return Err(e),
            (action, Some(edit)) if action.changes() => (action, edit),
            (action, _) => return Ok(Applied { action, config, backup: None }),
        };
        if let Some(blocker) = adapter.blocker(env) {
            return Err(blocker);
        }
        match file::commit(&edit, &|_| Ok(()))? {
            Committed::Written(backup) => return Ok(Applied { action, config, backup }),
            Committed::Changed => plan = plan_file(adapter, env, &scope, &change),
        }
    }
    Err(Error::KeptChanging(plan.config.unwrap_or_default()))
}

/// `{command, args}`: the entry Claude Desktop, Antigravity, Devin and Zed share (§5, §8, §9, "Google Antigravity").
fn plain(entry: &Entry) -> Value {
    json!({ "command": entry.command, "args": entry.args })
}

/// `{type: "stdio", command, args}`: Cursor's and VS Code's entry (§6 "Entry", §7 "Entry").
fn stdio(entry: &Entry) -> Value {
    json!({ "type": "stdio", "command": entry.command, "args": entry.args })
}

fn user_or_project(scope: &Scope, user: impl FnOnce() -> Result<PathBuf, Error>, project: &str) -> Result<PathBuf, Error> {
    match scope {
        Scope::User => user(),
        Scope::Project(dir) => Ok(dir.join(project)),
    }
}

/// Cursor, IDE and `agent` CLI: `~/.cursor/mcp.json` on every system, `mcpServers` (§6 "Locations"). Its CLI has no
/// `add` (§6 "CLI and deeplink"). Writing `type: "stdio"` is safe and its docs list it as required (§6 "Entry").
struct Cursor;

impl FileAdapter for Cursor {
    fn id(&self) -> &'static str {
        "cursor"
    }

    fn name(&self) -> &'static str {
        "Cursor"
    }

    fn detect(&self, env: &Env) -> bool {
        env.on_path(&["cursor", "cursor-agent"]) || env.home().is_ok_and(|h| h.join(".cursor").is_dir())
    }

    fn location(&self, env: &Env, scope: &Scope) -> Result<Location, Error> {
        let path = user_or_project(scope, || Ok(env.home()?.join(".cursor/mcp.json")), ".cursor/mcp.json")?;
        Ok(Location { path, dialect: Dialect::Jsonc, keys: &["mcpServers"], render: stdio })
    }

    fn restart(&self) -> Option<&'static str> {
        Some("restart Cursor")
    }
}

/// VS Code with Copilot: the default profile's `User/mcp.json`, JSONC, `servers`, `type: "stdio"` (§7 "Locations",
/// "Entry"). VS Code watches the file, so no restart (§7 "Quirks").
struct VsCode;

impl FileAdapter for VsCode {
    fn id(&self) -> &'static str {
        "vscode"
    }

    fn name(&self) -> &'static str {
        "VS Code"
    }

    fn detect(&self, env: &Env) -> bool {
        env.on_path(&["code"]) || env.app_data().is_ok_and(|d| d.join("Code/User").is_dir())
    }

    fn location(&self, env: &Env, scope: &Scope) -> Result<Location, Error> {
        let path = user_or_project(scope, || Ok(env.app_data()?.join("Code/User/mcp.json")), ".vscode/mcp.json")?;
        Ok(Location { path, dialect: Dialect::Jsonc, keys: &["servers"], render: stdio })
    }
}

/// Claude Desktop: `claude_desktop_config.json` in its `userData`, strict JSON, `mcpServers` with no `type` (§5
/// "Locations", "Entry"). It writes its whole in-memory copy back, so the file is changed only while it isn't
/// running (§5 "Quirks").
struct ClaudeDesktop;

/// The MSIX install's redirected AppData folder, which it reads instead of `%APPDATA%\Claude` (§5 "Locations").
const CLAUDE_MSIX: &str = "Packages/Claude_pzs8sxrjxfjjc/LocalCache/Roaming/Claude";

impl ClaudeDesktop {
    fn dir(env: &Env) -> Result<PathBuf, Error> {
        let msix = env.path_var("LOCALAPPDATA").map(|d| d.join(CLAUDE_MSIX)).filter(|d| d.is_dir());
        match (env.os(), msix) {
            (Os::Windows, Some(msix)) => Ok(msix),
            _ => Ok(env.app_data()?.join("Claude")),
        }
    }
}

impl FileAdapter for ClaudeDesktop {
    fn id(&self) -> &'static str {
        "claude-desktop"
    }

    fn name(&self) -> &'static str {
        "Claude Desktop"
    }

    /// §5 "Detection": the app bundle on macOS, the `.deb`'s binary on Linux, or the config directory.
    fn detect(&self, env: &Env) -> bool {
        let app = match env.os() {
            Os::MacOs => env.system("/Applications/Claude.app").is_dir(),
            Os::Linux => env.system("/usr/bin/claude-desktop").is_file() || env.on_path(&["claude-desktop"]),
            Os::Windows => false,
        };
        app || Self::dir(env).is_ok_and(|d| d.is_dir())
    }

    fn location(&self, env: &Env, scope: &Scope) -> Result<Location, Error> {
        let Scope::User = scope else { return Err(Error::NoProjectScope("Claude Desktop")) };
        let path = Self::dir(env)?.join("claude_desktop_config.json");
        Ok(Location { path, dialect: Dialect::Json, keys: &["mcpServers"], render: plain })
    }

    /// The process names are unconfirmed: the macOS bundle's executable is `Claude`, the Linux package's launcher
    /// `claude-desktop`.
    fn blocker(&self, env: &Env) -> Option<Error> {
        let names: &[&str] = match env.os() {
            Os::MacOs => &["Claude"],
            Os::Linux => &["claude-desktop", "Claude"],
            Os::Windows => &["Claude.exe"],
        };
        env.probe().running(names).then_some(Error::ClaudeDesktopRunning)
    }

    fn restart(&self) -> Option<&'static str> {
        Some("quit and restart Claude Desktop")
    }
}

/// Antigravity: `~/.gemini/config/mcp_config.json`, `mcpServers`, no `type`; workspace `.agents/mcp_config.json`.
/// Its CLI's syntax is unconfirmed ("Google Antigravity").
struct Antigravity;

impl FileAdapter for Antigravity {
    fn id(&self) -> &'static str {
        "antigravity"
    }

    fn name(&self) -> &'static str {
        "Antigravity"
    }

    /// Not `~/.gemini` alone, which is Gemini CLI's.
    fn detect(&self, env: &Env) -> bool {
        env.on_path(&["agy"]) || env.home().is_ok_and(|h| h.join(".gemini/config").is_dir())
    }

    fn location(&self, env: &Env, scope: &Scope) -> Result<Location, Error> {
        let path = user_or_project(scope, || Ok(env.home()?.join(".gemini/config/mcp_config.json")), ".agents/mcp_config.json")?;
        Ok(Location { path, dialect: Dialect::Jsonc, keys: &["mcpServers"], render: plain })
    }

    fn restart(&self) -> Option<&'static str> {
        Some("refresh Antigravity's MCP servers (`/mcp` in the CLI, Refresh in the IDE)")
    }
}

/// Devin Desktop and Devin CLI, in file form: `$XDG_CONFIG_HOME/devin/mcp_config.json` (`%APPDATA%\devin\` on
/// Windows), `mcpServers`, no `type`; project `.devin/mcp_config.json`, the file `devin mcp add -s project` writes
/// (§8 "Locations", "Entry"). The old `~/.codeium/windsurf/` file is only imported now, and never written.
pub(crate) struct DevinFile;

impl DevinFile {
    fn dir(env: &Env) -> Result<PathBuf, Error> {
        match env.os() {
            Os::Windows => env.path_var("APPDATA").map(|d| d.join("devin")).ok_or(Error::NoHome),
            Os::Linux | Os::MacOs => Ok(env.xdg_config()?.join("devin")),
        }
    }
}

impl FileAdapter for DevinFile {
    fn id(&self) -> &'static str {
        "devin"
    }

    fn name(&self) -> &'static str {
        "Devin"
    }

    fn detect(&self, env: &Env) -> bool {
        env.on_path(&["devin", "devin-desktop"]) || Self::dir(env).is_ok_and(|d| d.is_dir())
    }

    fn location(&self, env: &Env, scope: &Scope) -> Result<Location, Error> {
        let path = user_or_project(scope, || Ok(Self::dir(env)?.join("mcp_config.json")), ".devin/mcp_config.json")?;
        Ok(Location { path, dialect: Dialect::Jsonc, keys: &["mcpServers"], render: plain })
    }

    fn restart(&self) -> Option<&'static str> {
        Some("restart Devin")
    }
}

/// Zed: `settings.json` in its config directory, JSONC, `context_servers`, flat `{command, args}` with no `type`
/// (§9 "Locations", "Entry"). Zed reloads its settings live (§9 "Quirks").
struct Zed;

impl Zed {
    /// `$FLATPAK_XDG_CONFIG_HOME/zed`, else `$XDG_CONFIG_HOME/zed`, on Linux; `~/.config/zed` on macOS too;
    /// `%APPDATA%\Zed` on Windows (§9 "Locations").
    fn dir(env: &Env) -> Result<PathBuf, Error> {
        match env.os() {
            Os::Linux => env.path_var("FLATPAK_XDG_CONFIG_HOME").map_or_else(|| env.xdg_config(), Ok).map(|d| d.join("zed")),
            Os::MacOs => Ok(env.home()?.join(".config/zed")),
            Os::Windows => env.path_var("APPDATA").map(|d| d.join("Zed")).ok_or(Error::NoHome),
        }
    }
}

impl FileAdapter for Zed {
    fn id(&self) -> &'static str {
        "zed"
    }

    fn name(&self) -> &'static str {
        "Zed"
    }

    fn detect(&self, env: &Env) -> bool {
        let app = env.os() == Os::MacOs && env.system("/Applications/Zed.app").is_dir();
        app || env.on_path(&["zed", "zeditor"]) || Self::dir(env).is_ok_and(|d| d.is_dir())
    }

    fn location(&self, env: &Env, scope: &Scope) -> Result<Location, Error> {
        let path = user_or_project(scope, || Ok(Self::dir(env)?.join("settings.json")), ".zed/settings.json")?;
        Ok(Location { path, dialect: Dialect::Jsonc, keys: &["context_servers"], render: plain })
    }
}

/// opencode: `$XDG_CONFIG_HOME/opencode/` on every system, JSONC, `type: "local"` and one `command` array. V1 keeps
/// servers in `mcp`, V2 in `mcp.servers`, so the major version decides (§3 "V1 and V2", "Entry"). Neither CLI
/// removes a server, and V2's `add` writes the project file by default (§3 "CLI"), so the file is edited.
#[derive(Default)]
struct Opencode {
    major: OnceCell<Result<u64, String>>,
}

impl Opencode {
    /// The installed major version, from `opencode --version` (`1.19.4`, `opencode v2.0.28`), or 2 when only V2's
    /// `opencode2` shim is found (§3 "Detection").
    fn major(&self, env: &Env) -> Result<u64, Error> {
        let major = self.major.get_or_init(|| {
            let installed = || env.home().ok().map(|h| h.join(".opencode/bin/opencode")).filter(|p| p.is_file());
            let binary = env.which("opencode").or_else(installed);
            match binary {
                Some(binary) => env.probe().output(&binary, &["--version"]).as_deref().and_then(parse_major).ok_or_else(|| {
                    format!("`{} --version` printed no version", binary.display())
                }),
                None if env.on_path(&["opencode2"]) => Ok(2),
                None => Err("opencode is not on PATH".to_owned()),
            }
        });
        major.clone().map_err(Error::OpencodeVersion)
    }

    fn dir(env: &Env) -> Result<PathBuf, Error> {
        match env.os() {
            Os::Windows => Ok(env.home()?.join(".config/opencode")),
            Os::Linux | Os::MacOs => Ok(env.xdg_config()?.join("opencode")),
        }
    }
}

/// The major version in a version line: the first word that, without a leading `v`, starts with digits and a dot.
fn parse_major(text: &str) -> Option<u64> {
    text.split_whitespace().find_map(|word| {
        let (major, _) = word.trim_start_matches('v').split_once('.')?;
        major.parse().ok()
    })
}

/// The file to edit in an opencode config directory. V1 loads `config.json`, `opencode.json` and `opencode.jsonc`
/// in that order (`v1.19.5:packages/opencode/src/config/config.ts:140-148`): edit the last that already has `mcp`
/// (§3 "Quirks"); else the one `opencode mcp add` would, `opencode.json` if present, else `opencode.jsonc`, else a new
/// `opencode.json` (§3 "CLI").
fn opencode_file(dir: &Path) -> PathBuf {
    let has_mcp = |path: &PathBuf| {
        let text = std::fs::read_to_string(path).unwrap_or_default();
        crate::json::Document::parse(&text, Dialect::Jsonc).is_ok_and(|d| d.has_key("mcp"))
    };
    let candidates = ["config.json", "opencode.json", "opencode.jsonc"].map(|name| dir.join(name));
    candidates
        .iter()
        .rev()
        .find(|p| has_mcp(p))
        .or_else(|| candidates[1..].iter().find(|p| p.exists()))
        .unwrap_or(&candidates[1])
        .clone()
}

fn opencode_v1(entry: &Entry) -> Value {
    json!({ "type": "local", "command": entry.command_line(), "enabled": true })
}

fn opencode_v2(entry: &Entry) -> Value {
    json!({ "type": "local", "command": entry.command_line() })
}

impl FileAdapter for Opencode {
    fn id(&self) -> &'static str {
        "opencode"
    }

    fn name(&self) -> &'static str {
        "opencode"
    }

    fn detect(&self, env: &Env) -> bool {
        env.on_path(&["opencode", "opencode2"])
            || env.home().is_ok_and(|h| h.join(".opencode/bin/opencode").is_file())
            || Self::dir(env).is_ok_and(|d| d.is_dir())
    }

    fn location(&self, env: &Env, scope: &Scope) -> Result<Location, Error> {
        let dir = match scope {
            Scope::User => Self::dir(env)?,
            Scope::Project(dir) => dir.clone(),
        };
        let path = opencode_file(&dir);
        Ok(match self.major(env)? {
            0 | 1 => Location { path, dialect: Dialect::Jsonc, keys: &["mcp"], render: opencode_v1 },
            _ => Location { path, dialect: Dialect::Jsonc, keys: &["mcp", "servers"], render: opencode_v2 },
        })
    }

    fn restart(&self) -> Option<&'static str> {
        Some("restart opencode")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opencode_versions_parse() {
        assert_eq!(parse_major("1.19.4"), Some(1));
        assert_eq!(parse_major("opencode v2.0.28"), Some(2));
        assert_eq!(parse_major("opencode"), None);
    }

    #[test]
    fn opencode_edits_the_file_that_has_mcp() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(opencode_file(dir.path()), dir.path().join("opencode.json"));
        std::fs::write(dir.path().join("opencode.jsonc"), "{ \"theme\": \"x\" }").unwrap();
        assert_eq!(opencode_file(dir.path()), dir.path().join("opencode.jsonc"));
        std::fs::write(dir.path().join("opencode.json"), "{}").unwrap();
        assert_eq!(opencode_file(dir.path()), dir.path().join("opencode.json"));
        std::fs::write(dir.path().join("config.json"), "{ \"mcp\": {} }").unwrap();
        assert_eq!(opencode_file(dir.path()), dir.path().join("config.json"));
    }
}
