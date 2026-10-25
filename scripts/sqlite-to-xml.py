#!/usr/bin/env python3
"""Convert a GnuCash SQLite book to an uncompressed XML book with the GnuCash 5.17 oracle (decompressed on output), then load and save the
XML once more so it is in GnuCash 5.17's canonical form.

Usage (with PYTHONPATH pointing at the 5.17 bindings):
    scripts/sqlite-to-xml.py <in.sqlite.gnucash> <out.gnucash>
"""
import gzip
import os
import shutil
import sys
import tempfile

from gnucash import Session, SessionOpenMode, _sw_core_utils

assert _sw_core_utils.gnc_version() == "5.17", "the oracle must be GnuCash 5.17"


def touch(book):
    """Mark the book dirty with an edit that changes nothing, so save() writes it."""
    root = book.get_root_account()
    name = root.GetName()
    root.BeginEdit()
    root.SetName(name + " ")
    root.CommitEdit()
    root.BeginEdit()
    root.SetName(name)
    root.CommitEdit()


def main(src, dst):
    work = tempfile.mkdtemp()
    sql = os.path.join(work, "in.gnucash")
    shutil.copy(src, sql)
    out = os.path.join(work, "out.gnucash")
    s_in = Session("sqlite3://" + sql, SessionOpenMode.SESSION_READ_ONLY)
    s_out = Session("xml://" + out, SessionOpenMode.SESSION_NEW_OVERWRITE)
    s_out.swap_data(s_in)
    touch(s_out.book)
    s_out.save()
    s_out.end()
    s_in.end()
    # Load and save again so the file is in 5.17's canonical form.
    s = Session("xml://" + out, SessionOpenMode.SESSION_NORMAL_OPEN)
    touch(s.book)
    s.save()
    s.end()
    with open(out, "rb") as f:
        data = f.read()
    if data[:2] == b"\x1f\x8b":
        data = gzip.decompress(data)
    with open(dst, "wb") as f:
        f.write(data)


if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2])
