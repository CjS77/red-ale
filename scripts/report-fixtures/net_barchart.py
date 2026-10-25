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
"""Build `reports/net-days.gnucash`: the book of
`5.17:gnucash/report/reports/standard/test/test-standard-net-barchart.scm` (licence notice above, kept from that
file).

The test's five groups (`single-txn-test` `:58-101`, `two-txn-test` `:103-153`, `two-txn-test-2` `:156-203`,
`two-txn-test-income` `:205-252`, `closing-test` `:255-311`) never clear the session, so they share one book. Each
makes its own environment, so each group's accounts are again "account-1", "account-2", ... at the top level; the
reports select them by account, which here means by GUID (listed in `net-days.expected.json`).
`test-standard-net-linechart.scm` builds the first three groups the same way (`:55-170`), so its book is this one's
first three groups and its checks hold here.

Dates are relative to now: `gnc:get-start-this-month` and the following days. For the pinned now, 2024-03-15
12:00:00 UTC, date-0 is 2024-03-01 00:00:00 UTC and date-1, date-2, date-3 are 2024-03-02, -03 and -04.

Usage: TZ=UTC PYTHONPATH=<5.17 site-packages> scripts/report-fixtures/net_barchart.py [out-dir]
"""
from fractions import Fraction

from engine_extras import (ASSET, DAY, EQUITY, EXPENSE, INCOME, LIABILITY, check_book, create_test_env,
                           env_create_root_account, env_create_transaction, incdate_days, mnemonic_to_commodity,
                           new_book, out_dir, start_this_month)

# gnc:get-start-this-month, then gnc:time64-next-day (date-utilities.scm:498-499) three times
DATE_0 = start_this_month()
DATE_1 = incdate_days(DATE_0, 1)
DATE_2 = incdate_days(DATE_1, 1)
DATE_3 = incdate_days(DATE_2, 1)


def root_accounts(env, *types):
    usd = mnemonic_to_commodity("USD")
    return [env_create_root_account(env, t, usd) for t in types]


def single_txn_test():
    """`:58-72`."""
    env = create_test_env()
    asset, _expense, income = root_accounts(env, ASSET, EXPENSE, INCOME)
    env_create_transaction(env, DATE_0, income, asset, Fraction(-1))


def two_txn_test():
    """`:103-125`."""
    env = create_test_env()
    asset, _expense, income = root_accounts(env, ASSET, EXPENSE, INCOME)
    env_create_transaction(env, DATE_1, income, asset, Fraction(-1))
    env_create_transaction(env, DATE_2, income, asset, Fraction(-5))


def four_txn_test():
    """`two-txn-test-2` (`:156-174`) and `two-txn-test-income` (`:205-223`) build the same accounts and
    transactions."""
    env = create_test_env()
    asset, liability, expense, income = root_accounts(env, ASSET, LIABILITY, EXPENSE, INCOME)
    env_create_transaction(env, DATE_1, income, asset, Fraction(-1))
    env_create_transaction(env, DATE_1, expense, liability, Fraction(-1))
    env_create_transaction(env, DATE_2, income, asset, Fraction(-5))
    env_create_transaction(env, DATE_2, expense, liability, Fraction(-5))


def closing_test():
    """`:255-279`."""
    env = create_test_env()
    asset, _liability, _expense, income, equity = root_accounts(env, ASSET, LIABILITY, EXPENSE, INCOME, EQUITY)
    env_create_transaction(env, DATE_1, income, asset, Fraction(-1))
    env_create_transaction(env, DATE_2, income, asset, Fraction(-2))
    env_create_transaction(env, DATE_3, income, asset, Fraction(-3))
    env_create_transaction(env, DATE_2, asset, equity, 300).SetIsClosingTxn(True)


def main():
    out = out_dir() / "net-days.gnucash"
    book = new_book(out)
    single_txn_test()
    two_txn_test()
    four_txn_test()
    four_txn_test()
    closing_test()
    book.save()

    def end_of(day):
        return day + DAY - 1

    def guid(n):
        return f"a{n:031x}"

    # Each group's asset account at the end of each day: $1.00 (:86-88); $0.00, $1.00, $6.00 (:137-145, :187-195).
    # Income per day: $0.00, $1.00, $5.00 (:236-244) and $0.00, $1.00, $2.00, $3.00 (:292-303).
    check_book(out, [
        (guid(1), 1, end_of(DATE_0)),
        (guid(4), 0, end_of(DATE_0)),
        (guid(4), 1, end_of(DATE_1)),
        (guid(4), 6, end_of(DATE_2)),
        (guid(7), 6, end_of(DATE_2)),
        (guid(8), 6, end_of(DATE_2)),
        (guid(9), -6, end_of(DATE_2)),
        (guid(10), -6, end_of(DATE_2)),
        (guid(14), -6, end_of(DATE_2)),
        (guid(15), 1 + 2 + 3 + 300, end_of(DATE_3)),
        (guid(18), -6, end_of(DATE_3)),
        (guid(18), -3, end_of(DATE_2)),
        (guid(19), -300, None),
    ])


if __name__ == "__main__":
    main()
