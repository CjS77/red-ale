// lib/register: the Register report's options and renderer, which the General Journal is built on. The Register
// report itself is not a red-ale template (design/milestone5.md, "Decided", item 5): GnuCash runs it from a register
// window, on that window's query.
//
// Translated from 5.17:gnucash/report/reports/standard/register.scm, cited below as REG. Its header:
//
// register.scm
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
// Only what the General Journal reaches is translated: journal mode (`__reg/journal` #t), in which each transaction
// takes a row, a notes row under `__reg/double`, and a row per split. Left out, as no caller here can reach them:
//
// - the `__reg/query` option (REG:327-328): red-ale has no query option type, so the caller passes the split query
//   to `reg_renderer` (design/notes/report-transaction-engine.md §5, "General Journal query");
// - the `__reg/ledger-type` option and what it changes (REG:131-135, :190-191, :285, :307, :317, :546-548): the
//   "T-Num" heading, balances from the totals and the "Net Change" row;
// - the non-journal mode (REG:91-102, :141-143, :201-217): one row per split with a Memo column and a "Transfer"
//   column; `reg_renderer` refuses `__reg/journal` #f;
// - `gnc:register-report-create-internal` (REG:653-674), the register window's entry point, and the progress calls.

import "lib/collectors" as collectors;
import "lib/html-utilities" as html_utils;

/// `options-generator` (REG:324-404), without the query option. The num option is "Num/Action" in a book that uses
/// split actions for numbers, else "Num" (REG:349-355).
fn options(o, book) {
    o.internal("__reg", "journal", "", "", false);
    o.internal("__reg", "double", "", "", false);
    o.internal("__reg", "debit-string", "", "", "Debit");
    o.internal("__reg", "credit-string", "", "", "Credit");
    o.string("General", "Title", "a", "The title of the report.", "Register Report");
    o.boolean("Display", "Date", "b", "Display the date?", true);
    if book.uses_split_action_for_num() {
        o.boolean("Display", "Num/Action", "c", "Display the check number/action?", true);
    } else {
        o.boolean("Display", "Num", "c", "Display the check number?", true);
    }
    o.boolean("Display", "Description", "d", "Display the description?", true);
    o.boolean("Display", "Memo", "e", "Display the memo?", true);
    o.boolean("Display", "Account", "g", "Display the account?", true);
    o.boolean("Display", "Shares", "ha", "Display the number of shares?", false);
    o.boolean("Display", "Lot", "hb", "Display the name of lot the shares are in?", false);
    o.boolean("Display", "Price", "hc", "Display the shares price?", false);
    o.multichoice("Display", "Amount", "ia", "Display the amount?", "double",
                  [["single", "Single Column"], ["double", "Two Columns"]]);
    o.boolean("Display", "Value", "ib", "Display the value in transaction currency?", false);
    o.boolean("Display", "Running Balance", "k", "Display a running balance?", true);
    o.boolean("Display", "Totals", "l", "Display the totals?", true);
    o.set_default_section("General");
}

/// `build-column-used` (REG:73-121) in journal mode: the columns shown, in table order. In journal mode the
/// Description column holds the memo on split rows, so it is shown when the memo, the description or the notes row
/// (`double`) is, and there is no Memo column.
fn build_column_used(book, opts) {
    let num = if book.uses_split_action_for_num() { "Num/Action" } else { "Num" };
    let single = opts.get("Display", "Amount") == "single";
    let value = opts.get("Display", "Value");
    let wanted = [
        ["date", opts.get("Display", "Date")],
        ["num", opts.get("Display", num)],
        ["description", opts.get("Display", "Memo") || opts.get("Display", "Description") || opts.get("__reg", "double")],
        ["account", opts.get("Display", "Account")],
        ["shares", opts.get("Display", "Shares")],
        ["lot", opts.get("Display", "Lot")],
        ["price", opts.get("Display", "Price")],
        ["amount-single", single],
        ["debit", !single],
        ["credit", !single],
        ["value-single", value && single],
        ["value-debit", value && !single],
        ["value-credit", value && !single],
        ["balance", opts.get("Display", "Running Balance")],
    ];
    let used = [];
    for w in wanted { if w[1] { used.push(w[0]); } }
    used
}

/// The position of column `key` among `used`, or `()` when it is not shown (`date-col` and the rest, REG:32-61).
fn col(used, key) {
    let at = used.index_of(key);
    if at < 0 { () } else { at }
}

/// `make-heading-list` (REG:123-164) in journal mode: "Account", and the num heading by the num field source.
fn make_heading_list(used, debit_string, credit_string, amount_string, action_for_num) {
    let headings = #{
        date: "Date", num: if action_for_num { "Num/Action" } else { "Num" }, description: "Description",
        account: "Account", shares: "Shares", lot: "Lot", price: "Price", "amount-single": amount_string,
        debit: debit_string, credit: credit_string, "value-single": "Value", "value-debit": "Debit Value",
        "value-credit": "Credit Value", balance: "Balance",
    };
    let columns = [];
    for key in used { columns.push(#{ label: headings[key], role: column_role(key) }); }
    columns
}

private fn column_role(key) {
    switch key {
        "date" => "date",
        "num" | "description" | "account" | "lot" => "text",
        _ => "amount",
    }
}

/// A text cell, `()` for the empty string, which shows nothing.
private fn text_cell(s) { if s == "" { () } else { #{ text: s } } }

/// `n` empty cells.
private fn nulls(n) {
    let cells = [];
    for i in 0..n { cells.push(()); }
    cells
}

/// `gnc-get-num-action` (`5.17:libgnucash/engine/engine-helpers.c:56-76`): the split's action in a book that uses
/// split actions for numbers, else the transaction's num; the num for no split.
fn get_num_action(action_for_num, trans, split) {
    if split != () && action_for_num { split.action } else { trans.num }
}

/// The cell of column `key` on a split row of `split` (REG:180-288 with transaction-info? #f, split-info? #t):
/// the action under Num and the memo under Description, the account's full name, and the amounts.
private fn split_cell(t, key, split) {
    let trans = split.transaction;
    let account = split.account;
    let reverse = account.reverse_balance(t.ctx);
    let amount = split.amount;
    let value = split.value;
    let split_abs_amount = monetary(account.commodity, amount.abs());
    let split_abs_value = monetary(trans.currency, value.abs());
    switch key {
        // `gnc-get-action-num` of no transaction: the action (`engine-helpers.c:85-103`)
        "num" => text_cell(split.action),
        "description" => if t.memo { text_cell(split.memo) },
        "account" => text_cell(account.fullName),
        "shares" => #{ number: amount },
        "lot" => if split.lot != () { text_cell(split.lot.title) },
        "price" => #{ price: split.sharePrice, currency: trans.currency },
        "amount-single" =>
            html_utils::html_split_anchor(split, #{ monetary: monetary(account.commodity, if reverse { -amount } else { amount }) }),
        "debit" => if amount > 0 { html_utils::html_split_anchor(split, #{ monetary: split_abs_amount }) },
        "credit" => if !(amount > 0) { html_utils::html_split_anchor(split, #{ monetary: split_abs_amount }) },
        "value-single" => #{ monetary: monetary(trans.currency, if reverse { -value } else { value }) },
        "value-debit" => if value > 0 { #{ monetary: split_abs_value } },
        "value-credit" => if !(value > 0) { #{ monetary: split_abs_value } },
        // the date and, " " in GnuCash, the balance
        _ => (),
    }
}

/// The cell of column `key` on the transaction row of `split`, the split the query found (REG:180-288 with
/// transaction-info? #t, split-info? #f): the date, num and description, and the split's balance in its account,
/// negated when the account's sign is reversed.
private fn transaction_cell(t, key, split) {
    let trans = split.transaction;
    switch key {
        "date" => #{ date: trans.datePosted },
        "num" => text_cell(get_num_action(t.action_for_num, trans, split)),
        "description" => if t.description { text_cell(trans.description) },
        "balance" => {
            let account = split.account;
            let balance = if account.reverse_balance(t.ctx) { -split.balance } else { split.balance };
            html_utils::html_split_anchor(split, #{ monetary: monetary(account.commodity, balance) })
        }
        _ => (),
    }
}

/// The notes row under a transaction row, with `double` (REG:292-321): a blank under the date; under Num, when both
/// Num and Description are shown, the transaction's num in a book that uses split actions for numbers; then the
/// notes across the rest. In journal mode the Description column is always shown with `double`, so GnuCash's other
/// branch (REG:315-319), which builds a cell it never adds, is not reached.
private fn notes_row(t, trans) {
    let used = t.used;
    let cells = [];
    if col(used, "date") != () { cells.push(()); }
    if col(used, "num") != () && col(used, "description") != () {
        cells.push(if t.action_for_num { text_cell(get_num_action(true, trans, ())) });
    }
    let rest = used.len() - cells.len();
    let notes = trans.notes;
    cells += if notes == "" { nulls(rest) } else if rest > 1 { [#{ text: notes, span: rest }] } else { [#{ text: notes }] };
    #{ role: "data", alternate: false, cells: cells }
}

/// `add-split-row` (REG:166-322) in journal mode: the transaction's row (a `normal-row`), with `double` its notes
/// row, or a split's row (an `alternate-row`).
private fn transaction_rows(t, split) {
    let cells = [];
    for key in t.used { cells.push(transaction_cell(t, key, split)); }
    let rows = [#{ role: "data", alternate: false, cells: cells }];
    if t.double && (col(t.used, "num") != () || col(t.used, "description") != ()) {
        rows.push(notes_row(t, split.transaction));
    }
    rows
}

private fn split_row(t, split) {
    let cells = [];
    for key in t.used { cells.push(split_cell(t, key, split)); }
    #{ role: "data", alternate: true, cells: cells }
}

/// `accumulate-totals` (REG:474-490): the split's amount by the sign of the amount and its value by the sign of the
/// value, and both into the totals; amounts in the account's commodity, values in the transaction's currency.
private fn accumulate_totals(totals, split) {
    let amount = split.amount;
    let value = split.value;
    let commodity = split.account.commodity;
    let currency = split.transaction.currency;
    let amount_side = if amount > 0 { "debit_amount" } else { "credit_amount" };
    let value_side = if value > 0 { "debit_value" } else { "credit_value" };
    totals[amount_side] = added(totals[amount_side], commodity, amount);
    totals[value_side] = added(totals[value_side], currency, value);
    totals.total_amount = added(totals.total_amount, commodity, amount);
    totals.total_value = added(totals.total_value, currency, value);
    totals
}

private fn added(collector, commodity, amount) {
    collector.add(commodity, amount);
    collector
}

/// `splits-leader` (REG:492-496): the account of every split, when they share one.
private fn splits_leader(splits) {
    if splits.is_empty() { return (); }
    let leader = splits[0].account;
    for s in splits { if s.account != leader { return (); } }
    leader
}

/// `add-subtotal-row` (REG:426-472): when the table has the amount (or with `value`, the value) column(s), a rule,
/// then a "grand-total" row per commodity of `collector`: the label, then the total in the single column (negated
/// when the leader's sign is reversed), else as a positive figure in the credit column when negative and the debit
/// column otherwise. GnuCash spans the total from the second column to that one; here it sits in that column.
private fn add_subtotal_row(t, label, leader, collector, value) {
    let single = col(t.used, if value { "value-single" } else { "amount-single" });
    let credit = col(t.used, if value { "value-credit" } else { "credit" });
    let debit = col(t.used, if value { "value-debit" } else { "debit" });
    if single == () && credit == () && debit == () { return []; }
    let width = t.used.len();
    let rows = [#{ role: "rule", cells: nulls(width) }];
    for pair in collector {
        let m = monetary(pair[0], pair[1]);
        let negative = pair[1] < 0;
        let at = if single != () { single } else if negative { credit } else { debit };
        let shown = if single != () {
            if leader != () && leader.reverse_balance(t.ctx) { -m } else { m }
        } else if negative { -m } else { m };
        let cells = [#{ text: label }] + nulls(at - 1) + [#{ monetary: shown }] + nulls(width - at - 1);
        rows.push(#{ role: "grandTotal", cells: cells });
    }
    rows
}

/// `make-split-table` (REG:410-603) in journal mode: for each split the query found, its transaction's row (and
/// notes row), then a row for every split of the transaction, all of them counted in the totals; a split with no
/// account is skipped (REG:552-557). Then, with Totals, the debit and credit totals of amounts and of values, and the
/// change in value, each shown only when its columns are (REG:534-550).
fn make_split_table(book, opts, ctx, splits, debit_string, credit_string, amount_string) {
    let action_for_num = book.uses_split_action_for_num();
    let t = #{
        ctx: ctx, used: build_column_used(book, opts), action_for_num: action_for_num,
        double: opts.get("__reg", "double"), memo: opts.get("Display", "Memo"),
        description: opts.get("Display", "Description"),
    };
    let leader = splits_leader(splits);
    let totals = #{ total_amount: [], total_value: [], debit_amount: [], debit_value: [], credit_amount: [], credit_value: [] };
    let rows = [];
    for current in splits {
        if current.account == () { continue; }
        for s in current.transaction.splits() { totals = accumulate_totals(totals, s); }
        rows += transaction_rows(t, current);
        for s in current.transaction.splits() { rows.push(split_row(t, s)); }
    }
    if opts.get("Display", "Totals") {
        rows += add_subtotal_row(t, "Total Debits", leader, totals.debit_amount, false);
        rows += add_subtotal_row(t, "Total Credits", leader, totals.credit_amount, false);
        rows += add_subtotal_row(t, "Total Value Debits", leader, totals.debit_value, true);
        rows += add_subtotal_row(t, "Total Value Credits", leader, totals.credit_value, true);
    }
    rows += add_subtotal_row(t, "Value Change", leader, totals.total_value, true);
    #{
        kind: "table",
        columns: make_heading_list(t.used, debit_string, credit_string, amount_string, action_for_num),
        rows: rows,
    }
}

/// `reg-renderer` (REG:605-634): the splits of `query` (a `book.split_query` map), one per transaction in journal
/// mode (`xaccQueryGetSplitsUniqueTrans`), in a table titled by General/Title.
fn reg_renderer(book, opts, ctx, query) {
    if !opts.get("__reg", "journal") { throw "the Register report's non-journal mode is not translated"; }
    let q = query;
    q["unique"] = true;
    let splits = book.split_query(q);
    let table = make_split_table(book, opts, ctx, splits, opts.get("__reg", "debit-string"),
                                 opts.get("__reg", "credit-string"), "Amount");
    #{ title: opts.get("General", "Title"), sections: [#{ blocks: [table] }] }
}
