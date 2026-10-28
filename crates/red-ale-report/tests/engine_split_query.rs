//! The split query's states, closing match, sorts and unique transactions, and the reads the Transaction Report
//! engine adds (`design/reports.md` §5; `design/notes/report-transaction-engine.md` §3.1), on
//! `fixtures/reports/test-data.gnucash`, the book of `create-test-data`
//! (`5.17:bindings/guile/test/test-engine-extras.scm:389-480`, built by `scripts/report-fixtures/account_summary.py`),
//! and on `fixtures/synthetic/business.gnucash` for lots and invoices.
//!
//! In 1970 Bank has four splits not voided: trn3 (31 Dec 1969, +29, cleared 1 Mar 1970, from Income), trn1 (1 Jan,
//! -5 to Expenses), trn4 (1 Feb, -15 to Expenses) and trn7 (10 Feb, -8 to Liabilities, reconciled 1 Mar). trn2 is
//! voided; so is trn5, Liabilities' -10 to Expenses.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::sync::Arc;

use common::{GUID, Host, fixture, registry, template};
use red_ale_core::SlotValue;
use red_ale_core::ops::doc_links::DOC_LINK;
use serde_json::json;

const QUERY: &str = r#"
    let bank = book.account("Root:Asset:Bank");
    let liabilities = book.account("Root:Liabilities");
    let from = -86400; let to = 31535999;
    let nums = |splits| splits.map(|s| s.transaction.num).reduce(|a, b| a + "," + b, "");
    let q = |extra| { let q = #{ accounts: [bank], from: from, to: to }; q.mixin(extra); book.split_query(q) };
    print(`default=${nums.call(q.call(#{}))}`);
    print(`num-down=${nums.call(q.call(#{ sort: [#{ key: "num", ascending: false }] }))}`);
    print(`value=${nums.call(q.call(#{ sort: [#{ key: "value" }] }))}`);
    print(`reconciled=${nums.call(q.call(#{ sort: [#{ key: "dateReconciled", ascending: false }, #{ key: "num" }] }))}`);
    print(`corr=${nums.call(q.call(#{ sort: [#{ key: "corrAccountFullName" }, #{ key: "none" }, #{ key: "default" }] }))}`);
    print(`none=${nums.call(q.call(#{ sort: [#{ key: "none" }, #{ key: "num", ascending: false }] }))}`);
    print(`cleared=${nums.call(q.call(#{ reconcile: ["c", "y"] }))}`);
    let all = book.split_query(#{ accounts: [liabilities], reconcile: ["n", "c", "y", "f", "v"] });
    let voided = book.split_query(#{ accounts: [liabilities], reconcile: ["v"] });
    print(`states=${nums.call(all)} void=${nums.call(voided)} former=${voided[0].voidFormerAmount} ` +
          `${voided[0].voidFormerValue} ${voided[0].amount} ${all[1].voidFormerAmount}`);
    let equity = book.account("Root:Equity");
    print(`closing=${book.split_query(#{ accounts: [equity], isClosing: true }).len()} ` +
          `${book.split_query(#{ accounts: [equity], isClosing: false }).len()}`);
    let day = book.split_query(#{ accounts: [bank, "Root:Expenses"], from: 0, to: 86399 });
    let unique = book.split_query(#{ accounts: [bank, "Root:Expenses"], from: 0, to: 86399, unique: true });
    print(`unique=${day.len()} ${unique.len()} ${unique[0] == day[0]}`);
    print(`num-source=${book.uses_split_action_for_num()}`);
"#;

const READS: &str = r#"
    let bank = book.account("Root:Asset:Bank");
    let s = bank.splits_between(0, 86399)[0];
    print(`other=${s.otherSplit.account.fullName} ${s.otherSplit.otherSplit == s} [${s.corrAccountFullName}] [${s.corrAccountCode}]`);
    let multi = bank.splits().filter(|x| x.transaction.splitCount == 3)[0];
    print(`multi=${multi.otherSplit} [${multi.corrAccountFullName}] [${multi.corrAccountCode}]`);
    print(`link=[${s.transaction.docLink}] [${multi.transaction.docLink}] lot=${s.lot}`);
    let labels = |a| `${a.debitLabel}/${a.creditLabel}`;
    print(`labels=${labels.call(bank)} ${labels.call(book.account("Root:Expenses"))} ` +
          `${labels.call(book.account("Root:Liabilities"))} ${labels.call(book.root())}`);
"#;

fn log(host: &Host, body: &str) -> Vec<String> {
    let script = template(&format!("{body}\n#{{ title: \"t\", sections: [] }}"));
    let reg = registry(&[("query.rhai", &script)]);
    let result = host.run(&reg, GUID, &json!({})).unwrap();
    result.log.into_iter().map(|l| l.text).collect()
}

#[test]
fn the_split_query_filters_sorts_and_keeps_one_split_per_transaction() {
    let host = Host::new(fixture("test-data.gnucash"));
    assert_eq!(
        log(&host, QUERY),
        [
            "default=,trn3,trn1,trn4,trn7",
            "num-down=,trn7,trn4,trn3,trn1",
            "value=,trn4,trn7,trn1,trn3",
            "reconciled=,trn3,trn7,trn1,trn4",
            "corr=,trn1,trn4,trn3,trn7",
            "none=,trn3,trn1,trn4,trn7",
            "cleared=,trn3,trn7",
            "states=,trn5,trn6,trn7 void=,trn5 former=-10 -10 0 0",
            "closing=1 0",
            "unique=2 1 true",
            "num-source=false",
        ]
    );
}

#[test]
fn bad_split_queries_are_script_errors() {
    let host = Host::new(fixture("test-data.gnucash"));
    let bad = [
        r#"#{ accounts: [], sort: [#{ key: "colour" }] }"#,
        r#"#{ accounts: [], sort: [#{ key: "num", up: true }] }"#,
        r#"#{ accounts: [], sort: [#{ key: "num" }, #{ key: "num" }, #{ key: "num" }, #{ key: "num" }] }"#,
        r#"#{ accounts: [], reconcile: ["x"] }"#,
        r#"#{ accounts: [], reconcile: ["nc"] }"#,
        r"#{ accounts: [], isClosing: 1 }",
    ];
    bad.iter().for_each(|q| {
        let script = template(&format!("book.split_query({q}); #{{ title: \"t\", sections: [] }}"));
        let reg = registry(&[("bad.rhai", &script)]);
        assert!(host.run(&reg, GUID, &json!({})).is_err(), "{q}");
    });
}

#[test]
fn split_transaction_and_account_reads() {
    let mut host = Host::new(fixture("test-data.gnucash"));
    let trn1 = host
        .book
        .transactions
        .values()
        .find(|t| t.num.as_deref() == Some("trn1"))
        .unwrap()
        .guid;
    Arc::make_mut(host.book.transactions.get_mut(&trn1).unwrap())
        .slots
        .set(DOC_LINK, SlotValue::String("file:///receipts/trn1.pdf".into()));
    assert_eq!(
        log(&host, READS),
        [
            "other=Root:Expenses true [Root:Expenses] []",
            "multi= [-- Split Transaction --] [Split]",
            "link=[file:///receipts/trn1.pdf] [] lot=",
            "labels=Increase/Decrease Expense/Rebate Decrease/Increase Debit/Credit",
        ]
    );
}

#[test]
fn lots_name_their_invoices() {
    let host = Host::new(fixture("../synthetic/business.gnucash"));
    let body = r"
        let in_lots = book.accounts().map(|a| a.splits()).reduce(|a, b| a + b, []).filter(|s| s.lot != ());
        let lots = #{};
        let titles = #{};
        for s in in_lots { lots[s.lot.guid] = s.lot.invoice; titles[s.lot.guid] = s.lot.title; }
        print(`splits=${in_lots.len()} lots=${lots.len()} ${in_lots[0].lot == in_lots[1].lot}`);
        for g in lots.keys() { print(`${g}=${lots[g].id} ${lots[g].guid} ${titles[g]}`); }
    ";
    // Three lots, one per posted document: an invoice (posted and paid), a bill and an expense voucher, each the
    // first of its kind; `invoice` is the document the lot's `gncInvoice/invoice-guid` slot names, and `title` the
    // lot's title, which posting gives it (`gnc-lot-get-title`).
    assert_eq!(
        log(&host, body),
        [
            "splits=4 lots=3 true",
            "1a037a7010ce434eb82e5416259910e4=000001 6193d3b14af34c25b389fc13ac14d277 Expense 000001",
            "5fd1c24abee14dbd8c884729a0cadd7a=000001 fc2cd829ac2442269c964a2ee27244cc Bill 000001",
            "990068d856184ff2966e0d91c987124a=000001 35ad2d2ebe3847ea9a415e7cc53cc09f Invoice 000001",
        ]
    );
}
