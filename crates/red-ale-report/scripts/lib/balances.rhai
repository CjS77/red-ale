// lib/balances: account balances at dates, over intervals, by value and by transaction type.
//
// Translated from 5.17:gnucash/report/report-utilities.scm (lines 478-725 and 863-881), cited below as RU. Its header:
//
// report-utilities.scm -- Reporting utilities
//
// This program is free software; you can redistribute it and/or
// modify it under the terms of the GNU General Public License as
// published by the Free Software Foundation; either version 2 of
// the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program; if not, contact:
//
// Free Software Foundation           Voice:  +1-617-542-5942
// 51 Franklin Street, Fifth Floor    Fax:    +1-617-542-2652
// Boston, MA  02110-1301,  USA       gnu@gnu.org
//
// This translation is part of red-ale and is distributed under the GNU General Public License, version 3 or (at your
// option) any later version (GPL-3.0-or-later); see LICENSE.
//
// Balances come back as commodity collectors (lib/collectors) or Monetaries. GnuCash's keyword arguments are a map
// of the keys given; a transaction type ('((str "Closing Entries") (cased #f) (regexp #f) (closing #t))) is a map
// `#{ str, cased, regexp, closing }` whose missing keys read as #f. The functions that query splits read the Book,
// which GnuCash takes from the current book, so they take it first. Start and end dates may be `()` for no limit.

import "lib/collectors" as collectors;
import "lib/accounts" as accounts;

/// `gnc:account-get-balances-at-dates` (RU:478-489): for each time of `dates_list`, in the order given, the sum of
/// the amounts of `account`'s splits posted at or before it, as a Monetary of the account's commodity; 0 before the
/// first split. Unlike the running balance, this counts every split, the stock splits the engine skips included.
fn account_get_balances_at_dates(account, dates_list) {
    account_get_balances_at_dates(account, dates_list, ())
}

/// The same with `#:split->amount`: `split_to_amount(split)` instead of the split's amount, `()` counting as 0.
fn account_get_balances_at_dates(account, dates_list, split_to_amount) {
    let comm = account.commodity;
    let splits = account.splits();
    let n = splits.len();
    let i = 0;
    let balance = 0;
    let out = [];
    for date in dates_list {
        while i < n && splits[i].transaction.datePosted <= date {
            let amt = if split_to_amount == () { splits[i].amount } else { split_to_amount.call(splits[i]) };
            if amt != () {
                balance = balance + amt;
            }
            i += 1;
        }
        out.push(monetary(comm, balance));
    }
    out
}

/// `gnc:account-accumulate-at-dates` (RU:505-512) with its defaults: for each time of `dates`, in the order given,
/// the running balance of the last split posted at or before it, `()` before the first split.
fn account_accumulate_at_dates(acc, dates) {
    account_accumulate_at_dates(acc, dates, #{})
}

/// The same with its keyword arguments, a map: `split_to_elt` (a closure `|split|`, or the name of a running total
/// of the query API's `accumulate_at`: "balance", the default, "noclosingBalance", "clearedBalance",
/// "reconciledBalance") gives each split's value, and `nosplit_elt` (default `()`) the value before the first split.
/// GnuCash's `#:split->date` is not used by the reports in scope and is refused.
fn account_accumulate_at_dates(acc, dates, keys) {
    if "split_to_date" in keys {
        throw "account_accumulate_at_dates: split_to_date is not supported";
    }
    let nosplit_elt = if "nosplit_elt" in keys { keys.nosplit_elt } else { () };
    let split_to_elt = if "split_to_elt" in keys { keys.split_to_elt } else { "balance" };
    if type_of(split_to_elt) == "string" {
        return acc.accumulate_at(dates, split_to_elt).map(|v| if v == () { nosplit_elt } else { v });
    }
    // `accumulate_splits_by_dates` (5.17:bindings/engine.i:195-210): one cursor through the splits, moving forward.
    let splits = acc.splits();
    let n = splits.len();
    let i = 0;
    let result = nosplit_elt;
    let out = [];
    for date in dates {
        while i < n && splits[i].transaction.datePosted <= date {
            result = split_to_elt.call(splits[i]);
            i += 1;
        }
        out.push(result);
    }
    out
}

/// `gnc:account-get-comm-balance-at-date` (RU:516-530): the balances of `account` and, with `include_children`, its
/// descendants in book order, each strictly before `date` (`xaccAccountGetBalanceAsOfDate`), in a collector.
fn account_get_comm_balance_at_date(account, date, include_children) {
    let balance_collector = collectors::make_commodity_collector();
    let accts = [account];
    if include_children {
        accts += account.descendants();
    }
    for acct in accts {
        balance_collector.add(acct.commodity, acct.balance_as_of(date));
    }
    balance_collector
}

/// `gnc:account-get-comm-value-interval` (RU:536-546): the values of the splits of `account` (and its descendants
/// with `include_children`) posted from `start_date` to `end_date`, both inclusive, by transaction currency.
fn account_get_comm_value_interval(account, start_date, end_date, include_children) {
    let value_collector = collectors::make_commodity_collector();
    for split in account.splits_between(start_date, end_date, include_children) {
        value_collector.add(split.transaction.currency, split.value);
    }
    value_collector
}

/// `gnc:account-get-comm-value-at-date` (RU:552-553): the values up to `date`.
fn account_get_comm_value_at_date(account, date, include_children) {
    account_get_comm_value_interval(account, (), date, include_children)
}

/// `gnc:accounts-get-balance-helper` (RU:560-570): the collectors `get_balance_fn(account)` gives for each account,
/// merged, or minus-merged when `reverse_balance_fn(account)` is true.
fn accounts_get_balance_helper(accts, get_balance_fn, reverse_balance_fn) {
    let collector = collectors::make_commodity_collector();
    for acct in accts {
        let balance = get_balance_fn.call(acct);
        if reverse_balance_fn.call(acct) {
            collector.minusmerge(balance);
        } else {
            collector.merge(balance);
        }
    }
    collector
}

/// `gnc:accounts-get-comm-total-assets` (RU:577-583): the balances of the accounts that are not income or expense,
/// none reversed.
fn accounts_get_comm_total_assets(accts, get_balance_fn) {
    accounts_get_balance_helper(accts.filter(|a| !accounts::account_is_inc_exp(a)), get_balance_fn, |x| false)
}

/// `gnc:account-get-balance-interval` (RU:588-592): `account_get_comm_balance_interval`'s total in the account's
/// own commodity, a number.
fn account_get_balance_interval(book, account, from, to, include_children) {
    account_get_comm_balance_interval(book, account, from, to, include_children).getpair(account.commodity, false)[1]
}

/// `gnc:account-get-comm-balance-interval` (RU:594-598): the change in the balance of `account` (and its
/// descendants in display order, with `include_children`) from `from` to `to`, closing transactions left out.
fn account_get_comm_balance_interval(book, account, from, to, include_children) {
    let sub_accts = if include_children { account.descendants_sorted() } else { [] };
    account_get_trans_type_balance_interval(book, [account] + sub_accts, (), from, to)
}

/// `gnc:accountlist-get-comm-balance-interval` (RU:603-604): the change in the balances of `accountlist` from
/// `from` to `to`, closing transactions left out.
fn accountlist_get_comm_balance_interval(book, accountlist, from, to) {
    account_get_trans_type_balance_interval(book, accountlist, (), from, to)
}

/// `gnc:accountlist-get-comm-balance-interval-with-closing` (RU:606-607): the same, closing transactions included.
fn accountlist_get_comm_balance_interval_with_closing(book, accountlist, from, to) {
    account_get_trans_type_balance_interval_with_closing(book, accountlist, (), from, to)
}

/// `gnc:accountlist-get-comm-balance-at-date` (RU:609-610): the balances of `accountlist` up to `date` inclusive,
/// closing transactions left out.
fn accountlist_get_comm_balance_at_date(book, accountlist, date) {
    account_get_trans_type_balance_interval(book, accountlist, (), (), date)
}

/// `gnc:accountlist-get-comm-balance-at-date-with-closing` (RU:612-613): the same, closing transactions included.
fn accountlist_get_comm_balance_at_date_with_closing(book, accountlist, date) {
    account_get_trans_type_balance_interval_with_closing(book, accountlist, (), (), date)
}

/// `gnc:split-voided?` (RU:615-617): true when the split's transaction is void.
fn split_voided(split) {
    split.transaction.isVoid
}

/// `gnc:accounts-count-splits` (RU:650-652): the number of splits of all `accounts`.
fn accounts_count_splits(accounts) {
    accounts.reduce(|sum, a| sum + a.splits().len(), 0)
}

/// `gnc:account-get-trans-type-balance-interval` (RU:656-668): the amounts of the splits
/// `account_get_trans_type_splits_interval` finds, by account commodity; with no `trans_type`, closing transactions
/// are left out.
fn account_get_trans_type_balance_interval(book, account_list, trans_type, start_date, end_date) {
    let total = collectors::make_commodity_collector();
    for split in account_get_trans_type_splits_interval(book, account_list, trans_type, start_date, end_date) {
        if trans_type != () || !split.transaction.isClosing {
            total.add(split.account.commodity, split.amount);
        }
    }
    total
}

/// `gnc:account-get-trans-type-balance-interval-with-closing` (RU:672-683): the same, closing transactions included.
fn account_get_trans_type_balance_interval_with_closing(book, account_list, trans_type, start_date, end_date) {
    let total = collectors::make_commodity_collector();
    for split in account_get_trans_type_splits_interval(book, account_list, trans_type, start_date, end_date) {
        total.add(split.account.commodity, split.amount);
    }
    total
}

/// `trans_type`'s value for `key`, `()` when it has none: the `get-val` of RU:699-701.
private fn get_val(trans_type, key) {
    if trans_type != () && key in trans_type { trans_type[key] } else { () }
}

/// True for any value but `()` and false, as Scheme takes any value but #f as true.
private fn truthy(v) {
    v != () && v != false
}

/// `gnc:account-get-trans-type-splits-interval` (RU:692-725): the non-void splits of `account_list` posted from
/// `start_date` to `end_date`, and, when `trans_type` has a `str` or a true `closing`, whose transaction description
/// matches `str` (with `cased` and `regexp`) or which is a closing transaction. In split order; `[]` for no accounts.
fn account_get_trans_type_splits_interval(book, account_list, trans_type, start_date, end_date) {
    if account_list.is_empty() {
        return [];
    }
    let query = #{ accounts: account_list, from: start_date, to: end_date };
    let matchstr = get_val(trans_type, "str");
    if truthy(matchstr) {
        query["description"] = #{ text: matchstr, cased: truthy(get_val(trans_type, "cased")),
                                  regexp: truthy(get_val(trans_type, "regexp")) };
    }
    if truthy(get_val(trans_type, "closing")) {
        query["closing"] = true;
    }
    book.split_query(query)
}

/// `gnc:get-assoc-account-balances` (RU:863-868): `[account, get_balance_fn(account)]` for each of `accounts`.
fn get_assoc_account_balances(accounts, get_balance_fn) {
    accounts.map(|acct| [acct, get_balance_fn.call(acct)])
}

/// `gnc:select-assoc-account-balance` (RU:872-878): the balance of `account` in `account_balances`, the first
/// found; `()` when it has none.
fn select_assoc_account_balance(account_balances, account) {
    let i = account_balances.index_of(|acct_bal| acct_bal[0] == account);
    if i < 0 { () } else { account_balances[i][1] }
}

/// `gnc:get-assoc-account-balances-total` (RU:880-881): every balance of `account_balances` added.
fn get_assoc_account_balances_total(account_balances) {
    collectors::collector_plus(account_balances.map(|acct_bal| acct_bal[1]))
}
