//! `red-ale help` prints the Manual in the terminal, and `red-ale methods` names each method's Page.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::process::{Command, Output};

const BIN: &str = env!("CARGO_BIN_EXE_red-ale");

/// The binary, with every home directory a Configuration could live in pointed at one temporary directory for this
/// test binary, so a test never reads or writes the user's real Configuration.
fn run(args: &[&str]) -> Output {
    static HOME: std::sync::OnceLock<tempfile::TempDir> = std::sync::OnceLock::new();
    let home = HOME.get_or_init(|| tempfile::tempdir().unwrap()).path();
    Command::new(BIN)
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .env("APPDATA", home.join("AppData"))
        .args(args)
        .output()
        .unwrap()
}

/// Standard output of a run that must succeed.
fn stdout(args: &[&str]) -> String {
    let out = run(args);
    assert!(out.status.success(), "{args:?}: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8(out.stdout).unwrap()
}

#[test]
fn help_prints_the_start_page_as_text() {
    let start = stdout(&["help"]);
    assert!(start.starts_with("red-ale Manual\n=============="), "{start}");
    assert!(start.contains("book.open"), "{start}");
    assert!(!start.contains("```"), "text, not Markdown: {start}");
    assert!(start.trim_end().ends_with("(legal/gfdl)."), "the licence notice ends it: {start}");
}

#[test]
fn help_prints_a_page_with_its_examples_as_exec_lines() {
    let post = stdout(&["help", "transaction.post"]);
    assert!(post.starts_with("transaction.post\n"), "{post}");
    assert!(post.contains("red-ale exec --book book.gnucash"), "Examples for the CLI: {post}");
    assert!(!post.contains("\"jsonrpc\""), "{post}");
    assert_eq!(stdout(&["help", "method/transaction.post"]), post, "by Page ID or by method name");
}

#[test]
fn help_finds_errors_and_reports_by_name() {
    assert!(stdout(&["help", "unbalanced"]).starts_with("unbalanced\n"));
    assert!(stdout(&["help", "error/unbalanced"]).starts_with("unbalanced\n"));
    assert!(stdout(&["help", "Balance Sheet"]).starts_with("Balance Sheet\n"));
}

#[test]
fn help_search_lists_matching_pages() {
    let found = stdout(&["help", "--search", "open", "a", "book"]);
    let first = found.lines().next().unwrap();
    assert_eq!(first, "method/book.open  book.open", "{found}");
    assert!(
        found.lines().nth(1).unwrap().starts_with("    Open "),
        "the summary, indented: {found}"
    );
    assert_eq!(
        stdout(&["help", "--search", "open a book"]),
        found,
        "words as one argument or several"
    );

    let none = run(&["help", "--search", "zzqqxx"]);
    assert_eq!(none.status.code(), Some(1));
    assert!(none.stdout.is_empty());
}

#[test]
fn an_unknown_page_fails_with_the_closest_pages() {
    let out = run(&["help", "transaction-post"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(out.stdout.is_empty());
    let err = String::from_utf8(out.stderr).unwrap();
    assert!(
        err.starts_with("red-ale: no Page of the Manual is named \"transaction-post\""),
        "{err}"
    );
    assert!(err.contains("  method/transaction.post  "), "the closest Pages: {err}");
    assert!(err.contains("red-ale help --search"), "{err}");
}

#[test]
fn help_with_a_command_prints_its_options() {
    let exec = stdout(&["help", "exec"]);
    assert!(exec.contains("Usage: red-ale exec"), "{exec}");
    assert!(exec.contains("--script"), "{exec}");
}

#[test]
fn methods_lists_each_methods_page_id() {
    let methods = stdout(&["methods"]);
    let line = methods.lines().find(|l| l.starts_with("transaction.post ")).unwrap();
    let columns: Vec<&str> = line.split_whitespace().take(3).collect();
    assert_eq!(columns, ["transaction.post", "write", "method/transaction.post"], "{line}");
    assert!(methods.lines().all(|l| l.contains(" method/")), "{methods}");
}
