// lib/acct-table: GnuCash's account table: which accounts a statement shows, at what depth, with what balances and
// subtotals, as rows of a report result.
//
// Translated from 5.17:gnucash/report/html-acct-table.scm (lines 524-1071), cited below as HAT. Its header:
//
// html-acct-table.scm : generate a multi-columnar list of accounts
// including utilities to convert to <html-table> form
//
// By David Montenegro 2004.06.23 <sunrise2000@comcast.net>
//
// Borrowed largely from html-table.scm by Bill Gribble <grib@gnumatic.com>
// and html-utilities.scm by Christian Stimming <stimming@tu-harburg.de>
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
// The table's environment and each row's are maps keyed by GnuCash's names with `-` turned into `_` and `?`
// dropped (`display_tree_depth`, `children_displayed`); symbols are strings ("flatten", "account-row"). A missing
// key, `()` and false read as GnuCash's #f. The parameters (HAT:88-233):
//
// - display_tree_depth: an integer, or `()` for no limit, which works only with "flatten";
// - depth_limit_behavior: "summarize" (default), "flatten" or "truncate";
// - initial_indent (0), account_less_p (a closure `|a, b|` sorting each level, `()` for none), start_date and
//   end_date (default now), report_commodity (default the report currency), exchange_fn (lib/exchange),
//   get_balance_fn (a closure `|account, start_date, end_date|` returning a collector), parent_account_subtotal_mode
//   (true or false), zero_balance_mode ("show-leaf-acct" or "omit-leaf-acct"), account_label_mode ("anchor", the
//   default, or "name"), balance_mode ("post-closing", the default, or "pre-closing"), closing_pattern (a
//   transaction type of lib/balances).
//
// GnuCash's `report-budget` belongs to the budget reports, outside v0.2, and is refused; `column-header` is computed
// but never used (HAT:584-587) and is left out. GnuCash renders the table to HTML; here `add_account_balances` turns
// it into result rows: role ("account" or "subtotal", GnuCash's "primary-subheading"), `account`, `depth` (the
// label's `indented-depth`) and `level` (the amount's position counted from the right, which is the row's
// `display-depth`), with a label cell and an amount cell. A multi-commodity amount becomes a `commodities` cell,
// GnuCash's nested commodity table.

import "lib/collectors" as collectors;
import "lib/balances" as balances;
import "lib/exchange" as exchange;

/// `env`'s value for `key`, `()` when it has none: the `get-val` of HAT:557-559.
private fn env_get(env, key) {
    if key in env { env[key] } else { () }
}

/// True for any value but `()` and false, as Scheme takes any value but #f as true.
private fn env_truthy(v) {
    v != () && v != false
}

/// `(or v default)`.
private fn or_default(v, default_value) {
    if env_truthy(v) { v } else { default_value }
}

private fn max_int(a, b) {
    if a > b { a } else { b }
}

private fn min_int(a, b) {
    if a < b { a } else { b }
}

/// `gnc:make-html-acct-table/env/accts` (HAT:540-548): a table with the environment `env` and the accounts
/// `accts` added. Reads the Book and the run context, which GnuCash takes from the current book and preferences.
fn make_html_acct_table_env_accts(book, ctx, env, accts) {
    html_acct_table_add_accounts(book, ctx, #{ env: env, rows: [] }, accts)
}

/// `gnc:html-acct-table-num-rows` (HAT:827-828).
fn num_rows(acct_table) {
    acct_table.rows.len()
}

/// `gnc:html-acct-table-get-row-env` (HAT:847-849): the environment of row `row`, `()` past the last row.
fn get_row_env(acct_table, row) {
    if row >= 0 && row < acct_table.rows.len() { acct_table.rows[row] } else { () }
}

/// The balances by account GUID (`calculate-balances`, HAT:612-651): `get_balance_fn(account, start, end)` for
/// each account when given; else the amounts of each account's splits in the period, less the closing
/// transactions' with "pre-closing".
private fn calculate_balances(book, accts, start_date, end_date, get_balance_fn, balance_mode, closing_pattern) {
    let ret_hash = #{};
    if env_truthy(get_balance_fn) {
        for acct in accts {
            ret_hash[acct.guid] = get_balance_fn.call(acct, start_date, end_date);
        }
        return ret_hash;
    }
    let seen = #{};
    for acc in accts {
        if acc.guid in seen {
            continue;
        }
        seen[acc.guid] = true;
        for split in acc.splits_between(start_date, end_date) {
            let acct = split.account;
            if !(acct.guid in ret_hash) {
                ret_hash[acct.guid] = collectors::make_commodity_collector();
            }
            ret_hash[acct.guid].add(acct.commodity, split.amount);
        }
    }
    // "post-closing" counts closing entries; any other mode than these two adds nothing, as GnuCash's prints only.
    if balance_mode == "pre-closing" {
        for split in balances::account_get_trans_type_splits_interval(book, accts, closing_pattern, start_date, end_date) {
            let acct = split.account;
            if !(acct.guid in ret_hash) {
                ret_hash[acct.guid] = collectors::make_commodity_collector();
            }
            ret_hash[acct.guid].add(acct.commodity, -split.amount);
        }
    }
    ret_hash
}

/// `get-balance` (HAT:663-669): a new collector merging the account's cached balance, so in reverse order. A method
/// on the walk's state, so the cache is not copied; internal to this module.
fn map.acct_table_get_balance(acct) {
    let this_collector = collectors::make_commodity_collector();
    if acct.guid in this.balances {
        this_collector.merge(this.balances[acct.guid]);
    }
    this_collector
}

/// `get-balance-sub` (HAT:674-681): the account's balance and every descendant's, in display order, merged.
fn map.acct_table_get_balance_sub(account) {
    let this_collector = collectors::make_commodity_collector();
    for acct in [account] + account.descendants_sorted() {
        this_collector.merge(this.acct_table_get_balance(acct));
    }
    this_collector
}

/// `gnc:html-acct-table-add-accounts!` (HAT:550-825): walk the account tree from the root's children, adding a row
/// for each account of `accounts` the depth limit lets through, and a subtotal row after a parent's children when
/// asked; then give every row its column counts. Returns the table.
fn html_acct_table_add_accounts(book, ctx, acct_table, accounts) {
    let env = acct_table.env;
    let lim = env_get(env, "display_tree_depth");
    let depth_limit = if type_of(lim) == "i64" { lim } else { () };
    let less_p = env_get(env, "account_less_p");
    if less_p == true {
        // GnuCash names `gnc:account-code-less-p` here but never defines it (HAT:509, :573).
        throw "account_less_p true names gnc:account-code-less-p, which GnuCash does not define";
    }
    if env_truthy(env_get(env, "report_budget")) {
        throw "report_budget is for the budget reports, which red-ale does not have";
    }
    let zero_mode = env_get(env, "zero_balance_mode");
    let selected = #{};
    for a in accounts {
        selected[a.guid] = true;
    }
    let start_date = env_get(env, "start_date");
    let end_date = or_default(env_get(env, "end_date"), ctx.today());
    let closing_pattern = or_default(env_get(env, "closing_pattern"),
                                     #{ str: "Closing Entries", cased: false, regexp: false, closing: true });
    let state = #{
        env: env,
        depth_limit: depth_limit,
        limit_behavior: or_default(env_get(env, "depth_limit_behavior"), "summarize"),
        indent: or_default(env_get(env, "initial_indent"), 0),
        less_p: if env_truthy(less_p) { less_p } else { () },
        report_commodity: or_default(env_get(env, "report_commodity"), ctx.report_currency()),
        exchange_fn: env_get(env, "exchange_fn"),
        subtotal_mode: env_truthy(env_get(env, "parent_account_subtotal_mode")),
        zero_mode: if zero_mode == () || type_of(zero_mode) == "bool" { "show-leaf-acct" } else { zero_mode },
        label_mode: or_default(env_get(env, "account_label_mode"), "anchor"),
        selected: selected,
        balances: calculate_balances(book, accounts, start_date, end_date, env_get(env, "get_balance_fn"),
                                     or_default(env_get(env, "balance_mode"), "post-closing"), closing_pattern),
        rows: [],
        acct_depth_reached: 0,
        logi_depth_reached: if depth_limit != () { depth_limit - 1 } else { 0 },
        disp_depth_reached: 0,
    };
    state.acct_table_traverse_accounts(book.root().children_sorted(), 0, 0);
    let label_cols = state.disp_depth_reached + 1;
    let logi_depth_reached = state.logi_depth_reached;
    let rows = state.rows.map(|orig_env| {
        let tree_depth = env_get(orig_env, "display_tree_depth");
        if !env_truthy(tree_depth) {
            tree_depth = ();
        }
        let row_env = orig_env;
        row_env.account_colspan = label_cols - orig_env.display_depth;
        row_env.label_cols = label_cols;
        row_env.account_cols = orig_env.initial_indent + max_int(label_cols, if tree_depth == () { 0 } else { tree_depth });
        // GnuCash takes the minimum with +inf.0 when there is no limit, which makes the count a double; it is the
        // same number.
        row_env.logical_cols = if tree_depth == () { logi_depth_reached } else { min_int(logi_depth_reached, tree_depth) };
        row_env
    });
    acct_table.rows += rows;
    acct_table
}

/// `use-acct?` (HAT:655-659): the account is selected and above the depth limit, or the limit flattens. GnuCash
/// fails on `(< logi-depth #f)` with no limit unless flattening; so does this, with a message.
fn map.acct_table_use_acct(acct, logi_depth) {
    let flatten = this.limit_behavior == "flatten";
    if !flatten && this.depth_limit == () {
        throw "display_tree_depth must be an integer unless depth_limit_behavior is \"flatten\"";
    }
    (flatten || logi_depth < this.depth_limit) && acct.guid in this.selected
}

/// `traverse-accounts!` (HAT:653-788): add the rows of `accts` and their descendants, at `acct_depth` in the tree
/// and `logi_depth` among the selected accounts. True when a row was added. A method on the walk's state, which it
/// changes; it is internal to this module.
fn map.acct_table_traverse_accounts(accts, acct_depth, logi_depth) {
    let less_p = this.less_p;
    let sorted = accts;
    if less_p != () {
        sorted.sort(|a, b| if less_p.call(a, b) { -1 } else if less_p.call(b, a) { 1 } else { 0 });
    }
    let disp_depth = if this.depth_limit != () { min_int(this.depth_limit - 1, logi_depth) } else { logi_depth };
    let row_added = false;
    for acct in sorted {
        let subaccts = acct.children_sorted();
        let account_bal = this.acct_table_get_balance(acct);
        let recursive_bal = this.acct_table_get_balance_sub(acct);
        let anchor = #{ text: acct.name, account: acct };
        let grp_env = this.env;
        grp_env.initial_indent = this.indent;
        grp_env.account = acct;
        grp_env.account_name = acct.name;
        grp_env.account_code = acct.code;
        grp_env.account_type = acct.type;
        grp_env.account_type_string = acct.typeString;
        grp_env.account_guid = acct.guid;
        grp_env.account_description = acct.description;
        grp_env.account_notes = acct.notes;
        grp_env.account_path = acct.fullName;
        grp_env.account_parent = acct.parent;
        grp_env.account_children = subaccts;
        grp_env.account_depth = acct_depth;
        grp_env.logical_depth = logi_depth;
        grp_env.account_commodity = acct.commodity;
        grp_env.account_anchor = anchor;
        grp_env.account_bal = account_bal;
        grp_env.recursive_bal = recursive_bal;
        grp_env.report_comm_account_bal = collectors::sum_collector_commodity(account_bal, this.report_commodity, this.exchange_fn);
        grp_env.report_comm_recursive_bal = collectors::sum_collector_commodity(recursive_bal, this.report_commodity, this.exchange_fn);
        grp_env.report_commodity = this.report_commodity;
        grp_env.exchange_fn = this.exchange_fn;
        let label = switch this.label_mode {
            "anchor" => anchor,
            "name" => #{ text: acct.name },
            _ => (),
        };
        this.acct_depth_reached = max_int(this.acct_depth_reached, acct_depth);
        this.logi_depth_reached = max_int(this.logi_depth_reached, logi_depth);
        this.disp_depth_reached = max_int(this.disp_depth_reached, disp_depth);
        // Parent accounts with a zero recursive balance count as zero-balance leaves.
        let zero_omitted = collectors::commodity_collector_allzero(recursive_bal) && this.zero_mode == "omit-leaf-acct";
        let used = this.acct_table_use_acct(acct, logi_depth);
        let row = -1;
        if used && !zero_omitted {
            let row_env = grp_env;
            row_env.account_label = label;
            row_env.row_type = "account-row";
            row_env.display_depth = disp_depth;
            row_env.indented_depth = disp_depth + this.indent;
            this.rows.push(row_env);
            row = this.rows.len() - 1;
        }
        // Dive into an account even when it is not selected: some of its sub-accounts may be.
        let sub_logi_depth = if used { logi_depth + 1 } else { logi_depth };
        let children_displayed = this.acct_table_traverse_accounts(subaccts, acct_depth + 1, sub_logi_depth);
        // GnuCash appends this key after the others, so a key of the same name in the environment wins.
        if row >= 0 && !("children_displayed" in this.rows[row]) {
            this.rows[row].children_displayed = children_displayed;
        }
        if used && this.subtotal_mode && children_displayed && !zero_omitted {
            let total_label = if label == () { #{ text: "Total " } } else { label };
            total_label.text = "Total " + acct.name;
            this.disp_depth_reached = max_int(this.disp_depth_reached, disp_depth);
            let row_env = grp_env;
            row_env.account_label = total_label;
            row_env.row_type = "subtotal-row";
            row_env.display_depth = disp_depth;
            row_env.indented_depth = disp_depth + this.indent;
            this.rows.push(row_env);
        }
        row_added = row_added || children_displayed || row >= 0;
    }
    row_added
}

/// A label: text, or a cell map (an account anchor, `#{ text, account }`).
private fn label_cell(label) {
    if type_of(label) == "map" { label } else { #{ text: `${label}` } }
}

/// An amount cell: a Monetary, a `commodities` cell (from `gnc_commodity_table`) or `()` for an empty one; `rule`
/// puts an accounting rule above it.
private fn amount_cell(amount, rule) {
    let cell = if amount == () {
        if rule { #{ text: "" } } else { return (); }
    } else if type_of(amount) == "Monetary" {
        #{ monetary: amount }
    } else {
        amount
    };
    if rule {
        cell.rule = true;
    }
    cell
}

/// `gnc:html-table-add-labeled-amount-line!` (HAT:870-923): `rows` with a row of a label and an amount appended.
/// GnuCash pads the row with empty cells so that the amount lands `amount_depth` columns in, of `table_width`; here
/// the row has one label and one amount cell and the amount's `level` is its position from the right,
/// `table_width - amount_depth - 1`. `row_role` stands for GnuCash's row markup ("subtotal", "total" or
/// "grandTotal" for "primary-subheading"); `total_rule` puts an accounting rule above the amount. The label is text or
/// a cell map; the amount a Monetary, a cell map or `()`. Defaults as GnuCash's: `label_depth` 0, `amount_depth`
/// `label_depth + 1`, `table_width` `amount_depth + 1`; GnuCash ignores the colspans and so does this.
fn add_labeled_amount_line(rows, table_width, row_role, total_rule, label, label_depth, amount, amount_depth) {
    let lbl_depth = or_default(label_depth, 0);
    let amt_depth = if amount_depth == () { lbl_depth + 1 } else { amount_depth };
    let tbl_width = if table_width == () { amt_depth + 1 } else { table_width };
    let row = #{
        role: or_default(row_role, "data"),
        depth: lbl_depth,
        level: tbl_width - amt_depth - 1,
        cells: [label_cell(label), amount_cell(amount, env_truthy(total_rule))],
    };
    let out = if rows == () { [] } else { rows };
    out.push(row);
    out
}

/// `gnc-commodity-table` (HAT:925-948): a `commodities` cell with a line per commodity of the collector `amount`,
/// in its order: the amount and its value in `report_commodity` by `exchange_fn`.
fn gnc_commodity_table(amount, report_commodity, exchange_fn) {
    let lines = [];
    for bal in amount.format(|c, a| monetary(c, a)) {
        let line = #{ monetary: bal };
        let converted = exchange::apply_exchange_fn(exchange_fn, bal, report_commodity);
        if converted != () {
            line.converted = converted;
        }
        lines.push(line);
    }
    #{ commodities: lines }
}

/// `gnc:html-table-add-account-balances` (HAT:964-1071): `rows` (`()` for none) with a row for each row of the
/// account table. Each row's balance is the account's own or its subtotal, as the row type, the depth limit and
/// the `params` decide (GnuCash's `bal-method`), with zeros omitted, reversed balances negated (reading the run
/// context's `reverseBalancedAccounts`), and several commodities shown as a commodity table in "table" mode. The
/// `params`: parent_account_balance_mode ("immediate-bal", "recursive-bal" or "omit-bal", the default),
/// zero_balance_display_mode ("show-balance", the default, or "omit-balance"), multicommodity_mode ("table" or
/// false) and rule_mode (subtotal rows get a rule).
fn add_account_balances(ctx, rows, acct_table, params) {
    let out = if rows == () { [] } else { rows };
    for row_env in acct_table.rows {
        // GnuCash appends params after the row's environment, so the row's keys win.
        let env = row_env;
        for key in params.keys() {
            if !(key in env) {
                env[key] = params[key];
            }
        }
        let acct = env.account;
        let children = env.account_children;
        let report_commodity = env.report_commodity;
        let exchange_fn = env.exchange_fn;
        let display_depth = env.display_depth;
        let row_type = env.row_type;
        let rule_mode = row_type == "subtotal-row" && env_truthy(env_get(env, "rule_mode"));
        let multicommodity_mode = env_get(env, "multicommodity_mode");
        let limit_behavior = or_default(env_get(env, "depth_limit_behavior"), "summarize");
        let parent_acct_bal_mode = or_default(env_get(env, "parent_account_balance_mode"), "omit-bal");
        let bal_method = if row_type == "subtotal-row" {
            "recursive-bal"
        } else if display_depth + 1 == env_get(env, "display_tree_depth") {
            if limit_behavior == "summarize" {
                "recursive-bal"
            } else if children.is_empty() {
                "immediate-bal"
            } else {
                parent_acct_bal_mode
            }
        } else if !children.is_empty() {
            parent_acct_bal_mode
        } else {
            "immediate-bal"
        };
        let mode = env_get(env, "zero_balance_display_mode");
        let zero_mode = if mode == () || type_of(mode) == "bool" { "show-balance" } else { mode };
        let bal_sym = switch bal_method {
            "immediate-bal" => "account_bal",
            "recursive-bal" => "recursive_bal",
            _ => (),
        };
        let amt = ();
        let comm_amt = if bal_sym == () { () } else { env_get(env, bal_sym) };
        if env_truthy(comm_amt) {
            if zero_mode == "omit-balance" {
                comm_amt.remove_zeros();
            }
            amt = if zero_mode == "omit-balance" && collectors::commodity_collector_allzero(comm_amt) {
                ()
            } else if acct.reverse_balance(ctx) {
                collectors::commodity_collector_get_negated(comm_amt)
            } else {
                comm_amt
            };
        }
        let amount = if amt == () {
            ()
        } else if !collectors::uniform_commodity(amt, report_commodity) && multicommodity_mode == "table" && row_type == "account-row" {
            gnc_commodity_table(amt, report_commodity, exchange_fn)
        } else {
            collectors::sum_collector_commodity(amt, report_commodity, exchange_fn)
        };
        let account_cols = env.account_cols;
        let logical_cols = env.logical_cols;
        out = add_labeled_amount_line(out, account_cols + logical_cols, if row_type == "subtotal-row" { "subtotal" } else { "account" },
                                      rule_mode, env.account_label, env.indented_depth, amount,
                                      account_cols - 1 + logical_cols - display_depth);
        out[out.len() - 1].account = acct;
    }
    out
}
