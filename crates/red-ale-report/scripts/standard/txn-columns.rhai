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
// Translated from 5.17:gnucash/report/reports/standard/txn-columns.scm, cited below by line: one row per transaction
// of the chosen account, one column per account those transactions touch.
//
// Deliberate differences, each commented where it happens (design/reports.md §14):
// - a foreign split's "[value] amount" is a `commodities` cell, the amount converted to the value, not text;
// - the warnings' document is titled with the "Report name" option, where GnuCash leaves it untitled;
// - names compare bytewise in the column order, where GnuCash collates them (§12).

import "lib/accounts" as accounts;
import "lib/dates" as dates;
import "lib/html-utilities" as html;
import "lib/options" as opts_util;

/// The template (:254-260), named `reportname` (:33).
fn templates() {
    [#{ guid: "03603f0443494a6fa790b417f35d90d4", name: "Transaction Breakdown Report", menuPath: ["_Experimental"],
        params: #{} }]
}

/// `options-generator` (:41-56). "Account" has no default and no allowed types (:47-48), so it is unset until
/// chosen. The help texts of "Account" and "Description Filter" are their names, untranslated in GnuCash too.
fn options(params, o, book) {
    opts_util::add_date_interval(o, "General", "Start Date", "End Date", "a");
    o.account_sel("General", "Account", "b", "Account", (), #{ types: [] });
    o.string("General", "Description Filter", "c", "Description Filter", "");
    o.number_range("General", "Limit for number of columns", "d", "Set the upper limit for number of columns", 10, 1, 100, 1);
}

/// `acct-get-fullname-list` (:59-66): the names of `acct` and its ancestors below the root, from the top.
private fn acct_get_fullname_list(book, acct) {
    let root = book.root();
    let accum = [];
    while acct != root {
        accum.insert(0, acct.name);
        acct = acct.parent;
    }
    accum
}

/// `transactions` (:98-114): the non-void transactions with a split in `account` itself, not its sub-accounts,
/// posted from `from_date` to `to_date`, once each, in the split order of their first split there
/// (`xaccQueryGetSplitsUniqueTrans`), whose description contains `desc_filter` with its case (`string-contains`; an
/// empty filter matches every description). None without an account.
private fn find_transactions(book, account, from_date, to_date, desc_filter) {
    if account == () {
        return [];
    }
    let splits = book.split_query(#{ accounts: [account], from: from_date, to: to_date, unique: true });
    let found = [];
    for split in splits {
        let txn = split.transaction;
        if txn.description.contains(desc_filter) {
            found.push(txn);
        }
    }
    found
}

/// `account->sort-index` (:119-122): the position of `a`'s fundamental type in ASSET, LIABILITY, EQUITY, EXPENSE,
/// INCOME. No account with splits has another (TRADING accounts are left out before), but should one, GnuCash's
/// comparison fails on `#f` (:124-125), and so does this, with a message.
private fn account_sort_index(a) {
    let fundamental = accounts::account_type_fundamental(a.type);
    let index = ["ASSET", "LIABILITY", "EQUITY", "EXPENSE", "INCOME"].index_of(fundamental);
    if index < 0 {
        throw `Account ${a.fullName} of type ${a.type} has no fundamental type to sort by`;
    }
    index
}

/// `accounts` (:127-139): every account the transactions' splits are in, once, but TRADING accounts; sorted by
/// `xaccAccountOrder` across the whole tree (`account<?`, :116-117), then stably by fundamental type
/// (`account-type<?`, :124-125). The first sort is a total order, so the order the splits were met in does not show.
private fn breakdown_accounts(transactions) {
    let seen = #{};
    let found = [];
    for txn in transactions {
        for split in txn.splits() {
            let acc = split.account;
            if acc.type != "TRADING" && !(acc.guid in seen) {
                seen[acc.guid] = true;
                found.push(acc);
            }
        }
    }
    found.sort(|a, b| a.compare_order(b));
    found.sort(|a, b| account_sort_index(a) - account_sort_index(b));
    found
}

/// The column headers (:160-174): "Date", "Num" and "Desc" (untranslated in GnuCash) in the first header row, then
/// one column per account whose header is its full name split at the separator, one name per row from the top
/// (`gnc:html-table-set-multirow-col-headers!`). The renderer makes as many header rows as the deepest account has
/// names, `max-depth` (:160), and leaves the rows below a shallower account's last name blank, as GnuCash's padding
/// with #f does (:162-168).
private fn header_columns(book, accts) {
    let columns = [#{ label: "Date", role: "date" }, #{ label: "Num", role: "text" }, #{ label: "Desc", role: "text" }];
    for acc in accts {
        columns.push(#{ label: acc.fullName, role: "amount", labelLines: acct_get_fullname_list(book, acc) });
    }
    columns
}

/// The cell of one account in one transaction's row (:207-244): the sums of the transaction's splits to `acc`,
/// value and amount, each negated when `acc` is a reverse-balanced account (`maybe-negate`, :210-212), linked to the
/// first of those splits; `()` when it has none there. In the transaction's currency the cell is the value
/// (:228-231). Otherwise GnuCash writes `[<value>] <amount>`, the amount linked (:232-235); red-ale makes it a
/// `commodities` cell, the amount in the account's commodity converted to the value, which is what the pair means
/// (design/reports.md §14). Returns the cell and the amount to add to the account's total (0 for none, :237).
private fn account_cell(ctx, txn, acc) {
    let bal_value = ();
    let bal_amount = ();
    let first_split = ();
    let reverse = acc.reverse_balance(ctx);
    for split in txn.splits() {
        if split.account == acc {
            let value = if reverse { -split.value } else { split.value };
            let amount = if reverse { -split.amount } else { split.amount };
            bal_value = if bal_value == () { value } else { bal_value + value };
            bal_amount = if bal_amount == () { amount } else { bal_amount + amount };
            if first_split == () {
                first_split = split;
            }
        }
    }
    if bal_value == () {
        return [(), 0];
    }
    let txn_currency = txn.currency;
    let acc_commodity = acc.commodity;
    let cell = if acc_commodity == txn_currency {
        #{ monetary: monetary(txn_currency, bal_value) }
    } else {
        #{ commodities: [#{ monetary: monetary(acc_commodity, bal_amount), converted: monetary(txn_currency, bal_value) }] }
    };
    [html::html_split_anchor(first_split, cell), bal_amount]
}

/// The table (:159-248): a row per transaction (:192-206) with `qof-print-date` of its posted date, its num (the
/// transaction's, `xaccTransGetNum`, whatever the book's num source), its description and a cell per account; then
/// the total row (:179-191): "Total For " and the period across the three text columns, and per account the sum of
/// its (signed) amounts in its commodity.
private fn breakdown_table(book, ctx, transactions, accts, from_date, to_date) {
    let totals = [];
    totals.pad(accts.len(), rational(0));
    let rows = [];
    for txn in transactions {
        let cells = [#{ date: txn.datePosted }, #{ text: txn.num }, #{ text: txn.description }];
        for i in 0..accts.len() {
            let pair = account_cell(ctx, txn, accts[i]);
            cells.push(pair[0]);
            totals[i] += pair[1];
        }
        rows.push(#{ cells: cells });
    }
    let total_cells = [#{ text: "Total For " + ctx.print_date_interval(from_date, to_date), span: 3 }];
    for i in 0..accts.len() {
        total_cells.push(#{ monetary: monetary(accts[i].commodity, totals[i]) });
    }
    rows.push(#{ role: "total", cells: total_cells });
    #{ kind: "table", columns: header_columns(book, accts), rows: rows }
}

/// `txn-columns-renderer` (:68-250).
fn render(params, book, opts, ctx) {
    let to_date = dates::time64_end_day_time(ctx, opts.get("General", "End Date"));
    let from_date = dates::time64_start_day_time(ctx, opts.get("General", "Start Date"));
    let account = opts.get("General", "Account");
    let desc_filter = opts.get("General", "Description Filter");
    let opt_max_columns = opts.get("General", "Limit for number of columns");
    // GnuCash's warnings name the template (:146, :153) in a heading red-ale's warning blocks leave out, and their
    // document has no title (:246 sets it only for the table). The result's title is the "Report name" option.
    let report_title = opts.get("General", "Report name");

    let transactions = find_transactions(book, account, from_date, to_date, desc_filter);
    let accts = breakdown_accounts(transactions);
    if transactions.is_empty() {
        return #{ title: report_title, sections: [#{ blocks: [html::make_empty_data_warning(report_title)] }] };
    }
    if accts.len() > opt_max_columns {
        let warning = html::make_generic_warning(report_title, "Too many accounts",
            "The number of accounts linked by the transactions found exceeds the limit. Select a different subset of "
                + "transactions, or increase the limit in the options.");
        return #{ title: report_title, sections: [#{ blocks: [warning] }] };
    }
    // The table's document is titled with the template's name, not the "Report name" option (:246).
    let table = breakdown_table(book, ctx, transactions, accts, from_date, to_date);
    #{ title: "Transaction Breakdown Report", sections: [#{ blocks: [table] }] }
}
