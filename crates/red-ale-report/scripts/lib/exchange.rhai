// lib/exchange: prices from transactions and the price database, and the exchange functions reports convert with.
//
// Translated from 5.17:gnucash/report/commodity-utilities.scm (lines 64-898), cited below as CU. Its header:
//
// commodity-utilities.scm: Functions for handling different commodities.
// Copyright 2001 Christian Stimming <stimming@tu-harburg.de>
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
// GnuCash reads the current book; the functions here that read the Book take it as their first argument.
// Commodities are Commodity handles, amounts Rationals, foreign amounts Monetaries, dates Time64s.
//
// Function values. GnuCash's case functions return closures. A closure a Rhai module returns cannot be called once
// the module function has returned, so they return maps instead, which `apply_exchange_fn`, `apply_exchange_time_fn`
// and `apply_price_fn` call: `(exchange-fn mon cur)` is `exchange::apply_exchange_fn(exchange_fn, mon, cur)`. Each
// also takes a closure of the caller's (`|foreign, domestic| …`), so a report can pass its own, as GnuCash's do.
//
// Association lists stay lists: a price table is [[commodity, rate], ...], a report-list
// [[commodity, [domestic, foreign]], ...] with the two value collectors as Rationals, and a sumlist
// [[commodity, report-list], ...]. New entries go on the front and lookups take the first match, as GnuCash's
// `assoc` does (CU:313-432); which rate wins depends on it.
//
// GnuCash prints a warning to the console when a rate cannot be computed (CU:178-191, :311-331, :393-410); red-ale
// has no console, so the warnings and their `hide-warnings?` keys are left out, as are the progress-bar arguments
// (`start-percent`, `delta-percent`). Neither changes a figure.

import "lib/dates" as dates;

/// The position of `key`'s entry in the association list `alist`, -1 when it has none: `assoc`, first match.
private fn alist_index(alist, key) {
    alist.index_of(|entry| entry[0] == key)
}

/// `alist-delete`: `alist` without any entry for `key`.
private fn alist_delete(alist, key) {
    alist.filter(|entry| entry[0] != key)
}

/// True when `v` is a Monetary, GnuCash's `gnc:gnc-monetary?`.
private fn is_monetary(v) {
    type_of(v) == "Monetary"
}

// Splits with two commodities.

/// `get-all-commodity-splits` (CU:64-66): `gnc-get-match-commodity-splits`, through `book.commodity_splits`.
private fn get_all_commodity_splits_sorted(book, currency_accounts, end_date, commodity, sorted) {
    book.commodity_splits(currency_accounts, end_date, commodity, sorted)
}

/// `gnc:get-match-commodity-splits` (CU:73-76): the non-void splits of `currency_accounts` up to `end_date` (`()`
/// for no limit) whose transaction currency differs from their account's commodity and, unless `commodity` is
/// `()`, one of the two is `commodity`. Account after account, each in split order.
fn get_match_commodity_splits(book, currency_accounts, end_date, commodity) {
    get_all_commodity_splits_sorted(book, currency_accounts, end_date, commodity, false)
}

/// `gnc:get-match-commodity-splits-sorted` (CU:80-84): the same, all in split order.
fn get_match_commodity_splits_sorted(book, currency_accounts, end_date, commodity) {
    get_all_commodity_splits_sorted(book, currency_accounts, end_date, commodity, true)
}

/// `gnc:get-all-commodity-splits` (CU:88-89): every such split, whatever its commodities.
fn get_all_commodity_splits(book, currency_accounts, end_date) {
    get_match_commodity_splits(book, currency_accounts, end_date, ())
}

// Prices from transactions, over time.

/// `gnc:exchange-by-euro-numeric` (CU:98-102).
fn exchange_by_euro_numeric(foreign_commodity, foreign_numeric, domestic, date) {
    exchange_by_euro(monetary(foreign_commodity, foreign_numeric), domestic, date)
}

/// `gnc:get-commodity-totalavg-prices` (CU:122-128): the running volume-weighted average price of
/// `price_commodity` in `report_currency`, over the splits up to `end_date`: `[[time, price], ...]`.
fn get_commodity_totalavg_prices(book, currency_accounts, end_date, price_commodity, report_currency) {
    let splits = get_match_commodity_splits_sorted(book, currency_accounts, end_date, price_commodity);
    get_commodity_totalavg_prices_internal(currency_accounts, end_date, price_commodity, report_currency, splits)
}

/// `gnc:get-commodity-totalavg-prices-internal` (CU:130-193), over the given `commodity_splits`. A split with no
/// shares or no value is skipped; one whose rate cannot be found (neither side the report currency, and not two
/// euro currencies) is skipped too.
fn get_commodity_totalavg_prices_internal(currency_accounts, end_date, price_commodity, report_currency, commodity_splits) {
    let tot_foreign = 0;
    let tot_domestic = 0;
    let result = [];
    for a in commodity_splits {
        let share_amt = a.amount.abs();
        let value_amt = a.value.abs();
        let txn_comm = a.transaction.currency;
        let acc_comm = a.account.commodity;
        let txn_date = a.transaction.datePosted;
        if share_amt == 0 || value_amt == 0 {
            continue;
        }
        if acc_comm == report_currency {
            tot_foreign = tot_foreign + value_amt;
            tot_domestic = tot_domestic + share_amt;
        } else if txn_comm == report_currency {
            tot_foreign = tot_foreign + share_amt;
            tot_domestic = tot_domestic + value_amt;
        } else {
            let amt = exchange_by_euro_numeric(txn_comm, value_amt, report_currency, txn_date);
            if amt == () {
                continue;
            }
            tot_foreign = tot_foreign + share_amt;
            tot_domestic = tot_domestic + amt.amount;
        }
        result.push([txn_date, tot_domestic / tot_foreign]);
    }
    result
}

/// `gnc:get-commoditylist-totalavg-prices` (CU:200-223): for each commodity of `commodity_list` (duplicates
/// dropped), `[commodity, prices]` with the prices of `get_commodity_totalavg_prices_internal`, over the splits of
/// every account up to `end_date` that involve it.
fn get_commoditylist_totalavg_prices(book, commodity_list, report_currency, end_date) {
    let currency_accounts = book.root().descendants_sorted();
    let interesting_splits = get_match_commodity_splits_sorted(book, currency_accounts, end_date, ());
    let unique = [];
    for c in commodity_list {
        if !unique.contains(c) {
            unique.push(c);
        }
    }
    let out = [];
    for c in unique {
        let splits = interesting_splits.filter(|s| s.transaction.currency == c || s.account.commodity == c);
        out.push([c, get_commodity_totalavg_prices_internal(currency_accounts, end_date, c, report_currency, splits)]);
    }
    out
}

/// `gnc:get-commodity-inst-prices` (CU:230-275): the price of each split of `price_commodity` up to `end_date`, in
/// `report_currency`: `[[time, price], ...]`.
fn get_commodity_inst_prices(book, currency_accounts, end_date, price_commodity, report_currency) {
    let result = [];
    for a in get_match_commodity_splits_sorted(book, currency_accounts, end_date, price_commodity) {
        let txn_comm = a.transaction.currency;
        let acc_comm = a.account.commodity;
        let share_amt = a.amount.abs();
        let value_amt = a.value.abs();
        let txn_date = a.transaction.datePosted;
        if share_amt == 0 || value_amt == 0 {
            continue;
        }
        if acc_comm == report_currency {
            result.push([txn_date, share_amt / value_amt]);
        } else if txn_comm == report_currency {
            result.push([txn_date, value_amt / share_amt]);
        } else {
            let amt = exchange_by_euro_numeric(txn_comm, value_amt, report_currency, txn_date);
            if amt != () {
                result.push([txn_date, amt.amount / share_amt]);
            }
        }
    }
    result
}

/// `gnc:pricelist-price-find-nearest` (CU:281-291): the price in `pricelist` nearest to `date`, the later one on a
/// tie; `()` for an empty list.
fn pricelist_price_find_nearest(pricelist, date) {
    let n = pricelist.len();
    if n == 0 {
        return ();
    }
    let i = 0;
    loop {
        if i == n - 1 {
            return pricelist[i][1];
        }
        let date1 = pricelist[i][0];
        let date2 = pricelist[i + 1][0];
        if date2 < date {
            i += 1;
            continue;
        }
        if date - date1 < date2 - date {
            return pricelist[i][1];
        }
        return pricelist[i + 1][1];
    }
}

// Exchange rates at one time.

/// `make-newrate` of `gnc:resolve-unknown-comm` (CU:320-331): the pair `[a, b]` with `a` the unknown total and
/// `b` = un→known × known foreign / known domestic, 0 when the known domestic total is zero.
private fn make_newrate(unknown, un_to_known, known_pair) {
    let c = known_pair[1][0];
    let b = if c == 0 { rational(0) } else { un_to_known * known_pair[1][1] / c };
    [unknown, b]
}

/// `gnc:resolve-unknown-comm` (CU:313-432): the report-list of `report_commodity` in `sumlist`, with each other
/// commodity's exchanges resolved into it through a commodity already there, or through the euro.
fn resolve_unknown_comm(sumlist, report_commodity) {
    let at = alist_index(sumlist, report_commodity);
    if at < 0 {
        throw "gnc:resolve-unknown-comm: the sumlist has no entry for the report commodity";
    }
    let reportlist = sumlist[at][1];
    for entry in sumlist {
        let comm = entry[0];
        if comm == report_commodity {
            continue;
        }
        for pair in entry[1] {
            // The report commodity on the wrong side: add it the right way round.
            if pair[0] == report_commodity {
                reportlist.insert(0, [comm, [pair[1][1], pair[1][0]]]);
                continue;
            }
            let ia = alist_index(reportlist, comm);
            let pair_a = if ia >= 0 {
                reportlist[ia]
            } else {
                let euro_monetary = exchange_by_euro(monetary(comm, pair[1][1]), report_commodity, ());
                if euro_monetary == () { () } else { [report_commodity, [pair[1][1], euro_monetary.amount]] }
            };
            let ib = alist_index(reportlist, pair[0]);
            let pair_b = if ib >= 0 { reportlist[ib] } else { () };
            if pair_a == () && pair_b == () {
                // No rate to the report commodity.
                continue;
            }
            if pair_a != () && pair_b != () {
                // GnuCash's "exchange rate ambiguity error": left out.
                continue;
            }
            if pair_a != () {
                reportlist.insert(0, [pair[0], make_newrate(pair[1][0], pair[1][1], pair_a)]);
            } else {
                reportlist.insert(0, [comm, make_newrate(pair[1][1], pair[1][0], pair_b)]);
            }
        }
    }
    reportlist
}

/// `gnc:get-exchange-totals` (CU:441-511): the absolute amounts and values of every exchange up to `end_date`
/// (splits with no shares skipped), summed by commodity pair and resolved into `report_commodity`: a report-list.
fn get_exchange_totals(book, report_commodity, end_date) {
    let curr_accounts = book.root().descendants_sorted();
    let sumlist = [[report_commodity, []]];
    for a in get_all_commodity_splits(book, curr_accounts, end_date) {
        let txn_comm = a.transaction.currency;
        let acc_comm = a.account.commodity;
        let share_amt = a.amount.abs();
        let value_amt = a.value.abs();
        if share_amt == 0 {
            continue;
        }
        let i = alist_index(sumlist, txn_comm);
        if i >= 0 {
            let j = alist_index(sumlist[i][1], acc_comm);
            if j >= 0 {
                sumlist[i][1][j][1][0] += share_amt;
                sumlist[i][1][j][1][1] += value_amt;
            } else {
                let inner = sumlist[i][1];
                inner.insert(0, [acc_comm, [share_amt, value_amt]]);
                sumlist = alist_delete(sumlist, txn_comm);
                sumlist.insert(0, [txn_comm, inner]);
            }
            continue;
        }
        let i = alist_index(sumlist, acc_comm);
        if i >= 0 {
            let j = alist_index(sumlist[i][1], txn_comm);
            if j >= 0 {
                sumlist[i][1][j][1][0] += value_amt;
                sumlist[i][1][j][1][1] += share_amt;
            } else {
                let inner = sumlist[i][1];
                inner.insert(0, [txn_comm, [value_amt, share_amt]]);
                sumlist = alist_delete(sumlist, acc_comm);
                sumlist.insert(0, [acc_comm, inner]);
            }
            continue;
        }
        sumlist.insert(0, [acc_comm, [[txn_comm, [value_amt, share_amt]]]]);
    }
    resolve_unknown_comm(sumlist, report_commodity)
}

/// `gnc:get-exchange-cost-totals` (CU:520-597): the signed amounts and values, gains and losses included, of every
/// exchange up to `end_date` outside TRADING accounts, summed by commodity pair and resolved into
/// `report_commodity`: a report-list.
fn get_exchange_cost_totals(book, report_commodity, end_date) {
    let curr_accounts = book.root().descendants_sorted();
    let sumlist = [[report_commodity, []]];
    for a in get_all_commodity_splits(book, curr_accounts, end_date) {
        // Trading splits counterbalance the values and amounts back to zero.
        if a.account.type == "TRADING" {
            continue;
        }
        let txn_comm = a.transaction.currency;
        let acc_comm = a.account.commodity;
        let share_amt = a.amount;
        let value_amt = a.value;
        let i = alist_index(sumlist, txn_comm);
        if i >= 0 {
            let key = sumlist[i][0];
            let j = alist_index(sumlist[i][1], acc_comm);
            if j >= 0 {
                sumlist[i][1][j][1][0] += share_amt;
                sumlist[i][1][j][1][1] += value_amt;
            } else {
                let inner = sumlist[i][1];
                inner.insert(0, [acc_comm, [share_amt, value_amt]]);
                sumlist = alist_delete(sumlist, key);
                sumlist.insert(0, [key, inner]);
            }
            continue;
        }
        let i = alist_index(sumlist, acc_comm);
        if i >= 0 {
            let key = sumlist[i][0];
            let j = alist_index(sumlist[i][1], txn_comm);
            if j >= 0 {
                sumlist[i][1][j][1][0] += -value_amt;
                sumlist[i][1][j][1][1] += -share_amt;
            } else {
                let inner = sumlist[i][1];
                inner.insert(0, [txn_comm, [-value_amt, -share_amt]]);
                sumlist = alist_delete(sumlist, key);
                sumlist.insert(0, [key, inner]);
            }
            continue;
        }
        sumlist.insert(0, [acc_comm, [[txn_comm, [-value_amt, -share_amt]]]]);
    }
    resolve_unknown_comm(sumlist, report_commodity)
}

/// `gnc:make-exchange-alist` (CU:599-607): the price table of the "weighted average" price source, |foreign /
/// domestic| of each `get_exchange_totals` entry. A zero domestic total divides by zero, a `report-script` error, as
/// Guile raises one.
fn make_exchange_alist(book, report_commodity, end_date) {
    get_exchange_totals(book, report_commodity, end_date).map(|entry| [entry[0], (entry[1][1] / entry[1][0]).abs()])
}

/// `gnc:make-exchange-cost-alist` (CU:609-618): the price table of the "average cost" price source, |foreign /
/// domestic| of each `get_exchange_cost_totals` entry, 0 for a zero domestic total.
fn make_exchange_cost_alist(book, report_commodity, end_date) {
    get_exchange_cost_totals(book, report_commodity, end_date).map(|entry| {
        let denom = entry[1][0];
        [entry[0], if denom == 0 { rational(0) } else { (entry[1][1] / denom).abs() }]
    })
}

// Exchanging amounts.

/// `gnc:exchange-by-euro` (CU:635-644): `foreign` in `domestic` at the euro's fixed rates when both are euro
/// currencies, else `()`. `date` is not checked, as in GnuCash.
fn exchange_by_euro(foreign, domestic, date) {
    euro_convert(foreign, domestic)
}

/// `gnc:exchange-if-same` (CU:655-657): `foreign` itself when it is in `domestic`, else `()`.
fn exchange_if_same(foreign, domestic) {
    if foreign.commodity == domestic { foreign } else { () }
}

/// `gnc:make-exchange-function` (CU:663-678): an exchange function over a price table (`make_exchange_alist`,
/// `make_exchange_cost_alist`): the euro first, then the same commodity, then the table's rate, 0 for a commodity
/// the table lacks.
fn make_exchange_function(exchange_alist) {
    #{ exchange_fn: "alist", alist: exchange_alist }
}

/// The function `gnc:make-exchange-function` returns, applied.
private fn exchange_by_alist(exchange_alist, foreign, domestic) {
    if foreign == () {
        return ();
    }
    let euro = exchange_by_euro(foreign, domestic, ());
    if euro != () {
        return euro;
    }
    let same = exchange_if_same(foreign, domestic);
    if same != () {
        return same;
    }
    let i = alist_index(exchange_alist, foreign.commodity);
    // `gnc-numeric-mul` with GNC-DENOM-AUTO: the exact product.
    let amount = if i >= 0 { foreign.amount * exchange_alist[i][1] } else { 0 };
    monetary(domestic, amount)
}

/// `gnc:exchange-by-pricedb-latest` (CU:686-699): `foreign` in `domestic` at the latest price in the price
/// database; `()` when `foreign` is not a Monetary.
fn exchange_by_pricedb_latest(book, foreign, domestic) {
    if !is_monetary(foreign) {
        return ();
    }
    let euro = exchange_by_euro(foreign, domestic, ());
    if euro != () {
        return euro;
    }
    let same = exchange_if_same(foreign, domestic);
    if same != () {
        return same;
    }
    // The latest price ignores the time; the run's now is the host's.
    book.price_convert(foreign, domestic, 0, "latest")
}

/// `gnc:exchange-by-pricedb-nearest` (CU:708-721): at the price nearest to `date`; `()` when `foreign` is not a
/// Monetary or `date` is `()`.
fn exchange_by_pricedb_nearest(book, foreign, domestic, date) {
    exchange_by_pricedb(book, foreign, domestic, date, "nearest")
}

/// `gnc:exchange-by-pricedb-nearest-before` (CU:723-734): at the latest price at or before `date`.
fn exchange_by_pricedb_nearest_before(book, foreign, domestic, date) {
    exchange_by_pricedb(book, foreign, domestic, date, "before")
}

private fn exchange_by_pricedb(book, foreign, domestic, date, mode) {
    if !is_monetary(foreign) || date == () {
        return ();
    }
    let euro = exchange_by_euro(foreign, domestic, date);
    if euro != () {
        return euro;
    }
    let same = exchange_if_same(foreign, domestic);
    if same != () {
        return same;
    }
    book.price_convert(foreign, domestic, date, mode)
}

/// `gnc:exchange-by-pricealist-nearest` (CU:741-761): at the price of `pricealist` (`[[commodity, prices], ...]`, from
/// `get_commoditylist_totalavg_prices`) nearest to `date`, 0 when there is none.
fn exchange_by_pricealist_nearest(pricealist, foreign, domestic, date) {
    if !is_monetary(foreign) || date == () {
        return ();
    }
    let euro = exchange_by_euro(foreign, domestic, date);
    if euro != () {
        return euro;
    }
    let same = exchange_if_same(foreign, domestic);
    if same != () {
        return same;
    }
    let i = alist_index(pricealist, foreign.commodity);
    let price = if i >= 0 { pricelist_price_find_nearest(pricealist[i][1], date) } else { () };
    if price == () {
        price = 0;
    }
    // `gnc-numeric-mul` with GNC-DENOM-AUTO: the exact product.
    monetary(domestic, foreign.amount * price)
}

// Choosing an exchange function by the price-source option (lib/options' add_price_source).

/// `gnc:case-exchange-fn` (CU:769-795): the exchange function of `source_option` at `to_date_tp`. An unknown source
/// falls back to the nearest price, as GnuCash's does.
fn case_exchange_fn(book, source_option, report_currency, to_date_tp) {
    switch source_option {
        "average-cost" => make_exchange_function(make_exchange_cost_alist(book, report_currency, to_date_tp)),
        "weighted-average" => make_exchange_function(make_exchange_alist(book, report_currency, to_date_tp)),
        "pricedb-latest" => #{ exchange_fn: "pricedb-latest", book: book },
        "pricedb-before" => #{ exchange_fn: "pricedb-before", book: book, date: to_date_tp },
        _ => #{ exchange_fn: "pricedb-nearest", book: book, date: to_date_tp },
    }
}

/// `(exchange-fn foreign domestic)`: apply an exchange function of `case_exchange_fn` or `make_exchange_function`,
/// or a closure `|foreign, domestic|`.
fn apply_exchange_fn(exchange_fn, foreign, domestic) {
    if type_of(exchange_fn) == "Fn" {
        return exchange_fn.call(foreign, domestic);
    }
    switch exchange_fn.exchange_fn {
        "alist" => exchange_by_alist(exchange_fn.alist, foreign, domestic),
        "pricedb-latest" => exchange_by_pricedb_latest(exchange_fn.book, foreign, domestic),
        "pricedb-before" => exchange_by_pricedb_nearest_before(exchange_fn.book, foreign, domestic, exchange_fn.date),
        "pricedb-nearest" => exchange_by_pricedb_nearest(exchange_fn.book, foreign, domestic, exchange_fn.date),
        _ => throw `not an exchange function: ${exchange_fn.exchange_fn}`,
    }
}

/// `gnc:case-price-fn` (CU:797-810): the price function of `source` at `date`: a commodity's price in
/// `target_curr`. The price-database sources give the database's rate, 0 when there is none; the others exchange
/// one unit with `case_exchange_fn`, which GnuCash builds on every call and red-ale once, with the same result.
fn case_price_fn(book, source, target_curr, date) {
    switch source {
        "pricedb-nearest" => #{ price_fn: "pricedb-nearest", book: book, currency: target_curr, date: date },
        "pricedb-before" => #{ price_fn: "pricedb-before", book: book, currency: target_curr, date: date },
        "pricedb-latest" => #{ price_fn: "pricedb-latest", book: book, currency: target_curr },
        _ => #{ price_fn: "exchange", currency: target_curr, exchange_fn: case_exchange_fn(book, source, target_curr, date) },
    }
}

/// `(price-fn commodity)`: apply a price function of `case_price_fn`, or a closure `|commodity|`.
fn apply_price_fn(price_fn, commodity) {
    if type_of(price_fn) == "Fn" {
        return price_fn.call(commodity);
    }
    switch price_fn.price_fn {
        "pricedb-nearest" => price_fn.book.price_rate(commodity, price_fn.currency, price_fn.date, "nearest"),
        "pricedb-before" => price_fn.book.price_rate(commodity, price_fn.currency, price_fn.date, "before"),
        "pricedb-latest" => price_fn.book.price_rate(commodity, price_fn.currency, 0, "latest"),
        "exchange" => apply_exchange_fn(price_fn.exchange_fn, monetary(commodity, 1), price_fn.currency).amount,
        _ => throw `not a price function: ${price_fn.price_fn}`,
    }
}

/// `gnc:case-exchange-time-fn` (CU:829-887): the exchange function of `source_option` over time, taking the date of
/// each exchange. "average-cost" and "weighted-average" build their price tables at the end of that date's day and
/// keep each in a Memo, as GnuCash memoises them; the price-database sources look prices up at the date.
/// `commodity_list` is the commodities the weighted average prices. GnuCash also builds the table at `to_date_tp`
/// once, only to print its warnings; red-ale prints none, so `to_date_tp` is unused.
fn case_exchange_time_fn(book, ctx, source_option, report_currency, commodity_list, to_date_tp) {
    let kind = switch source_option {
        "average-cost" | "weighted-average" | "pricedb-before" | "pricedb-latest" => source_option,
        _ => "pricedb-nearest",
    };
    #{ exchange_time_fn: kind, book: book, ctx: ctx, report_currency: report_currency, commodity_list: commodity_list,
       memo: memo() }
}

/// `(exchange-fn foreign domestic date)`: apply an exchange function of `case_exchange_time_fn`, or a closure
/// `|foreign, domestic, date|`.
fn apply_exchange_time_fn(exchange_fn, foreign, domestic, date) {
    if type_of(exchange_fn) == "Fn" {
        return exchange_fn.call(foreign, domestic, date);
    }
    let book = exchange_fn.book;
    switch exchange_fn.exchange_time_fn {
        "average-cost" => {
            let end_day = dates::time64_end_day_time(exchange_fn.ctx, date);
            let f = exchange_fn.memo.get(end_day);
            if f == () {
                f = make_exchange_function(make_exchange_cost_alist(book, exchange_fn.report_currency, end_day));
                exchange_fn.memo.set(end_day, f);
            }
            apply_exchange_fn(f, foreign, domestic)
        }
        "weighted-average" => {
            let end_day = dates::time64_end_day_time(exchange_fn.ctx, date);
            let pricealist = exchange_fn.memo.get(end_day);
            if pricealist == () {
                pricealist = get_commoditylist_totalavg_prices(book, exchange_fn.commodity_list, exchange_fn.report_currency, end_day);
                exchange_fn.memo.set(end_day, pricealist);
            }
            exchange_by_pricealist_nearest(pricealist, foreign, domestic, date)
        }
        "pricedb-before" => exchange_by_pricedb_nearest_before(book, foreign, domestic, date),
        "pricedb-latest" => exchange_by_pricedb_latest(book, foreign, domestic),
        "pricedb-nearest" => exchange_by_pricedb_nearest(book, foreign, domestic, date),
        _ => throw `not an exchange function over time: ${exchange_fn.exchange_time_fn}`,
    }
}
