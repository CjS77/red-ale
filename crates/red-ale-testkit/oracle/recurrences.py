#!/usr/bin/env python3
"""Recurrence arithmetic through GnuCash 5.17's engine. The Python bindings do not wrap `Recurrence.h`, so its
functions are called through ctypes on libgnc-engine.so, which links no Guile
(`design/notes/engine-scheduled-transactions.md` §9).

Usage:
  recurrences.py   print, as JSON, GnuCash's results for a fixed table of recurrences and reference dates

Each recurrence is made two ways: by `recurrenceSet` ("set", normalised, multipliers 0-3) and field by field, as the
file readers do ("raw", multipliers 1-3). For each it prints the stored fields, the next instance after every
reference date, the first instances from the day before the start (as a scheduled transaction asks), instances 0-4
(`recurrenceNthInstance`) and, in UTC, the bounds of periods 0-2 (`recurrenceGetPeriodTime`). It also prints
`recurrenceListNextInstance` for a few lists. The table is deterministic.
"""
import ctypes
import datetime
import json

from gnucash import _sw_core_utils

from gnc_engine import engine

glib = ctypes.CDLL("libglib-2.0.so.0")
VP, U, I = ctypes.c_void_p, ctypes.c_uint, ctypes.c_int

PERIOD_TYPES = ["once", "day", "week", "month", "end of month", "nth weekday", "last weekday", "year"]
WEEKEND_ADJUSTS = ["none", "back", "forward"]


class GDate(ctypes.Structure):
    _fields_ = [("raw", ctypes.c_uint32 * 2)]


class Recurrence(ctypes.Structure):
    """`Recurrence` (`5.17:libgnucash/engine/Recurrence.h:76-82`)."""
    _fields_ = [("start", GDate), ("ptype", I), ("mult", ctypes.c_uint16), ("wadj", I)]


def declare(lib, name, restype, *argtypes):
    f = getattr(lib, name)
    f.restype = restype
    f.argtypes = list(argtypes)
    return f


recurrence_set = declare(engine, "recurrenceSet", None, VP, ctypes.c_uint16, I, VP, I)
next_instance = declare(engine, "recurrenceNextInstance", None, VP, VP, VP)
nth_instance = declare(engine, "recurrenceNthInstance", None, VP, U, VP)
period_time = declare(engine, "recurrenceGetPeriodTime", ctypes.c_int64, VP, U, I)
list_next_instance = declare(engine, "recurrenceListNextInstance", None, VP, VP, VP)
set_julian = declare(glib, "g_date_set_julian", None, VP, ctypes.c_uint32)
get_julian = declare(glib, "g_date_get_julian", ctypes.c_uint32, VP)
date_valid = declare(glib, "g_date_valid", I, VP)
date_clear = declare(glib, "g_date_clear", None, VP, U)
list_append = declare(glib, "g_list_append", VP, VP, VP)
list_free = declare(glib, "g_list_free", None, VP)


def gdate(d):
    g = GDate()
    set_julian(ctypes.byref(g), d.toordinal())  # GDate's Julian day 1 is 0001-01-01, as is ordinal 1.
    return g


def show(g):
    if not date_valid(ctypes.byref(g)):
        return None
    return datetime.date.fromordinal(get_julian(ctypes.byref(g))).isoformat()


def made_by_set(mult, pt, start, wadj):
    r = Recurrence()
    s = gdate(start)
    recurrence_set(ctypes.byref(r), mult, pt, ctypes.byref(s), wadj)
    return r


def made_raw(mult, pt, start, wadj):
    r = Recurrence()
    r.start, r.ptype, r.mult, r.wadj = gdate(start), pt, mult, wadj
    return r


def next_after(r, ref):
    out = GDate()
    date_clear(ctypes.byref(out), 1)
    g = gdate(ref)
    next_instance(ctypes.byref(r), ctypes.byref(g), ctypes.byref(out))
    return show(out)


def first_instances(r, count):
    """From the day before the start, as `xaccSchedXactionGetNextInstance` asks (`SchedXaction.cpp:885-926`)."""
    out, ref = [], datetime.date.fromordinal(get_julian(ctypes.byref(r.start))) - datetime.timedelta(days=1)
    for _ in range(count):
        nxt = next_after(r, ref)
        out.append(nxt)
        if nxt is None:
            break
        ref = datetime.date.fromisoformat(nxt)
    return out


def nth(r, n):
    out = GDate()
    nth_instance(ctypes.byref(r), n, ctypes.byref(out))
    return show(out)


def fields(r):
    return [PERIOD_TYPES[r.ptype], r.mult, show(r.start), WEEKEND_ADJUSTS[r.wadj]]


D = datetime.date
STARTS = [D(2024, 1, d) for d in (28, 29, 30, 31)] + [D(2024, 2, 28), D(2024, 2, 29), D(2025, 2, 28)] \
    + [D(2025, 4, d) for d in (28, 29, 30)] + [D(2025, 12, d) for d in (28, 29, 30, 31)] \
    + [D(2025, 3, 1), D(2025, 3, 2), D(2025, 5, 31), D(2025, 6, 1), D(2024, 3, 2), D(2024, 6, 30), D(2025, 11, 29)] \
    + [D(2025, 1, 8), D(2025, 1, 10), D(2025, 1, 15), D(2025, 1, 29), D(2024, 9, 1)]
OFFSETS = [-3, -2, -1, 0, 1, 2, 3, 27, 28, 29, 30, 31, 58, 59, 60, 61, 89, 90, 91, 364, 365, 366, 730, 1461]
FIXED_REFS = [D(2024, 2, 28), D(2024, 2, 29), D(2024, 3, 1), D(2025, 2, 28), D(2025, 3, 1), D(2028, 2, 28),
              D(2028, 2, 29), D(2025, 12, 31), D(2026, 1, 1), D(2025, 5, 30), D(2025, 6, 1), D(2025, 6, 2)]


def refs_for(start):
    return sorted({start + datetime.timedelta(days=k) for k in OFFSETS} | set(FIXED_REFS))


def case(mode, r, made_from, start):
    out = {"mode": mode, "made": made_from, "stored": fields(r)}
    out["next"] = {ref.isoformat(): next_after(r, ref) for ref in refs_for(start)}
    out["first"] = first_instances(r, 14)
    out["nth"] = [nth(r, n) for n in range(5)]
    if r.ptype != 0:  # `once` has no instance 1, so no end to period 0.
        out["periods"] = [[period_time(ctypes.byref(r), n, end) for end in (0, 1)] for n in range(3)]
    return out


def recurrences():
    cases = []
    for pt in range(len(PERIOD_TYPES)):
        for wadj in range(len(WEEKEND_ADJUSTS)):
            for start in STARTS:
                for mult in range(4):
                    made = [PERIOD_TYPES[pt], mult, start.isoformat(), WEEKEND_ADJUSTS[wadj]]
                    cases.append(case("set", made_by_set(mult, pt, start, wadj), made, start))
                    if mult > 0:
                        cases.append(case("raw", made_raw(mult, pt, start, wadj), made, start))
    return cases


LISTS = [
    [(1, 3, D(2025, 1, 1), 0), (1, 3, D(2025, 1, 15), 0)],                                   # semi-monthly
    [(1, 2, D(2025, 1, 6), 0), (1, 2, D(2025, 1, 9), 0), (2, 2, D(2025, 1, 10), 0)],         # weekly on several days
    [(1, 4, D(2025, 1, 31), 1), (3, 1, D(2025, 2, 3), 0), (0, 0, D(2025, 3, 15), 0)],        # mixed, with a once
    [(0, 0, D(2025, 3, 15), 0)],
    [],
]
LIST_REFS = [D(2025, 1, 1) + datetime.timedelta(days=k) for k in range(-2, 120, 3)]


def lists():
    out = []
    for spec in LISTS:
        recs = [made_by_set(mult, pt, start, wadj) for (mult, pt, start, wadj) in spec]
        glist = None
        for r in recs:
            glist = list_append(glist, ctypes.addressof(r))
        results = {}
        for ref in LIST_REFS:
            res = GDate()
            g = gdate(ref)
            list_next_instance(glist, ctypes.byref(g), ctypes.byref(res))
            results[ref.isoformat()] = show(res)
        list_free(glist)
        made = [[PERIOD_TYPES[pt], mult, start.isoformat(), WEEKEND_ADJUSTS[wadj]] for (mult, pt, start, wadj) in spec]
        out.append({"made": made, "next": results})
    return out


def guile_loaded():
    with open("/proc/self/maps") as maps:
        return any("libguile" in line for line in maps)


def main():
    assert _sw_core_utils.gnc_version() == "5.17", _sw_core_utils.gnc_version()
    out = {"recurrences": recurrences(), "lists": lists()}
    assert not guile_loaded(), "Guile was loaded"
    print(json.dumps(out))


if __name__ == "__main__":
    main()
