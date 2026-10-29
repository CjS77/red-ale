<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/export.csvTree
related: method/export.csvTransactions, method/export.accounts, method/import.preview
---
Returns the account tree as CSV text, in the layout of GnuCash's Export Account Tree to CSV, which GnuCash's and
red-ale's [account import](method/import.preview) read back. It writes no file: the text is in `csv`, with `rows` for
the number of accounts. Fields are quoted only when they hold the separator, a quote or a line break, unless `quotes`
is set. Siblings come in book order but are sorted by name bytewise, where GnuCash sorts by locale.

## Examples

```example
{"method": "account.create", "params": {"name": "Checking", "type": "BANK", "commodity": "USD"}}
{"result": "…"}
{"method": "export.csvTree", "params": {}}
{"result": {"csv": "Type,Full Account Name,Account Name,Account Code,Description,Account Color,Notes,Symbol,Namespace,Hidden,Tax Info,Placeholder\r\nBANK,Checking,Checking,,,,,USD,CURRENCY,F,F,F\r\n", "rows": 1}}
```
