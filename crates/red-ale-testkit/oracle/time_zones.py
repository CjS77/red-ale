"""GnuCash 5.17's local-time functions, for the time-zone tests. Run under the TZ being tested.

Usage: time_zones.py '<json>' where the JSON is {"instants": [time64...], "dates": [[y, m, d]...], "nows": [time64...]}.
Prints, as JSON:

- "dayStart", "dayEnd", "noon": gnc_time64_get_day_start, gnc_time64_get_day_end and time64CanonicalDayTime of each
  instant;
- "neutral": gnc_dmy2time64_neutral of each date;
- "today": [gnc_time(NULL), gnc_time64_get_today_start(), gnc_time64_get_today_end()];
- "fiscal": [gnc_accounting_period_fiscal_start(), gnc_accounting_period_fiscal_end()], from GnuCash's default
  preferences (this calendar year);
- "relative": for each now, gnc_relative_date_to_time64 of each of the 31 RelativeDatePeriod values.
"""
import ctypes
import json
import sys

import gnucash  # noqa: F401  (loads the engine with the bindings' library path)
from gnc_engine import engine
from gnucash import _sw_core_utils

if _sw_core_utils.gnc_version() != "5.17":
    raise SystemExit("the oracle must be GnuCash 5.17")


def fn(name, *argtypes):
    f = getattr(engine, name)
    f.restype = ctypes.c_int64
    f.argtypes = list(argtypes)
    return f


T64 = ctypes.c_int64
day_start = fn("gnc_time64_get_day_start", T64)
day_end = fn("gnc_time64_get_day_end", T64)
canonical = fn("time64CanonicalDayTime", T64)
neutral = fn("gnc_dmy2time64_neutral", ctypes.c_int, ctypes.c_int, ctypes.c_int)
gnc_time = fn("gnc_time", ctypes.c_void_p)
today_start = fn("gnc_time64_get_today_start")
today_end = fn("gnc_time64_get_today_end")
fiscal_start = fn("gnc_accounting_period_fiscal_start")
fiscal_end = fn("gnc_accounting_period_fiscal_end")
relative = fn("_Z27gnc_relative_date_to_time6418RelativeDatePeriodl", ctypes.c_int, T64)

spec = json.loads(sys.argv[1])
print(json.dumps({
    "dayStart": [day_start(t) for t in spec["instants"]],
    "dayEnd": [day_end(t) for t in spec["instants"]],
    "noon": [canonical(t) for t in spec["instants"]],
    "neutral": [neutral(d, m, y) for y, m, d in spec["dates"]],
    "today": [gnc_time(None), today_start(), today_end()],
    "fiscal": [fiscal_start(), fiscal_end()],
    "relative": [[relative(p, now) for p in range(31)] for now in spec["nows"]],
}))
