<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: guide/process-payments
title: Process payments
summary: Record a customer's payment (full, part or over-payment) against open invoices, pay a vendor's bills, turn an existing deposit into a payment, and write off a bad debt.
related: method/payment.openItems, method/payment.process, method/payment.fromTransaction, method/invoice.pay, method/invoice.get, method/invoice.list
---
Posting an invoice records that a customer owes you money, in a lot on your Receivable account. A **payment** is the
transaction that settles it: money arrives in the bank and the Receivable lot shrinks. For bills it is the other way
round: money leaves the bank, and the Payable lot shrinks.

A payment can cover one document, part of one, several, or more than is owed. What is left over from an over-payment stays
with the customer as a pre-payment, which a later document can use.

## Steps

1. **See what is open** for the party with [payment.openItems](method/payment.openItems): the owner and the Receivable or
   Payable account. It lists each open document, with its balance, and any pre-payments.
2. **Pay one document in full or in part** with [invoice.pay](method/invoice.pay): the document, the `transferAccount`
   (your bank account), the `amount` and the `date`. A smaller amount leaves the rest open.
3. **Pay several at once** with [payment.process](method/payment.process): the `owner` (`{"type": "customer", "id": ...}`),
   the `account`, the `transferAccount`, the `amount`, the `date`, and optionally `applyTo`, the documents to apply it to in
   order. Without `applyTo` it pays the oldest first only if you set `autoPay`; otherwise the amount waits as a
   pre-payment. The result's `paymentLot` is set when a pre-payment is left.
4. **Over-payment.** Pay more than is owed and the extra remains as a pre-payment lot, shown by
   [payment.openItems](method/payment.openItems), ready to be applied to the next document.
5. **Vendors and employees.** Use the same calls with the owner type `vendor` or `employee`. The `amount` is signed:
   positive when a customer pays you or when you pay a vendor or an employee, as the reference page for
   [payment.process](method/payment.process) explains.
6. **A deposit already in the book.** If a transaction was entered as a plain deposit, turn it into a payment with
   [payment.fromTransaction](method/payment.fromTransaction), naming the owner and the documents.
7. **Bad debt.** When a customer will not pay, pay the rest of the invoice with [invoice.pay](method/invoice.pay), with
   an expense account such as `Bad debts` as the `transferAccount`. The invoice closes and the expense records the loss.

## Example

```example fixture=synthetic/business-editable now=2024-03-25T12:00:00Z
{"method": "payment.openItems", "params": {"owner": {"type": "customer", "id": "5b22073fa4e34411b07c14a77147ce6b"}, "account": "Assets:Receivable"}}
{"result": {"items": [{"invoice": "aa210b7131de44c48b8513cd27c00e95", "balance": {"num": "87300", "denom": "100"}, "…": "…"}]}}
{"method": "payment.process", "params": {"owner": {"type": "customer", "id": "5b22073fa4e34411b07c14a77147ce6b"}, "account": "Assets:Receivable", "transferAccount": "Assets:Bank", "amount": "300.00", "date": "2024-03-25", "applyTo": ["aa210b7131de44c48b8513cd27c00e95"]}}
{"result": {"transaction": "…", "paymentLot": null}}
{"method": "payment.openItems", "params": {"owner": {"type": "customer", "id": "5b22073fa4e34411b07c14a77147ce6b"}, "account": "Assets:Receivable"}}
{"result": {"items": [{"invoice": "aa210b7131de44c48b8513cd27c00e95", "balance": {"num": "57300", "denom": "100"}, "…": "…"}]}}
{"method": "invoice.pay", "params": {"invoice": "95e2e7416db94eb3a7de7433f3e7875f", "transferAccount": "Assets:Bank", "amount": "20.00", "date": "2024-03-25", "memo": "Part payment"}}
{"result": {"transaction": "…", "paymentLot": null}}
{"method": "payment.process", "params": {"owner": {"type": "vendor", "id": "9697710198334d558a9bfde219bb1e41"}, "account": "Liabilities:Payable", "transferAccount": "Assets:Bank", "amount": "10.00", "date": "2024-03-25"}}
{"result": {"transaction": "…", "paymentLot": "…"}}
```
