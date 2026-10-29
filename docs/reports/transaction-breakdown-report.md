<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: report/transaction-breakdown-report
related: method/report.run, method/report.options, report/transaction-report, report/general-ledger
---
For one account, its transactions between two dates with the amount split by the other accounts, one column for each
of them, and a total row. Use it to see how each transaction in an account was spread, such as a card statement over
expense accounts. In GnuCash: Reports ▸ Experimental ▸ Transaction Breakdown Report.

The options that matter are `General/Account` (the account to break down), `General/Start Date` and `End Date`,
`General/Description Filter` and `General/Limit for number of columns`.

## Examples

```example fixture=reports/transaction now=2024-03-15T12:00:00Z
{"method": "report.run", "params": {"report": "Transaction Breakdown Report", "options": {"General": {"Account": "a0000000000000000000000000000003", "Start Date": {"absolute": "1970-01-01"}, "End Date": {"absolute": "1970-12-31"}}}}}
{"result": {"title": "Transaction Breakdown Report", "sections": [{"blocks": [{"kind": "table", "rows": ["…", "…", "…", {"role": "total", "cells": [{"text": "Total For 01/01/70 to 12/31/70", "…": "…"}, {"display": "-$28.00", "…": "…"}, {"display": "-$8.00", "…": "…"}, {"display": "$20.00", "…": "…"}], "…": "…"}, "…"], "…": "…"}, "…"], "…": "…"}, "…"], "…": "…"}}
```
