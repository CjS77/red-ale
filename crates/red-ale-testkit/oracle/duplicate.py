#!/usr/bin/env python3
"""Duplicate documents in GnuCash 5.17 and save the book: `gncInvoiceCopy`, then the resets
`gnc_ui_invoice_duplicate` makes when it runs without its dialog (`5.17:gnucash/gnome/dialog-invoice.c:3181-3238`):
active, the ID from `gncInvoiceNextID`, and the opened date and every line's date at the day's neutral time.

Usage: duplicate.py <book> <YYYY-MM-DD> <invoice guid>...

The book is changed in place; run it on a copy. Prints the new documents' GUIDs as a JSON list, in order.
"""
import ctypes
import json
import sys

from gnucash import Session, SessionOpenMode, _sw_core_utils

from gnc_engine import engine, ptr


class GList(ctypes.Structure):
    pass


GList._fields_ = [("data", ctypes.c_void_p), ("next", ctypes.POINTER(GList)), ("prev", ctypes.POINTER(GList))]

VP = ctypes.c_void_p


def fn(name, restype, *argtypes):
    f = getattr(engine, name)
    f.restype = restype
    f.argtypes = list(argtypes)
    return f


copy = fn("gncInvoiceCopy", VP, VP)
set_active = fn("gncInvoiceSetActive", None, VP, ctypes.c_int)
set_id = fn("gncInvoiceSetID", None, VP, ctypes.c_char_p)
next_id = fn("gncInvoiceNextID", ctypes.c_void_p, VP, VP)
get_owner = fn("gncInvoiceGetOwner", VP, VP)
set_opened = fn("gncInvoiceSetDateOpened", None, VP, ctypes.c_int64)
get_entries = fn("gncInvoiceGetEntries", ctypes.POINTER(GList), VP)
set_entry_date = fn("gncEntrySetDate", None, VP, ctypes.c_int64)
dmy_neutral = fn("gnc_dmy2time64_neutral", ctypes.c_int64, ctypes.c_int, ctypes.c_int, ctypes.c_int)
day_neutral = fn("gnc_time64_get_day_neutral", ctypes.c_int64, ctypes.c_int64)
string_to_guid = fn("string_to_guid", ctypes.c_int, ctypes.c_char_p, VP)
lookup = fn("qof_collection_lookup_entity", VP, VP, VP)
get_collection = fn("qof_book_get_collection", VP, VP, ctypes.c_char_p)
guid_of = fn("qof_instance_get_guid", ctypes.POINTER(ctypes.c_ubyte * 16), VP)
libc = ctypes.CDLL(None)
libc.free.argtypes = [VP]


def find_invoice(book, guid):
    raw = (ctypes.c_ubyte * 16)()
    string_to_guid(guid.encode(), raw)
    return lookup(get_collection(book, b"gncInvoice"), raw)


def duplicate(book, old, when):
    new = copy(old)
    set_active(new, 1)
    owner = get_owner(new)
    id_ptr = next_id(book, owner)
    set_id(new, ctypes.cast(id_ptr, ctypes.c_char_p).value)
    libc.free(id_ptr)
    set_opened(new, when)
    # As g_list_foreach does: the next node's address is read before the call, which may re-sort the list. (A ctypes
    # field read shares the node's memory, so the address is copied out.)
    node = get_entries(new)
    while node:
        following = ctypes.cast(node.contents.next, ctypes.c_void_p).value
        set_entry_date(node.contents.data, when)
        node = ctypes.cast(following, ctypes.POINTER(GList)) if following else None
    return bytes(guid_of(new).contents).hex()


def main(path, date, *guids):
    if _sw_core_utils.gnc_version() != "5.17":
        sys.exit("the oracle must be GnuCash 5.17")
    y, m, d = (int(x) for x in date.split("-"))
    when = day_neutral(dmy_neutral(d, m, y))
    session = Session("xml://" + path, SessionOpenMode.SESSION_NORMAL_OPEN)
    try:
        book = ptr(session.book).value
        out = [duplicate(book, find_invoice(book, g), when) for g in guids]
        session.save()
    finally:
        session.end()
    print(json.dumps(out))


if __name__ == "__main__":
    main(*sys.argv[1:])
