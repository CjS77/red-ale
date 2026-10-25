#!/usr/bin/env python3
"""GnuCash 5.17's view of an account's reconciliation, read through the engine's own getters.

Usage: reconcile.py <book> <account guid> <statement time, epoch seconds>

Prints JSON: the reconcile-info slots as GnuCash reads them (a slot of the wrong type reads as absent), and the
balances the reconcile window starts from (`5.17:gnucash/gnome/window-reconcile.cpp:1635-1698`, `:757-947`).
"""
import ctypes
import json
import sys

from gnucash import Session, SessionOpenMode, _sw_core_utils

from gnc_engine import GncNumeric, engine, frac, numeric_fn, ptr


def main(path, account_guid, statement_time):
    if _sw_core_utils.gnc_version() != "5.17":
        sys.exit("the oracle must be GnuCash 5.17")
    session = Session("xml://" + path, SessionOpenMode.SESSION_READ_ONLY)
    try:
        root = session.book.get_root_account()
        acc = next(a for a in root.get_descendants() if a.GetGUID().to_string() == account_guid)
        p = ptr(acc)
        last, pdate = ctypes.c_int64(), ctypes.c_int64()
        months, days = ctypes.c_int(), ctypes.c_int()
        pbal = GncNumeric()
        has_last = engine.xaccAccountGetReconcileLastDate(p, ctypes.byref(last))
        has_interval = engine.xaccAccountGetReconcileLastInterval(p, ctypes.byref(months), ctypes.byref(days))
        has_pdate = engine.xaccAccountGetReconcilePostponeDate(p, ctypes.byref(pdate))
        has_pbal = engine.xaccAccountGetReconcilePostponeBalance(p, ctypes.byref(pbal))
        as_of = numeric_fn("xaccAccountGetBalanceAsOfDate", ctypes.c_void_p, ctypes.c_int64)
        out = {
            "lastDate": last.value if has_last else None,
            "lastInterval": [months.value, days.value] if has_interval else None,
            "postponeDate": pdate.value if has_pdate else None,
            "postponeBalance": frac(pbal) if has_pbal else None,
            "includeChildren": bool(engine.xaccAccountGetReconcileChildrenStatus(p)),
            "reconciledBalance": frac(numeric_fn("xaccAccountGetReconciledBalance", ctypes.c_void_p)(p)),
            "clearedBalance": frac(numeric_fn("xaccAccountGetClearedBalance", ctypes.c_void_p)(p)),
            "balanceAsOf": frac(as_of(p, int(statement_time))),
        }
    finally:
        session.end()
    print(json.dumps(out, sort_keys=True))


if __name__ == "__main__":
    main(*sys.argv[1:4])
