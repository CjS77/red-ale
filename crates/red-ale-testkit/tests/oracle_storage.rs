//! Storage features checked against GnuCash 5.17: journals GnuCash writes replay in red-ale, and the files red-ale
//! writes (compressed or not, after edits) open in GnuCash.

#![cfg(feature = "oracle")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use red_ale_testkit::{TempBook, call, diff, model, open, oracle};
use serde_json::json;

/// Make GnuCash edit a copy of a book (post, edit and delete transactions) and save, leaving its journal. The book
/// must not have tabs or newlines in its text fields: GnuCash writes them raw, so its own journal cannot be read
/// back (red-ale skips such records; see `replay.rs`).
const GNUCASH_EDITS: &str = r#"
import sys, datetime
from gnucash import Session, SessionOpenMode, Transaction, Split, GncNumeric, _sw_core_utils
assert _sw_core_utils.gnc_version() == "5.17"
s = Session("xml://" + sys.argv[1], SessionOpenMode.SESSION_NORMAL_OPEN)
book = s.book
root = book.get_root_account()
accts = [a for a in root.get_descendants() if a.GetCommodity() and a.GetCommodity().get_namespace() == "CURRENCY" and not a.GetPlaceholder()]
cur = accts[0].GetCommodity().get_mnemonic()
accts = [a for a in accts if a.GetCommodity().get_mnemonic() == cur and a.GetSplitList()]
a, b = accts[0], accts[1]
usd = a.GetCommodity()
t = Transaction(book)
t.BeginEdit()
t.SetCurrency(usd)
t.SetDescription("Replayedé purchase")
t.SetNum("77")
t.SetNotes("from the journal")
t.SetDatePostedSecsNormalized(datetime.datetime(2024, 5, 4, 12, 0))
for acc, cents, memo in ((a, -4321, "out"), (b, 4321, "in")):
    sp = Split(book)
    sp.SetParent(t)
    sp.SetAccount(acc)
    sp.SetValue(GncNumeric(cents, 100))
    sp.SetAmount(GncNumeric(cents, 100))
    sp.SetMemo(memo)
t.CommitEdit()
done = set()
for acc in (a, b):
    for sp in acc.GetSplitList():
        old = sp.GetParent()
        g = old.GetGUID().to_string()
        if g in done or old.GetDescription() == "Replayedé purchase" or old.GetCurrency().get_mnemonic() != cur:
            continue
        done.add(g)
        old.BeginEdit()
        if len(done) == 1:
            old.SetDescription("edited by GnuCash")
            old.CommitEdit()
        else:
            old.Destroy()
            old.CommitEdit()
            break
    if len(done) >= 2:
        break
s.save()
s.end()
"#;

#[test]
fn a_gnucash_journal_replays_to_the_same_book() {
    let edited = TempBook::copy("gnucash/taxreport.gnucash");
    oracle::run_code(GNUCASH_EDITS, &[&edited.path_str()]).unwrap();
    let journal = std::fs::read_dir(edited.dir.path())
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .find(|p| p.extension().is_some_and(|x| x == "log"))
        .expect("GnuCash wrote a journal");
    let (book, mut d) = open("gnucash/taxreport.gnucash");
    let report = call(&mut d, "book.replayJournal", &json!({ "path": journal.display().to_string() }));
    assert_eq!(report["created"].as_array().unwrap().len(), 1, "{report}");
    assert_eq!(report["deleted"].as_array().unwrap().len(), 1, "{report}");
    assert!(!report["updated"].as_array().unwrap().is_empty(), "{report}");
    call(&mut d, "book.close", &json!({ "save": true }));
    let gnucash_edited = oracle::dump(&edited.path).unwrap();
    let replayed = oracle::dump(&book.path).unwrap();
    let differences = diff(&gnucash_edited["transactions"], &replayed["transactions"], &[]);
    assert!(differences.is_empty(), "{}", differences.join("\n"));
}

#[test]
fn compression_can_be_changed_and_gnucash_reads_both() {
    for compressed in [true, false] {
        let (book, mut d) = open("synthetic/business-editable.gnucash");
        let saved = call(&mut d, "book.save", &json!({ "compressed": compressed }));
        assert_eq!(saved["compressed"], compressed);
        call(&mut d, "book.close", &json!({}));
        let bytes = std::fs::read(&book.path).unwrap();
        assert_eq!(bytes.starts_with(&[0x1f, 0x8b]), compressed);
        let gnucash = oracle::dump(&book.path).unwrap();
        let (ours, _) = red_ale_xml::read_book(&bytes).unwrap();
        let differences = diff(&gnucash, &model::dump(&ours), &model::NOT_YET_COMPUTED);
        assert!(differences.is_empty(), "{}", differences.join("\n"));
    }
}
