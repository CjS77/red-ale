#!/usr/bin/env python3
"""Budgets through GnuCash 5.17's engine. The Python bindings do not wrap `gnc-budget.h`
(`5.17:bindings/python/gnucash_core.i:48-84`), so its functions are called through ctypes.

Usage:
  budgets.py make <book>   add a budget to the book with everything a budget can hold, and save it
  budgets.py dump <book>   print, as JSON, every budget as GnuCash reads it: fields, recurrence, and each account's
                           amounts and notes in the counted periods

`make` adds "Plan" (GUID 00000000000000000000000000b0d6e7): description "", every 2 months from Saturday 2024-03-02
with weekend adjustment forward (`recurrenceSet` keeps one for month, end of month and year only,
`5.17:libgnucash/engine/Recurrence.cpp:128-137`), 20 periods while it sets an amount at period 15 and then 12, so
that amount is kept but no longer read (`design/notes/engine-budgets.md` §1.3). Food gets `1234/100` at 0, an explicit `0/1` at 1, a note at 2
and an empty note at 3; Fees `-5/3` at 11. It also adds a note to "Budget 2024", the budget the book already has.
"""
import ctypes
import json
import sys

from gnucash import Session, SessionOpenMode, _sw_core_utils

from gnc_engine import GncNumeric, engine, ptr

PLAN = "00000000000000000000000000b0d6e7"
FOOD = "1feca2abf92347288c268ebf93753ec4"
FEES = "a8cf578f585b4708856fdf5b8928e552"
EXISTING = "0123456789abcdef0123456789abcdef"
WEEKEND_ADJ_FORWARD = 2
PERIOD_MONTH = 3

VP, U, I, S = ctypes.c_void_p, ctypes.c_uint, ctypes.c_int, ctypes.c_char_p


def declare(name, restype, *argtypes):
    f = getattr(engine, name)
    f.restype = restype
    f.argtypes = list(argtypes)
    return f


budget_new = declare("gnc_budget_new", VP, VP)
lookup = declare("gnc_budget_lookup", VP, ctypes.c_char_p, VP)
set_name = declare("gnc_budget_set_name", None, VP, S)
set_num_periods = declare("gnc_budget_set_num_periods", None, VP, U)
set_recurrence = declare("gnc_budget_set_recurrence", None, VP, VP)
set_value = declare("gnc_budget_set_account_period_value", None, VP, VP, U, GncNumeric)
set_note = declare("gnc_budget_set_account_period_note", None, VP, VP, U, S)
get_name = declare("gnc_budget_get_name", S, VP)
get_description = declare("gnc_budget_get_description", S, VP)
get_num_periods = declare("gnc_budget_get_num_periods", U, VP)
get_recurrence = declare("gnc_budget_get_recurrence", VP, VP)
is_set = declare("gnc_budget_is_account_period_value_set", ctypes.c_bool, VP, VP, U)
get_value = declare("gnc_budget_get_account_period_value", GncNumeric, VP, VP, U)
get_note = declare("gnc_budget_get_account_period_note", S, VP, VP, U)
period_start = declare("gnc_budget_get_period_start_date", ctypes.c_int64, VP, U)
get_mult = declare("recurrenceGetMultiplier", U, VP)
get_period_type = declare("recurrenceGetPeriodType", I, VP)
get_weekend_adjust = declare("recurrenceGetWeekendAdjust", I, VP)
period_type_name = declare("recurrencePeriodTypeToString", S, I)
weekend_adjust_name = declare("recurrenceWeekendAdjustToString", S, I)
recurrence_set = declare("recurrenceSet", None, VP, ctypes.c_uint16, I, VP, I)
get_collection = declare("qof_book_get_collection", VP, VP, S)
collection_foreach = declare("qof_collection_foreach", None, VP, VP, VP)
instance_guid = declare("qof_instance_get_guid", VP, VP)
guid_to_string = declare("guid_to_string", S, VP)
glib = ctypes.CDLL("libglib-2.0.so.0")
glib.g_date_new_dmy.restype = VP
glib.g_date_new_dmy.argtypes = [ctypes.c_uint8, I, ctypes.c_uint16]
FOREACH = ctypes.CFUNCTYPE(None, VP, VP)


def guid(text):
    """A GncGUID: its 16 bytes."""
    return ctypes.create_string_buffer(bytes.fromhex(text), 16)


def account(book, text):
    """The account with this GUID."""
    return next(ptr(a) for a in book.get_root_account().get_descendants() if a.GetGUID().to_string() == text)


def budget(book, text):
    found = lookup(guid(text), ptr(book))
    if not found:
        sys.exit(f"no budget {text}")
    return found


def make(path):
    session = Session("xml://" + path, SessionOpenMode.SESSION_NORMAL_OPEN)
    try:
        book = session.book
        food, fees = account(book, FOOD), account(book, FEES)
        b = budget_new(ptr(book))
        engine.qof_instance_set_guid.argtypes = [VP, ctypes.c_char_p]
        engine.qof_instance_set_guid(b, guid(PLAN))
        set_name(b, b"Plan")
        recurrence = ctypes.create_string_buffer(64)
        recurrence_set(recurrence, 2, PERIOD_MONTH, glib.g_date_new_dmy(2, 3, 2024), WEEKEND_ADJ_FORWARD)
        set_recurrence(b, recurrence)
        set_num_periods(b, 20)
        set_value(b, food, 15, GncNumeric(100, 1))
        set_num_periods(b, 12)
        set_value(b, food, 0, GncNumeric(1234, 100))
        set_value(b, food, 1, GncNumeric(0, 1))
        set_note(b, food, 2, "Groceries & <more>".encode())
        set_note(b, food, 3, b"")
        set_value(b, fees, 11, GncNumeric(-5, 3))
        set_note(budget(book, EXISTING), food, 11, b"December")
        session.save()
    finally:
        session.end()


def read(book, b):
    r = get_recurrence(b)
    accounts = book.get_root_account().get_descendants()
    amounts, notes = {}, {}
    for a in accounts:
        key = a.GetGUID().to_string()
        for p in range(get_num_periods(b)):
            if is_set(b, ptr(a), p):
                n = get_value(b, ptr(a), p)
                amounts.setdefault(key, {})[str(p)] = f"{n.num}/{n.denom}"
            note = get_note(b, ptr(a), p)
            if note:
                notes.setdefault(key, {})[str(p)] = note.decode()
    return {
        "name": get_name(b).decode(),
        "description": get_description(b).decode(),
        "numPeriods": get_num_periods(b),
        "recurrence": {
            "mult": get_mult(r),
            "periodType": period_type_name(get_period_type(r)).decode(),
            "weekendAdjust": weekend_adjust_name(get_weekend_adjust(r)).decode(),
            "periodStarts": [period_start(b, p) for p in range(3)],
        },
        "amounts": amounts,
        "notes": notes,
    }


def dump(path):
    session = Session("xml://" + path, SessionOpenMode.SESSION_READ_ONLY)
    try:
        book = session.book
        found = []
        callback = FOREACH(lambda instance, _: found.append(instance))
        collection_foreach(get_collection(ptr(book), b"Budget"), callback, None)
        budgets = {guid_to_string(instance_guid(b)).decode(): read(book, b) for b in found}
        print(json.dumps(budgets, sort_keys=True, indent=1))
    finally:
        session.end()


if __name__ == "__main__":
    if _sw_core_utils.gnc_version() != "5.17":
        sys.exit("the oracle must be GnuCash 5.17")
    {"make": make, "dump": dump}[sys.argv[1]](sys.argv[2])
