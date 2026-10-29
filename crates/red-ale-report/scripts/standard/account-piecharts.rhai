// account-piecharts.scm: shows piechart of accounts
//
// By Robert Merkel (rgmerk@mira.net)
// and Christian Stimming <stimming@tu-harburg.de>
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
// Translated from 5.17:gnucash/report/reports/standard/account-piecharts.scm, cited below by line. The five pie
// charts share one options generator and one renderer; what tells them apart (GnuCash's arguments to
// `build-report!`, :573-600) comes in as `params`.
//
// Deliberate differences, each commented where it happens:
// - percentages are `fixed` of the exact ratio, where GnuCash formats a double (design/reports.md §12);
// - the Securities pie sums by commodity in first-appearance order, where GnuCash uses a hash table (§12);
// - drill-down links are described in the result, where GnuCash makes report instances (§7, "Links");
// - the result's title is the report's name, where GnuCash's document has none (§14);
// - progress reporting (`gnc:report-percent-done`) is dropped, so the traversals return their data only (§12).

import "lib/options" as opts_util;
import "lib/accounts" as accounts;
import "lib/balances" as balances;
import "lib/collectors" as collectors;
import "lib/exchange" as exchange;
import "lib/dates" as dates;
import "lib/html-utilities" as html;

/// The account types of the asset and security pies (:617-621, :627-631).
private fn asset_types() {
    ["ASSET", "BANK", "CASH", "CHECKING", "SAVINGS", "MONEYMRKT", "RECEIVABLE", "STOCK", "MUTUAL", "CURRENCY"]
}

/// What `build-report!` (:573-600) passes to the generator and renderer: the template's GUID, the account types,
/// whether the report covers an interval (`income-expense?`, which is also `do-intervals?`), whether it works by
/// account depth (`depth-based?`; the Securities pie works by commodity) and whether balances are negated
/// (`reverse-balance?`).
private fn kind(report_guid, account_types, do_intervals, depth_based, reverse_balance) {
    #{ report_guid: report_guid, account_types: account_types, do_intervals: do_intervals, depth_based: depth_based,
       reverse_balance: reverse_balance }
}

/// The five templates (:602-642), with the names (:59-63), menu names (:36-40) and tips (:44-54) of 5.17.
fn templates() {
    [
        #{ guid: "e1bd09b8a1dd49dd85760db9d82b045c", name: "Income Accounts", menuPath: ["_Income & Expense"],
           menuName: "Income Piechart", menuTip: "Shows a piechart with the Income per given time interval",
           params: kind("e1bd09b8a1dd49dd85760db9d82b045c", ["INCOME"], true, true, true) },
        #{ guid: "9bf1892805cb4336be6320fe48ce5446", name: "Expense Accounts", menuPath: ["_Income & Expense"],
           menuName: "Expense Piechart", menuTip: "Shows a piechart with the Expenses per given time interval",
           params: kind("9bf1892805cb4336be6320fe48ce5446", ["EXPENSE"], true, true, false) },
        #{ guid: "5c7fd8a1fe9a4cd38884ff54214aa88a", name: "Assets", menuPath: ["_Assets & Liabilities"],
           menuName: "Asset Piechart", menuTip: "Shows a piechart with the Assets balance at a given time",
           params: kind("5c7fd8a1fe9a4cd38884ff54214aa88a", asset_types(), false, true, false) },
        #{ guid: "e9418ff64f2c11e5b61d1c7508d793ed", name: "Securities", menuPath: ["_Assets & Liabilities"],
           menuName: "Security Piechart", menuTip: "Shows a piechart with distribution of assets over securities",
           params: kind("e9418ff64f2c11e5b61d1c7508d793ed", asset_types(), false, false, false) },
        #{ guid: "3fe6dce77da24c66bdc8f8efdea7f9ac", name: "Liabilities", menuPath: ["_Assets & Liabilities"],
           menuName: "Liability Piechart", menuTip: "Shows a piechart with the Liabilities balance at a given time",
           params: kind("3fe6dce77da24c66bdc8f8efdea7f9ac", ["LIABILITY", "PAYABLE", "CREDIT", "CREDITLINE"], false, true, true) },
    ]
}

/// `options-generator` (:87-163).
fn options(params, o, book) {
    let account_types = params.account_types;
    if params.do_intervals {
        opts_util::add_date_interval(o, "General", "Start Date", "End Date", "a");
    } else {
        opts_util::add_report_date(o, "General", "End Date", "a");
    }
    opts_util::add_currency(o, "General", "Report's currency", "b");
    opts_util::add_price_source(o, "General", "Price Source", "c", "pricedb-nearest");
    if params.do_intervals {
        o.multichoice("General", "Show Average", "f",
                      "Select whether the amounts should be shown over the full time period or rather as the average e.g. per month.",
                      "None", [["None", "No Averaging"], ["YearDelta", "Yearly"], ["MonthDelta", "Monthly"], ["WeekDelta", "Weekly"]]);
    }
    o.account_list("Accounts", "Accounts", "a", "Report on these accounts, if chosen account level allows.",
                   accounts::filter_accountlist_type(account_types, book.root().descendants_sorted()), #{ types: account_types });
    if params.depth_based {
        opts_util::add_account_levels(o, "Accounts", "Levels of Subaccounts", "b",
                                      "Maximum number of levels in the account tree displayed.", 2);
    }
    let fullname_help =
        if params.depth_based { "Show the full account name in legend?" } else { "Show the full security name in the legend?" };
    o.boolean("Display", "Show long names", "a", fullname_help, false);
    // "Show Totals" and "Show Percents" share the key "b" (:138-145); red-ale keeps declaration order (§6).
    o.boolean("Display", "Show Totals", "b", "Show the total balance in legend?", true);
    o.boolean("Display", "Show Percents", "b", "Show the percentage in legend?", true);
    o.number_range("Display", "Maximum Slices", "c", "Maximum number of slices in pie.", 7, 2, 255, 1);
    opts_util::add_plot_size(o, "Display", "Plot Width", "Plot Height", "d", #{ percent: 100 }, #{ percent: 100 });
    opts_util::add_sort_method(o, "Display", "Sort Method", "e", "amount");
    o.set_default_section("General");
}

/// Every option of a template, by section and name, as `options` declares them and the engine adds the three
/// General ones (design/reports.md §6): what `gnc:options-copy-values` copies into a drill-down report.
private fn option_names(params) {
    let names = [["General", "Report name"], ["General", "Stylesheet"], ["General", "Add options summary"]];
    if params.do_intervals {
        names += [["General", "Start Date"], ["General", "Show Average"]];
    }
    names += [["General", "End Date"], ["General", "Report's currency"], ["General", "Price Source"], ["Accounts", "Accounts"]];
    if params.depth_based {
        names.push(["Accounts", "Levels of Subaccounts"]);
    }
    names + [["Display", "Show long names"], ["Display", "Show Totals"], ["Display", "Show Percents"],
             ["Display", "Maximum Slices"], ["Display", "Plot Width"], ["Display", "Plot Height"], ["Display", "Sort Method"]]
}

/// `display-name-accounts` (:167-170) and `display-name-security` (:173-176).
private fn display_name(env, item) {
    if env.depth_based {
        if env.show_fullname { item.fullName } else { item.name }
    } else if env.show_fullname {
        item.fullname
    } else {
        item.mnemonic
    }
}

/// -1, 0 or 1 as `a` sorts before, with or after `b`, bytewise where GnuCash uses `gnc:string-locale<?` (§12).
private fn compare_keys(a, b) {
    if a < b { -1 } else if a > b { 1 } else { 0 }
}

/// `sort-comparator-accounts` (:180-191) and `sort-comparator-security` (:194-205) as a three-way comparison of two
/// `[amount, account or commodity]` pairs. A security's code is its mnemonic.
private fn sort_compare(env, a, b) {
    switch env.sort_method {
        "acct-code" => if env.depth_based { compare_keys(a[1].code, b[1].code) } else { compare_keys(a[1].mnemonic, b[1].mnemonic) },
        "alphabetical" => compare_keys(display_name(env, a[1]), display_name(env, b[1])),
        _ => compare_keys(b[0], a[0]),
    }
}

/// `show-acct?` (:344-345): whether `a` was selected in the Accounts option.
private fn show_acct(env, a) {
    a.guid in env.selected
}

/// `profit-fn` (:351-356): the change over the period, or the balance at the report date, of `account` (and its
/// sub-accounts with `subaccts`), as a commodity collector.
private fn profit_fn(env, account, subaccts) {
    if env.do_intervals {
        balances::account_get_comm_balance_interval(env.book, account, env.from_date, env.to_date, subaccts)
    } else {
        balances::account_get_comm_balance_at_date(account, env.to_date, subaccts)
    }
}

/// `account-balance` (:409-410) through `collector->amount` (:397-405): the balance converted to the report's
/// currency, times the averaging multiplier. Exact, as GnuCash's Scheme number is.
private fn account_balance(env, a, subaccts) {
    collectors::sum_collector_commodity(profit_fn(env, a, subaccts), env.report_currency, env.exchange_fn).amount * env.averaging_multiplier
}

/// `traverse-accounts` (:217-251): `[balance, account]` for each selected account, with its sub-accounts' balances
/// once `current_depth` reaches the tree depth and without them above it, whose parents may not be selected. The
/// order is GnuCash's: each account's sub-accounts go before it, and later accounts before earlier ones, which only
/// equal amounts' order shows after the stable sort.
private fn traverse_accounts(env, current_depth, accts) {
    if current_depth < env.tree_depth {
        let res = [];
        for cur in accts {
            let subaccts = traverse_accounts(env, current_depth + 1, cur.children());
            let rest = if show_acct(env, cur) { [[account_balance(env, cur, false), cur]] + res } else { res };
            res = subaccts + rest;
        }
        res
    } else {
        let res = [];
        for a in accts {
            if show_acct(env, a) {
                res.push([account_balance(env, a, true), a]);
            }
        }
        res
    }
}

/// `traverse!` of `sum-securities` (:267-277): each selected account's own balance, added to its commodity's sum,
/// visiting an account, then its sub-accounts, then the accounts after it. `table` holds `[sum, commodity]` pairs.
private fn traverse_securities(env, remaining, table) {
    for cur in remaining {
        if show_acct(env, cur) {
            let comm = cur.commodity;
            let balance = account_balance(env, cur, false);
            let i = table.index_of(|entry| entry[1] == comm);
            if i < 0 {
                table.push([balance, comm]);
            } else {
                table[i][0] += balance;
            }
        }
        table = traverse_securities(env, cur.children(), table);
    }
    table
}

/// `sum-securities` (:257-283): the selected accounts' balances summed by commodity, keyed by its unique name.
/// GnuCash keeps the sums in a hash table and lists them in hash order (:260, :283); red-ale lists them in the order
/// each commodity first appears, so equal sums sort the same way every time. No figure changes (design/reports.md §12).
private fn sum_securities(env, accts) {
    traverse_securities(env, accts, [])
}

/// `round-scu` (:499-500): to the report currency's SCU with banker's rounding (`GNC-HOW-RND-ROUND`).
private fn round_scu(env, amt) {
    amt.round_to(env.scu, "round")
}

/// The sum of the amounts of `[amount, …]` pairs: `(apply + (unzip1 pairs))`.
private fn sum_amounts(pairs) {
    let sum = rational(0);
    for p in pairs {
        sum += p[0];
    }
    sum
}

/// The averaging multiplier (:365-378): one over the number of years, months or weeks from the start date to just
/// after the end date, or 1 with no averaging or no positive span. GnuCash's year, month and week fractions are
/// exact (5.17:bindings/guile/date-utilities.scm:194-251), so this is GnuCash's figure.
private fn averaging_multiplier(ctx, averaging_selection, from_date, to_date) {
    let fraction_func = dates::date_get_fraction_func(ctx, averaging_selection);
    if fraction_func == () {
        return rational(1);
    }
    let start_frac = dates::apply_fraction_func(fraction_func, from_date);
    let end_frac = dates::apply_fraction_func(fraction_func, to_date + 1);
    let diff = end_frac - start_frac;
    if diff > 0 { 1 / diff } else { rational(1) }
}

/// The title with its averaging suffix (:381-386).
private fn averaged_title(report_title, averaging_selection) {
    switch averaging_selection {
        "YearDelta" => report_title + " Yearly Average",
        "MonthDelta" => report_title + " Monthly Average",
        "WeekDelta" => report_title + " Weekly Average",
        _ => report_title,
    }
}

/// "Maximum Slices" as a count. GnuCash takes `inexact->exact` of the number (:329-330), and `take` then fails on a
/// fraction (:452); so does red-ale, with a message.
private fn slice_count(max_slices) {
    if !max_slices.is_integer() {
        throw `Maximum Slices must be a whole number, not ${max_slices}`;
    }
    parse_int(max_slices.to_string())
}

/// A plot size option's value as the chart takes it.
private fn plot_size(size) {
    let out = #{};
    out[size.unit] = size.value;
    out
}

/// Every option's value in wire form, by section and name: `gnc:options-copy-values` into a new report's options
/// (5.17:gnucash/report/html-utilities.scm:177-183), before a drill-down changes some of them.
private fn copied_options(params, opts) {
    let copy = #{};
    for name in option_names(params) {
        if !(name[0] in copy) {
            copy[name[0]] = #{};
        }
        copy[name[0]][name[1]] = opts.raw(name[0], name[1]);
    }
    copy
}

/// A link to this template with the current options and the Accounts option set to `accts`, plus any other
/// `[section, name, value]` changes: `gnc:make-report-anchor` (5.17:gnucash/report/html-utilities.scm:168-185) and
/// the "Other" slice's report (:459-470). GnuCash makes a report instance; red-ale describes the run (§7, "Links").
private fn report_link(env, accts, changes) {
    let options = env.copied_options;
    options.Accounts["Accounts"] = accts.map(|a| a.guid);
    for c in changes {
        options[c[0]][c[1]] = c[2];
    }
    #{ report: #{ template: env.report_guid, options: options } }
}

/// The link of one slice (:474-496): the "Other" slice's report, an account's register when it has no
/// sub-accounts, else this report on the account and its children, one level deeper and named after it.
private fn slice_link(env, series) {
    if type_of(series[1]) == "string" {
        return env.other_link;
    }
    let acct = series[1];
    let subaccts = acct.children();
    if subaccts.is_empty() {
        return #{ account: acct.guid };
    }
    let levels = if 6 < env.tree_depth + 1 { 6 } else { env.tree_depth + 1 };
    let name = if env.show_fullname { acct.fullName } else { acct.name };
    report_link(env, [acct] + subaccts, [["Accounts", "Levels of Subaccounts", levels], ["General", "Report name", name]])
}

/// The legend label of one slice (:535-556): its name, its rounded amount and its share of the total. GnuCash
/// formats the share of a double with `~2,1f`; red-ale formats the exact ratio the same way (`fixed`, rounding half
/// up), so a share that is a tie in exact arithmetic may differ in its last digit (design/reports.md §12).
private fn slice_label(env, series, total) {
    let name = if type_of(series[1]) == "string" { series[1] } else { display_name(env, series[1]) };
    if env.show_total {
        name += " - " + monetary(env.report_currency, round_scu(env, series[0])).to_string();
    }
    if env.show_percent {
        name += " (" + fixed(100 * series[0] / total, 2, 1) + "%)";
    }
    name
}

/// The chart's second title line (:511-525): the period or the report date, and the rounded total.
private fn subtitle(env, ctx, total) {
    let line = if env.do_intervals {
        ctx.print_date_interval(env.from_date, env.to_date)
    } else {
        "Balance at " + ctx.print_date(env.to_date)
    };
    if env.show_total {
        line += ": " + monetary(env.report_currency, round_scu(env, total)).to_string();
    }
    line
}

/// The pie (:472-558). The values are the slices rounded to the report currency's SCU (:528-532), the colours
/// GnuCash's palette (:531). The axes are hidden (:533), so there is no grid. A pie has one series, so its colour
/// is the first slice's; `colors` gives each slice its own.
private fn pie_chart(env, ctx, combined) {
    let total = sum_amounts(combined);
    let colors = html::assign_colors(combined.len());
    let values = [];
    let labels = [];
    for series in combined {
        values.push(round_scu(env, series[0]));
        labels.push(slice_label(env, series, total));
    }
    let series = #{ label: "Accounts", color: colors[0], values: values, colors: colors };
    if env.depth_based {
        let links = [];
        for s in combined {
            links.push(slice_link(env, s));
        }
        series.links = links;
    }
    #{
        kind: "chart", type: "pie", title: [env.report_title, subtitle(env, ctx, total)],
        width: env.width, height: env.height, grid: false,
        yAxis: #{ label: "", format: "currency", commodity: env.report_currency.id },
        labels: labels, series: [series],
    }
}

/// The slices (:443-470): the balances above zero after the sign is fixed (:432-437), sorted, and past "Maximum
/// Slices" the smallest folded into one "Other" slice. Returns the slices and the "Other" slice's link.
private fn slices(env, data) {
    let combined = [];
    for pair in data {
        let amount = if env.reverse_balance { -pair[0] } else { pair[0] };
        // `(not (>= 0.0 (car pair)))`: only positive amounts make slices.
        if !(0 >= amount) {
            combined.push([amount, pair[1]]);
        }
    }
    combined.sort(|a, b| sort_compare(env, a, b));
    let other_link = ();
    if combined.len() > env.max_slices {
        let finish = combined.extract(env.max_slices - 1);
        combined.truncate(env.max_slices - 1);
        combined.push([sum_amounts(finish), "Other"]);
        if env.depth_based {
            other_link = report_link(env, finish.map(|s| s[1]), []);
        }
    }
    #{ combined: combined, other_link: other_link }
}

/// `piechart-renderer` (:289-571).
fn render(params, book, opts, ctx) {
    let do_intervals = params.do_intervals;
    let depth_based = params.depth_based;
    let to_date = dates::time64_end_day_time(ctx, opts.get("General", "End Date"));
    let from_date = if do_intervals { dates::time64_start_day_time(ctx, opts.get("General", "Start Date")) } else { () };
    let accts = opts.get("Accounts", "Accounts");
    let account_levels = if depth_based { opts.get("Accounts", "Levels of Subaccounts") } else { "all" };
    let report_currency = opts.get("General", "Report's currency");
    let price_source = opts.get("General", "Price Source");
    let averaging_selection = if do_intervals { opts.get("General", "Show Average") } else { "None" };
    let topl_accounts = accounts::filter_accountlist_type(params.account_types, book.root().children_sorted());

    let selected = #{};
    for a in accts {
        selected[a.guid] = true;
    }
    let report_title = averaged_title(opts.get("General", "Report name"), averaging_selection);
    let env = #{
        book: book, do_intervals: do_intervals, depth_based: depth_based, reverse_balance: params.reverse_balance,
        from_date: from_date, to_date: to_date, selected: selected, report_currency: report_currency,
        scu: report_currency.fraction, report_title: report_title, report_guid: params.report_guid,
        exchange_fn: exchange::case_exchange_fn(book, price_source, report_currency, to_date),
        tree_depth: if account_levels == "all" { accounts::get_current_account_tree_depth(book) } else { account_levels },
        averaging_multiplier: averaging_multiplier(ctx, averaging_selection, from_date, to_date),
        show_fullname: opts.get("Display", "Show long names"), show_total: opts.get("Display", "Show Totals"),
        show_percent: opts.get("Display", "Show Percents"), max_slices: slice_count(opts.get("Display", "Maximum Slices")),
        height: plot_size(opts.get("Display", "Plot Height")), width: plot_size(opts.get("Display", "Plot Width")),
        sort_method: opts.get("Display", "Sort Method"), copied_options: copied_options(params, opts), other_link: (),
    };

    // GnuCash's document has no title of its own (5.17:gnucash/report/html-document.scm:131): the chart's first
    // title line and the warnings' heading name the report. The warning blocks leave that heading out, so the
    // result's title names the report instead.
    let block = if accts.is_empty() {
        html::make_no_account_warning(report_title)
    } else {
        let data = if depth_based { traverse_accounts(env, 1, topl_accounts) } else { sum_securities(env, topl_accounts) };
        let s = slices(env, data);
        env.other_link = s.other_link;
        if s.combined.is_empty() { html::make_empty_data_warning(report_title) } else { pie_chart(env, ctx, s.combined) }
    };
    #{ title: report_title, sections: [#{ blocks: [block] }] }
}
