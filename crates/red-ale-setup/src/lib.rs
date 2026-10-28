//! `red-ale setup`: registering red-ale's MCP server with the agent Harnesses installed on a machine
//! (`design/setup.md`).
//!
//! Each Harness is one adapter behind [`Harness`], which translates the red-ale entry ([`Entry`]) into that
//! Harness's format, so no external format leaks into the rest of red-ale. The crate knows nothing about Books and
//! is used only by the binary. What it reads of the machine (home directories, `PATH`, running processes) comes in
//! through an [`Env`], so tests never look at the real one.
//!
//! - [`FIRST_RELEASE`]: every Harness of the first release, by id; [`harness`] gives the adapter for an id.
//! - [`Harness::plan`] and [`Harness::plan_removal`] make a Setup plan for one Harness; [`Harness::apply`] carries
//!   it out under the rules of `design/setup.md`, "Editing a Harness config".

#![forbid(unsafe_code)]

mod adapters;
mod env;
mod file;
mod json;

use std::path::{Path, PathBuf};

use serde_json::Value;

pub use env::{Env, Os, Probe, SystemProbe};

/// The name of the red-ale entry in every Harness config (`design/setup.md`, "One name everywhere").
pub const NAME: &str = "red-ale";

/// Every Harness of the first release, `(id, name)`, in the order of `design/setup.md`, "First release".
pub const FIRST_RELEASE: [(&str, &str); 11] = [
    ("claude-code", "Claude Code"),
    ("codex", "Codex"),
    ("opencode", "opencode"),
    ("claude-desktop", "Claude Desktop"),
    ("cursor", "Cursor"),
    ("vscode", "VS Code"),
    ("copilot-cli", "GitHub Copilot CLI"),
    ("gemini", "Gemini CLI"),
    ("antigravity", "Antigravity"),
    ("devin", "Devin"),
    ("zed", "Zed"),
];

/// The adapter for a Harness id, or `None` for one of [`FIRST_RELEASE`] that is set up through its own CLI, which
/// this version does not do yet.
pub fn harness(id: &str) -> Option<Box<dyn Harness>> {
    adapters::by_id(id)
}

/// The red-ale entry: the server every Harness gets, before translation into its format (`design/setup.md`, "The
/// red-ale entry").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// The absolute path of the red-ale binary.
    pub command: String,
    /// `["mcp"]`, or `["mcp", "--config", <file>]`.
    pub args: Vec<String>,
}

impl Entry {
    /// The entry for the binary at `command` (absolute, as `std::env::current_exe` canonicalised gives it), passing
    /// `config` to `red-ale mcp` when given.
    pub fn new(command: &Path, config: Option<&Path>) -> Result<Self, Error> {
        let text = |path: &Path| path.to_str().map(str::to_owned).ok_or_else(|| Error::NotUtf8(path.to_path_buf()));
        let mut args = vec!["mcp".to_owned()];
        if let Some(config) = config {
            args.extend(["--config".to_owned(), text(config)?]);
        }
        Ok(Self { command: text(command)?, args })
    }

    /// The command and its arguments as one list, as opencode keeps them.
    fn command_line(&self) -> Vec<String> {
        std::iter::once(self.command.clone()).chain(self.args.iter().cloned()).collect()
    }
}

/// Where the red-ale entry goes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Scope {
    /// The Harness's user config: the default.
    User,
    /// The project config in this directory (`--project`), for the Harnesses that have one.
    Project(PathBuf),
}

/// What a Setup plan is for.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Change {
    /// Make the red-ale entry this one.
    Set(Entry),
    /// Remove the red-ale entry (`--remove`).
    Remove,
}

/// What `setup` will do to one Harness config.
#[derive(Debug)]
pub enum Action {
    /// There is no red-ale entry; add one.
    Add,
    /// There is a red-ale entry that differs; replace it whole. `diff` is a unified diff of the entry.
    Update {
        /// The old entry against the new one.
        diff: String,
    },
    /// Remove the red-ale entry.
    Remove,
    /// Nothing to do: the entry is already as wanted.
    Nothing,
    /// The Harness can't be set up, for this reason. Nothing is written.
    Skip(Error),
}

impl Action {
    /// Whether applying this writes anything.
    pub fn changes(&self) -> bool {
        matches!(self, Self::Add | Self::Update { .. } | Self::Remove)
    }
}

/// One Harness's part of the Setup plan.
#[derive(Debug)]
pub struct Plan {
    /// What will be done.
    pub action: Action,
    /// The Harness config, when it could be found.
    pub config: Option<PathBuf>,
    scope: Scope,
    change: Change,
    edit: Option<file::Edit>,
}

impl Plan {
    /// A plan that does nothing but say why.
    fn skip(scope: &Scope, change: &Change, config: Option<PathBuf>, error: Error) -> Self {
        Self {
            action: Action::Skip(error),
            config,
            scope: scope.clone(),
            change: change.clone(),
            edit: None,
        }
    }

    /// The exact change to the Harness config as a unified diff, for `--dry-run`; `None` when nothing changes.
    pub fn file_diff(&self) -> Option<String> {
        self.edit.as_ref().filter(|_| self.action.changes()).map(file::Edit::diff)
    }
}

/// What applying a plan did.
#[derive(Debug)]
pub struct Applied {
    /// What was done. Not the planned action when the Harness changed its config meanwhile and the plan was made again.
    pub action: Action,
    /// The Harness config written.
    pub config: PathBuf,
    /// The copy made of the Harness config before it was changed, if it existed.
    pub backup: Option<PathBuf>,
}

/// One Harness: how to find it, read its red-ale entry, plan a change and apply it.
pub trait Harness {
    /// The id used on the command line (`--only cursor`).
    fn id(&self) -> &'static str;
    /// The Harness's name, for people.
    fn name(&self) -> &'static str;
    /// Whether the Harness is installed: its binary on `PATH`, its app, or its config directory.
    fn detect(&self, env: &Env) -> bool;
    /// The Harness config for `scope`.
    fn config(&self, env: &Env, scope: &Scope) -> Result<PathBuf, Error>;
    /// The red-ale entry as the Harness config holds it now, if there is one.
    fn read_entry(&self, env: &Env, scope: &Scope) -> Result<Option<Value>, Error>;
    /// The plan to make the red-ale entry `entry`.
    fn plan(&self, env: &Env, scope: &Scope, entry: &Entry) -> Plan;
    /// The plan to remove the red-ale entry.
    fn plan_removal(&self, env: &Env, scope: &Scope) -> Plan;
    /// Carry out a plan made by this Harness. A plan that would change nothing writes nothing.
    fn apply(&self, env: &Env, plan: Plan) -> Result<Applied, Error>;
    /// What the user must do before the Harness sees the change, if anything ("restart Cursor").
    fn restart(&self) -> Option<&'static str>;
}

/// Why a Harness can't be set up.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// There is no home directory to look in.
    #[error("no home directory (HOME is not set)")]
    NoHome,
    /// A path that JSON can't hold.
    #[error("{} is not valid UTF-8", .0.display())]
    NotUtf8(PathBuf),
    /// The Harness config exists but can't be read.
    #[error("can't read {}: {source}", path.display())]
    Read {
        /// The Harness config.
        path: PathBuf,
        /// Why.
        source: std::io::Error,
    },
    /// The Harness config doesn't parse; it is left as it is.
    #[error("{} is not valid {format}, so it was left alone: {message}", path.display())]
    Invalid {
        /// The Harness config.
        path: PathBuf,
        /// "JSON" or "JSON with comments".
        format: &'static str,
        /// The parser's message.
        message: String,
    },
    /// A key on the way to the red-ale entry holds something other than an object.
    #[error("{} has `{key}` as something other than an object, so it was left alone", path.display())]
    NotAnObject {
        /// The Harness config.
        path: PathBuf,
        /// The keys from the top, joined with `.`.
        key: String,
    },
    /// The Harness config is a symbolic link to nothing.
    #[error("{} is a symbolic link to a file that doesn't exist", .0.display())]
    DanglingLink(PathBuf),
    /// The Harness config can't be written.
    #[error("can't write {}: {source}", path.display())]
    Write {
        /// The file being written.
        path: PathBuf,
        /// Why.
        source: std::io::Error,
    },
    /// The Harness kept changing its config while `setup` was writing it.
    #[error("{} kept changing while setup was writing it; run `red-ale setup` again", .0.display())]
    KeptChanging(PathBuf),
    /// Claude Desktop is running, and would write back its own copy of the file over the change.
    #[error("quit Claude Desktop and run `red-ale setup` again")]
    ClaudeDesktopRunning,
    /// The Harness has no project config.
    #[error("{0} has no project config")]
    NoProjectScope(&'static str),
    /// opencode's major version, which decides its layout, is unknown.
    #[error("can't tell which major version of opencode is installed: {0}")]
    OpencodeVersion(String),
}
