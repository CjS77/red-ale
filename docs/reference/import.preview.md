<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/import.preview
related: method/import.apply, method/import.discard, method/importMap.list
---
Reads a file and says what importing it would do, without changing the book. The answer, the Import proposal, is held
under an `id` until you [apply](method/import.apply) or [discard](method/import.discard) it. Nothing reaches the book
without a preview first, whatever the format.

`format` is one of `csvTransactions` (GnuCash's Import Transactions from CSV), `qif` (Import QIF), `csvAccounts`,
`csvPrices`, `csvDocuments` (Import Bills & Invoices) and `csvOwners` (Import Customers & Vendors). The file's content
goes in `files` as `text`, or as `base64` with an `encoding`; there is no path to read. Only QIF takes several files.
The other params depend on the format: for CSV transactions and prices, a `layout`, by name or written out whole, which
says the separator, the date and number formats and what each column is.

Only CSV transactions go through GnuCash's duplicate matcher (`action` is `add`, `clear`, `update` or `skip` for each
transaction, with `matches` to choose from). A transaction that does not balance is not refused, as in GnuCash: applying
it adds an `Imbalance-<CUR>` split, and the proposal flags it with `imbalance` beforehand. Rows that cannot be imported
are listed in `errors` with GnuCash's own messages and are left out.

Proposal IDs (`imp-1`, …) count up in each server process, so a proposal made before the book was opened, or pushed out
by 16 newer ones, is gone. Applying a proposal made at an older revision of the book is refused.

## Examples

A bank's statement in a CSV file, as a layout with a base account describes it. The transfer account is unknown, so each
transaction would be balanced against `Imbalance-USD`; give the account at `import.apply`.

```example
{"method": "account.create", "params": {"name": "Checking", "type": "BANK", "commodity": "USD"}}
{"result": "…"}
{"method": "import.preview", "params": {"format": "csvTransactions", "files": [{"name": "bank.csv", "text": "Date,Description,Amount\n2024-06-03,Coffee,-3.50\n2024-06-04,Lunch,-12.00\n"}], "layout": {"format": "csv", "separators": ",", "skipStartLines": 1, "dateFormat": "y-m-d", "currencyFormat": "period", "encoding": "utf-8", "baseAccount": "Checking", "columns": ["Date", "Description", "Amount"]}}}
{"result": {"id": "…", "format": "csvTransactions", "summary": {"add": 2, "imbalanced": 2, "errors": 0, "…": "…"}, "newAccounts": [{"name": "Imbalance-USD", "type": "BANK", "commodity": "CURRENCY::USD"}], "transactions": [{"index": 0, "description": "Coffee", "datePosted": "2024-06-03", "action": "add", "splits": [{"accountName": "Checking", "value": {"num": "-350", "denom": "100"}, "…": "…"}], "imbalance": {"value": {"num": "350", "denom": "100"}, "…": "…"}, "…": "…"}, {"index": 1, "description": "Lunch", "…": "…"}], "…": "…"}}
```

Prices from a file with a date and a rate on each line:

```example
{"method": "import.preview", "params": {"format": "csvPrices", "files": [{"text": "2024-06-28,1.0850\n"}], "layout": {"format": "csv", "separators": ",", "skipStartLines": 0, "dateFormat": "y-m-d", "currencyFormat": "period", "encoding": "utf-8", "columns": ["Date", "Amount"], "fromCommodity": "CURRENCY::EUR", "toCurrency": "USD"}}}
{"result": {"id": "…", "format": "csvPrices", "summary": {"add": 1, "errors": 0, "…": "…"}, "prices": [{"index": 0, "commodity": "CURRENCY::EUR", "currency": "CURRENCY::USD", "date": "2024-06-28", "amount": {"num": "10850", "denom": "10000"}, "action": "add", "existing": null, "…": "…"}], "…": "…"}}
```

Accounts, in the layout GnuCash's account export writes:

```example
{"method": "import.preview", "params": {"format": "csvAccounts", "files": [{"text": "Type,Full Account Name,Account Name,Account Code,Description,Account Color,Notes,Symbol,Namespace,Hidden,Tax Info,Placeholder\nBANK,Savings,Savings,,,,,USD,CURRENCY,F,F,F\n"}]}}
{"result": {"id": "…", "format": "csvAccounts", "newAccounts": [{"name": "Savings", "type": "BANK", "commodity": "CURRENCY::USD"}], "accounts": [{"index": 0, "fullName": "Savings", "action": "add", "…": "…"}], "…": "…"}}
```
