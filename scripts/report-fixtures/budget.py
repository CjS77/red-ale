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
"""Build `reports/budget.gnucash`: the book of `5.17:gnucash/report/reports/standard/test/test-budget.scm:66-69`
(licence notice above, kept from that file), `create-test-data` with the budget and transactions of
`gnc:create-budget-and-transactions` (`5.17:bindings/guile/test/test-engine-extras.scm:851-884`).

The Python bindings do not wrap `gnc-budget.h` (`5.17:bindings/python/gnucash_core.i:48-84`), so the budget is made
through ctypes on `libgnc-engine.so`, with the same calls in the same order. `gnc-budget-new` starts the budget's
monthly recurrence on the first of the current month (`5.17:libgnucash/engine/gnc-budget.cpp:115-117`); for the pinned
now, 2024-03-15, that is 2024-03-01, which this script sets explicitly so the book does not depend on the clock. The
six transactions are posted at each period's midpoint, so they follow from the recurrence. The budget's GUID is
`f000…0`, the first of kind `f`.

GnuCash's test sets no budget feature, and neither does this book: the bindings set none
(`design/notes/engine-budgets.md` §2.3).

Usage: TZ=UTC PYTHONPATH=<5.17 site-packages> scripts/report-fixtures/budget.py [out-dir]
"""
import ctypes
import sys

from account_summary import create_test_data
from engine_extras import check_book, create_test_env, dmy2time64, env_create_transaction, new_book, out_dir

from gnc_engine import GncNumeric, engine, ptr

VP, U, I, S = ctypes.c_void_p, ctypes.c_uint, ctypes.c_int, ctypes.c_char_p
PERIOD_MONTH = 3
WEEKEND_ADJ_NONE = 0


def declare(name, restype, *argtypes):
    f = getattr(engine, name)
    f.restype = restype
    f.argtypes = list(argtypes)
    return f


budget_new = declare("gnc_budget_new", VP, VP)
begin_edit = declare("gnc_budget_begin_edit", None, VP)
commit_edit = declare("gnc_budget_commit_edit", None, VP)
set_name = declare("gnc_budget_set_name", None, VP, S)
set_num_periods = declare("gnc_budget_set_num_periods", None, VP, U)
set_recurrence = declare("gnc_budget_set_recurrence", None, VP, VP)
set_value = declare("gnc_budget_set_account_period_value", None, VP, VP, U, GncNumeric)
set_note = declare("gnc_budget_set_account_period_note", None, VP, VP, U, S)
period_start = declare("gnc_budget_get_period_start_date", ctypes.c_int64, VP, U)
period_end = declare("gnc_budget_get_period_end_date", ctypes.c_int64, VP, U)
actual_value = declare("gnc_budget_get_account_period_actual_value", GncNumeric, VP, VP, U)
recurrence_set = declare("recurrenceSet", None, VP, ctypes.c_uint16, I, VP, I)
set_guid = declare("qof_instance_set_guid", None, VP, VP)
glib = ctypes.CDLL("libglib-2.0.so.0")
glib.g_date_new_dmy.restype = VP
glib.g_date_new_dmy.argtypes = [ctypes.c_uint8, I, ctypes.c_uint16]


def new_budget(book):
    """`gnc-budget-new`, with a fixed GUID and the recurrence it would get on the pinned now: every month from
    2024-03-01, no weekend adjustment."""
    budget = budget_new(ptr(book.book))
    set_guid(budget, ctypes.create_string_buffer(bytes.fromhex(book.guid("f")), 16))
    recurrence = ctypes.create_string_buffer(64)
    recurrence_set(recurrence, 1, PERIOD_MONTH, glib.g_date_new_dmy(1, 3, 2024), WEEKEND_ADJ_NONE)
    set_recurrence(budget, recurrence)
    return budget


def create_budget_and_transactions(book, env, accts):
    """`gnc:create-budget-and-transactions` (`test-engine-extras.scm:851-884`)."""
    budget = new_budget(book)
    bank, income, equity, expense = (ptr(accts[n]) for n in ("Bank", "Income", "Equity", "Expenses"))
    set_name(budget, b"test budget")
    begin_edit(budget)
    set_num_periods(budget, 6)
    for acct, period, amount in [(bank, 0, 20), (bank, 1, 40), (bank, 3, 60), (expense, 1, 30), (expense, 2, 20),
                                 (expense, 3, 40), (income, 0, -55), (income, 2, -65), (income, 3, -75)]:
        set_value(budget, acct, period, GncNumeric(amount, 1))
    commit_edit(budget)

    def midperiod(period):
        return (period_start(budget, period) + period_end(budget, period)) // 2

    bank, income, equity, expense = (accts[n] for n in ("Bank", "Income", "Equity", "Expenses"))
    env_create_transaction(env, midperiod(0), bank, income, 55)
    env_create_transaction(env, midperiod(2), bank, income, 67)
    env_create_transaction(env, midperiod(3), bank, income, 77)
    env_create_transaction(env, midperiod(0), expense, bank, 20)
    env_create_transaction(env, midperiod(1), expense, bank, 20)
    env_create_transaction(env, midperiod(1), income, equity, 55).SetIsClosingTxn(True)
    set_note(budget, ptr(income), 0, b"income-0 -$60")
    set_note(budget, ptr(expense), 1, b"expense-1 $25")
    return budget


def check_actuals(budget, accts, expected):
    """`gnc-budget-get-account-period-actual-value` of each account and period, against the actuals
    test-budget.scm's figures imply (`:94-108`): the no-closing change over the period."""
    failures = []
    for name, values in expected.items():
        for period, want in enumerate(values):
            got = actual_value(budget, ptr(accts[name]), period)
            if got.num * 1 != want * got.denom:
                failures.append(f"{name} period {period}: actual {got.num}/{got.denom} != {want}")
    if failures:
        sys.exit("\n".join(failures))


def main():
    out = out_dir() / "budget.gnucash"
    book = new_book(out)
    env = create_test_env()
    accts = create_test_data()
    budget = create_budget_and_transactions(book, env, accts)
    # test-budget.scm:94-108: Bank's "Act" cells $35.00, -$20.00, $67.00, $77.00, $0.00, $0.00; Income's -$55.00,
    # $0.00 (the closing transaction left out), -$67.00, -$77.00; Expenses' $20.00, $20.00.
    check_actuals(budget, accts, {
        "Bank": [35, -20, 67, 77, 0, 0],
        "Income": [-55, 0, -67, -77, 0, 0],
        "Expenses": [20, 20, 0, 0, 0, 0],
    })
    if (period_start(budget, 0), period_end(budget, 5)) != (dmy2time64(1, 3, 2024), dmy2time64(1, 9, 2024) - 1):
        sys.exit("the budget's periods are not March to August 2024")
    book.save()
    # create-test-data's balances (account_summary.py), with the budget's transactions: Bank +159, Income -199 and
    # the closing +55, Expenses +40, Equity -55.
    check_book(out, [
        ("Root.Asset.Bank", 2186 + 159, None),
        ("Root.Asset.GBP Bank", 608, None),
        ("Root.Asset.Wallet", 20, None),
        ("Root.Income", -2573 - 199 + 55, None),
        ("Root.Income-GBP", -612, None),
        ("Root.Expenses", 264 + 40, None),
        ("Root.Liabilities", -3, None),
        ("Root.Equity", 111 - 55, None),
    ])


if __name__ == "__main__":
    main()
