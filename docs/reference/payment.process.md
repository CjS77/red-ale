<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/payment.process
related: method/invoice.pay, method/payment.openItems, method/payment.fromTransaction
---
Records a payment by a customer, or to a vendor or employee, and applies it to the documents you choose (`applyTo`, in
that order), or with `autoPay` to all the owner's open documents, oldest first. `amount` is in the A/R or A/P account's
commodity and positive when a customer pays or you pay a vendor or employee; negative is a refund.

What is left over becomes a pre-payment, a lot with no document, and is returned as `paymentLot`; it is applied later by
a new payment of amount 0 naming it in `applyTo`. With neither `applyTo` nor `autoPay` the whole payment is a
pre-payment. An amount of 0 records no transaction and only links the lots named.

To pay in a commodity other than the account's, give `exchangeRate`.

## Examples

```example fixture=synthetic/business-editable
{"method": "payment.process", "params": {"owner": {"type": "customer", "id": "5b22073fa4e34411b07c14a77147ce6b"}, "account": "Assets:Receivable", "transferAccount": "Assets:Bank", "amount": "300.00", "date": "2024-03-25", "applyTo": ["aa210b7131de44c48b8513cd27c00e95"]}}
{"result": {"transaction": "…", "paymentLot": null}}
{"method": "payment.openItems", "params": {"owner": {"type": "customer", "id": "5b22073fa4e34411b07c14a77147ce6b"}, "account": "Assets:Receivable"}}
{"result": {"items": [{"invoice": "aa210b7131de44c48b8513cd27c00e95", "balance": {"num": "57300", "denom": "100"}, "…": "…"}]}}
```

Paying a vendor without saying what for leaves a pre-payment:

```example fixture=synthetic/business-editable
{"method": "payment.process", "params": {"owner": {"type": "vendor", "id": "9697710198334d558a9bfde219bb1e41"}, "account": "Liabilities:Payable", "transferAccount": "Assets:Bank", "amount": "10.00", "date": "2024-03-25"}}
{"result": {"transaction": "…", "paymentLot": "…"}}
```
