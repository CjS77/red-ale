#!/usr/bin/env python3
# Rebuilt from GnuCash 5.17's gnucash/report/reports/standard/test/test-average-balance.scm, which carries no licence
# notice of its own; it is part of GnuCash and cited under GnuCash's licence, GPL-2.0-or-later (5.17:LICENSE).
"""Build `reports/average-balance.gnucash`: the book of
`5.17:gnucash/report/reports/standard/test/test-average-balance.scm:35-81` with all three transfers.

The test runs its first report (`:66-79`) after the first two transfers and the others (`:81-99`) after the third,
an internal transfer between the two selected accounts. The third transfer leaves the selected accounts' combined
balance unchanged, so the first checks hold on this book too (see `average-balance.expected.json`).

No date depends on now; the report options are absolute dates.

Usage: TZ=UTC PYTHONPATH=<5.17 site-packages> scripts/report-fixtures/average_balance.py [out-dir]
"""
from engine_extras import (ASSET, INCOME, check_book, create_test_env, dmy2time64, env_create_account_structure_alist,
                           env_transfer, new_book, out_dir)


def main():
    out = out_dir() / "average-balance.gnucash"
    book = new_book(out)
    env = create_test_env()
    # :35-39, :44-51
    structure = ["Root", {"type": ASSET}, ["Bank"], ["Another Bank"], ["Income", {"type": INCOME}]]
    accts = env_create_account_structure_alist(env, structure)
    bank, bank2, income = accts["Bank"], accts["Another Bank"], accts["Income"]
    # :63-64
    env_transfer(env, 15, 2, 1979, income, bank, 100)
    env_transfer(env, 16, 4, 1979, income, bank, 100)
    # :81
    env_transfer(env, 15, 3, 1979, bank, bank2, 25)
    book.save()
    # Bank and Another Bank together hold $200 from 16 April (:71-73, maximum $200.00); $25 of it in Another Bank.
    end = dmy2time64(30, 6, 1979)
    check_book(out, [
        ("Root.Bank", 175, end),
        ("Root.Another Bank", 25, end),
        ("Root.Income", -200, end),
        ("Root.Bank", 75, dmy2time64(1, 4, 1979)),
    ])


if __name__ == "__main__":
    main()
