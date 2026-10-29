<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: guide/set-up-business
title: Set up the business features
summary: Prepare a book for invoices and bills: receivable and payable accounts, tax tables, billing terms, document numbering and your company's details.
related: method/account.create, method/taxTable.create, method/billTerm.create, method/counter.set, method/counter.list, method/book.setOptions, method/book.options, gnucash/preferences-and-book-options
from: guide/ch_bus_features.docbook#bus_setup
---
GnuCash's business features (customers, vendors, employees, invoices, bills and payments) rest on a few settings made
once per book. This guide sets them up. Then [Invoices and bills](guide/invoices-and-bills) and
[Process payments](guide/process-payments) can use them.

## Steps

1. **Accounts.** Every posted document goes to a control account: money customers owe you is in an `RECEIVABLE` account,
   money you owe vendors in a `PAYABLE` account, and each must be in the document's currency. Create them with
   [account.create](method/account.create). Also create an income account for what you sell, an expense account for what
   you buy, and a liability account for each tax you collect.
2. **Tax tables.** [taxTable.create](method/taxTable.create) names a tax and its lines: each line is a tax `account`, an
   `amount` and a `type`, either `percent` of the price or a fixed `value`. A table can have several lines, for a tax plus
   a levy.
3. **Billing terms.** [billTerm.create](method/billTerm.create) names a payment rule. `days`: due so many days after the
   posting date (`dueDays`). `proximo`: due on a day of the month, and `cutoff` says after which day of the month a
   document waits until the next. `discountDays` and `discount` store an early-payment discount.
4. **Numbering.** Documents, customers and others take their numbers from counters. [counter.list](method/counter.list)
   shows them; [counter.set](method/counter.set) sets `last` (the last number used, so the next is one more) and a
   `format` such as `INV-%04li`.
5. **Company details.** [book.setOptions](method/book.setOptions) sets `companyName`, `companyAddress` (lines separated by
   newlines), `companyContactPerson`, `companyPhoneNumber`, `companyFaxNumber`, `companyEmailAddress`,
   `companyWebsiteUrl`, `companyId` and `taxNumber`. They print in the header of the invoice
   ([Print an invoice](guide/print-an-invoice)). [book.options](method/book.options) reads them back.
6. **Defaults.** A customer or vendor can name a tax table and billing term of its own; see
   [customer.create](method/customer.create).

## Example

The accounts for the business features, in a new book:

```example
{"method": "account.create", "params": {"name": "Accounts Receivable", "type": "RECEIVABLE", "commodity": "USD"}}
{"result": {"type": "RECEIVABLE", "…": "…"}}
{"method": "account.create", "params": {"name": "Accounts Payable", "type": "PAYABLE", "commodity": "USD"}}
{"result": {"type": "PAYABLE", "…": "…"}}
{"method": "account.create", "params": {"name": "Sales Tax", "type": "LIABILITY", "commodity": "USD"}}
{"result": "…"}
{"method": "taxTable.create", "params": {"name": "Sales tax 8%", "entries": [{"account": "Sales Tax", "amount": "8", "type": "percent"}]}}
{"result": {"name": "Sales tax 8%", "entries": [{"accountName": "Sales Tax", "amount": {"num": "8", "denom": "1"}, "type": "percent", "…": "…"}], "…": "…"}}
{"method": "billTerm.create", "params": {"name": "Net14", "type": "days", "dueDays": 14}}
{"result": {"name": "Net14", "type": "days", "dueDays": 14, "…": "…"}}
{"method": "counter.set", "params": {"name": "gncInvoice", "last": 41, "format": "INV-%04li"}}
{"result": {"name": "gncInvoice", "last": 41, "format": "INV-%04li", "next": "INV-0042", "…": "…"}}
{"method": "book.setOptions", "params": {"companyName": "Acme Ltd", "companyAddress": "1 Main St\nSpringfield", "companyEmailAddress": "billing@acme.example"}}
{"result": {"companyName": "Acme Ltd", "companyEmailAddress": "billing@acme.example", "…": "…"}}
```
