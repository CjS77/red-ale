<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: report/cash-flow
related: method/report.run, method/report.options, report/cash-flow-barchart, report/income-statement
---
The money that came into and went out of the selected accounts (usually the bank, cash and credit card accounts)
between two dates, listed by the other account it came from or went to, with the difference between the two. Use it to
see where your money moved. Transfers between two selected accounts are left out. In GnuCash: Reports ▸ Income &
Expense ▸ Cash Flow.

The options that matter are `Accounts/Account` (the accounts that count as cash), `General/Start Date` and `End
Date`, and `General/Show Full Account Names`. The result starts with the list of the selected accounts
(`General/Show Selected Accounts`), then the table.

## Examples

```example fixture=reports/cash-flow now=2024-03-15T12:00:00Z
{"method": "report.run", "params": {"report": "Cash Flow"}}
{"result": {"title": "Cash Flow - 01/01/24 to 12/31/24", "sections": [{"blocks": ["…", "…", {"kind": "table", "rows": ["…", "…", "…", "…", "…", "…", "…", "…", "…", "…", {"role": "grandTotal", "cells": [{"text": "Difference", "…": "…"}, {"display": "$150.00", "…": "…"}], "…": "…"}, "…"], "…": "…"}, "…"], "…": "…"}, "…"], "…": "…"}}
```
