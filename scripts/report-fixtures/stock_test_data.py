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
"""Build `reports/stock-test-data.gnucash`: the book of
`5.17:gnucash/report/reports/standard/test/test-portfolios.scm:52`, `:84`, which is `create-stock-test-data`
(`5.17:bindings/guile/test/test-engine-extras.scm:952-1059`) with its `stock-split` helper (`:893-950`). The
licence notice above is test-engine-extras.scm's; test-portfolios.scm has none and is cited under GnuCash's licence.

Also holds the helpers the other investment fixture scripts share: `new_commodity` and `adopt_guids`.

No date in the book depends on now. The tests' only dependence on now is the reports' default Date, the end of the
accounting period, which with no preferences is the end of today: the tests run with the pinned now, 2024-03-15, so
the default Date is 2024-03-15 23:59:59 UTC. Any now after 1981-04-01 gives the same figures.

`stock-split` sets the transaction currency to `gnc-default-currency`, which under GnuCash's test (C locale, no
preferences) is USD; this script names USD. It leaves the entered date to the wall clock; here it is the pinned now
plus one second per transaction, like every other transaction.

Usage: TZ=UTC PYTHONPATH=<5.17 site-packages> scripts/report-fixtures/stock_test_data.py [out-dir]
"""
import gnucash.gnucash_core_c as c
from gnucash import GncCommodity, GncPrice, Split, Transaction

import engine_extras as ee
from engine_extras import (ASSET, EQUITY, EXPENSE, INCOME, STOCK, check_book, create_test_env, dmy2time64,
                           env_create_account_structure_alist, env_create_multisplit_transaction, env_transfer,
                           mnemonic_to_commodity, new_book, numeric, out_dir, set_guid)


def new_commodity(name, namespace, mnemonic, scu):
    """`gnc-commodity-new` with an empty CUSIP, then `gnc-commodity-table-insert` into the book's commodity table."""
    commodity = GncCommodity(ee.BOOK.book, name, namespace, mnemonic, "", scu)
    ee.BOOK.table.insert(commodity)
    return commodity


def is_fixed(guid, kind):
    """True for a GUID `FixtureBook.guid` gave out: the kind, then a zero-padded sequence number."""
    return guid.startswith(kind) and guid[len(kind):len(kind) + 16] == "0" * 16


def adopt_guids(txn):
    """Give fixed GUIDs to what committing `txn` made with random ones: the split the imbalance scrub adds
    (`5.17:libgnucash/engine/Transaction.cpp:1549-1558`) and the Imbalance account it may create, in split order."""
    for split in txn.GetSplitList():
        acct = split.GetAccount()
        if not is_fixed(acct.GetGUID().to_string(), "a"):
            set_guid(acct, ee.BOOK.guid("a"))
        if not is_fixed(split.GetGUID().to_string(), "d"):
            set_guid(split, ee.BOOK.guid("d"))


def stock_split(account, date, shares, description, price_amount, price_currency, cash_amount=None, cash_memo=None,
                cash_income=None, cash_asset=None):
    """`stock-split` (`test-engine-extras.scm:893-950`): a one-split stock split (`xaccSplitMakeStockSplit`, action
    "Split"), a price with source `user:stock-split` and type "unknown" at `date`, and optionally a cash-in-lieu
    pair of splits."""
    book = ee.BOOK
    accounts = []
    trans = Transaction(book.book)
    set_guid(trans, book.guid("c"))
    trans.BeginEdit()
    trans.SetCurrency(mnemonic_to_commodity("USD"))
    trans.SetDatePostedSecsNormalized(date)
    trans.SetDescription(description)

    stocksplit = Split(book.book)
    set_guid(stocksplit, book.guid("d"))
    account.BeginEdit()
    accounts.append(account)
    stocksplit.SetAccount(account)
    stocksplit.SetAmount(numeric(shares))
    stocksplit.MakeStockSplit()
    stocksplit.SetAction("Split")
    stocksplit.SetParent(trans)

    if price_amount is not None:
        price = GncPrice(book.book)
        set_guid(price, book.guid("e"))
        price.begin_edit()
        price.set_commodity(account.GetCommodity())
        price.set_currency(price_currency)
        price.set_time64(date)
        price.set_source(c.PRICE_SOURCE_STOCK_SPLIT)
        price.set_typestr("unknown")
        price.set_value(numeric(price_amount))
        price.commit_edit()
        book.pricedb.add_price(price)

    if cash_amount is not None:
        for acct, amount in ((cash_asset, cash_amount), (cash_income, -cash_amount)):
            split = Split(book.book)
            set_guid(split, book.guid("d"))
            acct.BeginEdit()
            accounts.append(acct)
            split.SetAccount(acct)
            split.SetParent(trans)
            split.SetAmount(numeric(amount))
            split.SetValue(numeric(amount))
            split.SetMemo(cash_memo)

    trans.SetDateEnteredSecs(book.next_entered())
    trans.CommitEdit()
    for acct in accounts:
        acct.CommitEdit()
    return trans


def currency_or_parent(account):
    """`gnc-account-get-currency-or-parent` (`5.17:libgnucash/engine/Account.cpp:3403-3412`)."""
    return c.gnc_account_get_currency_or_parent(account.instance)


def create_stock_test_data():
    """`create-stock-test-data` (`test-engine-extras.scm:952-1059`)."""
    structure = ["Root", {"type": ASSET},
                 ["Asset", ["Bank"]],
                 ["Income", {"type": INCOME}],
                 ["Expenses", {"type": EXPENSE}],
                 ["Equity", {"type": EQUITY}],
                 ["Broker",
                  ["AAPL", {"type": STOCK}],
                  ["MSFT", {"type": STOCK}],
                  ["TSLA", {"type": STOCK}]]]
    env = create_test_env()
    table = ee.BOOK.table
    aapl_c = GncCommodity(ee.BOOK.book, "Apple", "NASDAQ", "AAPL", "", 1)
    msft_c = GncCommodity(ee.BOOK.book, "Microsoft", "NASDAQ", "MSFT", "", 1)
    tsla_c = GncCommodity(ee.BOOK.book, "Tesla Motors", "NASDAQ", "TSLA", "", 1)
    a = env_create_account_structure_alist(env, structure)
    bank, inco, expe, equity = a["Bank"], a["Income"], a["Expenses"], a["Equity"]
    aapl, msft, tsla = a["AAPL"], a["MSFT"], a["TSLA"]

    # :980-986
    for commodity in (aapl_c, msft_c, tsla_c):
        table.insert(commodity)
    aapl.SetCommodity(aapl_c)
    msft.SetCommodity(msft_c)
    tsla.SetCommodity(tsla_c)

    # :988-1009
    env_transfer(env, 1, 1, 1980, equity, bank, 10000, description="seed money")
    env_create_multisplit_transaction(env, 1, 2, 1980, [(bank, -100, -100), (aapl, 100, 1)],
                                      description="buy 1 AAPL @ $100")
    env_create_multisplit_transaction(env, 1, 3, 1980, [(bank, -200, -200), (aapl, 200, 1)],
                                      description="buy 1 AAPL @ $200")
    env_create_multisplit_transaction(
        env, 1, 5, 1980,
        [(bank, 390, 390), (aapl, -400, -1), (inco, -300, -300), (expe, 10, 10), (aapl, 300, 0)],
        description="sell 1 AAPL @ $400 FIFO, brokerage fee = $10, into bank = $390")

    # :1014-1036
    stock_split(aapl, dmy2time64(1, 10, 1980), 9, "first 1:10 stock split", 40, currency_or_parent(aapl))
    stock_split(aapl, dmy2time64(1, 11, 1980), 90, "another 1:10 stock split", 4, currency_or_parent(aapl))
    stock_split(aapl, dmy2time64(1, 12, 1980), -67, "3:1 stock split with cash-in-lieu $4", 12,
                currency_or_parent(aapl), 4, "cash-in-lieu", inco, bank)

    # :1038-1058
    env_create_multisplit_transaction(env, 1, 1, 1981, [(bank, -500, -500), (aapl, 500, 10)],
                                      description="buy 10 AAPL @ $5")
    env_create_multisplit_transaction(
        env, 1, 3, 1981,
        [(bank, 3, 3), (aapl, -3, "-1/2"), (inco, "-5/2", "-5/2"), (aapl, "5/2", 0)],
        description="sell 1/2 AAPL @ $6 FIFO, capgain = $2.50 into bank = $200")
    env_create_multisplit_transaction(env, 1, 4, 1981, [(bank, 150, 150), (aapl, -150, 0)],
                                      description="spin-off $150")
    return a


def main():
    out = out_dir() / "stock-test-data.gnucash"
    book = new_book(out)
    create_stock_test_data()
    book.save()
    # AAPL: 1 + 1 - 1 + 9 + 90 - 67 + 10 - 1 = 42 shares (test-portfolios.scm:57, "42.00"; the -1/2 sale is
    # stored as -1 at SCU 1), and 2 shares on 1980-03-01 (:64), 1 on 1980-09-01 (:71). Bank: 10000 - 100 - 200 + 390
    # + 4 - 500 + 3 + 150. Income: -300 - 4 - 5/2.
    check_book(out, [
        ("Root.Broker.AAPL", 42, None),
        ("Root.Broker.AAPL", 2, dmy2time64(2, 3, 1980)),
        ("Root.Broker.AAPL", 1, dmy2time64(1, 9, 1980)),
        ("Root.Broker.MSFT", 0, None),
        ("Root.Broker.TSLA", 0, None),
        ("Root.Asset.Bank", 9747, None),
        ("Root.Income", "-613/2", None),
        ("Root.Expenses", 10, None),
        ("Root.Equity", -10000, None),
    ])


if __name__ == "__main__":
    main()
