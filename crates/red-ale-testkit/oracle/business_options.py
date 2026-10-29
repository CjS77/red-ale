#!/usr/bin/env python3
"""GnuCash 5.17 as the oracle for the business book options: the Business tab's strings, the Tax tab's Tax Number and
the Default Invoice Report.

Commands (run with PYTHONPATH=<5.17 bindings>, TZ=UTC):

  read <book>         open the book read-only and print each option as GnuCash reads it: the strings through
                      `qof_book_get_string_option` (what `gnc:book-get-option-value` reads for the invoice), the
                      Default Invoice Report through `qof_book_get_default_invoice_report_guid` and `-name`, and the
                      unknown-feature warning GnuCash's GUI would show (null when none)
  set <book> <json>   set the options a JSON object names, by slot path, through `qof_book_set_string_option`, and
                      `defaultInvoiceReport` ([guid, name]) through `qof_book_set_default_invoice_report`; then save
"""
import ctypes
import json
import os
import sys

from gnucash import Session, SessionOpenMode, _sw_core_utils
import gnucash.gnucash_core_c as core

import gnc_engine

PATHS = [
    "options/Business/Company Name",
    "options/Business/Company Address",
    "options/Business/Company Contact Person",
    "options/Business/Company Phone Number",
    "options/Business/Company Fax Number",
    "options/Business/Company Email Address",
    "options/Business/Company Website URL",
    "options/Business/Company ID",
    "options/Tax/Tax Number",
]


def check_version():
    v = _sw_core_utils.gnc_version()
    if v != "5.17":
        print(json.dumps({"error": f"the oracle must be GnuCash 5.17, found {v}"}))
        sys.exit(3)


def uri(path):
    """The session URI for the book at path: SQLite by its file header, XML otherwise."""
    with open(path, "rb") as f:
        sqlite = f.read(16) == b"SQLite format 3\x00"
    return ("sqlite3://" if sqlite else "xml://") + path


def get_string_option(book, path):
    f = gnc_engine.engine.qof_book_get_string_option
    f.restype = ctypes.c_char_p
    f.argtypes = [ctypes.c_void_p, ctypes.c_char_p]
    out = f(gnc_engine.ptr(book), path.encode())
    return out.decode() if out is not None else None


def read(path):
    s = Session(uri(path), SessionOpenMode.SESSION_READ_ONLY)
    try:
        book = s.book
        return {
            "strings": {p: get_string_option(book, p) for p in PATHS},
            "defaultInvoiceReport": {
                "guid": core.qof_book_get_default_invoice_report_guid(book.instance),
                "name": core.qof_book_get_default_invoice_report_name(book.instance),
            },
            "unknownFeatures": gnc_engine.unknown_features(book),
        }
    finally:
        s.end()


def set_options(path, values):
    s = Session(uri(path), SessionOpenMode.SESSION_NORMAL_OPEN)
    try:
        book = s.book
        f = gnc_engine.engine.qof_book_set_string_option
        f.argtypes = [ctypes.c_void_p, ctypes.c_char_p, ctypes.c_char_p]
        for key, value in values.items():
            if key == "defaultInvoiceReport":
                core.qof_book_set_default_invoice_report(book.instance, value[0], value[1])
            else:
                f(gnc_engine.ptr(book), key.encode(), value.encode())
        s.save()
    finally:
        s.end()


if __name__ == "__main__":
    os.environ.setdefault("TZ", "UTC")
    check_version()
    cmd = sys.argv[1]
    if cmd == "read":
        json.dump(read(sys.argv[2]), sys.stdout, indent=1, sort_keys=True)
    elif cmd == "set":
        set_options(sys.argv[2], json.loads(sys.argv[3]))
    else:
        raise SystemExit(f"unknown command {cmd}")
