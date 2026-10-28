#!/usr/bin/env python3
# Rebuilt from GnuCash 5.17's gnucash/report/reports/standard/test/test-investment-lots.scm, which carries no licence
# notice of its own; it is part of GnuCash and cited under GnuCash's licence, GPL-2.0-or-later (5.17:LICENSE).
"""Build the five books of `5.17:gnucash/report/reports/standard/test/test-investment-lots.scm`:

| Book | Built by | File |
|------|----------|------|
| A | `create-investment-lots-test-data` (`:59-122`) | `reports/investment-lots.gnucash` |
| B | `create-reverse-split-lot-test-data` (`:124-184`) | `reports/lots-reverse-split.gnucash` |
| C | `create-baseline-lot-test-data` (`:186-229`) | `reports/lots-baseline.gnucash` |
| D | `create-reverse-split-with-sale-test-data` (`:231-307`) | `reports/lots-reverse-split-sale.gnucash` |
| E | `create-equivalent-postsplit-sale-baseline-test-data` (`:309-369`) | `reports/lots-postsplit-baseline.gnucash` |

Book A's transactions do not balance (the AAPL and SPY splits' values are their share counts, the Cash splits the
prices paid), so each commit adds a split to an Imbalance-USD account it makes beside "Root", at the top of the tree
(`5.17:libgnucash/engine/Transaction.cpp:1549-1558`); those, made with random GUIDs, get fixed ones here, in the order
they appear. Each AAPL or SPY split adds a price of 1 USD (`5.17:bindings/guile/test/test-engine-extras.scm:279-286`).

Books B to E put every AAPL split in one lot (`gnc-lot-new`, `gnc-lot-add-split` after `xaccSplitSetParent`); the
lot has no title. Lots take GUIDs of kind `1` (`1000…0`). Objects get GUIDs in the order the test allocates them
(the lot, then each transaction and split as its `let*` binds them). The test leaves entered dates to the wall clock;
here they are the pinned now plus one second per transaction.

No date in the books depends on now. The test runs every report with the default Start date and End date, the start
of the accounting period and today (`5.17:gnucash/report/reports/standard/investment-lots.scm:353-359`); with no
preferences the period is today to today, so for the pinned now, 2024-03-15 12:00 UTC, both bounds are taken at the
end of 2024-03-15 (`:667-672`), and every split, all in 2020, falls before the window. So does any now after 2020.

Usage: TZ=UTC PYTHONPATH=<5.17 site-packages> scripts/report-fixtures/investment_lots.py [out-dir]
"""
import ctypes
import os
import shutil
import sys
import tempfile
from fractions import Fraction

from gnucash import GncLot, GncNumeric, Session, SessionOpenMode, Split, Transaction

import engine_extras as ee
from engine_extras import (ASSET, INCOME, STOCK, GncGUID, check_book, create_test_env,
                           env_create_account_structure_alist, env_create_multisplit_transaction,
                           mnemonic_to_commodity, new_book, out_dir, set_guid)
from gnc_engine import engine, ptr
from stock_test_data import adopt_guids, new_commodity


def broker_structure(usd, aapl, spy=None, income=False):
    """The tests' account tree: Root (ASSET, USD) > Assets > Broker > Cash, AAPL [, SPY] [; Root > Income]."""
    broker = ["Broker", {"commodity": usd}, ["Cash", {"commodity": usd}],
              ["AAPL", {"commodity": aapl, "type": STOCK}]]
    if spy is not None:
        broker.append(["SPY", {"commodity": spy, "type": STOCK}])
    structure = ["Root", {"type": ASSET, "commodity": usd}, ["Assets", broker]]
    if income:
        structure.append(["Income", {"type": INCOME}])
    return structure


def create_investment_lots_test_data():
    """Book A, `create-investment-lots-test-data` (`:59-122`)."""
    env = create_test_env()
    usd = mnemonic_to_commodity("USD")
    aapl_c = new_commodity("Apple", "NASDAQ", "AAPL", 1)
    spy_c = new_commodity("SPY", "NYSE", "SPY", 1)
    a = env_create_account_structure_alist(env, broker_structure(usd, aapl_c, spy_c, income=True))
    cash, aapl, spy = a["Cash"], a["AAPL"], a["SPY"]
    for dd, mm, yy, splits, description in [
            (1, 1, 2020, [(cash, -5000, -5000, ""), (aapl, 100, 100, "Buy 100 shares")], "Buy AAPL lot 1"),
            (1, 2, 2020, [(cash, -3000, -3000, ""), (aapl, 50, 50, "Buy 50 shares")], "Buy AAPL lot 2"),
            (1, 6, 2020, [(cash, 4800, 4800, ""), (aapl, -60, -60, "Sell 60 shares")], "Sell AAPL"),
            (1, 1, 2020, [(cash, -40000, -40000, ""), (spy, 200, 200, "Buy 200 shares")], "Buy SPY")]:
        adopt_guids(env_create_multisplit_transaction(env, dd, mm, yy, splits, description=description, currency=usd))


class LotBook:
    """The common part of books B to E: AAPL in its own book, and the objects the test allocates up front."""

    def __init__(self):
        env = create_test_env()
        self.usd = mnemonic_to_commodity("USD")
        aapl_c = new_commodity("Apple", "NASDAQ", "AAPL", 1)
        a = env_create_account_structure_alist(env, broker_structure(self.usd, aapl_c))
        self.cash, self.aapl = a["Cash"], a["AAPL"]
        self.lot = GncLot(ee.BOOK.book)
        set_guid(self.lot, ee.BOOK.guid("1"))

    @staticmethod
    def malloc_transaction(n_splits):
        """`xaccMallocTransaction` and its `xaccMallocSplit`s, in the order the test's `let*` binds them."""
        txn = Transaction(ee.BOOK.book)
        set_guid(txn, ee.BOOK.guid("c"))
        splits = [Split(ee.BOOK.book) for _ in range(n_splits)]
        for split in splits:
            set_guid(split, ee.BOOK.guid("d"))
        return txn, splits

    def fill(self, txn, splits, dmy, description, stock_amount, stock_value, cash_amount=None, stock_split=False):
        """One of the test's hand-built transactions: the AAPL split (into the lot), then the cash split if any."""
        txn.BeginEdit()
        txn.SetCurrency(self.usd)
        txn.SetDate(*dmy)
        txn.SetDescription(description)
        stock = splits[0]
        stock.SetAccount(self.aapl)
        stock.SetAmount(GncNumeric(stock_amount, 1))
        stock.SetValue(GncNumeric(stock_value, 1))
        if stock_split:
            stock.MakeStockSplit()
            stock.SetAction("Split")
        stock.SetParent(txn)
        self.lot.add_split(stock)
        if cash_amount is not None:
            cash = splits[1]
            cash.SetAccount(self.cash)
            cash.SetAmount(GncNumeric(cash_amount, 1))
            cash.SetValue(GncNumeric(cash_amount, 1))
            cash.SetParent(txn)
        txn.SetDateEnteredSecs(ee.BOOK.next_entered())
        txn.CommitEdit()


def create_reverse_split_lot_test_data(with_sale=False):
    """Book B, `create-reverse-split-lot-test-data` (`:124-184`); with `with_sale`, book D,
    `create-reverse-split-with-sale-test-data` (`:231-307`)."""
    b = LotBook()
    buy = b.malloc_transaction(2)
    split = b.malloc_transaction(1)
    sale = b.malloc_transaction(2) if with_sale else None
    b.fill(*buy, (1, 1, 2020), "Buy AAPL lot", 100, 5000, -5000)
    b.fill(*split, (1, 2, 2020), "Reverse Split", -50, 0, stock_split=True)
    if with_sale:
        b.fill(*sale, (1, 3, 2020), "Sell AAPL", -25, -3000, 3000)


def create_baseline_lot_test_data():
    """Book C, `create-baseline-lot-test-data` (`:186-229`)."""
    b = LotBook()
    b.fill(*b.malloc_transaction(2), (1, 1, 2020), "Buy AAPL lot baseline", 100, 5000, -5000)


def create_equivalent_postsplit_sale_baseline_test_data():
    """Book E, `create-equivalent-postsplit-sale-baseline-test-data` (`:309-369`)."""
    b = LotBook()
    buy = b.malloc_transaction(2)
    sale = b.malloc_transaction(2)
    b.fill(*buy, (1, 1, 2020), "Buy AAPL baseline", 50, 5000, -5000)
    b.fill(*sale, (1, 3, 2020), "Sell AAPL baseline", -25, -3000, 3000)


def check_lots(path, expected):
    """Open `path` read-only and compare each lot of Root.Assets.Broker.AAPL, in order: its GUID, title, the
    (description, amount, value) of its splits in list order, and the engine's lot balance (adjusted for stock
    splits). Exits with the differences."""
    work = tempfile.mkdtemp()
    copy = os.path.join(work, "book.gnucash")
    shutil.copy(path, copy)
    session = Session("xml://" + copy, SessionOpenMode.SESSION_READ_ONLY)
    try:
        aapl = session.book.get_root_account().lookup_by_full_name("Root.Assets.Broker.AAPL")
        got = [describe_lot(lot) for lot in aapl.GetLotList()]
        unassigned = [s.GetGUID().to_string() for s in aapl.GetSplitList() if s.GetLot() is None]
    finally:
        session.end()
        shutil.rmtree(work)
    if got != expected or unassigned:
        sys.exit(f"{path}: lots {got} != expected {expected}; splits outside a lot {unassigned}")


engine.qof_instance_get_guid.restype = ctypes.POINTER(GncGUID)
engine.qof_instance_get_guid.argtypes = [ctypes.c_void_p]


def describe_lot(lot):
    """The bindings give a lot no GUID getter, so it is read with `qof_instance_get_guid`."""
    def frac(n):
        return Fraction(n.num(), n.denom())
    guid = bytes(engine.qof_instance_get_guid(ptr(lot)).contents.data).hex()
    splits = [(s.parent.GetDescription(), frac(s.GetAmount()), frac(s.GetValue()), s.IsStockSplit())
              for s in lot.get_split_list()]
    return guid, lot.get_title(), splits, frac(lot.get_balance())


LOT = "10000000000000000000000000000000"


def build(out, create, balances, lots=None):
    book = new_book(out)
    create()
    book.save()
    check_book(out, balances)
    if lots is not None:
        check_lots(out, lots)


def main():
    out = out_dir()
    # Book A: AAPL 100 + 50 - 60 = 90 shares, SPY 200; Cash -5000 - 3000 + 4800 - 40000. The imbalances are the
    # negated sums of each transaction's values: 4900, 2950, -4740, 39800.
    build(out / "investment-lots.gnucash", create_investment_lots_test_data, [
        ("Root.Assets.Broker.AAPL", 90, None),
        ("Root.Assets.Broker.SPY", 200, None),
        ("Root.Assets.Broker.Cash", -43200, None),
        ("Imbalance-USD", 4900 + 2950 - 4740 + 39800, None),
        ("Root.Income", 0, None),
    ])
    # Books B to E: one lot, untitled, holding every AAPL split; Cash holds the rest. The engine's lot balance sums
    # adjusted amounts (5.17:libgnucash/engine/gnc-lot.cpp:505-511): a stock split scales the adjusted amounts of the
    # splits before it by new/old balance, and keeps its own raw amount, set by xaccSplitSetAmount
    # (5.17:libgnucash/engine/Account.cpp:2285-2308, Split.cpp:1260). So the reverse split is counted twice: B's lot
    # balance is 100 x 50/100 - 50 = 0 and D's -25, where the shares held are 50 and 25. Investment Lots never reads
    # it (design/notes/report-investments.md 4.5). A lot's splits come back in the order the XML file lists their
    # transactions, Cash's first (D: buy, sale, reverse split); the report sorts them by posted date.
    build(out / "lots-reverse-split.gnucash", create_reverse_split_lot_test_data, [
        ("Root.Assets.Broker.AAPL", 50, None),
        ("Root.Assets.Broker.Cash", -5000, None),
    ], [(LOT, None, [("Buy AAPL lot", 100, 5000, False), ("Reverse Split", -50, 0, True)], 0)])
    build(out / "lots-baseline.gnucash", create_baseline_lot_test_data, [
        ("Root.Assets.Broker.AAPL", 100, None),
        ("Root.Assets.Broker.Cash", -5000, None),
    ], [(LOT, None, [("Buy AAPL lot baseline", 100, 5000, False)], 100)])
    build(out / "lots-reverse-split-sale.gnucash", lambda: create_reverse_split_lot_test_data(with_sale=True), [
        ("Root.Assets.Broker.AAPL", 25, None),
        ("Root.Assets.Broker.Cash", -2000, None),
    ], [(LOT, None, [("Buy AAPL lot", 100, 5000, False), ("Sell AAPL", -25, -3000, False),
                     ("Reverse Split", -50, 0, True)], -25)])
    build(out / "lots-postsplit-baseline.gnucash", create_equivalent_postsplit_sale_baseline_test_data, [
        ("Root.Assets.Broker.AAPL", 25, None),
        ("Root.Assets.Broker.Cash", -2000, None),
    ], [(LOT, None, [("Buy AAPL baseline", 50, 5000, False), ("Sell AAPL baseline", -25, -3000, False)], 25)])


if __name__ == "__main__":
    main()
