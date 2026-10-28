// advanced-portfolio.scm
// by Martijn van Oosterhout (kleptog@svana.org) Feb 2002
// modified for GnuCash 1.8 by Herbert Thoma (herbie@hthoma.de) Oct 2002
//
// Heavily based on portfolio.scm
// by Robert Merkel (rgmerk@mira.net)
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
// Translated from 5.17:gnucash/report/reports/standard/advanced-portfolio.scm, cited below by line.
//
// The Advanced Portfolio: for each selected account its shares, price, basis, value, money in and out, realized and
// unrealized gains, income, brokerage fees and the rates of gain and return, worked out from the account's own
// splits in split order (design/notes/report-investments.md §3). Every figure is exact; GnuCash's roundings of units
// and prices to 10⁻⁸ and of money to a currency's SCU are `gnc_numeric` conversions, reproduced here with banker's
// rounding where it makes them. Only the two rates are doubles in GnuCash; here they are exact, and a number cell
// rounds them half away from zero as Guile's `~,2f` does. The progress calls and debug output are left out.

import "lib/accounts" as accounts;
import "lib/balances" as balances;
import "lib/collectors" as collectors;
import "lib/dates" as dates;
import "lib/exchange" as exchange;
import "lib/html-utilities" as html_utils;
import "lib/options" as opts_util;

/// The template (:1184-1190).
fn templates() {
    [#{ guid: "21d7cfc59fc74f22887596ebde7e462d", name: "Advanced Portfolio", menuPath: ["_Assets & Liabilities"],
        params: #{} }]
}

/// `options-generator` (:61-140). The accounts default to the STOCK and MUTUAL accounts (`gnc:account-is-stock?`,
/// RU:154-156) in display order.
fn options(params, o, book) {
    opts_util::add_report_date(o, "General", "Date", "a");
    opts_util::add_currency(o, "General", "Report's currency", "c");
    o.multichoice("General", "Price Source", "d", "The source of price information.", "pricedb-nearest", [
        ["pricedb-latest", "Most recent"],
        ["pricedb-nearest", "Nearest to report date"],
    ]);
    o.multichoice("General", "Basis calculation method", "e", "Basis calculation method.", "average-basis", [
        ["average-basis", "Average cost of all shares"],
        ["fifo-basis", "First-in first-out"],
        ["filo-basis", "Last-in first-out"],
    ]);
    o.boolean("General", "Set preference for price list data", "f",
              "Prefer use of price editor pricing over transactions, where applicable.", true);
    o.multichoice("General", "How to report brokerage fees", "g", "How to report commissions and other brokerage fees.",
                  "include-in-basis", [
        ["include-in-basis", "Include in basis"],
        ["include-in-gain", "Include in gain/loss"],
        ["ignore-brokerage", "Omit from report"],
    ]);

    o.boolean("Display", "Show ticker symbols", "a", "Display the ticker symbols.", true);
    o.boolean("Display", "Show listings", "b", "Display exchange listings.", true);
    o.boolean("Display", "Show number of shares", "c", "Display numbers of shares in accounts.", true);
    o.number_range("Display", "Share decimal places", "d", "The number of decimal places to use for share numbers.",
                   2, 0, 9, 1);
    o.boolean("Display", "Show prices", "e", "Display share prices.", true);

    o.account_list("Accounts", "Accounts", "b", "Stock Accounts to report on.",
                   accounts::filter_accountlist_type(["STOCK", "MUTUAL"], book.root().descendants_sorted()),
                   #{ types: ["ASSET", "BANK", "STOCK", "MUTUAL"] });
    o.boolean("Accounts", "Include accounts with no shares", "e", "Include accounts that have a zero share balances.",
              false);

    o.set_default_section("General");
}

/// "Share decimal places" as a count of places. GnuCash passes `inexact->exact` of the number to
/// `gnc-share-print-info-places` (:367-370), which takes only a whole number; so does red-ale, with a message.
private fn share_places(value) {
    if !value.is_integer() {
        throw `Share decimal places must be a whole number, not ${value}`;
    }
    parse_int(value.to_string())
}

/// True for `()`: GnuCash's `#f` or null.
private fn is_none(x) {
    type_of(x) == "()"
}

// Accounts.

/// The type of `account`, `"NONE"` for no account, as `xaccAccountGetType` of a null account gives `ACCT_TYPE_NONE`.
private fn account_type(account) {
    if type_of(account) == "Account" { account.type } else { "NONE" }
}

/// `same-account?` (:166-167): the accounts' GUIDs are equal, a missing account having the null GUID
/// (`gncAccountGetGUID` of null), so two missing accounts are the same.
private fn same_account(a1, a2) {
    let m1 = type_of(a1) != "Account";
    let m2 = type_of(a2) != "Account";
    if m1 || m2 { m1 && m2 } else { a1 == a2 }
}

/// `parent-or-sibling?` (:339-344): either account is the other's parent, or they share a parent.
private fn parent_or_sibling(a1, a2) {
    let a1parent = a1.parent;
    let a2parent = a2.parent;
    same_account(a2parent, a1) || same_account(a1parent, a2) || same_account(a1parent, a2parent)
}

/// `spin-off?` (:350-356): a split of `current` with no units in a transaction of two splits whose other split is in
/// neither an EXPENSE nor an INCOME account.
private fn spin_off(split, current) {
    if !split.amount.is_zero() || !same_account(current, split.account) {
        return false;
    }
    let other = split.otherSplit;
    type_of(other) != "()" && !["EXPENSE", "INCOME"].contains(account_type(other.account))
}

// The basis list (:169-319): `[units, price]` pairs in split order. The average method keeps one pair.

/// `x` rounded to 10⁻⁸ with banker's rounding: GnuCash's `units-denom` and `price-denom` with `GNC-RND-ROUND`
/// (:57-59).
private fn to_u(x) {
    x.round_to(100000000, "round")
}

/// `x` rounded to the fraction `frac` with banker's rounding (`GNC-RND-ROUND`).
private fn to_frac(x, frac) {
    x.round_to(frac, "round")
}

/// `sum-basis` (:170-176): each pair's units × price rounded to `frac`, summed; exact when `frac` is `()`, GnuCash's
/// `GNC-DENOM-AUTO` (:304).
private fn sum_basis(b_list, frac) {
    let sum = rational(0);
    for pair in b_list {
        let product = pair[0] * pair[1];
        sum += if is_none(frac) { product } else { to_frac(product, frac) };
    }
    sum
}

/// `units-basis` (:179-185): the units, added from the last pair, each sum rounded to 10⁻⁸.
private fn units_basis(b_list) {
    let sum = rational(0);
    let i = b_list.len();
    while i > 0 {
        i -= 1;
        sum = to_u(b_list[i][0] + sum);
    }
    sum
}

/// `apply-basis-ratio` (:189-196): every pair's units × `units_ratio` and price × `value_ratio`, each to 10⁻⁸.
private fn apply_basis_ratio(b_list, units_ratio, value_ratio) {
    b_list.map(|pair| [to_u(units_ratio * pair[0]), to_u(value_ratio * pair[1])])
}

/// `basis-builder` (:202-319): the basis list after `b_units` shares changed hands for `b_value` (report currency)
/// under `b_method`, or `#{ div0: reason }` where GnuCash throws `div/0`. Selling more than the first (FIFO) or last
/// (LIFO) pair holds drops it and sells the rest from the next; GnuCash recurses, red-ale loops, so the call depth
/// does not grow with the number of pairs.
private fn basis_builder(b_list, b_units, b_value, b_method) {
    loop {
        if !b_value.is_zero() && b_units > 0 {
            return add_to_basis(b_list, b_units, b_value, b_method);
        }
        if !b_value.is_zero() && b_units < 0 {
            if b_list.is_empty() {
                return [];
            }
            if b_method == "average-basis" {
                return [[to_u(b_list[0][0] + b_units), b_list[0][1]]];
            }
            if b_method != "fifo-basis" && b_method != "filo-basis" {
                throw `not a basis calculation method: ${b_method}`;
            }
            // The pair sold from: the first (FIFO, :242-255) or the last (LIFO, :256-271).
            let i = if b_method == "fifo-basis" { 0 } else { b_list.len() - 1 };
            let held = b_list[i][0];
            let sold = b_units.abs();
            if sold < held {
                b_list[i] = [to_u(b_units + held), b_list[i][1]];
                return b_list;
            }
            b_list.remove(i);
            if sold == held {
                return b_list;
            }
            // Only the sign of b-value matters from here (:254).
            b_units = to_u(b_units + held);
            continue;
        }
        if b_value.is_zero() && !b_units.is_zero() {
            return split_basis(b_list, b_units);
        }
        if b_units.is_zero() && !b_value.is_zero() {
            return spin_off_basis(b_list, b_value);
        }
        return b_list;
    }
}

/// A purchase (:211-235): the average method's one pair takes the new units at the weighted price, both to 10⁻⁸, the
/// numerator and divisor exact (`GNC-DENOM-REDUCE`); FIFO and LIFO, and the average from nothing, append a pair at
/// the value per unit.
private fn add_to_basis(b_list, b_units, b_value, b_method) {
    if b_method == "average-basis" && !b_list.is_empty() {
        let held = b_list[0];
        let denom = b_units + held[0];
        if denom.is_zero() {
            // `~0,4f` of the units (:227).
            return #{ div0: `buying ${fixed(b_units, 0, 4)} share units` };
        }
        return [[to_u(b_units + held[0]), to_u((b_value + held[0] * held[1]) / denom)]];
    }
    b_list.push([b_units, to_u(b_value / b_units)]);
    b_list
}

/// A split or merger (:280-297): units scale by (units + held) / held and prices by its inverse; with nothing held,
/// or nothing left, both ratios are zero (a worthless stock).
private fn split_basis(b_list, b_units) {
    let current_units = units_basis(b_list);
    let units_ratio = if current_units.is_zero() { rational(0) } else { (b_units + current_units) / current_units };
    let value_ratio = if units_ratio.is_zero() { rational(0) } else { 1 / units_ratio };
    apply_basis_ratio(b_list, units_ratio, value_ratio)
}

/// A spin-off (:302-313): prices scale by (value + basis) / basis, the basis exact.
private fn spin_off_basis(b_list, b_value) {
    let current_value = sum_basis(b_list, ());
    if current_value.is_zero() {
        // `~,2f` of the basis, always zero here (:306).
        return #{ div0: `spinoff of ${fixed(current_value, (), 2)} currency units` };
    }
    apply_basis_ratio(b_list, rational(1), (b_value + current_value) / current_value)
}

// Prices (:321-336, :393-490).

/// `find-price` (:324-336): from the first price of `price_list`, any price in `currency` replaces it and any price
/// of `currency` replaces it inverted (`gnc-price-invert`, an exact reciprocal, 0 for 0); the last match wins. A price
/// is `#{ price, currency, value }`: the stored price and, inverted or not, its currency and value. `()` when the
/// list is empty.
private fn find_price(price_list, currency) {
    if price_list.is_empty() {
        return ();
    }
    let price = #{ price: price_list[0], currency: price_list[0].currency, value: price_list[0].value };
    for p in price_list {
        if p.currency == currency {
            price = #{ price: p, currency: p.currency, value: p.value };
        }
        if p.commodity == currency {
            price = #{ price: p, currency: p.commodity, value: if p.value.is_zero() { rational(0) } else { 1 / p.value } };
        }
    }
    price
}

/// `price-fn` (:1011-1025): the price of `foreign` in `domestic` from the price database: the latest at or before now,
/// or the nearest to local noon of `date`. GnuCash's `pricedb-before` branch is never chosen; the option has no such
/// value.
private fn price_fn(env, foreign, domestic, date) {
    let prices = if env.price_source == "pricedb-latest" {
        env.book.prices_any_currency(foreign, env.ctx.now, "latest")
    } else {
        env.book.prices_any_currency(foreign, dates::time64_canonical_day_time(env.ctx, date), "nearest")
    };
    find_price(prices, domestic)
}

/// The newest split of `current` with two commodities, up to now (latest) or the Date (nearest), whose share price
/// converts to non-zero in the report's currency: `[transaction, price]`, the price a Monetary; `()` when none
/// (:450-480).
private fn transaction_price(env, current) {
    let end_date = if env.price_source == "pricedb-nearest" { env.to_date } else { env.ctx.now };
    let split_list = exchange::get_match_commodity_splits_sorted(env.book, [current], end_date, ());
    let i = split_list.len();
    while i > 0 {
        i -= 1;
        let split = split_list[i];
        if !split.amount.is_zero() && !split.value.is_zero() {
            let trans = split.transaction;
            let trans_price = exchange::apply_exchange_fn(env.exchange_fn, monetary(trans.currency, split.sharePrice), env.currency);
            if !trans_price.amount.is_zero() {
                return [trans, trans_price];
            }
        }
    }
    ()
}

/// The price an account's figures use (:393-490): `#{ use_txn, pricing_txn, price, price_currency, price_value }`.
/// From the price database unless it is missing, its currency converts 100 units to nothing, or the price list is
/// not preferred; then from the newest transaction that gives one (`use_txn`, `pricing_txn`); else 1 in the report's
/// currency (`use_txn` without `pricing_txn`). `price` is the database price of `find_price` or the Monetary.
private fn account_price(env, current, commodity) {
    let price = price_fn(env, commodity, env.currency, env.to_date);
    if !is_none(price) {
        let hundred = exchange::apply_exchange_fn(env.exchange_fn, monetary(price.currency, 100), env.currency);
        if hundred.amount.is_zero() {
            price = ();
        }
    }
    if is_none(price) || !env.prefer_pricelist {
        let found = transaction_price(env, current);
        if !is_none(found) {
            return #{ use_txn: true, pricing_txn: found[0], price: found[1], price_currency: found[1].commodity,
                      price_value: found[1].amount };
        }
    }
    if is_none(price) {
        let one = monetary(env.currency, 1);
        return #{ use_txn: true, pricing_txn: (), price: one, price_currency: env.currency, price_value: rational(1) };
    }
    #{ use_txn: false, pricing_txn: (), price: price, price_currency: price.currency, price_value: price.value }
}

/// `my-exchange-fn` (:411-430): the account's own commodity into the report's currency at the account's price, the
/// product rounded to the report currency's SCU, then through the exchange function from the price's currency;
/// anything else through the exchange function alone.
private fn my_exchange_fn(a, fromunits, tocurrency) {
    if a.currency == tocurrency && fromunits.commodity == a.commodity {
        let amount = to_frac(fromunits.amount * a.price_value, a.currency.fraction);
        return exchange::apply_exchange_fn(a.exchange_fn, monetary(a.price_currency, amount), tocurrency);
    }
    exchange::apply_exchange_fn(a.exchange_fn, fromunits, tocurrency)
}

/// `gnc:sum-collector-commodity` with `my-exchange-fn` (CU:900-910): each total exchanged into the report's
/// currency, rounded to its SCU with banker's rounding, and added.
private fn sum_with_my_exchange(a, collector) {
    let sum = rational(0);
    for pair in collector {
        sum += to_frac(my_exchange_fn(a, monetary(pair[0], pair[1]), a.currency).amount, a.currency.fraction);
    }
    monetary(a.currency, sum)
}

// The transaction walk (:498-776).

/// `collector` after `'add`: collectors are arrays, which Rhai copies, so the caller keeps the result.
private fn coll_add(collector, commodity, amount) {
    collector.add(commodity, amount);
    collector
}

/// Pass 1 (:531-596): what transaction `parent` does to `current`: income, brokerage, the shares traded, bought and
/// sold, the money spent and received, money moved out by a spin-off, and the asset account the income may have been
/// left in (`drp_account`: `()`, an Account or `"none"`) with the money left there. Money is rounded to the
/// transaction currency's SCU, shares to 10⁻⁸.
private fn pass1(current, parent) {
    let cc = parent.currency;
    let cf = cc.fraction;
    let t = #{ income: rational(0), brokerage: rational(0), shares: rational(0), shares_bought: rational(0),
               shares_sold: rational(0), sold: rational(0), bought: rational(0), spinoff: rational(0),
               drp_residual: rational(0), drp_account: () };
    for s in parent.splits() {
        let value = s.value;
        let acct = s.account;
        let kind = account_type(acct);
        if kind == "EXPENSE" {
            // Brokerage, unless the other split of two is in this account: a stock donation (:541-546).
            let other = s.otherSplit;
            if !same_account(current, if type_of(other) == "()" { () } else { other.account }) {
                t.brokerage = to_frac(t.brokerage + value, cf);
            }
        } else if kind == "INCOME" {
            t.income = to_frac(t.income - value, cf);
        } else if same_account(current, acct) {
            t = own_split(t, s, current, cf);
        } else if kind == "ASSET" {
            t = drp_split(t, acct, value, cc);
        }
    }
    t
}

/// A split of `current` in pass 1 (:552-578). One with no units is money out of a spin-off, or a gain split whose
/// matching income split was counted as income and is taken back.
private fn own_split(t, s, current, cf) {
    let units = s.amount;
    let value = s.value;
    t.shares = to_u(t.shares + units.abs());
    if units.is_zero() {
        if spin_off(s, current) {
            if value < 0 {
                t.spinoff = to_frac(t.spinoff - value, cf);
            }
        } else if !value.is_zero() {
            t.income = to_frac(t.income - value, cf);
        }
    } else if value > 0 {
        t.bought = to_frac(t.bought + value, cf);
        t.shares_bought = to_u(t.shares_bought + units);
    } else {
        t.sold = to_frac(t.sold - value, cf);
        t.shares_sold = to_u(t.shares_sold + units);
    }
    t
}

/// An ASSET split in pass 1 (:580-593): the first names the account reinvested income may stay in, if it is in the
/// transaction's currency; later ones add to the money left there while they are its parent, child or sibling.
private fn drp_split(t, acct, value, cc) {
    if type_of(t.drp_account) == "()" {
        t.drp_account = acct;
        if cc == acct.commodity {
            t.drp_residual = value;
        } else {
            t.drp_account = "none";
        }
    } else if type_of(t.drp_account) == "Account" {
        if parent_or_sibling(t.drp_account, acct) {
            t.drp_residual = to_frac(t.drp_residual + value, cc.fraction);
        } else {
            t.drp_account = "none";
        }
    }
    t
}

/// Between the passes (:619-690): income and brokerage go to the account's collectors; brokerage is shared between
/// purchases and sales by shares, unless omitted; reinvested income comes out of the money in, net of what stayed in
/// the holding account (a running total over the account's transactions); then money in and out.
private fn tally(a, st, t, cc) {
    let cf = cc.fraction;
    st.dividend = coll_add(st.dividend, cc, t.income);
    st.brokerage = coll_add(st.brokerage, cc, t.brokerage);
    if a.fees != "ignore-brokerage" && t.brokerage > 0 && t.shares > 0 {
        t.bought = to_frac(t.bought + to_frac(t.brokerage * (t.shares_bought / t.shares), cf), cf);
        t.sold = to_frac(t.sold + to_frac(t.brokerage * (t.shares_sold / t.shares), cf), cf);
    }
    let residual = t.drp_residual;
    if type_of(t.drp_account) == "Account" && t.income > 0 && t.bought > 0 {
        if type_of(st.drp_account) == "()" {
            st.drp_account = t.drp_account;
            st.drp_amount = residual;
        } else if type_of(st.drp_account) == "Account" && parent_or_sibling(t.drp_account, st.drp_account) {
            st.drp_amount = to_frac(st.drp_amount + residual, cf);
        } else {
            st.drp_account = "none";
            residual = rational(0);
            st.drp_amount = rational(0);
        }
    }
    let bought = t.bought;
    if t.income > 0 && bought > 0 {
        bought = to_frac(bought - t.income, cf);
        bought = to_frac(bought + residual, cf);
        bought = to_frac(bought - st.drp_amount, cf);
        if st.drp_amount < 0 && bought > 0 {
            st.drp_amount = to_frac(st.drp_amount + bought, cf);
        }
        if bought < 0 {
            bought = rational(0);
        }
    }
    st.moneyin = coll_add(st.moneyin, cc, bought);
    st.moneyout = coll_add(st.moneyout, cc, t.sold);
    st.moneyout = coll_add(st.moneyout, cc, t.spinoff);
    st
}

/// Pass 2 (:693-770): each split of `current` with units changes the basis at its value in the report's currency,
/// with its share of the brokerage when fees are in the basis; a sale, or a change leaving no basis, realizes the
/// money out less the basis it took. A spin-off changes the prices only. `st.error` is set where GnuCash throws.
private fn pass2(a, st, t, parent) {
    let cc = parent.currency;
    let cf = cc.fraction;
    let rf = a.currency.fraction;
    for s in parent.splits() {
        let units = s.amount;
        let value = s.value;
        if !units.is_zero() && same_account(a.current, s.account) {
            let split_value_currency = my_exchange_fn(a, monetary(cc, value), a.currency).amount;
            let orig_basis = sum_basis(st.basis_list, rf);
            let fee_ratio = units.abs() / t.shares;
            let fees_currency = my_exchange_fn(a, monetary(cc, to_frac(fee_ratio * t.brokerage, cf)), a.currency).amount;
            let with_fees = if a.fees == "include-in-basis" {
                to_frac(split_value_currency + fees_currency, rf)
            } else {
                split_value_currency
            };
            let built = basis_builder(st.basis_list, units, with_fees, a.method);
            if type_of(built) == "map" {
                st.error = built.div0;
                return st;
            }
            st.basis_list = built;
            if value <= 0 {
                let new_basis = sum_basis(st.basis_list, rf);
                if new_basis.is_zero() || value < 0 {
                    let gain = to_frac(with_fees.abs() - to_frac(orig_basis - new_basis, rf), rf);
                    st.gain = coll_add(st.gain, a.currency, gain);
                }
            }
        } else if spin_off(s, a.current) {
            let built = basis_builder(st.basis_list, units, my_exchange_fn(a, monetary(cc, value), a.currency).amount,
                                      a.method);
            if type_of(built) == "map" {
                st.error = built.div0;
                return st;
            }
            st.basis_list = built;
        }
    }
    st
}

/// The walk over the account's splits in split order (:498-776): each transaction posted by the end of the Date, once.
private fn walk(a) {
    let st = #{ basis_list: [], dividend: [], brokerage: [], moneyin: [], moneyout: [], gain: [], drp_account: (),
                drp_amount: rational(0), error: () };
    let seen = #{};
    for split in a.current.splits() {
        let parent = split.transaction;
        if parent.datePosted <= a.to_date && !seen.contains(parent.guid) {
            seen[parent.guid] = true;
            let t = pass1(a.current, parent);
            st = tally(a, st, t, parent.currency);
            st = pass2(a, st, t, parent);
            if !is_none(st.error) {
                return st;
            }
        }
    }
    st
}

/// `gnc-commodity-is-currency`: false for no commodity.
private fn is_currency(commodity) {
    type_of(commodity) == "Commodity" && commodity.isCurrency
}

/// Income and fees booked to the parent account (:778-838): when the parent is an ASSET or BANK account in a
/// currency, each of its splits up to the Date whose one other split is in a currency account with this account's
/// name adds its value to income (INCOME) or its negation to brokerage (EXPENSE), under that account's currency.
private fn parent_income(a, st) {
    let parent_account = a.current.parent;
    if type_of(parent_account) != "Account" || !["ASSET", "BANK"].contains(parent_account.type)
        || !is_currency(parent_account.commodity) {
        return st;
    }
    for split in parent_account.splits() {
        let other_split = split.otherSplit;
        let other_acct = if type_of(other_split) == "()" { () } else { other_split.account };
        if type_of(other_acct) == "Account" && split.transaction.datePosted <= a.to_date && other_acct.name == a.current.name
            && is_currency(other_acct.commodity) {
            let curr = other_acct.commodity;
            if other_acct.type == "INCOME" {
                st.dividend = coll_add(st.dividend, curr, split.value);
            } else if other_acct.type == "EXPENSE" {
                st.brokerage = coll_add(st.brokerage, curr, -split.value);
            }
        }
    }
    st
}

/// The figures of one account (:372-851): its units, price and value, then the walk, the parent's income and fees,
/// and, with fees in the gain, the fees taken from the realized gain. `error` is set when the walk stopped.
private fn account_figures(env, current) {
    let commodity = current.commodity;
    let unit_collector = balances::account_get_comm_balance_at_date(current, env.to_date, false);
    let units = unit_collector.getpair(commodity, false)[1];
    let a = account_price(env, current, commodity);
    a += #{ current: current, commodity: commodity, currency: env.currency, exchange_fn: env.exchange_fn,
            to_date: env.to_date, fees: env.fees, method: env.method };
    let value = my_exchange_fn(a, monetary(commodity, units), env.currency);
    let st = walk(a);
    if !is_none(st.error) {
        return #{ error: st.error };
    }
    st = parent_income(a, st);
    if env.fees == "include-in-gain" {
        let gain = st.gain;
        gain.minusmerge(st.brokerage);
        st.gain = gain;
    }
    #{ a: a, units: units, value: value, st: st, error: () }
}

/// The row's figures (:853-868), each in the report's currency: the collectors through `my-exchange-fn`, the
/// unrealized gain the value less the basis, and the totals of gains and return, each rounded to the SCU.
private fn row_figures(f) {
    let a = f.a;
    let st = f.st;
    let rf = a.currency.fraction;
    let gain = sum_with_my_exchange(a, st.gain);
    let ugain = to_frac(my_exchange_fn(a, f.value, a.currency).amount - sum_basis(st.basis_list, rf), rf);
    let bothgain = to_frac(gain.amount + ugain, rf);
    let income = sum_with_my_exchange(a, st.dividend);
    #{
        basis: sum_basis(st.basis_list, rf),
        moneyin: sum_with_my_exchange(a, st.moneyin),
        moneyout: sum_with_my_exchange(a, st.moneyout),
        brokerage: sum_with_my_exchange(a, st.brokerage),
        income: income,
        gain: gain,
        ugain: ugain,
        bothgain: bothgain,
        totalreturn: to_frac(bothgain + income.amount, rf),
    }
}

/// A rate cell (:914-922, :928-936): 100 × `amount` / `money_in` to two places, `%` after, rounded half away from
/// zero as Guile's `~,2f`; empty when no money went in. GnuCash divides doubles; red-ale the exact figures, so the
/// two differ only at a tie the double misses.
private fn rate_cell(amount, money_in) {
    if money_in.is_zero() {
        return ();
    }
    #{ number: 100 * amount / money_in, places: 2, suffix: "%", rounding: "halfUp" }
}

/// The price cell (:894-903): a transaction's price as a Monetary linked to the transaction, the price of 1 unlinked,
/// or the database price in its currency linked to the price; an inverted price links the price stored.
private fn price_cell(a) {
    if a.use_txn {
        let cell = #{ monetary: a.price };
        return if is_none(a.pricing_txn) { cell } else { html_utils::html_transaction_anchor(a.pricing_txn, cell) };
    }
    html_utils::html_price_anchor(a.price.price, a.price.currency, a.price.value)
}

/// One account's row (:870-943).
private fn stock_row(env, f, r, odd_row) {
    let a = f.a;
    let commodity = f.a.commodity;
    let cells = [#{ text: a.current.name, account: a.current }];
    if env.show_symbol {
        cells.push(#{ text: commodity.mnemonic });
    }
    if env.show_listing {
        cells.push(#{ text: commodity.namespace });
    }
    if env.show_shares {
        cells.push(#{ number: f.units, places: env.share_places, numberFormat: "share" });
    }
    if env.show_price {
        cells.push(price_cell(a));
    }
    let marker = if a.use_txn { if is_none(a.pricing_txn) { "**" } else { "*" } } else { " " };
    cells += [
        #{ text: marker },
        #{ monetary: monetary(a.currency, r.basis) },
        #{ monetary: f.value },
        #{ monetary: r.moneyin },
        #{ monetary: r.moneyout },
        #{ monetary: r.gain },
        #{ monetary: monetary(a.currency, r.ugain) },
        #{ monetary: monetary(a.currency, r.bothgain) },
        rate_cell(r.bothgain, r.moneyin.amount),
        #{ monetary: r.income },
    ];
    if env.fees != "ignore-brokerage" {
        cells.push(#{ monetary: r.brokerage });
    }
    cells += [#{ monetary: monetary(a.currency, r.totalreturn) }, rate_cell(r.totalreturn, r.moneyin.amount)];
    #{ alternate: !odd_row, cells: cells }
}

/// `table-add-stock-rows` (:359-956): a row per shown account in option order, alternating over shown rows, each
/// shown account's collectors merged into the totals. Stops at an account whose walk divides by zero, keeping the
/// rows and totals before it (:1076-1086).
private fn stock_rows(env) {
    let out = #{ rows: [], error: (), warn_price_dirty: false, warn_no_price: false,
                 basis: [], value: [], moneyin: [], moneyout: [], income: [], gain: [], ugain: [], brokerage: [] };
    let odd_row = true;
    for current in env.accounts {
        let f = account_figures(env, current);
        if !is_none(f.error) {
            out.error = f.error;
            return out;
        }
        if env.include_empty || !f.units.is_zero() {
            let r = row_figures(f);
            if f.a.use_txn {
                if is_none(f.a.pricing_txn) { out.warn_no_price = true; } else { out.warn_price_dirty = true; }
            }
            out.value = coll_add(out.value, f.value.commodity, f.value.amount);
            for pair in [["moneyin", f.st.moneyin], ["moneyout", f.st.moneyout], ["brokerage", f.st.brokerage],
                         ["income", f.st.dividend], ["gain", f.st.gain]] {
                let total = out[pair[0]];
                total.merge(pair[1]);
                out[pair[0]] = total;
            }
            out.ugain = coll_add(out.ugain, env.currency, r.ugain);
            out.basis = coll_add(out.basis, env.currency, r.basis);
            out.rows.push(stock_row(env, f, r, odd_row));
            odd_row = !odd_row;
        }
    }
    out
}

// The table and the document (:1007-1173).

/// An amount column labelled `label`.
private fn amount_column(label) {
    #{ label: label, role: "amount" }
}

/// The column headers (:1036-1074).
private fn columns(env) {
    let cols = [#{ label: "Account", role: "label" }];
    if env.show_symbol {
        cols.push(#{ label: "Symbol", role: "text" });
    }
    if env.show_listing {
        cols.push(#{ label: "Listing", role: "text" });
    }
    if env.show_shares {
        cols.push(amount_column("Shares"));
    }
    if env.show_price {
        cols.push(amount_column("Price"));
    }
    cols.push(#{ label: " ", role: "text" });
    for label in ["Basis", "Value", "Money In", "Money Out", "Realized Gain", "Unrealized Gain", "Total Gain",
                  "Rate of Gain", "Income"] {
        cols.push(amount_column(label));
    }
    if env.fees != "ignore-brokerage" {
        cols.push(amount_column("Brokerage Fees"));
    }
    cols + [amount_column("Total Return"), amount_column("Rate of Return")]
}

/// The table of the account rows (:1009-1088).
private fn portfolio_blocks(env) {
    [#{ kind: "table", columns: columns(env), rows: stock_rows(env).rows }]
}

/// `advanced-portfolio-renderer` (:149-1182).
fn render(params, book, opts, ctx) {
    let to_date = dates::time64_end_day_time(ctx, opts.get("General", "Date"));
    let accounts = opts.get("Accounts", "Accounts");
    let currency = opts.get("General", "Report's currency");
    let report_title = opts.get("General", "Report name");
    let price_source = opts.get("General", "Price Source");
    let title = report_title + " " + ctx.print_date(to_date);

    if accounts.is_empty() {
        return #{ title: title, sections: [#{ blocks: [html_utils::make_no_account_warning(report_title)] }] };
    }
    let env = #{
        book: book, ctx: ctx, to_date: to_date, accounts: accounts, currency: currency, price_source: price_source,
        include_empty: opts.get("Accounts", "Include accounts with no shares"),
        show_symbol: opts.get("Display", "Show ticker symbols"),
        show_listing: opts.get("Display", "Show listings"),
        show_shares: opts.get("Display", "Show number of shares"),
        show_price: opts.get("Display", "Show prices"),
        share_places: share_places(opts.get("Display", "Share decimal places")),
        method: opts.get("General", "Basis calculation method"),
        prefer_pricelist: opts.get("General", "Set preference for price list data"),
        fees: opts.get("General", "How to report brokerage fees"),
        exchange_fn: exchange::case_exchange_fn(book, price_source, currency, to_date),
    };
    #{ title: title, sections: [#{ blocks: portfolio_blocks(env) }] }
}
