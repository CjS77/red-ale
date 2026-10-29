<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/invoice.due
related: method/invoice.dueDate, method/invoice.list
---
Lists the posted documents that are still open and fall due within `daysInAdvance` days (default 7) of `now`: GnuCash's
reminders of invoices due (`kind: invoices`, customer documents) and bills due (`kind: bills`, vendor and employee
documents). Overdue documents are included. Sorted by due date; `amount` is the open balance, signed as on the
A/R or A/P account.

GnuCash shows these at book open, from its preferences; red-ale has no screen, so the preferences are the params.

## Examples

```example fixture=synthetic/business-editable
{"method": "invoice.due", "params": {"kind": "invoices", "daysInAdvance": 60, "now": "2024-04-01T00:00:00Z"}}
{"result": [{"invoice": "aa210b7131de44c48b8513cd27c00e95", "id": "000001", "company": "Acme & Sons <Ltd>", "dueDate": "2024-04-03", "amount": {"num": "87300", "denom": "100"}, "…": "…"}]}
```
