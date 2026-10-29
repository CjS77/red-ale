<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/invoice.pay
related: method/payment.process, method/payment.openItems
---
Records a payment of one posted document: a transaction between the A/R or A/P account and `transferAccount`, applied to
that document only. `amount` is positive when a customer pays or when you pay a vendor or employee; a negative amount is a
refund. A payment of less than the balance leaves the rest open; one of more leaves a pre-payment.

To pay several documents at once, or an owner's oldest first, use [payment.process](method/payment.process).

## Examples

```example fixture=synthetic/business-editable
{"method": "invoice.pay", "params": {"invoice": "95e2e7416db94eb3a7de7433f3e7875f", "transferAccount": "Assets:Bank", "amount": "20.00", "date": "2024-03-25", "memo": "Part payment"}}
{"result": {"transaction": "…", "paymentLot": null}}
{"method": "invoice.pay", "params": {"invoice": "71803cac5d8e4dceabfd0d56ec5e7521", "transferAccount": "Assets:Bank", "amount": "1.00", "date": "2024-03-25"}}
{"error": {"code": 2001, "message": "document 000002 is not posted", "data": {"help": "error/invalid"}}}
```
