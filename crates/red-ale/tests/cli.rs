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
