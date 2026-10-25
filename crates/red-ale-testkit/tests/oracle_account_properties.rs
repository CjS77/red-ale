//! Account colours, balance limits, the opening balance account, associated accounts, cascading and display symbols
//! checked against GnuCash 5.17: GnuCash reads every property red-ale wrote through its own getters (`dump.py`), finds
//! the same opening balance account, reaches the same balance limit verdicts, and cascading in GnuCash gives the same
//! book.

#![cfg(feature = "oracle")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use red_ale_testkit::{TempBook, call, diff, oracle, oracle_check};
use serde_json::{Value, json};

fn assert_same(d: &mut red_ale_rpc::Dispatcher, book: &TempBook) {
    let differences = oracle_check(d, book, &[]);
    assert!(differences.is_empty(), "{}", differences.join("\n"));
}

fn gnucash(script: &str, path: &str, args: &[&str]) -> Value {
    let all: Vec<&str> = std::iter::once(path).chain(args.iter().copied()).collect();
    serde_json::from_str(&oracle::run_code(script, &all).unwrap_or_else(|e| panic!("{e}"))).unwrap()
}

fn post(d: &mut red_ale_rpc::Dispatcher, debit: &str, credit: &str, amount: &str, date: &str) {
    let splits = json!([{ "account": debit, "value": amount }, { "account": credit, "value": format!("-{amount}") }]);
    call(d, "transaction.post", &json!({ "description": "x", "datePosted": date, "splits": splits }));
}

#[test]
fn every_property_reads_the_same_in_gnucash() {
    let (book, mut d) = red_ale_testkit::open("synthetic/rich.gnucash");
    call(&mut d, "account.create", &json!({ "name": "Coloured", "type": "BANK", "parent": "Assets", "color": "#c0ffee" }));
    call(&mut d, "account.update", &json!({ "account": "Assets", "color": "rgb(1, 2, 3)", "includeSubaccountBalances": true,
        "higherBalanceLimit": "12345.67", "lowerBalanceLimit": "-100" }));
    call(&mut d, "account.update", &json!({ "account": "Assets:Coloured", "higherBalanceLimit": "0", "lowerBalanceLimit": "0" }));
    call(&mut d, "account.create", &json!({ "name": "Opening", "type": "EQUITY", "commodity": "EUR", "openingBalance": true }));
    call(&mut d, "account.update", &json!({ "account": "Assets:Coloured", "associatedAccounts": {
        "stock-dividends": "Income", "stock-capgains": "Opening", "stock-broker-fees": "Expenses", "stock-cash-proceeds": "Assets" } }));
    call(&mut d, "account.update", &json!({ "account": "Assets:Coloured", "associatedAccounts": { "stock-broker-fees": null } }));
    call(&mut d, "commodity.update", &json!({ "commodity": "EUR", "userSymbol": "EUR€" }));
    call(&mut d, "commodity.update", &json!({ "commodity": "NASDAQ::ACME", "userSymbol": "Ⓐ" }));
    assert_same(&mut d, &book);
}

const OPENING: &str = r#"
import sys, json
from gnucash import Session, SessionOpenMode, Account, _sw_core_utils
from gnucash import gnucash_core_c as c
assert _sw_core_utils.gnc_version() == "5.17"
# The lookup half of gnc_find_or_create_equity_account (5.17:libgnucash/app-utils/gnc-ui-util.cpp:563-622), through
# GnuCash's own lookups; the feature flag comes from the caller, as the bindings cannot read Book features.
s = Session("xml://" + sys.argv[1], SessionOpenMode.SESSION_READ_ONLY)
out = {}
try:
    root = s.book.get_root_account()
    table = s.book.get_table()
    feature = sys.argv[2] == "true"
    def by_name(name, cur):
        a = c.gnc_account_lookup_by_name(root.instance, name)
        if a is None:
            return None
        a = Account(instance=a)
        if a.GetType() != c.ACCT_TYPE_EQUITY or a.GetCommodity().get_unique_name() != cur.get_unique_name():
            return None
        return a
    for code in sys.argv[3:]:
        cur = table.lookup("CURRENCY", code)
        found = None
        if feature:
            a = c.gnc_account_lookup_by_opening_balance(root.instance, cur.instance)
            if a is not None:
                found = [Account(instance=a).GetGUID().to_string(), "equityType"]
        if found is None:
            a = by_name("Opening Balances", cur)
            if a is not None:
                found = [a.GetGUID().to_string(), "name"]
        if found is None:
            a = by_name("Opening Balances - " + code, cur)
            if a is not None:
                found = [a.GetGUID().to_string(), "nameWithCurrency"]
        out[code] = found
finally:
    s.end()
print(json.dumps(out))
"#;

fn opening_balances(d: &mut red_ale_rpc::Dispatcher, codes: &[&str]) -> Value {
    codes
        .iter()
        .map(|code| {
            let r = call(d, "account.openingBalanceAccount", &json!({ "currency": code }));
            let found = if r["account"].is_null() { Value::Null } else { json!([r["account"]["guid"], r["foundBy"]]) };
            ((*code).to_owned(), found)
        })
        .collect::<serde_json::Map<_, _>>()
        .into()
}

#[test]
fn gnucash_finds_the_same_opening_balance_account() {
    let (book, mut d) = red_ale_testkit::create();
    call(&mut d, "account.create", &json!({ "name": "Equity", "type": "EQUITY", "commodity": "USD" }));
    call(&mut d, "account.create", &json!({ "name": "Deep", "type": "EQUITY", "parent": "Equity", "commodity": "USD" }));
    call(&mut d, "account.create", &json!({ "name": "Opening Balances", "type": "EQUITY", "parent": "Equity:Deep", "commodity": "USD" }));
    call(&mut d, "account.create", &json!({ "name": "Opening Balances", "type": "EQUITY", "parent": "Equity", "commodity": "EUR" }));
    call(&mut d, "account.create", &json!({ "name": "Opening Balances - EUR", "type": "EQUITY", "commodity": "EUR" }));
    call(&mut d, "account.create", &json!({ "name": "Opening Balances - GBP", "type": "INCOME", "commodity": "GBP" }));
    let codes = ["USD", "EUR", "GBP", "JPY"];
    let ours = opening_balances(&mut d, &codes);
    call(&mut d, "book.save", &json!({}));
    let theirs = gnucash(OPENING, &book.path_str(), &[&["false"], &codes[..]].concat());
    assert_eq!(theirs, ours, "by name");
    call(&mut d, "account.create", &json!({ "name": "OB", "type": "EQUITY", "parent": "Equity:Deep", "commodity": "USD", "openingBalance": true }));
    call(&mut d, "account.create", &json!({ "name": "OB", "type": "EQUITY", "commodity": "JPY", "openingBalance": true }));
    let ours = opening_balances(&mut d, &codes);
    assert_eq!(ours["USD"][1], "equityType");
    assert_same(&mut d, &book);
    let theirs = gnucash(OPENING, &book.path_str(), &[&["true"], &codes[..]].concat());
    assert_eq!(theirs, ours, "by flag");
    let (saved, _) = red_ale_xml::read_book(&std::fs::read(&book.path).unwrap()).unwrap();
    let feature = red_ale_core::ops::account_properties::OPENING_BALANCE_FEATURE;
    let described = format!("{feature} (requires at least Gnucash 4.3)");
    assert!(red_ale_core::book_options::features(&saved).contains(&(feature.to_owned(), described)), "GnuCash's name and description");
}

const LIMITS: &str = r#"
import sys, json, datetime
from fractions import Fraction
from gnucash import Session, SessionOpenMode, GncNumeric, _sw_core_utils
from gnucash import gnucash_core_c as c
assert _sw_core_utils.gnc_version() == "5.17"
# account_get_balance_as_of_date and get_balance_limit_info (5.17:libgnucash/app-utils/gnc-ui-balances.cpp:184-394),
# replayed through the engine's getters with the display sign reversed or not; the verdict is mapped to raw sign.
def numeric(n):
    return Fraction(n.num(), n.denom())
s = Session("xml://" + sys.argv[1], SessionOpenMode.SESSION_READ_ONLY)
out = {}
try:
    root = s.book.get_root_account()
    when = datetime.datetime.fromtimestamp(int(sys.argv[2]))
    for a in root.get_descendants():
        bal = a.GetBalanceAsOfDate(when)
        balance = numeric(bal)
        if a.GetIncludeSubAccountBalances():
            balance += sum(numeric(sub.GetBalanceAsOfDate(when)) for sub in a.get_descendants())
        hi, lo = GncNumeric(), GncNumeric()
        has_hi, has_lo = a.GetHigherBalanceLimit(hi), a.GetLowerBalanceLimit(lo)
        verdicts = []
        for reverse in (False, True):
            shown = -balance if reverse else balance
            def info(higher):
                if (higher and reverse) or (not higher and not reverse):
                    return (has_lo, numeric(lo))
                return (has_hi, numeric(hi))
            def reached(higher):
                valid, limit = info(higher)
                if not valid:
                    return (False, False)
                if shown == 0:
                    return (False, limit == 0)
                limit = -limit if reverse else limit
                return ((shown > limit) if higher else (shown < limit), info(higher)[1] == 0)
            (hr, hz), (lr, lz) = reached(True), reached(False)
            if hr and not hz:
                v = "top"
            elif lr and (not lz or not hz):
                v = "bottom"
            elif hr and not lz:
                v = "top"
            elif (lr or hr) and lz and hz:
                v = "shouldBeZero"
            else:
                v = None
            if v in ("top", "bottom"):
                v = "above" if (v == "top") != reverse else "below"
            verdicts.append(v)
        assert verdicts[0] == verdicts[1], verdicts
        out[a.GetGUID().to_string()] = verdicts[0]
finally:
    s.end()
print(json.dumps(out))
"#;

#[test]
fn balance_limit_verdicts_match_gnucash() {
    let (book, mut d) = red_ale_testkit::create();
    let mk = |d: &mut red_ale_rpc::Dispatcher, name: &str, ty: &str, parent: Option<&str>| {
        let mut p = json!({ "name": name, "type": ty, "commodity": "USD" });
        if let Some(parent) = parent {
            p["parent"] = json!(parent);
        }
        call(d, "account.create", &p)["guid"].as_str().unwrap().to_owned()
    };
    mk(&mut d, "Bank", "BANK", None);
    mk(&mut d, "Sub", "BANK", Some("Bank"));
    mk(&mut d, "Card", "CREDIT", None);
    mk(&mut d, "Zero", "ASSET", None);
    mk(&mut d, "Flat", "ASSET", None);
    mk(&mut d, "Income", "INCOME", None);
    post(&mut d, "Bank", "Income", "100.00", "2024-06-01");
    post(&mut d, "Bank:Sub", "Income", "50.00", "2024-06-02");
    post(&mut d, "Bank", "Card", "75.00", "2024-06-03");
    post(&mut d, "Zero", "Income", "0.50", "2024-06-04");
    post(&mut d, "Bank", "Income", "1000.00", "2024-08-01");
    let edits = [
        json!({ "account": "Bank", "higherBalanceLimit": "200", "lowerBalanceLimit": "0", "includeSubaccountBalances": true }),
        json!({ "account": "Bank:Sub", "lowerBalanceLimit": "60" }),
        json!({ "account": "Card", "lowerBalanceLimit": "-50", "higherBalanceLimit": "0" }),
        json!({ "account": "Zero", "lowerBalanceLimit": "0", "higherBalanceLimit": "0" }),
        json!({ "account": "Flat", "higherBalanceLimit": "-1" }),
        json!({ "account": "Income", "higherBalanceLimit": "-2000", "lowerBalanceLimit": "-2000.01" }),
    ];
    edits.iter().for_each(|e| drop(call(&mut d, "account.update", e)));
    let accounts = call(&mut d, "account.list", &json!({}))["accounts"].as_array().unwrap().clone();
    let ours: serde_json::Map<String, Value> = accounts
        .iter()
        .map(|a| {
            let r = call(&mut d, "account.checkBalanceLimit", &json!({ "account": a["guid"], "asOf": "2024-07-01" }));
            (a["guid"].as_str().unwrap().to_owned(), r["state"].clone())
        })
        .collect();
    assert_eq!(ours.values().filter(|v| !v.is_null()).count(), 5, "{ours:?}");
    call(&mut d, "book.save", &json!({}));
    // 2024-07-01 23:59:59 UTC, the end of the day in the oracle's TZ=UTC.
    let theirs = gnucash(LIMITS, &book.path_str(), &["1719878399"]);
    assert_eq!(diff(&theirs, &Value::Object(ours), &[]), Vec::<String>::new());
}

const CASCADE: &str = r#"
import sys, json
from gnucash import Session, SessionOpenMode, _sw_core_utils
assert _sw_core_utils.gnc_version() == "5.17"
# gnc_account_cascade_properties_dialog's OK and update_account_color
# (5.17:gnucash/gnome-utils/dialog-account.c:2388-2407, :2523-2589), replayed through the bindings.
path, name, color, replace, placeholder, hidden = sys.argv[1:7]
color = None if color in ("", "rgb(237,236,235)") else color
replace = replace == "true"
def update_color(acc, old):
    if color:
        if (not old or replace) and color != old:
            acc.SetColor(color)
    elif old and replace:
        acc.SetColor("")
s = Session("xml://" + path, SessionOpenMode.SESSION_NORMAL_OPEN)
try:
    root = s.book.get_root_account()
    acc = [a for a in root.get_descendants() if a.GetName() == name][0]
    for a in [acc] + acc.get_descendants():
        update_color(a, a.GetColor())
        if placeholder != "-":
            a.SetPlaceholder(placeholder == "true")
        if hidden != "-":
            a.SetHidden(hidden == "true")
    s.save()
finally:
    s.end()
print("{}")
"#;

#[test]
fn cascading_matches_gnucash_replaying_the_dialog() {
    let (book, mut d) = red_ale_testkit::open("synthetic/rich.gnucash");
    call(&mut d, "account.create", &json!({ "name": "Top", "type": "ASSET", "commodity": "USD", "color": "#010203" }));
    call(&mut d, "account.create", &json!({ "name": "Mid", "type": "ASSET", "parent": "Top", "color": "#ff0000", "hidden": true }));
    call(&mut d, "account.create", &json!({ "name": "Leaf", "type": "BANK", "parent": "Top:Mid" }));
    call(&mut d, "account.create", &json!({ "name": "Twig", "type": "BANK", "parent": "Top", "placeholder": true }));
    call(&mut d, "book.save", &json!({}));
    let cases = [
        ("#00ff00", false, Some(true), None),
        ("#0000ff", true, None, Some(false)),
        ("", true, Some(false), Some(true)),
        ("", false, None, None),
    ];
    cases.iter().for_each(|(color, replace, placeholder, hidden)| {
        let theirs = TempBook::new_path();
        std::fs::copy(&book.path, &theirs.path).unwrap();
        let mut p = json!({ "account": "Top", "color": color, "replaceColors": replace });
        placeholder.iter().for_each(|v| p["placeholder"] = json!(v));
        hidden.iter().for_each(|v| p["hidden"] = json!(v));
        call(&mut d, "account.cascade", &p);
        call(&mut d, "book.save", &json!({}));
        let flag = |v: &Option<bool>| v.map_or("-", |b| if b { "true" } else { "false" });
        let color = red_ale_core::ops::account_properties::parse_color(color).unwrap().unwrap_or_default();
        let replace = if *replace { "true" } else { "false" };
        gnucash(CASCADE, &theirs.path_str(), &["Top", &color, replace, flag(placeholder), flag(hidden)]);
        let (ours, gnc) = (oracle::dump(&book.path).unwrap(), oracle::dump(&theirs.path).unwrap());
        assert_eq!(diff(&gnc["accounts"], &ours["accounts"], &[]), Vec::<String>::new(), "cascade {p}");
    });
    assert_same(&mut d, &book);
}
