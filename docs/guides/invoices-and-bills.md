<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: guide/invoices-and-bills
title: Invoices and bills
summary: Add a customer or vendor, optionally a job, then create an invoice or bill, add its lines, post it to Receivable or Payable, and unpost or duplicate it.
related: method/customer.create, method/vendor.create, method/job.create, method/invoice.create, method/invoice.addEntry, method/invoice.post, method/invoice.unpost, method/invoice.duplicate, method/invoice.get, method/invoice.totals, method/invoice.list, concept/account-types, concept/business-documents
from: guide/ch_expenses.docbook#expenses-other
---
An **invoice** is what you send a customer to ask for payment; a **bill** is what a vendor sends you. Both are
*documents*: a list of lines (entries) with prices, taxes and discounts, which you edit freely until you **post** it.
Posting turns it into a transaction against your Accounts Receivable (invoice) or Accounts Payable (bill), and opens a lot
that [payments](guide/process-payments) will close. Before this, set the book up as in
[Set up the business features](guide/set-up-business).

## Steps

1. **Add the other party** with [customer.create](method/customer.create) or [vendor.create](method/vendor.create): a
   `name`, a `currency`, and optionally billing `terms`, a `taxTable` and a `discount`. Its ID comes from the counter
   unless you give one.
2. **Add a job** with [job.create](method/job.create) if the work is for a particular project; its `rate` becomes the
   default price of its lines. A job's documents belong to the job's owner.
3. **Create the document** with [invoice.create](method/invoice.create). The `owner` decides what it is: a customer makes
   an invoice, a vendor a bill, an employee an expense voucher, a job whatever its owner is. Give `billingId` (your
   customer's order number, or the vendor's invoice number). The number comes from the counter.
4. **Add lines** with [invoice.addEntry](method/invoice.addEntry): description, `quantity`, `price`, `account` (the income
   or expense account) and, on invoices, `discount` and tax. A line takes the owner's tax table and discount unless it says
   otherwise.
5. **Check the figures** with [invoice.totals](method/invoice.totals): subtotal, tax per tax account, and total.
6. **Post it** with [invoice.post](method/invoice.post): the Receivable or Payable `account` in the document's currency,
   and the `postDate`. The due date comes from the billing term unless you give `dueDate`. The result names the
   transaction and the lot.
7. **Posted by mistake?** [invoice.unpost](method/invoice.unpost) deletes the transaction and makes the document editable
   again, as long as no payment has been applied.
8. **Another one like it?** [invoice.duplicate](method/invoice.duplicate) copies a document's lines to a new, unposted one.
9. **Find documents** with [invoice.list](method/invoice.list) and read one with [invoice.get](method/invoice.get).

## Example

An invoice and a bill, in a book that already has a customer, a vendor, tax tables and Receivable and Payable accounts.

```example fixture=synthetic/business-editable now=2024-03-10T12:00:00Z
{"method": "customer.create", "params": {"name": "Globex", "currency": "EUR", "terms": "Net30"}}
{"result": {"id": "000002", "name": "Globex", "currency": "CURRENCY::EUR", "active": true, "…": "…"}}
{"method": "invoice.create", "params": {"owner": "5b22073fa4e34411b07c14a77147ce6b", "billingId": "PO-5"}}
{"result": {"id": "000003", "type": "invoice", "posted": false, "billingId": "PO-5", "entryCount": 0, "…": "…"}}
{"method": "invoice.addEntry", "params": {"invoice": "00ed0000000000000000000000000002", "description": "Design", "quantity": "4", "price": "90.00", "account": "Income"}}
{"result": {"description": "Design", "quantity": {"num": "4", "denom": "1"}, "…": "…"}}
{"method": "invoice.totals", "params": {"invoice": "00ed0000000000000000000000000002"}}
{"result": "…"}
{"method": "invoice.post", "params": {"invoice": "00ed0000000000000000000000000002", "account": "Assets:Receivable", "postDate": "2024-03-10"}}
{"result": {"transaction": "…", "lot": "…", "due": "…"}}
{"method": "invoice.create", "params": {"owner": "9697710198334d558a9bfde219bb1e41", "billingId": "INV-8841"}}
{"result": {"id": "000002", "type": "bill", "posted": false, "…": "…"}}
```
