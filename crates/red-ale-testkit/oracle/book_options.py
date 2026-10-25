#!/usr/bin/env python3
"""GnuCash 5.17 as the oracle for the num field source and the auto read-only threshold.

Commands (run with PYTHONPATH=<5.17 bindings>, TZ=UTC):

  inspect <book>          open the book read-only and print, as GnuCash computes them: both options, the threshold
                          date, the unknown-feature warning GnuCash's GUI would show (null when none), and for every
                          transaction whether it is read-only by date and each split's Number (`gnc_get_num_action`)
  set <book> <on> <days>  set both options as GnuCash's book options dialog does, then save
  slots <book>            print the book's `options` and `features` slot frames from the file
"""
import gzip
import json
import os
import sys
import xml.etree.ElementTree as ET

from gnucash import Session, SessionOpenMode, _sw_core_utils

import gnc_engine


def check_version():
    v = _sw_core_utils.gnc_version()
    if v != "5.17":
        print(json.dumps({"error": f"the oracle must be GnuCash 5.17, found {v}"}))
        sys.exit(3)


def threshold(book):
    """`qof_book_get_autoreadonly_gdate` as YYYY-MM-DD, or None."""
    glib = gnc_engine.ctypes.CDLL("libglib-2.0.so.0")
    f = gnc_engine.engine.qof_book_get_autoreadonly_gdate
    f.restype = gnc_engine.ctypes.c_void_p
    f.argtypes = [gnc_engine.ctypes.c_void_p]
    d = f(gnc_engine.ptr(book))
    if not d:
        return None
    for name in ("g_date_get_year", "g_date_get_month", "g_date_get_day"):
        getattr(glib, name).argtypes = [gnc_engine.ctypes.c_void_p]
    out = f"{glib.g_date_get_year(d):04}-{glib.g_date_get_month(d):02}-{glib.g_date_get_day(d):02}"
    glib.g_date_free.argtypes = [gnc_engine.ctypes.c_void_p]
    glib.g_date_free(d)
    return out


def inspect(path):
    s = Session("xml://" + path, SessionOpenMode.SESSION_READ_ONLY)
    gnc_engine.set_current_session(s)
    try:
        book = s.book
        txns = {}
        for a in book.get_root_account().get_descendants():
            for sp in a.GetSplitList():
                t = sp.GetParent()
                g = t.GetGUID().to_string()
                if g not in txns:
                    txns[g] = {
                        "readOnlyByDate": gnc_engine.is_readonly_by_posted_date(t),
                        "numbers": {x.GetGUID().to_string(): gnc_engine.get_num_action(t, x) or None for x in t.GetSplitList()},
                    }
        return {
            "splitActionForNum": bool(book.use_split_action_for_num_field()),
            "autoReadonlyDays": book.get_num_days_autoreadonly(),
            "threshold": threshold(book),
            "unknownFeatures": gnc_engine.unknown_features(book),
            "transactions": txns,
        }
    finally:
        gnc_engine.clear_current_session()
        s.end()


def set_options(path, on, days):
    s = Session("xml://" + path, SessionOpenMode.SESSION_NORMAL_OPEN)
    try:
        gnc_engine.set_num_source(s.book, on)
        gnc_engine.set_autoreadonly_days(s.book, days)
        s.save()
    finally:
        s.end()


def local(tag):
    return tag.split("}")[-1]


def frame(el):
    out = {}
    for slot in el:
        key = value = None
        for c in slot:
            if local(c.tag) == "key":
                key = c.text
            elif local(c.tag) == "value":
                value = frame(c) if c.get("type") == "frame" else [c.get("type"), c.text]
        out[key] = value
    return out


def slots(path):
    with open(path, "rb") as f:
        data = f.read()
    if data[:2] == b"\x1f\x8b":
        data = gzip.decompress(data)
    root = ET.fromstring(data)
    book = next(c for c in root if local(c.tag) == "book")
    el = next((c for c in book if local(c.tag) == "slots"), None)
    all_slots = frame(el) if el is not None else {}
    return {k: all_slots.get(k) for k in ("options", "features")}


if __name__ == "__main__":
    os.environ.setdefault("TZ", "UTC")
    check_version()
    cmd = sys.argv[1]
    if cmd == "inspect":
        json.dump(inspect(sys.argv[2]), sys.stdout, indent=1, sort_keys=True)
    elif cmd == "set":
        set_options(sys.argv[2], sys.argv[3] == "true", float(sys.argv[4]))
    elif cmd == "slots":
        json.dump(slots(sys.argv[2]), sys.stdout, indent=1, sort_keys=True)
    else:
        raise SystemExit(f"unknown command {cmd}")
