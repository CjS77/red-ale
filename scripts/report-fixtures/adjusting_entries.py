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
"""Build `reports/adjusting-entries.gnucash` and `reports/adjusting-entries-fx.gnucash`: the book of
`5.17:gnucash/report/reports/standard/test/test-trial-balance.scm` and
`5.17:gnucash/report/reports/standard/test/test-equity-statement.scm` (licence notice above, kept from both files).

Both tests build the same book: `create-test-data` (`5.17:bindings/guile/test/test-engine-extras.scm:389-480`,
ported in `account_summary.py`), GBP's user symbol "#" (`test-trial-balance.scm:65-67`,
`test-equity-statement.scm:64-66`), then three 2003 transfers (`test-trial-balance.scm:69-76`,
`test-equity-statement.scm:68-75`), the first flagged as a closing transaction. That book is
`adjusting-entries.gnucash`, for the Equity Statement and the Trial Balance's first three runs.

Before its last run the Trial Balance's test adds two foreign transfers (`test-trial-balance.scm:170-173`), each
adding a price as a side effect (`test-engine-extras.scm:279-286`). Each replaces `create-test-data`'s price of the
same pair, day and source (`5.17:libgnucash/engine/gnc-pricedb.cpp:1053-1071`): USD in GBP 10/14 on 2000-01-15 by
an equal one, and GBP in USD 9/6 on 2000-02-15 by 9/8. That book is `adjusting-entries-fx.gnucash`.

No date depends on now: the transactions are dated, and the reports' dates are absolute or (the Trial Balance's
defaults, the accounting period) after every transaction.

Usage: TZ=UTC PYTHONPATH=<5.17 site-packages> scripts/report-fixtures/adjusting_entries.py [out-dir]
"""
import os
import shutil
import tempfile
from fractions import Fraction

from gnucash import Session, SessionOpenMode

from account_summary import create_test_data
from engine_extras import (check_book, dmy2time64, env_transfer, env_transfer_foreign, mnemonic_to_commodity,
                           new_book, out_dir)


def adjusting_entries(accts):
    """`test-trial-balance.scm:69-76` (= `test-equity-statement.scm:68-75`). `env-transfer` moves `amount` from its
    first account to its second (`test-engine-extras.scm:199-223`)."""
    expense, equity, income = accts["Expenses"], accts["Equity"], accts["Income"]
    env_transfer(None, 30, 6, 2003, expense, equity, 111, description="Closing Entries").SetIsClosingTxn(True)
    env_transfer(None, 1, 6, 2003, expense, equity, 33, description="Adjusting Entries")
    env_transfer(None, 1, 7, 2003, income, equity, -2500, description="Adjusting Entries")


def foreign_transfers(accts):
    """`test-trial-balance.scm:170-173`."""
    gbp_bank, usd_bank = accts["GBP Bank"], accts["Bank"]
    env_transfer_foreign(None, 15, 1, 2000, gbp_bank, usd_bank, 10, 14, description="GBP 10 to USD 14")
    env_transfer_foreign(None, 15, 2, 2000, usd_bank, gbp_bank, 9, 8, description="USD 9 to GBP 8")


def build(out, with_fx):
    book = new_book(out)
    # test-trial-balance.scm:54, then :65-67
    accts = create_test_data()
    mnemonic_to_commodity("GBP").set_user_symbol("#")
    adjusting_entries(accts)
    if with_fx:
        foreign_transfers(accts)
    book.save()


def gbp_prices(path):
    """The GBP prices in USD of the saved book, as `(time, value)`, opened read-only in GnuCash."""
    work = tempfile.mkdtemp()
    copy = os.path.join(work, "book.gnucash")
    shutil.copy(path, copy)
    session = Session("xml://" + copy, SessionOpenMode.SESSION_READ_ONLY)
    try:
        table = session.book.get_table()
        gbp, usd = table.lookup("CURRENCY", "GBP"), table.lookup("CURRENCY", "USD")
        prices = session.book.get_price_db().get_prices(gbp, usd)
        # get_time64 gives a naive local datetime; under TZ=UTC its timestamp is the time64
        return sorted((int(p.get_time64().timestamp()), Fraction(p.get_value().num(), p.get_value().denom()))
                      for p in prices)
    finally:
        session.end()
        shutil.rmtree(work)


def main():
    plain, fx = out_dir() / "adjusting-entries.gnucash", out_dir() / "adjusting-entries-fx.gnucash"
    build(plain, with_fx=False)
    build(fx, with_fx=True)
    # create-test-data's balances (test-account-summary.scm:61-64) moved by the three transfers: Income -2573 + 2500,
    # Expenses 264 - 111 - 33, Equity 111 + 111 + 33 - 2500. These are the Trial Balance's own-balance figures
    # (test-trial-balance.scm:88-94): Bank $2,186.00, GBP Bank #608 ($912.00 at 3/2), Wallet $20.00, Liabilities
    # $3.00 credit, Income $73.00 credit, Expenses $120.00, Equity $2,245.00 credit.
    check_book(plain, [
        ("Root.Asset.Bank", 2186, None),
        ("Root.Asset.GBP Bank", 608, None),
        ("Root.Asset.Wallet", 20, None),
        ("Root.Liabilities", -3, None),
        ("Root.Income", -73, None),
        ("Root.Income-GBP", -612, None),
        ("Root.Expenses", 120, None),
        ("Root.Equity", -2245, None),
        # Before the adjustments (the pre-adj variation from 2000-01-01, :109-115): Expenses $264.00 less the
        # closing $111.00, Income $2,573.00 credit.
        ("Root.Expenses", 264, dmy2time64(1, 1, 2003)),
        ("Root.Income", -2573, dmy2time64(1, 1, 2003)),
        # The Equity Statement's opening capital (test-equity-statement.scm:86-88): the 1969 income of 29 is in
        # Bank on 1970-01-01, the void 10 counting 0.
        ("Root.Asset.Bank", 29, dmy2time64(1, 1, 1970)),
    ])
    check_book(fx, [
        ("Root.Asset.Bank", 2186 + 14 - 9, None),
        ("Root.Asset.GBP Bank", 608 - 10 + 8, None),
        ("Root.Income", -73, None),
        ("Root.Expenses", 120, None),
        ("Root.Equity", -2245, None),
    ])
    expected = [(dmy2time64(15, 2, 2000), Fraction(9, 8))]
    got = gbp_prices(fx)
    assert got == expected, f"{fx}: GBP prices in USD {got} != {expected}"
    assert gbp_prices(plain) == [(dmy2time64(15, 2, 2000), Fraction(3, 2))], f"{plain}: GBP prices in USD"


if __name__ == "__main__":
    main()
