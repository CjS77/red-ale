<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/invoice.duplicate
related: method/invoice.create, method/invoice.get
---
Copies a document, posted or not, and all its lines into a new unposted document, as GnuCash's Duplicate does. The
copy is active, takes the next number from its kind's counter (or `id`), and has `date` (default today) as its opened
date and the date of every line.

Not copied: the posting itself, and each line's payment method, which goes back to `cash`, so a duplicated expense
voucher loses its card lines. GnuCash does the same. To duplicate several documents with one date, call it once for
each.

## Examples

```example fixture=synthetic/business-editable
{"method": "invoice.duplicate", "params": {"invoice": "aa210b7131de44c48b8513cd27c00e95", "date": "2024-05-01"}}
{"result": {"guid": "…", "id": "000003", "posted": false, "billingId": "CUST-REF-9", "entryCount": 2, "entries": ["…"], "…": "…"}}
```
