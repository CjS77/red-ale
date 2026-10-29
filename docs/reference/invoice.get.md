<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/invoice.get
related: method/invoice.list, method/invoice.totals
---
Returns one document with its lines, in GnuCash's order (by date, then date entered, description, action and GUID). A
line has both an invoice side and a bill side, as in GnuCash; the side that applies depends on the document's kind.
Amounts are exact; the totals are in [invoice.totals](method/invoice.totals).

## Examples

```example fixture=synthetic/business-editable
{"method": "invoice.get", "params": {"invoice": "aa210b7131de44c48b8513cd27c00e95"}}
{"result": {"guid": "aa210b7131de44c48b8513cd27c00e95", "id": "000001", "type": "invoice", "posted": true, "billingId": "CUST-REF-9", "entryCount": 2, "entries": [{"description": "Consulting", "quantity": {"num": "10", "denom": "1"}, "invoicePrice": {"num": "12050", "denom": "100"}, "…": "…"}, "…"], "…": "…"}}
```
