// balsheet-pnl.scm: multi-column report. includes
// balance-sheet and p&l reports.
//
// By Christopher Lam, 2018
//
// Improved from balance-sheet.scm
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
// Translated from 5.17:gnucash/report/reports/standard/balsheet-pnl.scm, cited below by line. GnuCash builds one HTML
// table (two side by side with dual columns) with narrow indent cells and empty cells placing each amount; here it is
// a result table with a label column and one amount column per report column, rows carrying their indent as `depth`
// and, with one column and amount indenting, their staircase position as `level` (`maxindent - amount-indent`,
// :346-369). The walker, `add-multicolumn-acct-table` (:279-607), is local to this script, as in GnuCash.
//
// Kept as GnuCash 5.17 has them, each flagged below: the P&L leaves closing transactions out and the balance sheet
// keeps them; under "pricedb-latest" every P&L header and Transaction Report link ends now; a commodity converts only
// when the latest price converts it, whatever the price source; the rates row's average cost and weighted average are
// not the ones the amounts use; retained earnings stay unconverted unless every income and expense commodity has a
// price; linked reports take this report's name. Deviations (`design/reports.md` §14): the exchange rates are one row
// per commodity, where GnuCash puts a column's rates in one cell (:770-797); the links' main currency on a tie is the
// first in the accounts option's order, where GnuCash takes hash order (:804-812).

import "lib/accounts" as accounts;
import "lib/collectors" as collectors;
import "lib/dates" as dates;
import "lib/exchange" as exchange;
import "lib/html-utilities" as html_utils;
import "lib/options" as opts_util;

/// The two templates (:1242-1259).
fn templates() {
    [#{ guid: "065d5d5a77ba11e8b31e83ada73c5eea", name: "Balance Sheet (Multicolumn)", menuPath: ["_Assets & Liabilities"],
        params: #{ report_type: "balsheet" } },
     #{ guid: "0e94fd0277ba11e8825d43e27232c9d4", name: "Income Statement (Multicolumn)", menuPath: ["_Income & Expense"],
        params: #{ report_type: "pnl" } }]
}

/// `multicol-report-options-generator` (:128-277). The callbacks only make other options selectable (:147-169,
/// :211-219); red-ale has no dialog and passes none.
fn options(params, o, book) {
    opts_util::add_date_interval(o, "General", "Start Date", "End Date", "c");
    o.multichoice_callback("General", "Period duration", "c2", "Duration between time periods", "disabled", [
        ["disabled", "Disabled"],
        ["YearDelta", "One Year"],
        ["HalfYearDelta", "Half Year"],
        ["QuarterDelta", "Quarter Year"],
        ["MonthDelta", "One Month"],
        ["TwoWeekDelta", "Two Weeks"],
        ["WeekDelta", "One Week"],
    ], ());
    o.boolean("General", "Disable amount indenting", "c3",
              "Selecting this option will disable amount indenting, and condense amounts into a single column.", false);
    // "Enable chart" and "Period order is most recent first" share key c5 (:175-185); declaration order is kept.
    o.boolean("General", "Enable chart", "c5", "Enable link to chart", false);
    o.boolean("General", "Enable dual columns", "c4", "Selecting this option will enable double-column reporting.", true);
    o.boolean("General", "Period order is most recent first", "c5", "Period order is most recent first", true);

    o.account_list("Accounts", "Accounts", "a", "Report on these accounts, if display depth allows.",
                   accounts::filter_accountlist_type(["BANK", "CASH", "CREDIT", "ASSET", "LIABILITY", "STOCK", "MUTUAL",
                                                      "CURRENCY", "PAYABLE", "RECEIVABLE", "EQUITY", "INCOME", "EXPENSE",
                                                      "TRADING"], book.root().descendants_sorted()), #{});
    // "the depth-limit option is not well debugged; it may be better to disable it altogether" (:201-202)
    opts_util::add_account_levels(o, "Accounts", "Levels of Subaccounts", "b",
                                  "Maximum number of levels in the account tree displayed.", "all");

    o.complex_boolean("Commodities", "Common Currency", "b", "Convert all amounts to a single currency.", false, ());
    opts_util::add_currency(o, "Commodities", "Report's currency", "c");
    opts_util::add_price_source(o, "Commodities", "Price Source", "d", "pricedb-nearest");
    o.boolean("Commodities", "Show original currency amount", "e", "Also show original currency amounts", true);
    o.boolean("Commodities", "Show Exchange Rates", "f", "Show the exchange rates used.", true);

    o.boolean("Display", "Include accounts with zero total balances", "a",
              "Include accounts with zero total (recursive) balances in this report.", true);
    o.boolean("Display", "Omit zero balance figures", "b",
              "Show blank space in place of any zero balances which would be shown.", false);
    o.boolean("Display", "Parent account amounts include children", "c",
              "If this option is enabled, subtotals are displayed within parent amounts, and if parent has own amount, it "
              + "is displayed on the next row as a child account. If this option is disabled, subtotals are displayed "
              + "below parent and children groups.", true);
    o.boolean("Display", "Display accounts as hyperlinks", "e",
              "Shows each account in the table as a hyperlink to its register window.", true);
    o.boolean("Display", "Display amounts as hyperlinks", "e5",
              "Shows each amounts in the table as a hyperlink to a register or report.", true);
    o.boolean("Display", "Account full name instead of indenting", "f",
              "Selecting this option enables full account name instead, and disables indenting account names.", false);
    o.boolean("Display", "Label sections", "g", "Whether or not to include a label for sections.", true);
    o.boolean("Display", "Include totals", "h", "Whether or not to include a line indicating total amounts.", true);

    if params.report_type == "pnl" {
        o.boolean("General", "If more than 1 period column, include overall period?", "c6",
                  "If several profit & loss period columns are shown, also show overall period profit & loss.", false);
    }
    o.set_default_section("General");
}

/// True for `()`, GnuCash's #f where a value may be missing.
private fn none(x) {
    type_of(x) == "()"
}

/// True for a virtual row of the walker (`#(label fn)`, :1045-1051), as against an account.
private fn is_virtual(acc) {
    type_of(acc) == "map"
}

/// `multicol-report-renderer` (:613-1240).
fn render(params, book, opts, ctx) {
    let e = read_options(params, book, opts, ctx);
    // :851-858
    let when = if e.incr != "disabled" || e.pnl { ctx.print_date_interval(e.startdate, e.enddate) } else { ctx.print_date(e.enddate) };
    let title = `${e.report_title} ${when}`;
    if e.accounts.is_empty() {
        // :860-865. GnuCash then still adds the empty table (:1228-1235), which shows nothing.
        return #{ title: title, sections: [#{ blocks: [html_utils::make_no_account_warning(e.report_title)] }] };
    }
    let built = if e.pnl { pnl_tables(e) } else { balsheet_tables(e) };
    let blocks = [];
    if !none(built.chart) {
        blocks.push(#{ kind: "text", text: "Barchart", link: built.chart });
    }
    // :1228-1235
    if e.dual {
        blocks.push(#{ kind: "group", blocks: [make_table(e, built.left), make_table(e, built.right)] });
    } else {
        blocks.push(make_table(e, built.left));
    }
    #{ title: title, sections: [#{ blocks: blocks }] }
}

/// The option values the renderer reads (:621-842), and what it derives from them.
private fn read_options(params, book, opts, ctx) {
    let pnl = params.report_type == "pnl";
    let start = opts.get("General", "Start Date");
    let incr = opts.get("General", "Period duration");
    let limit = opts.get("Accounts", "Levels of Subaccounts");
    let common = if opts.get("Commodities", "Common Currency") { opts.get("Commodities", "Report's currency") } else { () };
    // Accounts with no commodity (a trading parent in a book with no root currency) hold no splits; GnuCash's
    // monetaries need a commodity, so they are left out (`design/reports.md` §14).
    let accts = opts.get("Accounts", "Accounts").filter(|a| type_of(a.commodity) != "()");
    let e = #{
        pnl: pnl,
        book: book,
        ctx: ctx,
        opts: opts,
        report_title: opts.get("General", "Report name"),
        // :622-629: the P&L starts at the start of the day, the balance sheet at its end.
        startdate: if pnl { ctx.start_of_day(start) } else { ctx.end_of_day(start) },
        enddate: ctx.end_of_day(opts.get("General", "End Date")),
        full_names: opts.get("Display", "Account full name instead of indenting"),
        incr: incr,
        // :633-638: read only without a period.
        amount_indent_disabled: incr == "disabled" && opts.get("General", "Disable amount indenting"),
        dual: incr == "disabled" && opts.get("General", "Enable dual columns"),
        accounts: accts,
        depth_limit: if limit == "all" { () } else { limit },
        show_zb_accts: opts.get("Display", "Include accounts with zero total balances"),
        omit_zb_bals: opts.get("Display", "Omit zero balance figures"),
        recursive: opts.get("Display", "Parent account amounts include children"),
        label_sections: opts.get("Display", "Label sections"),
        total_sections: opts.get("Display", "Include totals"),
        use_links: opts.get("Display", "Display accounts as hyperlinks"),
        use_amount_links: opts.get("Display", "Display amounts as hyperlinks"),
        include_chart: opts.get("General", "Enable chart"),
        common: common,
        // :677-678: the price source is read only with a common currency.
        price_source: if none(common) { () } else { opts.get("Commodities", "Price Source") },
        reverse: opts.get("General", "Period order is most recent first"),
        show_foreign: opts.get("Commodities", "Show original currency amount"),
        show_rates: opts.get("Commodities", "Show Exchange Rates"),
        include_overall: pnl && opts.get("General", "If more than 1 period column, include overall period?"),
        use_trading: book.uses_trading_accounts(),
        main_currency: book_main_currency(ctx, accts),
        data: memo(),
        cache: memo(),
        index: #{},
    };
    // :680-685
    e.dates = if incr != "disabled" {
        dates::make_date_list(ctx, e.startdate, e.enddate, dates::deltasym_to_delta(incr))
    } else if pnl {
        [e.startdate, e.enddate]
    } else {
        [e.enddate]
    };
    e.n = e.dates.len();
    // The balance sheet's columns are its dates (:1000); the P&L's the intervals between them, and the overall
    // period, here column -1, with two or more intervals when asked (:1164-1169).
    let cols = [];
    for i in 0..(if pnl { e.n - 1 } else { e.n }) {
        cols.push(i);
    }
    if e.include_overall && e.n > 2 {
        cols.push(-1);
    }
    e.cols = cols;
    if e.reverse {
        cols.reverse();
    }
    e.display_cols = cols;
    // :333: the staircase exists only with one column and amount indenting.
    e.amount_indenting = !e.amount_indent_disabled && e.cols.len() == 1;
    // :840-841
    let deepest = 0;
    for a in accts {
        if a.depth > deepest {
            deepest = a.depth;
        }
    }
    e.maxindent = deepest + 1;
    e.priced = #{};
    e.exchange_fn = ();
    if !none(common) {
        // :662-676: `has-price?` converts the commodity's fraction at the latest price into the common currency,
        // whatever the price source, so under "average-cost" or "weighted-average" a commodity with no price in the
        // database is never converted.
        for c in accounts::accounts_get_commodities(accts, ()) {
            e.priced[c.id] = book.price_convert(monetary(c, c.fraction), common, 0, "latest").amount != 0;
        }
        // :733-736
        e.exchange_fn = exchange::case_exchange_time_fn(book, ctx, e.price_source, common,
                                                        accounts::accounts_get_commodities(accts, ()), ());
    }
    for (a, i) in accts {
        e.index[a.guid] = i;
        e.data.set(i, account_cols_data(a, e.dates));
    }
    e
}

/// `book-main-currency` (:804-812): the currency most selected accounts are in, the report currency when none is.
/// GnuCash breaks a tie in hash order; red-ale takes the first in the accounts option's order (`design/reports.md`
/// §14).
private fn book_main_currency(ctx, accts) {
    let counts = #{};
    let order = [];
    for a in accts {
        let c = a.commodity;
        if c.isCurrency {
            if c.id in counts {
                counts[c.id] += 1;
            } else {
                counts[c.id] = 1;
                order.push(c);
            }
        }
    }
    let best = ctx.report_currency();
    let best_count = 0;
    for c in order {
        if counts[c.id] > best_count {
            best = c;
            best_count = counts[c.id];
        }
    }
    best
}

/// `accounts-cols-data` (:694-714): for each date, the last split posted at or before it, its no-closing balance and
/// balance with closing, and the running sum of every split's value by transaction currency up to it; zeros and an
/// empty collector before the first split. GnuCash snapshots the value collector at every split
/// (`gnc:account-accumulate-at-dates`, RU:505-512); only the snapshot at each date is ever read.
private fn account_cols_data(acc, report_dates) {
    let splits = acc.splits();
    let count = splits.len();
    let i = 0;
    let last = ();
    let val_coll = collectors::make_commodity_collector();
    let out = [];
    for date in report_dates {
        while i < count && splits[i].transaction.datePosted <= date {
            let s = splits[i];
            val_coll.add(s.transaction.currency, s.value);
            last = s;
            i += 1;
        }
        if none(last) {
            out.push(#{ split: (), nc: rational(0), bal: rational(0), vals: collectors::make_commodity_collector() });
        } else {
            out.push(#{ split: last, nc: last.noclosingBalance, bal: last.balance, vals: collectors::collector_plus([val_coll]) });
        }
    }
    out
}

/// The column data of `acc`.
private fn datums(e, acc) {
    e.data.get(e.index[acc.guid])
}

/// `get-cell-monetary-fn`: the balance sheet's balance with closing at the date (:868-870); the P&L's no-closing
/// balance at the interval's end less its start (:1095-1107), so the P&L leaves closing transactions out and the
/// balance sheet keeps them; the overall column runs from the first date to the last.
private fn cell_monetary(e, acc, col) {
    let d = datums(e, acc);
    if !e.pnl {
        return monetary(acc.commodity, d[col].bal);
    }
    let from = if col < 0 { 0 } else { col };
    let to = if col < 0 { e.n - 1 } else { col + 1 };
    monetary(acc.commodity, d[to].nc - d[from].nc)
}

/// `col-idx->price-date` (:741-749): the date a column's prices are looked up at. Under "pricedb-latest" it is now,
/// so every P&L header and Transaction Report link then ends today (:744, :1093). A P&L column but the last takes
/// 00:00 of its interval's last day.
private fn price_date(e, col) {
    if e.price_source == "pricedb-latest" {
        e.ctx.now
    } else if col < 0 {
        e.enddate
    } else if !e.pnl {
        e.dates[col]
    } else if col == e.n - 2 {
        e.enddate
    } else {
        dates::decdate(e.ctx, e.dates[col + 1], dates::deltasym_to_delta("DayDelta"))
    }
}

/// `convert-curr-fn` (:755-763): `mon` in the common currency at the column's price date, or `()` when there is no
/// common currency, it is already in it, or `has-price?` says it has no price.
private fn convert_curr(e, mon, col) {
    if none(e.common) || mon.commodity == e.common || !e.priced[mon.commodity.id] {
        return ();
    }
    exchange::apply_exchange_time_fn(e.exchange_fn, mon, e.common, price_date(e, col))
}

/// `sum-balances-of-accounts` (:843-849) for one column: the balances (`what` "bal") or value collectors ("vals") of
/// the selected accounts in `set`, taken in reverse of the accounts option's order as GnuCash's `fold` conses them,
/// added into one collector.
private fn sum_balances(e, set, col, what) {
    let coll = collectors::make_commodity_collector();
    let i = e.accounts.len() - 1;
    while i >= 0 {
        let a = e.accounts[i];
        if a.guid in set {
            let d = datums(e, a)[col];
            if what == "bal" {
                coll.add(a.commodity, d.bal);
            } else {
                coll.merge(d.vals);
            }
        }
        i -= 1;
    }
    coll
}

/// `monetaries->exchanged` (:918-927): the collector's monetaries exchanged into the common currency by
/// `gnc:case-exchange-fn` at `date`, summed exactly.
private fn exchanged(e, coll, date) {
    let f = exchange::case_exchange_fn(e.book, e.price_source, e.common, date);
    let sum = monetary(e.common, 0);
    for pair in coll {
        sum = sum + exchange::apply_exchange_fn(f, monetary(pair[0], pair[1]), e.common);
    }
    sum
}

/// The date the virtual rows exchange at (:936-938, :953-955): now under "pricedb-latest".
private fn virtual_date(e, col) {
    if e.price_source == "pricedb-latest" { e.ctx.now } else { e.dates[col] }
}

/// `unrealized-gain-fn` (:933-946): the asset and liability value balances less their balances, exchanged.
private fn unrealized_gain(e, col) {
    let balance = sum_balances(e, e.asset_liability_set, col, "bal");
    let basis = sum_balances(e, e.asset_liability_set, col, "vals");
    exchanged(e, collectors::collector_minus([basis, balance]), virtual_date(e, col))
}

/// `retained-earnings-fn` (:951-963): the income and expense balances with closing, exchanged into one monetary only
/// when every income and expense commodity has a price, else the unconverted list.
private fn retained_earnings(e, col) {
    let balance = sum_balances(e, e.income_expense_set, col, "bal");
    let all_priced = !none(e.common);
    for c in e.income_expense_commodities {
        all_priced = all_priced && e.priced[c.id];
    }
    if all_priced {
        exchanged(e, balance, virtual_date(e, col))
    } else {
        balance.map(|pair| monetary(pair[0], pair[1]))
    }
}

/// A virtual row's value in a column, worked once: the same figures every time it is asked for.
private fn virtual_value(e, acc, col) {
    let key = (if acc.calc == "retained" { 1000000 } else { 2000000 }) + col;
    let v = e.cache.get(key);
    if none(v) {
        v = if acc.calc == "retained" { retained_earnings(e, col) } else { unrealized_gain(e, col) };
        e.cache.set(key, v);
    }
    v
}

/// `sum-accounts-at-col` (:452-467): the accounts' monetaries in one column, converted when `convert` asks and one
/// applies, a virtual row's list appended unconverted, summed by `monetary+` (:371-384) with the section's sign. GnuCash
/// conses each onto the front and sums that list, so the sum walks `flat` from its end.
private fn sum_accounts_at_col(e, s, accts, col, convert) {
    let flat = [];
    for acc in accts {
        let mon = if is_virtual(acc) { virtual_value(e, acc, col) } else { cell_monetary(e, acc, col) };
        if type_of(mon) == "array" {
            flat += mon;
        } else {
            if convert && !none(e.common) {
                let converted = convert_curr(e, mon, col);
                if !none(converted) {
                    mon = converted;
                }
            }
            flat.push(mon);
        }
    }
    let coll = collectors::make_commodity_collector();
    let i = flat.len() - 1;
    while i >= 0 {
        let mon = flat[i];
        coll.add(mon.commodity, if s.negate { -mon.amount } else { mon.amount });
        i -= 1;
    }
    coll.map(|pair| monetary(pair[0], pair[1]))
}

/// `is-not-zero?` (:469-481): whether the accounts' unconverted sum is non-zero in any column.
private fn is_not_zero(e, s, accts) {
    for col in e.cols {
        if sum_accounts_at_col(e, s, accts, col, false).some(|m| m.amount != 0) {
            return true;
        }
    }
    false
}

/// `list-of-monetary->html-text` (:386-411) as a cell: zero monetaries left out under "Omit zero balance figures";
/// each other converted when "Show original currency amount" is on and a conversion applies, the original kept beside
/// it. One unconverted monetary is a monetary cell; anything else a `commodities` cell; nothing left, no cell.
private fn amount_cell(e, monetaries, col, link) {
    let lines = [];
    for mon in monetaries {
        if e.omit_zb_bals && mon.amount == 0 {
            continue;
        }
        let converted = if e.show_foreign { convert_curr(e, mon, col) } else { () };
        lines.push(if none(converted) { #{ monetary: mon } } else { #{ monetary: mon, converted: converted } });
    }
    if lines.is_empty() {
        return ();
    }
    let cell = if lines.len() == 1 && !("converted" in lines[0]) { #{ monetary: lines[0].monetary } } else { #{ commodities: lines } };
    if !none(link) {
        cell.link = link;
    }
    cell
}

/// `get-cell-anchor-fn`, when "Display amounts as hyperlinks" is on: the balance sheet links an account's amount to its
/// last split at or before the date, none before the first split and none for a subtotal (:881-886); the P&L links
/// account and subtotal amounts to a Transaction Report of their accounts over the column's interval (:1109-1124).
private fn amount_link(e, target, col) {
    if !e.use_amount_links {
        return ();
    }
    if e.pnl {
        return trep_link(e, if type_of(target) == "array" { target } else { [target] }, col);
    }
    if type_of(target) == "array" {
        return ();
    }
    let split = datums(e, target)[col].split;
    if none(split) { () } else { #{ split: split } }
}

/// The Transaction Report link of :1109-1124, copying this report's values of the options both reports have (its
/// "Report name" too, so the linked report is titled as this one). "Common Currency" is set to the commodity, which
/// the Transaction Report's boolean reads as true (`5.17:bindings/guile/gnc-optiondb.i:356-358`).
private fn trep_link(e, accts, col) {
    let from = if col < 0 { e.startdate } else { e.dates[col] };
    let has_common = !none(e.common);
    let curr = if has_common { e.common } else { e.main_currency };
    // trep-uuid (:123), the Transaction Report
    html_utils::make_report_anchor("2fe3b9833af044abb929a88d5a59620f", e.opts, [["General", "Start Date"], ["General", "End Date"], ["Accounts", "Accounts"]], [
        ["General", "Start Date", #{ absolute: e.ctx.print_timestamp(from) }],
        ["General", "End Date", #{ absolute: e.ctx.print_timestamp(price_date(e, col)) }],
        ["Currency", "Show original currency amount", has_common],
        ["Currency", "Common Currency", has_common],
        ["Currency", "Report's currency", curr.id],
        ["Display", "Amount", "double"],
        ["Accounts", "Accounts", accts.map(|a| a.guid)],
    ])
}

/// A chart link (:965-976, :1126-1138), with this report's values of the options both reports have.
private fn chart_link(e, uuid, step, accts) {
    let curr = if none(e.common) { e.main_currency } else { e.common };
    let price_source = if none(e.price_source) { "pricedb-nearest" } else { e.price_source };
    html_utils::make_report_anchor(uuid, e.opts, [["General", "Start Date"], ["General", "End Date"], ["Accounts", "Accounts"]], [
        ["General", "Start Date", #{ absolute: e.ctx.print_timestamp(e.startdate) }],
        ["General", "End Date", #{ absolute: e.ctx.print_timestamp(e.enddate) }],
        ["General", "Report's currency", curr.id],
        ["General", "Step Size", step],
        ["General", "Price Source", price_source],
        ["Accounts", "Accounts", accts.map(|a| a.guid)],
    ])
}

/// `append-reverse`: `a` reversed, then `b`.
private fn append_reverse(a, b) {
    let out = a;
    out.reverse();
    out + b
}

/// A set of the accounts' GUIDs.
private fn guid_set(accts) {
    let set = #{};
    for a in accts {
        if !is_virtual(a) {
            set[a.guid] = true;
        }
    }
    set
}

/// The balance sheet (:867-1083): its sections, left and right, and the chart link.
private fn balsheet_tables(e) {
    let split_up = accounts::decompose_accountlist(e.accounts);
    let asset_liability = append_reverse(split_up.ASSET, split_up.LIABILITY);
    let income_expense = append_reverse(split_up.INCOME, split_up.EXPENSE);
    e.asset_liability_set = guid_set(asset_liability);
    e.income_expense_set = guid_set(income_expense);
    e.income_expense_commodities = accounts::accounts_get_commodities(income_expense, ());
    let virtuals = [];
    if e.use_trading {
        virtuals = split_up.TRADING;
    } else if !none(e.common) {
        virtuals = [#{ label: "Unrealized Gains", calc: "unrealized" }];
    }
    if !income_expense.is_empty() {
        virtuals.push(#{ label: "Retained Earnings", calc: "retained" });
    }
    let tables = [[], []];
    let right = if e.dual { 1 } else { 0 };
    if e.incr != "disabled" {
        tables[0] += add_section(e, header_section("Date", "date"));
    }
    if !split_up.ASSET.is_empty() {
        tables[0] += add_section(e, section("Asset", split_up.ASSET));
    }
    if !split_up.LIABILITY.is_empty() {
        tables[right] += add_section(e, negated(section("Liability", split_up.LIABILITY)));
    }
    tables[right] += add_section(e, negated(section("Equity", split_up.EQUITY + virtuals)));
    let total = negated(section("Liability and Equity", split_up.LIABILITY + split_up.EQUITY + virtuals));
    total.show_title = false;
    total.show_accounts = false;
    total.total_role = "grandTotal";
    tables[right] += add_section(e, total);
    if !none(e.common) && e.show_rates {
        let rates = header_section("Exchange Rates", "rates");
        rates.accounts = asset_liability + split_up.EQUITY;
        tables[right] += add_section(e, rates);
    }
    let chart = if e.include_chart && e.incr != "disabled" {
        // networth-barchart-uuid (:124)
        chart_link(e, "cbba1696c8c24744848062c7f1cf4a72", e.incr, asset_liability)
    } else {
        ()
    };
    #{ left: tables[0], right: tables[1], chart: chart }
}

/// The P&L (:1085-1226): its sections, left and right, and the chart link, which needs no period.
private fn pnl_tables(e) {
    let split_up = accounts::decompose_accountlist(e.accounts);
    let income_expense = append_reverse(split_up.INCOME, split_up.EXPENSE);
    let tables = [[], []];
    let right = if e.dual { 1 } else { 0 };
    if e.incr != "disabled" {
        tables[0] += add_section(e, header_section("Period", "date"));
    }
    if !split_up.INCOME.is_empty() {
        tables[0] += add_section(e, negated(section("Income", split_up.INCOME)));
    }
    if !split_up.EXPENSE.is_empty() {
        tables[right] += add_section(e, section("Expense", split_up.EXPENSE));
    }
    if !split_up.INCOME.is_empty() && !split_up.EXPENSE.is_empty() {
        let net = negated(section("Net Income", income_expense));
        net.show_accounts = false;
        net.force_total = true;
        net.total_role = "grandTotal";
        tables[0] += add_section(e, net);
    }
    if !none(e.common) && e.show_rates {
        let rates = header_section("Exchange Rates", "rates");
        rates.accounts = income_expense;
        tables[0] += add_section(e, rates);
    }
    let chart = if e.include_chart {
        // pnl-barchart-uuid (:125), the Income/Expense Chart
        chart_link(e, "80769921e87943adade887b9835a7685", if e.incr == "disabled" { "MonthDelta" } else { e.incr }, income_expense)
    } else {
        ()
    };
    #{ left: tables[0], right: tables[1], chart: chart }
}

/// The keyword defaults of `add-to-table` (:989-996, :1155-1160) for a section of `accts` titled `title`.
private fn section(title, accts) {
    #{ title: title, accounts: accts, header: "", show_title: true, show_accounts: true, show_total: true,
       force_total: false, negate: false, total_role: "total" }
}

/// The same, negated (`#:negate-amounts? #t`).
private fn negated(s) {
    s.negate = true;
    s
}

/// A section with only a title row of column headers (`#:get-col-header-fn`): the dates, or the exchange rates.
private fn header_section(title, header) {
    let s = section(title, []);
    s.header = header;
    s.show_accounts = false;
    s.show_total = false;
    s
}

/// `add-multicolumn-acct-table` (:279-607) as `add-to-table` calls it: the section's rows.
private fn add_section(e, s) {
    // :1007, :1009, :1011-1012
    s.depth_limit = if s.header != "" { 0 } else { e.depth_limit };
    s.show_title = s.show_title && e.label_sections;
    s.show_total = (e.total_sections && s.show_total) || s.force_total;
    s.set = guid_set(s.accounts);
    s.desc = memo();
    let rows = [];
    // :537-548. The exchange rates live in the title row, so they vanish with "Label sections" off (:1072, :1215).
    if s.show_title {
        if s.header == "rates" {
            rows += rates_rows(e, s);
        } else {
            let cells = [];
            for col in e.display_cols {
                cells.push(if s.header == "date" { header_cell(e, col) } else { () });
            }
            rows.push(indented_row(e, 0, #{ text: s.title }, "heading", e.maxindent, cells, ()));
        }
    }
    if s.show_accounts {
        rows += account_rows(e, s);
    }
    // :586-606
    if s.show_total {
        let cells = [];
        for col in e.display_cols {
            cells.push(amount_cell(e, sum_accounts_at_col(e, s, s.accounts, col, true), col, ()));
        }
        rows.push(indented_row(e, 0, #{ text: "Total For " + s.title }, s.total_role, e.maxindent, cells, ()));
    }
    // :607
    rows.push(#{ role: "spacer", cells: column_nulls(e, true) });
    rows
}

/// The walker's loop (:550-584).
private fn account_rows(e, s) {
    let rows = [];
    let accts = s.accounts;
    for (curr, i) in accts {
        let lvl_curr = depth_of(curr);
        let lvl_next = if i + 1 < accts.len() { depth_of(accts[i + 1]) } else { 0 };
        let parent = descendants_in(e, s, curr).len() > 0;
        if e.recursive && parent {
            rows.push(recursive_subtotal(e, s, lvl_curr, curr, true));
            if is_not_zero(e, s, [curr]) {
                rows.push(account_row(e, s, lvl_curr + 1, curr, true, 0));
            }
        } else {
            rows.push(account_row(e, s, lvl_curr, curr, parent, if parent { 1 } else { 0 }));
        }
        if !e.recursive && lvl_curr > lvl_next {
            let lvl = lvl_curr - 1;
            let lvl_acct = curr.parent;
            while lvl != 0 && !none(lvl_acct) && lvl_acct.guid in s.set && lvl >= lvl_next {
                rows.push(recursive_subtotal(e, s, lvl, lvl_acct, false));
                lvl -= 1;
                lvl_acct = lvl_acct.parent;
            }
        }
    }
    rows.filter(|r| !none(r))
}

/// `account->depth` (:413-415): a virtual row is at depth 0.
private fn depth_of(acc) {
    if is_virtual(acc) { 0 } else { acc.depth }
}

/// `account->descendants` among the section's accounts (:448-450, :557-559), in book order.
private fn descendants_in(e, s, acc) {
    if is_virtual(acc) {
        return [];
    }
    let key = e.index[acc.guid];
    let out = s.desc.get(key);
    if none(out) {
        out = [];
        for d in acc.descendants() {
            if d.guid in s.set {
                out.push(d);
            }
        }
        s.desc.set(key, out);
    }
    out
}

/// `account-and-descendants` (:448-450).
private fn account_and_descendants(e, s, acc) {
    [acc] + descendants_in(e, s, acc)
}

/// `show-depth?` (:343-344).
private fn show_depth(s, lvl) {
    none(s.depth_limit) || lvl <= s.depth_limit
}

/// `add-recursive-subtotal` (:483-507): a parent's row of its own and its descendants' sums, shown when zero rows are
/// or any column is non-zero. In recursive mode it is the parent's own row (`account-style-normal?`), its name linked;
/// in multilevel mode a "Total For" row; `()` when hidden.
private fn recursive_subtotal(e, s, lvl, acct, normal) {
    let accts = account_and_descendants(e, s, acct);
    if !((e.show_zb_accts || is_not_zero(e, s, accts)) && show_depth(s, lvl)) {
        return ();
    }
    let cells = [];
    for col in e.display_cols {
        cells.push(amount_cell(e, sum_accounts_at_col(e, s, accts, col, true), col, amount_link(e, accts, col)));
    }
    indented_row(e, lvl, render_account(e, acct, !normal), if normal { "account" } else { "subtotal" }, e.maxindent - lvl,
                 cells, acct)
}

/// `add-account-row` (:509-534): an account's own amounts, converted only with "Show original currency amount" off
/// (:529), else shown with their originals; `()` when hidden. A multilevel parent is always shown, one step further
/// left (:570-572).
private fn account_row(e, s, lvl, curr, override_show, account_indent) {
    if !((e.show_zb_accts || override_show || is_not_zero(e, s, [curr])) && show_depth(s, lvl)) {
        return ();
    }
    let cells = [];
    for col in e.display_cols {
        let link = if is_virtual(curr) { () } else { amount_link(e, curr, col) };
        cells.push(amount_cell(e, sum_accounts_at_col(e, s, [curr], col, !e.show_foreign), col, link));
    }
    let role = if is_virtual(curr) { "data" } else { "account" };
    indented_row(e, lvl, render_account(e, curr, false), role, e.maxindent - lvl - account_indent, cells, curr)
}

/// `render-account` (:421-440): the name (full with "Account full name instead of indenting"), "Total For " before it
/// on a total, linked to the account unless a total, virtual or placeholder.
private fn render_account(e, acc, total) {
    if is_virtual(acc) {
        return #{ text: acc.label };
    }
    let name = if e.full_names { acc.fullName } else { acc.name };
    if total {
        #{ text: "Total For " + name }
    } else if e.use_links && !acc.placeholder {
        #{ text: name, account: acc }
    } else {
        #{ text: name }
    }
}

/// `add-indented-row` (:346-369): the label indented `indent` levels (none with full names), the column cells in
/// display order, and with the staircase the amount at `maxindent - amount_indent` from the right.
private fn indented_row(e, indent, label, role, amount_indent, cells, acct) {
    let row = #{ role: role, cells: [label] + cells };
    if !e.full_names {
        row.depth = indent;
    }
    if e.amount_indenting {
        row.level = e.maxindent - amount_indent;
    }
    if !none(acct) && !is_virtual(acct) {
        row.account = acct.guid;
    }
    row
}

/// An empty cell per data column, and one for the label when `label` is true.
private fn column_nulls(e, label) {
    let cells = if label { [()] } else { [] };
    for col in e.cols {
        cells.push(());
    }
    cells
}

/// A column header: the balance sheet's date (:978-987); the P&L's interval, "<from> to <to>" (:1140-1153), its end
/// the column's price date, so under "pricedb-latest" today (:744). An end before the start (dates given swapped
/// without a period, or "pricedb-latest" before the period) is text, since a period cannot end before it starts.
private fn header_cell(e, col) {
    if !e.pnl {
        return #{ date: e.dates[col] };
    }
    let from = if col < 0 { e.startdate } else { e.dates[col] };
    let to = price_date(e, col);
    if to >= from {
        #{ date: from, "until": to }
    } else {
        #{ text: `${e.ctx.print_date(from)} to ${e.ctx.print_date(to)}` }
    }
}

/// `get-exchange-rates-fn` (:770-797): the "Exchange Rates" title row, then one row per commodity of the section's
/// accounts other than the common currency, sorted, showing one unit and each column's rate by `gnc:case-price-fn` at
/// the price date, or "<symbol> missing" without a price. GnuCash puts a column's rates in one cell, separated by line
/// breaks; red-ale gives each commodity its row (`design/reports.md` §14). Under "average-cost" the rate is the cost to
/// the price date itself while the amounts use costs to the end of its day, and under "weighted-average" a book-wide
/// average while the amounts use the running one (CU:772-774, :849-865): GnuCash's figures, kept.
private fn rates_rows(e, s) {
    let rows = [indented_row(e, 0, #{ text: s.title }, "heading", e.maxindent, column_nulls(e, false), ())];
    let price_fns = [];
    for col in e.display_cols {
        price_fns.push(exchange::case_price_fn(e.book, e.price_source, e.common, price_date(e, col)));
    }
    for comm in accounts::accounts_get_commodities_sorted(s.accounts, e.common) {
        let cells = [];
        for f in price_fns {
            cells.push(if e.priced[comm.id] {
                #{ price: exchange::apply_price_fn(f, comm), currency: e.common }
            } else {
                #{ text: `${e.common.symbol} missing` }
            });
        }
        rows.push(indented_row(e, 0, #{ monetary: monetary(comm, 1) }, "data", e.maxindent, cells, ()));
    }
    rows
}

/// A result table of the rows: the label column, then one amount column per report column, or one with the staircase.
private fn make_table(e, rows) {
    let columns = [#{ label: "", role: "label" }];
    if e.amount_indenting {
        columns.push(#{ label: "", role: "amount", levels: e.maxindent });
    } else {
        for col in e.cols {
            columns.push(#{ label: "", role: "amount" });
        }
    }
    #{ kind: "table", columns: columns, rows: rows }
}
