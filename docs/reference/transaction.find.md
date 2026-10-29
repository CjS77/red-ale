<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/transaction.find
related: method/transaction.list, method/transaction.get
---
Finds splits with the criteria of GnuCash's Find Transaction dialog and returns each match as a
`{transaction, split, account}` triple. A `query` is an object with one key: a criterion (`description`, `notes`,
`memo`, `num`, `action`, `datePosted`, `value`, `amount`, `reconcile`, `void`, `accounts`, and more) or `all`, `any`
or `not` of other queries. Text matches ignore case unless `matchCase` is set; `limit` keeps the last n matches.

The result lists splits, not transactions, so a two-split transaction that matches on its description appears twice.
Regular expressions (`matchesRegex`) are Rust's `regex` syntax, close to GnuCash's POSIX extended; text ties in the
sort go by code point instead of locale.

## Examples

```example fixture=reports/test-data
{"method": "transaction.find", "params": {"query": {"all": [{"description": {"text": "$22 expense"}}, {"datePosted": {"op": "before", "date": "1978-03-01"}}]}}}
{"result": {"matches": [{"transaction": "c000000000000000000000000000000d", "split": "…", "account": "…"}, {"transaction": "c000000000000000000000000000000d", "…": "…"}, {"transaction": "c0000000000000000000000000000011", "…": "…"}, {"transaction": "c0000000000000000000000000000011", "…": "…"}]}}
```
