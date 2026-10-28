#!/usr/bin/env python3
# Rebuilt from GnuCash 5.17's gnucash/report/reports/standard/test/test-ifrs-cost-basis.scm, which carries no licence
# notice of its own; it is part of GnuCash and cited under GnuCash's licence, GPL-2.0-or-later (5.17:LICENSE).
"""Build `reports/ifrs-cost-basis.gnucash`: the book of `create-ifrs-cost-basis-test-data`
(`5.17:gnucash/report/reports/standard/test/test-ifrs-cost-basis.scm:54-243`).

SPY is `NYSE::SPY` with SCU 10000 (`:59`). The SPY transactions are in CAD with the actions the test gives each split
(`:103-241`); every split into USD Cash or USD Commissions adds a USD price in CAD (`test-engine-extras.scm:279-286`),
and the CAD to USD transfer one more (`:97-101`), none of which the report reads: its rate is CAD to CAD.

Rows 10 and 11 of the report are two transactions posted on 2020-06-10 (`:183-201`); the report orders them by
`xaccSplitOrder`, so by entered date, which here is creation order, the pinned now plus one second per transaction.

No date depends on now: the test's options are absolute dates (`:256-259`).

Usage: TZ=UTC PYTHONPATH=<5.17 site-packages> scripts/report-fixtures/ifrs_cost_basis.py [out-dir]
"""
from engine_extras import (ASSET, EQUITY, EXPENSE, INCOME, STOCK, check_book, create_test_env, dmy2time64,
                           env_create_account_structure_alist, env_create_multisplit_transaction, env_transfer,
                           env_transfer_foreign, mnemonic_to_commodity, new_book, out_dir)
from stock_test_data import new_commodity


def create_ifrs_cost_basis_test_data():
    """`create-ifrs-cost-basis-test-data` (`test-ifrs-cost-basis.scm:54-243`)."""
    env = create_test_env()
    usd = mnemonic_to_commodity("USD")
    cad = mnemonic_to_commodity("CAD")
    spy_c = new_commodity("SPY", "NYSE", "SPY", 10000)
    structure = ["Root", {"type": ASSET, "commodity": cad},
                 ["Asset",
                  ["Broker", {"commodity": cad},
                   ["CAD Cash", {"commodity": cad}],
                   ["USD Cash", {"commodity": usd}],
                   ["SPY", {"commodity": spy_c, "type": STOCK}]],
                  ["Current Assets", ["Checking Account"]]],
                 ["Income", {"type": INCOME},
                  ["USD CapGain", {"commodity": usd}],
                  ["USD Interest", {"commodity": usd}]],
                 ["Expenses", {"type": EXPENSE},
                  ["USD Commissions", {"commodity": usd}]],
                 ["Equity", {"type": EQUITY},
                  ["Opening Balances"],
                  ["Opening Balances USD", {"commodity": usd}]]]
    a = env_create_account_structure_alist(env, structure)
    usd_cash, usd_comm, inc_capg, spy = a["USD Cash"], a["USD Commissions"], a["USD CapGain"], a["SPY"]

    # :87-101
    env_transfer(env, 1, 5, 2020, a["Opening Balances"], a["Checking Account"], 200000, description="opening cash")
    env_transfer(env, 6, 5, 2020, a["Checking Account"], a["CAD Cash"], 100000, description="fund trading account")
    env_transfer_foreign(env, 10, 5, 2020, a["CAD Cash"], usd_cash, 100000, 85000, description="Convert CAD to USD")

    # :103-241, each (day, month, year, splits, description) in the test's order; all in CAD.
    for dd, mm, yy, splits, description in [
            (1, 7, 2019, [(usd_cash, "-2000995/100", "-2000995/100", ""), (spy, 20000, 100, "Buy"),
                          (spy, "995/100", 0, "Fee")], "Buy SPY"),
            (11, 12, 2019, [(usd_cash, "-1600995/100", "-1600995/100", ""), (spy, 16000, 50, "Buy"),
                            (spy, "995/100", 0, "Fee")], "Buy SPY"),
            (18, 3, 2020, [(usd_comm, "995/100", "995/100", "Fee"), (usd_cash, "1199005/100", "1199005/100", ""),
                           (inc_capg, "600995/100", "600995/100", ""), (spy, -12000, -75, "Sell"),
                           (spy, "-600995/100", 0, "")], "Sell SPY"),
            (1, 4, 2020, [(usd_cash, "-4200995/100", "-4200995/100", ""), (spy, 42000, 250, "Buy"),
                          (spy, "995/100", 0, "Fee")], "Buy SPY"),
            (16, 4, 2020, [(usd_cash, 2500, 2500), (spy, -2500, 0, "Buy")], "Return of Capital"),
            (2, 5, 2020, [(usd_cash, -47500, -47500, ""), (spy, 47500, 125, "Buy"), (spy, 0, 0, "Fee")], "Buy spy"),
            (11, 5, 2020, [(spy, 0, 450, "Buy")], "stock split"),
            (21, 5, 2020, [(usd_comm, "995/100", "995/100", "Fee"), (usd_cash, "2149005/100", "2149005/100", ""),
                           (inc_capg, "574702/100", "574702/100", ""), (spy, -21500, -135, "Sell"),
                           (spy, "-574702/100", 0, "")], "Sell SPY"),
            (3, 6, 2020, [(usd_cash, -21000, -21000, ""), (spy, 21000, 150, "Buy"), (spy, 0, 0, "Fee")], "Buy spy"),
            (10, 6, 2020, [(usd_comm, "995/100", "995/100", "Fee"), (usd_cash, "12809005/100", "12809005/100", ""),
                           (inc_capg, "1783309/100", "1783309/100", ""), (spy, -128100, -915, "Sell"),
                           (spy, "-1783309/100", 0, "")], "Sell SPY"),
            (10, 6, 2020, [(spy, "995/100", 0, "Fee"), (usd_cash, "1189005/100", "1189005/100", ""),
                           (spy, -11900, -85, "Sell")], "Sell SPY Short"),
            (15, 6, 2020, [(spy, "995/100", 0, "Fee"), (usd_cash, "1104005/100", "1104005/100", ""),
                           (spy, -11050, -65, "Sell")], "Sell SPY Short"),
            (18, 6, 2020, [(spy, 5000, 50, "Sell"), (usd_comm, "995/100", "995/100", "Fee"),
                           (spy, "264337/100", 0, ""), (usd_cash, "-500995/100", "-500995/100", ""),
                           (inc_capg, "-264337/100", "-264337/100", "")], "Buy SPY Close Short"),
            (20, 6, 2020, [(spy, 8000, 100, "Sell"), (usd_comm, "498/100", "498/100", "Fee"),
                           (spy, "728673/100", 0, ""), (usd_cash, "-800498/100", "-800498/100", ""),
                           (inc_capg, "-728673/100", "-728673/100", "")], "Buy SPY Close Short"),
            (21, 6, 2020, [(usd_cash, "-800498/100", "-800498/100", ""), (spy, 8000, 100, "Buy"),
                           (spy, "498/100", 0, "Fee")], "Buy SPY")]:
        env_create_multisplit_transaction(env, dd, mm, yy, splits, description=description, currency=cad)
    return a


def main():
    out = out_dir() / "ifrs-cost-basis.gnucash"
    book = new_book(out)
    create_ifrs_cost_basis_test_data()
    book.save()
    # SPY units after each row (test-ifrs-cost-basis.scm:264-357, "cumul-units"): 100, 150, 75, 325, 325, 450, 900,
    # 765, 915, 0, -85, -150, -100, 0, 100.
    check_book(out, [
        ("Root.Asset.Broker.SPY", 100, dmy2time64(2, 7, 2019)),
        ("Root.Asset.Broker.SPY", 150, dmy2time64(12, 12, 2019)),
        ("Root.Asset.Broker.SPY", 75, dmy2time64(19, 3, 2020)),
        ("Root.Asset.Broker.SPY", 325, dmy2time64(17, 4, 2020)),
        ("Root.Asset.Broker.SPY", 450, dmy2time64(3, 5, 2020)),
        ("Root.Asset.Broker.SPY", 900, dmy2time64(12, 5, 2020)),
        ("Root.Asset.Broker.SPY", 765, dmy2time64(22, 5, 2020)),
        ("Root.Asset.Broker.SPY", 915, dmy2time64(4, 6, 2020)),
        ("Root.Asset.Broker.SPY", -85, dmy2time64(11, 6, 2020)),
        ("Root.Asset.Broker.SPY", -150, dmy2time64(16, 6, 2020)),
        ("Root.Asset.Broker.SPY", -100, dmy2time64(19, 6, 2020)),
        ("Root.Asset.Broker.SPY", 0, dmy2time64(21, 6, 2020)),
        ("Root.Asset.Broker.SPY", 100, None),
        # USD Cash: the 85,000 converted, then each SPY transaction's cash split.
        ("Root.Asset.Broker.USD Cash", "10445049/100", None),
        ("Root.Income.USD CapGain", "491499/25", None),
        ("Root.Expenses.USD Commissions", "2239/50", None),
        ("Root.Asset.Current Assets.Checking Account", 100000, None),
        ("Root.Asset.Broker.CAD Cash", 0, None),
        ("Root.Equity.Opening Balances", -200000, None),
    ])


if __name__ == "__main__":
    main()
