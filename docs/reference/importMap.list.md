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

```example fixture=synthetic/rich
{"method": "importMap.list", "params": {}}
{"result": {"entries": []}}
```
