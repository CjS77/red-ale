<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: gnucash/business-menu
title: Business menu
summary: GnuCash's Business menu mapped to methods - Customers, Vendors, Employees, Jobs, Invoices, Bills, Expense Vouchers, Process Payment, due reminders, Sales Tax Tables and Billing Terms. Find windows are list methods; printing is report.run.
related: concept/business-documents, concept/account-types, gnucash/reports-menu, method/customer.create, method/invoice.create, method/invoice.post, method/payment.process, method/taxTable.create, method/billTerm.create
from: manual/ch_GUIMenus.docbook#AccTree-business-menu
---
GnuCash's **Business** menu has a submenu for each kind of Owner. red-ale has the same objects, each with `create`,
`get`, `list` and `update` methods, and a "Find" window is a `list` method. See
[Business documents](concept/business-documents) for the model.

## Customer

| GnuCash | red-ale |
|---|---|
| Customers Overview | [`customer.list`](method/customer.list) |
| New Customer... | [`customer.create`](method/customer.create): ID (from the next counter if you leave it out), name, address, terms, tax settings, credit limit. Edit with [`customer.update`](method/customer.update); [`customer.get`](method/customer.get) reads one |
| Find Customer... | [`customer.list`](method/customer.list), filtered by you |
| New Invoice... | [`invoice.create`](method/invoice.create) for the Customer, then [`invoice.addEntry`](method/invoice.addEntry) for each line. Make a credit note with `creditNote: true` |
| Edit Invoice | [`invoice.update`](method/invoice.update), [`invoice.updateEntry`](method/invoice.updateEntry) and [`invoice.removeEntry`](method/invoice.removeEntry), while it is unposted. [`invoice.totals`](method/invoice.totals) gives the subtotal, tax and total |
| Post (in the invoice window) | [`invoice.post`](method/invoice.post): the A/Receivable Account, the posting date, an optional due date |
| Unpost | [`invoice.unpost`](method/invoice.unpost) |
| Duplicate Invoice | [`invoice.duplicate`](method/invoice.duplicate) |
| Find Invoice... | [`invoice.list`](method/invoice.list) |
| New Job..., Find Job... | [`job.create`](method/job.create), [`job.list`](method/job.list) |
| Process Payment... | [`payment.openItems`](method/payment.openItems) lists what can be paid, then [`payment.process`](method/payment.process); one document: [`invoice.pay`](method/invoice.pay) |
| Invoices Due Reminder | [`invoice.due`](method/invoice.due) |

## Vendor

| GnuCash | red-ale |
|---|---|
| Vendors Overview, Find Vendor... | [`vendor.list`](method/vendor.list), [`vendor.get`](method/vendor.get) |
| New Vendor... | [`vendor.create`](method/vendor.create), [`vendor.update`](method/vendor.update) |
| New Bill..., Find Bill... | [`invoice.create`](method/invoice.create) for the Vendor, [`invoice.list`](method/invoice.list). Everything above for invoices applies to bills; post to an A/Payable Account |
| New Job..., Find Job... | [`job.create`](method/job.create), [`job.list`](method/job.list) |
| Process Payment... | [`payment.process`](method/payment.process) |
| Bills Due Reminder | [`invoice.due`](method/invoice.due) |

## Employee

| GnuCash | red-ale |
|---|---|
| Employees Overview, Find Employee... | [`employee.list`](method/employee.list), [`employee.get`](method/employee.get) |
| New Employee... | [`employee.create`](method/employee.create), [`employee.update`](method/employee.update) |
| New Expense Voucher..., Find Expense Voucher... | [`invoice.create`](method/invoice.create) for the Employee, [`invoice.list`](method/invoice.list) |
| Process Payment... | [`payment.process`](method/payment.process) |

## The rest of the menu

| GnuCash | red-ale |
|---|---|
| Sales Tax Table | [`taxTable.create`](method/taxTable.create), `.get`, `.list`, `.update` and `.delete`. A table in use cannot be deleted |
| Billing Terms Editor | [`billTerm.create`](method/billTerm.create), `.get`, `.list`, `.update` and `.delete` |
| Business Linked Documents | [`docLink.documents`](method/docLink.documents) |
| Orders | [`order.create`](method/order.create), [`order.addEntry`](method/order.addEntry), [`order.close`](method/order.close) and the rest of `order.*` |
| File > Properties > Counters tab | [`counter.list`](method/counter.list), [`counter.set`](method/counter.set) |
| File > Properties > Business tab | [`book.options`](method/book.options), [`book.setOptions`](method/book.setOptions): the company details, the default invoice report |

## Printing an invoice

GnuCash's File > Print Invoice runs the Book's **Default Invoice Report** for the document, falling back to the
[Printable Invoice](report/printable-invoice) when that setting is not an invoice report. In red-ale that is
[`report.run`](method/report.run) (or [`report.export`](method/report.export) for HTML) with the invoice report and its
**Invoice Number** option set to the document's GUID:

1. Read `defaultInvoiceReport` from [`book.options`](method/book.options).
2. If it names an invoice report red-ale has, run that. If it is empty, or names one red-ale does not have (the Easy or
   Fancy Invoice, say), run the Printable Invoice, as GnuCash falls back to it.
3. Pass `{"General": {"Invoice Number": "<guid>"}}` as `options`.

The company details printed on it are the book's Business options. The Easy, Fancy, Tax and Australian Tax invoices and
the Receipt are not ported; see [Reports menu](gnucash/reports-menu).

## What is not here

GnuCash has no payroll system and neither has red-ale. The customer, vendor, employee and job reports, aging reports and
the Customer Summary are not ported: write them as [User reports](concept/report-scripts).
