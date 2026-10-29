<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/export.csvTransactions
related: method/export.csvTree, method/import.preview, method/transaction.list
---
Returns the transactions of some accounts, between two dates, as CSV text in GnuCash's Export Transactions to CSV
layout. It writes no file. `layout: complex` (the default) writes one line per split with the transaction's ID;
`simple` writes one line per transaction, for its split in the exported account. A transaction is written once, under
the first account in `accounts` it has a split in, so the order of `accounts` matters.

Dates and numbers follow the Locale (the `locale` setting, or `locale` for this call), so the default `C` locale gives
`06/03/24` and `$3.50`. In the simple layout the header says `Account Name` but the column holds the full name, as in
GnuCash.

## Examples

```example
{"method": "account.create", "params": {"name": "Checking", "type": "BANK", "commodity": "USD"}}
{"result": "…"}
{"method": "account.create", "params": {"name": "Food", "type": "EXPENSE", "commodity": "USD"}}
{"result": "…"}
{"method": "transaction.post", "params": {"datePosted": "2024-06-03", "description": "Coffee", "splits": [{"account": "Food", "value": "3.50"}, {"account": "Checking", "value": "-3.50"}]}}
{"result": "…"}
{"method": "export.csvTransactions", "params": {"accounts": ["Checking"], "layout": "simple"}}
{"result": {"csv": "Date,Account Name,Number,Description,Full Category Path,Reconcile,Amount With Sym,Amount Num.,Value With Sym,Value Num.,Rate/Price\r\n06/03/24,Checking,,Coffee,Food,n,-$3.50,-3.50,-$3.50,-3.50,1.00\r\n", "transactions": 1, "lines": 1}}
{"method": "export.csvTransactions", "params": {"accounts": ["Checking"], "from": "2024-07-01"}}
{"result": {"transactions": 0, "lines": 0, "…": "…"}}
```
