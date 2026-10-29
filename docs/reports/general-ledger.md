<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: report/general-ledger
related: method/report.run, method/report.options, report/transaction-report, report/general-journal
---
A ledger for each of the selected accounts: its splits between two dates, with the date, description, the other
account and the amount, and a total for each account and for all of them. It is a Transaction Report preset for the
accounting view. In GnuCash: Reports ▸ Assets & Liabilities ▸ General Ledger.

The options that matter are `Accounts/Accounts` (at least one, or it says `No accounts selected`) and `General/Start
Date` and `End Date`; the transaction report's `Display` and `Sorting` options also apply.

## Examples

```example fixture=reports/transaction now=2024-03-15T12:00:00Z
{"method": "report.run", "params": {"report": "General Ledger", "options": {"Accounts": {"Accounts": ["a0000000000000000000000000000003"]}, "General": {"Start Date": {"absolute": "1970-01-01"}, "End Date": {"absolute": "1970-12-31"}}}}}
{"result": {"title": "General Ledger", "sections": [{"blocks": ["…", {"kind": "table", "rows": ["…", "…", "…", "…", "…", "…", {"role": "grandTotal", "cells": [{"text": "Grand Total", "…": "…"}, null, {"display": "$28.00", "…": "…"}], "…": "…"}, "…"], "…": "…"}, "…"], "…": "…"}, "…"], "…": "…"}}
```
