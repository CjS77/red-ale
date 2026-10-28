// portfolio.scm
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
// Translated from 5.17:gnucash/report/reports/standard/portfolio.scm, cited below by line.
//
// The Investment Portfolio: each selected account's units at the report date, their price and their value in the
// report's currency, and the total value. The commodity list and price database the renderer computes and never
// reads (:184-187) and the progress calls are left out.

import "lib/accounts" as accounts;
import "lib/balances" as balances;
import "lib/collectors" as collectors;
import "lib/dates" as dates;
import "lib/exchange" as exchange;
import "lib/html-utilities" as html_utils;
import "lib/options" as opts_util;

/// The template (:286-292).
fn templates() {
    [#{ guid: "4a6b82e8678c4f3d9e85d9f09634ca89", name: "Investment Portfolio", menuPath: ["_Assets & Liabilities"],
        params: #{} }]
}

/// `options-generator` (:39-76).
fn options(params, o, book) {
    opts_util::add_report_date(o, "General", "Date", "a");
    opts_util::add_currency(o, "General", "Report's currency", "c");
    opts_util::add_price_source(o, "General", "Price Source", "d", "pricedb-latest");
    o.number_range("General", "Share decimal places", "e", "The number of decimal places to use for share numbers.",
                   2, 0, 9, 1);

    o.account_list("Accounts", "Accounts", "b", "Stock Accounts to report on.",
                   accounts::filter_accountlist_type(["STOCK", "MUTUAL"], book.root().descendants_sorted()),
                   #{ types: ["ASSET", "BANK", "STOCK", "MUTUAL"] });
    o.boolean("Accounts", "Include accounts with no shares", "e", "Include accounts that have a zero share balances.",
              false);

    o.set_default_section("General");
}

/// "Share decimal places" as a count of places. GnuCash passes `inexact->exact` of the number to
/// `gnc-share-print-info-places` (:99-101), which takes only a whole number; so does red-ale, with a message.
private fn share_places(value) {
    if !value.is_integer() {
        throw `Share decimal places must be a whole number, not ${value}`;
    }
    parse_int(value.to_string())
}

/// `price-fn` (:189-240): `[price, value]` for one unit of `foreign`, `price` being the price-database entry or `()`.
///
/// The averages exchange 10,000 units and divide the result by 10,000, to five significant figures with banker's
/// rounding (`GNC-DENOM-SIGFIGS 5`, :191-200). The latest price is the first of GnuCash's latest-any-currency list, at
/// or before now (:201-218); GnuCash also inverts it when it is quoted the other way round, but the row prints the
/// price as stored (:116-119), so the inverted value is never shown and red-ale does not compute it. The before and
/// nearest prices are the first of their lists at local noon of `date` (`time64CanonicalDayTime`), not inverted
/// (:219-240). With no price, the value is 0.
private fn price_info(book, ctx, price_source, exchange_fn, currency, foreign, date) {
    switch price_source {
        "weighted-average" | "average-cost" => {
            let units = exchange::apply_exchange_fn(exchange_fn, monetary(foreign, 10000), currency);
            [(), (units.amount / 10000).round_sigfigs(5, "round")]
        }
        "pricedb-latest" => first_price(book.prices_any_currency(foreign, ctx.now, "latest")),
        "pricedb-before" => first_price(book.prices_any_currency(foreign, dates::time64_canonical_day_time(ctx, date), "before")),
        "pricedb-nearest" => first_price(book.prices_any_currency(foreign, dates::time64_canonical_day_time(ctx, date), "nearest")),
        _ => throw `not a price source: ${price_source}`,
    }
}

/// The first price of a lookup and its value, or `[(), 0]` when it found none.
private fn first_price(prices) {
    if prices.is_empty() { [(), rational(0)] } else { [prices[0], prices[0].value] }
}

/// One account's row (:107-147), or `()` when it holds no units and empty accounts are left out. The price cell is
/// a monetary of the price's own currency and value, linked to the price (`gnc:html-price-anchor`,
/// `5.17:gnucash/report/html-utilities.scm:217-224`), or of the report's currency and the computed value.
private fn stock_row(env, current, odd_row) {
    let commodity = current.commodity;
    let unit_collector = balances::account_get_comm_balance_at_date(current, env.to_date, false);
    let units = unit_collector.getpair(commodity, false)[1];
    let price_info = price_info(env.book, env.ctx, env.price_source, env.exchange_fn, env.currency, commodity, env.to_date);
    let price = price_info[0];
    let price_cell = if price == () {
        #{ monetary: monetary(env.currency, price_info[1]) }
    } else {
        #{ monetary: monetary(price.currency, price.value), link: #{ price: price } }
    };
    let value = exchange::apply_exchange_fn(env.exchange_fn, monetary(commodity, units), env.currency);
    if !env.include_empty && units.is_zero() {
        return ();
    }
    #{ value: value.amount,
       row: #{ alternate: !odd_row, cells: [
           #{ text: current.name, account: current },
           #{ text: commodity.mnemonic },
           #{ text: commodity.namespace },
           #{ number: units, places: env.share_places, numberFormat: "share" },
           price_cell,
           #{ monetary: value },
       ] } }
}

/// `table-add-stock-rows` (:94-150): a row per shown account in option order, alternating from `normal-row` over the
/// shown rows only, each shown account's value added to `collector`.
private fn table_add_stock_rows(env, collector) {
    let rows = [];
    let odd_row = true;
    for current in env.accounts {
        let shown = stock_row(env, current, odd_row);
        if shown != () {
            collector.add(env.currency, shown.value);
            rows.push(shown.row);
            odd_row = !odd_row;
        }
    }
    [rows, collector]
}

/// The table (:242-273): the stock rows, a rule over the six columns, then a "Total" row per commodity of the
/// collector, the figure spanning five columns. With no row shown the collector is empty and there is no total.
private fn portfolio_table(env) {
    let added = table_add_stock_rows(env, collectors::make_commodity_collector());
    let rows = added[0];
    rows.push(#{ role: "rule", cells: [(), (), (), (), (), ()] });
    let totals = added[1].format(|currency, amount| #{ role: "grandTotal", cells: [
        #{ text: "Total" }, #{ monetary: monetary(currency, amount), span: 5 },
    ] });
    rows += totals;
    let columns = [
        #{ label: "Account", role: "label" },
        #{ label: "Symbol", role: "text" },
        #{ label: "Listing", role: "text" },
        #{ label: "Units", role: "amount" },
        #{ label: "Price", role: "amount" },
        #{ label: "Value", role: "amount" },
    ];
    #{ kind: "table", columns: columns, rows: rows }
}

/// `portfolio-renderer` (:84-284).
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
        share_places: share_places(opts.get("General", "Share decimal places")),
        exchange_fn: exchange::case_exchange_fn(book, price_source, currency, to_date),
    };
    #{ title: title, sections: [#{ blocks: [portfolio_table(env)] }] }
}
