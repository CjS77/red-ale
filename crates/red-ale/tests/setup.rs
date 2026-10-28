//! `red-ale setup` from the command line, in a temporary home with an empty `PATH`, so no real Harness config is
//! read or written.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;
use std::process::{Command, Output, Stdio};

const BIN: &str = env!("CARGO_BIN_EXE_red-ale");

fn setup(home: &Path, args: &[&str]) -> Output {
    Command::new(BIN)
        .arg("setup")
        .args(args)
        .env_clear()
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .env("APPDATA", home.join("AppData"))
        .env("PATH", home.join("bin"))
        .stdin(Stdio::null())
        .output()
        .unwrap()
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn cursor_installed() -> tempfile::TempDir {
    let home = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(home.path().join(".cursor")).unwrap();
    home
}

#[test]
fn dry_run_shows_the_change_and_writes_nothing() {
    let home = cursor_installed();
    let out = setup(home.path(), &["--dry-run", "--only", "cursor"]);
    assert_eq!(out.status.code(), Some(0), "{}", String::from_utf8_lossy(&out.stderr));
    let text = stdout(&out);
    let config = home.path().join(".cursor/mcp.json");
    assert!(text.contains(&format!("cursor          add      {}", config.display())), "{text}");
    assert!(text.contains("+++ ") && text.contains("+      \"args\": [\"mcp\"]"), "{text}");
    let exe = std::fs::canonicalize(BIN).unwrap();
    assert!(text.contains(&format!("\"command\": \"{}\"", exe.display())), "the entry runs this binary: {text}");
    assert!(!config.exists());
}

#[test]
fn applying_needs_yes_without_a_terminal_then_writes_once() {
    let home = cursor_installed();
    let config = home.path().join(".cursor/mcp.json");
    let refused = setup(home.path(), &["--only", "cursor"]);
    assert_eq!(refused.status.code(), Some(2));
    assert!(!config.exists());

    let out = setup(home.path(), &["--only", "cursor", "--yes"]);
    assert_eq!(out.status.code(), Some(0), "{}", stdout(&out));
    assert!(stdout(&out).contains("added"), "{}", stdout(&out));
    let written = std::fs::read_to_string(&config).unwrap();

    let again = setup(home.path(), &["--only", "cursor"]);
    assert_eq!(again.status.code(), Some(0), "a second run needs no --yes: nothing to do");
    assert!(stdout(&again).contains("nothing"), "{}", stdout(&again));
    assert_eq!(std::fs::read_to_string(&config).unwrap(), written);

    let removed = setup(home.path(), &["--remove", "--only", "cursor", "--yes"]);
    assert_eq!(removed.status.code(), Some(0));
    assert_eq!(std::fs::read_to_string(&config).unwrap(), "{}\n");
}

#[test]
fn a_named_harness_that_is_missing_fails() {
    let home = tempfile::tempdir().unwrap();
    let out = setup(home.path(), &["--only", "zed", "--yes"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(stdout(&out).contains("not installed"), "{}", stdout(&out));
    let unnamed = setup(home.path(), &["--yes"]);
    assert_eq!(unnamed.status.code(), Some(0), "harnesses that aren't installed are not failures: {}", stdout(&unnamed));
}

#[test]
fn list_shows_every_harness() {
    let home = cursor_installed();
    let out = setup(home.path(), &["--list"]);
    assert_eq!(out.status.code(), Some(0));
    let text = stdout(&out);
    let line = |id: &str| text.lines().find(|l| l.starts_with(&format!("{id} "))).unwrap_or_else(|| panic!("{id}: {text}")).to_owned();
    assert!(line("cursor").contains(&format!("yes        no             {}", home.path().join(".cursor/mcp.json").display())), "{text}");
    assert!(line("zed").starts_with("zed             no"), "{text}");
    assert!(line("claude-code").ends_with("not yet supported"), "{text}");
    assert_eq!(text.lines().count(), 12, "a header and the eleven first-release harnesses: {text}");
}

#[test]
fn an_unknown_harness_is_a_bad_argument() {
    let home = tempfile::tempdir().unwrap();
    assert_eq!(setup(home.path(), &["--only", "notepad"]).status.code(), Some(2));
}
