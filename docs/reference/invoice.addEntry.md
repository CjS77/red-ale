<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/invoice.addEntry
related: method/invoice.updateEntry, method/invoice.removeEntry, method/invoice.totals, method/order.addEntry
---
Adds a line to an unposted document, and optionally to an open order too (customer invoices only). A line has a
description, a quantity, a price and an income account (invoices) or expense account (bills and vouchers).

Anything you leave out is filled in as GnuCash's entry register would: the price from the job's rate, the discount
from the customer, the tax table from the customer, vendor or the book's default, tax included from the owner. So
a line on a customer's invoice can come back with a discount and a tax table you never asked for.

Fields that GnuCash's register has no column for on a kind of document are refused: discounts on bills, `billable` and
`billTo` on invoices, tax settings on vouchers. So are accounts the register does not offer: A/R, A/P, bank, cash, equity
and placeholder accounts, an expense account on an invoice or an income account on a bill. On a credit note, `quantity`
is the quantity as printed, and is stored negated.

## Examples

```example fixture=synthetic/business-editable
{"method": "invoice.addEntry", "params": {"invoice": "71803cac5d8e4dceabfd0d56ec5e7521", "description": "Design", "quantity": "4", "account": "Income"}}
{"result": {"guid": "…", "description": "Design", "quantity": {"num": "4", "denom": "1"}, "invoicePrice": {"num": "9000", "denom": "100"}, "discount": {"num": "5", "denom": "1"}, "discountType": "percent", "invoiceTaxTable": "489200ce534b46379dd206e300012248", "…": "…"}}
```

Posted documents are closed to new lines:

```example fixture=synthetic/business-editable
{"method": "invoice.addEntry", "params": {"invoice": "aa210b7131de44c48b8513cd27c00e95", "description": "Late extra", "quantity": "1", "price": "10", "account": "Income"}}
{"error": {"code": 2003, "message": "…", "data": {"help": "error/conflict"}}}
```
