<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/payment.fromTransaction
related: method/payment.process, method/payment.openItems, method/transaction.post
---
Turns a transaction you entered by hand (a deposit that was really a customer's cheque) into a payment, as GnuCash's
Assign as Payment does, or changes the matching of an existing payment. It is destructive: every split of the
transaction except its transfer split, the one in a bank, cash, asset, liability or equity account, is deleted and
rebuilt as the A/R or A/P split of a payment in a new lot, then applied as in [payment.process](method/payment.process).

Without `applyTo` it applies to the documents the transaction already pays. A posted document's own transaction cannot
be made a payment: unpost the document instead. A void or read-only transaction is refused too.

## Examples

```example fixture=synthetic/business-editable
{"method": "transaction.post", "params": {"datePosted": "2024-03-20", "description": "Acme cheque", "splits": [{"account": "Assets:Bank", "value": "500.00"}, {"account": "Assets:Receivable", "value": "-500.00"}]}}
{"result": "…"}
{"method": "payment.fromTransaction", "params": {"transaction": "00ed0000000000000000000000000003", "owner": {"type": "customer", "id": "5b22073fa4e34411b07c14a77147ce6b"}, "applyTo": ["aa210b7131de44c48b8513cd27c00e95"]}}
{"result": {"transaction": "00ed0000000000000000000000000003", "paymentLot": null}}
{"method": "payment.openItems", "params": {"owner": {"type": "customer", "id": "5b22073fa4e34411b07c14a77147ce6b"}, "account": "Assets:Receivable"}}
{"result": {"items": [{"invoice": "aa210b7131de44c48b8513cd27c00e95", "balance": {"num": "37300", "denom": "100"}, "…": "…"}]}}
```

```example fixture=synthetic/business-editable
{"method": "payment.fromTransaction", "params": {"transaction": "0ffd3608651e4faa8e39816c17e6a3b1"}}
{"error": {"code": 2001, "message": "a posted document's transaction cannot become a payment; unpost the document", "data": {"help": "error/invalid"}}}
```
