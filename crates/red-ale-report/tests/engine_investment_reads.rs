//! The query reads the investment reports add (`design/reports.md` §5; `design/notes/report-investments.md` §8), on
//! the books GnuCash 5.17 built for its investment tests: `stock-test-data` (`create-stock-test-data`,
//! `5.17:bindings/guile/test/test-engine-extras.scm:952-1059`), `lots-reverse-split-sale` (book D of
//! `5.17:gnucash/report/reports/standard/test/test-investment-lots.scm:232-308`), `ifrs-cost-basis`
//! (`standard/test/test-ifrs-cost-basis.scm:54-243`) and `balsheet-pnl`. Expected values are read off the fixtures'
//! XML: prices at midnight UTC, the lot's three splits in file order (buy, sale, then the reverse split).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::{GUID, Host, fixture, registry};
use red_ale_core::Book;
use serde_json::json;

/// The log lines of `body`, run as a template's `render` on `book`.
fn log_of(book: Book, body: &str) -> Vec<String> {
    let script = format!(
        "fn templates() {{ [#{{ guid: \"{GUID}\", name: \"Reads\" }}] }}\nfn options(p, o, b) {{}}\n\
         fn render(params, book, opts, ctx) {{\n{body}\n#{{ title: \"reads\", sections: [] }}\n}}"
    );
    let reg = registry(&[("reads.rhai", &script)]);
    let result = Host::new(book).run(&reg, GUID, &json!({})).unwrap();
    result.log.into_iter().map(|l| l.text).collect()
}

/// A report fixture's XML with `from` replaced by `to` once, read as a Book: the fixtures carry no lot titles and
/// no num field source, so the tests write them in.
fn edited(name: &str, from: &str, to: &str) -> Book {
    let path = format!("{}/../../fixtures/reports/{name}", env!("CARGO_MANIFEST_DIR"));
    let text = std::fs::read_to_string(path).unwrap();
    assert!(text.contains(from), "{name} has {from:?}");
    red_ale_xml::read_book(text.replacen(from, to, 1).as_bytes()).unwrap().0
}

/// `book.prices_any_currency`: AAPL's prices are all in USD, so each lookup gives at most one, newest first; at
/// 1980-03-02 12:00 the nearest is the $200 of 1980-03-01, before 1980-12-15 the $12 split price of 1980-12-01, and
/// the latest at the run's now (2024-03-15) the $6 of 1981-03-01. Before the first price there is none. On
/// `balsheet-pnl` USD meets two other commodities, GBP and FUNDS, so it gets one price of each, the newer first and
/// as stored: for GBP the 1980-02-01 price of USD in GBP, newer than the 1975 GBP prices in USD; for FUNDS its 1975
/// price in USD.
#[test]
fn prices_in_any_currency() {
    let body = r#"
        let show = |list| list.map(|p| `${p.commodity.mnemonic}/${p.currency.mnemonic} ${p.value} ${p.time} ${p.source}`);
        let t1980_03_02_noon = 320810400; let t1980_12_15 = 345686400; let t1979 = 283996800;
        print(`nearest=${show.call(book.prices_any_currency("NASDAQ::AAPL", t1980_03_02_noon, "nearest"))}`);
        print(`before=${show.call(book.prices_any_currency(book.commodity("NASDAQ::AAPL"), t1980_12_15, "before"))}`);
        print(`latest=${show.call(book.prices_any_currency("NASDAQ::AAPL", 0, "latest"))}`);
        print(`none=${book.prices_any_currency("NASDAQ::AAPL", t1979, "before").len()}`);
        print(`msft=${book.prices_any_currency("NASDAQ::MSFT", t1980_12_15, "nearest").len()}`);
        try { book.prices_any_currency("NASDAQ::AAPL", 0, "newest"); } catch (e) { print(`mode=${e}`); }
    "#;
    let lines = log_of(fixture("stock-test-data.gnucash"), body);
    let want = [
        "nearest=[\"AAPL/USD 200 320716800 user:xfer-dialog\"]",
        "before=[\"AAPL/USD 12 344476800 user:stock-split\"]",
        "latest=[\"AAPL/USD 6 352252800 user:xfer-dialog\"]",
        "none=0",
        "msft=0",
        "mode=price mode must be \"nearest\", \"before\" or \"latest\", got \"newest\"",
    ];
    assert_eq!(lines, want);
    let lines = log_of(
        fixture("balsheet-pnl.gnucash"),
        r#"print(`${book.prices_any_currency("USD", 0, "latest").map(|p| `${p.commodity.mnemonic}/${p.currency.mnemonic} ${p.value}`)}`);"#,
    );
    assert_eq!(lines, ["[\"USD/GBP 10/17\", \"FUNDS/USD 50005/104\"]"]);
}

/// `a.currency_or_parent()`: a currency account is its own; a stock account takes its parent's (Broker, USD); the
/// root of `create-stock-test-data` is an ASSET in USD (TEE:320-357). The template root has no commodity.
#[test]
fn currency_or_parent() {
    let body = r#"
        print(`aapl=${book.account("Root:Broker:AAPL").currency_or_parent().id}`);
        print(`bank=${book.account("Root:Asset:Bank").currency_or_parent().id}`);
        print(`root=${book.root().currency_or_parent()}`);
    "#;
    let lines = log_of(fixture("stock-test-data.gnucash"), body);
    assert_eq!(lines, ["aapl=CURRENCY::USD", "bank=CURRENCY::USD", "root="]);
}

/// Split `isStockSplit` and `actionNum` on the AAPL splits of `create-stock-test-data`: the three stock splits carry
/// `split-type` `stock-split` and the action "Split" (`xaccSplitMakeStockSplit`); the 1981-04-01 spin-off is a plain
/// zero-amount split. With the num field source off, `actionNum` is the split's action.
#[test]
fn stock_splits_and_action_num() {
    let body = r#"
        let aapl = book.account("Root:Broker:AAPL");
        print(`stock-split=${aapl.splits().map(|s| s.isStockSplit)}`);
        print(`action-num=${aapl.splits().map(|s| s.actionNum)}`);
    "#;
    let lines = log_of(fixture("stock-test-data.gnucash"), body);
    let want = [
        "stock-split=[false, false, false, false, true, true, true, false, false, false, false]",
        "action-num=[\"\", \"\", \"\", \"\", \"Split\", \"Split\", \"Split\", \"\", \"\", \"\", \"\"]",
    ];
    assert_eq!(lines, want);
}

/// `actionNum` follows the book's num field source (`gnc_get_action_num`, `5.17:libgnucash/engine/engine-helpers.c:85-102`):
/// the split's action ("Buy", "Fee") with it off, the transaction's num (empty in this book) with it on.
#[test]
fn action_num_follows_the_num_field_source() {
    let body = r#"
        let spy = book.account("Root:Asset:Broker:SPY");
        print(`${spy.splits().map(|s| s.actionNum).filter(|a| a != "").len()} ${spy.splits()[0].actionNum} ${spy.splits()[0].action}`);
    "#;
    let off = log_of(fixture("ifrs-cost-basis.gnucash"), body);
    assert_eq!(off, ["23 Buy Buy"]);
    let id = "<book:id type=\"guid\">b0000000000000000000000000000000</book:id>\n";
    let slots = format!(
        "{id}<book:slots>\n<slot>\n<slot:key>options</slot:key>\n<slot:value type=\"frame\">\n<slot>\n\
         <slot:key>Accounts</slot:key>\n<slot:value type=\"frame\">\n<slot>\n\
         <slot:key>Use Split Action Field for Number</slot:key>\n<slot:value type=\"string\">t</slot:value>\n\
         </slot>\n</slot:value>\n</slot>\n</slot:value>\n</slot>\n</book:slots>\n"
    );
    let on = log_of(edited("ifrs-cost-basis.gnucash", id, &slots), body);
    assert_eq!(on, ["0  Buy"]);
}

/// The Lot handle of book D: its account, no title or notes until set, and its splits in list order, which is
/// Book order (`gnc_lot_add_split` appends as the file loads): the buy, the sale, then the reverse split, though the
/// split was posted before the sale. `split.lot` leads back to it.
#[test]
fn lot_handle() {
    let body = r#"
        let aapl = book.account("Root:Assets:Broker:AAPL");
        let lot = aapl.splits()[0].lot;
        print(`lot=${lot.guid} ${lot.account.name} [${lot.title}] [${lot.notes}] ${lot.invoice}`);
        print(`splits=${lot.splits().map(|s| `${s.transaction.description} ${s.amount} ${s.isStockSplit}`)}`);
        let first = lot.guid;
        print(`back=${lot.splits().all(|s| s.lot.guid == first)}`);
    "#;
    let lines = log_of(fixture("lots-reverse-split-sale.gnucash"), body);
    let want = [
        "lot=10000000000000000000000000000000 AAPL [] [] ",
        "splits=[\"Buy AAPL lot 100 false\", \"Sell AAPL -25 false\", \"Reverse Split -50 true\"]",
        "back=true",
    ];
    assert_eq!(lines, want);
    let id = "<lot:id type=\"guid\">10000000000000000000000000000000</lot:id>\n";
    let slots = format!(
        "{id}<lot:slots>\n<slot>\n<slot:key>title</slot:key>\n<slot:value type=\"string\">Lot 1</slot:value>\n</slot>\n\
         <slot>\n<slot:key>notes</slot:key>\n<slot:value type=\"string\">bought in 2020</slot:value>\n</slot>\n</lot:slots>\n"
    );
    let lines = log_of(edited("lots-reverse-split-sale.gnucash", id, &slots), body);
    assert_eq!(lines[0], "lot=10000000000000000000000000000000 AAPL [Lot 1] [bought in 2020] ");
}

/// `r.round_sigfigs(figs, mode)` and `m.to_string(use_symbol)`: the Portfolio's five significant figures of
/// $700/3 (`standard/portfolio.scm:193-200`), and Investment Lots' amounts with and without the mnemonic
/// (`standard/investment-lots.scm:808-812`): a share amount's exact decimals, no padding; a currency's SCU.
#[test]
fn significant_figures_and_print_info() {
    let body = r#"
        print(`${rational(700, 3).round_sigfigs(5, "round")} ${rational("2.00005").round_sigfigs(5, "half-up")}`);
        try { rational(1).round_sigfigs(5, "nearest"); } catch (e) { print(`mode=${e}`); }
        let aapl = book.commodity("NASDAQ::AAPL");
        print(`${monetary(aapl, rational("1234.5")).to_string(true)}|${monetary(aapl, 42).to_string(false)}`);
        print(`${monetary("USD", rational("-1234.5")).to_string(true)}|${monetary("USD", 6).to_string(false)}`);
    "#;
    let lines = log_of(fixture("stock-test-data.gnucash"), body);
    let want = [
        "23333/100 20001/10000",
        "mode=not a rounding mode: \"nearest\"",
        "1,234.5 AAPL|42.",
        "-$1,234.50|6.00",
    ];
    assert_eq!(lines, want);
}
