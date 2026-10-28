#!/usr/bin/env python3
"""Build the SQLite copies of the canonical XML fixtures with the GnuCash 5.17 oracle.

The recipe is GnuCash's own Save As (design/notes/sql-backend.md §10.1, after 5.17:gnucash/gnome-utils/gnc-file.c:1640-1729):
open the XML book read-only, begin a new sqlite3:// session with SESSION_NEW_STORE, swap the data in, mark the book
dirty (the bindings lack qof_book_mark_session_dirty, so it is called through ctypes), save, and end both sessions.
The book is then opened once read-only, which sets the ISO-dates Book feature as every GnuCash load does
(5.17:libgnucash/backend/dbi/gnc-backend-dbi.cpp:879-880), so the fixture is in the state GnuCash 5.17 leaves it.
The journals the bindings write beside the book are deleted. Sessions are ended, never destroyed: destroy() after an
SQLite session aborts the oracle.

Every source is copied to a temporary directory first, so the fixtures themselves are never opened by GnuCash.

Usage:
    scripts/sql-fixtures/build.py [out-dir]          (default fixtures/sqlite)

The bindings are found through RED_ALE_ORACLE_PYTHONPATH, else $HOME/.local/opt/gnucash-5.17/lib/python3.12/site-packages.
Writes <out-dir>/<dir>/<name> for each fixture and <out-dir>/MANIFEST.json, which records the oracle's GnuCash,
libdbi and SQLite versions and the SHA-256 of each source.
"""
import ctypes
import glob
import hashlib
import json
import os
import shutil
import sys
import tempfile

BINDINGS = os.environ.get(
    "RED_ALE_ORACLE_PYTHONPATH", os.path.expanduser("~/.local/opt/gnucash-5.17/lib/python3.12/site-packages")
)
sys.path.insert(0, BINDINGS)

from gnucash import Session, SessionOpenMode, _sw_core_utils  # noqa: E402

assert _sw_core_utils.gnc_version() == "5.17", "the oracle must be GnuCash 5.17"

HERE = os.path.dirname(os.path.abspath(__file__))
FIXTURES = os.path.normpath(os.path.join(HERE, "..", "..", "fixtures"))

# The canonical books of fixtures/README.md: GnuCash 5.17 loads and saves each back byte for byte.
SOURCES = [
    "gnucash/sample1.gnucash",
    "gnucash/bug-746937-sx-split-order.gnucash",
    "synthetic/business.gnucash",
    "synthetic/business-editable.gnucash",
    "synthetic/rich.gnucash",
] + sorted(
    os.path.relpath(p, FIXTURES)
    for d in ("piecash", "reports")
    for p in glob.glob(os.path.join(FIXTURES, d, "*.gnucash"))
    if not open(p, "rb").read(16).startswith(b"SQLite format 3\0")
)

ENGINE = ctypes.CDLL(os.path.join(BINDINGS, "..", "..", "libgnc-engine.so"))
ENGINE.qof_book_mark_session_dirty.argtypes = [ctypes.c_void_p]


def library_versions():
    """The libdbi and SQLite libraries GnuCash's DBI backend runs on."""
    sqlite = ctypes.CDLL("libsqlite3.so.0")
    sqlite.sqlite3_libversion.restype = ctypes.c_char_p
    dbi = ctypes.CDLL("libdbi.so.1")
    dbi.dbi_version.restype = ctypes.c_char_p
    return {"libdbi": dbi.dbi_version().decode(), "sqlite": sqlite.sqlite3_libversion().decode()}


def convert(src, dst):
    """Save the XML book `src` as the SQLite book `dst`, as GnuCash's Save As does."""
    old = Session("xml://" + src, SessionOpenMode.SESSION_READ_ONLY)
    new = Session()
    new.begin("sqlite3://" + dst, SessionOpenMode.SESSION_NEW_STORE)
    new.swap_data(old)
    ENGINE.qof_book_mark_session_dirty(int(new.book.instance))
    new.save()
    errors = new.pop_all_errors()
    new.end()
    old.end()
    if errors:
        raise RuntimeError(f"saving {dst}: {errors}")
    # One read-only load sets the ISO-dates feature, as GnuCash's first open of a Save As result does.
    reopened = Session("sqlite3://" + dst, SessionOpenMode.SESSION_READ_ONLY)
    errors = reopened.pop_all_errors()
    reopened.end()
    if errors:
        raise RuntimeError(f"reopening {dst}: {errors}")


def sha256(path):
    with open(path, "rb") as f:
        return hashlib.sha256(f.read()).hexdigest()


def main(out):
    manifest = {"gnucash": _sw_core_utils.gnc_version(), **library_versions(), "books": {}}
    for name in SOURCES:
        work = tempfile.mkdtemp()
        try:
            src = os.path.join(work, "in.gnucash")
            dst = os.path.join(work, "out.gnucash")
            shutil.copy(os.path.join(FIXTURES, name), src)
            convert(src, dst)
            target = os.path.join(out, name)
            os.makedirs(os.path.dirname(target), exist_ok=True)
            shutil.copy(dst, target)
            manifest["books"][name] = {"source": name, "sourceSha256": sha256(os.path.join(FIXTURES, name))}
            print(f"{name}: {os.path.getsize(target)} bytes", file=sys.stderr)
        finally:
            # The journals and lock files GnuCash leaves go with the work directory.
            shutil.rmtree(work)
    with open(os.path.join(out, "MANIFEST.json"), "w") as f:
        json.dump(manifest, f, indent=2, sort_keys=True)
        f.write("\n")


if __name__ == "__main__":
    main(sys.argv[1] if len(sys.argv) > 1 else os.path.join(FIXTURES, "sqlite"))
