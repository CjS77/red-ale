# GnuCash 5.17: time zones

What GnuCash 5.17 does with time zones: where the local zone comes from, which instants are local and which are
day-neutral, and how a calendar day becomes an instant. Probed against the oracle where the source was not enough
(`TZ` values below). What red-ale does with this is in [`../api.md`](../api.md), "Dates, times and the local time
zone".

## 1. Where the local zone comes from

GnuCash never asks the C library for the local time. Every conversion goes through one process-wide
`TimeZoneProvider`, built once when the engine loads (`static const TimeZoneProvider ltzp`,
`5.17:libgnucash/engine/gnc-datetime.cpp:68-69`). Its constructor (`5.17:libgnucash/engine/gnc-timezone.cpp:743-761`)
tries, in order:

1. **`TZ` as a zone file.** A leading `:` is dropped; an absolute path is read as given, anything else is read from
   `$TZDIR/<TZ>`, or `/usr/share/zoneinfo/<TZ>` when `TZDIR` is unset (`find_tz_file`, `:354-389`). The file must be
   TZif. This covers IANA names (`Pacific/Kiritimati`, `:Pacific/Kiritimati`) and names that happen to be files
   (`UTC`, `EST5EDT`).
2. **`TZ` as a POSIX zone string**, parsed by Boost.Date_Time's `posix_time_zone` (`construct`, `:722-738`). Boost
   differs from POSIX and glibc in ways that matter (boost 1.83,
   `boost/date_time/local_time/posix_time_zone.hpp`, `calc_zone` and `calc_rules`):
   - **The offset's sign is reversed**: `XYZ11` is UTC+11 to GnuCash and UTC−11 to glibc. Probed: under `TZ=XYZ11`
     the oracle's day starts at 13:00 UTC.
   - The offset must lie within −12 h to +14 h; `XYZ-13` and `XYZ-14` are refused.
   - Names are letters only, so the quoted form `<-11>11` is refused.
   - A DST name needs both rule fields (`M`m`.`w`.`d, `J`n or n, each with an optional `/`time from 0 to 24 h,
     default 02:00); the second offset is the DST *adjustment* (default 1 h), not the DST zone's offset.
   - An empty string is refused. A name with no offset (`UTC` when no file of that name exists) is UTC.
3. **`/etc/localtime`**, read as TZif.
4. **UTC** (`PTZ("UTC0")`).

A `TZ` that fails both 1 and 2 falls through to `/etc/localtime`, exactly as an unset `TZ` does (probed with
`TZ=""`, `TZ=Nowhere/Land`, `TZ=<-11>11`, `TZ=XYZ-13`).

The provider turns a TZif file into one Boost zone per year, each a standard offset with at most one DST rule
(`TimeZoneProvider::parse_file`, `:635-720`; `get`, `:767-776`). For the regular zones of the present day this
gives the same offsets as the file's transitions. It can differ in a year in which a zone changed its rules, and
after the last transition in a "slim" TZif file, whose future rules live only in the footer that GnuCash ignores.

## 2. Day parts

A `GncDateTime` built from a date and a *day part* (`LDT_from_date_daypart`, `5.17:libgnucash/engine/gnc-datetime.cpp:244-268`)
is one of:

- **start**: local 00:00:00;
- **end**: local 23:59:59;
- **neutral**: 10:59:00 UTC, which is the same calendar day in every zone from UTC−10 to UTC+13. When the zone's
  offset at that instant is below −10 h, the time moves later by `−(hours + 10)` hours; above +13 h, earlier by
  `hours − 13`, where *hours* is the offset's whole hours, truncated. So UTC−11 gives 11:59 UTC and UTC+14 gives
  09:59 UTC. Probed: `Pacific/Pago_Pago` 11:59, `Pacific/Kiritimati` 09:59.

A local time that does not exist (in a gap) is retried one hour later; an ambiguous one (in a fold) is taken one hour
later and moved back an hour, which picks the later of the two instants (`LDT_with_pushup` and `LDT_from_date_time`,
`:195-229`). If the retry fails too, the conversion throws, and `gnc_dmy2time64*` return `INT64_MAX`
(`5.17:libgnucash/engine/gnc-date.cpp:1240-1259`).

## 3. What uses which

**Day-neutral** (a date chosen by the user):

- `gdate_to_time64` (`5.17:libgnucash/engine/gnc-date.cpp:1323-1328`), and so posted dates set from a date
  (`xaccTransSetDatePostedGDate`, `5.17:libgnucash/engine/Transaction.cpp:1914-1926`).
- The date edit widget without a time field (`gnc_date_edit_get_date`, `5.17:gnucash/gnome-utils/gnc-date-edit.c:1069`,
  `:1084-1094`): the price editor's date, the post dialog's post and due dates
  (`5.17:gnucash/gnome/dialog-date-close.c:90-98`), the find dialog's "before" and "on or after".
- Billing term due dates (`compute_time`, `5.17:libgnucash/engine/gncBillTerm.c:800-819`), which take the local date
  of the posting time.

**Local** (`gnc_localtime_r`, `gnc_mktime`, `GncDateTime(time64)`):

- Day start and end of an instant (`gnc_time64_get_day_start`, `gnc_time64_get_day_end`,
  `5.17:libgnucash/engine/gnc-date.cpp:1366-1394`), and today's (`:1417`).
- The calendar date of an instant (`time64_to_gdate`, `:1284-1296`), except a posted date with no `date-posted`
  slot, which is read as its **UTC** date (`xaccTransGetDatePostedGDate`, `5.17:libgnucash/engine/Transaction.cpp:2259-2289`).
- "The same day" of two prices: both are moved to local noon of their local day and compared
  (`time64CanonicalDayTime`, `5.17:libgnucash/engine/gnc-date.cpp:403-410`; `price_is_duplicate`,
  `5.17:libgnucash/engine/gnc-pricedb.cpp:696-703`; `lookup_nearest_in_time`, `:2215-2260`). So
  `gnc_pricedb_add_price`'s same-day replacement is local.
- The find dialog's "is on" (local day, `QOF_DATE_MATCH_DAY`, `5.17:libgnucash/engine/qofquerycore.cpp:350-364`),
  "before or on" and "after" (local end of day, `5.17:gnucash/gnome-search/search-date.c:160-170`).
- Dates of invoices and entries: local noon of the chosen day (`time64CanonicalDayTime (gdate_to_time64 (date))`,
  `5.17:libgnucash/engine/gncInvoice.c:491`, `5.17:libgnucash/engine/gncEntry.c:514`).
- The accounting period: local 00:00:00 of its first day and 23:59:59 of its last
  (`5.17:libgnucash/engine/gnc-accounting-period.c:60-100`, `:223`, `:318`).
- Relative dates of report options (`gnc_relative_date_to_time64`, `5.17:libgnucash/engine/gnc-option-date.cpp:519-597`):
  see §4.
- The statement date of a reconciliation (`gnc_date_edit_get_date_end`, `5.17:gnucash/gnome-utils/gnc-date-edit.c:1117-1128`).
- Remove old prices: the cutoff is the neutral time of the chosen day, and periods compare local dates
  (`5.17:gnucash/gnome/dialog-price-edit-db.cpp:559-596`; `5.17:libgnucash/engine/gnc-pricedb.cpp:1482-1483`).
- Backup and journal file names, `YYYYMMDDhhmmss` in local time (`GncDateTimeImpl::timestamp`,
  `5.17:libgnucash/engine/gnc-datetime.cpp:638-644`).

**UTC**: stored times (`YYYY-MM-DD HH:MM:SS +0000`, [`xml-format-core.md`](xml-format-core.md) §4), the journal's
time fields and a void's `void-time` (`gnc_time64_to_iso8601_buff`).

## 4. Relative dates

`gnc_relative_date_to_time64(period, now)` (`5.17:libgnucash/engine/gnc-option-date.cpp:519-597`) works on the local
`struct tm` of `now`. The 31 periods (`reldates`, `:73-323`) are `today`, one week/month/three months/six
months/one year ago and ahead (keep the time of day), and the start and end of this, the previous and the next month,
quarter and calendar year, and of the accounting period. Rules worth knowing:

- `today` returns `now` itself; the accounting period's start and end are `gnc_accounting_period_fiscal_start` and
  `_end`.
- A *start* is local 00:00:00 of its day, an *end* local 23:59:59; "ago" and "ahead" keep `now`'s time of day.
- Days past a month's end roll into the next month (`normalize_reldate_tm`, `:455-485`): one month before 31 March
  is 2 or 3 March.
- Quarters count from the month of the accounting period's start. When that start is not the first of a month, a
  quarter starts and ends on that day of the month (`reldate_set_day_and_time`, `:487-516`, and the quarter case,
  `:556-568`).
