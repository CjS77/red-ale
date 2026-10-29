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

Proposal IDs (`imp-1`, …) count up from 1 in each server and are never reused while it runs, so a proposal made before
the book was opened, or pushed out by 16 newer ones, is gone. Applying a proposal made at an older revision of the book is refused.

## Examples

A bank's statement in a CSV file, as a layout with a base account describes it. The transfer account is unknown, so each
transaction would be balanced against `Imbalance-USD`; give the account at `import.apply`.

```example
{"method": "account.create", "params": {"name": "Checking", "type": "BANK", "commodity": "USD"}}
{"result": "…"}
{"method": "import.preview", "params": {"format": "csvTransactions", "files": [{"name": "bank.csv", "text": "Date,Description,Amount\n2024-06-03,Coffee,-3.50\n2024-06-04,Lunch,-12.00\n"}], "layout": {"format": "csv", "separators": ",", "skipStartLines": 1, "dateFormat": "y-m-d", "currencyFormat": "period", "encoding": "utf-8", "baseAccount": "Checking", "columns": ["Date", "Description", "Amount"]}}}
{"result": {"id": "…", "format": "csvTransactions", "summary": {"add": 2, "imbalanced": 2, "errors": 0, "…": "…"}, "newAccounts": [{"name": "Imbalance-USD", "type": "BANK", "commodity": "CURRENCY::USD"}], "transactions": [{"index": 0, "description": "Coffee", "datePosted": "2024-06-03", "action": "add", "splits": [{"accountName": "Checking", "value": {"num": "-350", "denom": "100"}, "…": "…"}], "imbalance": {"value": {"num": "350", "denom": "100"}, "…": "…"}, "…": "…"}, {"index": 1, "description": "Lunch", "…": "…"}], "…": "…"}}
```

The base account can also be given beside the layout, which overrides the layout's own or gives one to a layout
that has none, as choosing the account in GnuCash's assistant does. A saved layout can then serve several accounts:

```example
{"method": "account.create", "params": {"name": "Savings", "type": "BANK", "commodity": "USD"}}
{"result": "…"}
{"method": "import.preview", "params": {"format": "csvTransactions", "files": [{"text": "2024-06-03,Interest,1.25\n"}], "layout": {"format": "csv", "separators": ",", "dateFormat": "y-m-d", "currencyFormat": "period", "encoding": "utf-8", "columns": ["Date", "Description", "Amount"]}, "baseAccount": "Savings"}}
{"result": {"transactions": [{"index": 0, "description": "Interest", "splits": [{"accountName": "Savings", "value": {"num": "125", "denom": "100"}, "…": "…"}], "…": "…"}], "…": "…"}}
```

In a book that has imported before, the Bayesian matcher proposes the account the same words and weekday went to
last time (`from: "bayes"`), so `import.apply` needs no account for them:

```example fixture=synthetic/import-map
{"method": "import.preview", "params": {"format": "csvTransactions", "files": [{"text": "2024-06-03,Corner Grocer,-23.40\n2024-06-28,Salary ACME,2000.00\n"}], "layout": {"columns": ["Date", "Description", "Amount"], "dateFormat": "y-m-d"}, "baseAccount": "Assets:Checking"}}
{"result": {"transactions": [{"description": "Corner Grocer", "action": "add", "other": {"accountName": "Expenses:Groceries", "from": "bayes", "…": "…"}, "…": "…"}, {"description": "Salary ACME", "other": {"accountName": "Income:Salary", "from": "bayes", "…": "…"}, "…": "…"}], "…": "…"}}
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
