#!/usr/bin/env python3
"""Load a book with the GnuCash 5.17 oracle, mark it dirty with an edit that changes nothing, save it uncompressed
to a new path, and print that path. Used to check that a fixture is in GnuCash 5.17's canonical form.

Usage (with PYTHONPATH pointing at the 5.17 bindings): scripts/oracle-resave.py <book> <out>
"""
import gzip
import os
import shutil
import sys
import tempfile

from gnucash import Session, SessionOpenMode, _sw_core_utils

assert _sw_core_utils.gnc_version() == "5.17", "the oracle must be GnuCash 5.17"


def main(src, dst):
    work = tempfile.mkdtemp()
    path = os.path.join(work, "book.gnucash")
    shutil.copy(src, path)
    s = Session("xml://" + path, SessionOpenMode.SESSION_NORMAL_OPEN)
    root = s.book.get_root_account()
    name = root.GetName()
    for n in (name + " ", name):
        root.BeginEdit()
        root.SetName(n)
        root.CommitEdit()
    s.save()
    s.end()
    with open(path, "rb") as f:
        data = f.read()
    if data[:2] == b"\x1f\x8b":
        data = gzip.decompress(data)
    with open(dst, "wb") as f:
        f.write(data)
    shutil.rmtree(work)


if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2])
