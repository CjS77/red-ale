<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/invoice.create
related: method/invoice.addEntry, method/invoice.post, method/customer.create, method/vendor.create, method/employee.create
---
Creates an unposted document: an invoice, a bill or an expense voucher, with no lines yet. The `owner` decides which: a
customer gives an invoice, a vendor a bill, an employee an expense voucher, and a job gives the kind of its owner.
Add lines with [invoice.addEntry](method/invoice.addEntry), then post it with [invoice.post](method/invoice.post).

The document takes its owner's currency and billing term (give `terms: null` for none), and its number from the counter
of its kind (`gncInvoice`, `gncBill` or `gncExpVoucher`) unless you give `id`. A number taken is never given back, even
if the document is later deleted. `creditNote: true` makes a credit note, which cannot be changed afterwards; it also
sets the book feature `Credit Notes`, which older GnuCash versions do not know, so they will refuse the book.

## Examples

```example fixture=synthetic/business-editable
{"method": "invoice.create", "params": {"owner": "5b22073fa4e34411b07c14a77147ce6b", "billingId": "PO-5"}}
{"result": {"guid": "…", "id": "000003", "type": "invoice", "posted": false, "terms": "3416d941790749378beeb338b8357a72", "billingId": "PO-5", "entryCount": 0, "entries": [], "…": "…"}}
{"method": "invoice.create", "params": {"owner": "9697710198334d558a9bfde219bb1e41"}}
{"result": {"guid": "…", "id": "000002", "type": "bill", "posted": false, "…": "…"}}
```
