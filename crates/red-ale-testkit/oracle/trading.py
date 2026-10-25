#!/usr/bin/env python3
"""Trading accounts in GnuCash 5.17.

Usage:
    trading.py post <book> <transactions.json>   GnuCash posts the transactions itself, then saves.
    trading.py show <book>                       The trading tree and every transaction's splits, in stored order.
    trading.py check <book>                      Which transactions GnuCash finds unbalanced, which its imbalance scrub
                                                 would change, and what the engine logged while doing it.

A transaction in the JSON is {"description", "currency": "NS::MN", "date": "YYYY-MM-DD", "splits": [{"account": full
name joined by ":", "value": [num, denom], "amount": [num, denom]}]}. Posting commits each one, so GnuCash's
`xaccTransCommitEdit` adds the trading splits (`5.17:libgnucash/engine/Transaction.cpp:1548-1560`).

The current session is set to the opened book first, as the GUI sets it: `find_root_currency` reads the root of the
current session's book when it makes trading parents (`5.17:libgnucash/engine/Scrub.cpp:493-516`).
"""
import ctypes
import json
import math
import os
import sys
import tempfile

from gnucash import GncNumeric, Session, SessionOpenMode, Split, Transaction, _sw_core_utils

from gnc_engine import engine, ptr

engine.gnc_set_current_session.argtypes = [ctypes.c_void_p]
engine.xaccTransIsBalanced.argtypes = [ctypes.c_void_p]
engine.xaccTransIsBalanced.restype = ctypes.c_int
engine.xaccTransScrubImbalance.argtypes = [ctypes.c_void_p, ctypes.c_void_p, ctypes.c_void_p]


def full_name(a):
    """The account's full name with ":", whatever separator the bindings' preferences give."""
    parts = []
    while a is not None and not a.is_root():
        parts.append(a.GetName())
        a = a.get_parent()
    return ":".join(reversed(parts))


def frac(n):
    """A bindings GncNumeric as a reduced "num/denom", as dump.py prints amounts."""
    g = math.gcd(n.num(), n.denom()) or 1
    return f"{n.num() // g}/{n.denom() // g}"


def commodity_id(cm):
    return cm.get_unique_name() if cm is not None else None


def transactions(root):
    seen, out = set(), []
    for a in root.get_descendants():
        for s in a.GetSplitList():
            t = s.GetParent()
            g = t.GetGUID().to_string()
            if g not in seen:
                seen.add(g)
                out.append(t)
    return out


def splits(t):
    return [[full_name(s.GetAccount()), frac(s.GetValue()), frac(s.GetAmount())] for s in t.GetSplitList()]


def open_book(path, mode):
    session = Session("xml://" + path, mode)
    engine.gnc_set_current_session(ptr(session))
    return session


def close_book(session, save):
    try:
        if save:
            session.save()
    finally:
        engine.gnc_set_current_session(None)
        session.end()


def post(path, spec_path):
    with open(spec_path) as f:
        specs = json.load(f)
    session = open_book(path, SessionOpenMode.SESSION_NORMAL_OPEN)
    try:
        book = session.book
        accounts = {full_name(a): a for a in book.get_root_account().get_descendants()}
        table = book.get_table()
        for spec in specs:
            t = Transaction(book)
            t.BeginEdit()
            t.SetCurrency(table.lookup(*spec["currency"].split("::")))
            year, month, day = (int(x) for x in spec["date"].split("-"))
            t.SetDate(day, month, year)
            t.SetDescription(spec["description"])
            for s in spec["splits"]:
                split = Split(book)
                split.SetParent(t)
                split.SetAccount(accounts[s["account"]])
                split.SetValue(GncNumeric(*s["value"]))
                split.SetAmount(GncNumeric(*s["amount"]))
            t.CommitEdit()
    except BaseException:
        close_book(session, False)
        raise
    close_book(session, True)


def show(path):
    session = open_book(path, SessionOpenMode.SESSION_READ_ONLY)
    try:
        root = session.book.get_root_account()
        trading = [
            {
                "name": full_name(a),
                "commodity": commodity_id(a.GetCommodity()),
                "scu": a.GetCommoditySCU(),
                "placeholder": bool(a.GetPlaceholder()),
            }
            for a in root.get_descendants()
            if a.GetType() == 14
        ]
        txns = {t.GetDescription(): {"currency": commodity_id(t.GetCurrency()), "splits": splits(t)} for t in transactions(root)}
        print(json.dumps({"trading": sorted(trading, key=lambda a: a["name"]), "transactions": txns}, indent=1, sort_keys=True))
    finally:
        close_book(session, False)


def check(path):
    log = tempfile.TemporaryFile()
    saved_stderr = os.dup(2)
    os.dup2(log.fileno(), 2)
    try:
        session = open_book(path, SessionOpenMode.SESSION_NORMAL_OPEN)
        try:
            root = session.book.get_root_account()
            accounts_before = len(root.get_descendants())
            unbalanced, changed = [], []
            for t in transactions(root):
                name = t.GetDescription()
                if not engine.xaccTransIsBalanced(ptr(t)):
                    unbalanced.append(name)
                before = sorted(splits(t))
                engine.xaccTransScrubImbalance(ptr(t), ptr(root), None)
                if sorted(splits(t)) != before:
                    changed.append(name)
            accounts_added = len(root.get_descendants()) - accounts_before
        finally:
            close_book(session, False)
    finally:
        sys.stderr.flush()
        os.dup2(saved_stderr, 2)
    log.seek(0)
    lines = log.read().decode(errors="replace").splitlines()
    # A build without the GUI always warns that its preference transformations file is missing.
    noise = ("pref_transformations.xml",)
    logged = [l for l in lines if any(k in l for k in (" WARN ", " ERROR ", " CRIT ")) and not any(n in l for n in noise)]
    print(json.dumps({"unbalanced": unbalanced, "changed": changed, "accountsAdded": accounts_added, "logged": logged}))


def main(argv):
    if _sw_core_utils.gnc_version() != "5.17":
        sys.exit("the oracle must be GnuCash 5.17")
    command, path = argv[1], argv[2]
    if command == "post":
        post(path, argv[3])
    elif command == "show":
        show(path)
    elif command == "check":
        check(path)
    else:
        sys.exit("unknown command " + command)


if __name__ == "__main__":
    main(sys.argv)
