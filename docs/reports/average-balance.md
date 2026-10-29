<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: report/average-balance
related: method/report.run, method/report.options, report/assets-over-time, report/account-summary
---
For the selected accounts, the average, maximum and minimum balance in each period, and the money that came in, went
out and the net change. Use it to see how much of a period an account's money was actually there, for interest or fees
charged on a daily balance. With `Display/Show table` it also gives the figures as a table; the chart plots the
averages. In GnuCash: Reports ▸ Examples ▸ Average Balance.

The options that matter are `Accounts/Accounts`, `General/Start Date` and `End Date`, and `General/Step Size`. The
default `General/Price Source` is `weighted-average`.

## Examples

```example fixture=reports/average-balance now=2024-03-15T12:00:00Z
{"method": "report.run", "params": {"report": "Average Balance", "options": {"Accounts": {"Accounts": ["a0000000000000000000000000000002", "a0000000000000000000000000000003"]}, "General": {"Start Date": {"absolute": "1979-01-01"}, "End Date": {"absolute": "1979-06-30"}}, "Display": {"Show table": true}}}}
{"result": {"title": "Average Balance", "sections": [{"blocks": ["…", {"kind": "table", "rows": ["…", {"role": "data", "cells": [{"display": "02/01/79", "…": "…"}, {"display": "02/28/79", "…": "…"}, {"display": "$50.00", "…": "…"}, {"display": "$100.00", "…": "…"}, "…"], "…": "…"}, "…"], "…": "…"}, "…"], "…": "…"}, "…"], "…": "…"}}
```
