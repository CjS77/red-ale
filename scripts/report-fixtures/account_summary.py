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
"""Build `reports/test-data.gnucash`: the book of `create-test-data`
(`5.17:bindings/guile/test/test-engine-extras.scm:389-480`, licence notice above), with GBP's user symbol set to "#"
as `5.17:gnucash/report/reports/standard/test/test-account-summary.scm:26-33` does. test-account-summary.scm has no
licence notice of its own and is cited under GnuCash's licence, GPL-2.0-or-later.

No date depends on now. The Account Summary runs with its default report date, today (the pinned now,
2024-03-15), after every transaction.

Usage: TZ=UTC PYTHONPATH=<5.17 site-packages> scripts/report-fixtures/account_summary.py [out-dir]
"""
from engine_extras import (ASSET, EQUITY, EXPENSE, INCOME, LIABILITY, check_book, create_test_env, dmy2time64,
                           env_create_account_structure_alist, env_create_multisplit_transaction, env_transfer,
                           env_transfer_foreign, mnemonic_to_commodity, new_book, out_dir)


def create_test_data():
    """`create-test-data` (`test-engine-extras.scm:389-480`)."""
    env = create_test_env()
    gbp = mnemonic_to_commodity("GBP")
    structure = ["Root", {"type": ASSET},
                 ["Asset", ["Bank"], ["GBP Bank", {"commodity": gbp}], ["Wallet"]],
                 ["Income", {"type": INCOME}],
                 ["Income-GBP", {"type": INCOME, "commodity": gbp}],
                 ["Expenses", {"type": EXPENSE}],
                 ["Liabilities", {"type": LIABILITY}],
                 ["Equity", {"type": EQUITY}]]
    accts = env_create_account_structure_alist(env, structure)
    bank, gbp_bank, wallet = accts["Bank"], accts["GBP Bank"], accts["Wallet"]
    income, gbp_income, expense = accts["Income"], accts["Income-GBP"], accts["Expenses"]
    liability, equity = accts["Liabilities"], accts["Equity"]

    # :420-448, old transactions
    env_transfer(env, 1, 1, 1970, bank, expense, 5, description="desc-1", num="trn1", memo="memo-3")
    env_transfer(env, 31, 12, 1969, income, bank, 10, description="desc-2", num="trn2", void_reason="void",
                 notes="notes3")
    env_transfer(env, 31, 12, 1969, income, bank, 29, description="desc-3", num="trn3",
                 reconcile=("c", dmy2time64(1, 3, 1970)))
    env_transfer(env, 1, 2, 1970, bank, expense, 15, description="desc-4", num="trn4", notes="notes2",
                 memo="memo-1")
    env_transfer(env, 10, 1, 1970, liability, expense, 10, description="desc-5", num="trn5", void_reason="any")
    env_transfer(env, 10, 1, 1970, liability, expense, 11, description="desc-6", num="trn6", notes="notes1")
    env_transfer(env, 10, 2, 1970, bank, liability, 8, description="desc-7", num="trn7", notes="notes1",
                 memo="memo-2", reconcile=("y", dmy2time64(1, 3, 1970)))
    # :450-456
    env_create_multisplit_transaction(env, 14, 2, 1971, [(bank, -100, -100), (expense, 80, 80), (wallet, 20, 20)],
                                      description="$100bank -> $80expenses + $20wallet", notes="multisplit")
    # :458-461, flagged as closing after it is committed
    env_transfer(env, 31, 12, 1977, expense, equity, 111, description="Closing").SetIsClosingTxn(True)
    # :463-467, each adding a price (:279-286): USD in GBP 10/14, then GBP in USD 9/6
    env_transfer_foreign(env, 15, 1, 2000, gbp_bank, bank, 10, 14, description="GBP 10 to USD 14")
    env_transfer_foreign(env, 15, 2, 2000, bank, gbp_bank, 9, 6, description="USD 9 to GBP 6")
    # :469-479
    for m in range(12):
        env_transfer(env, 8, m + 1, 1978, gbp_income, gbp_bank, 51, description="#51 income")
        env_transfer(env, 3, m + 1, 1978, income, bank, 103, description="$103 income")
        env_transfer(env, 15, m + 1, 1978, bank, expense, 22, description="$22 expense")
        env_transfer(env, 9, m + 1, 1978, income, bank, 109, description="$109 income")


def main():
    out = out_dir() / "test-data.gnucash"
    book = new_book(out)
    # test-account-summary.scm:26-33
    mnemonic_to_commodity("GBP").set_user_symbol("#")
    create_test_data()
    book.save()
    # test-account-summary.scm:61-64: Bank $2,186.00, GBP Bank #608.00, Wallet $20.00, Income-GBP -#612.00 as of
    # today; the others follow from the transactions above.
    check_book(out, [
        ("Root.Asset.Bank", 2186, None),
        ("Root.Asset.GBP Bank", 608, None),
        ("Root.Asset.Wallet", 20, None),
        ("Root.Income", -2573, None),
        ("Root.Income-GBP", -612, None),
        ("Root.Expenses", 264, None),
        ("Root.Liabilities", -3, None),
        ("Root.Equity", 111, None),
    ])


if __name__ == "__main__":
    main()
