<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/scheduledTransaction.get
related: method/scheduledTransaction.list, method/scheduledTransaction.update
---
Returns one scheduled transaction with its Schedule, end, last occurrence, next occurrence, deferred instances and
template transactions. `variables` lists every variable its formulas name, with `i`. Template transactions live apart
from the account tree, so no balance or report sees them.

## Examples

```example fixture=synthetic/rich
{"method": "scheduledTransaction.get", "params": {"scheduledTransaction": "52f96837087e9670c933f9421b0ac626"}}
{"result": {"guid": "52f96837087e9670c933f9421b0ac626", "name": "Monthly rent", "enabled": true, "end": {"date": "2025-07-01"}, "lastOccurrence": "2024-09-01", "nextOccurrence": "2024-10-01", "schedule": [{"mult": 1, "periodType": "month", "…": "…"}], "templateTransactions": [{"description": "Rent", "splits": [{"debitFormula": "456", "creditFormula": "", "…": "…"}, "…"], "…": "…"}], "variables": ["i"], "…": "…"}}
```
