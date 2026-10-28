#!/usr/bin/env python3
"""Build `reports/balsheet-pnl-extras.gnucash` and `reports/balsheet-pnl-extras-trading.gnucash`: small books for the
hand-worked checks of the Multicolumn Balance Sheet and P&L (`design/notes/report-statements-charts.md` sections 5.5
and 11). No GnuCash test builds them; they add what `balsheet-pnl.gnucash` lacks: expense accounts, a closing
transaction, placeholders, an account with no splits, a priced foreign currency (EUR) and an unpriced one (CHF), and,
in the second book, trading accounts.

Both books are made with the helpers of `engine_extras.py` (fixed GUIDs, entered dates the pinned now plus one second
per transaction, uncompressed XML). No date depends on now: every transaction and price is dated 2023 or early 2024,
before the pinned now of 2024-03-15.

Accounts, in creation order (GUID `a…1` to `a…10`), all USD unless noted; the five top-level parents are
placeholders:

    Assets                 a…1      Equity                 a…8      Expenses               a…e
      Checking             a…2        Opening Balances     a…9        Rent                 a…f
      Savings (no splits)  a…3        Retained Earnings    a…a        Groceries            a…10
      Euro Account (EUR)   a…4      Income                 a…b
      Swiss Account (CHF)  a…5        Salary               a…c
    Liabilities            a…6        Consulting CHF (CHF) a…d
      Credit Card (CREDIT) a…7

Transactions (`env-transfer` moves the amount from its first account to its second):

    2023-01-01  Opening Balances -> Checking         1000
    2023-01-31  Salary -> Checking                   3000
    2023-02-05  Checking -> Rent                     1200
    2023-02-10  Credit Card -> Groceries              150
    2023-03-01  Checking -> Euro Account             $550 = EUR 500, adding the price EUR in USD 11/10 on 2023-03-01
    2023-06-15  Consulting CHF -> Swiss Account      CHF 800, in CHF: no price, and CHF never gets one
    2023-12-31  "Closing Entries", flagged closing: Salary +3000, Rent -1200, Groceries -150, Retained Earnings -1650
                (Consulting CHF is left open, so its income stays in the books unconverted)
    2024-01-31  Salary -> Checking                   3000
    2024-02-05  Checking -> Rent                     1200
    2024-02-20  Credit Card -> Groceries               80
    2024-02-25  Checking -> Credit Card               150
    price       EUR in USD 6/5 on 2024-01-01

Balances (debit positive):

    account               end of 2023      end (2024-03-15)
    Checking                    2250            3900
    Savings                        0               0
    Euro Account             EUR 500         EUR 500       $550 at cost; $600 at 6/5
    Swiss Account            CHF 800         CHF 800       no price
    Credit Card                 -150             -80
    Opening Balances           -1000           -1000
    Retained Earnings          -1650           -1650
    Salary                         0           -3000
    Consulting CHF          CHF -800        CHF -800
    Rent                           0            1200
    Groceries                      0              80

At 2024-03-15 in USD at 6/5, leaving CHF out: assets 3900 + 600 = 4500 = liabilities 80 + equity 2650 + 2024's net
income 1720 + unrealized gain 50 (EUR 500 bought for $550, now $600).

The trading book sets "Use Trading Accounts" (the book's `trading-accts` property, `options/Accounts/Use Trading
Accounts` = "t", `5.17:libgnucash/engine/qofbook.cpp:203-206`) before any transaction, with GnuCash's current session
set to it as the GUI has it (`find_root_currency`, `5.17:libgnucash/engine/Scrub.cpp:493-516`). Committing the EUR
purchase then makes GnuCash add the trading splits itself (`5.17:libgnucash/engine/Transaction.cpp:1548-1558`,
`Scrub.cpp:518-575`, `:715-782`), creating Trading (placeholder, USD), Trading.CURRENCY (placeholder, USD),
Trading.CURRENCY.USD and Trading.CURRENCY.EUR. Those accounts and splits come with random GUIDs; this script then
gives them the next fixed ones, in tree and split order, so the book is the same on every run: Trading `a…11`,
Trading.CURRENCY `a…12`, Trading.CURRENCY.EUR `a…13`, Trading.CURRENCY.USD `a…14`. The purchase's
trading splits are USD +550 (value $550) and EUR -500 (value -$550); no other transaction mixes commodities.

Usage: TZ=UTC PYTHONPATH=<5.17 site-packages> scripts/report-fixtures/balsheet_pnl_extras.py [out-dir]
"""
from engine_extras import (ASSET, CREDIT, EQUITY, EXPENSE, INCOME, LIABILITY, check_book, create_test_env,
                           dmy2time64, env_create_account_structure_alist, env_create_multisplit_transaction,
                           env_transfer, env_transfer_foreign, gnc_pricedb_create, mnemonic_to_commodity, new_book,
                           out_dir, set_guid)
from gnc_engine import clear_current_session, engine, gobject, ptr, set_current_session


def structure(env):
    """The four top-level trees, each with its parent made a placeholder."""
    usd, eur, chf = (mnemonic_to_commodity(m) for m in ("USD", "EUR", "CHF"))
    trees = [
        ["Assets", {"type": ASSET, "commodity": usd},
         ["Checking"], ["Savings"], ["Euro Account", {"commodity": eur}], ["Swiss Account", {"commodity": chf}]],
        ["Liabilities", {"type": LIABILITY, "commodity": usd}, ["Credit Card", {"type": CREDIT}]],
        ["Equity", {"type": EQUITY, "commodity": usd}, ["Opening Balances"], ["Retained Earnings"]],
        ["Income", {"type": INCOME, "commodity": usd}, ["Salary"], ["Consulting CHF", {"commodity": chf}]],
        ["Expenses", {"type": EXPENSE, "commodity": usd}, ["Rent"], ["Groceries"]],
    ]
    accts = {}
    for tree in trees:
        accts.update(env_create_account_structure_alist(env, tree))
        parent = accts[tree[0]]
        parent.BeginEdit()
        parent.SetPlaceholder(True)
        parent.CommitEdit()
    return accts


def transactions(env, a, book):
    """The transactions and the price of the module docstring, in order."""
    env_transfer(env, 1, 1, 2023, a["Opening Balances"], a["Checking"], 1000, description="Opening balance")
    env_transfer(env, 31, 1, 2023, a["Salary"], a["Checking"], 3000, description="Salary")
    env_transfer(env, 5, 2, 2023, a["Checking"], a["Rent"], 1200, description="Rent")
    env_transfer(env, 10, 2, 2023, a["Credit Card"], a["Groceries"], 150, description="Groceries")
    purchase = env_transfer_foreign(env, 1, 3, 2023, a["Checking"], a["Euro Account"], 550, 500,
                                    description="Buy EUR 500 at $1.10")
    fix_trading_guids(book, purchase)
    env_transfer(env, 15, 6, 2023, a["Consulting CHF"], a["Swiss Account"], 800, description="Consulting")
    env_create_multisplit_transaction(
        env, 31, 12, 2023,
        [(a["Salary"], 3000, 3000), (a["Rent"], -1200, -1200), (a["Groceries"], -150, -150),
         (a["Retained Earnings"], -1650, -1650)],
        description="Closing Entries").SetIsClosingTxn(True)
    env_transfer(env, 31, 1, 2024, a["Salary"], a["Checking"], 3000, description="Salary")
    env_transfer(env, 5, 2, 2024, a["Checking"], a["Rent"], 1200, description="Rent")
    env_transfer(env, 20, 2, 2024, a["Credit Card"], a["Groceries"], 80, description="Groceries")
    env_transfer(env, 25, 2, 2024, a["Checking"], a["Credit Card"], 150, description="Pay credit card")
    gnc_pricedb_create(mnemonic_to_commodity("USD"), mnemonic_to_commodity("EUR"), dmy2time64(1, 1, 2024), "6/5")


def is_fixed(book, kind, instance):
    """Whether the instance has one of the GUIDs this script has given out for `kind`."""
    fixed = {f"{kind}{n:0{32 - len(kind)}x}" for n in range(book.counts.get(kind, 0))}
    return instance.GetGUID().to_string() in fixed


def fix_trading_guids(book, txn):
    """Give the accounts and splits GnuCash's commit added the next fixed GUIDs, accounts in tree order."""
    if book.book.use_trading_accounts():
        for acct in book.root.get_descendants():
            if not is_fixed(book, "a", acct):
                set_guid(acct, book.guid("a"))
    for split in txn.GetSplitList():
        if not is_fixed(book, "d", split):
            set_guid(split, book.guid("d"))


def use_trading_accounts(book):
    """Turn on "Use Trading Accounts" as the book options dialog does, through the book's property."""
    engine.qof_book_begin_edit(ptr(book))
    gobject.g_object_set(ptr(book), b"trading-accts", b"t", None)
    engine.qof_book_commit_edit(ptr(book))


def build(out, trading):
    book = new_book(out)
    set_current_session(book.session)
    try:
        if trading:
            use_trading_accounts(book.book)
        env = create_test_env()
        transactions(env, structure(env), book)
    finally:
        clear_current_session()
    book.save()


def main():
    plain, trading = out_dir() / "balsheet-pnl-extras.gnucash", out_dir() / "balsheet-pnl-extras-trading.gnucash"
    build(plain, trading=False)
    build(trading, trading=True)
    end_2023 = dmy2time64(1, 1, 2024)
    balances = [
        ("Assets.Checking", 2250, end_2023), ("Assets.Checking", 3900, None),
        ("Assets.Savings", 0, None),
        ("Assets.Euro Account", 500, None),
        ("Assets.Swiss Account", 800, None),
        ("Liabilities.Credit Card", -150, end_2023), ("Liabilities.Credit Card", -80, None),
        ("Equity.Opening Balances", -1000, None),
        ("Equity.Retained Earnings", -1650, None),
        ("Income.Salary", 0, end_2023), ("Income.Salary", -3000, None),
        ("Income.Consulting CHF", -800, None),
        ("Expenses.Rent", 0, end_2023), ("Expenses.Rent", 1200, None),
        ("Expenses.Groceries", 0, end_2023), ("Expenses.Groceries", 80, None),
    ]
    check_book(plain, balances)
    check_book(trading, balances + [
        ("Trading.CURRENCY.USD", 550, None),
        ("Trading.CURRENCY.EUR", -500, None),
        ("a0000000000000000000000000000011", 0, None),
    ])


if __name__ == "__main__":
    main()
