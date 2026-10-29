<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/account.update
related: method/account.get, method/account.move, method/account.types, error/conflict
---
Changes an account's name, type, commodity, smallest fraction, code, description, notes, flags (`placeholder`,
`hidden`), tax settings, colour, balance limits, opening balance flag or associated accounts. A field you leave out is
kept; an empty string clears a text field.

GnuCash's account dialog rules apply. The commodity can change only while the account has no splits, and a type only
to one in `changesWithSplits` once it has; changing a type that sub-accounts cannot sit under needs
`retypeChildren: true`. A new name must not clash with a sibling's and may not contain `:`. Existing amounts are not
re-rounded when `scu` changes.

## Examples

```example fixture=reports/test-data
{"method": "account.update", "params": {"account": "Root:Asset:Wallet", "code": "110", "description": "Cash on hand"}}
{"result": {"fullName": "Root:Asset:Wallet", "code": "110", "description": "Cash on hand", "…": "…"}}
```

The commodity of an account with transactions cannot change:

```example fixture=reports/test-data
{"method": "account.update", "params": {"account": "Root:Asset:Bank", "commodity": "EUR"}}
{"error": {"code": 2003, "message": "…", "data": {"help": "error/conflict"}}}
```
