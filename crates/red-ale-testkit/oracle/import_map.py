#!/usr/bin/env python3
"""GnuCash 5.17's own Import map lookups on a book.

Usage (with PYTHONPATH=<5.17 bindings>):
    import_map.py <book> <queries.json>

The queries file holds {"bayes": [{"source": guid, "tokens": [...]}, ...],
"plain": [{"source": guid, "category": str, "key": str}, ...]}. The book is opened read-only, and for each query the
account GnuCash finds is printed (its GUID, or null), with which of the two Bayesian features the book declares:
    {"features": [...], "bayes": [...], "plain": [...]}

The bindings take no GList of strings, so `gnc_account_imap_find_account_bayes` is called through ctypes
(`libgnucash/engine/Account.cpp:5511`), with the tokens in the order given.
"""
import ctypes
import json
import sys

from gnucash import Session, SessionOpenMode

import gnc_engine

glib = ctypes.CDLL("libglib-2.0.so.0")
glib.g_list_append.restype = ctypes.c_void_p
glib.g_list_append.argtypes = [ctypes.c_void_p, ctypes.c_void_p]
glib.g_list_free.argtypes = [ctypes.c_void_p]
engine = gnc_engine.engine
engine.gnc_account_imap_find_account_bayes.restype = ctypes.c_void_p
engine.gnc_account_imap_find_account_bayes.argtypes = [ctypes.c_void_p, ctypes.c_void_p]
engine.gnc_account_imap_find_account.restype = ctypes.c_void_p
engine.gnc_account_imap_find_account.argtypes = [ctypes.c_void_p, ctypes.c_char_p, ctypes.c_char_p]
engine.qof_instance_get_guid.restype = ctypes.c_void_p
engine.qof_instance_get_guid.argtypes = [ctypes.c_void_p]
engine.guid_to_string.restype = ctypes.c_char_p
engine.guid_to_string.argtypes = [ctypes.c_void_p]
engine.gnc_features_check_used.restype = ctypes.c_bool
engine.gnc_features_check_used.argtypes = [ctypes.c_void_p, ctypes.c_char_p]

# The two Bayesian features GnuCash declares (`libgnucash/engine/gnc-features.h:58-59`).
BAYES_FEATURES = ["Account GUID based Bayesian data", "Account GUID based bayesian with flat KVP"]


def account(book, guid):
    return next(a for a in book.get_root_account().get_descendants() if a.GetGUID().to_string() == guid)


def guid_of(acc_ptr):
    if not acc_ptr:
        return None
    return engine.guid_to_string(engine.qof_instance_get_guid(acc_ptr)).decode()


def find_bayes(book, query):
    source = account(book, query["source"])
    buffers = [ctypes.create_string_buffer(t.encode()) for t in query["tokens"]]
    tokens = None
    for b in buffers:
        tokens = glib.g_list_append(tokens, ctypes.cast(b, ctypes.c_void_p))
    found = engine.gnc_account_imap_find_account_bayes(gnc_engine.ptr(source), tokens)
    glib.g_list_free(tokens)
    return guid_of(found)


def find_plain(book, query):
    source = account(book, query["source"])
    found = engine.gnc_account_imap_find_account(gnc_engine.ptr(source), query["category"].encode(), query["key"].encode())
    return guid_of(found)


def main(path, queries_path):
    with open(queries_path) as f:
        queries = json.load(f)
    session = Session("xml://" + path, SessionOpenMode.SESSION_READ_ONLY)
    try:
        book = session.book
        out = {
            "features": [name for name in BAYES_FEATURES if engine.gnc_features_check_used(gnc_engine.ptr(book), name.encode())],
            "bayes": [find_bayes(book, q) for q in queries.get("bayes", [])],
            "plain": [find_plain(book, q) for q in queries.get("plain", [])],
        }
    finally:
        session.end()
    print(json.dumps(out))


if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2])
