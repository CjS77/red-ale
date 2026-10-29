// investment-lots.scm
// by Brent McBride (mcbridebt@hotmail.com) Nov 2022
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
// Translated from 5.17:gnucash/report/reports/standard/investment-lots.scm, cited below by line.
//
// Investment Lots: for each selected account, a row per lot with what was bought, sold and held in the report's
// window, the realized and unrealized gains (short and long term), their ROI and compound annual growth rate, and
// totals per account and for the report (design/notes/report-investments.md §4). The report does its own basis
// arithmetic from the lots' splits: raw amounts, a forward stock split counted as a purchase, values converted to the
// report's currency. Sums are exact; a sale's basis and gain and a lot's end value are rounded to the report
// currency's SCU as GnuCash rounds them. GnuCash's doubles (ROI, CAGR and their weighted averages) are exact here,
// except the CAGR itself, a fractional power, which `calculate_cagr` gives to 10⁻¹². A figure GnuCash cannot compute
// (#f) is `()`, an empty cell. The test-only helpers `calculate-gain` and `is-long-term?` (:466-469, :488-497), the
// debug output and the cell markup are left out.

import "lib/accounts" as accounts;
import "lib/balances" as balances;
import "lib/dates" as dates;
import "lib/exchange" as exchange;
import "lib/options" as opts_util;

/// The template (:2321-2326).
fn templates() {
    [#{ guid: "ab2acc24afd14630a551f98f1a35fa81", name: "Investment Lots", menuPath: ["_Assets & Liabilities"],
        params: #{} }]
}

/// `options-generator` (:141-439). The callbacks of the complex options only make others selectable in GnuCash's
/// dialog, so they are `()`.
fn options(params, o, book) {
    o.account_list("Accounts", "Accounts", "a", "Stock Accounts to report on.",
                   accounts::filter_accountlist_type(["STOCK", "MUTUAL"], book.root().descendants_sorted()),
                   #{ types: ["ASSET", "BANK", "STOCK", "MUTUAL"] });
    o.boolean("Accounts", "Include accounts with no shares", "b", "Include accounts that have a zero share balances.",
              true);
    o.boolean("Accounts", "Include accounts with no lots", "c", "Include accounts with no lots", false);

    o.complex_boolean("Chart", "Show Chart", "a",
                      "Include a chart that shows lot gains, grouped by account and gain type", true, ());
    o.multichoice("Chart", "Chart type", "b", "What kind of chart to include", "bar-stacked", [
        ["bar", "Bar Chart"],
        ["bar-stacked", "Stacked Bar Chart"],
    ]);
    o.multichoice("Chart", "Chart location", "c", "Where to place the chart", "top", [
        ["top", "Top"],
        ["bottom", "Bottom"],
    ]);
    opts_util::add_plot_size(o, "Chart", "Plot width", "Plot height", "d", #{ percent: 100 }, #{ percent: 50 });

    o.boolean("Columns", "Show lot GUID column", "a", "Show the lot GUID table column", false);
    o.boolean("Columns", "Show date columns", "b", "Show the lot open and close table columns", true);
    o.boolean("Columns", "Show bought columns", "c", "Show purchase-related table columns", true);
    o.boolean("Columns", "Show sold columns", "d", "Show sale-related table columns", true);
    o.boolean("Columns", "Show end columns", "e", "Show end date amount and value table columns", true);
    o.complex_boolean("Columns", "Show realized gain columns", "f", "Show realized gain table column(s) for sold shares",
                      true, ());
    o.complex_boolean("Columns", "Show unrealized gain columns", "g",
                      "Show unrealized gain table column(s) for unsold shares", true, ());
    o.multichoice_callback("Columns", "Group gains by age (short term and long term)", "h",
                           "Group gains (and sales?) by long-term (LT) and short-term (ST)", "gains-only", [
        ["no", "No"],
        ["gains-only", "Gains Only"],
        ["gains-and-sales", "Gains and Sales"],
    ], ());
    o.number_range("Columns", "Long term gains age (years)", "i",
                   "Commodities held longer than this many years count as long-term (LT).", 1, 0, 10000000000, 1);
    o.boolean("Columns", "Show ROI columns", "j", "Show Return On Investment (ROI) columns", false);
    o.boolean("Columns", "Show CAGR columns", "k", "Show Compound Annual Growth Rate (CAGR) columns", true);

    o.boolean("Display", "Show long account names", "a", "Show long (instead of short) account names", true);
    o.boolean("Display", "Show mnemonic in amounts", "b", "Show mnemonics with commodity amounts", true);
    o.boolean("Display", "Include closed lots", "c", "Include closed lots in addition to open lots", true);
    o.boolean("Display", "Show blanks instead of zeros in table cells", "d",
              "Show blank text instead of zero values for inner table cells. Does not apply to footer rows.", true);
    o.boolean("Display", "Show lot split rows", "e", "Add a row for each split belonging to a lot, under the lot row.",
              false);

    // A default End date of today, not the end of the accounting period (:353-359).
    opts_util::add_date_interval(o, "General", "Start date", "End date", "a");
    o.set_default("General", "End date", "today");
    opts_util::add_currency(o, "General", "Report's currency", "b");
    o.multichoice("General", "Price source", "c", "The source of price information.", "pricedb-before", [
        ["pricedb-before", "Last up through report date"],
        ["pricedb-nearest", "Closest to report date"],
        ["pricedb-latest", "Most recent"],
    ]);

    o.boolean("Validation", "Include only accounts with warnings", "a",
              "Only show accounts that contain warnings. This is useful for quickly finding potential lot errors.",
              false);
    o.boolean("Validation", "Warn if a lot has more than one bought split", "b",
              "Lots with more than one purchase split are not well formed. It may make ambiguous the capital gains age",
              true);
    o.boolean("Validation", "Warn if a lot's balance drops below zero", "c",
              "Lots with a negative balance are not well formed.", true);
    o.boolean("Validation", "Warn if a lot has a blank title", "d",
              "Lot titles are optional. This warning applies to titles that are empty or only whitespace.", false);
    o.boolean("Validation", "Warn if the 'Realized Gain/Loss' split(s) sum does not match the computed gains", "e",
              "Detect possible errors in 'Realized Gain/Loss' splits that are created when adding a sale split to a lot",
              true);
    o.multichoice("Validation", "Warn if a split is not assigned to a lot", "f",
                  "Detect splits that have not been assigned to a lot.", "count", [
        ["no", "No"],
        ["count", "Count"],
        ["list", "List"],
    ]);
    o.boolean("Validation", "Warn if the account balance does not match the computed lots' end balance", "g",
              "Balance mismatches may indicate a split that is not yet included in a lot", true);
}

// Figures that may be undefined: GnuCash's gnc-numeric calls turn #f into an error numeric and give #f back
// (5.17:bindings/guile/gnc-engine-guile.cpp:1709-1734), so one undefined figure blanks every sum it reaches. Its sign
// tests read an undefined figure as neither zero, negative nor positive.

/// True for `()`, GnuCash's `#f`: a figure the report could not compute.
private fn undefined(x) {
    type_of(x) == "()"
}

/// `gnc-numeric-add-fixed`: `a + b`, exact, or `()`.
private fn num_add(a, b) {
    if undefined(a) || undefined(b) { () } else { a + b }
}

/// `gnc-numeric-sub-fixed`: `a - b`, exact, or `()`.
private fn num_sub(a, b) {
    if undefined(a) || undefined(b) { () } else { a - b }
}

/// `a × b`, exact, or `()`.
private fn num_mul(a, b) {
    if undefined(a) || undefined(b) { () } else { a * b }
}

/// `gnc-numeric-div … GNC-DENOM-AUTO GNC-DENOM-REDUCE`: `a / b`, exact, or `()` when either is undefined or `b` is
/// zero, where `gnc_numeric_div` gives an error.
private fn num_div(a, b) {
    if undefined(a) || undefined(b) || b == 0 { () } else { a / b }
}

/// `gnc-numeric-zero-p`.
private fn num_zero(x) {
    !undefined(x) && x == 0
}

/// `gnc-numeric-negative-p`.
private fn num_negative(x) {
    !undefined(x) && x < 0
}

/// `gnc-numeric-positive-p`.
private fn num_positive(x) {
    !undefined(x) && x > 0
}

/// `x` rounded to the report currency's SCU with banker's rounding (`GNC-RND-ROUND`), or `()`.
private fn to_report_scu(env, x) {
    if undefined(x) { () } else { x.round_to(env.fraction, "round") }
}

// The helpers GnuCash's test calls directly (:442-536), kept public so tests can call them as it does.

/// `calculate-roi` (:475-481): `gain / basis`, or `()` for a zero basis. GnuCash divides doubles.
fn calculate_roi(basis, gain) {
    if basis == 0 { () } else { gain / basis }
}

/// `stock-split-event?` (:508-511): the split is marked a stock split (`xaccSplitGetType`), or its action is "split"
/// in any case.
fn stock_split_event(split_type, split_action) {
    split_type == "stock-split" || split_action.to_lower() == "split"
}

/// `non-realizing-share-reduction?` (:516-521): a stock split that takes shares away for nothing, a reverse split.
fn non_realizing_share_reduction(split_type, split_action, amount, value) {
    stock_split_event(split_type, split_action) && amount < 0 && value == 0
}

/// `classify-lot-split` (:524-536). A forward stock split, shares for nothing, is a purchase.
fn classify_lot_split(split_type, split_action, amount, value) {
    if non_realizing_share_reduction(split_type, split_action, amount, value) {
        "non-realizing-share-reduction"
    } else if amount > 0 {
        "purchase"
    } else if amount < 0 {
        "sale"
    } else {
        "realized-gain-split"
    }
}

/// `xaccSplitGetType`: "stock-split" or "normal".
private fn split_type(split) {
    if split.isStockSplit { "stock-split" } else { "normal" }
}

// Lots and splits.

/// `collect-lots-and-unassigned-splits` (:540-562): the lots of `splits` in order of first appearance, and the splits
/// in no lot, newest first.
fn collect_lots_and_unassigned_splits(splits) {
    let lots = [];
    let seen = [];
    let unassigned = [];
    for split in splits {
        let lot = split.lot;
        if undefined(lot) {
            unassigned.insert(0, split);
        } else if !seen.contains(lot.guid) {
            seen.push(lot.guid);
            lots.push(lot);
        }
    }
    [lots, unassigned]
}

/// `split-sort<=?` (:566-585) as a three-way comparison: by posted date; on one date purchases first, purchases among
/// themselves in transaction order (`xaccTransOrder`), everything else equal.
private fn compare_lot_splits(s1, s2) {
    let t1 = s1.transaction;
    let t2 = s2.transaction;
    let date1 = t1.datePosted;
    let date2 = t2.datePosted;
    if date1 != date2 {
        return if date1 < date2 { -1 } else { 1 };
    }
    let is_purchase_s1 = s1.amount > 0;
    let is_purchase_s2 = s2.amount > 0;
    if is_purchase_s1 && is_purchase_s2 {
        t1.order(t2)
    } else if is_purchase_s1 == is_purchase_s2 {
        0
    } else if is_purchase_s1 {
        -1
    } else {
        1
    }
}

/// `lot-splits-up-to-date` (:588-600): the lot's splits posted up to `to_date`, void ones too, sorted. GnuCash conses
/// them off the lot's list, reversing it, then sorts with a stable merge sort; splits its order leaves equal (sales on
/// one date) therefore come in the reverse of the lot's list order, which red-ale takes as Book order.
private fn lot_splits_up_to_date(lot, to_date) {
    let picked = [];
    for split in lot.splits() {
        if split.transaction.datePosted <= to_date {
            picked.insert(0, split);
        }
    }
    picked.sort(|a, b| compare_lot_splits(a, b));
    picked
}

/// `get-all-splits` (:871-882): the account's splits posted up to `to_date` in split order, leaving out void
/// transactions; earlier ones are kept for the running balance and basis.
private fn get_all_splits(env, account) {
    let splits = [];
    for split in account.splits_between((), env.to_date) {
        if !split.transaction.isVoid {
            splits.push(split);
        }
    }
    splits
}

// Prices and values.

/// `find-price` (:620-636): the first price of `price_list` in `currency`, else the first whose commodity is
/// `currency`, inverted, else the first as it is; `()` for an empty list. A price is `#{ price, currency, value, time
/// }`, `price` being the stored price, which an inverted one still names (GnuCash makes a temporary price, not in the
/// book; a link must name a real one).
fn find_price(price_list, currency) {
    if price_list.is_empty() {
        return ();
    }
    for price in price_list {
        if price.currency == currency {
            return #{ price: price, currency: price.currency, value: price.value, time: price.time };
        }
        if price.commodity == currency {
            // gnc_numeric_invert gives 0 for 0.
            let value = if price.value == 0 { rational(0) } else { 1 / price.value };
            return #{ price: price, currency: price.commodity, value: value, time: price.time };
        }
    }
    let first = price_list[0];
    #{ price: first, currency: first.currency, value: first.value, time: first.time }
}

/// `price-fn` (:732-753): the price of `commodity` from the price source, any currency: at local noon of the End date
/// (`time64CanonicalDayTime`) at or before it or nearest it, or the latest at or before now.
private fn price_of(env, commodity) {
    let at = dates::time64_canonical_day_time(env.ctx, env.to_date);
    let prices = switch env.price_source {
        "pricedb-latest" => env.book.prices_any_currency(commodity, env.ctx.now, "latest"),
        "pricedb-nearest" => env.book.prices_any_currency(commodity, at, "nearest"),
        _ => env.book.prices_any_currency(commodity, at, "before"),
    };
    find_price(prices, env.currency)
}

/// `value->report-currency-value` (:857-864): `value` in `currency` converted to the report's currency with the
/// exchange function at the End date, which rounds a converted value to the report currency's SCU.
private fn report_value(env, value, currency) {
    exchange::apply_exchange_fn(env.exchange_fn, monetary(currency, value), env.currency).amount
}

/// `price->report-currency-value` (:920-924).
private fn price_report_value(env, price) {
    report_value(env, price.value, price.currency)
}

// Holding periods.

/// `gnc:date-year-delta` from `bought` to `sold`, or `()` when either is unknown, where GnuCash's fails on `'()`
/// (5.17:bindings/guile/date-utilities.scm:201-217).
private fn years_between(env, bought, sold) {
    if undefined(bought) || undefined(sold) { () } else { dates::date_year_delta(env.ctx, bought, sold) }
}

/// `long-term?` (:779-789): held strictly longer than the "Long term gains age (years)", counted by
/// `gnc:date-year-delta`; false when either date is unknown.
private fn long_term(env, bought, sold) {
    let years = years_between(env, bought, sold);
    !undefined(years) && years > env.long_term_years
}

// The lot statistics collector (`create-lot-stats-collector`, :1177-2069): a map of figures for one lot (`kind`
// "lot"), the lots of one account ("account") or every shown account ("grand"). GnuCash's closures over mutable
// variables become functions that take the map and give it back changed.

/// A collector of `kind` with every figure zero.
private fn new_stats(kind) {
    let z = rational(0);
    #{
        kind: kind,
        bought_split_count: 0, old_bought_amount: z, old_bought_value: z, bought_amount: z, bought_value: z,
        sold_split_count: 0,
        st_sold_amount: z, st_sold_basis: z, st_sold_value: z, st_realized_gain: z,
        lt_sold_amount: z, lt_sold_basis: z, lt_sold_value: z, lt_realized_gain: z,
        weighted_realized_cagr: z,
        end_amount: z, end_basis: z, end_value: z,
        unrealized_gain: z, st_unrealized_gain: z, lt_unrealized_gain: z, weighted_unrealized_cagr: z,
        has_warnings: false, is_active_in_window: false, currency: (),
        splits_realized_gain: z, split_rows: [], first_negative_split: (),
        earliest_bought_split: (), earliest_bought_split_date: (), latest_bought_split_date: (), last_sold_split: (),
        lot: (), account: (), unassigned_splits: [],
    }
}

/// `init-for-account` (:1255-1259): an account's collector, active in the window when it has splits in no lot.
private fn account_stats(account, unassigned_splits) {
    let st = new_stats("account");
    st.account = account;
    st.unassigned_splits = unassigned_splits;
    st.is_active_in_window = !unassigned_splits.is_empty();
    st
}

/// `merge-lot` (:1663-1712): the lot's splits merged in order, then the end value at the price in the report's
/// currency, the unrealized gain short or long term by the latest purchase, and whether the lot is active in the
/// window: sold in it, or holding shares at its end.
private fn merge_lot(env, lot, currency, price) {
    let st = new_stats("lot");
    st.lot = lot;
    st.currency = currency;
    for split in lot_splits_up_to_date(lot, env.to_date) {
        st = merge_split(env, st, split);
    }
    st.end_value = if undefined(price) {
        rational(0)
    } else {
        to_report_scu(env, num_mul(st.end_amount, price_report_value(env, price)))
    };
    st.unrealized_gain = num_sub(st.end_value, st.end_basis);
    let is_lot_long_term = long_term(env, st.latest_bought_split_date, env.to_date);
    st.st_unrealized_gain = if is_lot_long_term { rational(0) } else { st.unrealized_gain };
    st.lt_unrealized_gain = if is_lot_long_term { st.unrealized_gain } else { rational(0) };
    st.is_active_in_window = st.sold_split_count > 0 || num_positive(st.end_amount) || st.has_warnings;
    st
}

/// `merge-split` (:1715-1757): one split by its kind, its value in the report's currency.
private fn merge_split(env, st, split) {
    let trans = split.transaction;
    let trans_date = trans.datePosted;
    let value = report_value(env, split.value, trans.currency);
    let amount = split.amount;
    switch classify_lot_split(split_type(split), split.action, amount, value) {
        "purchase" => merge_purchase_split(env, st, split, trans_date, amount, value),
        "non-realizing-share-reduction" => merge_share_reduction(env, st, split, trans_date, -amount),
        "sale" => merge_sale_split(env, st, split, trans_date, -amount, -value),
        _ => {
            // A "Realized Gain/Loss" split has no amount; its value is checked against the computed gains.
            if trans_date >= env.from_date {
                st.splits_realized_gain = num_add(st.splits_realized_gain, value);
            }
            st
        }
    }
}

/// `merge-non-realizing-share-reduction-split` (:1761-1767): fewer shares, the basis and gains unchanged.
private fn merge_share_reduction(env, st, split, trans_date, amount) {
    st.end_amount = num_sub(st.end_amount, amount);
    if trans_date >= env.from_date && undefined(st.first_negative_split) && num_negative(st.end_amount) {
        st.first_negative_split = split;
    }
    st
}

/// `accumulate-sale-term!` (:1771-1791).
private fn accumulate_sale_term(st, is_long_term, amount, basis, value, gain) {
    if is_long_term {
        st.lt_sold_amount = num_add(st.lt_sold_amount, amount);
        st.lt_sold_basis = num_add(st.lt_sold_basis, basis);
        st.lt_sold_value = num_add(st.lt_sold_value, value);
        st.lt_realized_gain = num_add(st.lt_realized_gain, gain);
    } else {
        st.st_sold_amount = num_add(st.st_sold_amount, amount);
        st.st_sold_basis = num_add(st.st_sold_basis, basis);
        st.st_sold_value = num_add(st.st_sold_value, value);
        st.st_realized_gain = num_add(st.st_realized_gain, gain);
    }
    st
}

/// `merge-sale-split` (:1824-1884): the sold fraction of the lot's shares takes that fraction of its basis, rounded
/// to the report currency's SCU, and so does the gain; the basis and shares go down also for a sale before the
/// window. A sale against no shares divides by zero, so its basis and everything after it is `()`. In the window: the
/// years held from the latest purchase and the CAGR, weighted by shares; the first negative balance; the sums, the
/// split row, the count and the last sale. With no purchase the years are `()` and so is the CAGR, where GnuCash's
/// report fails (`gnc:date-year-delta` on `'()`, :1860-1862).
private fn merge_sale_split(env, st, split, trans_date, amount, value) {
    let sold_frac = num_div(amount, st.end_amount);
    let basis = to_report_scu(env, num_mul(sold_frac, st.end_basis));
    let gain = to_report_scu(env, num_sub(value, basis));
    let is_long_term = long_term(env, st.latest_bought_split_date, trans_date);
    st.end_basis = num_sub(st.end_basis, basis);
    st.end_amount = num_sub(st.end_amount, amount);
    if trans_date >= env.from_date {
        let years_held = years_between(env, st.latest_bought_split_date, trans_date);
        let cagr = calculate_cagr(basis, value, years_held);
        if undefined(st.first_negative_split) && num_negative(st.end_amount) {
            st.first_negative_split = split;
        }
        if !undefined(cagr) {
            st.weighted_realized_cagr = num_add(st.weighted_realized_cagr, cagr * amount);
        }
        st = accumulate_sale_term(st, is_long_term, amount, basis, value, gain);
        st = add_split_row(env, st, split, trans_date, #{
            bought_amount: (), bought_value: (), sold_amount: amount, sold_basis: basis, sold_value: value,
            sold_gain: gain, is_long_term: is_long_term, cagr: cagr,
        });
        st.sold_split_count += 1;
        st.last_sold_split = split;
    }
    st
}

/// `merge-purchase-split` (:1886-1925): the earliest and latest purchase dates, the basis and shares, the bought
/// sums in the window or before it, the split row, and the count of purchases, those before the window too.
private fn merge_purchase_split(env, st, split, trans_date, amount, value) {
    if undefined(st.earliest_bought_split) || trans_date < st.earliest_bought_split_date {
        st.earliest_bought_split = split;
        st.earliest_bought_split_date = trans_date;
    }
    if undefined(st.latest_bought_split_date) || trans_date > st.latest_bought_split_date {
        st.latest_bought_split_date = trans_date;
    }
    st.end_basis = num_add(st.end_basis, value);
    st.end_amount = num_add(st.end_amount, amount);
    if trans_date >= env.from_date {
        st.bought_amount = num_add(st.bought_amount, amount);
        st.bought_value = num_add(st.bought_value, value);
        st = add_split_row(env, st, split, trans_date, #{
            bought_amount: amount, bought_value: value, sold_amount: (), sold_basis: (), sold_value: (),
            sold_gain: (), is_long_term: false, cagr: (),
        });
    } else {
        st.old_bought_value = num_add(st.old_bought_value, value);
        st.old_bought_amount = num_add(st.old_bought_amount, amount);
    }
    st.bought_split_count += 1;
    st
}

/// `merge-stats` (:1928-2034): another collector's figures added in. Shares are added only into an account's
/// collector, which takes the first lot's commodity; the grand total mixes commodities, so it has none.
private fn merge_stats(st, other) {
    st.bought_split_count += other.bought_split_count;
    st.sold_split_count += other.sold_split_count;
    for key in ["old_bought_value", "bought_value", "st_sold_basis", "st_sold_value", "st_realized_gain",
                "lt_sold_basis", "lt_sold_value", "lt_realized_gain", "weighted_realized_cagr", "end_basis",
                "end_value", "unrealized_gain", "st_unrealized_gain", "lt_unrealized_gain",
                "weighted_unrealized_cagr"] {
        st[key] = num_add(st[key], other[key]);
    }
    st.has_warnings = st.has_warnings || other.has_warnings;
    st.is_active_in_window = st.is_active_in_window || other.is_active_in_window;
    if st.kind == "account" {
        for key in ["old_bought_amount", "bought_amount", "st_sold_amount", "lt_sold_amount", "end_amount"] {
            st[key] = num_add(st[key], other[key]);
        }
        if undefined(st.currency) {
            st.currency = other.currency;
        }
    }
    st
}

// Cells and rows.

/// `account->name` (:791-795).
private fn account_name(env, account) {
    if env.show_long_account_names { account.fullName } else { account.name }
}

/// `amount->monetary-string` (:806-812): `amount` printed in `currency` with GnuCash's commodity print info, its
/// symbol only with "Show mnemonic in amounts". An account collector that merged no lot has no commodity, and
/// GnuCash prints with the default print info, the default currency's (`gnc_commodity_print_info` of NULL); red-ale's
/// stand-in for the default currency is the report settings' currency.
private fn amount_string(env, currency, amount) {
    if undefined(amount) {
        return "";
    }
    let commodity = if undefined(currency) { env.ctx.report_currency() } else { currency };
    monetary(commodity, amount).to_string(env.show_mnemonics)
}

/// `value->monetary-string` (:874-877): `value` in the report's currency with its symbol, unrounded; `""` for `()`,
/// as `xaccPrintAmount` prints an error numeric.
private fn value_string(env, value) {
    if undefined(value) { "" } else { monetary(env.currency, value).to_string(true) }
}

/// `get-average-price` (:967-978): `value / amount`, 0 for no shares, `()` when either is undefined.
private fn average_price(amount, value) {
    if undefined(amount) || undefined(value) {
        ()
    } else if amount == 0 {
        rational(0)
    } else {
        value / amount
    }
}

/// `to-cell` (:1119-1125): `make()`'s cell, or `()` for an undefined figure or, outside total rows with "Show blanks
/// instead of zeros", a zero.
private fn to_cell(env, is_bold, val, cell) {
    if undefined(val) || (!is_bold && env.show_blanks_for_zeros && val == 0) { () } else { cell }
}

/// `amount->cell` (:1129-1133): shares printed in the lot's commodity.
private fn amount_cell(env, is_bold, currency, amount) {
    to_cell(env, is_bold, amount, #{ text: amount_string(env, currency, amount) })
}

/// `integer->cell` (:1136-1139): a count, `~d`.
private fn integer_cell(env, is_bold, number) {
    to_cell(env, is_bold, number, #{ text: if undefined(number) { "" } else { number.to_string() } })
}

/// `percentage->cell` (:1141-1162): `~,1f%`, half away from zero; `number` is already a percentage.
private fn percentage_cell(env, is_bold, number) {
    to_cell(env, is_bold, number, #{ number: number, places: 1, suffix: "%", rounding: "halfUp" })
}

/// `value->cell` (:1166-1170): a Monetary in the report's currency.
private fn value_cell(env, is_bold, value) {
    to_cell(env, is_bold, value, #{ monetary: if undefined(value) { () } else { monetary(env.currency, value) } })
}

/// A date cell linked to `split` (`to-split-cell`, :829-837), as `qof-print-date` prints the date.
private fn split_date_cell(date, split) {
    #{ date: date, link: #{ split: split } }
}

/// `compute-gain-metrics` (:499-513): the total gain and the ROI and CAGR as percentages. The ROI is `()` without a
/// basis or a total, 0 for a zero basis, else `calculate-roi`.
private fn gain_metrics(basis, short_gain, long_gain, cagr) {
    let total_gain = num_add(short_gain, long_gain);
    let roi = if undefined(basis) || undefined(total_gain) {
        ()
    } else if basis == 0 {
        rational(0)
    } else {
        calculate_roi(basis, total_gain)
    };
    [total_gain, num_mul(roi, 100), num_mul(cagr, 100)]
}

/// `get-gains-fn` (:1174-1202): the gain columns of one kind, realized or unrealized.
private fn gains_cells(env, is_bold, show_columns, basis, short_gain, long_gain, cagr) {
    if !show_columns {
        return [];
    }
    let metrics = gain_metrics(basis, short_gain, long_gain, cagr);
    let cells = if env.group_gains_by_age {
        [value_cell(env, is_bold, short_gain), value_cell(env, is_bold, long_gain)]
    } else {
        [value_cell(env, is_bold, metrics[0])]
    };
    if env.show_roi_columns {
        cells.push(percentage_cell(env, is_bold, metrics[1]));
    }
    if env.show_cagr_columns {
        cells.push(percentage_cell(env, is_bold, metrics[2]));
    }
    cells
}

/// The sold columns of a row (:1240-1271).
private fn sold_cells(env, is_bold, currency, f) {
    let sold_amount = num_add(f.st_sold_amount, f.lt_sold_amount);
    let sold_value = num_add(f.st_sold_value, f.lt_sold_value);
    let sold_basis = num_add(f.st_sold_basis, f.lt_sold_basis);
    let cells = [integer_cell(env, is_bold, f.sold_split_count)];
    cells += if env.group_sales_by_age {
        [amount_cell(env, is_bold, currency, f.st_sold_amount), value_cell(env, is_bold, f.st_sold_basis),
         value_cell(env, is_bold, f.st_sold_value), amount_cell(env, is_bold, currency, f.lt_sold_amount),
         value_cell(env, is_bold, f.lt_sold_basis), value_cell(env, is_bold, f.lt_sold_value)]
    } else {
        [amount_cell(env, is_bold, currency, sold_amount), value_cell(env, is_bold, sold_basis),
         value_cell(env, is_bold, sold_value)]
    };
    cells.push(value_cell(env, is_bold, average_price(sold_amount, sold_value)));
    cells
}

/// A rule across the table (`gnc:html-table-append-ruler!`).
private fn rule_row(env) {
    let cells = [];
    cells.pad(env.column_count, ());
    #{ role: "rule", cells: cells }
}

/// `add-data-row` (:1101-1306): a row of `f`'s figures under the shown columns, with a rule before a total row
/// (`is_bold`). Total rows are "grand-total" rows; others take `is_odd_row`'s shade.
private fn add_data_row(env, amount_currency, is_bold, is_odd_row, first_cell, lot, open_cell, close_cell, f) {
    let rows = if is_bold { [rule_row(env)] } else { [] };
    let cells = [first_cell];
    if env.show_lot_guid_column {
        cells.push(if undefined(lot) { () } else { #{ text: lot.guid } });
    }
    if env.show_date_columns {
        cells += [open_cell, close_cell];
    }
    if env.show_bought_columns {
        cells += [amount_cell(env, is_bold, amount_currency, f.bought_amount), value_cell(env, is_bold, f.bought_value),
                  value_cell(env, is_bold, average_price(f.bought_amount, f.bought_value))];
    }
    if env.show_sold_columns {
        cells += sold_cells(env, is_bold, amount_currency, f);
    }
    if env.show_end_columns {
        cells += [amount_cell(env, is_bold, amount_currency, f.end_amount), value_cell(env, is_bold, f.end_basis),
                  value_cell(env, is_bold, f.end_value)];
    }
    let sold_basis = num_add(f.st_sold_basis, f.lt_sold_basis);
    cells += gains_cells(env, is_bold, env.show_realized_gain_columns, sold_basis, f.st_realized_gain,
                         f.lt_realized_gain, f.realized_cagr);
    cells += gains_cells(env, is_bold, env.show_unrealized_gain_columns, f.end_basis, f.st_unrealized_gain,
                         f.lt_unrealized_gain, f.unrealized_cagr);
    rows.push(if is_bold { #{ role: "grandTotal", cells: cells } } else { #{ alternate: !is_odd_row, cells: cells } });
    rows
}

/// `add-split-row` (:1262-1312): with "Show lot split rows", a row for a purchase or a sale in the window under its
/// lot, with the lot's running end amount and basis; a sale's figures go to its term's columns.
private fn add_split_row(env, st, split, trans_date, s) {
    st
}

/// The row of a collector (`add-to-table`, :1315-1440): the figures, the lot's split rows and warnings after a lot
/// row; the lot's unrealized CAGR weighted by its shares is kept for the account's. Gives `#{ rows, stats }`.
private fn add_to_table(env, st, is_odd_row) {
    let is_lot = st.kind == "lot";
    let is_grand = st.kind == "grand";
    let open_cell = if is_lot && !undefined(st.earliest_bought_split_date) {
        split_date_cell(st.earliest_bought_split_date, st.earliest_bought_split)
    } else {
        ()
    };
    let close_cell = if is_lot && num_zero(st.end_amount) && !undefined(st.last_sold_split) {
        split_date_cell(st.last_sold_split.transaction.datePosted, st.last_sold_split)
    } else {
        ()
    };
    let first_cell = switch st.kind {
        "lot" => if undefined(st.lot.title) { () } else { #{ text: st.lot.title } },
        "account" => #{ text: "Account Lots Total" },
        _ => #{ text: "Grand Total" },
    };
    let years_held = if is_lot { years_between(env, st.latest_bought_split_date, env.to_date) } else { () };
    let unrealized_cagr = switch st.kind {
        "lot" => if undefined(st.end_value) { () } else { calculate_cagr(st.end_basis, st.end_value, years_held) },
        "account" => if num_zero(st.end_amount) { rational(0) } else { num_div(st.weighted_unrealized_cagr, st.end_amount) },
        _ => (),
    };
    let sold_amount = num_add(st.lt_sold_amount, st.st_sold_amount);
    let realized_cagr = if is_grand {
        ()
    } else if num_zero(sold_amount) {
        rational(0)
    } else {
        num_div(st.weighted_realized_cagr, sold_amount)
    };
    // The grand total mixes commodities, so it shows no shares.
    let f = #{
        bought_amount: unless_grand(is_grand, st.bought_amount), bought_value: st.bought_value,
        sold_split_count: st.sold_split_count,
        st_sold_amount: unless_grand(is_grand, st.st_sold_amount), st_sold_basis: st.st_sold_basis,
        st_sold_value: st.st_sold_value,
        lt_sold_amount: unless_grand(is_grand, st.lt_sold_amount), lt_sold_basis: st.lt_sold_basis,
        lt_sold_value: st.lt_sold_value,
        end_amount: unless_grand(is_grand, st.end_amount), end_basis: st.end_basis, end_value: st.end_value,
        st_realized_gain: st.st_realized_gain, lt_realized_gain: st.lt_realized_gain, realized_cagr: realized_cagr,
        st_unrealized_gain: st.st_unrealized_gain, lt_unrealized_gain: st.lt_unrealized_gain,
        unrealized_cagr: unrealized_cagr,
    };
    let currency = if is_grand { () } else { st.currency };
    let rows = add_data_row(env, currency, !is_lot, is_odd_row, first_cell, if is_lot { st.lot } else { () },
                            open_cell, close_cell, f);
    if is_lot {
        if !undefined(unrealized_cagr) {
            st.weighted_unrealized_cagr = num_add(st.weighted_unrealized_cagr, num_mul(unrealized_cagr, st.end_amount));
        }
        for row in st.split_rows {
            row.alternate = !is_odd_row;
            rows.push(row);
        }
    }
    #{ rows: rows, stats: st }
}

// The table.

/// `x`, or `()` in the grand total, whose header and figures leave out what mixes commodities.
private fn unless_grand(is_grand_total, label) {
    if is_grand_total { () } else { label }
}

/// The gain labels of one kind (`get-gains-column-header-list`, :989-1004).
private fn gains_labels(env, is_grand_total, short_term, long_term, gain, roi, cagr) {
    let labels = if env.group_gains_by_age { [short_term, long_term] } else { [gain] };
    if env.show_roi_columns {
        labels.push(roi);
    }
    if env.show_cagr_columns {
        labels.push(unless_grand(is_grand_total, cagr));
    }
    labels
}

/// `get-column-header-list` (:984-1063): the labels of the shown columns; for the grand total's header, `()` over
/// the columns whose figures mix commodities or would make no sense there.
private fn column_header_list(env, is_grand_total) {
    let g = is_grand_total;
    let labels = [unless_grand(g, "Lot Title")];
    if env.show_lot_guid_column {
        labels.push(unless_grand(g, "GUID"));
    }
    if env.show_date_columns {
        labels += [unless_grand(g, "Opened"), unless_grand(g, "Closed")];
    }
    if env.show_bought_columns {
        labels += [unless_grand(g, "Bought Amount"), "Bought Value (Basis)",
                   unless_grand(g, "Bought Average Price")];
    }
    if env.show_sold_columns {
        labels.push("Sold Splits");
        labels += if env.group_sales_by_age {
            [unless_grand(g, "ST Sold Amount"), "ST Sold Basis", "ST Sold Value",
             unless_grand(g, "LT Sold Amount"), "LT Sold Basis", "LT Sold Value"]
        } else {
            [unless_grand(g, "Sold Amount"), "Sold Basis", "Sold Value"]
        };
        labels.push(unless_grand(g, "Sold Average Price"));
    }
    if env.show_end_columns {
        labels += [unless_grand(g, "End Amount"), "End Basis", "End Value"];
    }
    if env.show_realized_gain_columns {
        labels += gains_labels(env, g, "ST Realized Gain", "LT Realized Gain", "Realized Gain", "Realized ROI",
                               "Realized CAGR");
    }
    if env.show_unrealized_gain_columns {
        labels += gains_labels(env, g, "ST Unrealized Gain", "LT Unrealized Gain", "Unrealized Gain", "Unrealized ROI",
                               "Unrealized CAGR");
    }
    labels
}

/// The table's columns: the lot title, the GUID as text, the rest figures, right-aligned as GnuCash's number cells.
private fn columns(env) {
    let labels = column_header_list(env, false);
    let cols = [];
    for i in 0..labels.len() {
        let label = labels[i];
        cols.push(if i == 0 {
            #{ label: label, role: "label" }
        } else if label == "GUID" {
            #{ label: label, role: "text" }
        } else {
            #{ label: label, role: "amount", align: "right" }
        });
    }
    cols
}

/// `add-header-row` (:1070-1076): the column labels as a heading row.
private fn header_row(env, is_grand_total) {
    #{ role: "heading", cells: column_header_list(env, is_grand_total).map(|label| if type_of(label) == "()" { () } else { #{ text: label } }) }
}

/// `add-padding-rows` (:1310-1321): `row_count` blank rows.
private fn padding_rows(env, row_count) {
    let cells = [];
    cells.pad(env.column_count, ());
    let rows = [];
    for i in 0..row_count {
        rows.push(#{ role: "spacer", cells: cells });
    }
    rows
}

/// The account's name, linked to it, across the table (:2244-2253).
private fn account_name_row(env, account) {
    #{ role: "heading", account: account,
       cells: [#{ text: account_name(env, account), account: account, span: env.column_count }] }
}

/// `add-lots-rows` (:2185-2262): the account's name, its price and the header, a row per lot active in the window
/// (closed lots only with "Include closed lots"), and the account's total. The account is shown when it is active in
/// the window and, with "Include only accounts with warnings", has a warning; its figures then go to the grand total
/// and the chart.
private fn add_lots_rows(env, run, account, currency, price, lots, unassigned_splits) {
    let acct = account_stats(account, unassigned_splits);
    let rows = [account_name_row(env, account), header_row(env, false)];
    let is_odd_row = true;
    for lot in lots {
        let lot_info = merge_lot(env, lot, currency, price);
        if lot_info.is_active_in_window && (env.include_closed_lots || !num_zero(lot_info.end_amount)) {
            let added = add_to_table(env, lot_info, is_odd_row);
            rows += added.rows;
            acct = merge_stats(acct, added.stats);
            is_odd_row = !is_odd_row;
        }
    }
    let added = add_to_table(env, acct, true);
    rows += added.rows;
    acct = added.stats;
    if acct.is_active_in_window && (!env.include_only_accounts_with_warnings || acct.has_warnings) {
        run.rows += padding_rows(env, 3);
        run.rows += rows;
        run.grand = merge_stats(run.grand, acct);
    }
    run
}

/// `add-account-report` (:2072-2264): the account's lots, when it holds shares at the End date or "Include accounts
/// with no shares" is on, and has lots or "Include accounts with no lots" is on.
private fn add_account_report(env, run, account) {
    let currency = account.commodity;
    let price = price_of(env, currency);
    let collected = collect_lots_and_unassigned_splits(get_all_splits(env, account));
    let lots = collected[0];
    let balance = balances::account_get_comm_balance_at_date(account, env.to_date, false).getpair(currency, false)[1];
    if !(env.include_empty_accounts || balance != 0) || !(env.include_lotless_accounts || !lots.is_empty()) {
        return run;
    }
    add_lots_rows(env, run, account, currency, price, lots, collected[1])
}

/// The options and what follows from them (:639-764).
private fn report_env(book, opts, ctx) {
    let group = opts.get("Columns", "Group gains by age (short term and long term)");
    let currency = opts.get("General", "Report's currency");
    let price_source = opts.get("General", "Price source");
    let to_date = dates::time64_end_day_time(ctx, opts.get("General", "End date"));
    let env = #{
        book: book, ctx: ctx,
        accounts: opts.get("Accounts", "Accounts"),
        include_empty_accounts: opts.get("Accounts", "Include accounts with no shares"),
        include_lotless_accounts: opts.get("Accounts", "Include accounts with no lots"),
        show_chart: opts.get("Chart", "Show Chart"),
        chart_type: opts.get("Chart", "Chart type"),
        chart_location: opts.get("Chart", "Chart location"),
        chart_width: opts.get("Chart", "Plot width"),
        chart_height: opts.get("Chart", "Plot height"),
        show_lot_guid_column: opts.get("Columns", "Show lot GUID column"),
        show_date_columns: opts.get("Columns", "Show date columns"),
        show_bought_columns: opts.get("Columns", "Show bought columns"),
        show_sold_columns: opts.get("Columns", "Show sold columns"),
        show_end_columns: opts.get("Columns", "Show end columns"),
        show_realized_gain_columns: opts.get("Columns", "Show realized gain columns"),
        show_unrealized_gain_columns: opts.get("Columns", "Show unrealized gain columns"),
        group_gains_by_age: group != "no",
        group_sales_by_age: group == "gains-and-sales",
        long_term_years: opts.get("Columns", "Long term gains age (years)"),
        show_roi_columns: opts.get("Columns", "Show ROI columns"),
        show_cagr_columns: opts.get("Columns", "Show CAGR columns"),
        show_long_account_names: opts.get("Display", "Show long account names"),
        show_mnemonics: opts.get("Display", "Show mnemonic in amounts"),
        include_closed_lots: opts.get("Display", "Include closed lots"),
        show_blanks_for_zeros: opts.get("Display", "Show blanks instead of zeros in table cells"),
        show_split_rows: opts.get("Display", "Show lot split rows"),
        from_date: dates::time64_end_day_time(ctx, opts.get("General", "Start date")),
        to_date: to_date,
        currency: currency,
        fraction: currency.fraction,
        price_source: price_source,
        exchange_fn: exchange::case_exchange_fn(book, price_source, currency, to_date),
        include_only_accounts_with_warnings: opts.get("Validation", "Include only accounts with warnings"),
        warn_if_multiple_bought_splits: opts.get("Validation", "Warn if a lot has more than one bought split"),
        warn_if_balance_negative: opts.get("Validation", "Warn if a lot's balance drops below zero"),
        warn_if_lot_title_blank: opts.get("Validation", "Warn if a lot has a blank title"),
        warn_if_gains_mismatch: opts.get("Validation",
                                         "Warn if the 'Realized Gain/Loss' split(s) sum does not match the computed gains"),
        warn_type_if_split_not_in_lot: opts.get("Validation", "Warn if a split is not assigned to a lot"),
        warn_if_balance_mismatch: opts.get("Validation",
                                           "Warn if the account balance does not match the computed lots' end balance"),
    };
    env.column_count = column_header_list(env, false).len();
    env
}

/// `investment-lots-renderer` (:607-2318). Both dates are taken at the end of their day (:667-672), so the Start
/// date's own trades fall before the window.
fn render(params, book, opts, ctx) {
    let env = report_env(book, opts, ctx);
    let title = opts.get("General", "Report name") + ", " + ctx.print_date_interval(env.from_date, env.to_date);
    let run = #{ rows: [], grand: new_stats("grand") };
    if !env.accounts.is_empty() {
        for account in env.accounts {
            run = add_account_report(env, run, account);
        }
        run.rows += padding_rows(env, 3);
        run.rows += [rule_row(env), rule_row(env)];
        run.rows.push(header_row(env, true));
        run.rows += add_to_table(env, run.grand, true).rows;
    }
    let table = #{ kind: "table", columns: columns(env), rows: run.rows };
    #{ title: title, sections: [#{ blocks: [table] }] }
}
