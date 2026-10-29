<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/payment.openItems
related: method/payment.process, method/invoice.pay
---
Lists what an owner has open in an A/R or A/P account: posted documents with a balance and pre-payments, in the order
GnuCash's payment dialog applies them (by due date). Pass the GUIDs it gives, in this order, as `applyTo` of
[payment.process](method/payment.process) to pay as the dialog would.

`balance` is signed as on the account: a customer's unpaid invoice is positive and a pre-payment is negative; for a
vendor it is the other way round. `invoice` and `id` are null for a pre-payment, whose `type` is `Pre-Payment`.

## Examples

```example fixture=synthetic/business-editable
{"method": "payment.openItems", "params": {"owner": {"type": "customer", "id": "5b22073fa4e34411b07c14a77147ce6b"}, "account": "Assets:Receivable"}}
{"result": {"items": [{"lot": "83f95c95c93e444f8941d784bff4de2c", "invoice": "aa210b7131de44c48b8513cd27c00e95", "id": "000001", "type": "Invoice", "balance": {"num": "87300", "denom": "100"}, "…": "…"}]}}
```
