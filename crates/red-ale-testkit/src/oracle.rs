//! The GnuCash 5.17 oracle: GnuCash's library and Python bindings, run as a subprocess.
//!
//! Set `RED_ALE_ORACLE_PYTHONPATH` to the bindings' `site-packages`; the default is
//! `$HOME/.local/opt/gnucash-5.17/lib/python3.12/site-packages`. The oracle must report version 5.17, or every
//! call fails: a system GnuCash of another version is never the oracle.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

/// The pinned GnuCash release.
pub const PINNED_VERSION: &str = "5.17";

/// The oracle's directory of scripts.
pub fn scripts_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("oracle")
}

/// The bindings' `site-packages` directory.
pub fn python_path() -> PathBuf {
    std::env::var_os("RED_ALE_ORACLE_PYTHONPATH").map_or_else(
        || PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".local/opt/gnucash-5.17/lib/python3.12/site-packages"),
        PathBuf::from,
    )
}

/// Run a Python script with the oracle's environment (`PYTHONPATH`, `TZ=UTC`). Returns standard output; fails with
/// the standard error when the script does.
pub fn run_script(script: &Path, args: &[&str]) -> Result<String, String> {
    run_script_in(script, args, Some("UTC"))
}

/// [`run_script`] with another `TZ`, or with `TZ` unset (`None`), for the time-zone tests.
pub fn run_script_in(script: &Path, args: &[&str], tz: Option<&str>) -> Result<String, String> {
    let mut command = Command::new("python3");
    command.arg(script).args(args).env("PYTHONPATH", python_path());
    match tz {
        Some(tz) => command.env("TZ", tz),
        None => command.env_remove("TZ"),
    };
    let output = command.output().map_err(|e| format!("cannot run python3: {e}"))?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !output.status.success() {
        return Err(format!("{} failed ({}):\n{stderr}", script.display(), output.status));
    }
    String::from_utf8(output.stdout).map_err(|e| format!("non-UTF-8 output: {e}"))
}

/// Run Python code given as text, with the oracle's environment.
pub fn run_code(code: &str, args: &[&str]) -> Result<String, String> {
    run_code_in(code, args, Some("UTC"))
}

/// [`run_code`] with another `TZ`, or with `TZ` unset (`None`).
pub fn run_code_in(code: &str, args: &[&str], tz: Option<&str>) -> Result<String, String> {
    let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
    let script = dir.path().join("script.py");
    std::fs::write(&script, code).map_err(|e| e.to_string())?;
    run_script_in(&script, args, tz)
}

/// The oracle's version. Errors if it cannot run.
pub fn version() -> Result<String, String> {
    run_code("from gnucash import _sw_core_utils\nprint(_sw_core_utils.gnc_version())\n", &[]).map(|s| s.trim().to_owned())
}

/// Fail unless the oracle runs and is the pinned version.
pub fn check_version() -> Result<(), String> {
    let found = version()?;
    if found == PINNED_VERSION { Ok(()) } else { Err(format!("the oracle must be GnuCash {PINNED_VERSION}, found {found}")) }
}

/// GnuCash's view of a book, as canonical JSON (see `oracle/dump.py`). The book is opened read-only.
pub fn dump(book: &Path) -> Result<Value, String> {
    check_version()?;
    let text = run_script(&scripts_dir().join("dump.py"), &[&book.display().to_string()])?;
    serde_json::from_str(&text).map_err(|e| format!("oracle output is not JSON: {e}"))
}
