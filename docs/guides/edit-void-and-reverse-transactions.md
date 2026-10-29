<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: guide/edit-void-and-reverse-transactions
title: Edit, void and reverse transactions
summary: Correct a transaction in place, void it while keeping a record, undo it with a reversing transaction, or delete it; attach a document link to a receipt.
related: method/transaction.update, method/transaction.delete, method/transaction.void, method/transaction.unvoid, method/transaction.reverse, method/transaction.get, method/docLink.setPathHead, method/docLink.transactions, method/docLink.documents
---
Mistakes happen, and a bookkeeper has four tools for them, from lightest to heaviest. Which to use depends on whether
you want the history to show the mistake.

| You want to | Use |
|-------------|-----|
| Fix a typo or a wrong date or account, as if it had never been wrong | [transaction.update](method/transaction.update) |
| Keep the transaction in the book but count it as nothing | [transaction.void](method/transaction.void) |
| Cancel its effect with an equal and opposite transaction on a later date | [transaction.reverse](method/transaction.reverse) |
| Remove it altogether | [transaction.delete](method/transaction.delete) |

## Steps

1. **Find the transaction** and its GUID with [transaction.list](method/transaction.list) or
   [transaction.find](method/transaction.find) (see [Find transactions](guide/find-transactions)).
2. **Edit it** with [transaction.update](method/transaction.update): description, notes, date, num, currency and
   splits. The result must still balance, or the change is refused.
3. **Void it** with [transaction.void](method/transaction.void), giving a `reason`. The splits' values are zeroed and
   the old values kept in slots, so the book remembers what was voided and why. [transaction.unvoid](method/transaction.unvoid)
   restores it. A voided transaction is read-only until then.
4. **Reverse it** with [transaction.reverse](method/transaction.reverse): it posts a new transaction on the `date` you
   give, with every value negated. Use it when the original belongs to a closed period and must stay as it was. A
   transaction can be reversed once; a second call is refused as a conflict.
5. **Delete it** with [transaction.delete](method/transaction.delete) when it should never have existed. Deleting
   cannot be undone, short of restoring a backup.
6. **Link a document.** To keep a receipt with its transaction, set the book's path head once with
   [docLink.setPathHead](method/docLink.setPathHead) (the folder your documents live in), then put a `docLink` on the
   transaction with [transaction.update](method/transaction.update). A relative link is resolved against the path head;
   [docLink.transactions](method/docLink.transactions) lists the links and the full paths they resolve to.

A transaction older than the book's auto read-only threshold, may be refused;
the error says why.

## Example

```example fixture=reports/test-data now=2024-07-01T12:00:00Z
{"method": "transaction.update", "params": {"guid": "c0000000000000000000000000000005", "description": "Fuel", "notes": "corrected"}}
{"result": {"description": "Fuel", "notes": "corrected", "balanced": true, "…": "…"}}
{"method": "transaction.void", "params": {"guid": "c0000000000000000000000000000000", "reason": "Duplicate"}}
{"result": {"void": true, "voidReason": "Duplicate", "…": "…"}}
{"method": "transaction.unvoid", "params": {"guid": "c0000000000000000000000000000000"}}
{"result": {"void": false, "…": "…"}}
{"method": "transaction.reverse", "params": {"guid": "c0000000000000000000000000000003", "date": "2024-07-01"}}
{"result": {"datePosted": "2024-07-01", "splits": [{"accountName": "Root:Asset:Bank", "value": {"num": "1500", "denom": "100"}, "…": "…"}, {"accountName": "Root:Expenses", "value": {"num": "-1500", "denom": "100"}, "…": "…"}], "…": "…"}}
{"method": "docLink.setPathHead", "params": {"pathHead": "/home/me/receipts"}}
{"result": "…"}
{"method": "transaction.update", "params": {"guid": "c0000000000000000000000000000005", "docLink": "2024/fuel.pdf"}}
{"result": "…"}
{"method": "docLink.transactions", "params": {}}
{"result": {"links": [{"transaction": "c0000000000000000000000000000005", "docLink": "2024/fuel.pdf", "relative": true, "…": "…"}], "…": "…"}}
{"method": "transaction.delete", "params": {"guid": "c0000000000000000000000000000006"}}
{"result": {}}
```
