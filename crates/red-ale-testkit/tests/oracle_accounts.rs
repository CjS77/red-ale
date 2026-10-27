//! Accounts and the accounting period checked against GnuCash 5.17: every edit made through the API reads the same in
//! GnuCash, and GnuCash computes the same tree order, display order, balances, deletions and period dates.

#![cfg(feature = "oracle")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use red_ale_core::Numeric;
use red_ale_testkit::{TempBook, call, call_err, diff, oracle, oracle_check};
use serde_json::{Value, json};

fn assert_same(d: &mut red_ale_rpc::Dispatcher, book: &TempBook) {
    let differences = oracle_check(d, book, &[]);
    assert!(differences.is_empty(), "{}", differences.join("\n"));
}

/// Run a script with the book's path and parse its JSON output.
fn gnucash(script: &str, book: &TempBook, args: &[&str]) -> Value {
    let path = book.path_str();
    let all: Vec<&str> = std::iter::once(path.as_str()).chain(args.iter().copied()).collect();
    serde_json::from_str(&oracle::run_code(script, &all).unwrap_or_else(|e| panic!("{e}"))).unwrap()
}

fn reduced(v: &Value) -> String {
    let n = Numeric::new(
        v["num"].as_str().unwrap().parse().unwrap(),
        v["denom"].as_str().unwrap().parse().unwrap(),
    )
    .reduce();
    format!("{}/{}", n.num(), n.denom())
}

fn guid_of(d: &mut red_ale_rpc::Dispatcher, account: &str) -> String {
    call(d, "account.get", &json!({ "account": account }))["guid"]
        .as_str()
        .unwrap()
        .to_owned()
}

const TREE: &str = r#"
import sys, json
from gnucash import Session, SessionOpenMode, _sw_core_utils
assert _sw_core_utils.gnc_version() == "5.17"
# The bindings' separator is the engine's default ".", not the GUI's ":", so full names are built here.
def full_name(a):
    names = []
    while a.get_parent() is not None:
        names.insert(0, a.GetName())
        a = a.get_parent()
    return ":".join(names)
s = Session("xml://" + sys.argv[1], SessionOpenMode.SESSION_READ_ONLY)
try:
    root = s.book.get_root_account()
    out = {
        "preorder": [full_name(a) for a in root.get_descendants()],
        "sorted": {a.GetGUID().to_string(): [c.GetName() for c in a.get_children_sorted()] for a in [root] + root.get_descendants()},
        "codes": {a.GetGUID().to_string(): a.GetCode() for a in root.get_descendants()},
        "hidden": {a.GetGUID().to_string(): bool(a.IsHidden()) for a in root.get_descendants()},
    }
finally:
    s.end()
print(json.dumps(out))
"#;

fn full_names(d: &mut red_ale_rpc::Dispatcher) -> Vec<Value> {
    call(d, "account.list", &json!({}))["accounts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["fullName"].clone())
        .collect()
}

#[test]
fn hierarchy_new_and_moved_accounts_keep_gnucash_order() {
    let (book, mut d) = red_ale_testkit::open("synthetic/rich.gnucash");
    call(
        &mut d,
        "account.create",
        &json!({ "name": "Projects", "type": "ASSET", "parent": "Assets", "placeholder": true }),
    );
    call(
        &mut d,
        "account.create",
        &json!({ "name": "Alpha", "type": "BANK", "parent": "Assets:Projects", "commodity": "EUR" }),
    );
    // Liabilities comes after Assets:Cash in the file: GnuCash refuses a child written before its parent.
    call(
        &mut d,
        "account.move",
        &json!({ "account": "Assets:Cash", "parent": "Liabilities" }),
    );
    call(&mut d, "account.move", &json!({ "account": "Expenses:Fees", "parent": "Income" }));
    call(
        &mut d,
        "account.move",
        &json!({ "account": "Assets:Projects", "parent": "Liabilities:Cash" }),
    );
    let ours = full_names(&mut d);
    assert_same(&mut d, &book);
    let theirs = gnucash(TREE, &book, &[]);
    assert_eq!(theirs["preorder"].as_array().unwrap(), &ours, "full names and children order");
    assert!(ours.contains(&json!("Liabilities:Cash:Projects:Alpha")));
}

#[test]
fn every_choosable_type_and_receivable_payable() {
    let (book, mut d) = red_ale_testkit::open("synthetic/rich.gnucash");
    let types = call(&mut d, "account.types", &json!({}));
    let choosable: Vec<String> = types
        .as_array()
        .unwrap()
        .iter()
        .filter(|t| t["choosable"] == true)
        .map(|t| t["name"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(choosable.len(), 13);
    choosable.iter().for_each(|t| {
        let commodity = if t == "STOCK" || t == "MUTUAL" { "NASDAQ::ACME" } else { "USD" };
        call(
            &mut d,
            "account.create",
            &json!({ "name": format!("Type {t}"), "type": t, "commodity": commodity }),
        );
    });
    call(
        &mut d,
        "account.create",
        &json!({ "name": "Customers", "type": "RECEIVABLE", "parent": "Assets" }),
    );
    call(
        &mut d,
        "account.create",
        &json!({ "name": "Suppliers", "type": "PAYABLE", "parent": "Liabilities", "code": "2100" }),
    );
    assert_same(&mut d, &book);
}

#[test]
fn flags_codes_notes_scu_and_tax_settings() {
    let (book, mut d) = red_ale_testkit::open("gnucash/taxreport.gnucash");
    let accounts = call(&mut d, "account.list", &json!({}))["accounts"].as_array().unwrap().clone();
    let tax: Vec<&Value> = accounts.iter().filter(|a| a["taxRelated"] == true).collect();
    let plain: Vec<&Value> = accounts.iter().filter(|a| a["taxRelated"] == false && a["children"] == 0).collect();
    assert!(tax.len() >= 2 && plain.len() >= 2, "the fixture has tax-related and plain accounts");
    call(
        &mut d,
        "account.update",
        &json!({ "account": tax[0]["guid"], "taxRelated": false, "taxUsCode": "" }),
    );
    call(
        &mut d,
        "account.update",
        &json!({ "account": tax[1]["guid"], "taxUsCode": "N521", "taxUsPayerNameSource": "parent", "taxUsCopyNumber": 3 }),
    );
    call(
        &mut d,
        "account.update",
        &json!({
            "account": plain[0]["guid"], "code": "4100", "description": "Described", "notes": "Noted\nover lines", "placeholder": true,
            "hidden": true, "taxRelated": true, "taxUsCode": "N286",
        }),
    );
    call(
        &mut d,
        "account.update",
        &json!({ "account": plain[1]["guid"], "scu": 1000, "name": "Renamed é" }),
    );
    let parent = accounts.iter().find(|a| a["children"].as_u64() > Some(0)).unwrap()["guid"].clone();
    call(&mut d, "account.update", &json!({ "account": parent, "hidden": true }));
    let ours: Vec<(String, bool)> = call(&mut d, "account.list", &json!({}))["accounts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| (a["guid"].as_str().unwrap().to_owned(), a["hiddenInTree"].as_bool().unwrap()))
        .collect();
    assert_same(&mut d, &book);
    let theirs = gnucash(TREE, &book, &[]);
    ours.iter()
        .for_each(|(g, hidden)| assert_eq!(theirs["hidden"][g].as_bool(), Some(*hidden), "xaccAccountIsHidden for {g}"));
}

#[test]
fn commodity_change_without_splits_resets_the_scu() {
    let (book, mut d) = red_ale_testkit::open("synthetic/rich.gnucash");
    call(
        &mut d,
        "account.create",
        &json!({ "name": "Spare", "type": "BANK", "parent": "Assets", "scu": 1000 }),
    );
    let changed = call(&mut d, "account.update", &json!({ "account": "Assets:Spare", "commodity": "EUR" }));
    assert_eq!(
        (changed["scu"].as_i64(), changed["nonStandardScu"].as_bool()),
        (Some(1000), Some(true)),
        "the dialog keeps the chosen fraction"
    );
    call(
        &mut d,
        "account.create",
        &json!({ "name": "Fund", "type": "MUTUAL", "parent": "Assets", "commodity": "FUND::ZETA" }),
    );
    call(
        &mut d,
        "account.update",
        &json!({ "account": "Assets:Fund", "type": "STOCK", "commodity": "NASDAQ::ACME", "scu": "commodity" }),
    );
    assert_same(&mut d, &book);
}

#[test]
fn renumber_and_display_order_match_gnucash() {
    let (book, mut d) = red_ale_testkit::open("synthetic/rich.gnucash");
    let theirs = gnucash(TREE, &book, &[]);
    let tree = call(&mut d, "account.tree", &json!({ "sorted": true }));
    let mut stack = vec![tree];
    while let Some(node) = stack.pop() {
        let names: Vec<Value> = node["children"].as_array().unwrap().iter().map(|c| c["name"].clone()).collect();
        assert_eq!(
            theirs["sorted"][node["guid"].as_str().unwrap()].as_array().unwrap(),
            &names,
            "children of {}",
            node["fullName"]
        );
        stack.extend(node["children"].as_array().unwrap().iter().cloned());
    }
    let assets = call(&mut d, "account.renumber", &json!({ "account": "Assets" }));
    let expenses = call(
        &mut d,
        "account.renumber",
        &json!({ "account": "Expenses", "prefix": "E", "interval": 250, "digits": 1 }),
    );
    assert_eq!(expenses[1]["code"], "E-500", "two children: 500 needs three digits");
    assert_same(&mut d, &book);
    let theirs = gnucash(TREE, &book, &[]);
    assets.as_array().unwrap().iter().chain(expenses.as_array().unwrap()).for_each(|c| {
        assert_eq!(theirs["codes"][c["account"].as_str().unwrap()], c["code"]);
    });
}

/// GnuCash's `do_delete_account` for an account with splits and no sub-accounts: move the splits, then destroy it.
const GNUCASH_DELETE: &str = r#"
import sys
from gnucash import Session, SessionOpenMode, _sw_core_utils
assert _sw_core_utils.gnc_version() == "5.17"
s = Session("xml://" + sys.argv[1], SessionOpenMode.SESSION_NORMAL_OPEN)
try:
    root = s.book.get_root_account()
    def lookup(full):
        a = root
        for name in full.split(":"):
            a = [c for c in a.get_children() if c.GetName() == name][0]
        return a
    doomed, target = lookup(sys.argv[2]), lookup(sys.argv[3])
    doomed.BeginEdit()
    doomed.MoveAllSplits(target)
    doomed.CommitEdit()
    doomed.BeginEdit()
    doomed.Destroy()
    s.save()
finally:
    s.end()
print("{}")
"#;

#[test]
fn delete_with_move_matches_gnucash() {
    let theirs = TempBook::copy("synthetic/rich.gnucash");
    gnucash(GNUCASH_DELETE, &theirs, &["Expenses:Food", "Expenses:Fees"]);
    let (book, mut d) = red_ale_testkit::open("synthetic/rich.gnucash");
    let r = call(
        &mut d,
        "account.delete",
        &json!({ "account": "Expenses:Food", "transactions": { "moveTo": "Expenses:Fees" } }),
    );
    assert!(!r["movedSplits"].as_array().unwrap().is_empty());
    assert_eq!(
        call_err(&mut d, "account.delete", &json!({ "account": "Assets:Brokerage" })).code,
        red_ale_rpc::codes::CONFLICT,
        "two sub-accounts"
    );
    assert_same(&mut d, &book);
    let (a, b) = (oracle::dump(&theirs.path).unwrap(), oracle::dump(&book.path).unwrap());
    let differences: Vec<String> = ["accounts", "transactions", "lots"]
        .iter()
        .flat_map(|k| diff(&a[*k], &b[*k], &[]))
        .collect();
    assert!(differences.is_empty(), "{}", differences.join("\n"));
}

#[test]
fn delete_reparenting_the_sub_account_under_a_later_account() {
    let (book, mut d) = red_ale_testkit::open("synthetic/rich.gnucash");
    call(
        &mut d,
        "account.create",
        &json!({ "name": "Owners", "type": "EQUITY", "commodity": "USD" }),
    );
    let opening = guid_of(&mut d, "Equity:Opening Balances");
    let r = call(
        &mut d,
        "account.delete",
        &json!({ "account": "Equity", "subaccount": { "moveTo": "Owners" } }),
    );
    assert_eq!(r["reparented"], json!(opening));
    assert_eq!(
        call(&mut d, "account.get", &json!({ "account": opening }))["fullName"],
        "Owners:Opening Balances"
    );
    assert_same(&mut d, &book);
}

const BALANCES: &str = r#"
import sys, json, math
from gnucash import Session, SessionOpenMode, _sw_core_utils
assert _sw_core_utils.gnc_version() == "5.17"
def frac(n):
    # GncNumeric has num()/denom() methods; the raw struct some getters return has num/denom fields.
    num = n.num() if callable(n.num) else n.num
    den = n.denom() if callable(n.denom) else n.denom
    g = math.gcd(num, den) or 1
    return "%d/%d" % (num // g, den // g)
s = Session("xml://" + sys.argv[1], SessionOpenMode.SESSION_READ_ONLY)
cuts = [int(c) for c in sys.argv[2:]]
try:
    out = {}
    for a in s.book.get_root_account().get_descendants():
        c = a.GetCommodity()
        out[a.GetGUID().to_string()] = {
            "balance": frac(a.GetBalance()), "cleared": frac(a.GetClearedBalance()), "reconciled": frac(a.GetReconciledBalance()),
            "asOf": [frac(a.GetBalanceAsOfDate(t)) for t in cuts],
            "reconciledAsOf": [frac(a.GetReconciledBalanceAsOfDate(t)) for t in cuts],
            "withSubaccounts": frac(a.GetBalanceInCurrency(c, True)) if c else None,
        }
finally:
    s.end()
print(json.dumps(out))
"#;

fn compare_balances(fixture: &str) {
    let (book, mut d) = red_ale_testkit::open(fixture);
    let days: Vec<String> = ["2005-12-31", "2014-03-02", "2019-06-30", "2024-01-31"]
        .iter()
        .map(|s| (*s).to_owned())
        .collect();
    let cuts: Vec<String> = days
        .iter()
        .map(|day| red_ale_core::Date::parse(day).unwrap().end_utc().seconds().to_string())
        .collect();
    let cut_refs: Vec<&str> = cuts.iter().map(String::as_str).collect();
    let theirs = gnucash(BALANCES, &book, &cut_refs);
    let accounts = call(&mut d, "account.list", &json!({}))["accounts"].as_array().unwrap().clone();
    let failures: Vec<String> = accounts
        .iter()
        .flat_map(|a| {
            let g = a["guid"].as_str().unwrap();
            let expected = &theirs[g];
            let now = call(&mut d, "account.balance", &json!({ "account": g }));
            let mut ours = vec![
                ("balance", reduced(&now["balances"]["balance"]), expected["balance"].clone()),
                ("cleared", reduced(&now["balances"]["cleared"]), expected["cleared"].clone()),
                (
                    "reconciled",
                    reduced(&now["balances"]["reconciled"]),
                    expected["reconciled"].clone(),
                ),
            ];
            days.iter().enumerate().for_each(|(i, day)| {
                let at = call(&mut d, "account.balance", &json!({ "account": g, "asOf": day }));
                ours.push(("asOf", reduced(&at["balances"]["balance"]), expected["asOf"][i].clone()));
                ours.push((
                    "reconciledAsOf",
                    reduced(&at["balances"]["reconciled"]),
                    expected["reconciledAsOf"][i].clone(),
                ));
            });
            let sub = call(&mut d, "account.balance", &json!({ "account": g, "includeSubaccounts": true }));
            if sub["unconverted"].as_array().unwrap().is_empty() {
                ours.push((
                    "withSubaccounts",
                    reduced(&sub["balances"]["balance"]),
                    expected["withSubaccounts"].clone(),
                ));
            }
            ours.into_iter()
                .filter(|(_, mine, theirs)| theirs.as_str() != Some(mine.as_str()))
                .map(|(what, mine, theirs)| format!("{fixture} {} {what}: GnuCash {theirs} != red-ale {mine}", a["fullName"]))
                .collect::<Vec<_>>()
        })
        .collect();
    call(&mut d, "book.close", &json!({ "discard": true }));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn balances_match_gnucash() {
    [
        "synthetic/rich.gnucash",
        "gnucash/taxreport.gnucash",
        "gnucash/sample1.gnucash",
        "piecash/all_account_types.gnucash",
        "synthetic/business-editable.gnucash",
    ]
    .iter()
    .for_each(|f| compare_balances(f));
}

/// GnuCash's own period computation, reached through ctypes because the bindings do not wrap it.
const PERIODS: &str = r#"
import sys, os, json, ctypes
import gnucash
from gnucash import _sw_core_utils
assert _sw_core_utils.gnc_version() == "5.17"
lib = ctypes.CDLL(os.path.join(os.path.dirname(gnucash.__file__), "..", "..", "..", "libgnc-engine.so"))
glib = ctypes.CDLL("libglib-2.0.so.0")
glib.g_date_new_dmy.restype = ctypes.c_void_p
glib.g_date_new_dmy.argtypes = [ctypes.c_ubyte, ctypes.c_int, ctypes.c_ushort]
for f in ("g_date_get_day", "g_date_get_month", "g_date_get_year"):
    getattr(glib, f).argtypes = [ctypes.c_void_p]
for f in ("gnc_accounting_period_start_gdate", "gnc_accounting_period_end_gdate"):
    getattr(lib, f).restype = ctypes.c_void_p
    getattr(lib, f).argtypes = [ctypes.c_int, ctypes.c_void_p, ctypes.c_void_p]
def show(d):
    return "%04d-%02d-%02d" % (glib.g_date_get_year(d), glib.g_date_get_month(d), glib.g_date_get_day(d))
def gdate(s):
    y, m, d = (int(x) for x in s.split("-"))
    return glib.g_date_new_dmy(d, m, y)
out = {}
for fy in sys.argv[2].split(","):
    fy_end = gdate("2001-" + fy)
    for day in sys.argv[3].split(","):
        for which in range(9):
            c = gdate(day)
            out["%s %s %d" % (fy, day, which)] = [show(lib.gnc_accounting_period_start_gdate(which, fy_end, c)),
                                                   show(lib.gnc_accounting_period_end_gdate(which, fy_end, c))]
print(json.dumps(out))
"#;

#[test]
fn accounting_period_dates_match_gnucash() {
    let book = TempBook::copy("synthetic/rich.gnucash");
    let fys = ["06-30", "12-31", "03-31", "02-28", "01-01"];
    let days = [
        "2024-02-29",
        "2024-03-31",
        "2023-12-31",
        "2024-01-01",
        "2024-06-30",
        "2024-07-01",
        "2025-02-28",
        "2024-11-30",
    ];
    let theirs = gnucash(PERIODS, &book, &[&fys.join(","), &days.join(",")]);
    let mut d = red_ale_testkit::dispatcher();
    let choices = call(&mut d, "period.get", &json!({}))["choices"].as_array().unwrap().clone();
    let failures: Vec<String> = fys
        .iter()
        .flat_map(|fy| days.iter().map(move |day| (fy, day)))
        .flat_map(|(fy, day)| choices.iter().enumerate().map(move |(i, c)| (fy, day, i, c.clone())))
        .filter_map(|(fy, day, i, choice)| {
            let ours = call(
                &mut d,
                "period.dates",
                &json!({ "date": day, "start": choice, "end": choice, "fiscalYearEnd": fy }),
            );
            let expected = &theirs[format!("{fy} {day} {i}")];
            let mine = json!([ours["start"], ours["end"]]);
            (&mine != expected).then(|| format!("{choice} on {day}, fiscal year ending {fy}: GnuCash {expected} != red-ale {mine}"))
        })
        .collect();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
