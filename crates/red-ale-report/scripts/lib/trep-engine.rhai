// lib/trep-engine: the Transaction Report engine, on which the Transaction Report, the General Ledger and the
// Reconciliation Report are built.
//
// Translated from 5.17:gnucash/report/trep-engine.scm, cited below as TE. Its header:
//
// trep-engine.scm : Transaction Report engine
//
// Original report by Robert Merkel <rgmerk@mira.net>
// Contributions by Bryan Larsen <blarsen@ada-works.com>
// More contributions for new report generation code by Robert Merkel
// More contributions by Christian Stimming <stimming@tuhh.de>
// Modified to support the intersection of two account lists by
// Michael T. Garrison Stuber
// Modified account names display by Tomas Pospisek
// <tpo_deb@sourcepole.ch> with a lot of help from "warlord"
// Refactored by Christopher Lam (2017)
// - introduced account/transaction substring/regex matcher
// - add custom sorter in scheme
// - common currency - optionally show original currency amount
//   and enable multiple data columns
// - add support for indenting for better grouping
// - add subtotal summary grid
// - by default, exclude closing transactions from the report
// - converted to module in 2019
// - CSV export, exports the report headers and totals
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
// The first half holds the options generator (TE:521-1012) and the renderer up to the sorted, filtered splits
// (TE:2203-2594): the parameters, the warnings before the query, the query, the split filter and the custom sort.
// The second half builds the document from them (TE:1014-2178, :2596-2641): the columns and calculated cells, the
// rows with their subheadings and subtotals, the Subtotal Table and the CSV rows. design/notes/report-transaction-
// engine.md (§2) walks through both.
//
// GnuCash's keylists (sortkey-list, date-subtotal-list, …) hold lambdas; here they are functions of the key, and the
// renderer-fn and split-sortvalue a parameter holds is a map naming its key (design/reports.md §12, "Function values
// are maps"), applied with `apply_renderer_fn` and `apply_split_sortvalue`. A calculated cell's calc-fn and
// friendly-heading-fn are likewise names of the default ones, or a caller's closure. Symbols are strings. GnuCash's
// `gnc:string-locale<?` compares bytewise here, as every name sort of red-ale does (design/reports.md §14).

import "lib/accounts" as accounts;
import "lib/collectors" as collectors;
import "lib/options" as opts_util;
import "lib/dates" as dates;
import "lib/exchange" as exchange;
import "lib/html-utilities" as html_utils;

/// `DATE-SORTING-TYPES` (TE:149-150): the keys sorted and grouped by a date subtotal.
fn date_sorting_types() { ["date", "reconciled-date"] }

/// `SORTKEY-INFORMAL-HEADERS` (TE:156-157): the keys whose subheadings can show debit and credit labels.
fn sortkey_informal_headers() { ["account-name", "account-code"] }

/// `reconcile-list` (TE:159-164): each reconcile state and its label, in this order.
fn reconcile_list() {
    [["n", "Unreconciled"], ["c", "Cleared"], ["y", "Reconciled"], ["f", "Frozen"], ["v", "Voided"]]
}

/// `sortkey-list` (TE:166-280): each sort key, its label and its QOF sort parameter (a `book.split_query` key, `()` for
/// GnuCash's #f, `"none"` for its empty list). The split-sortvalue and renderer-fn of each are
/// `sortkey_split_sortvalue` and `sortkey_renderer_fn`.
fn sortkey_list(split_action) {
    [
        ["account-name", "Account Name", "accountFullName"],
        ["account-code", "Account Code", "accountCode"],
        ["date", "Date", "datePosted"],
        ["reconciled-date", "Reconciled Date", "dateReconciled"],
        ["reconciled-status", "Reconciled Status", ()],
        ["register-order", "Register Order", "default"],
        ["corresponding-acc-name", "Other Account Name", "corrAccountFullName"],
        ["corresponding-acc-code", "Other Account Code", "corrAccountCode"],
        ["amount", "Amount", "value"],
        ["description", "Description", "description"],
        if split_action { ["number", "Number/Action", "action"] } else { ["number", "Number", "num"] },
        ["t-number", "Transaction Number", "num"],
        ["memo", "Memo", "memo"],
        ["notes", "Notes", ()],
        ["none", "None", "none"],
    ]
}

/// The QOF sort key of `key` (TE:2541-2549): a `book.split_query` key, or `()` for the keys only the custom sort
/// can order.
fn sortkey_sortkey(split_action, key) {
    for k in sortkey_list(split_action) {
        if k[0] == key { return k[2]; }
    }
    throw `unknown sort key ${key}`;
}

/// The keys whose split-sortvalue is #f (TE:198, :216, :278).
private fn no_split_sortvalue(key) { key in ["register-order", "none"] }

/// The keys whose renderer-fn is #f: they cannot be subtotalled (TE:192, :198, :216, :234, :278).
private fn no_renderer_fn(key) { key in ["date", "reconciled-date", "register-order", "amount", "none"] }

/// A sort key's split-sortvalue (TE:166-280): what the custom sort compares, `()` for #f.
fn sortkey_split_sortvalue(split_action, key, split) {
    let trans = split.transaction;
    switch key {
        "account-name" => split.account.fullName,
        "account-code" => split.account.code,
        "date" => trans.datePosted,
        // `xaccSplitGetDateReconciled` is 0 for a split never reconciled.
        "reconciled-date" => if split.reconcileDate == () { 0 } else { split.reconcileDate },
        // The length of the tail of (n c y f v) from the split's state: voided sorts first (TE:204-207).
        "reconciled-status" => 5 - reconcile_index(split.reconcile),
        "corresponding-acc-name" => split.corrAccountFullName,
        "corresponding-acc-code" => split.corrAccountCode,
        "amount" => split.value,
        "description" => trans.description,
        "number" => if split_action { split.action } else { trans.num },
        "t-number" => trans.num,
        "memo" => split.memo,
        "notes" => trans.notes,
        _ => (),
    }
}

/// The place of a reconcile state in `reconcile_list`.
private fn reconcile_index(state) {
    let states = reconcile_list();
    for i in 0..states.len() {
        if states[i][0] == state { return i; }
    }
    throw `unknown reconcile state ${state}`;
}

/// A sort key's renderer-fn (TE:166-280): what a subheading shows, an Account for the account keys, `()` for #f.
fn sortkey_renderer_fn(split_action, key, split) {
    if no_renderer_fn(key) { return (); }
    switch key {
        "account-name" | "account-code" => split.account,
        "reconciled-status" => reconcile_list()[reconcile_index(split.reconcile)][1],
        "corresponding-acc-name" | "corresponding-acc-code" =>
            if split.otherSplit == () { () } else { split.otherSplit.account },
        _ => sortkey_split_sortvalue(split_action, key, split),
    }
}

/// `date-subtotal-list`'s keys and labels (TE:298-349).
fn date_subtotal_list() {
    [["none", "None"], ["daily", "Daily"], ["weekly", "Weekly"], ["monthly", "Monthly"], ["quarterly", "Quarterly"],
     ["yearly", "Yearly"]]
}

/// A date subtotal's date-sortvalue of the time `t` (TE:282-293, `()` for `none`): the group a day falls in, from
/// its local date.
fn date_sortvalue(ctx, date_subtotal, t) {
    if date_subtotal == "none" { return (); }
    let lt = ctx.local(t);
    switch date_subtotal {
        "daily" => 500 * lt.year + lt.yearday,
        "weekly" => dates::date_get_week(ctx, t),
        "monthly" => 100 * lt.year + lt.month,
        "quarterly" => 10 * lt.year + (lt.month - 1) / 3 + 1,
        "yearly" => lt.year,
        _ => throw `unknown date subtotal ${date_subtotal}`,
    }
}

/// A date subtotal's renderer-fn (TE:313-349): the group's label, from the split's date posted.
fn date_subtotal_renderer_fn(ctx, date_subtotal, split) {
    let t = split.transaction.datePosted;
    switch date_subtotal {
        "daily" => ctx.print_date(t),
        "weekly" => dates::date_get_week_year_string(ctx, t),
        "monthly" => ctx.print_month_year(t),
        // `gnc:date-get-quarter-year-string` (DU:154-161)
        "quarterly" => `Q${(ctx.local(t).month - 1) / 3 + 1} ${ctx.print_year(t)}`,
        "yearly" => ctx.print_year(t),
        _ => (),
    }
}

/// Apply a parameter's split-sortvalue (`primary_key_split_sortvalue`, `secondary_key_split_sortvalue`). A date
/// subtotal's groups by date posted whatever the key (TE:295-296, :2393-2406).
fn apply_split_sortvalue(ctx, f, split) {
    if f.date_subtotal != () {
        date_sortvalue(ctx, f.date_subtotal, split.transaction.datePosted)
    } else {
        sortkey_split_sortvalue(f.split_action, f.key, split)
    }
}

/// Apply a parameter's renderer-fn (`primary_key_renderer_fn`, `secondary_key_renderer_fn`).
fn apply_renderer_fn(ctx, f, split) {
    if f.date_subtotal != () {
        date_subtotal_renderer_fn(ctx, f.date_subtotal, split)
    } else {
        sortkey_renderer_fn(f.split_action, f.key, split)
    }
}

/// `SUBTOTAL-ENABLED?` (TE:448-451): whether `key` can be subtotalled.
fn subtotal_enabled(key) { !no_renderer_fn(key) }

/// `CUSTOM-SORTING?` (TE:453-461): the keys with a split-sortvalue and no QOF sort key, which only the custom sort
/// can order.
fn custom_sorting(split_action, key) {
    !no_split_sortvalue(key) && sortkey_sortkey(split_action, key) == ()
}

/// The choices of the keylists that are only labels (TE:351-435).
private fn filter_list() {
    [["none", "Do not do any filtering"], ["include", "Include Transactions to/from Filter Accounts"],
     ["exclude", "Exclude Transactions to/from Filter Accounts"]]
}
private fn show_void_list() {
    [["non-void-only", "Non-void only"], ["void-only", "Void only"],
     ["both", "Both (and include void transactions in totals)"]]
}
private fn show_closing_list() {
    [["exclude-closing", "Exclude closing transactions"], ["include-both", "Show both closing and regular transactions"],
     ["closing-only", "Show closing transactions only"]]
}
private fn reconcile_status_list() {
    [["all", "Show All Transactions"], ["unreconciled", "Unreconciled only"], ["cleared", "Cleared only"],
     ["reconciled", "Reconciled only"]]
}
private fn ascending_list() { [["ascend", "Ascending"], ["descend", "Descending"]] }

/// `sign-reverse-list` (TE:420-435): each choice's label and the account types it reverses, `()` for the global
/// preference.
fn sign_reverse_list() {
    [["global", "Use Global Preference", ()], ["none", "Don't change any displayed amounts", []],
     ["income-expense", "Income and Expense", ["INCOME", "EXPENSE"]],
     ["credit-accounts", "Credit Accounts", ["LIABILITY", "PAYABLE", "EQUITY", "CREDIT", "INCOME"]]]
}

/// `keylist->vectorlist` (TE:440-446): `[key, label]` choices.
private fn choices(keylist) { keylist.map(|k| [k[0], k[1]]) }

/// `gnc:trep-options-generator` (TE:521-1012). The callbacks GnuCash registers only make other options selectable
/// in its dialog (TE:552-559, :665-673, :687-753, :862-899); red-ale never calls them, so they are `()`
/// (design/reports.md §6).
fn trep_options_generator(o, book) {
    let split_action = book.uses_split_action_for_num();

    // General options
    opts_util::add_date_interval(o, "General", "Start Date", "End Date", "a");
    o.multichoice("General", "Date Filter", "a5", "Specify date to filter by…", "posted",
                  [["posted", "Date Posted"], ["reconciled", "Reconciled Date"], ["entered", "Date Entered"]]);
    o.complex_boolean("Currency", "Common Currency", "a", "Convert all transactions into a common currency.", false, ());
    o.boolean("Currency", "Show original currency amount", "b", "Also show original currency amounts", false);
    opts_util::add_currency(o, "Currency", "Report's currency", "c");
    opts_util::add_price_source(o, "Currency", "Price Source", "d", "pricedb-nearest");
    o.boolean("General", "Table for Exporting", "g",
              "Formats the table suitable for cut & paste exporting with extra cells.", false);

    // Filtering Options
    o.string("Filter", "Account Name Filter", "a5",
             "Show only accounts whose full name matches this filter e.g. ':Travel' will match " +
             "Expenses:Travel:Holiday and Expenses:Business:Travel. It can be left blank, which will disable the filter.", "");
    o.boolean("Filter", "Use regular expressions for account name filter", "a6",
              "By default the account filter will search substring only. Set this to true to enable full POSIX regular " +
              "expressions capabilities. 'Car|Flights' will match both Expenses:Car and Expenses:Flights. Use a period " +
              "(.) to match a single character e.g. '20../.' will match 'Travel 2017/1 London'. ", false);
    o.boolean("Filter", "Account Name Filter excludes matched strings", "a7",
              "If this option is selected, accounts matching filter are excluded.", false);
    o.string("Filter", "Transaction Filter", "i1",
             "Show only transactions where description, notes, or memo matches this filter.\ne.g. '#gift' will find all " +
             "transactions with #gift in description, notes or memo. It can be left blank, which will disable the filter.", "");
    o.boolean("Filter", "Use regular expressions for transaction filter", "i2",
              "By default the transaction filter will search substring only. Set this to true to enable full POSIX " +
              "regular expressions capabilities. '#work|#family' will match both tags within description, notes or memo.",
              false);
    o.boolean("Filter", "Transaction Filter excludes matched strings", "i3",
              "If this option is selected, transactions matching filter are excluded.", false);
    o.boolean("Filter", "Transaction Filter is case insensitive", "i4",
              "If this option is selected, transactions matching filter is not case sensitive.", false);
    o.multichoice("Filter", "Reconciled Status", "j1", "Filter by reconcile status.", "all", reconcile_status_list());
    o.multichoice("Filter", "Void Transactions", "k", "How to handle void transactions.", "non-void-only",
                  show_void_list());
    o.multichoice("Filter", "Closing transactions", "l",
                  "By default most users should not include closing transactions in a transaction report. Closing " +
                  "transactions are transfers from income and expense accounts to equity, and must usually be excluded " +
                  "from periodic reporting.", "exclude-closing", show_closing_list());

    // Accounts options: no account by default, so the no-account warning shows (TE:650-658).
    o.account_list("Accounts", "Accounts", "a", "Report on these accounts.", [], #{});
    o.account_list("Accounts", "Filter By…", "c1", "Filter on these accounts.", [], #{});
    o.multichoice_callback("Accounts", "Filter Type", "c", "Filter account.", "none", filter_list(), ());

    // Sorting options
    let key_choice_list = choices(sortkey_list(split_action));
    o.multichoice_callback("Sorting", "Primary Key", "a", "Sort by this criterion first.", "account-name",
                           key_choice_list, ());
    o.boolean("Sorting", "Show Full Account Name", "j1", "Show the full account name for subtotals and subheadings?", false);
    o.boolean("Sorting", "Show Account Code", "j2", "Show the account code for subtotals and subheadings?", false);
    o.boolean("Sorting", "Show Account Description", "j3", "Show the account description for subheadings?", false);
    o.boolean("Sorting", "Show Informal Debit/Credit Headers", "j4",
              "Show the informal headers for debit/credit accounts?", false);
    o.boolean("Sorting", "Add indenting columns", "j5", "Add indenting columns with grouping and subtotals?", true);
    o.boolean("Sorting", "Show subtotals only (hide transactional data)", "j6",
              "Show subtotals only, hiding transactional detail?", false);
    o.complex_boolean("Sorting", "Primary Subtotal", "e5", "Subtotal according to the primary key?", true, ());
    o.multichoice_callback("Sorting", "Primary Subtotal for Date Key", "e2", "Do a date subtotal.", "monthly",
                           date_subtotal_list(), ());
    o.multichoice("Sorting", "Primary Sort Order", "e", "Order of primary sorting.", "ascend", ascending_list());
    o.multichoice_callback("Sorting", "Secondary Key", "f", "Sort by this criterion second.", "register-order",
                           key_choice_list, ());
    o.complex_boolean("Sorting", "Secondary Subtotal", "i5", "Subtotal according to the secondary key?", false, ());
    o.multichoice_callback("Sorting", "Secondary Subtotal for Date Key", "i2", "Do a date subtotal.", "monthly",
                           date_subtotal_list(), ());
    o.multichoice("Sorting", "Secondary Sort Order", "i", "Order of Secondary sorting.", "ascend", ascending_list());

    // Display options (TE:900-928): option name, sort tag, help, default.
    let booleans = [
        ["Date", "a", "Display the date?", true],
        ["Reconciled Date", "a2", "Display the reconciled date?", false],
        ["Date Entered", "a3", "Display the entered date?", false],
        [if split_action { "Num/Action" } else { "Num" }, "b", "Display the check number?", true],
        ["Description", "c", "Display the description?", true],
        ["Notes", "d2", "Display the notes if the memo is unavailable?", true],
        ["Use Full Account Name", "f", "Display the full account name?", true],
        ["Account Code", "g", "Display the account code?", false],
        ["Use Full Other Account Name", "i", "Display the full account name?", false],
        ["Other Account Code", "j", "Display the other account code?", false],
        ["Shares", "k", "Display the number of shares?", false],
        ["Link", "l5", "Display the transaction linked document", false],
        ["Price", "l", "Display the shares price?", false],
        ["Subtotal Table", "m5", "Display a subtotal summary table.", false],
        ["Account Balance", "n", "Display the balance of the underlying account on each line?", false],
        ["Grand Total", "o", "Display a grand total section at the bottom?", true],
    ];
    for b in booleans { o.boolean("Display", b[0], b[1], b[2], b[3]); }
    if split_action { o.boolean("Display", "Trans Number", "b2", "Display the trans number?", false); }
    o.complex_boolean("Display", "Memo", "d", "Display the memo?", true, ());
    o.boolean("Display", "Invoice", "d5", "Display invoice details", false);
    o.complex_boolean("Display", "Account Name", "e", "Display the account name?", false, ());
    o.complex_boolean("Display", "Other Account Name", "h5",
                      "Display the other account name? If this is a split transaction, this parameter is guessed.", true, ());
    o.multichoice_callback("Display", "Detail Level", "h", "Amount of detail to display per transaction.", "single",
                           [["multi-line", "One split per line"], ["single", "One transaction per line"]], ());
    o.multichoice_callback("Display", "Amount", "m", "Display the amount?", "double",
                           [["none", "Hide"], ["single", "Single Column"], ["double", "Two Columns"]], ());
    o.boolean("Display", "Enable Links", "m2", "Enable hyperlinks in amounts.", true);
    o.multichoice("Display", "Sign Reverses", "m1", "Reverse amount display for certain account types.", "global",
                  choices(sign_reverse_list()));
    o.multichoice("Display", "Running Totals", "o2", "Display running totals as per report sort order?", "none",
                  [["none", "None"], ["all", "Grand Total and Subtotals"], ["grand", "Grand Total Only"],
                   ["sub", "Subtotals Only"]]);

    // Whether the query keeps one split per transaction; a derived report sets it (TE:1005-1009).
    o.internal("__trep", "unique-transactions", "", "", false);
    o.set_default_section("General");
}

/// The renderer's keyword arguments (TE:2179-2201), as the keys of its `hooks` map. Function values are the
/// caller's closures or function pointers. `#:export-type` is `ctx.export_type()`.
fn trep_hooks() {
    ["custom_calculated_cells", "empty_report_message", "custom_split_filter", "split_to_date",
     "split_to_date_include_false", "custom_source_accounts"]
}

private fn hook(hooks, name) { if name in hooks { hooks[name] } else { () } }

private fn between(from, t, to) { from <= t && t <= to }

/// The compiled expression of a filter whose regular-expression option is on, or GnuCash's error symbol as a string
/// when it does not compile (TE:2212-2218, :2245-2254); `()` when the option is off.
private fn matcher_regexp(use_regexp, pattern, icase, error) {
    if !use_regexp { return (); }
    let re = make_regexp(pattern, icase);
    if re == () { error } else { re }
}

/// `match?` (TE:2439-2446): the expression found anywhere in `str`, else the substring, ignoring case when asked
/// (`string-contains-ci`; lowercased, as `transaction.find` folds).
private fn match_str(re, icase, matcher, str) {
    if type_of(re) == "Regexp" {
        re.is_match(str)
    } else if icase {
        str.to_lower().contains(matcher.to_lower())
    } else {
        str.contains(matcher)
    }
}

/// `is-filter-member` (TE:2491-2499): whether another split of the split's transaction is in `accounts`.
private fn is_filter_member(split, accounts) {
    for s in split.transaction.splits() {
        if s != split && s.account != () && s.account in accounts { return true; }
    }
    false
}

/// GnuCash's `cleared-filter` as reconcile states: the Reconciled Status choice's flags AND the Void Transactions
/// choice's (TE:362-410, :2259-2262). `unreconciled` with `void-only` leaves none, and nothing matches.
private fn cleared_filter(reconcile_filter, void_filter) {
    let states = switch reconcile_filter {
        "all" => ["n", "c", "y", "f", "v"],
        "unreconciled" => ["n"],
        "cleared" => ["c"],
        "reconciled" => ["y"],
    };
    let allowed = switch void_filter { "non-void-only" => ["n", "c", "y", "f"], "void-only" => ["v"], _ => states };
    let both = [];
    for s in states { if s in allowed { both.push(s); } }
    both
}

/// What `primary-get-info` and `secondary-get-info` give (TE:2393-2406): for a date key, its date subtotal's
/// functions, #f for `none`; otherwise the key's, when it can be subtotalled and its subtotal option is on. A key's
/// renderer-fn and split-sortvalue are #f together, so one map stands for both; `()` is #f.
private fn key_get_info(split_action, key, date_subtotal, subtotal) {
    if key in date_sorting_types() {
        if date_subtotal == "none" { () } else { #{ key: key, date_subtotal: date_subtotal, split_action: split_action } }
    } else if subtotal_enabled(key) && subtotal {
        #{ key: key, date_subtotal: (), split_action: split_action }
    } else {
        ()
    }
}

/// The parameters (TE:2289-2429): the options read once, under GnuCash's names with `-` and `/` as `_`. GnuCash's
/// `(and a b)` of a boolean option and a value is the value or false.
private fn parameters(book, opts, c_account_1, transaction_matcher, begindate, enddate) {
    let split_action = book.uses_split_action_for_num();
    let single = opts.get("Display", "Detail Level") == "single";
    let common = opts.get("Currency", "Common Currency");
    let amount = opts.get("Display", "Amount");
    let running = opts.get("Display", "Running Totals");
    let running_sub = running == "sub" || running == "all";
    let primary_key = opts.get("Sorting", "Primary Key");
    let secondary_key = opts.get("Sorting", "Secondary Key");
    let primary_date_subtotal = opts.get("Sorting", "Primary Subtotal for Date Key");
    let secondary_date_subtotal = opts.get("Sorting", "Secondary Subtotal for Date Key");
    let primary = key_get_info(split_action, primary_key, primary_date_subtotal, opts.get("Sorting", "Primary Subtotal"));
    let secondary = key_get_info(split_action, secondary_key, secondary_date_subtotal,
                                 opts.get("Sorting", "Secondary Subtotal"));
    let sign_reverses = opts.get("Display", "Sign Reverses");
    #{
        split_action: split_action,
        accounts_matched: if c_account_1.is_empty() { true } else { c_account_1 },
        common_currency: if common { opts.get("Currency", "Report's currency") } else { false },
        common_currency_original: common && opts.get("Currency", "Show original currency amount"),
        common_currency_price_source: if common { opts.get("Currency", "Price Source") } else { false },
        date: opts.get("Display", "Date"),
        reconciled_date: opts.get("Display", "Reconciled Date"),
        entered: opts.get("Display", "Date Entered"),
        num: opts.get("Display", if split_action { "Num/Action" } else { "Num" }),
        description: opts.get("Display", "Description"),
        account_name: opts.get("Display", "Account Name"),
        other_account_name: single && opts.get("Display", "Other Account Name"),
        shares: opts.get("Display", "Shares"),
        price: opts.get("Display", "Price"),
        doclink: opts.get("Display", "Link"),
        amount_single: amount == "single",
        amount_double: amount == "double",
        running_balance: opts.get("Display", "Account Balance"),
        account_full_name: opts.get("Display", "Use Full Account Name"),
        memo: opts.get("Display", "Memo"),
        invoice: opts.get("Display", "Invoice"),
        notes: opts.get("Display", "Notes"),
        account_code: opts.get("Display", "Account Code"),
        other_account_code: single && opts.get("Display", "Other Account Code"),
        other_account_full_name: single && opts.get("Display", "Use Full Other Account Name"),
        trans_number: split_action && opts.get("Display", "Trans Number"),
        links: opts.get("Display", "Enable Links"),
        // `(or (eq? reversed-signs 'none) reversed-signs)` (TE:2345): true for "none", else the choice.
        reversed_signs: if sign_reverses == "none" { true } else { sign_reverses },
        multiline: opts.get("Display", "Detail Level") == "multi-line",
        grand_total: opts.get("Display", "Grand Total"),
        running_grand_total: running == "grand" || running == "all",
        begindate: begindate,
        enddate: enddate,
        export_table: opts.get("General", "Table for Exporting"),
        primary_key: primary_key,
        primary_date_subtotal: if primary_key in date_sorting_types() { primary_date_subtotal } else { () },
        secondary_key: secondary_key,
        secondary_date_subtotal: if secondary_key in date_sorting_types() { secondary_date_subtotal } else { () },
        indenting: opts.get("Sorting", "Add indenting columns"),
        sort_account_code: opts.get("Sorting", "Show Account Code"),
        sort_account_full_name: opts.get("Sorting", "Show Full Account Name"),
        sort_account_description: opts.get("Sorting", "Show Account Description"),
        informal_headers: opts.get("Sorting", "Show Informal Debit/Credit Headers"),
        // A running balance with a balance brought forward keeps most defaults (TE:2370-2389).
        bal_bf: single && opts.get("General", "Date Filter") == "posted" && transaction_matcher == ""
            && opts.get("Filter", "Reconciled Status") == "all" && opts.get("Filter", "Void Transactions") == "non-void-only"
            && primary_key in ["account-name", "account-code"] && secondary_key in ["register-order", "date"]
            && opts.get("Sorting", "Primary Subtotal") && opts.get("Sorting", "Secondary Sort Order") == "ascend",
        running_prime: primary != () && running_sub,
        running_sec: secondary != () && running_sub,
        primary_key_renderer_fn: primary,
        primary_key_split_sortvalue: primary,
        secondary_key_renderer_fn: secondary,
        secondary_key_split_sortvalue: secondary,
        subtotals_only: opts.get("Sorting", "Show subtotals only (hide transactional data)") && (primary != () || secondary != ()),
    }
}

/// `custom-sort?` (TE:2431-2437): a date key with a date subtotal, or a key only the custom sort can order.
private fn custom_sort(p) {
    (p.primary_date_subtotal != () && p.primary_date_subtotal != "none")
        || (p.secondary_date_subtotal != () && p.secondary_date_subtotal != "none")
        || custom_sorting(p.split_action, p.primary_key) || custom_sorting(p.split_action, p.secondary_key)
}

/// The value `generic-less?` compares (TE:2448-2472): for a date key, its date subtotal's date-sortvalue of the
/// key's date, so a `reconciled-date` key sorts by the reconcile date's group; otherwise the key's split-sortvalue.
private fn generic_sortvalue(ctx, split_action, key, date_subtotal, split) {
    let value = sortkey_split_sortvalue(split_action, key, split);
    if key in date_sorting_types() { date_sortvalue(ctx, date_subtotal, value) } else { value }
}

/// `stable-sort!` by `generic-less?` (TE:2448-2472, :2591-2594): ascending by `<`, descending by `>`, on strings
/// bytewise. A key whose value is #f orders nothing, so the sort is left out.
private fn stable_sort(ctx, split_action, splits, key, date_subtotal, ascend) {
    let unordered = if key in date_sorting_types() { date_subtotal == "none" } else { no_split_sortvalue(key) };
    if unordered { return splits; }
    let decorated = [];
    for s in splits { decorated.push([generic_sortvalue(ctx, split_action, key, date_subtotal, s), s]); }
    if ascend {
        decorated.sort(|a, b| if a[0] < b[0] { -1 } else if b[0] < a[0] { 1 } else { 0 });
    } else {
        decorated.sort(|a, b| if a[0] > b[0] { -1 } else if b[0] > a[0] { 1 } else { 0 });
    }
    decorated.map(|d| d[1])
}

/// The split query (TE:2533-2553): the splits of the filtered accounts in the reconcile states asked for, posted
/// between the bounds under the posted date filter, closing or not as asked; sorted by the primary key, the
/// secondary and split order unless the custom sort will order them. One split per transaction under
/// `__trep`/`unique-transactions`.
private fn run_query(book, opts, p, accounts, date_source, custom) {
    let q = #{
        accounts: accounts,
        reconcile: cleared_filter(opts.get("Filter", "Reconciled Status"), opts.get("Filter", "Void Transactions")),
        unique: opts.get("__trep", "unique-transactions"),
    };
    if date_source == "posted" {
        q["from"] = p.begindate;
        q["to"] = p.enddate;
    }
    let closing = opts.get("Filter", "Closing transactions");
    if closing != "include-both" { q["isClosing"] = closing == "closing-only"; }
    if !custom {
        q["sort"] = [
            #{ key: sortkey_sortkey(p.split_action, p.primary_key), ascending: opts.get("Sorting", "Primary Sort Order") == "ascend" },
            #{ key: sortkey_sortkey(p.split_action, p.secondary_key), ascending: opts.get("Sorting", "Secondary Sort Order") == "ascend" },
            #{ key: "default" },
        ];
    }
    book.split_query(q)
}

/// The combined filter (TE:2557-2589): the date source, Filter Type, the Transaction Filter and the caller's split
/// filter, in that order.
private fn keep_split(split, f) {
    let trans = split.transaction;
    let dated = switch f.date_source {
        "posted" => true,
        // Only a reconciled split is checked against the dates; `xaccSplitGetDateReconciled` is 0 when never set.
        "reconciled" => split.reconcile != "y"
            || between(f.begindate, if split.reconcileDate == () { 0 } else { split.reconcileDate }, f.enddate),
        "entered" => between(f.begindate, trans.dateEntered, f.enddate),
        "custom" => {
            let split_to_date = f.split_to_date;
            let date = split_to_date.call(split);
            if date == () || date == false { f.split_to_date_include_false == true } else { between(f.begindate, date, f.enddate) }
        },
        _ => throw `invalid date-source ${f.date_source}`,
    };
    let filtered = switch f.filter_mode {
        "include" => is_filter_member(split, f.filter_by),
        "exclude" => !is_filter_member(split, f.filter_by),
        _ => true,
    };
    let matched = true;
    if f.matcher != "" {
        let found = match_str(f.re, f.icase, f.matcher, trans.description)
            || match_str(f.re, f.icase, f.matcher, trans.notes) || match_str(f.re, f.icase, f.matcher, split.memo);
        matched = found != f.exclude;
    }
    let custom_split_filter = f.custom_split_filter;
    dated && filtered && matched && (custom_split_filter == () || custom_split_filter.call(split))
}

/// The first half of `gnc:trep-renderer` (TE:2179-2594): the parameters, then the splits the report shows, filtered
/// and sorted. `hooks` holds the renderer's keyword arguments (`trep_hooks`), each optional. Returns
/// `#{ parameters, report_title, subtotal_table, splits, warning, export_error }`:
///
/// - `parameters`: the map `parameters` builds, which the table reads;
/// - `subtotal_table`: GnuCash's `subtotal-table?` (TE:2281-2287);
/// - `splits`: the splits in report order; `()` when a warning came before the query (no account, or a regular
///   expression that does not compile), `[]` when no split matched;
/// - `warning` and `export_error`: the warning block and export error of those cases (TE:2501-2531, :2596-2605),
///   else `()`. The empty-report message follows only the first kind (TE:2526-2531).
fn trep_splits(book, opts, ctx, hooks) {
    for k in hooks.keys() {
        if !(k in trep_hooks()) { throw `unknown trep-renderer hook ${k}`; }
    }
    let report_title = opts.get("General", "Report name");
    let account_matcher = opts.get("Filter", "Account Name Filter");
    let account_re = matcher_regexp(opts.get("Filter", "Use regular expressions for account name filter"), account_matcher,
                                    false, "invalid-account-regex");
    let c_account_0 = hook(hooks, "custom_source_accounts") ?? opts.get("Accounts", "Accounts");
    let c_account_1 = c_account_0;
    if account_matcher != "" {
        let exclude = opts.get("Filter", "Account Name Filter excludes matched strings");
        c_account_1 = [];
        for a in c_account_0 {
            if match_str(account_re, false, account_matcher, a.fullName) != exclude { c_account_1.push(a); }
        }
    }
    let transaction_matcher = opts.get("Filter", "Transaction Filter");
    let icase = opts.get("Filter", "Transaction Filter is case insensitive");
    let transaction_re = matcher_regexp(opts.get("Filter", "Use regular expressions for transaction filter"),
                                        transaction_matcher, icase, "invalid-transaction-regex");
    let begindate = ctx.start_of_day(opts.get("General", "Start Date"));
    let enddate = ctx.end_of_day(opts.get("General", "End Date"));
    let p = parameters(book, opts, c_account_1, transaction_matcher, begindate, enddate);
    // `subtotal-table?` (TE:2281-2287) reads the Primary Subtotal option, not whether the key can be subtotalled.
    let primary_subtotal = if p.primary_key in date_sorting_types() {
        opts.get("Sorting", "Primary Subtotal for Date Key") != "none"
    } else {
        opts.get("Sorting", "Primary Subtotal")
    };
    let result = #{
        parameters: p,
        report_title: report_title,
        subtotal_table: opts.get("Display", "Subtotal Table") && primary_subtotal && opts.get("Display", "Amount") != "none",
        splits: (),
        warning: (),
        export_error: (),
    };

    let warning = if c_account_1.is_empty() {
        html_utils::make_no_account_warning(report_title)
    } else if type_of(account_re) == "string" || type_of(transaction_re) == "string" {
        let error = if type_of(account_re) == "string" { account_re } else { transaction_re };
        html_utils::make_generic_warning(report_title, `Error ${error}`, "")
    };
    if warning != () {
        result.warning = warning;
        result.export_error = "No accounts, or regexp error";
        return result;
    }

    let custom = custom_sort(p);
    let split_to_date = hook(hooks, "split_to_date");
    let date_source = if split_to_date != () { "custom" } else { opts.get("General", "Date Filter") };
    let splits = run_query(book, opts, p, c_account_1, date_source, custom);
    let f = #{
        date_source: date_source, begindate: begindate, enddate: enddate,
        split_to_date: split_to_date, split_to_date_include_false: hook(hooks, "split_to_date_include_false"),
        filter_mode: opts.get("Accounts", "Filter Type"), filter_by: opts.get("Accounts", "Filter By…"),
        matcher: transaction_matcher, re: transaction_re, icase: icase,
        exclude: opts.get("Filter", "Transaction Filter excludes matched strings"),
        custom_split_filter: hook(hooks, "custom_split_filter"),
    };
    let kept = [];
    for s in splits { if keep_split(s, f) { kept.push(s); } }

    // GnuCash sorts by date first, with `generic-less?` of key 'date and date subtotal 'none, which never orders two
    // splits (TE:2483-2484, :2592): that sort is left out.
    if custom {
        kept = stable_sort(ctx, p.split_action, kept, p.secondary_key, p.secondary_date_subtotal,
                           opts.get("Sorting", "Secondary Sort Order") == "ascend");
        kept = stable_sort(ctx, p.split_action, kept, p.primary_key, p.primary_date_subtotal,
                           opts.get("Sorting", "Primary Sort Order") == "ascend");
    }
    result.splits = kept;
    if kept.is_empty() {
        result.warning = html_utils::make_generic_warning(report_title, "No matching transactions found",
            "No transactions were found that match the time interval and account selection specified in the Options panel.");
        result.export_error = "No splits found";
    }
    result
}

// ---------------------------------------------------------------------------------------------------------------
// The second half: the table (`make-split-table`, TE:1052-2063) and the document (TE:2596-2641).

/// `ACCOUNT-SORTING-TYPES` (TE:153-155): the keys whose subheadings name an account.
private fn account_sorting_types() { ["account-name", "corresponding-acc-name", "account-code", "corresponding-acc-code"] }

/// `n` empty cells (`gnc:html-make-empty-cells`).
private fn nulls(n) {
    let cells = [];
    for i in 0..n { cells.push(()); }
    cells
}

/// A text cell, `()` for the empty string, which GnuCash's tests read as no text.
private fn text_cell(s) { if s == "" { () } else { #{ text: s } } }

/// Whether the report converts to a common currency: the parameter is the currency, or false.
private fn common(p) { type_of(p.common_currency) != "bool" }

/// `account-namestring` (TE:1798-1814): the code and a space when asked and set, then the full name or the name.
fn account_namestring(account, show_code, show_name, show_full_name) {
    if account == () { return "Split Transaction"; }
    let s = "";
    if show_code && account.code != "" { s += account.code + " "; }
    if show_name { s += if show_full_name { account.fullName } else { account.name }; }
    s
}

/// `gnc-get-num-action` (`5.17:libgnucash/engine/engine-helpers.c:56-76`): the split's action in a book that uses
/// it for the num field, else the transaction's num; the num for no split.
fn get_num_action(split_action, trans, split) {
    if split != () && split_action { split.action } else { trans.num }
}

/// The amount the amount cells show (TE:1262-1264): a void transaction's former amount.
private fn split_amount(split) {
    if split.transaction.isVoid { split.voidFormerAmount } else { split.amount }
}

/// `split->invoice-anchors` (TE:1036-1049): the invoices of the lots of the transaction's A/P and A/R splits, each
/// new one put in front, so in reverse order of their splits.
private fn invoice_anchors(split) {
    let anchors = [];
    let seen = [];
    for s in split.transaction.splits() {
        if s.account == () || !(s.account.type in ["PAYABLE", "RECEIVABLE"]) || s.lot == () { continue; }
        let inv = s.lot.invoice;
        if inv == () || inv.guid in seen { continue; }
        seen.push(inv.guid);
        anchors.insert(0, html_utils::invoice_anchor(inv.guid, inv.id));
    }
    anchors
}

/// The left columns (TE:1103-1252), each `[on, heading, kind, role]`.
private fn left_column_list(p) {
    [
        [p.date, "Date", "date", "date"],
        [p.entered, "Date Entered", "entered", "date"],
        [p.reconciled_date, "Reconciled Date", "reconciled-date", "date"],
        [p.num, if p.trans_number { "Num/T-Num" } else { "Num" }, "num", "text"],
        [p.description, "Description", "description", "text"],
        [p.memo, if p.notes { "Memo/Notes" } else { "Memo" }, "memo", "text"],
        [p.invoice, "Invoice", "invoice", "text"],
        [p.account_name || p.account_code, "Account", "account", "text"],
        [p.other_account_name || p.other_account_code, "Transfer from/to", "other-account", "text"],
        [p.shares, "Shares", "shares", "amount"],
        [p.doclink, "", "doclink", "text"],
        [p.price, "Price", "price", "amount"],
    ]
}

/// The left columns shown, each `#{ heading, kind, role }`; with only subtotals shown, or none chosen but a total or
/// subheading to label, one empty column instead (TE:1246-1252).
private fn left_columns(p) {
    let cols = [];
    for c in left_column_list(p) { if c[0] { cols.push(#{ heading: c[1], kind: c[2], role: c[3] }); } }
    if p.subtotals_only || (cols.is_empty()
        && (p.grand_total || p.primary_key_renderer_fn != () || p.secondary_key_renderer_fn != ())) {
        return [#{ heading: "", kind: "blank", role: "label" }];
    }
    cols
}

/// A left column's cell for `split` (the renderer-fns of TE:1103-1244); `tr` is whether the row is the
/// transaction's own, which some show only on.
private fn left_cell(p, kind, split, tr) {
    let trans = split.transaction;
    switch kind {
        "date" => if tr { #{ date: trans.datePosted } },
        "entered" => if tr { #{ date: trans.dateEntered } },
        "reconciled-date" => if split.reconcile == "y" { #{ date: split.reconcileDate ?? 0 } },
        "num" => if tr {
            let num = get_num_action(p.split_action, trans, split);
            let t_num = if p.trans_number { get_num_action(p.split_action, trans, ()) } else { "" };
            text_cell(if t_num == "" { num } else { `${num}/${t_num}` })
        },
        "description" => if tr { text_cell(trans.description) },
        "memo" => text_cell(if split.memo == "" && p.notes { trans.notes } else { split.memo }),
        "invoice" => if tr {
            let anchors = invoice_anchors(split);
            if !anchors.is_empty() { #{ anchors: anchors } }
        },
        "account" => text_cell(account_namestring(split.account, p.account_code, p.account_name, p.account_full_name)),
        // The other split's account, "Split Transaction" when there is no one other split (TE:1198-1210).
        "other-account" => if trans.splitCount > 1 {
            let other = split.otherSplit;
            text_cell(account_namestring(if other == () { () } else { other.account }, p.other_account_code,
                                         p.other_account_name, p.other_account_full_name))
        },
        // The amount in a number cell, two places (TE:1212-1218).
        "shares" => #{ number: split.amount },
        "doclink" => if trans.docLink != "" {
            if p.links { html_utils::html_doclink_anchor(trans.docLink, #{ text: "L" }) } else { #{ text: "L" } }
        },
        "price" => #{ price: split.sharePrice, currency: trans.currency },
        _ => (),
    }
}

/// A calculated cell (TE:1350-1365) with GnuCash's keys; `calc_fn` and `friendly_heading_fn` name default
/// functions ("converted-amount", "debit", "bal-bf", …) or are a caller's closures; `()` is #f.
fn calc_cell(heading, calc_fn, reverse, subtotal, start_dual, friendly, merge_dual) {
    #{ heading: heading, calc_fn: calc_fn, reverse_column: reverse, subtotal: subtotal, start_dual_column: start_dual,
       friendly_heading_fn: friendly, merge_dual_column: merge_dual }
}

/// `default-calculated-cells` (TE:1260-1555): the converted columns, then with the original currency amount the
/// same in each account's commodity. Converted headings name the report currency (`header-commodity`, TE:1270-1277).
private fn default_calculated_cells(p) {
    let cells = [];
    let any_renderer = p.primary_key_renderer_fn != () || p.secondary_key_renderer_fn != ();
    for original in [false, true] {
        if original && !p.common_currency_original { break; }
        let h = if original || !common(p) { "" } else { ` (${p.common_currency.mnemonic})` };
        let f = if original { "original-" } else { "converted-" };
        if p.amount_single { cells.push(calc_cell(`Amount${h}`, f + "amount", true, true, false, "", false)); }
        if p.amount_double {
            cells.push(calc_cell(`Debit${h}`, f + "debit", false, true, true, "debit", true));
            cells.push(calc_cell(`Credit${h}`, f + "credit", false, true, false, "credit", false));
        }
        if p.running_balance && p.bal_bf {
            cells.push(calc_cell(`Running Balance${h}`, f + "balance", true, false, false,
                                 if original { "original-bal-bf" } else { "bal-bf" }, false));
        } else if p.running_balance {
            cells.push(calc_cell(`Account Balance${h}`, f + "balance", true, false, false, (), false));
        }
        if p.running_sec { cells.push(calc_cell(`Running Secondary Subtotal${h}`, f + "running-sec", false, false, false, "", false)); }
        if p.running_prime {
            let name = if p.secondary_key_renderer_fn != () { "Running Primary Subtotal" } else { "Running Subtotal" };
            cells.push(calc_cell(name + h, f + "running-prime", false, false, false, "", false));
        }
        if p.running_grand_total {
            let name = if any_renderer { "Running Grand Total" } else { "Running Total" };
            cells.push(calc_cell(name + h, f + "running-total", false, false, false, "", false));
        }
    }
    cells
}

/// `upgrade-vector-to-assoclist` (TE:1014-1026): a cell of the legacy form `[heading, calc-fn of the split,
/// reverse?, subtotal?, start-dual?, friendly-heading-fn, merge-dual?]`.
private fn upgrade_vector(col) {
    let cell = calc_cell(col[0], col[1], col[2], col[3], col[4], col[5], col.len() >= 7 && col[6]);
    cell.legacy = true;
    cell
}

/// The calculated cells (TE:1557-1573): the caller's `custom_calculated_cells` of the options, or the defaults when
/// it gives none or a cell lacks a key (logged, as GnuCash's `gnc:error`); with only subtotals shown, the cells that
/// are subtotalled.
private fn calculated_cells(p, opts, custom) {
    let cells = default_calculated_cells(p);
    if custom != () {
        let ccc = custom.call(opts);
        let keys = ["heading", "calc_fn", "reverse_column", "subtotal", "start_dual_column", "friendly_heading_fn",
                    "merge_dual_column"];
        if type_of(ccc) != "array" || ccc.is_empty() {
            print(`welp ${ccc}`);
        } else if type_of(ccc[0]) == "array" {
            cells = [];
            for col in ccc { cells.push(upgrade_vector(col)); }
        } else {
            let valid = true;
            for cell in ccc {
                for k in keys { if valid && !(k in cell) { print(`field ${k} missing in cell ${cell}`); valid = false; } }
            }
            if valid { cells = ccc; }
        }
    }
    if !p.subtotals_only { return cells; }
    let kept = [];
    for c in cells { if c.subtotal { kept.push(c); } }
    kept
}

/// The table's environment: what `make-split-table` defines once (TE:1054-1100, :1557-1595), and `mem`, a Memo
/// holding the six running-total collectors by number (0-5: converted and original total, primary, secondary).
private fn table_env(book, opts, ctx, p, hooks) {
    let types = ();
    for e in sign_reverse_list() { if type_of(p.reversed_signs) == "string" && e[0] == p.reversed_signs { types = e[2]; } }
    let exchange_fn = if !common(p) { () } else {
        exchange::case_exchange_time_fn(book, ctx, p.common_currency_price_source, p.common_currency,
                                        accounts::accounts_get_commodities(p.accounts_matched, ()), p.enddate)
    };
    let left = left_columns(p);
    let cells = calculated_cells(p, opts, hook(hooks, "custom_calculated_cells"));
    let any_subtotal = false;
    for c in cells { if c.subtotal { any_subtotal = true; } }
    let primary_indent = if p.indenting && p.primary_key_renderer_fn != () { 1 } else { 0 };
    let secondary_indent = if p.indenting && p.secondary_key_renderer_fn != () { 1 } else { 0 };
    #{
        book: book, ctx: ctx, p: p, types_to_reverse: types, exchange_fn: exchange_fn, left: left, cells: cells,
        any_subtotal: any_subtotal, primary_indent: primary_indent, indent_level: primary_indent + secondary_indent,
        width_left: left.len(), mem: memo(),
        subtotals_only_option: opts.get("Sorting", "Show subtotals only (hide transactional data)"),
        sec_subtotal_option: opts.get("Sorting", "Secondary Subtotal"),
    }
}

/// `acc-reverse?` (TE:1072-1080): whether the amounts of `account` are shown negated. Sign Reverses "none" is #t as
/// a parameter (TE:2345); `sign-reverse-list` has no entry for #t, and `assq-ref` of #f is #f, so it reads the
/// global preference, as "global" does.
private fn acc_reverse(t, account) {
    if t.types_to_reverse == () { account.reverse_balance(t.ctx) } else { account.type in t.types_to_reverse }
}

/// `exchange-fn` (TE:1082-1089) of `foreign` into `domestic` at `date`.
private fn exchange_at(t, foreign, domestic, date) {
    if t.exchange_fn == () {
        exchange::exchange_by_pricedb_nearest(t.book, foreign, domestic, date)
    } else {
        exchange::apply_exchange_time_fn(t.exchange_fn, foreign, domestic, date)
    }
}

/// `row-currency` (TE:1266-1267): the report currency under Common Currency, else the account's commodity.
private fn row_currency(t, account) {
    if common(t.p) { t.p.common_currency } else { account.commodity }
}

/// `add-split-to-collector` (TE:1313-1320): add `m` to running collector `k` and give its total in `m`'s commodity.
private fn running_add(t, k, m) {
    let c = t.mem.get(k) ?? [];
    c.add(m.commodity, m.amount);
    t.mem.set(k, c);
    c.getmonetary(m.commodity, false)
}

/// A cell's calc-fn applied to `split` (TE:1260-1348); `tr` is whether the row is the transaction's own.
private fn apply_calc_fn(t, cell, split, tr) {
    let f = cell.calc_fn;
    if type_of(f) == "Fn" { return if "legacy" in cell { f.call(split) } else { f.call(split, tr) }; }
    let account = split.account;
    let date = split.transaction.datePosted;
    let original = f.starts_with("original-");
    let amt = split_amount(split);
    let amount = monetary(account.commodity, amt);
    if !original { amount = exchange_at(t, amount, row_currency(t, account), date); }
    let base = if original { 1 } else { 0 };
    switch f.sub_string(if original { 9 } else { 10 }) {
        "amount" => amount,
        "debit" => if amt > 0 { amount },
        "credit" => if !(amt > 0) { -amount },
        "balance" => {
            let bal = monetary(account.commodity, split.balance);
            if original { bal } else { exchange_at(t, bal, row_currency(t, account), dates::time64_canonical_day_time(t.ctx, date)) }
        }
        "running-total" => if tr { running_add(t, base, amount) },
        "running-prime" => if tr { running_add(t, 2 + base, amount) },
        "running-sec" => if tr { running_add(t, 4 + base, amount) },
        _ => throw `unknown calc-fn ${f}`,
    }
}

/// `add-split-row` (TE:1868-1901): the row of `split`, or `()` when only subtotals are shown. A cell is negated when
/// its column reverses and the account does, and links to the split under Enable Links when it is subtotalled or no
/// cell is.
private fn split_row(t, split, alternate, tr) {
    if t.p.subtotals_only { return (); }
    let cells = nulls(t.indent_level);
    for c in t.left { cells.push(left_cell(t.p, c.kind, split, tr)); }
    let reversible = acc_reverse(t, split.account);
    for c in t.cells {
        let m = apply_calc_fn(t, c, split, tr);
        if m == () { cells.push(()); continue; }
        let cell = #{ monetary: if c.reverse_column && reversible { -m } else { m } };
        if t.p.links && (c.subtotal || !t.any_subtotal) { cell = html_utils::html_split_anchor(split, cell); }
        cells.push(cell);
    }
    #{ role: "data", alternate: alternate, cells: cells }
}

/// `render-summary` (TE:1816-1850): the label of `split`'s group at `level` ("primary" or "secondary"): the date
/// group, the account (with its code and description as the Sorting options ask), or the key's own renderer.
private fn render_summary(t, split, level) {
    let p = t.p;
    let sortkey = if level == "primary" { p.primary_key } else { p.secondary_key };
    if sortkey in date_sorting_types() {
        let date_subtotal = if level == "primary" { p.primary_date_subtotal } else { p.secondary_date_subtotal };
        return date_subtotal_renderer_fn(t.ctx, date_subtotal, split);
    }
    if !(sortkey in account_sorting_types()) { return sortkey_renderer_fn(p.split_action, sortkey, split); }
    let account = sortkey_renderer_fn(p.split_action, sortkey, split);
    let name = account_namestring(account, p.sort_account_code, true, p.sort_account_full_name);
    // A split transaction's other account is none, which has no description: GnuCash fails there (TE:1828-1829).
    let desc = if account == () { "" } else { account.description };
    if p.sort_account_description && desc != "" { `${name}: ${desc}` } else { name }
}

/// The label cells of a subheading or subtotal row (TE:1626-1630, :1700-1711): the label across the indent left
/// over and the left columns, or with Table for Exporting the label then empty cells.
private fn label_cells(t, label, right_indent) {
    let n = right_indent + t.width_left;
    if t.p.export_table { return [#{ text: label }] + nulls(n - 1); }
    [if n > 1 { #{ text: label, span: n } } else { #{ text: label } }]
}

/// The cell of a subheading under calculated cell `c` (TE:1632-1664): under a running balance with an account key,
/// the account's "Balance b/f", brought to the split's day at noon; with informal headers, two amount columns and an
/// account key, the column's friendly heading for the account.
private fn subheading_cell(t, c, split, sortkey) {
    let p = t.p;
    let f = c.friendly_heading_fn;
    let acc = split.account;
    let account_key = sortkey in account_sorting_types();
    if f == () { return (); }
    if f == "bal-bf" || f == "original-bal-bf" {
        if !account_key { return (); }
        let bal = monetary(acc.commodity, acc.balance_as_of(p.begindate));
        if f == "bal-bf" {
            bal = exchange_at(t, bal, row_currency(t, acc), dates::time64_canonical_day_time(t.ctx, split.transaction.datePosted));
        }
        return #{ monetary: if acc_reverse(t, acc) { -bal } else { bal } };
    }
    if !(p.informal_headers && p.amount_double && sortkey in sortkey_informal_headers()) { return (); }
    text_cell(switch f { "debit" => acc.debitLabel, "credit" => acc.creditLabel, "" => "", _ => f.call(acc) })
}

/// `add-subheading` (TE:1597-1665): the heading of the group `split` starts at `level`, or `()` when only subtotals
/// are shown; "Balance b/f" follows an account's name when a column brings its balance forward.
private fn subheading(t, label, split, level) {
    let p = t.p;
    if p.subtotals_only { return (); }
    let sortkey = if level == "primary" { p.primary_key } else { p.secondary_key };
    let bal_bf = false;
    for c in t.cells { if c.friendly_heading_fn == "bal-bf" { bal_bf = true; } }
    if bal_bf && sortkey in account_sorting_types() { label += ": Balance b/f"; }
    let left_indent = if level == "primary" { 0 } else { t.primary_indent };
    let cells = nulls(left_indent) + label_cells(t, label, t.indent_level - left_indent);
    for c in t.cells { cells.push(subheading_cell(t, c, split, sortkey)); }
    #{ role: "heading", cells: cells }
}

/// `subtotal-label` (TE:1063-1071): "Total For " in front, unless only subtotals are shown and `level` is the one
/// shown in detail, as the Secondary Subtotal option says.
private fn subtotal_label(t, label, level) {
    let detail = t.subtotals_only_option
        && switch level { "secondary" => t.sec_subtotal_option, "primary" => !t.sec_subtotal_option, _ => false };
    if detail { label } else { `Total For ${label}` }
}

/// The commodities of the collectors, each column's in its collector's order, duplicates dropped (TE:1688-1693).
private fn collector_commodities(colls) {
    let list = [];
    for coll in colls { for pair in coll { if !list.contains(pair[0]) { list.push(pair[0]); } } }
    list
}

/// The total of `commodity` in `coll`, `()` when it has none (`retrieve-commodity`, TE:1695-1698).
private fn retrieve(coll, commodity) {
    for pair in coll { if pair[0] == commodity { return pair[1]; } }
    ()
}

/// `data-columns` (TE:1713-1765): a subtotal row's cells for `commodity`. A start-dual column is held and netted
/// against the next: the difference shows in the debit column when positive, else in the credit column.
private fn data_columns(t, colls, commodity) {
    let cells = [];
    let last = ();
    let merging = false;
    for (coll, j) in colls {
        let this_column = retrieve(coll, commodity);
        if t.cells[j].start_dual_column {
            merging = true;
            last = this_column;
        } else if merging {
            let sum = if last == () && this_column == () { () } else { (last ?? rational(0)) - (this_column ?? rational(0)) };
            let cell = if sum == () { () } else { #{ monetary: monetary(commodity, sum.abs()) } };
            cells += if sum != () && sum > 0 { [cell, ()] } else { [(), cell] };
            merging = false;
            last = ();
        } else {
            cells.push(if this_column == () { () } else { #{ monetary: monetary(commodity, this_column) } });
        }
    }
    cells
}

/// `add-subtotal-row` (TE:1680-1791): one row per commodity of the collectors, the label on the first.
private fn subtotal_rows(t, label, colls, role, level) {
    let left_indent = switch level { "total" => 0, "primary" => t.primary_indent, _ => t.indent_level };
    let rows = [];
    for commodity in collector_commodities(colls) {
        let cells = nulls(left_indent) + label_cells(t, label, t.indent_level - left_indent) + data_columns(t, colls, commodity);
        rows.push(#{ role: role, cells: cells });
        label = "";
    }
    rows
}

/// `n` empty commodity collectors.
private fn new_collectors(n) {
    let colls = [];
    for i in 0..n { colls.push([]); }
    colls
}

/// The subtotalled cells of a transaction's own row, added to each of `colls` (TE:1903-1917).
private fn add_to_collectors(t, colls, split) {
    for (c, j) in t.cells {
        if !c.subtotal { continue; }
        let v = apply_calc_fn(t, c, split, true);
        if v == () { continue; }
        let coll = colls[j];
        coll.add(v.commodity, v.amount);
        colls[j] = coll;
    }
    colls
}

/// The table's columns: the indent, the left columns, the calculated cells (TE:1929-1933).
private fn table_columns(t) {
    let columns = [];
    for i in 0..t.indent_level { columns.push(#{ label: "", role: "label" }); }
    for c in t.left { columns.push(#{ label: c.heading, role: c.role }); }
    for c in t.cells { columns.push(#{ label: c.heading, role: "amount" }); }
    columns
}

/// The CSV rows of `make-split-table` (TE:2052-2063): each cell's heading and its grand totals; with a dual column,
/// GnuCash's reason there are none.
private fn csv_list(t, total) {
    let csvlist = [];
    for (c, j) in t.cells {
        if c.start_dual_column { return "CSV disabled for double column amounts"; }
        let row = [c.heading];
        for pair in total[j] { row.push(pair[1]); }
        csvlist.push(row);
    }
    csvlist
}

/// `make-split-table` (TE:1052-2063): the table of `splits` and its CSV rows. A group ends where the next split's
/// split-sortvalue differs (`equal?`); its subtotal rows come first, then the next group's subheadings and a reset of
/// its running totals. Returns `#{ table, csvlist }`.
private fn make_split_table(book, opts, ctx, p, splits, hooks) {
    let t = table_env(book, opts, ctx, p, hooks);
    let primary = new_collectors(t.cells.len());
    let secondary = new_collectors(t.cells.len());
    let total = new_collectors(t.cells.len());
    let pk = p.primary_key_split_sortvalue;
    let sk = p.secondary_key_split_sortvalue;
    let rows = [];
    if pk != () { rows.push(subheading(t, render_summary(t, splits[0], "primary"), splits[0], "primary")); }
    if sk != () { rows.push(subheading(t, render_summary(t, splits[0], "secondary"), splits[0], "secondary")); }
    for (current, i) in splits {
        let next = if i + 1 < splits.len() { splits[i + 1] } else { () };
        // Shaded by split, odd ones and multi-line rows plain (TE:1967-1971).
        rows.push(split_row(t, current, !(i % 2 == 0 || p.multiline), true));
        primary = add_to_collectors(t, primary, current);
        secondary = add_to_collectors(t, secondary, current);
        total = add_to_collectors(t, total, current);
        if p.multiline {
            for other in current.transaction.splits() { if other != current { rows.push(split_row(t, other, true, false)); } }
        }
        let primary_ends = pk != () && (next == () || apply_split_sortvalue(ctx, pk, current) != apply_split_sortvalue(ctx, pk, next));
        let secondary_ends = sk != ()
            && (primary_ends || next == () || apply_split_sortvalue(ctx, sk, current) != apply_split_sortvalue(ctx, sk, next));
        if secondary_ends {
            rows += subtotal_rows(t, subtotal_label(t, render_summary(t, current, "secondary"), "secondary"), secondary,
                                  "subtotal", "secondary");
            secondary = new_collectors(t.cells.len());
        }
        if primary_ends {
            rows += subtotal_rows(t, subtotal_label(t, render_summary(t, current, "primary"), "primary"), primary, "subtotal",
                                  "primary");
            primary = new_collectors(t.cells.len());
        }
        if next != () && primary_ends {
            rows.push(subheading(t, render_summary(t, next, "primary"), next, "primary"));
            if sk != () { rows.push(subheading(t, render_summary(t, next, "secondary"), next, "secondary")); }
            for k in [2, 3, 4, 5] { t.mem.set(k, []); }
        } else if next != () && secondary_ends {
            rows.push(subheading(t, render_summary(t, next, "secondary"), next, "secondary"));
            for k in [4, 5] { t.mem.set(k, []); }
        }
    }
    let width = t.indent_level + t.width_left + t.cells.len();
    if p.grand_total {
        rows.push(#{ role: "rule", cells: nulls(width) });
        rows += subtotal_rows(t, "Grand Total", total, "grandTotal", "total");
    }
    let shown = [];
    for r in rows { if r != () { shown.push(r); } }
    #{ table: #{ kind: "table", columns: table_columns(t), rows: shown }, csvlist: csv_list(t, total) }
}

/// `gnc:trep-renderer` (TE:2179-2641): the document. A warning when no account or expression allows a query (then
/// the caller's `empty_report_message`) or no split matched; otherwise the dates, then the table. Under
/// `ctx.export_type()` "csv", the export rows (the dates in ISO form, then `make-split-table`'s) or why there are
/// none (TE:2524, :2605, :2627-2641).
fn trep_renderer(book, opts, ctx, hooks) {
    let r = trep_splits(book, opts, ctx, hooks);
    let blocks = [];
    let export_rows = ();
    let export_error = r.export_error;
    if r.warning != () {
        blocks.push(r.warning);
        let message = hook(hooks, "empty_report_message");
        if r.splits == () && message != () {
            blocks.push(if type_of(message) == "string" { #{ kind: "text", text: message } } else { message });
        }
    } else {
        let p = r.parameters;
        let st = make_split_table(book, opts, ctx, p, r.splits, hooks);
        blocks.push(#{ kind: "text", role: "heading", text: ctx.print_date_interval(p.begindate, p.enddate) });
        blocks.push(st.table);
        if type_of(st.csvlist) == "string" {
            export_error = st.csvlist;
        } else {
            export_rows = [["from", ctx.print_date_iso(p.begindate)], ["to", ctx.print_date_iso(p.enddate)]] + st.csvlist;
        }
    }
    let doc = #{ title: r.report_title, sections: [#{ blocks: blocks }] };
    if ctx.export_type() == "csv" {
        if export_error != () { doc.exportError = export_error; } else { doc["export"] = export_rows; }
    }
    doc
}
