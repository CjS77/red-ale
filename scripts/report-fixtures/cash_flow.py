#!/usr/bin/env python3
# This program is free software; you can redistribute it and/or
# modify it under the terms of the GNU General Public License as
# published by the Free Software Foundation; either version 2 of
# the License, or (at your option) any later version.
#
# This program is distributed in the hope that it will be useful,
# but WITHOUT ANY WARRANTY; without even the implied warranty of
# MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
# GNU General Public License for more details.
#
# You should have received a copy of the GNU General Public License
# along with this program; if not, contact:
#
# Free Software Foundation           Voice:  +1-617-542-5942
# 51 Franklin Street, Fifth Floor    Fax:    +1-617-542-2652
# Boston, MA  02110-1301,  USA       gnu@gnu.org
"""Build `reports/cash-flow.gnucash` and `reports/cashflow-barchart.gnucash`: the books of
`5.17:gnucash/report/reports/standard/test/test-cash-flow.scm` (no licence notice of its own; part of GnuCash and
cited under GnuCash's licence, GPL-2.0-or-later, 5.17:LICENSE) and
`5.17:gnucash/report/reports/standard/test/test-cashflow-barchart.scm` (licence notice above, kept from that file).

Each test runs three groups that never clear the session, so each book holds three groups. Every group makes its own
environment and its own "Root" account at the top level, so names repeat across groups; the reports select accounts
by GUID (listed in the books' `expected.json`).

Dates are relative to now. Under the pinned now, 2024-03-15 12:00:00 UTC, with TZ=UTC:

- test-cash-flow.scm (`:36-38`, and the same in each group): `today` is `(gnc-localtime (current-time))`, 2024-03-15;
  `to-date-t64` is `gnc-dmy2time64-end` of today, 2024-03-15 23:59:59 UTC; `from-date-t64` is `to-date-t64` less
  86400 s (`NDayDelta`, `:21-24`), 2024-03-14 23:59:59 UTC. Every transaction is made at `to-date-t64`, and
  `env-create-transaction` posts it on that local day (`test-engine-extras.scm:118-142`): 2024-03-15, 10:59 UTC.
- test-cashflow-barchart.scm (`:69-71`, and the same in each group): `date-0` is `gnc:get-start-this-month`,
  2024-03-01 00:00:00 UTC (`5.17:bindings/guile/date-utilities.scm:589-596`); `date-1` and `date-2` are
  `gnc:time64-next-day` of the one before, `incdate` by a day (`:498-499`), 2024-03-02 and 2024-03-03 00:00:00 UTC.
  Transactions are posted on 2024-03-02 and 2024-03-03.

Usage: TZ=UTC PYTHONPATH=<5.17 site-packages> scripts/report-fixtures/cash_flow.py [out-dir]
"""
import calendar

from engine_extras import (ASSET, DAY, EXPENSE, INCOME, NOW, check_book, create_test_env,
                           env_create_account_structure_alist, env_create_transaction, incdate_days, localtime,
                           new_book, out_dir, start_this_month)

# test-cash-flow.scm:36-38: gnc-dmy2time64-end of today, then NDayDelta by one day
_TODAY = localtime(NOW)
TO_DATE = calendar.timegm((_TODAY.tm_year, _TODAY.tm_mon, _TODAY.tm_mday, 23, 59, 59))
FROM_DATE = TO_DATE - DAY

# test-cashflow-barchart.scm:69-71
DATE_0 = start_this_month()
DATE_1 = incdate_days(DATE_0, 1)
DATE_2 = incdate_days(DATE_1, 1)


def cash_flow_structure(env):
    """`test-cash-flow.scm:14-19`."""
    return env_create_account_structure_alist(
        env, ["Root", {"type": ASSET}, ["Asset", ["Bank"], ["Wallet"]], ["Expenses", {"type": EXPENSE}]])


def barchart_structure(env):
    """`test-cashflow-barchart.scm:51-57`."""
    return env_create_account_structure_alist(
        env, ["Root", {"type": ASSET}, ["Asset", ["Bank"], ["Wallet"]], ["Expenses", {"type": EXPENSE}],
              ["Income", {"type": INCOME}]])


def one_tx_in():
    """`test-one-tx-in-cash-flow` (`test-cash-flow.scm:30-41`): Bank +100 from Expenses."""
    env = create_test_env()
    a = cash_flow_structure(env)
    env_create_transaction(env, TO_DATE, a["Bank"], a["Expenses"], 100)


def one_tx_skip():
    """`test-one-tx-skip-cash-flow` (`:73-84`): Bank +100 from Wallet."""
    env = create_test_env()
    a = cash_flow_structure(env)
    env_create_transaction(env, TO_DATE, a["Bank"], a["Wallet"], 100)


def both_way():
    """`test-both-way-cash-flow` (`:106-118`): Bank +100 from Expenses, then Expenses +50 from Bank."""
    env = create_test_env()
    a = cash_flow_structure(env)
    env_create_transaction(env, TO_DATE, a["Bank"], a["Expenses"], 100)
    env_create_transaction(env, TO_DATE, a["Expenses"], a["Bank"], 50)


def in_txn():
    """`test-in-txn` (`test-cashflow-barchart.scm:60-73`)."""
    env = create_test_env()
    a = barchart_structure(env)
    env_create_transaction(env, DATE_1, a["Bank"], a["Income"], 1)
    env_create_transaction(env, DATE_2, a["Wallet"], a["Income"], 5)


def out_txn():
    """`test-out-txn` (`:103-120`)."""
    env = create_test_env()
    a = barchart_structure(env)
    env_create_transaction(env, DATE_1, a["Bank"], a["Income"], 100)
    env_create_transaction(env, DATE_1, a["Expenses"], a["Bank"], 1)
    env_create_transaction(env, DATE_2, a["Wallet"], a["Income"], 100)
    env_create_transaction(env, DATE_2, a["Expenses"], a["Wallet"], 5)


def null_txn():
    """`test-null-txn` (`:153-168`): `test-in-txn` with Bank +20 from Wallet on date-1."""
    env = create_test_env()
    a = barchart_structure(env)
    env_create_transaction(env, DATE_1, a["Bank"], a["Income"], 1)
    env_create_transaction(env, DATE_1, a["Bank"], a["Wallet"], 20)
    env_create_transaction(env, DATE_2, a["Wallet"], a["Income"], 5)


def build(out, groups):
    book = new_book(out)
    for group in groups:
        group()
    book.save()


def guid(n):
    return f"a{n:031x}"


def main():
    cash_flow, barchart = out_dir() / "cash-flow.gnucash", out_dir() / "cashflow-barchart.gnucash"
    build(cash_flow, [one_tx_in, one_tx_skip, both_way])
    build(barchart, [in_txn, out_txn, null_txn])
    # Five accounts a group (Root, Asset, Bank, Wallet, Expenses). Money into Bank from Expenses $100 (:55-64);
    # Bank from Wallet $100, not counted (:95-102); $100 in from and $50 out to Expenses (:137-144). Nothing is
    # posted before the period starts.
    check_book(cash_flow, [
        (guid(3), 100, None), (guid(5), -100, None), (guid(3), 0, FROM_DATE),
        (guid(8), 100, None), (guid(9), -100, None),
        (guid(13), 50, None), (guid(15), -50, None),
    ])
    # Six accounts a group (Root, Asset, Bank, Wallet, Expenses, Income). in-txn: Bank $1 on date-1 and Wallet $5
    # on date-2, both from Income (:91-93); out-txn: $100 in and $1 out of Bank on date-1, $100 in and $5 out of
    # Wallet on date-2 (:139-141); null-txn: in-txn with $20 from Wallet to Bank on date-1.
    end_1, end_2 = DATE_1 + DAY - 1, DATE_2 + DAY - 1
    check_book(barchart, [
        (guid(3), 0, DATE_1 - 1), (guid(3), 1, end_1), (guid(4), 5, end_2), (guid(6), -6, None),
        (guid(9), 99, end_1), (guid(10), 95, end_2), (guid(11), 6, None), (guid(12), -200, None),
        (guid(15), 21, end_1), (guid(16), -20, end_1), (guid(16), -15, end_2), (guid(18), -6, None),
    ])


if __name__ == "__main__":
    main()
