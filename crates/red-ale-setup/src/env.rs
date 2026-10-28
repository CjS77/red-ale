//! What `setup` reads of the machine: environment variables, the operating system, the file system root and running
//! processes. Tests build an [`Env`] over a temporary directory, so they never see the real home.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::Error;

/// The operating systems whose Harness paths `setup` knows. Windows builds are deferred (`design/milestone5.md`,
/// "Out"), but its paths are kept for then.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Os {
    /// Linux and other Unixes that follow the XDG layout.
    Linux,
    /// macOS.
    MacOs,
    /// Windows.
    Windows,
}

impl Os {
    /// The system this binary was built for.
    pub fn current() -> Self {
        if cfg!(target_os = "macos") {
            Self::MacOs
        } else if cfg!(windows) {
            Self::Windows
        } else {
            Self::Linux
        }
    }
}

/// What `setup` asks about other programs.
pub trait Probe {
    /// Whether a process with one of these names is running.
    fn running(&self, names: &[&str]) -> bool;
    /// What `program` prints on standard output when run with `args`, if it runs and succeeds.
    fn output(&self, program: &Path, args: &[&str]) -> Option<String>;
}

/// The real machine's processes.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemProbe;

impl Probe for SystemProbe {
    fn running(&self, names: &[&str]) -> bool {
        if cfg!(target_os = "linux") {
            running_in_proc(names)
        } else {
            names.iter().any(|name| {
                let status = Command::new("pgrep").args(["-x", name]).stdout(Stdio::null()).stderr(Stdio::null()).status();
                status.is_ok_and(|s| s.success())
            })
        }
    }

    fn output(&self, program: &Path, args: &[&str]) -> Option<String> {
        let output = Command::new(program).args(args).stdin(Stdio::null()).stderr(Stdio::null()).output().ok()?;
        output.status.success().then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
    }
}

/// Whether any process's name (`/proc/<pid>/comm`) is one of `names`.
fn running_in_proc(names: &[&str]) -> bool {
    std::fs::read_dir("/proc")
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_name().to_str().is_some_and(|n| n.bytes().all(|b| b.is_ascii_digit())))
        .filter_map(|e| std::fs::read_to_string(e.path().join("comm")).ok())
        .any(|comm| names.contains(&comm.trim_end()))
}

/// The machine as `setup` sees it.
pub struct Env {
    vars: BTreeMap<String, OsString>,
    os: Os,
    root: PathBuf,
    probe: Box<dyn Probe>,
}

impl std::fmt::Debug for Env {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Env").field("vars", &self.vars).field("os", &self.os).field("root", &self.root).finish_non_exhaustive()
    }
}

impl Env {
    /// This process's environment, on this machine.
    pub fn from_process() -> Self {
        let vars = std::env::vars_os().filter_map(|(k, v)| k.into_string().ok().map(|k| (k, v)));
        Self::new(vars, Os::current(), PathBuf::from("/"), Box::new(SystemProbe))
    }

    /// An environment of these variables on `os`, with system-wide paths such as `/Applications` under `root` and
    /// other programs answered by `probe`.
    pub fn new(vars: impl IntoIterator<Item = (String, OsString)>, os: Os, root: PathBuf, probe: Box<dyn Probe>) -> Self {
        Self {
            vars: vars.into_iter().collect(),
            os,
            root,
            probe,
        }
    }

    /// The operating system.
    pub fn os(&self) -> Os {
        self.os
    }

    pub(crate) fn probe(&self) -> &dyn Probe {
        self.probe.as_ref()
    }

    /// A variable holding an absolute path, ignored when empty or relative as the XDG spec says.
    pub(crate) fn path_var(&self, name: &str) -> Option<PathBuf> {
        self.vars.get(name).map(PathBuf::from).filter(|p| p.is_absolute())
    }

    /// The home directory: `HOME`, or `USERPROFILE` on Windows.
    pub(crate) fn home(&self) -> Result<PathBuf, Error> {
        let name = if self.os == Os::Windows { "USERPROFILE" } else { "HOME" };
        self.path_var(name).ok_or(Error::NoHome)
    }

    /// `$XDG_CONFIG_HOME`, else `~/.config`.
    pub(crate) fn xdg_config(&self) -> Result<PathBuf, Error> {
        self.path_var("XDG_CONFIG_HOME").map_or_else(|| Ok(self.home()?.join(".config")), Ok)
    }

    /// Where Electron apps keep their data (`app.getPath("appData")`): the XDG config directory on Linux,
    /// `~/Library/Application Support` on macOS and `%APPDATA%` on Windows.
    pub(crate) fn app_data(&self) -> Result<PathBuf, Error> {
        match self.os {
            Os::Linux => self.xdg_config(),
            Os::MacOs => Ok(self.home()?.join("Library/Application Support")),
            Os::Windows => self.path_var("APPDATA").ok_or(Error::NoHome),
        }
    }

    /// The first `name` on `PATH`.
    pub(crate) fn which(&self, name: &str) -> Option<PathBuf> {
        self.which_all(name).next()
    }

    /// Every `name` on `PATH`, in order.
    pub(crate) fn which_all(&self, name: &str) -> impl Iterator<Item = PathBuf> {
        let file = if self.os == Os::Windows { format!("{name}.exe") } else { name.to_owned() };
        let dirs: Vec<PathBuf> = self.vars.get("PATH").map(|p| std::env::split_paths(p).collect()).unwrap_or_default();
        dirs.into_iter().map(move |dir| dir.join(&file)).filter(|p| p.is_file())
    }

    /// Run a Harness CLI with this environment's variables only, so it changes the config this environment names.
    /// Its standard error, or how it exited, when it fails.
    pub(crate) fn run(&self, program: &Path, args: &[String], dir: Option<&Path>) -> Result<(), String> {
        let mut command = Command::new(program);
        command.args(args).env_clear().envs(&self.vars).stdin(Stdio::null()).stdout(Stdio::null());
        if let Some(dir) = dir {
            command.current_dir(dir);
        }
        let output = command.output().map_err(|e| e.to_string())?;
        if output.status.success() {
            return Ok(());
        }
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        Err(if stderr.is_empty() { format!("it exited with {}", output.status) } else { stderr })
    }

    /// Whether any of `names` is on `PATH`.
    pub(crate) fn on_path(&self, names: &[&str]) -> bool {
        names.iter().any(|name| self.which(name).is_some())
    }

    /// A system-wide path such as `/Applications/Zed.app`, under this environment's root.
    pub(crate) fn system(&self, path: &str) -> PathBuf {
        self.root.join(path.trim_start_matches('/'))
    }
}
