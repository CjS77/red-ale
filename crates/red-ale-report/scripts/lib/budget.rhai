// lib/budget: Budget amounts summed over periods and rolled up from sub-accounts, and balances at a budget's start.
//
// Translated from 5.17:gnucash/report/report-utilities.scm (lines 727-856), cited below as RU. Its header:
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
// A budget is a `Budget` of the query API; periods count from 0, and a start or end period of `()` or false is #f,
// the budget's first or last. The roll-up converts children's amounts through the price database, so it takes the
// Book first, which GnuCash reads from the current book.

import "lib/collectors" as collectors;
import "lib/balances" as balances;

/// `(or v default)`.
private fn value_or(v, default_value) {
    if v == () || v == false { default_value } else { v }
}

/// `gnc:budget-get-start-date` (RU:727-729): the start of period 0.
fn budget_get_start_date(budget) {
    budget.period_start_date(0)
}

/// `gnc:budget-get-end-date` (RU:731-733): the end of the last period.
fn budget_get_end_date(budget) {
    budget.period_end_date(budget.numPeriods - 1)
}

/// `gnc:budget-account-get-net` (RU:744-760): a collector of `account`'s own Budget amounts, unset ones as 0, for
/// periods `start_period` (inclusive) to `end_period` (exclusive). Sub-accounts are not rolled up.
fn budget_account_get_net(budget, account, start_period, end_period) {
    let period = value_or(start_period, 0);
    let maxperiod = value_or(end_period, budget.numPeriods);
    let net = collectors::make_commodity_collector();
    while period < maxperiod {
        net.add(account.commodity, budget.account_period_value(account, period));
        period += 1;
    }
    net
}

/// `gnc:budget-accountlist-get-net` (RU:762-776): `budget_account_get_net` of each account, merged.
fn budget_accountlist_get_net(budget, accountlist, start_period, end_period) {
    let net = collectors::make_commodity_collector();
    for account in accountlist {
        net.merge(budget_account_get_net(budget, account, start_period, end_period));
    }
    net
}

/// `gnc:budget-account-get-initial-balance` (RU:778-786): `account`'s balance at the budget's start, without its
/// sub-accounts and not sign-adjusted.
fn budget_account_get_initial_balance(budget, account) {
    balances::account_get_comm_balance_at_date(account, budget_get_start_date(budget), false)
}

/// `gnc:budget-accountlist-get-initial-balance` (RU:788-794): the same of each account, merged.
fn budget_accountlist_get_initial_balance(budget, accountlist) {
    let net = collectors::make_commodity_collector();
    for account in accountlist {
        net.merge(budget_account_get_initial_balance(budget, account));
    }
    net
}

/// `budget-account-sum` (RU:806-817): the children's rolled-up values in `period`, each converted to `currency` at
/// the nearest price to the period's start, summed exactly.
private fn budget_account_sum(book, budget, children, period, currency) {
    let start = budget.period_start_date(period);
    let sum = rational(0);
    for child in children {
        let value = get_account_period_rolledup_budget_value(book, budget, child, period);
        sum += book.price_convert(monetary(child.commodity, value), currency, start, "nearest").amount;
    }
    sum
}

/// `gnc:get-account-period-rolledup-budget-value` (RU:819-838): `acct`'s Budget amount in `period` when it is set;
/// else, with children, the sum of theirs, rolled up in turn; else 0.
fn get_account_period_rolledup_budget_value(book, budget, acct, period) {
    let children = acct.children();
    if budget.is_account_period_value_set(acct, period) {
        budget.account_period_value(acct, period)
    } else if !children.is_empty() {
        budget_account_sum(book, budget, children, period, acct.commodity)
    } else {
        rational(0)
    }
}

/// `gnc:budget-account-get-rolledup-net` (RU:840-856): the rolled-up values of `account` for periods
/// `start_period` to `end_period`, both inclusive.
fn budget_account_get_rolledup_net(book, budget, account, start_period, end_period) {
    let end = value_or(end_period, budget.numPeriods - 1);
    let period = value_or(start_period, 0);
    let sum = rational(0);
    while period <= end {
        sum += get_account_period_rolledup_budget_value(book, budget, account, period);
        period += 1;
    }
    sum
}
