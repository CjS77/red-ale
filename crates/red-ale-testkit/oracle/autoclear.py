#!/usr/bin/env python3
"""GnuCash 5.17's auto-clear: run `gnc_account_get_autoclear_splits` on an account for each target, as the auto-clear
window does (no date limit, `5.17:gnucash/gnome/window-autoclear.c:142-146`).

Usage: autoclear.py <book> <account guid> <target num/denom>...

Prints a JSON list, one entry per target: {"code": GnuCash's Autoclear code (0 found, 1 already at the balance,
2 multiple, 3 timeout, 4 unreachable), "message": the error message or null, "splits": the split GUIDs found, sorted}.
"""
import ctypes
import json
import sys

from gnucash import Session, SessionOpenMode, _sw_core_utils

from gnc_engine import LIB_DIR, GncNumeric, engine, ptr

app_utils = ctypes.CDLL(str(LIB_DIR / "libgnc-app-utils.so"))


class GList(ctypes.Structure):
    pass


GList._fields_ = [("data", ctypes.c_void_p), ("next", ctypes.POINTER(GList)), ("prev", ctypes.POINTER(GList))]


class GError(ctypes.Structure):
    _fields_ = [("domain", ctypes.c_uint32), ("code", ctypes.c_int), ("message", ctypes.c_char_p)]


autoclear = app_utils.gnc_account_get_autoclear_splits
autoclear.restype = ctypes.POINTER(GList)
autoclear.argtypes = [ctypes.c_void_p, GncNumeric, ctypes.c_int64, ctypes.POINTER(ctypes.POINTER(GError)), ctypes.c_double]
engine.qof_instance_get_guid.restype = ctypes.POINTER(ctypes.c_ubyte * 16)
engine.qof_instance_get_guid.argtypes = [ctypes.c_void_p]

INT64_MAX = 2**63 - 1


def split_guid(split):
    return bytes(engine.qof_instance_get_guid(split).contents).hex()


def run(account, target):
    num, denom = (int(x) for x in target.split("/"))
    error = ctypes.POINTER(GError)()
    node = autoclear(account, GncNumeric(num, denom), INT64_MAX, ctypes.byref(error), 5.0)
    splits = []
    while node:
        splits.append(split_guid(node.contents.data))
        node = node.contents.next
    if error:
        return {"code": error.contents.code, "message": error.contents.message.decode(), "splits": []}
    return {"code": 0, "message": None, "splits": sorted(splits)}


def main(path, account_guid, *targets):
    if _sw_core_utils.gnc_version() != "5.17":
        sys.exit("the oracle must be GnuCash 5.17")
    session = Session("xml://" + path, SessionOpenMode.SESSION_READ_ONLY)
    try:
        root = session.book.get_root_account()
        acc = next(a for a in root.get_descendants() if a.GetGUID().to_string() == account_guid)
        out = [run(ptr(acc), t) for t in targets]
    finally:
        session.end()
    print(json.dumps(out, sort_keys=True))


if __name__ == "__main__":
    main(*sys.argv[1:])
