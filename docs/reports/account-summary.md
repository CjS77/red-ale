<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: report/account-summary
related: method/report.run, method/report.options, report/balance-sheet, report/trial-balance
---
Lists the balance of each selected account on one date, with sub-accounts rolled up to the depth you choose. Use it
for a quick look at where every account stands. Unlike the [Balance Sheet](report/balance-sheet) it has no sections,
totals or equity lines, and it shows income and expense accounts too. In GnuCash: Reports ▸ Account Summary.

The options that matter are `Accounts/Accounts` and `Accounts/Levels of Subaccounts` (what to show and how deep),
`General/Date` (the date the balances are taken at) and `Commodities/Price Source` (how foreign balances are
converted to the report currency). A balance in another currency has no figure of its own in a cell and lists the
amount in that currency with its converted value.

## Examples

```example fixture=reports/test-data now=2024-03-15T12:00:00Z
{"method": "report.run", "params": {"report": "Account Summary", "options": {"General": {"Date": {"absolute": "1978-12-31"}}}}}
{"result": {"title": " Account Summary 12/31/78", "sections": [{"blocks": [{"kind": "table", "rows": ["…", "…", {"role": "account", "cells": [{"text": "", "…": "…"}, {"text": "Bank", "…": "…"}, {"display": "$2,181.00", "…": "…"}], "…": "…"}, "…"], "…": "…"}, "…"], "…": "…"}, "…"], "…": "…"}}
```
