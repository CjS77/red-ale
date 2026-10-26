//! The command line drives the same methods as the server: exec in process, call over HTTP.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Command, Output, Stdio};

use serde_json::{Value, json};

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

#[test]
fn a_script_drives_several_areas_and_saves_once() {
    let dir = tempfile::tempdir().unwrap();
    let book = dir.path().join("new.gnucash").display().to_string();
    let script = dir.path().join("changes.txt");
    std::fs::write(
        &script,
        r#"account.create {"name": "Checking", "type": "BANK", "commodity": "USD"}
account.create {"name": "Salary", "type": "INCOME", "commodity": "USD"}
transaction.post {"datePosted": "2024-06-20", "description": "Pay", "splits": [{"account": "Checking", "value": "1000.00"}, {"account": "Salary", "value": "-1000.00"}]}
price.add {"commodity": "EUR", "currency": "USD", "value": "1.08", "date": "2024-06-03"}
customer.create {"name": "Acme", "currency": "USD"}
"#,
    )
    .unwrap();
    let out = run(&["exec", "--create", "--book", &book, "--script", &script.display().to_string()]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(json(&out.stdout).as_array().unwrap().len(), 5);
    let info = json(&run(&["exec", "--book", &book, "--read-only", "book.info"]).stdout);
    let counts = &info["counts"];
    assert_eq!((&counts["accounts"], &counts["transactions"], &counts["prices"], &counts["customers"]), (&json!(3), &json!(1), &json!(1), &json!(1)));
    let balance = json(&run(&["exec", "--book", &book, "--read-only", "account.balance", "account=Checking"]).stdout);
    assert_eq!(balance["balances"]["balance"], json!({ "num": "100000", "denom": "100" }));
}

#[test]
fn import_files_are_read_on_the_cli_side() {
    let dir = tempfile::tempdir().unwrap();
    let book = dir.path().join("new.gnucash").display().to_string();
    let out = run(&["exec", "--create", "--book", &book, "account.create", "name=Checking", "type=BANK", "commodity=USD"]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let file = dir.path().join("statement.csv");
    std::fs::write(&file, b"Date,Description,Amount\n2024-03-01,Caf\xe9,-4.50\n").unwrap();
    let layout = r#"layout:={"skipStartLines": 1, "columns": ["Date", "Description", "Amount"], "baseAccount": "Checking"}"#;
    let file = file.display().to_string();
    let args = ["exec", "--book", &book, "import.preview", "format=csvTransactions", layout, "--file", &file, "--encoding", "iso-8859-1"];
    let out = run(&args);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let proposal = json(&out.stdout);
    assert_eq!(proposal["transactions"][0]["description"], "Café");
    assert_eq!(proposal["ignoredLines"][0]["line"], 1);
    let out = run(&["exec", "--book", &book, "import.preview", "format=csvTransactions", layout, "--file", &file]);
    assert_eq!(out.status.code(), Some(2), "not UTF-8");
    assert_eq!(json(&out.stderr)["error"]["code"], 4000);
}

/// The local time zone GnuCash would read for this `TZ` (`None`: unset), and today's date in it before and after
/// `f` runs, so a midnight in between cannot fail the test.
fn in_zone<T>(tz: Option<&str>, f: impl FnOnce() -> T) -> (red_ale_core::TimeZone, [red_ale_core::Date; 2], T) {
    let tzdir = std::env::var_os("TZDIR").map(std::path::PathBuf::from);
    let zone = red_ale_service::zone::zone_from(tz, tzdir.as_deref(), &|p| std::fs::read(p).ok());
    let today = |z: &red_ale_core::TimeZone| {
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
        z.local_date(red_ale_core::Time64(i64::try_from(now).unwrap())).unwrap()
    };
    let before = today(&zone);
    let out = f();
    let after = today(&zone);
    (zone, [before, after], out)
}

fn rfc3339(t: red_ale_core::Time64) -> String {
    t.to_file_string().replacen(' ', "T", 1).replace(" +0000", "Z")
}

fn run_in(tz: Option<&str>, args: &[&str]) -> Output {
    let mut command = Command::new(BIN);
    command.args(args);
    match tz {
        Some(tz) => command.env("TZ", tz),
        None => command.env_remove("TZ"),
    };
    command.output().unwrap()
}

#[test]
fn today_and_backup_names_follow_tz_at_both_extremes_and_unset() {
    [Some("Pacific/Pago_Pago"), Some("Pacific/Kiritimati"), Some("XYZ-11"), Some(":Pacific/Kiritimati"), None].iter().for_each(|tz| {
        let dir = tempfile::tempdir().unwrap();
        let book = dir.path().join("b.gnucash").display().to_string();
        let (zone, days, out) = in_zone(*tz, || run_in(*tz, &["exec", "--create", "--book", &book, "period.dates", "start=today", "end=today"]));
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
        let dates = json(&out.stdout);
        let today = days.iter().find(|d| dates["today"] == d.to_string()).unwrap_or_else(|| panic!("TZ={tz:?}: {dates} is not {days:?}"));
        assert_eq!(dates["startTime"], rfc3339(zone.day_start(*today)), "TZ={tz:?}");
        assert_eq!(dates["endTime"], rfc3339(zone.day_end(*today)), "TZ={tz:?}");

        let (_, days, out) = in_zone(*tz, || run_in(*tz, &["exec", "--book", &book, "account.create", "name=Bank", "type=BANK", "commodity=USD"]));
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
        let names: Vec<String> = std::fs::read_dir(dir.path()).unwrap().filter_map(|e| e.unwrap().file_name().into_string().ok()).collect();
        let backup = names.iter().find(|n| n.starts_with("b.gnucash.") && n.ends_with(".gnucash")).unwrap_or_else(|| panic!("TZ={tz:?}: {names:?}"));
        let stamp = &backup["b.gnucash.".len().."b.gnucash.".len() + 8];
        let local_days = days.map(|d| d.to_string().replace('-', ""));
        assert!(local_days.iter().any(|d| d == stamp), "TZ={tz:?}: backup {backup} is not dated {local_days:?}");
    });
}

/// One user report reading the Book, in a report directory named by a Configuration in `dir`. Returns the
/// Configuration's path.
fn report_config(dir: &Path) -> String {
    let reports = dir.join("reports");
    std::fs::create_dir_all(&reports).unwrap();
    std::fs::write(
        reports.join("names.rhai"),
        r#"
fn templates() { [#{ guid: "0000000000000000000000000000b001", name: "Account Names" }] }
fn options(params, o, book) { o.number_range("Display", "Levels", "a", "How deep.", 2, 1, 10, 1); }
fn render(params, book, opts, ctx) {
    let rows = book.accounts().map(|a| #{ cells: [#{ text: a.name }] });
    #{ title: `Accounts to ${opts.get("Display", "Levels")}`,
       sections: [#{ blocks: [#{ kind: "table", columns: [#{ label: "Account", role: "label" }], rows: rows }] }] }
}
"#,
    )
    .unwrap();
    let config = dir.join("config.json");
    std::fs::write(&config, r#"{"reports": {"directory": "reports"}}"#).unwrap();
    config.display().to_string()
}

#[test]
fn exec_runs_reports_and_writes_the_export_to_a_file() {
    let dir = tempfile::tempdir().unwrap();
    let book = copy(dir.path(), "reports/balsheet-pnl.gnucash");
    let config = report_config(dir.path());
    let before = std::fs::read(&book).unwrap();
    let html = dir.path().join("out.html");
    let html_arg = html.display().to_string();
    let options = r#"options:={"Display": {"Levels": 3}}"#;
    let args = ["exec", "--book", &book, "--config", &config, "report.export", "report=Account Names", options, "--output", &html_arg];
    let out = run(&args);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let written = std::fs::read_to_string(&html).unwrap();
    assert_eq!(json(&out.stdout), json!({ "title": "Accounts to 3", "path": html_arg, "bytes": written.len() }));
    assert!(written.starts_with("<!DOCTYPE html>") && !written.contains("<script"));
    assert_eq!(std::fs::read(&book).unwrap(), before, "exec never saves a book a report only read");
    let entries = std::fs::read_dir(dir.path()).unwrap();
    let names: Vec<String> = entries.map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    assert!(!names.iter().any(|n| Path::new(n).extension().is_some_and(|x| x == "log")), "{names:?}");

    let out = run(&["exec", "--book", &book, "--read-only", "--config", &config, "report.run", "report=Account Names", options]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(json(&out.stdout)["options"]["Display"]["Levels"], 3, "nested options parse");
    let out = run(&["exec", "--book", &book, "--config", &config, "report.run", "report=Nope"]);
    assert_eq!((out.status.code(), json(&out.stderr)["error"]["code"].clone()), (Some(2), json!(2000)));
    let out = run(&["exec", "--book", &book, "book.info", "--output", &html_arg]);
    assert_eq!(out.status.code(), Some(1), "--output needs an html result");
}

#[test]
fn call_saves_a_configuration_and_exports_it_to_a_file() {
    let dir = tempfile::tempdir().unwrap();
    let book = copy(dir.path(), "reports/balsheet-pnl.gnucash");
    let config = report_config(dir.path());
    let args = ["serve", "--listen", "127.0.0.1:0", "--book", &book, "--read-only", "--config", &config];
    let mut server = Command::new(BIN).args(args).stderr(Stdio::piped()).spawn().unwrap();
    let mut lines = BufReader::new(server.stderr.take().unwrap()).lines();
    let url = lines
        .by_ref()
        .map_while(Result::ok)
        .find_map(|l| l.split("serving JSON-RPC on ").nth(1).map(str::to_owned))
        .expect("the server reports its address");
    let options = r#"options:={"Display": {"Levels": 4}}"#;
    let out = run(&["call", "--url", &url, "report.saveConfiguration", "name=Four levels", "report=Account Names", options]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(json(&out.stdout)["kind"], "saved");
    let stored: Value = serde_json::from_str(&std::fs::read_to_string(&config).unwrap()).unwrap();
    assert_eq!(stored["reports"]["saved"][0]["options"], json!({ "Display": { "Levels": 4 } }));
    let html = dir.path().join("four.html").display().to_string();
    let out = run(&["call", "--url", &url, "report.export", "report=Four levels", "--output", &html]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(json(&out.stdout)["title"], "Accounts to 4");
    assert!(std::fs::read_to_string(&html).unwrap().contains("<title>Accounts to 4</title>"));
    Command::new("kill").args(["-TERM", &server.id().to_string()]).status().unwrap();
    assert!(server.wait().unwrap().success());
}

/// `red-ale schema | head` closes the pipe early. The schema is larger than a pipe's buffer, so the write is sure to
/// fail; `methods` usually fails too, as its reader closes before the process starts writing.
#[test]
fn a_reader_closing_the_pipe_ends_output_quietly() {
    ["schema", "methods"].into_iter().for_each(|command| {
        let mut child = Command::new(BIN).arg(command).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
        drop(child.stdout.take());
        let out = child.wait_with_output().unwrap();
        assert!(out.status.success(), "{command}: {:?} {}", out.status, String::from_utf8_lossy(&out.stderr));
        assert!(out.stderr.is_empty(), "{command}: {}", String::from_utf8_lossy(&out.stderr));
    });
}

/// Start `red-ale serve` with `args` and return it with its URL.
fn serve_with(args: &[&str]) -> (std::process::Child, String) {
    let mut server = Command::new(BIN).arg("serve").args(["--listen", "127.0.0.1:0"]).args(args).stderr(Stdio::piped()).spawn().unwrap();
    let mut lines = BufReader::new(server.stderr.take().unwrap()).lines();
    let url = lines
        .by_ref()
        .map_while(Result::ok)
        .find_map(|l| l.split("serving JSON-RPC on ").nth(1).map(str::to_owned))
        .expect("the server reports its address");
    (server, url)
}

fn stop(mut server: std::process::Child) {
    Command::new("kill").args(["-TERM", &server.id().to_string()]).status().unwrap();
    assert!(server.wait().unwrap().success());
}

#[test]
fn exec_changes_the_configuration_with_no_book() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("red-ale.json").display().to_string();
    let set = r#"changes:={"import.settings.matchThreshold": 3, "quotes.services.finnhub.key": "SECRET-FH"}"#;
    let out = run(&["exec", "--config", &config, "config.set", set]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(json(&out.stdout)["values"]["quotes.services.finnhub.key"], json!({ "set": true }));
    let out = run(&["exec", "--config", &config, "config.get", "path=quotes.services.finnhub"]);
    assert!(!String::from_utf8_lossy(&out.stdout).contains("SECRET-FH"));
    assert_eq!(json(&out.stdout)["value"]["key"], json!({ "set": true }));
    let out = run(&["exec", "--config", &config, "config.set", r#"changes:={"import.settings.matchThreshold": "x"}"#]);
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(json(&out.stderr)["error"]["code"], -32602);
    let out = run(&["exec", "--config", &config, "config.get", "path=import.settings.matchThreshold"]);
    assert_eq!(json(&out.stdout)["value"], 3, "a failed set changed nothing");
    let out = run(&["exec", "--create", "config.get"]);
    assert!(!out.status.success() && String::from_utf8_lossy(&out.stderr).contains("--book"), "--create needs --book");
}

#[test]
fn call_changes_the_configuration_of_a_running_server() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("red-ale.json").display().to_string();
    let (server, url) = serve_with(&["--config", &config]);
    let out = run(&["call", "--url", &url, "config.set", r#"changes:={"quotes.network": true}"#]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let out = run(&["call", "--url", &url, "config.get", "path=quotes.network"]);
    assert_eq!(json(&out.stdout)["value"], true);
    let out = run(&["call", "--url", &url, "config.schema"]);
    assert_eq!(json(&out.stdout)["properties"]["quotes"]["properties"]["network"]["default"], false);
    stop(server);
}

/// POST one JSON-RPC request of about `bytes` bytes to `url`, returning the HTTP status line.
fn post_large(url: &str, bytes: usize) -> String {
    use std::io::Read;
    let padding = "x".repeat(bytes);
    let body = format!(r#"{{"jsonrpc": "2.0", "id": 1, "method": "config.schema", "params": {{"padding": "{padding}"}}}}"#);
    let addr = url.trim_start_matches("http://").trim_end_matches('/');
    let mut stream = std::net::TcpStream::connect(addr).unwrap();
    let length = body.len();
    let head = format!("POST / HTTP/1.1\r\nHost: {addr}\r\nContent-Type: application/json\r\nContent-Length: {length}\r\nConnection: close\r\n\r\n");
    stream.write_all(head.as_bytes()).unwrap();
    // The server may answer 413 and close before reading the whole body.
    let _ = stream.write_all(body.as_bytes());
    let mut response = String::new();
    let _ = stream.read_to_string(&mut response);
    response.lines().next().unwrap_or_default().to_owned()
}

#[test]
fn the_request_size_limit_comes_from_the_configuration() {
    let dir = tempfile::tempdir().unwrap();
    let (server, url) = serve_with(&[]);
    let status = post_large(&url, 12 << 20);
    assert!(status.contains("200"), "above jsonrpsee's 10 MB default, within red-ale's 64 MiB: {status}");
    stop(server);
    let config = dir.path().join("red-ale.json");
    std::fs::write(&config, r#"{"jsonRpc": {"maxRequestBytes": 1048576}}"#).unwrap();
    let (server, url) = serve_with(&["--config", &config.display().to_string()]);
    let status = post_large(&url, 2 << 20);
    assert!(status.contains("413"), "{status}");
    stop(server);
}
