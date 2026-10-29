<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: concept/business-documents
title: Business documents
summary: Owners (Customers, Vendors, Employees, Jobs) and their invoices, bills and expense vouchers - Entries, tax tables, billing terms, posting to A/Receivable or A/Payable, payments through Lots, credit notes, and printing an invoice.
related: concept/account-types, concept/lots-and-capital-gains, concept/report-scripts, method/customer.create, method/vendor.create, method/invoice.create, method/invoice.post, method/payment.process, method/taxTable.create, method/billTerm.create
from: guide/ch_bus_features.docbook#bus-intro
---
GnuCash has business features for customers that owe you, vendors you owe and employees you reimburse. For most people
at home they are more than needed; they earn their place when many bills and receipts flow in and out and you do not
want to lose track of one because you were not paid, or did not pay, at once. **GnuCash has no payroll system**: you can
track payroll expenses, but calculating taxes and deductions happens outside it.

## The parts

| Part | What it is |
|------|------------|
| Owner | Whoever a document belongs to: a Customer, a Vendor, an Employee or a Job |
| Customer, Vendor, Employee | An Owner with an ID, a name, an Address and terms. Customers have a credit limit and tax settings; Employees a rate |
| Job | A piece of work grouping documents for one Customer or Vendor. Optional |
| Business document | An **invoice** (to a Customer), a **bill** (from a Vendor) or an **expense voucher** (from an Employee). **⚠** GnuCash's code calls all three *invoice*; red-ale says *document* for the general case |
| Credit note | A document that reverses an invoice, bill or voucher of its kind: the paper that corrects what was wrongly billed or returned. Made with the same methods |
| Entry | One line of a document: description, quantity, price, discount, tax |
| Order | A group of Entries for one Owner, later invoiced |
| Tax table | Named tax rates, each a percentage or a fixed value posted to an Account |
| Billing term | When a document falls due and any early-payment discount: *days* or *proximo* |
| Counter | The last number issued for a kind of document or Owner. Set a starting number with [`counter.set`](method/counter.set) |

A document's **end owner**, the Customer or Vendor a Job belongs to, otherwise the Owner itself, decides its kind.

Make each with its own methods: [`customer.create`](method/customer.create), [`vendor.create`](method/vendor.create),
[`employee.create`](method/employee.create), [`job.create`](method/job.create), [`taxTable.create`](method/taxTable.create),
[`billTerm.create`](method/billTerm.create), [`invoice.create`](method/invoice.create) with
[`invoice.addEntry`](method/invoice.addEntry), [`order.create`](method/order.create). Each has `get`, `list` and `update`
too. Totals are exact: [`invoice.totals`](method/invoice.totals) returns the subtotal, tax and total as printed.

## Accounts receivable and payable

- **A/R**, Accounts Receivable, records sales for which you have not yet been paid: an asset, since you expect payment.
  Its Account type is `RECEIVABLE`.
- **A/P**, Accounts Payable, records bills you have received but not yet paid: a liability. Its type is `PAYABLE`.

You do not post to these Accounts yourself. Documents and payments do.

## Posting

An invoice sits as a draft, with its Entries, until you **post** it with [`invoice.post`](method/invoice.post), naming the
A/R (or A/P) Account, the posting date and, optionally, a due date. Posting:

- makes a Transaction that credits the income Accounts of the Entries, adds the tax, and debits the A/R Account for the
  total (the other way round for a bill, in A/P);
- makes a **Lot** in the A/R or A/P Account for the document, which later holds its payments
  ([Lots and capital gains](concept/lots-and-capital-gains));
- freezes copies of the document's tax tables and billing term, so editing them later does not change posted documents.

A posted document cannot be edited. [`invoice.unpost`](method/invoice.unpost) reverses the posting (it deletes the
posted Transaction and lot links), after which you can edit again. The Transaction a posting made is read-only.
[`invoice.duplicate`](method/invoice.duplicate) copies a document to a new unposted one with the next number.

## Payments

A **payment** is a Transaction that settles an Owner's open documents through their Lots. [`payment.openItems`](method/payment.openItems)
lists what a payment can be applied to; [`payment.process`](method/payment.process) records a payment and applies it to
chosen documents, or to all open ones; [`invoice.pay`](method/invoice.pay) pays one document. A payment bigger than the
documents leaves a **pre-payment** lot, matched when the document arrives; a smaller one leaves the document partly paid.
[`payment.fromTransaction`](method/payment.fromTransaction) turns an existing Transaction into a payment.
[`invoice.due`](method/invoice.due) lists posted, unpaid documents falling due within a number of days.

## Printing an invoice

An invoice is printed by running a report: [`report.run`](method/report.run) with the
[Printable Invoice](report/printable-invoice) and its *Invoice Number* option set to the document. The Book's *Default invoice report* names the report GnuCash would run for Print
Invoice; if it names a report red-ale does not have, run the Printable Invoice, as GnuCash does when the setting is not
an invoice report. Company details on the invoice come from the book's *Business options* (name, address, ID, contact,
phone, fax, email, URL, tax number), set with [`book.setOptions`](method/book.setOptions). See
[From GnuCash: Reports menu](gnucash/reports-menu).

## Other business reports

The other business reports of GnuCash - the customer, vendor and employee reports, aging, the Customer Summary, the Easy
and Fancy invoices, the Tax invoice - are not part of red-ale. Write the one you need as a
[User report](concept/report-scripts); the query API reads documents, Entries, Owners, Orders and billing terms.
