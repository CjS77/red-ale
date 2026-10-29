<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/importMap.delete
related: method/importMap.list
---
Deletes import map entries: named ones (`entries`, each an account, a `kind` and a `key`), every entry of a kind on an
account (leave out the `key`), or every invalid one (`invalid: true`), as GnuCash's import map editor does. Give either
`entries` or `invalid`, not both. A `bayes` token is kept once per target account; `target` picks one, otherwise the
token goes for all of them. It is destructive: the matcher forgets what it learned.

## Examples

```example fixture=synthetic/rich
{"method": "importMap.delete", "params": {"invalid": true}}
{"result": {"deleted": []}}
{"method": "importMap.delete", "params": {"entries": [{"account": "Assets:Cash", "kind": "desc", "key": "Coffee"}]}}
{"error": {"code": 2000, "message": "…", "data": {"help": "error/not-found", "…": "…"}}}
```
