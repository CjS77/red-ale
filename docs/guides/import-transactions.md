<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: guide/import-transactions
title: Import transactions
summary: Bring bank transactions in from a CSV or QIF file: preview what would happen, choose the accounts, apply the proposal, and let red-ale learn which account a description belongs to.
related: method/import.preview, method/import.apply, method/import.discard, method/importMap.list, method/importMap.delete, method/account.create, method/book.save
---
Most banks let you download your transactions as a file. Importing saves typing them, and red-ale checks each against
what is already in the book so that a transaction is not entered twice. Nothing changes until you have seen what would
happen: import is always two steps, **preview** and **apply**.

## Steps

1. **Create the account** the file belongs to, a bank or credit card account in the file's currency
   ([Set up accounts](guide/set-up-accounts)).
2. **Preview** with [import.preview](method/import.preview). `format` is `csvTransactions` or `qif`. The file's content goes
   in `files` as `text` (or `base64`); the server never reads a path you name. From the command line,
   `red-ale exec --file bank.csv import.preview '{...}'` reads the file on your side and adds it to `files`.
   - For CSV, give a `layout`: the separator, how many lines to skip, the date and number formats, the `baseAccount` and
     what each column is (`Date`, `Description`, `Amount`, `Account`, and so on).
   - For QIF, the dates and the accounts the file names are read for you.
3. **Read the proposal.** Its `id` names it. `transactions` lists what would be done to each row: `add` it, `clear` an
   existing transaction it matches, `update` one, or `skip`. A row that matches an existing transaction lists the
   `matches`. `errors` are rows that cannot be imported and are left out. A transaction with no account to balance it is
   marked `imbalance`.
4. **Apply** with [import.apply](method/import.apply) and the proposal `id`. In `changes` give the `account` that balances
   a row, or change a row's action. `skipErrors: true` applies the good rows when others have errors. Applying changes the
   book in one step: everything or nothing.
5. **Or discard** the proposal with [import.discard](method/import.discard). Proposals are kept in memory only, so one
   made before the book was opened is gone.
6. **Learn.** When you give a transfer account for a description, red-ale remembers it, so that the next import suggests it.
   [importMap.list](method/importMap.list) shows what it has learned and [importMap.delete](method/importMap.delete) forgets
   an entry.
7. **Save** with [book.save](method/book.save).

The proposal's `id` counts up in each server process (`imp-1`, `imp-2`, ...), so the apply call below is shown and not run
as an Example.

## Example

A bank's CSV with two rows. The preview adds both and notes that neither is balanced yet.

```example
{"method": "account.create", "params": {"name": "Checking", "type": "BANK", "commodity": "USD"}}
{"result": "…"}
{"method": "import.preview", "params": {"format": "csvTransactions", "files": [{"name": "bank.csv", "text": "Date,Description,Amount\n2024-06-03,Coffee,-3.50\n2024-06-04,Lunch,-12.00\n"}], "layout": {"format": "csv", "separators": ",", "skipStartLines": 1, "dateFormat": "y-m-d", "currencyFormat": "period", "encoding": "utf-8", "baseAccount": "Checking", "columns": ["Date", "Description", "Amount"]}}}
{"result": {"id": "…", "format": "csvTransactions", "summary": {"add": 2, "imbalanced": 2, "errors": 0, "…": "…"}, "transactions": [{"index": 0, "description": "Coffee", "datePosted": "2024-06-03", "action": "add", "…": "…"}, {"index": 1, "description": "Lunch", "action": "add", "…": "…"}], "…": "…"}}
```

Applying it, with Food as the account that balances the first row (the `id` is the one the preview gave):

```json
{"method": "import.apply", "params": {"proposal": "imp-1", "changes": [{"index": 0, "account": "Food"}]}}
```
