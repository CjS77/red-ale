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
"""Build the books of `5.17:gnucash/report/reports/standard/test/test-standard-category-report.scm` (licence notice
above, kept from that file):

- `reports/category-daily.gnucash`: `single-txn-test` (`:81-94`). `asset-test` (`:263-275`) builds the same accounts
  and transactions, so it runs on this book too.
- `reports/category-average.gnucash`: `single-txn-test-average` (`:117-136`).
- `reports/category-multi.gnucash`: `multi-acct-test` (`:211-229`).
- `reports/category-liability.gnucash`: `liability-test` (`:302-318`).

GnuCash clears the session after the first two tests only (`:72-73`, `:115`, `:209`), so it runs
`multi-acct-test`, `asset-test` and `liability-test` in one book. Each report selects its own accounts, so the
separate books here give the same figures.

`single-txn-test`, `multi-acct-test` and `asset-test` date their transactions in the current month. For the pinned
now, 2024-03-15 12:00:00 UTC, that is March 2024: one transaction per day from 2024-03-31 back to 2024-03-01, each
of the day of the month in dollars, and the "start/end of this month" options read 2024-03-01 00:00:00 to
2024-03-31 23:59:59 UTC.

Usage: TZ=UTC PYTHONPATH=<5.17 site-packages> scripts/report-fixtures/category_report.py [out-dir]
"""
from fractions import Fraction

from engine_extras import (ASSET, CREDIT, EXPENSE, INCOME, DAY, check_book, create_test_env, dmy2time64,
                           end_this_month, env_create_account_structure, env_create_daily_transactions,
                           env_create_root_account, env_create_transaction, env_expense_account_structure,
                           incdate_days, list_flatten, mnemonic_to_commodity, new_book, out_dir, start_this_month)


def root_accounts(env, *types):
    usd = mnemonic_to_commodity("USD")
    return [env_create_root_account(env, t, usd) for t in types]


def single_txn_test():
    """`:81-94`."""
    env = create_test_env()
    asset, _expense, income = root_accounts(env, ASSET, EXPENSE, INCOME)
    env_create_daily_transactions(env, start_this_month(), end_this_month(), asset, income)


def single_txn_test_average():
    """`:117-136`: 52 weekly transactions from 1980-01-01 of $1.10, $2.20, ..., and $22.40 on 1980-07-01."""
    env = create_test_env()
    asset, _expense, income = root_accounts(env, ASSET, EXPENSE, INCOME)
    date, amt = dmy2time64(1, 1, 1980), Fraction(11, 10)
    for _ in range(52):
        env_create_transaction(env, date, asset, income, amt)
        date, amt = incdate_days(date, 7), amt + Fraction(11, 10)
    env_create_transaction(env, dmy2time64(1, 7, 1980), asset, income, Fraction(224, 10))


def multi_acct_test():
    """`:211-229`: every account of the expense tree, parents included (`gnc:list-flatten`), gets the daily
    transactions from Assets:Bank."""
    env = create_test_env()
    expense_accounts = env_expense_account_structure(env)
    asset_accounts = env_create_account_structure(env, ["Assets", {"type": ASSET}, ["Bank"]])
    bank = asset_accounts[1][0]
    for expense_account in list_flatten(expense_accounts):
        env_create_daily_transactions(env, start_this_month(), end_this_month(), expense_account, bank)


def liability_test():
    """`:302-318`: a $100 loan, then five months of $10 salary and $9 repayments."""
    env = create_test_env()
    asset, liability, income = root_accounts(env, ASSET, CREDIT, INCOME)
    env_create_transaction(env, dmy2time64(1, 10, 2016), asset, liability, 100)
    for month in range(1, 6):
        env_create_transaction(env, dmy2time64(1, month, 2017), asset, income, 10)
        env_create_transaction(env, dmy2time64(2, month, 2017), liability, asset, 9)


def build(out, test):
    book = new_book(out)
    test()
    book.save()


def guid(n):
    return f"a{n:031x}"


def main():
    names = ["category-daily", "category-average", "category-multi", "category-liability"]
    paths = {name: out_dir() / f"{name}.gnucash" for name in names}
    for name, test in zip(names, [single_txn_test, single_txn_test_average, multi_acct_test, liability_test]):
        build(paths[name], test)
    march = start_this_month()
    month_total = sum(range(1, 32))
    # Day N holds $N (:107-113); the asset's balance is $1.00 on day 1 and $406.00 on day 28 (:294-299).
    check_book(paths["category-daily"], [
        ("account-1", 1, march + DAY),
        ("account-1", 406, march + 28 * DAY),
        ("account-1", month_total, None),
        ("account-3", -month_total, None),
    ])
    check_book(paths["category-average"], [
        ("account-1", Fraction(11, 10) * sum(range(1, 53)) + Fraction(224, 10), None),
        ("account-3", -(Fraction(11, 10) * sum(range(1, 53)) + Fraction(224, 10)), None),
        ("account-1", Fraction(11, 10) * sum(range(1, 6)), dmy2time64(1, 2, 1980)),
    ])
    # Each of the seven expense accounts gets $N on day N, so day 2 has Auto (with its three children) $8 of $14:
    # 57.14 % (:257-259).
    check_book(paths["category-multi"], [
        ("Expenses", month_total, None),
        ("Expenses.Auto.Petrol", month_total, None),
        ("Expenses.Auto", 1, march + DAY - 1),
        ("Expenses.Auto", 3, march + 2 * DAY - 1),
        ("Assets.Bank", -7 * month_total, None),
    ])
    # The credit card owes $100.00 in January 2017 and $55.00 after the fifth repayment (:337-342).
    check_book(paths["category-liability"], [
        (guid(2), -100, dmy2time64(2, 1, 2017)),
        (guid(2), -91, dmy2time64(3, 1, 2017)),
        (guid(2), -55, None),
        (guid(1), 105, None),
        (guid(3), -50, None),
    ])


if __name__ == "__main__":
    main()
