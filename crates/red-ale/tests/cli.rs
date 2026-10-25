//! The command line drives the same methods as the server: exec in process, call over HTTP.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Command, Output, Stdio};

use serde_json::Value;

const BIN: &str = env!("CARGO_BIN_EXE_red-ale");

fn fixture(name: &str) -> String {
    format!("{}/../../fixtures/{name}", env!("CARGO_MANIFEST_DIR"))
}

fn run(args: &[&str]) -> Output {
    Command::new(BIN).args(args).output().unwrap()
}

fn json(bytes: &[u8]) -> Value {
    serde_json::from_slice(bytes).unwrap_or_else(|e| panic!("not JSON ({e}): {}", String::from_utf8_lossy(bytes)))
}

fn copy(dir: &Path, name: &str) -> String {
    let dest = dir.join("book.gnucash");
    std::fs::copy(fixture(name), &dest).unwrap();
    dest.display().to_string()
}

#[test]
fn exec_creates_opens_and_reports() {
    let dir = tempfile::tempdir().unwrap();
    let book = dir.path().join("new.gnucash").display().to_string();
    let out = run(&["exec", "--create", "--book", &book, "book.info"]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(json(&out.stdout)["counts"]["accounts"], 1);
    let out = run(&["exec", "--book", &book, "--read-only", "book.info"]);
    assert_eq!(json(&out.stdout)["readOnlyReasons"][0]["code"], "requested");
    assert!(!dir.path().join("new.gnucash.LCK").exists(), "the lock is released on exit");
}

#[test]
fn errors_exit_with_code_2_and_save_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let book = copy(dir.path(), "synthetic/rich.gnucash");
    let before = std::fs::read(&book).unwrap();
    let out = run(&["exec", "--book", &book, "no.such.method"]);
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(json(&out.stderr)["error"]["code"], -32601);
    assert_eq!(std::fs::read(&book).unwrap(), before);
    let sql = copy(dir.path(), "piecash/simple_sample.sqlite.gnucash");
    let out = run(&["exec", "--book", &sql, "book.info"]);
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(json(&out.stderr)["error"]["code"], 1004);
    let out = run(&["exec", "--book", &book, "book.open", "novalue"]);
    assert_eq!(out.status.code(), Some(1), "bad arguments are usage errors");
}

#[test]
fn scripts_run_in_order_and_stop_at_the_first_failure() {
    let dir = tempfile::tempdir().unwrap();
    let book = copy(dir.path(), "synthetic/rich.gnucash");
    let mut child = Command::new(BIN)
        .args(["exec", "--book", &book, "--script", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(b"# comment\nbook.info\n{\"method\": \"book.info\", \"params\": {}}\n").unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(json(&out.stdout).as_array().unwrap().len(), 2);
}

#[test]
fn methods_and_schema_list_the_api() {
    let out = run(&["methods"]);
    assert!(String::from_utf8_lossy(&out.stdout).lines().any(|l| l.starts_with("book.open")));
    let schema = json(&run(&["schema"]).stdout);
    assert!(schema["methods"].as_array().unwrap().iter().any(|m| m["name"] == "book.replayJournal"));
}

#[test]
fn serve_and_call_over_http() {
    let dir = tempfile::tempdir().unwrap();
    let book = copy(dir.path(), "synthetic/rich.gnucash");
    let mut server = Command::new(BIN).args(["serve", "--listen", "127.0.0.1:0", "--book", &book]).stderr(Stdio::piped()).spawn().unwrap();
    let mut lines = BufReader::new(server.stderr.take().unwrap()).lines();
    let url = lines
        .by_ref()
        .map_while(Result::ok)
        .find_map(|l| l.split("serving JSON-RPC on ").nth(1).map(str::to_owned))
        .expect("the server reports its address");
    let out = run(&["call", "--url", &url, "book.info"]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(json(&out.stdout)["readOnly"], false);
    let out = run(&["call", "--url", &url, "book.open", &format!("path={book}")]);
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(json(&out.stderr)["error"]["code"], 1001);
    let lock = format!("{book}.LCK");
    assert!(Path::new(&lock).exists());
    Command::new("kill").args(["-TERM", &server.id().to_string()]).status().unwrap();
    assert!(server.wait().unwrap().success());
    assert!(!Path::new(&lock).exists(), "SIGTERM closes the book and releases the lock");
}
