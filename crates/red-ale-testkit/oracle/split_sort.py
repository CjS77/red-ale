"""GnuCash 5.17's QofQuery over splits with a sort, for oracle_split_sort.rs.

Usage: split_sort.py <book> '<json>' where the JSON is a list of queries
[{"cleared": mask|null, "closing": bool|null, "sort": [[param...], [param...], [param...]],
  "increasing": [bool, bool, bool], "unique": bool}...].
Each query is on every split of the book (no account term), with xaccQueryAddClearedMatch of the cleared mask
(CLEARED_NO 1, CLEARED_CLEARED 2, CLEARED_RECONCILED 4, CLEARED_FROZEN 8, CLEARED_VOIDED 16) and
xaccQueryAddClosingTransMatch when given, sorted by qof_query_set_sort_order and qof_query_set_sort_increasing, and run
by qof_query_run, or xaccQueryGetSplitsUniqueTrans with "unique". Prints, as JSON, the split GUIDs each query finds,
in order. The book is opened read-only.
"""
import ctypes
import json
import sys

from gnucash import Query, Session, SessionOpenMode, _sw_core_utils
from gnc_engine import engine, ptr

if _sw_core_utils.gnc_version() != "5.17":
    raise SystemExit("the oracle must be GnuCash 5.17")


class GList(ctypes.Structure):
    _fields_ = [("data", ctypes.c_void_p), ("next", ctypes.c_void_p), ("prev", ctypes.c_void_p)]


engine.qof_instance_get_guid.restype = ctypes.c_void_p
engine.qof_instance_get_guid.argtypes = [ctypes.c_void_p]
engine.guid_to_string.restype = ctypes.c_char_p
engine.guid_to_string.argtypes = [ctypes.c_void_p]
engine.qof_query_run.restype = ctypes.c_void_p
engine.qof_query_run.argtypes = [ctypes.c_void_p]
engine.xaccQueryGetSplitsUniqueTrans.restype = ctypes.c_void_p
engine.xaccQueryGetSplitsUniqueTrans.argtypes = [ctypes.c_void_p]
engine.xaccQueryAddClearedMatch.argtypes = [ctypes.c_void_p, ctypes.c_int, ctypes.c_int]
engine.xaccQueryAddClosingTransMatch.argtypes = [ctypes.c_void_p, ctypes.c_int, ctypes.c_int]

QUERY_AND = 1


def glist(p):
    """The data pointers of a GList."""
    out = []
    while p:
        node = GList.from_address(p)
        out.append(node.data)
        p = node.next
    return out


def guid_of(p):
    return engine.guid_to_string(engine.qof_instance_get_guid(p)).decode()


def run(book, spec):
    q = Query()
    q.search_for("Split")
    q.set_book(book)
    if spec.get("cleared") is not None:
        engine.xaccQueryAddClearedMatch(ptr(q), spec["cleared"], QUERY_AND)
    if spec.get("closing") is not None:
        engine.xaccQueryAddClosingTransMatch(ptr(q), spec["closing"], QUERY_AND)
    # The parameter lists point into these strings, which must live until the query runs.
    sort = [list(params) for params in spec["sort"]]
    q.set_sort_order(*sort)
    q.set_sort_increasing(*spec["increasing"])
    found = (engine.xaccQueryGetSplitsUniqueTrans if spec.get("unique") else engine.qof_query_run)(ptr(q))
    out = [guid_of(s) for s in glist(found)]
    del sort
    return out


def main():
    path, specs = sys.argv[1], json.loads(sys.argv[2])
    s = Session("xml://" + path, SessionOpenMode.SESSION_READ_ONLY)
    try:
        print(json.dumps([run(s.book, spec) for spec in specs]))
    finally:
        s.end()


main()
