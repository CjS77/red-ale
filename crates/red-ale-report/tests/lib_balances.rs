//! `lib/balances` (`5.17:gnucash/report/report-utilities.scm:478-725`, `:863-881`, RU) on
//! `fixtures/reports/test-data.gnucash`, the book of `create-test-data`
//! (`5.17:gnucash/report/reports/standard/test/test-engine-extras.scm:389-480`; `scripts/report-fixtures/account_summary.py`).
//!
//! Bank's splits: +$29 on 1969-12-31 (desc-3; desc-2 the same day is void), −$5 on 1970-01-01, −$15 on 1970-02-01,
//! −$8 on 1970-02-10, −$100 on 1971-02-14 (the multisplit), +$190 each month of 1978, +$14 (10 GBP) on 2000-01-15 and
//! −$9 (6 GBP) on 2000-02-15. Expenses: $5, $15, $11 in 1970, $80 in 1971, −$111 by the closing transaction of
//! 1977-12-31 and $22 each month of 1978. GBP Bank: 51 GBP each month of 1978, −10 and +6 GBP in 2000.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;
mod libs;

use common::{Host, fixture};
use libs::{check, lines};

const PRELUDE: &str = r#"
import "lib/balances" as balances;
import "lib/collectors" as collectors;
let usd = book.commodity("USD"); let gbp = book.commodity("GBP");
let end_of = |y, m, d| ctx.make_time(#{ year: y, month: m, day: d, hour: 23, minute: 59, second: 59 });
let start_of = |y, m, d| ctx.make_time(#{ year: y, month: m, day: d, hour: 0, minute: 0 });
let show = |c| c.format(|comm, amt| `${comm.mnemonic}:${amt}`);
let acct = |n| book.account(n);
let bank = acct.call("Root:Asset:Bank"); let expenses = acct.call("Root:Expenses");
"#;

fn run(body: &str) -> Vec<String> {
    lines(&Host::new(fixture("test-data.gnucash")), &format!("{PRELUDE}\n{body}"))
}

/// gnc:account-get-balances-at-dates (RU:478-489) and gnc:account-accumulate-at-dates (RU:505-512) move one cursor
/// forward through the splits, so a date out of order keeps the balance already reached (engine.i:195-210).
#[test]
fn balances_and_accumulations_at_dates() {
    let got = run(r#"
        let dates = [end_of.call(1969, 12, 30), end_of.call(1969, 12, 31), end_of.call(1970, 2, 10), end_of.call(1971, 12, 31),
                     end_of.call(1970, 1, 1)];
        print(`balances=${balances::account_get_balances_at_dates(bank, dates).map(|m| m.to_string())}`);
        let no_closing = |s| if s.transaction.isClosing { () } else { s.amount };
        let at_1977 = [end_of.call(1977, 12, 31)];
        print(`split-to-amount=${balances::account_get_balances_at_dates(expenses, at_1977)[0]} ` +
              `${balances::account_get_balances_at_dates(expenses, at_1977, no_closing)[0]}`);
        let strs = |l| l.map(|v| if v == () { "()" } else { `${v}` });
        print(`accumulate=${strs.call(balances::account_accumulate_at_dates(bank, dates))}`);
        print(`nosplit=${strs.call(balances::account_accumulate_at_dates(bank, dates, #{ nosplit_elt: 0 }))}`);
        print(`description=${balances::account_accumulate_at_dates(bank, dates, #{ split_to_elt: |s| s.transaction.description })}`);
        print(`noclosing=${strs.call(balances::account_accumulate_at_dates(expenses, at_1977, #{ split_to_elt: "noclosingBalance" }))} ` +
              `${strs.call(balances::account_accumulate_at_dates(expenses, at_1977))}`);
        try { balances::account_accumulate_at_dates(bank, dates, #{ split_to_date: |s| 0 }); } catch (e) { print(`${e}`); }
    "#);
    check(&got, &[
        "balances=[\"$0.00\", \"$29.00\", \"$1.00\", \"-$99.00\", \"-$99.00\"]",
        "split-to-amount=$0.00 $111.00",
        "accumulate=[\"()\", \"29\", \"1\", \"-99\", \"-99\"]",
        "nosplit=[\"0\", \"29\", \"1\", \"-99\", \"-99\"]",
        // desc-2 and desc-3 share a date; split order puts trn3 last.
        "description=[(), \"desc-3\", \"desc-7\", \"$100bank -> $80expenses + $20wallet\", \"$100bank -> $80expenses + $20wallet\"]",
        "noclosing=[\"111\"] [\"0\"]",
        "account_accumulate_at_dates: split_to_date is not supported",
    ]);
}

/// Balances strictly before a date (xaccAccountGetBalanceAsOfDate) and values by transaction currency (RU:516-553).
#[test]
fn comm_balances_and_values() {
    let got = run(r#"
        let asset = acct.call("Root:Asset");
        let feb_2000 = start_of.call(2000, 2, 1);
        print(`balance=${show.call(balances::account_get_comm_balance_at_date(asset, feb_2000, true))} ` +
              `alone=${show.call(balances::account_get_comm_balance_at_date(asset, feb_2000, false))}`);
        // The 2000-01-15 transaction is in GBP, the first split's account's commodity; the 2000-02-15 one in USD.
        let in_2000 = balances::account_get_comm_value_interval(bank, start_of.call(2000, 1, 1), end_of.call(2000, 12, 31), false);
        print(`value-2000=${show.call(in_2000)}`);
        print(`value-at=${show.call(balances::account_get_comm_value_at_date(bank, end_of.call(2000, 12, 31), false))}`);
        print(`value-tree=${show.call(balances::account_get_comm_value_at_date(asset, end_of.call(1971, 12, 31), true))}`);
    "#);
    check(&got, &[
        // Asset's own $0, then Bank's $2,195 and GBP Bank's 602 GBP; Wallet's $20.
        "balance=[\"GBP:602\", \"USD:2215\"] alone=[\"USD:0\"]",
        "value-2000=[\"USD:-9\", \"GBP:10\"]",
        "value-at=[\"GBP:10\", \"USD:2172\"]",
        // Bank's −$99 and Wallet's $20.
        "value-tree=[\"USD:-79\"]",
    ]);
}

/// gnc:accounts-get-balance-helper (RU:560-570) and gnc:accounts-get-comm-total-assets (RU:577-583), and the
/// association lists of balances (RU:863-881).
#[test]
fn balance_helpers() {
    let got = run(r#"
        let now = end_of.call(2001, 1, 1);
        let bal = |a| balances::account_get_comm_balance_at_date(a, now, false);
        let inc_exp = [acct.call("Root:Income"), acct.call("Root:Income-GBP"), expenses];
        print(`helper=${show.call(balances::accounts_get_balance_helper(inc_exp, bal, |a| a.type == "INCOME"))}`);
        let all = book.root().descendants_sorted();
        print(`total-assets=${show.call(balances::accounts_get_comm_total_assets(all, bal))}`);
        let assoc = balances::get_assoc_account_balances(inc_exp, bal);
        print(`assoc=${assoc.map(|e| `${e[0].name}=${show.call(e[1])}`)}`);
        print(`select=${show.call(balances::select_assoc_account_balance(assoc, expenses))} ` +
              `missing=[${balances::select_assoc_account_balance(assoc, bank)}]`);
        print(`total=${show.call(balances::get_assoc_account_balances_total(assoc))}`);
        print(`count=${balances::accounts_count_splits([bank])} ${balances::accounts_count_splits([bank, expenses])} ` +
              `${balances::accounts_count_splits([])}`);
    "#);
    check(&got, &[
        // Income's −$2,573 and Income-GBP's −612 GBP reversed, Expenses' $264 merged.
        "helper=[\"GBP:612\", \"USD:2837\"]",
        // Every account but income and expense: Bank $2,186, Wallet $20, Liabilities −$3, Equity $111; GBP Bank.
        "total-assets=[\"GBP:608\", \"USD:2314\"]",
        "assoc=[\"Income=[\\\"USD:-2573\\\"]\", \"Income-GBP=[\\\"GBP:-612\\\"]\", \"Expenses=[\\\"USD:264\\\"]\"]",
        "select=[\"USD:264\"] missing=[]",
        // gnc:collector+ merges each in turn.
        "total=[\"GBP:-612\", \"USD:-2309\"]",
        // Bank: 6 in 1969-1971, 36 in 1978, 2 in 2000; Expenses: 5, the closing, 12.
        "count=44 62 0",
    ]);
}

/// The balance over an interval leaves closing transactions out unless asked (RU:588-613, :656-683).
#[test]
fn balances_over_intervals_and_closing_entries() {
    let got = run(r#"
        let from = start_of.call(1977, 1, 1);
        let to = end_of.call(1978, 12, 31);
        print(`interval=${show.call(balances::account_get_comm_balance_interval(book, expenses, from, to, false))} ` +
              `number=${balances::account_get_balance_interval(book, expenses, from, to, false)}`);
        print(`list=${show.call(balances::accountlist_get_comm_balance_interval(book, [expenses], from, to))} ` +
              `with-closing=${show.call(balances::accountlist_get_comm_balance_interval_with_closing(book, [expenses], from, to))}`);
        let end_1977 = end_of.call(1977, 12, 31);
        print(`at-date=${show.call(balances::accountlist_get_comm_balance_at_date(book, [expenses], end_1977))} ` +
              `with-closing=${show.call(balances::accountlist_get_comm_balance_at_date_with_closing(book, [expenses], end_1977))}`);
        let root = acct.call("Root");
        print(`children=${show.call(balances::account_get_comm_balance_interval(book, root, (), (), true))}`);
        // A transaction type counts its closing splits even without the -with-closing.
        let closing = #{ str: "Closing", cased: false, regexp: false, closing: true };
        print(`closing-type=${show.call(balances::account_get_trans_type_balance_interval(book, [expenses], closing, (), ()))}`);
    "#);
    check(&got, &[
        "interval=[\"USD:264\"] number=264",
        "list=[\"USD:264\"] with-closing=[\"USD:153\"]",
        "at-date=[\"USD:111\"] with-closing=[\"USD:0\"]",
        // Every split under Root: the transactions in one commodity net to zero, the two exchanges of 2000 leave
        // +$14 − $9 and −10 + 6 GBP.
        "children=[\"GBP:-4\", \"USD:5\"]",
        "closing-type=[\"USD:-111\"]",
    ]);
}

/// gnc:account-get-trans-type-splits-interval (RU:692-725): the description matches as a substring, with case
/// and regexp as asked, or-ed with the closing flag; void transactions never match; any value but false is true.
#[test]
fn trans_type_splits() {
    let got = run(r#"
        let from = start_of.call(1978, 1, 1); let to = end_of.call(1978, 12, 31);
        let in_1978 = |t| balances::account_get_trans_type_splits_interval(book, [bank], t, from, to).len();
        print(`income=${in_1978.call(#{ str: "income" })} upper=${in_1978.call(#{ str: "INCOME", cased: false })} ` +
              `cased=${in_1978.call(#{ str: "INCOME", cased: true })} regexp=${in_1978.call(#{ str: "^\\$10[39]", regexp: true })} ` +
              `plain=${in_1978.call(#{ str: "^\\$10[39]" })} truthy=${in_1978.call(#{ str: "INCOME", cased: 0 })} ` +
              `none=${in_1978.call(())}`);
        let all_time = |accts, t| balances::account_get_trans_type_splits_interval(book, accts, t, (), ())
            .map(|s| s.transaction.description);
        print(`desc=${all_time.call([bank], #{ str: "desc-[0-9]", regexp: true })}`);
        print(`closing=${all_time.call([expenses], #{ closing: true })} ` +
              `or=${all_time.call([expenses], #{ str: "desc-1", closing: true })}`);
        print(`not-closing=${all_time.call([expenses], #{ closing: false }).len()} empty=${all_time.call([], #{ closing: true })}`);
        print(`voided=${balances::split_voided(bank.splits()[0])} ${balances::split_voided(bank.splits()[1])}`);
    "#);
    check(&got, &[
        "income=24 upper=24 cased=0 regexp=24 plain=0 truthy=0 none=36",
        // desc-2 is void.
        "desc=[\"desc-3\", \"desc-1\", \"desc-4\", \"desc-7\"]",
        "closing=[\"Closing\"] or=[\"desc-1\", \"Closing\"]",
        // A false closing flag is no condition at all; desc-5 is void.
        "not-closing=17 empty=[]",
        "voided=true false",
    ]);
}
