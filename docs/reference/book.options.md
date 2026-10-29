<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/book.options
related: method/book.setOptions, method/book.info
---
Reads the book's options from GnuCash's File > Properties dialog: whether the Num field of a transaction shows the
split action instead (`splitActionForNum`), the auto read-only threshold in days (`autoReadOnlyDays`) with the
`readOnlyThreshold` date it means today, whether the book uses trading accounts, the default budget, your company's
details from the Business and Tax tabs, which the [Printable Invoice](report/printable-invoice) prints, and the
Default Invoice Report (`defaultInvoiceReport`).

## Examples

```example fixture=reports/test-data
{"method": "book.options", "params": {}}
{"result": {"splitActionForNum": false, "autoReadOnlyDays": 0, "useTradingAccounts": false, "defaultBudget": null, "companyName": "", "companyAddress": "", "companyContactPerson": "", "companyPhoneNumber": "", "companyFaxNumber": "", "companyEmailAddress": "", "companyWebsiteUrl": "", "companyId": "", "taxNumber": "", "defaultInvoiceReport": null}}
```
