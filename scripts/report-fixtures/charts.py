#!/usr/bin/env python3
# Rebuilt from GnuCash 5.17's gnucash/report/reports/standard/test/test-charts.scm, which carries no licence notice
# of its own; it is part of GnuCash and cited under GnuCash's licence, GPL-2.0-or-later (5.17:LICENSE).
"""Build the two books of `5.17:gnucash/report/reports/standard/test/test-charts.scm`:

- `reports/charts-net.gnucash`: book B of `test-net-chart-variant` (`:85-114`), for the monthly net chart checks
  (`:116-146`);
- `reports/charts-daily.gnucash`: book A of `test-chart-variant` (`:148-186`) with the 101 daily transactions the
  net-worth and income-expense bar chart variants add (`:192-200`), for the daily checks (`:201-238`).

In GnuCash's run, `test-chart-variant` calls `test-net-chart-variant` at its end (`:254-255`) before the session is
cleared (`:79-83`), so book B's accounts are added to book A's. Every checked report selects its accounts, all from
one of the two, so here they are separate books.

Book A dates nine transactions in the current year (`:161`, `:177-185`); for the pinned now, 2024-03-15, that is
2024, and the default "start/end of this calendar year" options (`:163-175`) read 2024-01-01 00:00:00 to
2024-12-31 23:59:59 UTC. Its checked figures (`:204-238`) use absolute 1970 dates and do not depend on now.

Usage: TZ=UTC PYTHONPATH=<5.17 site-packages> scripts/report-fixtures/charts.py [out-dir]
"""
from engine_extras import (ASSET, EQUITY, EXPENSE, INCOME, LIABILITY, NOW, check_book, create_test_env, dmy2time64,
                           env_create_account_structure_alist, env_create_transaction, env_transfer, incdate_days,
                           localtime, new_book, out_dir)

# :59-69
STRUCTURE = ["Root", {"type": ASSET},
             ["Asset", ["Bank1"], ["Bank2"], ["Bank3"], ["Bank"]],
             ["Income", {"type": INCOME}],
             ["Liability", {"type": LIABILITY}],
             ["Expenses", {"type": EXPENSE}],
             ["Equity", {"type": EQUITY}]]


def build_net(out):
    """Book B (`:90-114`)."""
    book = new_book(out)
    env = create_test_env()
    a = env_create_account_structure_alist(env, STRUCTURE)
    bank1, bank2, bank3, income, equity = a["Bank1"], a["Bank2"], a["Bank3"], a["Income"], a["Equity"]
    env_transfer(env, 12, 1, 1970, income, bank1, 10)
    env_transfer(env, 18, 1, 1970, income, bank1, 15)
    env_transfer(env, 3, 3, 1970, income, bank1, 200)
    env_transfer(env, 18, 1, 1970, income, bank2, 50)
    env_transfer(env, 18, 2, 1970, income, bank2, 50)
    env_transfer(env, 5, 5, 1969, income, bank3, 25)
    env_transfer(env, 5, 1, 1970, income, bank3, 25)
    # :112-114, a closing transaction the income-expense charts ignore
    env_transfer(env, 3, 1, 1970, equity, income, 25).SetIsClosingTxn(True)
    book.save()


def build_daily(out):
    """Book A (`:153-185`) and the daily transactions (`:195-200`)."""
    book = new_book(out)
    env = create_test_env()
    a = env_create_account_structure_alist(env, STRUCTURE)
    bank, liability, income, expense, equity = a["Bank"], a["Liability"], a["Income"], a["Expenses"], a["Equity"]
    year = localtime(NOW).tm_year
    env_transfer(env, 1, 1, year, equity, bank, 3)
    env_transfer(env, 11, 1, year, bank, expense, 8)
    env_transfer(env, 11, 2, year, income, bank, 29)
    env_transfer(env, 21, 2, year, income, bank, 10, void_reason="void")
    env_transfer(env, 22, 2, year, liability, expense, 27)
    env_transfer(env, 1, 3, year, bank, expense, 15)
    env_transfer(env, 10, 5, year, bank, expense, 10, void_reason="any")
    env_transfer(env, 10, 7, year, expense, bank, 11)
    env_transfer(env, 10, 9, year, income, bank, 8)
    # 101 daily transactions from 1970-01-01, day N moving $N from Income to Bank
    date = dmy2time64(1, 1, 1970)
    for idx in range(101):
        env_create_transaction(env, date, bank, income, idx)
        date = incdate_days(date, 1)
    book.save()


def main():
    net, daily = out_dir() / "charts-net.gnucash", out_dir() / "charts-daily.gnucash"
    build_net(net)
    build_daily(daily)
    # Book B: net worth $25.00 at 1970-01-01 and $375.00 at 1970-04-15 (:130-135).
    check_book(net, [
        ("Root.Asset.Bank3", 25, dmy2time64(2, 1, 1970)),
        ("Root.Asset.Bank1", 225, dmy2time64(16, 4, 1970)),
        ("Root.Asset.Bank2", 100, dmy2time64(16, 4, 1970)),
        ("Root.Asset.Bank3", 50, dmy2time64(16, 4, 1970)),
        ("Root.Income", -350, None),
        ("Root.Equity", -25, None),
    ])
    # Book A: Bank $105.00 at the end of 1970-01-15 and $2,701.00 at the end of 1970-03-15 (:213-217).
    check_book(daily, [
        ("Root.Asset.Bank", 105, dmy2time64(16, 1, 1970)),
        ("Root.Asset.Bank", 2701, dmy2time64(16, 3, 1970)),
        ("Root.Asset.Bank", 5050 + 28, None),
        ("Root.Income", -5050 - 37, None),
        ("Root.Expenses", 39, None),
        ("Root.Liability", -27, None),
        ("Root.Equity", -3, None),
    ])


if __name__ == "__main__":
    main()
