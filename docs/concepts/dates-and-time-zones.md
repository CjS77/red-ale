<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: concept/dates-and-time-zones
title: Dates and time zones
summary: A Date is a calendar day; a timestamp is an instant in UTC. How red-ale stores a posted date, which time zone defines "today" and where a day starts and ends, and how to ask for the accounting period.
related: concept/transactions-and-splits, concept/closing-the-books-and-accounting-period, method/period.dates, method/period.get, method/config.get
---
red-ale handles dates and time zones as GnuCash 5.17 does, so a red-ale server and a GnuCash in the same time zone
agree on every date.

## Dates and timestamps

- A **Date** is a calendar day with no time: `2024-07-01`. A Transaction's `datePosted`, a Price's day and a due date
  are Dates. This is what you send and what you usually read back.
- A **timestamp** is an instant, always written in UTC with a `Z`: `2024-07-01T10:59:00Z`. Entered times, reconcile
  times and the time a method ran are timestamps.

A Date you choose is stored as the **day-neutral time**: 10:59:00 UTC on that day, moved to 11:59 in zones west of
UTC-10 and to 09:59 in zones east of UTC+13, so that it falls on that day wherever the book is read. A posted date also
gets a `date-posted` Slot holding the calendar day. Do not compute with the stored instant: read the Date.

## The local time zone

Day boundaries depend on one time zone, the **local time zone** of the process that serves red-ale, read once at
start-up as GnuCash reads it: the `TZ` environment variable as a zone file (`Europe/Paris`), else `TZ` as a POSIX
string, else the machine's zone, else UTC. **⚠** In a POSIX string GnuCash reads the sign the opposite way round to the
C library (`XYZ11` is UTC+11); red-ale does the same.

A **local day** runs from 00:00:00 to 23:59:59 local time. This decides:

- **"Today"**, wherever a method defaults to it: the local date of the server's clock, not of the client's;
- which Prices fall on the **same day**;
- the start and end of the accounting period and of a report's date ranges;
- balances "as of" a day.

If an agent and the server are in different zones, send explicit Dates. Run the server with `TZ` set to the zone the
book belongs to.

## The accounting period

The **accounting period** is the pair of dates that reports and `*Period` options mean by "this year" or "start of the
accounting period", set by the Configuration's `accountingPeriod.start`, `.end` and `.fiscalYearEnd` (or, without a
setting, the book's own fiscal-year-end Slot). It is a preference of the person running red-ale, not Book data: see
[The Configuration](concept/configuration) and [Closing the books and the accounting period](concept/closing-the-books-and-accounting-period).

[`period.dates`](method/period.dates) gives the first and last day of the period that contains a date, plus the
instants GnuCash uses (local 00:00:00 of the first day, 23:59:59 of the last). Use it to work out the dates you pass to
a report or to [`transaction.list`](method/transaction.list).

```example book=none now=2024-07-15T12:00:00Z
{"method": "period.dates", "params": {"date": "2024-07-15", "start": "fiscal-year", "end": "fiscal-year", "fiscalYearEnd": "03-31"}}
{"result": {"start": "2024-04-01", "end": "2025-03-31", "…": "…"}}
```

## Relative dates

Report options can take a **relative date**, GnuCash's names for a day worked out from now: `today`,
`one-month-ago`, `start-this-month`, `end-prev-quarter`, `start-accounting-period` and so on. They are resolved from the
server's clock, zone and accounting period before a report runs, and a report result records the dates it used. To
reproduce an earlier run, pass the same `now` to `report.run`.

## Where red-ale and GnuCash differ

red-ale follows a zone file's transitions exactly. GnuCash fits each year with one rule, which puts local midnight an
hour late on the autumn change in some European zones and drops daylight saving from 2037. This changes only which day
an instant within that hour belongs to (a balance "as of" a day, a same-day price lookup), never what is written.
