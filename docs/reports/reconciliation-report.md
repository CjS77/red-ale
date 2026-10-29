<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: report/reconciliation-report
related: method/report.run, method/report.options, report/transaction-report, report/general-ledger
---
The splits of the selected accounts grouped by reconcile state: reconciled, cleared and unreconciled, each with a
subtotal, for a date range. Use it to see what is outstanding when a statement does not agree, or to print a
reconciliation. It is the Transaction Report with presets for this. In GnuCash: Reports ▸ Reconciliation Report.

The options that matter are `Accounts/Accounts` (at least one), `General/Date Filter` (filter on the reconcile date or
the posted date), `General/Start Date` and `End Date` and `Filter/Reconciled Status`.

## Examples

```example fixture=reports/reconcile now=2024-03-15T12:00:00Z
{"method": "report.run", "params": {"report": "Reconciliation Report", "options": {"Accounts": {"Accounts": ["a0000000000000000000000000000003"]}, "General": {"Start Date": {"absolute": "1969-12-01"}, "End Date": {"absolute": "1970-12-31"}}}}}
{"result": {"title": "Reconciliation Report", "sections": [{"blocks": ["…", {"kind": "table", "rows": ["…", "…", "…", "…", "…", "…", "…", "…", "…", "…", "…", {"role": "grandTotal", "cells": [{"text": "Grand Total", "…": "…"}, {"display": "$29.00", "…": "…"}, {"display": "$28.00", "…": "…"}], "…": "…"}, "…"], "…": "…"}, "…"], "…": "…"}, "…"], "…": "…"}}
```
