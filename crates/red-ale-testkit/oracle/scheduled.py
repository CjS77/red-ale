#!/usr/bin/env python3
"""Scheduled transactions through GnuCash 5.17's engine. The Python bindings do not wrap `SchedXaction.h`, so its
functions are called through ctypes on `libgnc-engine.so`, which links no Guile. `libgnc-expressions.so` loads Guile
and is never loaded here: no formula is evaluated.

Usage:
  scheduled.py make <book>   add a scheduled transaction with every field a file can hold but slots, and a template
                             transaction with formulas and a marker split, and save the book
  scheduled.py dump <book>   print, as JSON, every scheduled transaction as GnuCash reads it, with its template
                             transactions' splits and their `sched-xaction` properties
  scheduled.py cases <book>  add the scheduled transactions of CASES (temporal state and schedule only) and save
  scheduled.py next <book>   print, as JSON, each scheduled transaction's `xaccSchedXactionGetNextInstance` with no
                             state, the occurrences stepped from its temporal state, and the next instance of each
                             deferred instance

Every command checks, before it exits, that no Guile library was mapped into the process.

`make` adds "Loan & rates" (GUID 000000000000000000000000005c4ed1): disabled, auto-create with notify, 3 days to
create and 2 to remind in advance, instance count 7, from 2024-01-31, last occurrence 2024-05-31, 12 occurrences of
which 7 remain, a schedule of two recurrences (end of month adjusted back, every 2 weeks) and two deferred instances
(its state as set, and that state advanced once, `gnc_sx_incr_temporal_state`). The template transaction, in USD, has
a debit with a formula of variables, a credit whose numeric GnuCash would compute, and a marker split.
"""
import ctypes
import json
import sys

from gnucash import Session, SessionOpenMode, _sw_core_utils

from gnc_engine import GncNumeric, engine, gobject, ptr

LOAN = "000000000000000000000000005c4ed1"
FOOD = "1feca2abf92347288c268ebf93753ec4"
CASH = "c080059e1aec4c0b9ad4671e80965686"
(PERIOD_ONCE, PERIOD_DAY, PERIOD_WEEK, PERIOD_MONTH, PERIOD_END_OF_MONTH, PERIOD_NTH_WEEKDAY, PERIOD_LAST_WEEKDAY,
 PERIOD_YEAR) = range(8)
WEEKEND_ADJ_NONE, WEEKEND_ADJ_BACK, WEEKEND_ADJ_FORWARD = 0, 1, 2

VP, I, S = ctypes.c_void_p, ctypes.c_int, ctypes.c_char_p
glib = ctypes.CDLL("libglib-2.0.so.0")


class GList(ctypes.Structure):
    pass


GList._fields_ = [("data", VP), ("next", ctypes.POINTER(GList)), ("prev", ctypes.POINTER(GList))]


class TmpState(ctypes.Structure):
    """`SXTmpStateData` (`libgnucash/engine/SchedXaction.h:130-136`); a GDate is 8 bytes."""
    _fields_ = [("last_date", ctypes.c_uint64), ("num_occur_rem", I), ("num_inst", I)]


def declare(lib, name, restype, *argtypes):
    f = getattr(lib, name)
    f.restype = restype
    f.argtypes = list(argtypes)
    return f


def e(name, restype, *argtypes):
    return declare(engine, name, restype, *argtypes)


sx_malloc = e("xaccSchedXactionMalloc", VP, VP)
set_guid = e("qof_instance_set_guid", None, VP, S)
set_name = e("xaccSchedXactionSetName", None, VP, S)
set_enabled = e("xaccSchedXactionSetEnabled", None, VP, I)
set_auto = e("xaccSchedXactionSetAutoCreate", None, VP, I, I)
set_adv_create = e("xaccSchedXactionSetAdvanceCreation", None, VP, I)
set_adv_remind = e("xaccSchedXactionSetAdvanceReminder", None, VP, I)
set_instances = e("gnc_sx_set_instance_count", None, VP, I)
set_start = e("xaccSchedXactionSetStartDate", None, VP, VP)
set_last = e("xaccSchedXactionSetLastOccurDate", None, VP, VP)
set_num_occur = e("xaccSchedXactionSetNumOccur", None, VP, I)
set_rem_occur = e("xaccSchedXactionSetRemOccur", None, VP, I)
set_schedule = e("gnc_sx_set_schedule", None, VP, VP)
create_state = e("gnc_sx_create_temporal_state", VP, VP)
incr_state = e("gnc_sx_incr_temporal_state", None, VP, VP)
add_defer = e("gnc_sx_add_defer_instance", None, VP, VP)
get_sxes = e("gnc_book_get_schedxactions", VP, VP)
add_sx = e("gnc_sxes_add_sx", None, VP, VP)
recurrence_set = e("recurrenceSet", None, VP, ctypes.c_uint16, I, VP, I)
next_instance = e("xaccSchedXactionGetNextInstance", ctypes.c_uint64, VP, VP)
set_end = e("xaccSchedXactionSetEndDate", None, VP, VP)

get_name = e("xaccSchedXactionGetName", S, VP)
get_enabled = e("xaccSchedXactionGetEnabled", I, VP)
get_auto = e("xaccSchedXactionGetAutoCreate", None, VP, ctypes.POINTER(I), ctypes.POINTER(I))
get_adv_create = e("xaccSchedXactionGetAdvanceCreation", I, VP)
get_adv_remind = e("xaccSchedXactionGetAdvanceReminder", I, VP)
get_instances = e("gnc_sx_get_instance_count", I, VP, VP)
get_start = e("xaccSchedXactionGetStartDate", VP, VP)
get_last = e("xaccSchedXactionGetLastOccurDate", VP, VP)
has_end = e("xaccSchedXactionHasEndDate", I, VP)
get_end = e("xaccSchedXactionGetEndDate", VP, VP)
has_occur = e("xaccSchedXactionHasOccurDef", I, VP)
get_num_occur = e("xaccSchedXactionGetNumOccur", I, VP)
get_rem_occur = e("xaccSchedXactionGetRemOccur", I, VP)
get_schedule = e("gnc_sx_get_schedule", ctypes.POINTER(GList), VP)
get_deferred = e("gnc_sx_get_defer_instances", ctypes.POINTER(GList), VP)
get_splits = e("xaccSchedXactionGetSplits", ctypes.POINTER(GList), VP)
get_mult = e("recurrenceGetMultiplier", ctypes.c_uint, VP)
get_period_type = e("recurrenceGetPeriodType", I, VP)
get_weekend_adjust = e("recurrenceGetWeekendAdjust", I, VP)
get_recurrence_date = e("recurrenceGetDate", ctypes.c_uint64, VP)
period_type_name = e("recurrencePeriodTypeToString", S, I)
weekend_adjust_name = e("recurrenceWeekendAdjustToString", S, I)

get_collection = e("qof_book_get_collection", VP, VP, S)
collection_foreach = e("qof_collection_foreach", None, VP, VP, VP)
instance_guid = e("qof_instance_get_guid", VP, VP)
guid_to_string = e("guid_to_string", S, VP)
account_name = e("xaccAccountGetName", S, VP)
malloc_trans = e("xaccMallocTransaction", VP, VP)
trans_begin = e("xaccTransBeginEdit", None, VP)
trans_commit = e("xaccTransCommitEdit", None, VP)
trans_set_currency = e("xaccTransSetCurrency", None, VP, VP)
trans_set_description = e("xaccTransSetDescription", None, VP, S)
trans_set_num = e("xaccTransSetNum", None, VP, S)
trans_get_description = e("xaccTransGetDescription", S, VP)
trans_get_num = e("xaccTransGetNum", S, VP)
trans_get_currency = e("xaccTransGetCurrency", VP, VP)
commodity_name = e("gnc_commodity_get_unique_name", S, VP)
split_parent = e("xaccSplitGetParent", VP, VP)
malloc_split = e("xaccMallocSplit", VP, VP)
split_set_parent = e("xaccSplitSetParent", None, VP, VP)
split_set_account = e("xaccSplitSetAccount", None, VP, VP)
split_set_memo = e("xaccSplitSetMemo", None, VP, S)
split_set_value = e("xaccSplitSetValue", None, VP, GncNumeric)
split_set_amount = e("xaccSplitSetAmount", None, VP, GncNumeric)
split_get_memo = e("xaccSplitGetMemo", S, VP)
split_get_value = e("xaccSplitGetValue", GncNumeric, VP)
split_get_amount = e("xaccSplitGetAmount", GncNumeric, VP)
split_get_account = e("xaccSplitGetAccount", VP, VP)

glib.g_date_new_dmy.restype = VP
glib.g_date_new_dmy.argtypes = [ctypes.c_uint8, I, ctypes.c_uint16]
glib.g_list_append.restype = VP
glib.g_list_append.argtypes = [VP, VP]
glib.g_malloc0.restype = VP
glib.g_malloc0.argtypes = [ctypes.c_size_t]
for name, restype in [("g_date_valid", I), ("g_date_get_year", ctypes.c_uint16), ("g_date_get_month", I), ("g_date_get_day", ctypes.c_uint8)]:
    declare(glib, name, restype, VP)
gobject.g_object_get.restype = None
gobject.g_object_set.restype = None
FOREACH = ctypes.CFUNCTYPE(None, VP, VP)


def guid(text):
    return ctypes.create_string_buffer(bytes.fromhex(text), 16)


def guid_text(p):
    return guid_to_string(p).decode() if p else None


def date(y, m, d):
    return glib.g_date_new_dmy(d, m, y)


def date_text(p):
    if not p or not glib.g_date_valid(p):
        return None
    return f"{glib.g_date_get_year(p):04}-{glib.g_date_get_month(p):02}-{glib.g_date_get_day(p):02}"


def glist(head):
    items = []
    while head:
        items.append(head.contents.data)
        head = head.contents.next
    return items


def recurrence(mult, period_type, start, weekend_adjust):
    """A recurrence the scheduled transaction owns and frees."""
    r = glib.g_malloc0(64)
    recurrence_set(r, mult, period_type, start, weekend_adjust)
    return r


def template_account(sx):
    out = VP()
    gobject.g_object_get(VP(sx), b"template-account", ctypes.byref(out), None)
    return out.value


def template_split(book, trans, acct, memo, frame):
    s = malloc_split(book)
    split_set_parent(s, trans)
    split_set_account(s, acct)
    split_set_memo(s, memo)
    split_set_value(s, GncNumeric(0, 1))
    split_set_amount(s, GncNumeric(0, 1))
    for key, value in frame:
        if key == "sx-account":
            gobject.g_object_set(VP(s), b"sx-account", guid(value), None)
        elif key.endswith("numeric"):
            gobject.g_object_set(VP(s), key.encode(), ctypes.byref(GncNumeric(*value)), None)
        else:
            gobject.g_object_set(VP(s), key.encode(), value.encode(), None)


def make(path):
    session = Session("xml://" + path, SessionOpenMode.SESSION_NORMAL_OPEN)
    try:
        book = session.book
        b = ptr(book)
        sx = sx_malloc(b)
        set_guid(sx, guid(LOAN))
        set_name(sx, "Loan & rates".encode())
        set_enabled(sx, 0)
        set_auto(sx, 1, 1)
        set_adv_create(sx, 3)
        set_adv_remind(sx, 2)
        set_instances(sx, 7)
        set_start(sx, date(2024, 1, 31))
        set_last(sx, date(2024, 5, 31))
        set_num_occur(sx, 12)
        set_rem_occur(sx, 7)
        schedule = glib.g_list_append(None, recurrence(1, PERIOD_END_OF_MONTH, date(2024, 1, 31), WEEKEND_ADJ_BACK))
        schedule = glib.g_list_append(schedule, recurrence(2, PERIOD_WEEK, date(2024, 1, 3), WEEKEND_ADJ_NONE))
        set_schedule(sx, schedule)
        add_defer(sx, create_state(sx))
        later = create_state(sx)
        incr_state(sx, later)
        add_defer(sx, later)
        add_sx(get_sxes(b), sx)

        usd = ptr(book.get_table().lookup("CURRENCY", "USD"))
        acct = template_account(sx)
        trans = malloc_trans(b)
        trans_begin(trans)
        trans_set_currency(trans, usd)
        trans_set_description(trans, "Loan payment".encode())
        trans_set_num(trans, b"7")
        template_split(b, trans, acct, b"interest", [
            ("sx-account", FOOD), ("sx-credit-formula", ""), ("sx-credit-numeric", (0, 1)),
            ("sx-debit-formula", "ipmt( rate / 12 : i : 360 : -principal : 0 : 0 )"), ("sx-debit-numeric", (0, 1)),
        ])
        template_split(b, trans, acct, b"from cash", [
            ("sx-account", CASH), ("sx-credit-formula", "1234.50"), ("sx-credit-numeric", (123450, 100)),
            ("sx-debit-formula", ""), ("sx-debit-numeric", (0, 1)),
        ])
        template_split(b, trans, acct, b"marker", [("sx-account", CASH), ("sx-credit-formula", ""), ("sx-debit-formula", "")])
        trans_commit(trans)
        session.save()
    finally:
        session.end()


def numeric(n):
    return f"{n.num}/{n.denom}"


def split_property(split, name, kind):
    out = kind()
    gobject.g_object_get(VP(split), name.encode(), ctypes.byref(out), None)
    if kind is S:
        return out.value.decode() if out.value is not None else None
    if not out.value:
        return None
    if name == "sx-account":
        return guid_to_string(out.value).decode()
    n = ctypes.cast(out.value, ctypes.POINTER(GncNumeric)).contents
    return numeric(n)


def read_split(s):
    trans = split_parent(s)
    num = trans_get_num(trans)
    return {
        "transaction": guid_text(instance_guid(trans)),
        "description": trans_get_description(trans).decode(),
        "num": num.decode() if num else "",
        "currency": commodity_name(trans_get_currency(trans)).decode(),
        "split": guid_text(instance_guid(s)),
        "memo": split_get_memo(s).decode(),
        "value": numeric(split_get_value(s)),
        "amount": numeric(split_get_amount(s)),
        "templateAccount": guid_text(instance_guid(split_get_account(s))),
        "account": split_property(s, "sx-account", VP),
        "creditFormula": split_property(s, "sx-credit-formula", S),
        "debitFormula": split_property(s, "sx-debit-formula", S),
        "creditNumeric": split_property(s, "sx-credit-numeric", VP),
        "debitNumeric": split_property(s, "sx-debit-numeric", VP),
        "shares": split_property(s, "sx-shares", S),
    }


def read_deferred(p):
    state = ctypes.cast(p, ctypes.POINTER(TmpState)).contents
    last = ctypes.c_uint64(state.last_date)
    return {"last": date_text(ctypes.addressof(last)), "remOccur": state.num_occur_rem, "instanceCount": state.num_inst}


def read_recurrence(r):
    start = ctypes.c_uint64(get_recurrence_date(r))
    return {
        "mult": get_mult(r),
        "periodType": period_type_name(get_period_type(r)).decode(),
        "weekendAdjust": weekend_adjust_name(get_weekend_adjust(r)).decode(),
        "start": date_text(ctypes.addressof(start)),
    }


def read(sx):
    auto, notify = I(), I()
    get_auto(sx, ctypes.byref(auto), ctypes.byref(notify))
    acct = template_account(sx)
    return {
        "name": get_name(sx).decode(),
        "enabled": bool(get_enabled(sx)),
        "autoCreate": bool(auto.value),
        "autoCreateNotify": bool(notify.value),
        "advanceCreateDays": get_adv_create(sx),
        "advanceRemindDays": get_adv_remind(sx),
        "instanceCount": get_instances(sx, None),
        "start": date_text(get_start(sx)),
        "last": date_text(get_last(sx)),
        "end": date_text(get_end(sx)) if has_end(sx) else None,
        "occurrences": [get_num_occur(sx), get_rem_occur(sx)] if has_occur(sx) else None,
        "templateAccount": {"guid": guid_text(instance_guid(acct)), "name": account_name(acct).decode()},
        "schedule": [read_recurrence(r) for r in glist(get_schedule(sx))],
        "deferred": [read_deferred(d) for d in glist(get_deferred(sx))],
        "splits": [read_split(s) for s in glist(get_splits(sx))],
    }


def dump(path):
    session = Session("xml://" + path, SessionOpenMode.SESSION_READ_ONLY)
    try:
        found = []
        callback = FOREACH(lambda instance, _: found.append(instance))
        collection_foreach(get_collection(ptr(session.book), b"SchedXaction"), callback, None)
        print(json.dumps({guid_text(instance_guid(sx)): read(sx) for sx in found}, sort_keys=True, indent=1))
    finally:
        session.end()


# (guid, name, start, last, end, (total, remaining), instance count, [(mult, period type, start, weekend)],
#  deferred states as (remaining, instance count), taken before the last occurrence is set)
CASES = [
    ("0000000000000000000000000000ca01", "Weekend", (2025, 5, 31), None, None, None, 1,
     [(1, PERIOD_MONTH, (2025, 5, 31), WEEKEND_ADJ_BACK)], []),
    ("0000000000000000000000000000ca02", "Ends", (2025, 1, 1), (2025, 2, 1), None, (2, 0), 3,
     [(1, PERIOD_MONTH, (2025, 1, 1), WEEKEND_ADJ_NONE)], []),
    ("0000000000000000000000000000ca03", "EndDate", (2025, 1, 1), (2025, 2, 1), (2025, 2, 1), None, 3,
     [(1, PERIOD_MONTH, (2025, 1, 1), WEEKEND_ADJ_NONE)], []),
    ("0000000000000000000000000000ca04", "Defer", (2025, 1, 1), (2025, 2, 1), None, (6, 4), 3,
     [(1, PERIOD_MONTH, (2025, 1, 1), WEEKEND_ADJ_NONE)], [(6, 1)]),
    ("0000000000000000000000000000ca05", "Twice", (2025, 1, 1), (2025, 1, 15), None, None, 3,
     [(1, PERIOD_MONTH, (2025, 1, 1), WEEKEND_ADJ_NONE), (1, PERIOD_MONTH, (2025, 1, 15), WEEKEND_ADJ_NONE)], []),
    ("0000000000000000000000000000ca06", "Once", (2026, 3, 9), None, None, None, 1,
     [(0, PERIOD_ONCE, (2026, 3, 9), WEEKEND_ADJ_NONE)], []),
    ("0000000000000000000000000000ca07", "Daily", (2025, 2, 26), None, None, (5, 5), 0,
     [(1, PERIOD_DAY, (2025, 2, 26), WEEKEND_ADJ_NONE)], []),
    ("0000000000000000000000000000ca08", "Fortnight", (2024, 1, 3), (2024, 2, 14), (2024, 6, 30), None, 4,
     [(2, PERIOD_WEEK, (2024, 1, 3), WEEKEND_ADJ_NONE)], []),
    ("0000000000000000000000000000ca09", "Weekdays", (2025, 1, 14), None, None, (9, 9), 1,
     [(1, PERIOD_NTH_WEEKDAY, (2025, 1, 14), WEEKEND_ADJ_NONE), (2, PERIOD_LAST_WEEKDAY, (2025, 1, 28), WEEKEND_ADJ_NONE)],
     []),
    ("0000000000000000000000000000ca0a", "Leap", (2024, 2, 29), None, None, None, 1,
     [(1, PERIOD_YEAR, (2024, 2, 29), WEEKEND_ADJ_FORWARD)], []),
    ("0000000000000000000000000000ca0b", "Late start", (2025, 1, 31), (2020, 1, 1), None, None, 6,
     [(1, PERIOD_END_OF_MONTH, (2025, 1, 31), WEEKEND_ADJ_FORWARD)], []),
]


def cases(path):
    session = Session("xml://" + path, SessionOpenMode.SESSION_NORMAL_OPEN)
    try:
        b = ptr(session.book)
        for (g, name, start, last, end, occur, count, recurrences, deferred) in CASES:
            sx = sx_malloc(b)
            set_guid(sx, guid(g))
            set_name(sx, name.encode())
            set_start(sx, date(*start))
            schedule = None
            for (mult, pt, rstart, wadj) in recurrences:
                schedule = glib.g_list_append(schedule, recurrence(mult, pt, date(*rstart), wadj))
            set_schedule(sx, schedule)
            for (rem, inst) in deferred:
                state = create_state(sx)
                tmp = ctypes.cast(state, ctypes.POINTER(TmpState)).contents
                tmp.num_occur_rem, tmp.num_inst = rem, inst
                add_defer(sx, state)
            if last:
                set_last(sx, date(*last))
            if end:
                set_end(sx, date(*end))
            if occur:
                set_num_occur(sx, occur[0])
                set_rem_occur(sx, occur[1])
            set_instances(sx, count)
            add_sx(get_sxes(b), sx)
        session.save()
    finally:
        session.end()


def next_text(sx, state):
    d = ctypes.c_uint64(next_instance(sx, state))
    return date_text(ctypes.addressof(d))


def stepped(sx, count):
    """Occurrences from the temporal state, each with the state's instance count and remaining before it."""
    state = create_state(sx)
    tmp = ctypes.cast(state, ctypes.POINTER(TmpState)).contents
    out = []
    while len(out) < count:
        d = next_text(sx, state)
        if d is None:
            break
        out.append([d, tmp.num_inst, tmp.num_occur_rem])
        incr_state(sx, state)
    return out


def nexts(path):
    session = Session("xml://" + path, SessionOpenMode.SESSION_READ_ONLY)
    try:
        found = []
        callback = FOREACH(lambda instance, _: found.append(instance))
        collection_foreach(get_collection(ptr(session.book), b"SchedXaction"), callback, None)
        print(json.dumps({guid_text(instance_guid(sx)): {
            "null": next_text(sx, None),
            "steps": stepped(sx, 15),
            "deferred": [next_text(sx, d) for d in glist(get_deferred(sx))],
        } for sx in found}, sort_keys=True, indent=1))
    finally:
        session.end()


def no_guile():
    with open("/proc/self/maps") as maps:
        if "libguile" in maps.read():
            sys.exit("Guile was loaded")


if __name__ == "__main__":
    if _sw_core_utils.gnc_version() != "5.17":
        sys.exit("the oracle must be GnuCash 5.17")
    {"make": make, "dump": dump, "cases": cases, "next": nexts}[sys.argv[1]](sys.argv[2])
    no_guile()
