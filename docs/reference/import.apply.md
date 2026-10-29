<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/import.apply
related: method/import.preview, method/import.discard, method/importMap.list
---
Applies a held Import proposal as one change, with the changes you make to it, or does nothing. It is destructive: it
adds transactions, accounts, prices, documents or customers to the book.

`changes` alter a record by its `index`: a new `action` (`add`, `clear`, `update`, `skip`; `clear` and `update` need a
`match`, the split GUID of one of the transaction's `matches`), and for an `add` the `account` that balances it and its
`amount` when that account is in another commodity. The account you choose is learned into the import map, so the next
file is proposed with it. Records you do not name keep the proposal's choice. Errors in the proposal stop the apply
unless you pass `skipErrors`, which leaves those rows out.

The result lists what was added, cleared, updated and skipped, the accounts and commodities created, every
`Imbalance-<CUR>` split made, and what was learned. A proposal stays held after a failed apply, so you can fix it and
try again. If the book has changed since the preview, the proposal is refused with `conflict`: preview again.

## Examples

A proposal that is not held, because it was applied, discarded, pushed out or made before the book was opened, is
refused:

```example
{"method": "import.apply", "params": {"proposal": "imp-999999"}}
{"error": {"code": 2000, "message": "…", "data": {"kind": "importProposal", "id": "imp-999999", "help": "error/not-found"}}}
```

A bank statement previewed and applied, giving Food as the account that balances Coffee. Proposal IDs count up from
`imp-1` in each server. Lunch, given no account, is balanced against `Imbalance-USD`:

```example
{"method": "account.create", "params": {"name": "Checking", "type": "BANK", "commodity": "USD"}}
{"result": "…"}
{"method": "account.create", "params": {"name": "Food", "type": "EXPENSE", "commodity": "USD"}}
{"result": "…"}
{"method": "import.preview", "params": {"format": "csvTransactions", "files": [{"name": "bank.csv", "text": "Date,Description,Amount\n2024-06-03,Coffee,-3.50\n2024-06-04,Lunch,-12.00\n"}], "layout": {"format": "csv", "separators": ",", "skipStartLines": 1, "dateFormat": "y-m-d", "currencyFormat": "period", "encoding": "utf-8", "baseAccount": "Checking", "columns": ["Date", "Description", "Amount"]}}}
{"result": {"id": "imp-1", "…": "…"}}
{"method": "import.apply", "params": {"proposal": "imp-1", "changes": [{"index": 0, "account": "Food"}]}}
{"result": {"added": [{"index": 0, "transaction": "…"}, {"index": 1, "transaction": "…"}], "skipped": [], "imbalanceSplits": [{"index": 1, "account": "…", "value": {"num": "1200", "denom": "100"}, "…": "…"}], "learned": "…", "warnings": "…", "…": "…"}}
```
