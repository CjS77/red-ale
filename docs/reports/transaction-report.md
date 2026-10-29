<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: report/transaction-report
related: method/report.run, method/report.options, report/general-ledger, report/reconciliation-report, report/general-journal
---
GnuCash's general listing of splits: for the selected accounts and dates, each split with its date, number,
description, memo, the other account and the amount, grouped and subtotalled as you choose. It is the one to use to
find transactions or to build your own statement; the [General Ledger](report/general-ledger) and [Reconciliation
Report](report/reconciliation-report) are presets of it. In GnuCash: Reports ▸ Transaction Report.

The options that matter are `Accounts/Accounts` (at least one, or it says `No accounts selected`), `General/Start Date`
and `End Date`, the `Filter` options (text, reconcile state, void), `Sorting` (primary and secondary key, subtotals)
and `Display`, which chooses the columns. `General/Table for Exporting` gives one row per split with every field, for
export.

## Examples

```example fixture=reports/transaction now=2024-03-15T12:00:00Z
{"method": "report.run", "params": {"report": "Transaction Report", "options": {"Accounts": {"Accounts": ["a0000000000000000000000000000003"]}, "General": {"Start Date": {"absolute": "1970-01-01"}, "End Date": {"absolute": "1970-12-31"}}}}}
{"result": {"title": "Transaction Report", "sections": [{"blocks": ["…", {"kind": "table", "rows": ["…", "…", "…", "…", "…", "…", {"role": "grandTotal", "cells": [{"text": "Grand Total", "…": "…"}, null, {"display": "$28.00", "…": "…"}], "…": "…"}, "…"], "…": "…"}, "…"], "…": "…"}, "…"], "…": "…"}}
```
