<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/importMap.list
related: method/importMap.delete, method/import.apply
---
Lists what the importers have learned: the entries in the book that propose a transfer account for an imported
transaction. They are the `bayes` tokens GnuCash's Bayesian matcher trains, the plain `desc` and `memo` maps, the
`csvAccountMap` of account names in CSV files, and the online IDs of bank connections (`onlineId`, `hbci`,
`ofxIncomeAccount`). Each says which account holds it, its key, the account it points to and whether it is still valid.

An entry is invalid when the account it points to no longer exists; `invalidOnly` lists what
[importMap.delete](method/importMap.delete) with `invalid` would remove. A book that has never imported has none.

## Examples

A book whose checking account has learned from a few imports. GnuCash's Bayesian matcher keeps one entry per token
and account (the words of a description, and the day of the week), the plain maps one per description or memo, and
the CSV map one per account name in a file. The `Coffee` token points to an account deleted since, so it is invalid:

```example fixture=synthetic/import-map
{"method": "importMap.list", "params": {}}
{"result": {"entries": [{"account": {"guid": "…", "fullName": "Assets:Checking"}, "kind": "bayes", "key": "ACME", "target": {"guid": "…", "fullName": "Income:Salary"}, "count": 1, "valid": true}, {"kind": "bayes", "key": "Coffee", "target": null, "valid": false, "…": "…"}, {"kind": "bayes", "key": "Corner", "target": {"guid": "…", "fullName": "Expenses:Groceries"}, "…": "…"}, {"kind": "bayes", "key": "Friday", "…": "…"}, {"kind": "bayes", "key": "Grocer", "…": "…"}, {"kind": "bayes", "key": "Monday", "…": "…"}, {"kind": "bayes", "key": "Salary", "…": "…"}, {"kind": "desc", "key": "Corner Grocer", "target": {"guid": "…", "fullName": "Expenses:Groceries"}, "…": "…"}, {"kind": "memo", "key": "March rent", "target": {"guid": "…", "fullName": "Expenses:Rent"}, "…": "…"}, {"kind": "csvAccountMap", "key": "CHK-001", "target": {"guid": "…", "fullName": "Assets:Checking"}, "…": "…"}]}}
{"method": "importMap.list", "params": {"invalidOnly": true}}
{"result": {"entries": [{"account": {"guid": "…", "fullName": "Assets:Checking"}, "kind": "bayes", "key": "Coffee", "target": null, "count": 1, "valid": false}]}}
```

A book that has never imported has none:

```example fixture=synthetic/rich
{"method": "importMap.list", "params": {}}
{"result": {"entries": []}}
```
