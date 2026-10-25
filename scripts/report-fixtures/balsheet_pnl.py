#!/usr/bin/env python3
# Rebuilt from GnuCash 5.17's gnucash/report/reports/standard/test/test-balsheet-pnl.scm, which carries no licence
# notice of its own; it is part of GnuCash and cited under GnuCash's licence, GPL-2.0-or-later (5.17:LICENSE).
"""Build `reports/balsheet-pnl.gnucash`: the book of `create-test-data` in
`5.17:gnucash/report/reports/standard/test/test-balsheet-pnl.scm:92-166`, with the module-level setup it relies on
(`:51-62`): GBP's user symbol "#" and the FUNDS commodity.

GnuCash's test makes FUNDS with `gnc-commodity-new` but never inserts it into the commodity table (`:55-61`); a
book saved that way names a commodity it does not declare, so here it is inserted.

No date depends on now; every report option in the test is an absolute date.

Usage: TZ=UTC PYTHONPATH=<5.17 site-packages> scripts/report-fixtures/balsheet_pnl.py [out-dir]
"""
from fractions import Fraction

from gnucash import GncCommodity

from engine_extras import (ASSET, EQUITY, INCOME, LIABILITY, STOCK, check_book, create_test_env, dmy2time64,
                           env_create_account_structure_alist, env_transfer, env_transfer_foreign,
                           gnc_pricedb_create, mnemonic_to_commodity, new_book, out_dir)


def create_test_data(usd, gbp, funds):
    """`create-test-data` (`test-balsheet-pnl.scm:92-166`) on the structure of `:64-85`."""
    env = create_test_env()
    structure = ["Root", {"type": ASSET, "commodity": usd},
                 ["Asset",
                  ["Bank1", ["Savings"], ["Bonds"], ["Empty"], ["Current"]],
                  ["House"],
                  ["ForeignBank", {"commodity": gbp}, ["ForeignSavings"]],
                  ["Broker", ["Funds", {"type": STOCK, "commodity": funds}]]],
                 ["Liability", {"type": LIABILITY}, ["Bank2", ["Loan"], ["CreditCard"]]],
                 ["Equity", {"type": EQUITY}],
                 ["Income", {"type": INCOME}, ["Income-GBP", {"commodity": gbp}]]]
    a = env_create_account_structure_alist(env, structure)
    savings, bonds, current, house = a["Savings"], a["Bonds"], a["Current"], a["House"]
    foreignsavings, broker, brokerfunds = a["ForeignSavings"], a["Broker"], a["Funds"]
    loan, creditcard, equity, income, income_gbp = a["Loan"], a["CreditCard"], a["Equity"], a["Income"], a["Income-GBP"]

    # :110-117
    env_transfer(env, 1, 1, 1970, equity, savings, 100)
    env_transfer(env, 1, 1, 1970, equity, bonds, 2000)
    env_transfer(env, 1, 1, 1970, equity, current, 9000)
    env_transfer(env, 1, 1, 1970, equity, house, 100000)
    # :119-129, each adding a GBP price in USD (test-engine-extras.scm:279-286)
    for (dd, mm, usd_amount, gbp_amount, text) in [
            (1, 1, 130, 100, "buy 100GBP at $1.30"), (1, 2, 140, 100, "buy 100GBP at $1.40"),
            (15, 2, -142, -100, "sell 100GBP at $1.42"), (1, 3, 150, 100, "buy 100GBP at $1.50"),
            (1, 4, 155, 100, "buy 100GBP at $1.55"), (15, 4, -157, -100, "sell 100GBP at $1.57"),
            (1, 5, -160, -100, "sell 100GBP at $1.60"), (1, 6, 155, 100, "buy 100GBP at $1.55"),
            (1, 7, -145, -100, "sell 100GBP at $1.45"), (1, 8, 165, 100, "buy 100GBP at $1.65")]:
        env_transfer_foreign(env, dd, mm, 1970, current, foreignsavings, usd_amount, gbp_amount, description=text)
    # :131-132
    env_transfer(env, 1, 1, 1970, equity, broker, 2000)
    # :134-141, each adding a FUNDS price in USD
    for (dd, mm, usd_amount, units, text) in [
            (1, 1, 2000, 10, "buy 10FUND at $200"), (1, 2, 2100, 10, "buy 10FUND at $210"),
            (1, 3, 2250, 10, "buy 10FUND at $225"), (1, 4, 2440, 10, "buy 10FUND at $244"),
            (1, 5, -2640, -10, "sell 10FUND at $264"), (1, 6, -2550, -10, "sell 10FUND at $255"),
            (1, 7, 2500, 10, "buy 10FUND at $250")]:
        env_transfer_foreign(env, dd, mm, 1970, current, brokerfunds, usd_amount, units, description=text)
    # :143-147
    env_transfer(env, 1, 1, 1970, equity, loan, -9000)
    env_transfer(env, 1, 1, 1970, equity, creditcard, -500)
    # :149-161
    for (year, rate) in [(1971, Fraction(150, 101)), (1972, Fraction(169, 102)), (1973, Fraction(178, 103)),
                         (1974, Fraction(187, 104)), (1975, Fraction(196, 105))]:
        gnc_pricedb_create(usd, gbp, dmy2time64(1, 1, year), rate)
    for (year, rate) in [(1971, Fraction(30001, 100)), (1972, Fraction(35002, 101)), (1973, Fraction(40003, 102)),
                         (1974, Fraction(45004, 103)), (1975, Fraction(50005, 104))]:
        gnc_pricedb_create(usd, funds, dmy2time64(1, 1, year), rate)
    # :163-166
    env_transfer(env, 1, 1, 1980, income, current, 250)
    env_transfer(env, 1, 1, 1980, income_gbp, foreignsavings, 500)
    env_transfer_foreign(env, 1, 2, 1980, income_gbp, current, 100, 170, description="earn 100GBP into $170")


def main():
    out = out_dir() / "balsheet-pnl.gnucash"
    book = new_book(out)
    # :51-62
    usd = mnemonic_to_commodity("USD")
    gbp = mnemonic_to_commodity("GBP")
    funds = GncCommodity(book.book, "Funds", "FUNDS", "FUNDS", "FUNDS", 1000)
    book.table.insert(funds)
    gbp.set_user_symbol("#")
    create_test_data(usd, gbp, funds)
    book.save()
    t1971, t1980 = dmy2time64(1, 1, 1971), dmy2time64(1, 3, 1980)
    # The Balance Sheet at 1971-01-01 (:219-272): Bonds $2,000, Current $2,609, Savings $100, Broker $2,000, Funds
    # 30 units, ForeignSavings #200, House $100,000; liabilities $9,500 (:183-185). At 1980-03-01 (:562-568):
    # Current $3,029, ForeignSavings #700. The Income Statement for 1980 (:415-422): Income -$250, Income-GBP -#600.
    check_book(out, [
        ("Root.Asset.Bank1.Bonds", 2000, t1971),
        ("Root.Asset.Bank1.Current", 2609, t1971),
        ("Root.Asset.Bank1.Savings", 100, t1971),
        ("Root.Asset.Bank1.Empty", 0, t1971),
        ("Root.Asset.Broker", 2000, t1971),
        ("Root.Asset.Broker.Funds", 30, t1971),
        ("Root.Asset.ForeignBank.ForeignSavings", 200, t1971),
        ("Root.Asset.House", 100000, t1971),
        ("Root.Liability.Bank2.Loan", -9000, t1971),
        ("Root.Liability.Bank2.CreditCard", -500, t1971),
        ("Root.Equity", -103600, t1971),
        ("Root.Asset.Bank1.Current", 3029, t1980),
        ("Root.Asset.ForeignBank.ForeignSavings", 700, t1980),
        ("Root.Income", -250, None),
        ("Root.Income.Income-GBP", -600, None),
    ])


if __name__ == "__main__":
    main()
