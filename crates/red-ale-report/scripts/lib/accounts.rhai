// lib/accounts: account predicates, account lists by type and the commodities of accounts.
//
// Translated from 5.17:gnucash/report/report-utilities.scm (lines 149-257), cited below as RU, and
// `gnc-accounts-and-all-descendants` (5.17:bindings/engine.i:241-254). The header of report-utilities.scm:
//
// report-utilities.scm -- Reporting utilities
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
// Account types are the names the query API gives them ("ASSET", "BANK", ...). Names compare bytewise where
// GnuCash's `gnc:string-locale<?` uses the locale's collation (`design/reports.md` §12).

/// `gnc:account-has-shares?` (RU:149-151): true for STOCK, MUTUAL and CURRENCY accounts.
fn account_has_shares(account) {
    ["STOCK", "MUTUAL", "CURRENCY"].contains(account.type)
}

/// `gnc:account-is-stock?` (RU:154-156): true for STOCK and MUTUAL accounts.
fn account_is_stock(account) {
    ["STOCK", "MUTUAL"].contains(account.type)
}

/// `gnc:account-full-name<?` (RU:160-161): true when `a`'s full name sorts before `b`'s. GnuCash compares in the
/// locale's collation; red-ale bytewise, as everywhere (`design/reports.md` §12). Sort with
/// `accts.sort(|a, b| if accounts::account_full_name_lt(a, b) { -1 } else if accounts::account_full_name_lt(b, a) { 1 } else { 0 })`.
fn account_full_name_lt(a, b) {
    a.fullName < b.fullName
}

/// `gnc:accounts-get-children-depth` (RU:163-168): the depth of the deepest account in `accounts`' subtrees, a
/// top-level account being at 1. An error for no accounts, as `(apply max '())` is.
fn accounts_get_children_depth(accounts) {
    if accounts.is_empty() {
        throw "accounts_get_children_depth: no accounts";
    }
    let deepest = 0;
    for acct in accounts {
        let d = acct.depth + acct.tree_depth();
        if d > deepest {
            deepest = d;
        }
    }
    deepest - 1
}

/// `gnc:account-is-inc-exp?` (RU:172-174): true for INCOME and EXPENSE accounts.
fn account_is_inc_exp(account) {
    ["INCOME", "EXPENSE"].contains(account.type)
}

/// `gnc:filter-accountlist-type` (RU:178-181): the accounts of `accounts` with a type in `typelist`, in order.
fn filter_accountlist_type(typelist, accounts) {
    accounts.filter(|a| a != () && typelist.contains(a.type))
}

/// `gnc:decompose-accountlist` (RU:188-205): `accounts` by kind, each in order: a map from ASSET, LIABILITY, EQUITY,
/// INCOME, EXPENSE and TRADING to the accounts of the types each stands for. GnuCash returns an association list,
/// which reports read only with `assoc-ref`; a map reads the same.
fn decompose_accountlist(accounts) {
    #{
        ASSET: filter_accountlist_type(["ASSET", "BANK", "CASH", "CHECKING", "SAVINGS", "MONEYMRKT", "RECEIVABLE", "STOCK",
                                        "MUTUAL", "CURRENCY"], accounts),
        LIABILITY: filter_accountlist_type(["LIABILITY", "PAYABLE", "CREDIT", "CREDITLINE"], accounts),
        EQUITY: filter_accountlist_type(["EQUITY"], accounts),
        INCOME: filter_accountlist_type(["INCOME"], accounts),
        EXPENSE: filter_accountlist_type(["EXPENSE"], accounts),
        TRADING: filter_accountlist_type(["TRADING"], accounts),
    }
}

/// `gnc:accounts-get-commodities` (RU:235-240): the commodities of `accounts` but `exclude_commodity` (`()` for
/// none), each once. GnuCash builds the list from the end, so each commodity stands where it last appears.
fn accounts_get_commodities(accounts, exclude_commodity) {
    let accum = [];
    let i = accounts.len() - 1;
    while i >= 0 {
        let comm = accounts[i].commodity;
        if !(exclude_commodity != () && comm == exclude_commodity) && !accum.contains(comm) {
            accum.insert(0, comm);
        }
        i -= 1;
    }
    accum
}

/// `gnc:accounts-get-commodities-sorted` (RU:242-251): those commodities sorted by namespace, then by nice symbol,
/// both stable sorts.
fn accounts_get_commodities_sorted(accounts, exclude_commodity) {
    let commodities = accounts_get_commodities(accounts, exclude_commodity);
    commodities.sort(|a, b| compare(a.symbol, b.symbol));
    commodities.sort(|a, b| compare(a.namespace, b.namespace));
    commodities
}

/// -1, 0 or 1 as `a` sorts before, with or after `b`, bytewise.
private fn compare(a, b) {
    if a < b { -1 } else if a > b { 1 } else { 0 }
}

/// `gnc:get-current-account-tree-depth` (RU:255-257): the depth of the account tree, the root counting as 1.
fn get_current_account_tree_depth(book) {
    book.root().tree_depth()
}

/// `gnc-accounts-and-all-descendants` (5.17:bindings/engine.i:241-254): `accounts` and all their descendants.
/// GnuCash returns them in a hash set's order; red-ale gives each account followed by its descendants in display
/// order, and drops repeats (`design/reports.md` §12).
fn accounts_and_all_descendants(accounts) {
    let seen = #{};
    let out = [];
    for a in accounts {
        for d in [a] + a.descendants_sorted() {
            if !(d.guid in seen) {
                seen[d.guid] = true;
                out.push(d);
            }
        }
    }
    out
}
