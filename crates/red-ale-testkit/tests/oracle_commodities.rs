//! GnuCash 5.17 reads the commodities red-ale's security editor writes, and computes the same euro conversions.

#![cfg(feature = "oracle")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use red_ale_testkit::{TempBook, call, create, open, oracle, oracle_check};
use serde_json::{Value, json};

/// GnuCash's view of some commodities' quote settings and symbols, including the `auto_quote_control` override.
const QUOTES: &str = r#"
import json, sys
from gnucash import Session, SessionOpenMode
from gnucash import gnucash_core_c as c
s = Session("xml://" + sys.argv[1], SessionOpenMode.SESSION_READ_ONLY)
try:
    t = s.book.get_table()
    out = {}
    for uid in sys.argv[2:]:
        ns, m = uid.split("::")
        cm = t.lookup(ns, m)
        qs = cm.get_quote_source()
        out[uid] = {
            "quoteFlag": bool(cm.get_quote_flag()),
            "quoteSource": c.gnc_quote_source_get_internal_name(qs) if qs is not None and cm.get_quote_flag() else None,
            "quoteTz": cm.get_quote_tz() or None,
            "userSymbol": cm.get_user_symbol() or None,
            "fraction": cm.get_fraction(),
            "fullname": cm.get_fullname() or None,
            "cusip": cm.get_cusip() or None,
        }
    print(json.dumps(out, sort_keys=True))
finally:
    s.end()
"#;

fn ours(d: &mut red_ale_rpc::Dispatcher, id: &str) -> Value {
    let c = call(d, "commodity.get", &json!({ "commodity": id }));
    json!({
        "quoteFlag": c["quoteFlag"],
        "quoteSource": if c["quoteFlag"] == true { c["quoteSource"].clone() } else { Value::Null },
        "quoteTz": c["quoteTz"].as_str().filter(|s| !s.is_empty()),
        "userSymbol": c["userSymbol"],
        "fraction": c["fraction"],
        "fullname": c["fullname"],
        "cusip": c["cusip"],
    })
}

#[test]
fn edited_commodities_read_the_same_in_gnucash() {
    let (book, mut d) = open("synthetic/rich.gnucash");
    call(
        &mut d,
        "commodity.create",
        &json!({ "namespace": "NYSE", "mnemonic": "IBM", "fullname": "Intl Business Machines", "cusip": "US4592001014", "fraction": 100,
                 "quoteFlag": true, "quoteSource": "my_private_src", "quoteTz": "Mars/Olympus", "userSymbol": "IBM$" }),
    );
    call(&mut d, "commodity.update", &json!({ "commodity": "NASDAQ::ACME", "mnemonic": "ACM", "fraction": 100, "quoteSource": "alphavantage" }));
    call(&mut d, "commodity.update", &json!({ "commodity": "USD", "quoteFlag": false }));
    call(&mut d, "commodity.update", &json!({ "commodity": "GBP", "quoteFlag": true, "quoteTz": "Europe/London", "userSymbol": "GB£" }));
    call(&mut d, "commodity.update", &json!({ "commodity": "EUR", "userSymbol": "€uro" }));
    call(&mut d, "commodity.create", &json!({ "namespace": "X", "mnemonic": "Y", "fullname": "y" }));
    call(&mut d, "commodity.delete", &json!({ "commodity": "X::Y" }));
    let diffs = oracle_check(&mut d, &book, &[]);
    assert!(diffs.is_empty(), "{}", diffs.join("\n"));

    let ids = ["NYSE::IBM", "NASDAQ::ACM", "CURRENCY::USD", "CURRENCY::GBP", "CURRENCY::EUR", "FUND::ZETA"];
    let path = book.path_str();
    let args: Vec<&str> = std::iter::once(path.as_str()).chain(ids).collect();
    let gnucash: Value = serde_json::from_str(&oracle::run_code(QUOTES, &args).unwrap()).unwrap();
    call(&mut d, "book.open", &json!({ "path": path, "readOnly": true }));
    ids.iter().for_each(|id| assert_eq!(gnucash[*id], ours(&mut d, id), "{id}"));
    assert_eq!(gnucash["CURRENCY::USD"]["quoteFlag"], false, "the override survives GnuCash's load");
}

/// GnuCash's `gnc_convert_to_euro` and `gnc_convert_from_euro`, which the bindings do not expose, through ctypes.
const EURO: &str = r#"
import ctypes, json, os, sys
from gnucash import Session, SessionOpenMode
class N(ctypes.Structure):
    _fields_ = [("num", ctypes.c_int64), ("denom", ctypes.c_int64)]
lib = ctypes.CDLL(os.path.join(os.path.dirname(os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(
    __import__("gnucash").__file__))))), "libgnc-engine.so"))
for f in (lib.gnc_convert_to_euro, lib.gnc_convert_from_euro):
    f.restype = N
    f.argtypes = [ctypes.c_void_p, N]
s = Session("xml://" + sys.argv[1], SessionOpenMode.SESSION_READ_ONLY)
try:
    t = s.book.get_table()
    out = []
    for case in sys.argv[2:]:
        code, num, denom, direction = case.split(",")
        cm = int(t.lookup("CURRENCY", code).instance)
        f = lib.gnc_convert_to_euro if direction == "to" else lib.gnc_convert_from_euro
        r = f(cm, N(int(num), int(denom)))
        out.append({"num": str(r.num), "denom": str(r.denom)})
    print(json.dumps(out))
finally:
    s.end()
"#;

#[test]
fn euro_conversions_match_gnucash() {
    let (book, mut d) = create();
    let cases = [
        ("DEM", 10_000, 100, "to"),
        ("DEM", -9779, 1_000_000, "to"),
        ("FRF", 123_457, 100, "to"),
        ("ITL", 1_000_000, 1, "to"),
        ("BGN", 195_583, 100, "to"),
        ("ITL", 100, 100, "from"),
        ("IEP", -12_345, 100, "from"),
        ("BGN", 1, 100, "from"),
        ("HRK", 99_999, 1000, "from"),
        ("EUR", 1234, 100, "to"),
    ];
    let ours: Vec<Value> = cases
        .iter()
        .map(|(code, num, denom, dir)| {
            let (from, to) = if *dir == "to" { (*code, "EUR") } else { ("EUR", *code) };
            let amount = format!("{num}/{denom}");
            call(&mut d, "currency.euroConvert", &json!({ "amount": amount, "from": from, "to": to }))["amount"].clone()
        })
        .collect();
    call(&mut d, "book.close", &json!({}));
    let specs: Vec<String> = cases.iter().map(|(c, n, dn, dir)| format!("{c},{n},{dn},{dir}")).collect();
    let path = book.path_str();
    let args: Vec<&str> = std::iter::once(path.as_str()).chain(specs.iter().map(String::as_str)).collect();
    let gnucash: Value = serde_json::from_str(&oracle::run_code(EURO, &args).unwrap()).unwrap();
    assert_eq!(gnucash, Value::Array(ours));
}

#[test]
fn a_new_book_lists_what_gnucash_lists() {
    let book = TempBook::new_path();
    let mut d = red_ale_testkit::dispatcher();
    call(&mut d, "book.create", &json!({ "path": book.path_str() }));
    let ours: Vec<String> =
        call(&mut d, "commodity.list", &json!({})).as_array().unwrap().iter().map(|c| format!("{} {}", c["id"].as_str().unwrap(), c["fraction"])).collect();
    call(&mut d, "book.close", &json!({}));
    let code = r#"
import sys
from gnucash import Session, SessionOpenMode
s = Session("xml://" + sys.argv[1], SessionOpenMode.SESSION_READ_ONLY)
try:
    for ns in s.book.get_table().get_namespaces_list():
        for cm in ns.get_commodity_list():
            print(cm.get_unique_name(), cm.get_fraction())
finally:
    s.end()
"#;
    let mut gnucash: Vec<String> = oracle::run_code(code, &[&book.path_str()]).unwrap().lines().map(str::to_owned).collect();
    gnucash.sort();
    let mut sorted = ours.clone();
    sorted.sort();
    assert_eq!(gnucash, sorted);
}
