//! Scheduled transactions against GnuCash 5.17 (`oracle/scheduled.py`): GnuCash adds a scheduled transaction with
//! every field a file holds and a template transaction with formulas, red-ale reads it and generates it exactly, and
//! after an unrelated edit saves a book in which GnuCash reads every scheduled transaction and template split as it
//! read its own. No Guile is loaded: the script calls `libgnc-engine.so` only.

#![cfg(feature = "oracle")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use red_ale_core::{Guid, TemplateSplit};
use red_ale_testkit::{TempBook, call, dispatcher, oracle};
use serde_json::{Value, json};

const LOAN: &str = "000000000000000000000000005c4ed1";
const RENT: &str = "52f96837087e9670c933f9421b0ac626";

fn script(args: &[&str]) -> String {
    oracle::check_version().unwrap();
    oracle::run_script(&oracle::scripts_dir().join("scheduled.py"), args).unwrap_or_else(|e| panic!("oracle: {e}"))
}

/// Every scheduled transaction as GnuCash reads it.
fn gnucash_scheduled(book: &TempBook) -> Value {
    serde_json::from_str(&script(&["dump", &book.path_str()])).unwrap()
}

/// red-ale reads the same fields GnuCash reads.
fn assert_red_ale_reads(book: &TempBook, expected: &Value) {
    let (read, residue) = red_ale_xml::read_book(&std::fs::read(&book.path).unwrap()).unwrap();
    assert_eq!(residue.originals_kept(), 0, "red-ale generates GnuCash's scheduled transactions as GnuCash wrote them");
    assert_eq!(read.scheduled.len(), 2);
    let loan = &read.scheduled[&Guid::parse(LOAN).unwrap()];
    let g = &expected[LOAN];
    assert_eq!(json!(loan.name), g["name"]);
    let flags = json!([g["enabled"], g["autoCreate"], g["autoCreateNotify"]]);
    assert_eq!(json!([loan.enabled, loan.auto_create, loan.auto_create_notify]), flags);
    assert_eq!(json!([loan.advance_create_days, loan.advance_remind_days, loan.instance_count]), json!([3, 2, 7]));
    let occurrences = loan.occurrences.map(|o| [o.total, o.remaining]);
    assert_eq!(json!(occurrences), g["occurrences"]);
    assert_eq!(json!(loan.last.map(red_ale_core::Date::to_file_string)), g["last"]);
    let deferred: Vec<Value> = loan
        .deferred
        .iter()
        .map(|d| {
            let last = d.last.map(red_ale_core::Date::to_file_string);
            json!({ "last": last, "remOccur": d.remaining, "instanceCount": d.instance_count })
        })
        .collect();
    assert_eq!(json!(deferred), g["deferred"]);
    let schedule: Vec<Value> = loan
        .schedule
        .iter()
        .map(|r| {
            let (period, weekend) = (r.period_type().as_str(), r.weekend_adjust().as_str());
            json!({ "mult": r.mult(), "periodType": period, "weekendAdjust": weekend, "start": r.start().to_file_string() })
        })
        .collect();
    assert_eq!(json!(schedule), g["schedule"]);
    assert_eq!(json!(loan.template_account.map(|a| a.to_string())), g["templateAccount"]["guid"]);
    let formulas: Vec<Value> = read
        .templates
        .splits()
        .filter(|(_, s)| s.account == loan.template_account)
        .filter_map(|(_, s)| TemplateSplit::of(s))
        .map(|t| json!([t.account().map(|a| a.to_string()), t.credit_formula(), t.debit_formula()]))
        .collect();
    let gnucash: Vec<Value> = g["splits"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| json!([s["account"], s["creditFormula"], s["debitFormula"]]))
        .collect();
    assert_eq!(formulas.len(), 3);
    assert!(formulas.iter().all(|f| gnucash.contains(f)), "{formulas:?} {gnucash:?}");
}

#[test]
fn scheduled_transactions_red_ale_saved_read_back_in_gnucash_unchanged() {
    let book = TempBook::copy("synthetic/rich.gnucash");
    script(&["make", &book.path_str()]);
    let expected = gnucash_scheduled(&book);
    let loan = &expected[LOAN];
    assert_eq!(loan["name"], "Loan & rates");
    assert_eq!(loan["occurrences"], json!([12, 7]));
    assert_eq!(loan["deferred"].as_array().map(Vec::len), Some(2));
    assert_eq!(loan["splits"].as_array().map(Vec::len), Some(3));
    assert_eq!(expected[RENT]["end"], "2025-07-01");
    assert_red_ale_reads(&book, &expected);

    let mut d = dispatcher();
    call(&mut d, "book.open", &json!({ "path": book.path_str() }));
    call(&mut d, "account.update", &json!({ "account": "Assets:Cash", "description": "petty" }));
    call(&mut d, "book.close", &json!({ "save": true }));
    let (text, _) = red_ale_xml::book::decode(&std::fs::read(&book.path).unwrap()).unwrap();
    assert!(text.contains("<act:description>petty</act:description>"), "the book was rewritten");
    assert_eq!(gnucash_scheduled(&book), expected);
}

/// red-ale's view of the questions `scheduled.py next` asks GnuCash about each scheduled transaction.
fn red_ale_next(sx: &red_ale_core::ScheduledTransaction) -> Value {
    use red_ale_core::ops::scheduled::{TemporalState, Upcoming, next_instance};
    let shown = |d: Option<red_ale_core::Date>| d.map(red_ale_core::Date::to_file_string);
    let steps: Vec<Value> = Upcoming::after(sx, TemporalState::of(sx))
        .take(15)
        .map(|o| o.unwrap())
        .map(|o| json!([o.date.to_file_string(), o.instance_count, o.before.remaining]))
        .collect();
    let deferred: Vec<Value> = sx
        .deferred
        .iter()
        .map(|d| json!(shown(next_instance(sx, Some(&TemporalState::of_deferred(d))).unwrap())))
        .collect();
    json!({ "null": shown(next_instance(sx, None).unwrap()), "steps": steps, "deferred": deferred })
}

/// `xaccSchedXactionGetNextInstance` (`5.17:libgnucash/engine/SchedXaction.cpp:885-926`) and
/// `gnc_sx_incr_temporal_state` (`:1044-1054`), called by GnuCash on books holding the fixture's rent, `make`'s loan
/// and `cases`' schedules (weekend adjustment, ended counts, end dates, deferred instances, two recurrences, once,
/// instance count 0, every period type, a leap day and a last occurrence before the start), agree with red-ale's next
/// occurrences, with no state, stepped from the temporal state, and from each deferred instance.
#[test]
fn next_instances_agree_with_gnucash() {
    let book = TempBook::copy("synthetic/rich.gnucash");
    script(&["make", &book.path_str()]);
    script(&["cases", &book.path_str()]);
    let gnucash: Value = serde_json::from_str(&script(&["next", &book.path_str()])).unwrap();
    let (read, _) = red_ale_xml::read_book(&std::fs::read(&book.path).unwrap()).unwrap();
    assert_eq!(read.scheduled.len(), 13);
    read.scheduled.values().for_each(|sx| {
        let expected = &gnucash[sx.guid.to_string()];
        assert_eq!(red_ale_next(sx), *expected, "{}", sx.name);
    });
    assert_eq!(gnucash["0000000000000000000000000000ca04"]["deferred"], json!(["2025-01-01"]), "the note's §9.2 Defer");
    assert_eq!(gnucash["0000000000000000000000000000ca01"]["null"], "2025-06-30", "the note's Fact 11");
}
