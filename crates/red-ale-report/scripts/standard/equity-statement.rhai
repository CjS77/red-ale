// equity-statement.scm: statement of owner's equity (net worth)
//
// By David Montenegro 2004.06.23 <sunrise2000@comcast.net>
//
//  * Based on balance-sheet.scm by Robert Merkel <rgmerk@mira.net>
//
//  * BUGS:
//
//    The multicurrency support has NOT been tested and IS ALPHA.  I
//    really don't if I used the correct exchange functions.  Search
//    code for regexp "*exchange-fn".
//
//    I have also made the educated assumption <grin> that a decrease
//    in the value of a liability or equity also represents an
//    unrealized loss.  I *think* that is right, but am not sure.
//
//    This code makes the assumption that you want your equity
//    statement to no more than daily resolution.
//
//    The Accounts option panel needs a way to select (and select by
//    default) capital and draw accounts. There really should be a
//    contra account type or attribute....
//
//    The variables in this code could use more consistent naming.
//
//    See also any "FIXME"s in the code.
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
// Translated from 5.17:gnucash/report/reports/standard/equity-statement.scm, cited below by line. GnuCash's table is
// three columns wide with a first row of two empty 60-pixel cells (:422-423); here it is a label column and one
// amount column of two staircase levels, and the spacer row is left out as layout. The exchange rates, which GnuCash
// puts in a bordered table of one row under the two dates (:473-491), are a group of two tables titled with them.

import "lib/accounts" as accounts;
import "lib/acct-table" as acct_table;
import "lib/balances" as balances;
import "lib/collectors" as collectors;
import "lib/dates" as dates;
import "lib/exchange" as exchange;
import "lib/html-utilities" as html_utils;
import "lib/options" as opts_util;

/// The template (:501-507).
fn templates() {
    [#{ guid: "c2a996c8970f43448654ca84f17dda24", name: "Equity Statement", menuPath: ["_Income & Expense"], params: #{} }]
}

/// The account types the "Accounts" option selects by default (:114-121).
private fn default_account_types() {
    ["BANK", "CASH", "CREDIT", "ASSET", "LIABILITY", "STOCK", "MUTUAL", "CURRENCY", "PAYABLE", "RECEIVABLE", "EQUITY",
     "INCOME", "EXPENSE", "TRADING"]
}

/// `equity-statement-options-generator` (:97-162).
fn options(params, o, book) {
    o.string("General", "Report Title", "a", "Title for this report.", "Equity Statement");
    opts_util::add_date_interval(o, "General", "Start Date", "End Date", "c");

    o.account_list("Accounts", "Accounts", "a", "Report only on these accounts.",
                   accounts::filter_accountlist_type(default_account_types(), book.root().descendants_sorted()), #{});

    opts_util::add_currency(o, "Commodities", "Report's currency", "a");
    opts_util::add_price_source(o, "Commodities", "Price Source", "b", "pricedb-nearest");
    // Declared, so saved configurations keep it, and read nowhere, as in GnuCash: every figure is one amount in the
    // report's currency (:132-134).
    o.boolean("Commodities", "Show Foreign Currencies", "c", "Display any foreign currency amount in an account.", true);
    o.boolean("Commodities", "Show Exchange Rates", "d", "Show the exchange rates used.", false);

    o.boolean("Display", "Show accounting-style rules", "f", "Use rules beneath columns of added numbers like accountants do.",
              false);

    // "transactions really should have a field where we can put transaction types like
    // "Adjusting/Closing/Correcting Entries"" (:147-148)
    o.string("Entries", "Closing Entries pattern", "a", "Any text in the Description column which identifies closing entries.",
             "Closing Entries");
    o.boolean("Entries", "Closing Entries pattern is case-sensitive", "b",
              "Causes the Closing Entries Pattern match to be case-sensitive.", false);
    o.boolean("Entries", "Closing Entries Pattern is regular expression", "c",
              "Causes the Closing Entries Pattern to be treated as a regular expression.", false);

    o.set_default_section("Accounts");
}

/// The option values `equity-statement-renderer` reads (:197-262), by its names. "this code makes the assumption that
/// you want your equity statement to no more than daily resolution" (:200-201): the period starts after 23:59:59 of
/// the day before the Start Date.
private fn read_options(book, opts, ctx) {
    let start_date_printable = opts.get("General", "Start Date");
    let start_date = dates::time64_end_day_time(ctx, dates::time64_previous_day(ctx, start_date_printable));
    let end_date = dates::time64_end_day_time(ctx, opts.get("General", "End Date"));
    let report_commodity = opts.get("Commodities", "Report's currency");
    let price_source = opts.get("Commodities", "Price Source");
    #{
        report_title: opts.get("General", "Report Title"),
        start_date_printable: start_date_printable,
        start_date: start_date,
        end_date: end_date,
        accounts: opts.get("Accounts", "Accounts"),
        report_commodity: report_commodity,
        show_rates: opts.get("Commodities", "Show Exchange Rates"),
        use_rules: opts.get("Display", "Show accounting-style rules"),
        // GnuCash adds `'positive #f` (:249), which nothing reads (5.17:gnucash/report/report-utilities.scm:697-703).
        closing_pattern: #{
            str: opts.get("Entries", "Closing Entries pattern"),
            cased: opts.get("Entries", "Closing Entries pattern is case-sensitive"),
            regexp: opts.get("Entries", "Closing Entries Pattern is regular expression"),
            closing: true,
        },
        start_exchange_fn: exchange::case_exchange_fn(book, price_source, report_commodity, start_date),
        end_exchange_fn: exchange::case_exchange_fn(book, price_source, report_commodity, end_date),
        start_price_fn: exchange::case_price_fn(book, price_source, report_commodity, start_date),
        end_price_fn: exchange::case_price_fn(book, price_source, report_commodity, end_date),
    }
}

/// `equity-statement-renderer` (:189-499).
fn render(params, book, opts, ctx) {
    let o = read_options(book, opts, ctx);
    let start = ctx.print_date(o.start_date_printable);
    let title = `${book.company_name()} ${o.report_title} For Period Covering ${start} to ${ctx.print_date(o.end_date)}`;
    let blocks = if o.accounts.is_empty() {
        // "is this *really* necessary??  i'd be fine with an all-zero income statement" (:285-293)
        [html_utils::make_no_account_warning("Equity Statement")]
    } else {
        statement_blocks(book, ctx, o)
    };
    #{ title: title, sections: [#{ blocks: blocks }] }
}

/// `account-get-total-flow` (:164-180): for each split of `target_account_list`, void ones included (their amount is
/// 0), whose transaction is posted from `from_date` to `to_date` inclusive, its amount when positive (`"in"`) or
/// negative (`"out"`), added under the transaction's currency. GnuCash adds the account's amount under the
/// transaction's currency, so a foreign-currency equity account mixes units; kept, as GnuCash does (:177).
private fn account_get_total_flow(direction, target_account_list, from_date, to_date) {
    let total_flow = collectors::make_commodity_collector();
    for target_account in target_account_list {
        for split in target_account.splits() {
            let transaction = split.transaction;
            let split_value = split.amount;
            let posted = transaction.datePosted;
            if from_date <= posted && posted <= to_date
               && ((direction == "in" && split_value > 0) || (direction == "out" && split_value < 0)) {
                total_flow.add(transaction.currency, split_value);
            }
        }
    }
    total_flow
}

/// `(gnc:accounts-get-comm-total-assets accts get-*-balance-fn)` (:272-276, :296-336): the balances of `accts`
/// strictly before `date`, children excluded, income and expense accounts left out, merged in order. This is
/// lib/balances' `accounts_get_comm_total_assets` with the balance function inlined: a closure of this script that
/// calls lib/balances cannot be handed to lib/balances (balance-sheet.rhai's `total_value` says why). The trading
/// accounts' parents have no commodity in a book with no root currency; GnuCash adds their zero balance under #f, and
/// they are left out here, as the Balance Sheet leaves them out.
private fn total_balance(accts, date) {
    let balances_list = [];
    for acct in accts {
        if !accounts::account_is_inc_exp(acct) && acct.commodity != () {
            balances_list.push(balances::account_get_comm_balance_at_date(acct, date, false));
        }
    }
    collectors::collector_plus(balances_list)
}

/// `unrealized-gains-at-date` (:264-270): `book_balance` by `exchange_fn` less the same at average cost, each summed
/// in the report's currency.
private fn unrealized_gains_at_date(book, o, book_balance, exchange_fn, date) {
    let cost_fn = exchange::case_exchange_fn(book, "average-cost", o.report_commodity, date);
    collectors::monetaries_add([
        collectors::sum_collector_commodity(book_balance, o.report_commodity, exchange_fn),
        -collectors::sum_collector_commodity(book_balance, o.report_commodity, cost_fn),
    ])
}

/// The figures of the statement (:296-397), as commodity collectors.
private fn statement_figures(book, o) {
    let split_up = accounts::decompose_accountlist(o.accounts);
    let income_expense_accounts = split_up.INCOME + split_up.EXPENSE + split_up.TRADING;
    let equity_accounts = split_up.EQUITY;
    let start_date = o.start_date;
    let end_date = o.end_date;

    let neg_pre_start_retained_earnings =
        balances::accountlist_get_comm_balance_at_date_with_closing(book, income_expense_accounts, start_date);
    let neg_pre_end_retained_earnings =
        balances::accountlist_get_comm_balance_at_date_with_closing(book, income_expense_accounts, end_date);
    let income_expense_closing = balances::account_get_trans_type_balance_interval_with_closing(
        book, income_expense_accounts, o.closing_pattern, start_date, end_date);
    let net_income = collectors::collector_minus([
        income_expense_closing,
        balances::accountlist_get_comm_balance_interval_with_closing(book, income_expense_accounts, start_date, end_date),
    ]);
    let neg_start_equity_balance = total_balance(equity_accounts, start_date);
    let neg_end_equity_balance = total_balance(equity_accounts, end_date);
    let start_book_balance = collectors::collector_plus([total_balance(split_up.ASSET, start_date),
        total_balance(split_up.LIABILITY, start_date), neg_start_equity_balance, neg_pre_start_retained_earnings]);
    let end_book_balance = collectors::collector_plus([total_balance(split_up.ASSET, end_date),
        total_balance(split_up.LIABILITY, end_date), neg_end_equity_balance, neg_pre_end_retained_earnings]);
    let start_unrealized_gains = unrealized_gains_at_date(book, o, start_book_balance, o.start_exchange_fn, start_date);
    let net_unrealized_gains = collectors::collector_minus([
        unrealized_gains_at_date(book, o, end_book_balance, o.end_exchange_fn, end_date), start_unrealized_gains]);
    let equity_closing = balances::account_get_trans_type_balance_interval_with_closing(
        book, equity_accounts, o.closing_pattern, start_date, end_date);
    let neg_pre_closing_equity = collectors::collector_minus([neg_end_equity_balance, equity_closing]);
    let net_investment = collectors::collector_minus([neg_start_equity_balance, neg_pre_closing_equity]);
    // "assume that positive shares on an equity account are debits" (:373-377): every debit to an equity account is
    // a withdrawal, so a closing entry that closes a net loss into equity counts as one. Kept, as GnuCash does (:378-379).
    let withdrawals = account_get_total_flow("in", equity_accounts, start_date, end_date);
    let investments = collectors::collector_plus([net_investment, withdrawals]);
    let capital_increase = collectors::collector_plus([net_income, investments, net_unrealized_gains,
                                                       collectors::collector_minus([withdrawals])]);
    let start_total_equity = collectors::collector_minus([start_unrealized_gains, neg_start_equity_balance,
                                                          neg_pre_start_retained_earnings]);
    #{
        start_total_equity: start_total_equity,
        net_income: net_income,
        investments: investments,
        withdrawals: withdrawals,
        net_unrealized_gains: net_unrealized_gains,
        capital_increase: capital_increase,
        end_total_equity: collectors::collector_plus([start_total_equity, capital_increase]),
    }
}

/// `add-report-line` (:405-418): a row labelled `neg_label` (and negated) when `amount` is negative in the report's
/// currency by `exchange_fn` and there is a `neg_label`, its amount `col + 1` cells into a table three wide: level 0
/// (rightmost) for column 1, level 1 for column 0. `row_role` stands for GnuCash's row markup: "heading" for its
/// "primary-subheading", "data" for none.
private fn add_report_line(rows, o, pos_label, neg_label, amount, col, exchange_fn, rule, row_role) {
    let neg = amount != () && neg_label != ()
        && collectors::sum_collector_commodity(amount, o.report_commodity, exchange_fn).amount < 0;
    let label = if neg { neg_label } else { pos_label };
    let pos_bal = if neg { collectors::collector_minus([amount]) } else { amount };
    acct_table::add_labeled_amount_line(rows, 3, row_role, rule, label, 0,
                                        collectors::sum_collector_commodity(pos_bal, o.report_commodity, exchange_fn), col + 1)
}

/// The statement (:405-491): seven lines, the unrealized one only when there are unrealized gains, and the exchange
/// rates at both dates when asked.
private fn statement_blocks(book, ctx, o) {
    let f = statement_figures(book, o);
    let period_for = " for Period";
    let rows = [];
    rows = add_report_line(rows, o, `Capital, ${ctx.print_date(o.start_date_printable)}`, (), f.start_total_equity, 1,
                           o.start_exchange_fn, false, "heading");
    rows = add_report_line(rows, o, "Net income" + period_for, "Net loss" + period_for, f.net_income, 0,
                           o.end_exchange_fn, false, "data");
    rows = add_report_line(rows, o, "Investments" + period_for, (), f.investments, 0, o.end_exchange_fn, false, "data");
    rows = add_report_line(rows, o, "Withdrawals" + period_for, (), f.withdrawals, 0, o.end_exchange_fn, false, "data");
    if !collectors::commodity_collector_allzero(f.net_unrealized_gains) {
        rows = add_report_line(rows, o, "Unrealized Gains for Period", "Unrealized Losses for Period", f.net_unrealized_gains,
                               0, o.end_exchange_fn, false, "data");
    }
    rows = add_report_line(rows, o, "Increase in capital", "Decrease in capital", f.capital_increase, 1, o.end_exchange_fn,
                           o.use_rules, "data");
    rows = add_report_line(rows, o, `Capital, ${ctx.print_date(o.end_date)}`, (), f.end_total_equity, 1, o.end_exchange_fn,
                           false, "heading");
    if !o.show_rates {
        return [statement_table(rows)];
    }
    // GnuCash appends the ruler to the statement table after adding it to the document, so it ends that table
    // (:469, :489).
    rows.push(#{ role: "rule", cells: [(), ()] });
    [statement_table(rows), #{ kind: "group", blocks: [
        rates_table(o, o.start_price_fn, ctx.print_date(o.start_date_printable)),
        rates_table(o, o.end_price_fn, ctx.print_date(o.end_date)),
    ] }]
}

/// `gnc:html-make-rates-table` over the selected accounts by `price_fn` (:478-481), titled with its date. Accounts
/// with no commodity are left out, as the Balance Sheet leaves them out of its rates.
private fn rates_table(o, price_fn, date) {
    let table = html_utils::make_rates_table(o.report_commodity, price_fn, o.accounts.filter(|a| a.commodity != ()));
    table.title = date;
    table
}

/// The statement's table: the labels, and the amounts in two staircase levels.
private fn statement_table(rows) {
    #{ kind: "table", columns: [#{ label: "", role: "label" }, #{ label: "", role: "amount", levels: 2 }], rows: rows }
}
