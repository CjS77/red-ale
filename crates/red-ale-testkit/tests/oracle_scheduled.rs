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
/// `ranges` are GnuCash's occurrence counts, keyed `start..end`, whose ranges red-ale counts too.
fn red_ale_next(sx: &red_ale_core::ScheduledTransaction, ranges: &Value) -> Value {
    use red_ale_core::ops::scheduled::{TemporalState, Upcoming, next_instance, occurrences_in_range};
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
    let counted: serde_json::Map<String, Value> = ranges
        .as_object()
        .unwrap()
        .keys()
        .map(|key| {
            let (start, end) = key.split_once("..").unwrap();
            let date = |d: &str| red_ale_core::Date::parse(d).unwrap();
            (key.clone(), json!(occurrences_in_range(sx, date(start), date(end)).unwrap()))
        })
        .collect();
    json!({ "null": shown(next_instance(sx, None).unwrap()), "steps": steps, "deferred": deferred, "ranges": counted })
}

/// `xaccSchedXactionGetNextInstance` (`5.17:libgnucash/engine/SchedXaction.cpp:885-926`) and
/// `gnc_sx_incr_temporal_state` (`:1044-1054`), called by GnuCash on books holding the fixture's rent, `make`'s loan
/// and `cases`' schedules (weekend adjustment, ended counts, end dates, deferred instances, two recurrences, once,
/// instance count 0, every period type, a leap day and a last occurrence before the start), agree with red-ale's next
/// occurrences, with no state, stepped from the temporal state, and from each deferred instance; and
/// `gnc_sx_get_num_occur_daterange` (`:739-812`), which the cash flow counts with, agrees with
/// `occurrences_in_range` over twelve ranges.
#[test]
fn next_instances_agree_with_gnucash() {
    // One book per command: two saves within a second would collide on GnuCash's timestamped backup name.
    let gnucash: Vec<Value> = [("make", 2), ("cases", 12)]
        .into_iter()
        .map(|(command, count)| {
            let book = TempBook::copy("synthetic/rich.gnucash");
            script(&[command, &book.path_str()]);
            let gnucash: Value = serde_json::from_str(&script(&["next", &book.path_str()])).unwrap();
            let (read, _) = red_ale_xml::read_book(&std::fs::read(&book.path).unwrap()).unwrap();
            assert_eq!(read.scheduled.len(), count);
            read.scheduled.values().for_each(|sx| {
                let expected = &gnucash[sx.guid.to_string()];
                assert_eq!(red_ale_next(sx, &expected["ranges"]), *expected, "{}", sx.name);
            });
            gnucash
        })
        .collect();
    assert_eq!(gnucash[1]["0000000000000000000000000000ca04"]["deferred"], json!(["2025-01-01"]), "the note's §9.2 Defer");
    assert_eq!(gnucash[1]["0000000000000000000000000000ca01"]["null"], "2025-06-30", "the note's Fact 11");
    let rent = &gnucash[0][RENT]["ranges"];
    let exp6 = ["2024-09-01..2024-12-31", "2024-08-01..2024-09-30", "2024-11-29..2024-11-29", "2024-12-01..2024-12-31", "2025-01-01..2026-12-31"];
    assert_eq!(exp6.map(|k| rent[k].clone()), [3, 0, 1, 0, 7].map(|n| json!(n)), "the engine-budgets note's exp6");
}

/// Since Last Run on the fixture's rent (monthly from 2024-08-01 adjusted back, last 2024-09-01, Instance count 2, a
/// deferred instance saved at 2024-09-01, 5 days to remind) on 2024-11-15, applied through a Session and saved: the
/// deferred instance ignored, October created and November postponed. GnuCash 5.17 reads back the created
/// transaction's fields and the scheduled transaction's temporal state as red-ale wrote them
/// (`gnc-sx-instance-model.c:1042-1058`, `:1310-1422`, `:1445-1554`), and its view of the whole book is red-ale's.
#[test]
fn gnucash_reads_back_the_instances_red_ale_applied() {
    use red_ale_core::formula::NumberFormat;
    use red_ale_core::ops::scheduled::{ApplyOptions, Choice, InstanceState, apply, since_last_run};
    use red_ale_core::{Date, DomainError, FixedEnv, Time64};
    use red_ale_service::{Session, Settings};

    let book = TempBook::copy("synthetic/rich.gnucash");
    let mut session = Session::open(&book.path_str(), false, Settings::default()).unwrap();
    session.set_env(FixedEnv::new(Time64(1_731_672_000), 0xfeed_0000_0000_0000_0000_0000_0000_0000));
    let today = Date::from_ymd(2024, 11, 15).unwrap();
    let proposal = session.query(|b, _| since_last_run(b, today, NumberFormat::default())).unwrap();
    let shown: Vec<Value> = proposal
        .instances()
        .map(|(_, i)| json!([i.date.to_file_string(), i.state.as_str(), i.instance_count()]))
        .collect();
    let expected = [json!(["2024-10-01", "postponed", 3]), json!(["2024-10-01", "toCreate", 2]), json!(["2024-11-01", "toCreate", 3])];
    assert_eq!(shown, expected);
    let choice = |index, state| Choice {
        index,
        state: Some(state),
        ..Choice::default()
    };
    let choices = [choice(0, InstanceState::Ignored), choice(2, InstanceState::Postponed)];
    let applied = session
        .mutate(|b, env| apply(b, env, &proposal, &choices, ApplyOptions::default()).map_err(|e| DomainError::invalid(e.to_string())))
        .unwrap();
    session.save(None).unwrap();
    session.close(false).unwrap();
    let created = applied.created[0].transactions[0].transaction.to_string();

    let rent = &gnucash_scheduled(&book)[RENT];
    assert_eq!(json!([rent["last"], rent["instanceCount"]]), json!(["2024-11-01", 4]));
    assert_eq!(rent["deferred"], json!([{ "last": "2024-10-01", "remOccur": 0, "instanceCount": 3 }]));
    let transactions: Value = serde_json::from_str(&script(&["created", &book.path_str()])).unwrap();
    assert_eq!(transactions.as_object().map(serde_json::Map::len), Some(1));
    let t = &transactions[&created];
    assert_eq!(t["scheduledTransaction"], RENT);
    assert_eq!(json!([t["description"], t["num"], t["notes"], t["currency"]]), json!(["Rent", "", null, "CURRENCY::USD"]));
    let dates = json!([t["datePosted"], t["datePostedGDate"], t["dateEntered"]]);
    assert_eq!(dates, json!([1_727_780_340, "2024-10-01", 1_731_672_000]));
    let split = |account: &str, value: &str| json!({ "account": account, "memo": "", "action": "", "value": value, "amount": value });
    let food = split("1feca2abf92347288c268ebf93753ec4", "45600/100");
    assert_eq!(t["splits"], json!([food, split("c080059e1aec4c0b9ad4671e80965686", "-45600/100")]));

    let gnucash = oracle::dump(&book.path).unwrap();
    let (ours, _) = red_ale_xml::read_book(&std::fs::read(&book.path).unwrap()).unwrap();
    let skip: Vec<&str> = red_ale_testkit::model::NOT_YET_COMPUTED.to_vec();
    assert_eq!(red_ale_testkit::diff(&gnucash, &red_ale_testkit::model::dump(&ours), &skip), Vec::<String>::new());
}

/// Since Last Run over JSON-RPC on the rent with its 456 made a variable `rent` (and its numerics 0, as GnuCash's
/// register writes them for a formula with a variable), on 2024-11-28: the deferred instance ignored, October and
/// November created with their own `rent`, the reminder for November 29th left alone; then saved. GnuCash 5.17 reads
/// the two created transactions with the amounts given, the scheduled transaction advanced past November with its
/// deferred list emptied, and the whole book as red-ale does.
#[test]
fn gnucash_reads_the_instances_since_last_run_created_over_rpc() {
    let book = TempBook::copy("synthetic/rich.gnucash");
    let text = std::fs::read_to_string(&book.path)
        .unwrap()
        .replace("<slot:value type=\"string\">456</slot:value>", "<slot:value type=\"string\">rent</slot:value>")
        .replace("<slot:value type=\"numeric\">456/1</slot:value>", "<slot:value type=\"numeric\">0/1</slot:value>");
    std::fs::write(&book.path, text).unwrap();
    let mut d = red_ale_rpc::Dispatcher::new(red_ale_service::Settings::default());
    let nov_28 = red_ale_core::Time64(1_732_795_200);
    d.set_env_factory(Box::new(move || Box::new(red_ale_core::FixedEnv::new(nov_28, 0x51 << 100))));
    call(&mut d, "book.open", &json!({ "path": book.path_str() }));
    let proposal = call(&mut d, "scheduledTransaction.sinceLastRun", &json!({}));
    assert_eq!(proposal["summary"]["needVariables"], 2);
    let choices = json!([
        { "index": 0, "state": "ignored" },
        { "index": 1, "variables": { "rent": "456.10" } },
        { "index": 2, "variables": { "rent": "470.05" } },
    ]);
    let applied = call(&mut d, "scheduledTransaction.apply", &json!({ "proposal": proposal["id"], "choices": choices }));
    assert_eq!(applied["reminders"], json!([3]));
    let created: Vec<String> = applied["created"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["transactions"][0].as_str().unwrap().to_owned())
        .collect();
    call(&mut d, "book.close", &json!({ "save": true }));

    let rent = &gnucash_scheduled(&book)[RENT];
    assert_eq!(json!([rent["last"], rent["instanceCount"], rent["deferred"]]), json!(["2024-11-01", 4, []]));
    let transactions: Value = serde_json::from_str(&script(&["created", &book.path_str()])).unwrap();
    assert_eq!(transactions.as_object().map(serde_json::Map::len), Some(2));
    let read = |guid: &str| {
        let t = &transactions[guid];
        json!([t["scheduledTransaction"], t["datePostedGDate"], t["splits"][0]["value"], t["splits"][1]["amount"]])
    };
    assert_eq!(read(&created[0]), json!([RENT, "2024-10-01", "45610/100", "-45610/100"]));
    assert_eq!(read(&created[1]), json!([RENT, "2024-11-01", "47005/100", "-47005/100"]));

    let gnucash = oracle::dump(&book.path).unwrap();
    let (ours, _) = red_ale_xml::read_book(&std::fs::read(&book.path).unwrap()).unwrap();
    let skip: Vec<&str> = red_ale_testkit::model::NOT_YET_COMPUTED.to_vec();
    assert_eq!(red_ale_testkit::diff(&gnucash, &red_ale_testkit::model::dump(&ours), &skip), Vec::<String>::new());
}

/// What `scheduled.py dump` prints for a scheduled transaction but its splits, built from red-ale's
/// `scheduledTransaction.get` and, for the Schedule's names, the scheduled transaction red-ale reads from the file.
fn as_gnucash_dumps(info: &Value, sx: &red_ale_core::ScheduledTransaction) -> Value {
    let end = &info["end"];
    let occurrences = end.get("occurrences").map_or(Value::Null, |n| json!([n, end["remaining"]]));
    let schedule: Vec<Value> = sx
        .schedule
        .iter()
        .map(|r| {
            let (period, weekend) = (r.period_type().as_str(), r.weekend_adjust().as_str());
            json!({ "mult": r.mult(), "periodType": period, "weekendAdjust": weekend, "start": r.start().to_file_string() })
        })
        .collect();
    assert_eq!(Some(sx.schedule.len()), info["schedule"].as_array().map(Vec::len));
    json!({
        "name": info["name"], "enabled": info["enabled"], "autoCreate": info["autoCreate"], "autoCreateNotify": info["notify"],
        "advanceCreateDays": info["advanceCreateDays"], "advanceRemindDays": info["advanceRemindDays"],
        "instanceCount": info["instanceCount"], "start": info["startDate"], "last": info["lastOccurrence"],
        "end": end.get("date").cloned().unwrap_or(Value::Null), "occurrences": occurrences,
        "templateAccount": { "guid": info["templateAccount"], "name": info["guid"] },
        "schedule": schedule, "deferred": [],
    })
}

/// Each template split as `scheduled.py dump` prints it, keyed by split GUID, from red-ale's `scheduledTransaction.get`.
fn template_splits(info: &Value) -> std::collections::BTreeMap<String, Value> {
    let numeric = |n: &Value| json!(format!("{}/{}", n["num"].as_str().unwrap(), n["denom"].as_str().unwrap()));
    info["templateTransactions"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|t| {
            t["splits"].as_array().unwrap().iter().map(move |s| {
                let split = json!({
                    "transaction": t["guid"], "description": t["description"], "num": t["num"].as_str().unwrap_or(""),
                    "currency": t["currency"], "split": s["guid"], "memo": s["memo"].as_str().unwrap_or(""), "value": "0/100",
                    "amount": "0/1", "templateAccount": info["templateAccount"], "account": s["account"],
                    "creditFormula": s["creditFormula"], "debitFormula": s["debitFormula"],
                    "creditNumeric": numeric(&s["creditNumeric"]), "debitNumeric": numeric(&s["debitNumeric"]), "shares": null,
                });
                (s["guid"].as_str().unwrap().to_owned(), split)
            })
        })
        .collect()
}

/// Scheduled transactions red-ale made, saved: GnuCash 5.17 reads each as red-ale wrote it, through its engine
/// getters (`xaccSchedXaction*`, the template account as `xaccSchedXactionInit` makes it,
/// `5.17:libgnucash/engine/SchedXaction.cpp:393-406`, and the template splits' `sched-xaction` properties): one made
/// with `create` (a variable, a marker split, an occurrence count), one with `fromTransaction` (quarterly, ending on
/// a date) and a loan with escrow and taxes, six payments made. `xaccSchedXactionGetNextInstance` and
/// `gnc_sx_incr_temporal_state` agree with red-ale's occurrences for each (`:885-926`, `:1044-1054`), and GnuCash's
/// view of the whole book is red-ale's.
#[test]
fn gnucash_reads_the_scheduled_transactions_red_ale_made() {
    let book = TempBook::copy("synthetic/rich.gnucash");
    let mut d = dispatcher();
    call(&mut d, "book.open", &json!({ "path": book.path_str() }));
    let made = call(&mut d, "scheduledTransaction.create", &json!({
        "name": "Groceries & more", "startDate": "2024-07-31", "end": { "occurrences": 6, "remaining": 5 }, "advanceRemindDays": 3,
        "schedule": [{ "mult": 1, "periodType": "endOfMonth", "weekendAdjust": "back" }, { "mult": 2, "periodType": "week" }],
        "templateTransactions": [{
            "description": "Shop", "num": "7", "notes": "weekly",
            "splits": [
                { "account": "Expenses:Food", "memo": "food", "debitFormula": "amount * 2 + i" },
                { "account": "Assets:Cash", "creditFormula": "amount * 2 + i" },
                { "account": "Assets:Cash", "memo": "marker" },
            ],
        }],
    }));
    let from = call(&mut d, "scheduledTransaction.fromTransaction", &json!({
        "transaction": "f485efcd743943ddb6eede20c1d22c05", "frequency": "quarterly", "end": { "date": "2026-06-30" },
    }));
    let loan = call(&mut d, "scheduledTransaction.loan", &json!({
        "name": "House", "loanAccount": "Liabilities", "principal": "100000", "rate": "6", "startDate": "2024-01-01",
        "length": { "years": 30 }, "monthsRemaining": 354, "escrow": "Assets:NonStd",
        "repayment": { "from": "Assets:Cash", "interestTo": "Expenses:Fees", "start": "2024-07-01" },
        "otherPayments": [{ "kind": "taxes", "amount": "250.75", "to": "Expenses:Fees" }],
    }))["scheduledTransaction"]
        .clone();
    let ours: Vec<Value> = [&made, &from, &loan]
        .iter()
        .map(|sx| call(&mut d, "scheduledTransaction.get", &json!({ "scheduledTransaction": sx["guid"] })))
        .collect();
    call(&mut d, "book.close", &json!({ "save": true }));

    let (read, _) = red_ale_xml::read_book(&std::fs::read(&book.path).unwrap()).unwrap();
    let core = |info: &Value| &read.scheduled[&Guid::parse(info["guid"].as_str().unwrap()).unwrap()];
    let gnucash = gnucash_scheduled(&book);
    assert_eq!(gnucash.as_object().map(serde_json::Map::len), Some(4), "the rent and the three made");
    ours.iter().for_each(|info| {
        let mut fields = gnucash[info["guid"].as_str().unwrap()].clone();
        let splits = fields.as_object_mut().unwrap().remove("splits").unwrap();
        assert_eq!(fields, as_gnucash_dumps(info, core(info)), "{}", info["name"]);
        let read: std::collections::BTreeMap<String, Value> =
            splits.as_array().unwrap().iter().map(|s| (s["split"].as_str().unwrap().to_owned(), s.clone())).collect();
        assert_eq!(read, template_splits(info), "{}", info["name"]);
    });
    assert_eq!(gnucash[loan["guid"].as_str().unwrap()]["splits"].as_array().map(Vec::len), Some(7), "two templates");

    let next: Value = serde_json::from_str(&script(&["next", &book.path_str()])).unwrap();
    ours.iter().for_each(|info| {
        let sx = core(info);
        let expected = &next[info["guid"].as_str().unwrap()];
        assert_eq!(red_ale_next(sx, &expected["ranges"]), *expected, "{}", sx.name);
        assert_eq!(expected["steps"][0][0], info["nextOccurrence"], "{}", sx.name);
    });
    assert_eq!(next[loan["guid"].as_str().unwrap()]["steps"][0], json!(["2024-07-01", 7, 0]), "six payments made");

    let whole = oracle::dump(&book.path).unwrap();
    let skip: Vec<&str> = red_ale_testkit::model::NOT_YET_COMPUTED.to_vec();
    assert_eq!(red_ale_testkit::diff(&whole, &red_ale_testkit::model::dump(&read), &skip), Vec::<String>::new());
}

/// The first scheduled transaction of a book that had none: red-ale makes the template root as GnuCash does, with no
/// commodity (`5.17:libgnucash/engine/SX-book.cpp:113-125`), and saves it. GnuCash 5.17 reads the scheduled
/// transaction and its template splits as red-ale wrote them, and its view of the whole book is red-ale's.
#[test]
fn gnucash_reads_the_first_scheduled_transaction_of_a_book() {
    let book = TempBook::new_path();
    std::fs::write(&book.path, red_ale_testkit::synthetic::synthetic_book_text(10)).unwrap();
    let mut d = dispatcher();
    call(&mut d, "book.open", &json!({ "path": book.path_str() }));
    let made = call(&mut d, "scheduledTransaction.create", &json!({
        "name": "Daily", "startDate": "2024-06-01", "schedule": [{ "mult": 1, "periodType": "day" }], "end": { "occurrences": 5 },
        "templateTransactions": [{
            "description": "Scheduled",
            "splits": [
                { "account": "Expenses:Expenses 0000", "debitFormula": "12.34" },
                { "account": "Assets:Assets 0000", "creditFormula": "12.34" },
            ],
        }],
    }));
    let info = call(&mut d, "scheduledTransaction.get", &json!({ "scheduledTransaction": made["guid"] }));
    call(&mut d, "book.close", &json!({ "save": true }));

    let (read, _) = red_ale_xml::read_book(&std::fs::read(&book.path).unwrap()).unwrap();
    let guid = info["guid"].as_str().unwrap();
    let gnucash = gnucash_scheduled(&book);
    let mut fields = gnucash[guid].clone();
    let splits = fields.as_object_mut().unwrap().remove("splits").unwrap();
    assert_eq!(fields, as_gnucash_dumps(&info, &read.scheduled[&Guid::parse(guid).unwrap()]));
    let splits: std::collections::BTreeMap<String, Value> =
        splits.as_array().unwrap().iter().map(|s| (s["split"].as_str().unwrap().to_owned(), s.clone())).collect();
    assert_eq!(splits, template_splits(&info));
    let whole = oracle::dump(&book.path).unwrap();
    let skip: Vec<&str> = red_ale_testkit::model::NOT_YET_COMPUTED.to_vec();
    assert_eq!(red_ale_testkit::diff(&whole, &red_ale_testkit::model::dump(&read), &skip), Vec::<String>::new());
}
