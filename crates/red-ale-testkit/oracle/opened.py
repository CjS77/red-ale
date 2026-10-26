#!/usr/bin/env python3
"""What GnuCash 5.17 says when it opens a book for editing.

Usage (with PYTHONPATH=<5.17 bindings>):
    opened.py <book>

Opens the book as GnuCash's GUI does (SESSION_NORMAL_OPEN), closes it without saving, and prints:
  - unknownFeatures: the warning the GUI shows for features it does not know (`gnc_features_test_unknown`), or null;
  - logged: every WARN, ERROR or CRIT line the engine wrote while opening and closing the book.
"""
import json
import os
import sys
import tempfile

from gnucash import Session, SessionOpenMode, _sw_core_utils

import gnc_engine

# A build without the GUI always warns that its preference transformations file is missing.
NOISE = ("pref_transformations.xml",)


def opened(path):
    log = tempfile.TemporaryFile()
    saved_stderr = os.dup(2)
    os.dup2(log.fileno(), 2)
    try:
        session = Session(("xml://" if "://" not in path else "") + path, SessionOpenMode.SESSION_NORMAL_OPEN)
        try:
            unknown = gnc_engine.unknown_features(session.book)
        finally:
            session.end()
    finally:
        sys.stderr.flush()
        os.dup2(saved_stderr, 2)
    log.seek(0)
    lines = log.read().decode(errors="replace").splitlines()
    logged = [l for l in lines if any(k in l for k in (" WARN ", " ERROR ", " CRIT ")) and not any(n in l for n in NOISE)]
    return {"unknownFeatures": unknown, "logged": logged}


def main(argv):
    if _sw_core_utils.gnc_version() != "5.17":
        sys.exit("the oracle must be GnuCash 5.17")
    print(json.dumps(opened(argv[1])))


if __name__ == "__main__":
    main(sys.argv)
