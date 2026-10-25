#!/usr/bin/env python3
"""Let GnuCash 5.17 scrub a book itself, then save it.

Usage: scrub.py <book>

Opening the book already runs GnuCash's load-time scrubs (`5.17:libgnucash/backend/xml/io-gncxml-v2.cpp:282-283`,
`:830-833`): orphans, transaction currencies and split amounts. The script then runs the body of Check & Repair's
imbalance step, `AccountScrubImbalance` (`5.17:libgnucash/engine/Scrub.cpp:353-391`), on every transaction reached
through the account tree, passing the book's root explicitly: `xaccTransScrubOrphans`, `xaccTransScrubCurrency`,
`xaccTransScrubImbalance(trans, root, NULL)`. The bindings do not expose the scrubs, so they are called through ctypes.
"""
import ctypes
import sys

from gnucash import Session, SessionOpenMode, _sw_core_utils

from gnc_engine import engine, ptr


def main(path):
    if _sw_core_utils.gnc_version() != "5.17":
        sys.exit("the oracle must be GnuCash 5.17")
    engine.xaccTransScrubImbalance.argtypes = [ctypes.c_void_p, ctypes.c_void_p, ctypes.c_void_p]
    session = Session("xml://" + path, SessionOpenMode.SESSION_NORMAL_OPEN)
    # As in the GUI, the open book is the current session's: trading parents take its root currency
    # (`5.17:libgnucash/engine/Scrub.cpp:493-516`).
    engine.gnc_set_current_session.argtypes = [ctypes.c_void_p]
    engine.gnc_set_current_session(ptr(session))
    try:
        root = session.book.get_root_account()
        seen, transactions = set(), []
        for account in root.get_descendants():
            for split in account.GetSplitList():
                t = split.GetParent()
                guid = t.GetGUID().to_string()
                if guid not in seen:
                    seen.add(guid)
                    transactions.append(t)
        for t in transactions:
            engine.xaccTransScrubOrphans(ptr(t))
            engine.xaccTransScrubCurrency(ptr(t))
            engine.xaccTransScrubImbalance(ptr(t), ptr(root), None)
        # A no-op rename marks the book dirty even if no scrub changed anything, so save always writes.
        root.SetName(root.GetName())
        session.save()
    finally:
        engine.gnc_set_current_session(None)
        session.end()


if __name__ == "__main__":
    main(sys.argv[1])
