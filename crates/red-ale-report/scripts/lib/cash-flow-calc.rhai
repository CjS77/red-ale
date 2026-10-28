// lib/cash-flow-calc: the money into and out of a set of accounts over a period, shared by the Cash Flow and the
// Cash Flow Barchart.
//
// Translated from 5.17:gnucash/report/reports/cash-flow-calc.scm, cited below as CFC. Its header:
//
// cash-flow-calc.scm: Cash Flow in-out calculation.
//
// copyright 2015 Peter Broadberry
// copyright 2019 Christopher Lam
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

import "lib/balances" as balances;
import "lib/collectors" as collectors;
import "lib/exchange" as exchange;

/// `cash-flow-calc-money-in-out` (CFC:33-106). `settings` is a map of:
///
/// - `book`: the Book;
/// - `accounts`: the selected accounts;
/// - `from_date`, `to_date`: the period, both ends included;
/// - `report_currency`: the commodity every figure is added under;
/// - `include_trading_accounts`: GnuCash's option, which works backwards (CFC:68-70): when true, splits of TRADING
///   accounts are skipped; when false, they count. Kept as 5.17 has it (`design/reports.md` §14);
/// - `time_exchange_fn`: an exchange function over time of lib/exchange (`case_exchange_time_fn`), or a closure
///   `|foreign, domestic, date|` giving a Monetary. Each split's absolute value goes through it in the
///   transaction's currency at the transaction's posted date, as both callers' `to-report-currency` does
///   (`standard/cash-flow.scm:228-232`, `standard/cashflow-barchart.scm:197-201`).
///
/// Every split of each transaction that has a split in `accounts` within the period counts once, unless its account
/// is one of `accounts` (or a skipped TRADING account): a negative value is money in from its account, a positive
/// one money out to it, zero neither. The result is a map of `money_in_accounts` and `money_out_accounts` (the
/// accounts, most recently first seen first, as GnuCash's alists), `money_in_alist` and `money_out_alist` (`[account,
/// collector]` pairs in the same order) and the total collectors `money_in_collector` and `money_out_collector`. All
/// amounts are positive and nothing is rounded.
///
/// GnuCash converts every split before testing it (CFC:59-63); red-ale converts only the splits it counts, which
/// gives the same figures. A split with no account is reported in the report log, where GnuCash prints the line to
/// standard output (CFC:65-67).
fn cash_flow_calc_money_in_out(settings) {
    let accounts = settings.accounts;
    let report_currency = settings.report_currency;
    let skip_trading = settings.include_trading_accounts;
    let money_in = [];
    let money_in_collector = collectors::make_commodity_collector();
    let money_out = [];
    let money_out_collector = collectors::make_commodity_collector();
    let splits_seen = #{};

    for split in balances::account_get_trans_type_splits_interval(settings.book, accounts, (), settings.from_date,
                                                                   settings.to_date) {
        let parent = split.transaction;
        for s in parent.splits() {
            let s_account = s.account;
            if s_account == () {
                print(`WARNING: s-account is NULL for split: ${s.guid}`);
                continue;
            }
            if (skip_trading && s_account.type == "TRADING") || accounts.contains(s_account) || (s.guid in splits_seen) {
                continue;
            }
            let s_value = s.value;
            if s_value == 0 {
                continue;
            }
            splits_seen[s.guid] = true;
            let s_report_value = exchange::apply_exchange_time_fn(settings.time_exchange_fn,
                                                                  monetary(parent.currency, s_value.abs()),
                                                                  report_currency, parent.datePosted).amount;
            if s_value < 0 {
                money_in_collector.add(report_currency, s_report_value);
                money_in = tally(money_in, s_account, report_currency, s_report_value);
            } else {
                money_out_collector.add(report_currency, s_report_value);
                money_out = tally(money_out, s_account, report_currency, s_report_value);
            }
        }
    }

    #{
        money_in_accounts: money_in.map(|p| p[0]),
        money_in_alist: money_in,
        money_in_collector: money_in_collector,
        money_out_accounts: money_out.map(|p| p[0]),
        money_out_alist: money_out,
        money_out_collector: money_out_collector,
    }
}

/// `alist` with `amount` of `commodity` added to `account`'s collector; an account not yet in it gets a new
/// collector at the front, as `assoc-set!` puts it (CFC:75-84).
private fn tally(alist, account, commodity, amount) {
    let i = alist.index_of(|p| p[0] == account);
    if i < 0 {
        alist.insert(0, [account, collectors::make_commodity_collector()]);
        i = 0;
    }
    let coll = alist[i][1];
    coll.add(commodity, amount);
    alist[i][1] = coll;
    alist
}
