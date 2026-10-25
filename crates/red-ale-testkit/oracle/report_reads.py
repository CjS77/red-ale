"""GnuCash 5.17's balance and price reads that red-ale's report query API stands on, for oracle_report_reads.rs.

Usage: report_reads.py <book> '<json>' where the JSON is
{"times": [time64...], "currency": "NS::MNEMONIC", "commodities": ["NS::MNEMONIC"...], "amount": [num, denom],
 "queries": [{"accounts": [guid...], "from": time64|null, "to": time64|null, "str": text|null, "cased": bool,
              "regexp": bool, "closing": bool}...]}.
The book is opened read-only. Prints, as JSON, amounts as reduced "num/denom":

- "accounts": per account GUID (the root left out), at each time: xaccAccountGetBalanceAsOfDate ("asOf"),
  xaccAccountGetReconciledBalanceAsOfDate ("reconciledAsOf"), the noclosing balance as of the time in the account's
  own commodity ("noclosingAsOf"), xaccAccountGetBalanceAsOfDateInCurrency and
  xaccAccountGetNoclosingBalanceAsOfDateInCurrency in "currency" with sub-accounts ("inCurrency",
  "noclosingInCurrency"), and from each time to the next xaccAccountGetBalanceChangeForPeriod and
  xaccAccountGetNoclosingBalanceChangeForPeriod with sub-accounts ("change", "noclosingChange"); and "splits", in
  xaccAccountGetSplits order, [guid, balance, noclosing, cleared, reconciled] of each split.
- "prices": per "FROM>TO" pair of "commodities", at each time, gnc_pricedb_get_nearest_price ("nearest") and
  gnc_pricedb_get_nearest_before_price ("before"), gnc_pricedb_get_latest_price ("latest"), and "amount" converted
  by the three gnc_pricedb_convert_balance_* functions ("convertNearest", "convertBefore", "convertLatest").
- "anyCurrency": per commodity, at each time, the GUIDs gnc_pricedb_lookup_nearest_in_time_any_currency_t64
  ("nearest") and gnc_pricedb_lookup_nearest_before_any_currency_t64 ("before") return, in order.
- "queries": per query, the GUIDs of the splits gnc:account-get-trans-type-splits-interval's query finds
  (5.17:gnucash/report/report-utilities.scm:693-725), in order.
"""
import ctypes
import json
import math
import sys
from datetime import datetime, timezone

from gnucash import GncNumeric, Session, SessionOpenMode, _sw_core_utils
from gnc_engine import engine, ptr

if _sw_core_utils.gnc_version() != "5.17":
    raise SystemExit("the oracle must be GnuCash 5.17")

glib = ctypes.CDLL("libglib-2.0.so.0")
glib.g_list_append.restype = ctypes.c_void_p
glib.g_list_append.argtypes = [ctypes.c_void_p, ctypes.c_void_p]


class GList(ctypes.Structure):
    _fields_ = [("data", ctypes.c_void_p), ("next", ctypes.c_void_p), ("prev", ctypes.c_void_p)]


engine.qof_instance_get_guid.restype = ctypes.c_void_p
engine.qof_instance_get_guid.argtypes = [ctypes.c_void_p]
engine.guid_to_string.restype = ctypes.c_char_p
engine.guid_to_string.argtypes = [ctypes.c_void_p]
engine.qof_query_create_for.restype = ctypes.c_void_p
engine.qof_query_create_for.argtypes = [ctypes.c_char_p]
engine.qof_query_set_book.argtypes = [ctypes.c_void_p, ctypes.c_void_p]
engine.qof_query_run.restype = ctypes.c_void_p
engine.qof_query_run.argtypes = [ctypes.c_void_p]
engine.qof_query_merge_in_place.argtypes = [ctypes.c_void_p, ctypes.c_void_p, ctypes.c_int]
engine.qof_query_destroy.argtypes = [ctypes.c_void_p]
engine.xaccQueryAddClearedMatch.argtypes = [ctypes.c_void_p, ctypes.c_int, ctypes.c_int]
engine.xaccQueryAddAccountMatch.argtypes = [ctypes.c_void_p, ctypes.c_void_p, ctypes.c_int, ctypes.c_int]
engine.xaccQueryAddDateMatchTT.argtypes = [ctypes.c_void_p, ctypes.c_int, ctypes.c_int64, ctypes.c_int, ctypes.c_int64, ctypes.c_int]
engine.xaccQueryAddDescriptionMatch.argtypes = [ctypes.c_void_p, ctypes.c_char_p, ctypes.c_int, ctypes.c_int, ctypes.c_int, ctypes.c_int]
engine.xaccQueryAddClosingTransMatch.argtypes = [ctypes.c_void_p, ctypes.c_int, ctypes.c_int]

QUERY_AND, QUERY_OR = 1, 2
GUID_MATCH_ANY = 1
COMPARE_CONTAINS = 7
CLEARED_ALL, CLEARED_VOIDED = 0x1F, 0x10


def frac(n):
    """A GncNumeric as a reduced "num/denom"; "error" for GnuCash's error values."""
    # Methods the bindings wrap return GncNumeric; the others the raw struct, whose parts are attributes.
    num, denom = (n.num(), n.denom()) if callable(n.num) else (n.num, n.denom)
    if denom <= 0:
        return "error"
    g = math.gcd(num, denom) or 1
    return f"{num // g}/{denom // g}"


def guid_of(obj):
    """The GUID of any engine object, as 32 hex digits."""
    p = obj if isinstance(obj, int) else ptr(obj).value
    return engine.guid_to_string(engine.qof_instance_get_guid(p)).decode()


def glist(p):
    """The data pointers of a GList."""
    out = []
    while p:
        node = GList.from_address(p)
        out.append(node.data)
        p = node.next
    return out


def at(t):
    return datetime.fromtimestamp(t, timezone.utc)


def run_query(book, accounts, q):
    """The query of gnc:account-get-trans-type-splits-interval, with qof-query-run's default sort."""
    query = engine.qof_query_create_for(b"Split")
    engine.qof_query_set_book(query, ptr(book))
    engine.xaccQueryAddClearedMatch(query, CLEARED_ALL & ~CLEARED_VOIDED, QUERY_AND)
    acct_list = None
    for g in q["accounts"]:
        acct_list = glib.g_list_append(acct_list, ptr(accounts[g]))
    engine.xaccQueryAddAccountMatch(query, acct_list, GUID_MATCH_ANY, QUERY_AND)
    start, end = q.get("from"), q.get("to")
    if start is not None or end is not None:
        engine.xaccQueryAddDateMatchTT(query, start is not None, start or 0, end is not None, end or 0, QUERY_AND)
    if q.get("str") is not None or q.get("closing"):
        query2 = engine.qof_query_create_for(b"Split")
        if q.get("str") is not None:
            engine.xaccQueryAddDescriptionMatch(query2, q["str"].encode(), q["cased"], q["regexp"], COMPARE_CONTAINS, QUERY_OR)
        if q.get("closing"):
            engine.xaccQueryAddClosingTransMatch(query2, 1, QUERY_OR)
        engine.qof_query_merge_in_place(query, query2, QUERY_AND)
        engine.qof_query_destroy(query2)
    found = [guid_of(s) for s in glist(engine.qof_query_run(query))]
    engine.qof_query_destroy(query)
    return found


def main():
    path, spec = sys.argv[1], json.loads(sys.argv[2])
    times = spec["times"]
    s = Session("xml://" + path, SessionOpenMode.SESSION_READ_ONLY)
    try:
        book = s.book
        table = book.get_table()
        look = lambda uid: table.lookup(*uid.split("::"))
        currency = look(spec["currency"])
        accounts = {a.GetGUID().to_string(): a for a in book.get_root_account().get_descendants()}
        out = {"accounts": {}, "prices": {}, "anyCurrency": {}, "queries": []}
        for g, a in accounts.items():
            own = a.GetCommodity()
            pairs = list(zip(times, times[1:]))
            out["accounts"][g] = {
                "asOf": [frac(a.GetBalanceAsOfDate(at(t))) for t in times],
                "reconciledAsOf": [frac(a.GetReconciledBalanceAsOfDate(at(t))) for t in times],
                "noclosingAsOf": [frac(a.GetNoclosingBalanceAsOfDateInCurrency(at(t), own, False)) for t in times],
                "inCurrency": [frac(a.GetBalanceAsOfDateInCurrency(at(t), currency, True)) for t in times],
                "noclosingInCurrency": [frac(a.GetNoclosingBalanceAsOfDateInCurrency(at(t), currency, True)) for t in times],
                "change": [frac(a.GetBalanceChangeForPeriod(at(t1), at(t2), True)) for t1, t2 in pairs],
                "noclosingChange": [frac(a.GetNoclosingBalanceChangeForPeriod(at(t1), at(t2), True)) for t1, t2 in pairs],
                "splits": [[sp.GetGUID().to_string(), frac(sp.GetBalance()), frac(sp.GetNoclosingBalance()),
                            frac(sp.GetClearedBalance()), frac(sp.GetReconciledBalance())] for sp in a.GetSplitList()],
            }
        pdb = book.get_price_db()
        amount = GncNumeric(*spec["amount"])
        commodities = [look(c) for c in spec["commodities"]]
        for f, fc in zip(spec["commodities"], commodities):
            for t_, tc in zip(spec["commodities"], commodities):
                out["prices"][f + ">" + t_] = {
                    "nearest": [frac(pdb.get_nearest_price(fc, tc, at(t))) for t in times],
                    "before": [frac(pdb.get_nearest_before_price(fc, tc, at(t))) for t in times],
                    "latest": frac(pdb.get_latest_price(fc, tc)),
                    "convertNearest": [frac(pdb.convert_balance_nearest_price_t64(amount, fc, tc, at(t))) for t in times],
                    "convertBefore": [frac(pdb.convert_balance_nearest_before_price_t64(amount, fc, tc, at(t))) for t in times],
                    "convertLatest": frac(pdb.convert_balance_latest_price(amount, fc, tc)),
                }
            out["anyCurrency"][f] = {
                "nearest": [[guid_of(p) for p in pdb.lookup_nearest_in_time_any_currency_t64(fc, at(t))] for t in times],
                "before": [[guid_of(p) for p in pdb.lookup_nearest_before_any_currency_t64(fc, at(t))] for t in times],
            }
        out["queries"] = [run_query(book, accounts, q) for q in spec["queries"]]
        print(json.dumps(out))
    finally:
        s.end()


main()
