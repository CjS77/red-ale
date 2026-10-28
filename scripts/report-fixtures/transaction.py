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
"""Build the books of `5.17:gnucash/report/reports/standard/test/test-transaction.scm` (TT). TT has no licence notice
of its own and is cited under GnuCash's licence, GPL-2.0-or-later; the notice above is kept from
`5.17:bindings/guile/test/test-engine-extras.scm`, whose helpers build the books (`engine_extras.py`,
`invoice_data.py`).

- `reports/transaction.gnucash`: the trep book (`trep-tests`, TT:104-226), then `create-test-invoice-data`
  (test-engine-extras.scm:487-849) in the same book, as TT:1007 builds it. The invoice tree has a second top-level
  "Root" whose accounts TT:1012-1017 look up by full name beside the trep tree's, so both stay in one book.
- `reports/reconcile.gnucash`: the reconcile tree of `reconcile-tests` (TT:1077-1104) alone. GnuCash builds it into
  the same book again, but the Reconciliation Report reads only its Bank and Liabilities, so no figure depends on
  the other trees (`design/notes/report-transaction-engine.md` §4.5).

Dates are pinned to the now of `design/reports.md` section 13, 2024-03-15 12:00:00 UTC, run with TZ=UTC:

- `YEAR` (TT:117, the year of `gnc:get-today`) is 2024. The twelve months of transfers (TT:221-226) are posted on
  2024-MM-08 (#51 income), -03 ($103 income), -15 ($22 expense) and -09 ($109 income) for MM = 01..12, at 10:59 UTC;
  the ten after the pinned now (from 2024-03-15's $22 expense on) are in the future, as they are in any year
  GnuCash's test runs. `start-cal-year` and `end-cal-year`, the default dates of TT's options, read
  2024-01-01 00:00:00 and 2024-12-31 23:59:59 UTC.
- Entered dates are the pinned now plus one second per transaction in creation order: the 59 trep transactions
  2024-03-15 12:00:01 to 12:00:59, the eight invoice postings 12:01:00 to 12:01:07 and the payment 12:01:08. Void
  times are the voided transaction's entered date. The reconcile book starts again at 12:00:01.
- `(current-time)` dates the invoice entries: 2024-03-15, stored as noon, 12:00:00 UTC (`invoice_data.py`). Each
  invoice is opened on its posting day (1980-09-01 to -08), which the test leaves unset (`invoice_data.py`).
- The Reconciliation Report's default dates (`standard/reconcile-report.scm:42-47`), used only by its render-only
  run (TT:1108-1109), read `start-prev-quarter` 2023-10-01 00:00:00 to `today` 2024-03-15 23:59:59 UTC.

Every other date is absolute in the test (1969-2000 transfers, 1980 invoices).

GUIDs are fixed as in `engine_extras.py` and `invoice_data.py`; the script fails if any instance is left with an
engine-made GUID.

Usage: TZ=UTC PYTHONPATH=<5.17 site-packages> scripts/report-fixtures/transaction.py [out-dir]
"""
from fractions import Fraction

from engine_extras import (ASSET, EQUITY, EXPENSE, INCOME, LIABILITY, check_book, create_test_env, dmy2time64,
                           env_create_account_structure_alist, env_create_multisplit_transaction, env_transfer,
                           env_transfer_foreign, mnemonic_to_commodity, new_book, out_dir)
from invoice_data import check_fixed_guids, create_test_invoice_data, invoice_is_paid

# TT:85-97
STRUCTURE = ["Root", {"type": ASSET},
             ["Asset", ["Bank"], ["GBP Bank"], ["USD Bank"], ["Wallet"]],
             ["Income", {"type": INCOME}],
             ["Income-GBP", {"type": INCOME}],
             ["Expenses", {"type": EXPENSE}],
             ["Liabilities", {"type": LIABILITY}],
             ["Equity", {"type": EQUITY}]]
YEAR = 2024


def old_transfers(env, accts, trn7_to):
    """The seven 1969-1970 transfers (TT:182-190, and TT:1096-1104 with trn7 going to `trn7_to`)."""
    bank, income, expense, liability = (accts[n] for n in ("Bank", "Income", "Expenses", "Liabilities"))
    env_transfer(env, 1, 1, 1970, bank, expense, 5, description="desc-1", num="trn1", memo="memo-3")
    env_transfer(env, 31, 12, 1969, income, bank, 10, description="desc-2", num="trn2", void_reason="void",
                 notes="notes3")
    env_transfer(env, 31, 12, 1969, income, bank, 29, description="desc-3", num="trn3",
                 reconcile=("c", dmy2time64(1, 3, 1970)))
    env_transfer(env, 1, 2, 1970, bank, expense, 15, description="desc-4", num="trn4", notes="notes2",
                 memo="memo-1")
    env_transfer(env, 10, 1, 1970, liability, expense, 10, description="desc-5", num="trn5", void_reason="any")
    env_transfer(env, 10, 1, 1970, liability, expense, 11, description="desc-6", num="trn6", notes="notes1")
    env_transfer(env, 10, 2, 1970, bank, trn7_to, 8, description="desc-7", num="trn7", notes="notes1",
                 memo="memo-2", reconcile=("y", dmy2time64(1, 3, 1970)))


def trep_book():
    """`trep-tests` up to its first report (TT:104-226)."""
    env = create_test_env()
    accts = env_create_account_structure_alist(env, STRUCTURE)
    bank, gbp_bank, usd_bank, wallet = (accts[n] for n in ("Bank", "GBP Bank", "USD Bank", "Wallet"))
    income, gbp_income, expense = accts["Income"], accts["Income-GBP"], accts["Expenses"]
    equity = accts["Equity"]
    foreign1, foreign2 = mnemonic_to_commodity("USD"), mnemonic_to_commodity("GBP")

    # TT:146-153
    income.SetDescription("Salaries etc")
    expense.SetCode("EXP-001")
    for acct in accts.values():
        acct.SetCommodity(mnemonic_to_commodity("USD"))
    # TT:157-175, each in an edit (with-account)
    foreign2.set_user_symbol("#")
    for acct, code, commodity in [(gbp_bank, "01-GBP", foreign2), (gbp_income, "01-GBP", foreign2),
                                  (usd_bank, "02-USD", foreign1)]:
        acct.BeginEdit()
        acct.SetCode(code)
        acct.SetCommodity(commodity)
        acct.CommitEdit()

    old_transfers(env, accts, accts["Liabilities"])
    # TT:196-202
    env_create_multisplit_transaction(env, 14, 2, 1971, [(bank, -100, -100), (expense, 80, 80), (wallet, 20, 20)],
                                      description="$100bank -> $80expenses + $20wallet", notes="multisplit")
    # TT:205-206, flagged as closing after it is committed
    env_transfer(env, 31, 12, 1999, expense, equity, 111, description="Closing").SetIsClosingTxn(True)
    # TT:210-211, each adding a price: USD in GBP 10/14, then GBP in USD 9/6
    env_transfer_foreign(env, 15, 1, 2000, gbp_bank, usd_bank, 10, 14, description="GBP 10 to USD 14")
    env_transfer_foreign(env, 15, 2, 2000, usd_bank, gbp_bank, 9, 6, description="USD 9 to GBP 6")
    # TT:221-226
    for m in range(12):
        env_transfer(env, 8, m + 1, YEAR, gbp_income, gbp_bank, 51, description="#51 income")
        env_transfer(env, 3, m + 1, YEAR, income, bank, 103, description="$103 income")
        env_transfer(env, 15, m + 1, YEAR, bank, expense, 22, description="$22 expense")
        env_transfer(env, 9, m + 1, YEAR, income, bank, 109, description="$109 income")


def build_transaction(out):
    book = new_book(out)
    trep_book()
    invoices, _ = create_test_invoice_data()
    # The FIXME at test-engine-extras.scm:808-819: the payment of $17,479.18 settles invoice 0008 exactly.
    if not invoice_is_paid(invoices[7]):
        raise SystemExit("invoice 0008 is not paid")
    check_fixed_guids()
    book.save()
    check_book(out, [
        # Trep tree. Bank: the 1969-1970 transfers (+1, trn2 void), the 1971 multisplit (-100), 2024's twelve months
        # of 103 + 109 - 22 (TT:219, $2,280); its balance at the start of 2024 is -$99 (TT:676-678).
        ("a0000000000000000000000000000003", 1 - 100 + 2280, None),
        ("a0000000000000000000000000000003", -99, dmy2time64(1, 1, YEAR)),
        ("a0000000000000000000000000000004", -10 + 6 + 12 * 51, None),   # GBP Bank, in GBP (TT:947-950)
        ("a0000000000000000000000000000005", 14 - 9, None),              # USD Bank (TT:606-614)
        ("a0000000000000000000000000000006", 20, None),                  # Wallet
        ("a0000000000000000000000000000007", -29 - 12 * 212, None),      # Income (TT:955-959)
        ("a0000000000000000000000000000008", -12 * 51, None),            # Income-GBP, in GBP (TT:960-963)
        # Expenses: $31 in 1969-1970 (TT:301), $80 in 1971, the closing -$111 (TT:438-450), 2024's $264 (TT:951-954)
        ("a0000000000000000000000000000009", 31 + 80 - 111 + 264, None),
        ("a000000000000000000000000000000a", -3, None),                  # Liabilities: -11 + 8, trn5 void
        ("a000000000000000000000000000000b", 111, None),                 # Equity
        # Invoice tree at the end of 1980-09-07 (TT:1009-1021): A/Receivable 0001 + 0002 - 0005, A/Payable -0003 - 0004 + 0006 + 0007
        ("a0000000000000000000000000000014", 6, dmy2time64(8, 9, 1980)),
        ("a0000000000000000000000000000015", 0, dmy2time64(8, 9, 1980)),
        # 0008 posted on 1980-09-08 and paid in full on 1980-09-10 into the invoice tree's Bank
        ("a0000000000000000000000000000014", 6, None),
        ("a000000000000000000000000000000e", Fraction(1747918, 100), None),
        ("a0000000000000000000000000000013", 0, None),                   # Expense: 6 + 6 - 6 - 6
    ])


def build_reconcile(out):
    book = new_book(out)
    env = create_test_env()
    accts = env_create_account_structure_alist(env, STRUCTURE)
    old_transfers(env, accts, accts["Expenses"])
    check_fixed_guids()
    book.save()
    # TT:1113-1121: reconciled $8 (trn7), cleared $29 (trn3), unreconciled $31 (trn1, trn4, trn6) over Bank and
    # Liabilities; trn2 and trn5 are void.
    check_book(out, [
        ("a0000000000000000000000000000003", -5 + 29 - 15 - 8, None),   # Bank
        ("a000000000000000000000000000000a", -11, None),                # Liabilities
        ("a0000000000000000000000000000007", -29, None),                # Income
        ("a0000000000000000000000000000009", 5 + 15 + 11 + 8, None),    # Expenses
    ])


def main():
    build_transaction(out_dir() / "transaction.gnucash")
    build_reconcile(out_dir() / "reconcile.gnucash")


if __name__ == "__main__":
    main()
