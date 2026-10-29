<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/scheduledTransaction.occurrences
related: method/scheduledTransaction.create, method/scheduledTransaction.sinceLastRun
---
Lists the next `count` dates (default 10) a scheduled transaction will occur on, from its current state, or those of a
Schedule you have not saved yet (`schedule`, with `startDate` and `end`): the calendar GnuCash's editor shows while you
set one up. Each date comes with the instance number `i` it would have. Use it to check a Schedule before creating it.

Occurrences already due and not yet acted on are included; deferred (postponed) instances are not, they appear in Since
Last Run. A start on a weekend with a `back` adjustment skips the first occurrence, as in GnuCash.

## Examples

```example fixture=synthetic/rich
{"method": "scheduledTransaction.occurrences", "params": {"scheduledTransaction": "52f96837087e9670c933f9421b0ac626", "count": 3}}
{"result": {"occurrences": [{"date": "2024-10-01", "instanceCount": 2}, {"date": "2024-11-01", "instanceCount": 3}, {"date": "2024-11-29", "instanceCount": 4}]}}
{"method": "scheduledTransaction.occurrences", "params": {"schedule": [{"mult": 1, "periodType": "endOfMonth"}], "startDate": "2024-01-31", "count": 4}}
{"result": {"occurrences": [{"date": "2024-01-31", "instanceCount": 1}, {"date": "2024-02-29", "instanceCount": 2}, {"date": "2024-03-31", "instanceCount": 3}, {"date": "2024-04-30", "instanceCount": 4}]}}
```
