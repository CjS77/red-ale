<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: report/general-journal
related: method/report.run, method/report.options, report/general-ledger, report/transaction-report
---
Every transaction in the book that is not void, oldest first, in journal form: its date and description and each of
its splits with the account and the debit or credit. Use it to read the book in the order things were posted. It takes
only a title: there are no date or account options, so on a large book the result is large. For particular accounts
and dates use the [General Ledger](report/general-ledger) or the [Transaction Report](report/transaction-report). In
GnuCash: Reports ▸ Assets & Liabilities ▸ General Journal.



## Examples

```example fixture=reports/reconcile now=2024-03-15T12:00:00Z
{"method": "report.run", "params": {"report": "General Journal"}}
{"result": {"title": "General Journal", "sections": [{"blocks": [{"kind": "table", "rows": ["…", "…", {"role": "data", "cells": [null, null, {"text": "Root:Asset:Bank", "…": "…"}, {"display": "$29.00", "…": "…"}, null], "…": "…"}, "…"], "…": "…"}, "…"], "…": "…"}, "…"], "…": "…"}}
```
