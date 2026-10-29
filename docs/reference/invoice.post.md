<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/invoice.post
related: method/invoice.totals, method/invoice.unpost, method/invoice.pay, method/payment.process
---
Posts an unposted document to an A/Receivable account (invoices) or A/Payable account (bills, vouchers) in the
document's currency: it makes the transaction, puts the balance in a new lot, and freezes copies of the document's
billing term and the tax tables of its lines. The transaction is read-only until the document is unposted.

By default the lines are accumulated into one split per income or expense account (`accumulateSplits`), as GnuCash's
default preference is. `dueDate` defaults to the billing term's. Lines or tax accounts in another commodity need a
`prices` entry each. Everything is checked first, and a document with no lines, a line without an account or a
wrong posting account is refused with `invalid` and nothing changed; GnuCash can stop halfway.

## Examples

```example fixture=synthetic/business-editable
{"method": "invoice.post", "params": {"invoice": "71803cac5d8e4dceabfd0d56ec5e7521", "account": "Assets:Receivable", "postDate": "2024-03-10", "dueDate": "2024-04-10"}}
{"result": {"transaction": "…", "lot": "…", "due": "2024-04-10T10:59:00Z"}}
{"method": "invoice.post", "params": {"invoice": "71803cac5d8e4dceabfd0d56ec5e7521", "account": "Assets:Receivable", "postDate": "2024-03-10"}}
{"error": {"code": 2003, "message": "document 000002 is already posted", "data": {"help": "error/conflict"}}}
```
