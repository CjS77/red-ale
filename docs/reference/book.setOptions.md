<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/book.setOptions
related: method/book.options, error/invalid, error/conflict
---
Changes any of the book's options; an option you leave out is left alone. `autoReadOnlyDays` (0 to 3650) makes
transactions older than that many days read-only, as GnuCash's red line does, and the result's `readOnlyThreshold`
is the date it works out to today. `useTradingAccounts` switches the book to trading accounts, which changes how
transactions in several currencies are recorded.

Pitfall: once a threshold is set, posting a transaction dated before it is refused with `invalid`, and changing or deleting an existing
one with `conflict`; both carry `data.reason` `auto-read-only`. Set it to 0 to lift it.

## Examples

```example fixture=reports/test-data now=2024-07-01T12:00:00Z
{"method": "book.setOptions", "params": {"autoReadOnlyDays": 30}}
{"result": {"splitActionForNum": false, "autoReadOnlyDays": 30, "readOnlyThreshold": "2024-06-01", "useTradingAccounts": false, "defaultBudget": null, "companyName": "", "companyAddress": "", "companyContactPerson": "", "companyPhoneNumber": "", "companyFaxNumber": "", "companyEmailAddress": "", "companyWebsiteUrl": "", "companyId": "", "taxNumber": "", "defaultInvoiceReport": null}}
{"method": "transaction.post", "params": {"datePosted": "2024-03-01", "description": "Too old", "splits": [{"account": "Root:Asset:Bank", "value": "5.00"}, {"account": "Root:Expenses", "value": "-5.00"}]}}
{"error": {"code": 2001, "message": "…", "data": {"reason": "auto-read-only", "date": "2024-03-01", "threshold": "2024-06-01", "help": "error/invalid"}}}
{"method": "transaction.delete", "params": {"guid": "c0000000000000000000000000000000"}}
{"error": {"code": 2003, "message": "…", "data": {"reason": "auto-read-only", "date": "1970-01-01", "threshold": "2024-06-01", "help": "error/conflict"}}}
```
