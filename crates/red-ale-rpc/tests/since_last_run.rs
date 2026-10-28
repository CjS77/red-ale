//! Since Last Run through `scheduledTransaction.sinceLastRun`, `apply` and `discard` on the rent of
//! `fixtures/synthetic/rich.gnucash` (`design/api/scheduled-transactions.md`, "Since Last Run" and "Tests"): monthly
//! from 2024-08-01 adjusted back off weekends, last occurrence 2024-09-01, Instance count 2, a Deferred instance saved
//! at 2024-09-01, 5 days to remind, ending 2025-07-01, 456.00 from Checking to Food.
//!
//! The rich fixture's SQLite copy has the same rent without its Deferred instance, which GnuCash's SQL backend drops
//! (`design/sql-fidelity.md` §8); postponing an instance there is refused.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::needless_pass_by_value)]

use red_ale_core::{FixedEnv, Time64};
use red_ale_rpc::{Dispatcher, RpcError, codes};
use red_ale_service::Settings;
use serde_json::{Value, json};

/// 2024-11-28 12:00 UTC.
const NOV_28: i64 = 1_732_795_200;
const RENT: &str = "52f96837087e9670c933f9421b0ac626";
const FOOD: &str = "1feca2abf92347288c268ebf93753ec4";

/// The rich fixture copied into a temporary directory, its rent's debit and credit formulas replaced, and opened in a
/// dispatcher whose clock reads `now`.
struct Book {
    dir: tempfile::TempDir,
    d: Dispatcher,
}

impl Book {
    fn open(now: i64, debit: &str, credit: &str) -> Self {
        Self::open_with(now, debit, credit, false)
    }

    fn open_with(now: i64, debit: &str, credit: &str, read_only: bool) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("book.gnucash");
        let fixture = format!("{}/../../fixtures/synthetic/rich.gnucash", env!("CARGO_MANIFEST_DIR"));
        // The first `456` is the debit split's debit formula, the second the credit split's credit formula.
        let formula = |f: &str| format!("<slot:value type=\"string\">{f}</slot:value>");
        let text = std::fs::read_to_string(fixture)
            .unwrap()
            .replacen(&formula("456"), &formula("@debit"), 1)
            .replacen(&formula("456"), &formula("@credit"), 1)
            .replacen(&formula("@debit"), &formula(debit), 1)
            .replacen(&formula("@credit"), &formula(credit), 1);
        std::fs::write(&path, text).unwrap();
        let mut d = Dispatcher::new(Settings::default());
        d.set_env_factory(Box::new(move || Box::new(FixedEnv::new(Time64(now), 0x5c << 100))));
        d.call("book.open", json!({ "path": path.display().to_string(), "readOnly": read_only }))
            .unwrap();
        Self { dir, d }
    }

    fn rent(now: i64) -> Self {
        Self::open(now, "456", "456")
    }

    fn ok(&mut self, method: &str, params: Value) -> Value {
        self.d
            .call(method, params.clone())
            .unwrap_or_else(|e| panic!("{method} {params}: {e} {:?}", e.data))
    }

    fn err(&mut self, method: &str, params: Value) -> RpcError {
        match self.d.call(method, params.clone()) {
            Ok(v) => panic!("{method} {params} should fail but returned {v}"),
            Err(e) => e,
        }
    }

    fn propose(&mut self) -> Value {
        self.ok("scheduledTransaction.sinceLastRun", json!({}))
    }

    fn apply(&mut self, proposal: &Value, choices: Value) -> Result<Value, RpcError> {
        self.d.call(
            "scheduledTransaction.apply",
            json!({ "proposal": proposal["id"], "choices": choices }),
        )
    }

    fn transactions(&mut self) -> Value {
        self.ok("book.info", json!({}))["counts"]["transactions"].clone()
    }
}

/// Each instance as `[date, state, i]`.
fn instances(proposal: &Value) -> Vec<Value> {
    proposal["schedules"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|s| s["instances"].as_array().unwrap().iter())
        .map(|i| json!([i["date"], i["state"], i["instanceCount"]]))
        .collect()
}

fn instance(proposal: &Value, index: usize) -> &Value {
    proposal["schedules"][0]["instances"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["index"] == index)
        .unwrap()
}

fn cents(n: i64) -> Value {
    json!({ "num": n.to_string(), "denom": "100" })
}

fn reason(e: &RpcError) -> (i32, Value) {
    (e.code, e.data.clone().unwrap_or_default()["reason"].clone())
}

#[test]
fn proposes_then_creates_postpones_and_ignores_as_one_mutation() {
    let mut b = Book::rent(NOV_28);
    let p = b.propose();
    assert_eq!(p["today"], "2024-11-28");
    assert_eq!(p["thresholdDate"], Value::Null);
    let expected = [
        json!(["2024-10-01", "postponed", 3]),
        json!(["2024-10-01", "toCreate", 2]),
        json!(["2024-11-01", "toCreate", 3]),
        json!(["2024-11-29", "reminder", 4]),
    ];
    assert_eq!(instances(&p), expected, "December 1st is a Sunday, moved back to Friday");
    let summary =
        json!({ "toCreate": 2, "reminders": 1, "postponed": 1, "autoCreate": 0, "needVariables": 0, "readOnlyByDate": 0, "errors": 0 });
    assert_eq!(p["summary"], summary);
    assert_eq!(
        json!([p["schedules"][0]["scheduledTransaction"], p["schedules"][0]["name"]]),
        json!([RENT, "Monthly rent"])
    );
    let october = instance(&p, 1);
    assert_eq!(
        october["variables"],
        json!([{ "name": "i", "value": { "num": "2", "denom": "1" }, "editable": false }])
    );
    let preview = &october["transactions"][0];
    assert_eq!(
        json!([preview["description"], preview["currency"], preview["imbalance"]]),
        json!(["Rent", "CURRENCY::USD", null])
    );
    assert_eq!(preview["splits"][0]["account"], FOOD);
    assert_eq!(
        json!([preview["splits"][0]["value"], preview["splits"][1]["value"]]),
        json!([cents(45600), cents(-45600)])
    );
    assert_eq!(b.transactions(), 7, "proposing changes nothing");

    let applied = b
        .apply(
            &p,
            json!([{ "index": 0, "state": "ignored" }, { "index": 2, "state": "postponed" }]),
        )
        .unwrap();
    assert_eq!(
        json!([applied["postponed"], applied["ignored"], applied["reminders"]]),
        json!([[2], [0], [3]])
    );
    let created = &applied["created"][0];
    assert_eq!(
        json!([created["index"], created["date"], created["scheduledTransaction"]]),
        json!([1, "2024-10-01", RENT])
    );
    let state = json!([{
        "scheduledTransaction": RENT, "lastOccurrence": "2024-11-01", "instanceCount": 4, "end": { "date": "2025-07-01" },
        "deferredInstances": [{ "lastOccurrence": "2024-10-01", "remainingOccurrences": 0, "instanceCount": 3 }],
    }]);
    assert_eq!(
        applied["schedules"], state,
        "the deferred instance proposed left the list and November's joined it"
    );
    assert_eq!(json!([applied["imbalanceSplits"], applied["warnings"]]), json!([[], []]));
    let txn = b.ok("transaction.get", json!({ "guid": created["transactions"][0] }));
    assert_eq!(
        json!([txn["description"], txn["datePosted"], txn["splits"][0]["value"]]),
        json!(["Rent", "2024-10-01", cents(45600)])
    );
    assert_eq!(b.transactions(), 8);

    let journal = b.ok("book.info", json!({}))["journal"].as_str().unwrap().to_owned();
    let guid = created["transactions"][0].as_str().unwrap();
    assert!(
        std::fs::read_to_string(journal).unwrap().contains(guid),
        "the journal records the created transaction"
    );

    let again = b.propose();
    assert_eq!(again["summary"]["toCreate"], 0, "run again the same day, nothing is to create");
    assert_eq!(
        instances(&again),
        [json!(["2024-11-01", "postponed", 3]), json!(["2024-11-29", "reminder", 4])]
    );
    assert!(b.dir.path().join("book.gnucash").exists());
}

#[test]
fn held_proposals_are_refused_once_stale_applied_or_discarded() {
    let mut b = Book::rent(NOV_28);
    let p = b.propose();
    assert!(p["id"].as_str().unwrap().starts_with("slr-"), "{}", p["id"]);
    b.ok("account.update", json!({ "account": FOOD, "description": "groceries" }));
    let stale = b.apply(&p, json!([])).unwrap_err();
    assert_eq!(reason(&stale), (codes::CONFLICT, json!("book-revision")));
    assert_eq!(
        json!([
            stale.data.as_ref().unwrap()["proposalRevision"],
            stale.data.as_ref().unwrap()["bookRevision"]
        ]),
        json!([0, 1])
    );

    let p = b.propose();
    assert_eq!(p["bookRevision"], 1);
    assert_eq!(
        b.ok("scheduledTransaction.discard", json!({ "proposal": p["id"] })),
        json!({ "discarded": true })
    );
    let gone = b.apply(&p, json!([])).unwrap_err();
    assert_eq!(
        (gone.code, gone.data.unwrap()["kind"].clone()),
        (codes::NOT_FOUND, json!("sinceLastRunProposal"))
    );
    assert_eq!(
        b.err("scheduledTransaction.discard", json!({ "proposal": p["id"] })).code,
        codes::NOT_FOUND
    );
    let other = b.propose();
    assert_eq!(
        b.err("import.discard", json!({ "proposal": other["id"] })).code,
        codes::NOT_FOUND,
        "import holds its own"
    );

    let held: Vec<Value> = (0..17).map(|_| b.propose()).collect();
    assert_eq!(
        b.apply(&held[0], json!([])).unwrap_err().code,
        codes::NOT_FOUND,
        "sixteen newer pushed it out"
    );
    b.apply(&held[16], json!([])).unwrap();
    assert_eq!(b.apply(&held[16], json!([])).unwrap_err().code, codes::NOT_FOUND, "applied once");
}

#[test]
fn choices_that_break_the_rules_are_refused_and_change_nothing() {
    let mut b = Book::rent(NOV_28);
    let p = b.propose();
    let refused = |b: &mut Book, choices: Value| reason(&b.apply(&p, choices).unwrap_err());
    let invalid = |r: &str| (codes::INVALID, json!(r));
    let e = b.apply(&p, json!([{ "index": 1, "state": "reminder" }])).unwrap_err();
    assert_eq!(reason(&e), invalid("reminder-first"));
    assert_eq!(e.data.unwrap()["indexes"], json!([1, 2]), "the reminder and the instance after it");
    assert_eq!(refused(&mut b, json!([{ "index": 9 }])), invalid("index"));
    assert_eq!(
        refused(&mut b, json!([{ "index": 1, "state": "created" }])),
        invalid("created-state")
    );
    assert_eq!(
        refused(&mut b, json!([{ "index": 1 }, { "index": 1 }])),
        invalid("duplicate-choice")
    );
    assert_eq!(
        refused(&mut b, json!([{ "index": 1, "variables": { "i": "7" } }])),
        invalid("not-editable")
    );
    assert_eq!(
        refused(&mut b, json!([{ "index": 1, "variables": { "rent": "7" } }])),
        invalid("unknown-variable")
    );
    let auto_only = json!({ "proposal": p["id"], "choices": [{ "index": 1 }], "autoCreateOnly": true });
    assert_eq!(reason(&b.err("scheduledTransaction.apply", auto_only)), invalid("not-auto-create"));
    let none = b.ok("scheduledTransaction.apply", json!({ "proposal": p["id"], "autoCreateOnly": true }));
    assert_eq!(
        json!([none["created"], none["schedules"]]),
        json!([[], []]),
        "the rent is not auto-create"
    );
    assert_eq!(b.transactions(), 7);
}

#[test]
fn variables_are_given_per_instance() {
    let mut b = Book::open(NOV_28, "rent", "rent");
    let p = b.propose();
    assert_eq!(p["summary"]["needVariables"], 2);
    let october = instance(&p, 1);
    assert_eq!(october["variables"][1], json!({ "name": "rent", "value": null, "editable": true }));
    assert_eq!(october["transactions"], Value::Null);
    let e = b.apply(&p, json!([{ "index": 1, "variables": { "rent": "500.25" } }])).unwrap_err();
    assert_eq!(json!([reason(&e).1, e.data.unwrap()["index"]]), json!(["unbound-variable", 2]));
    let choices =
        json!([{ "index": 1, "variables": { "rent": "500.25" } }, { "index": 2, "variables": { "rent": { "num": "510", "denom": "1" } } }]);
    let applied = b.apply(&p, choices).unwrap();
    let values: Vec<Value> = applied["created"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| b.ok("transaction.get", json!({ "guid": c["transactions"][0] }))["splits"][0]["value"].clone())
        .collect();
    assert_eq!(values, [cents(50025), cents(51000)]);
}

#[test]
fn a_failing_formula_refuses_the_apply_and_names_the_instance() {
    let mut b = Book::open(NOV_28, "100 / rent", "100 / rent");
    let p = b.propose();
    let e = b
        .apply(
            &p,
            json!([{ "index": 1, "variables": { "rent": "0" } }, { "index": 2, "variables": { "rent": "4" } }]),
        )
        .unwrap_err();
    assert_eq!(e.code, codes::FORMULA);
    let data = e.data.unwrap();
    let expected = json!(["division-by-zero", "100 / rent", "debit", 1, RENT, null]);
    assert_eq!(
        json!([
            data["reason"],
            data["formula"],
            data["side"],
            data["index"],
            data["scheduledTransaction"],
            data["position"]
        ]),
        expected
    );
    assert_eq!(b.transactions(), 7, "nothing is created");
    let ok = b
        .apply(
            &p,
            json!([{ "index": 1, "state": "ignored" }, { "index": 2, "variables": { "rent": "4" } }]),
        )
        .unwrap();
    assert_eq!(ok["created"][0]["index"], 2, "the proposal stays held after a failed apply");

    let mut b = Book::open(NOV_28, "456 +", "456");
    let p = b.propose();
    let errors = &instance(&p, 1)["errors"];
    assert_eq!(
        json!([errors[0]["code"], errors[0]["data"]["reason"], errors[0]["data"]["side"]]),
        json!([6000, "syntax", "debit"])
    );
    assert_eq!(p["summary"]["errors"], 4, "every instance of the schedule");
    assert_eq!(
        b.apply(&p, json!([{ "index": 0, "state": "ignored" }])).unwrap_err().code,
        codes::FORMULA
    );
}

#[test]
fn rounding_adds_an_imbalance_split_and_says_so() {
    let mut b = Book::open(NOV_28, "1.005", "1");
    let p = b.propose();
    let imbalance = &instance(&p, 1)["transactions"][0]["imbalance"];
    assert_eq!(
        *imbalance,
        json!({ "value": cents(-1), "currency": "CURRENCY::USD", "account": "Imbalance-USD" })
    );
    let applied = b.apply(&p, json!([{ "index": 2, "state": "ignored" }])).unwrap();
    assert_eq!(
        json!([applied["imbalanceSplits"][0]["index"], applied["imbalanceSplits"][0]["value"]]),
        json!([1, cents(-1)])
    );
    assert_eq!(applied["warnings"][0]["code"], "imbalance");
}

#[test]
fn instances_before_the_read_only_threshold_need_an_explicit_choice() {
    let mut b = Book::rent(NOV_28);
    b.ok("book.setOptions", json!({ "autoReadOnlyDays": 30 }));
    let p = b.propose();
    assert_eq!(p["thresholdDate"], "2024-10-29");
    assert_eq!(
        json!([instance(&p, 1)["readOnlyByDate"], instance(&p, 2)["readOnlyByDate"]]),
        json!([true, false])
    );
    assert_eq!(p["summary"]["readOnlyByDate"], 2, "the postponed instance too");
    let e = b.apply(&p, json!([])).unwrap_err();
    assert_eq!(json!([reason(&e).1, e.data.unwrap()["index"]]), json!(["read-only-by-date", 1]));
    let applied = b.apply(&p, json!([{ "index": 1, "state": "toCreate" }])).unwrap();
    assert_eq!(applied["created"].as_array().map(Vec::len), Some(2));
    let warnings: Vec<Value> = applied["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|w| json!([w["code"], w["index"]]))
        .collect();
    assert_eq!(warnings, [json!(["read-only-by-date", 1])]);
}

#[test]
fn a_read_only_book_proposes_but_never_applies() {
    let mut b = Book::open_with(NOV_28, "456", "456", true);
    let p = b.propose();
    assert_eq!(p["summary"]["toCreate"], 2);
    assert_eq!(b.apply(&p, json!([])).unwrap_err().code, codes::READ_ONLY);
}

/// An SQLite book has no place for postponed instances, so choosing one is refused and nothing changes; creating
/// and ignoring still apply (`design/sql-fidelity.md` §8; the engine's `can_postpone`).
#[test]
fn an_sqlite_book_refuses_postponing_an_instance() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("book.gnucash");
    std::fs::copy(
        format!("{}/../../fixtures/sqlite/synthetic/rich.gnucash", env!("CARGO_MANIFEST_DIR")),
        &path,
    )
    .unwrap();
    let mut d = Dispatcher::new(Settings::default());
    d.set_env_factory(Box::new(|| Box::new(FixedEnv::new(Time64(NOV_28), 0x5c << 100))));
    let info = d.call("book.open", json!({ "path": path.display().to_string() })).unwrap();
    assert_eq!(json!([info["format"], info["readOnly"]]), json!(["sqlite", false]));
    let p = d.call("scheduledTransaction.sinceLastRun", json!({})).unwrap();
    assert!(
        instances(&p).iter().all(|i| i[1] != "postponed"),
        "the SQLite copy holds no Deferred instance"
    );
    let choose = |state: &str| json!({ "proposal": p["id"], "choices": [{ "index": 0, "state": state }] });
    let err = d.call("scheduledTransaction.apply", choose("postponed")).unwrap_err();
    assert_eq!(reason(&err), (codes::INVALID, json!("sql-deferred-instances")));
    assert_eq!(d.call("book.info", json!({})).unwrap()["dirty"], false);
    d.call("scheduledTransaction.apply", choose("ignored")).unwrap();
    d.call("book.save", json!({})).unwrap();
}
