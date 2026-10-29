<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/billTerm.list
related: method/billTerm.get, method/billTerm.create
---
Lists the visible billing terms, by name. Posting a document freezes a copy of its term, so that editing the term later
leaves posted documents alone. These copies are invisible and are listed only with `includeFrozen`.

## Examples

```example fixture=synthetic/business-editable
{"method": "billTerm.list", "params": {}}
{"result": {"billTerms": [{"name": "Net30", "type": "days", "dueDays": 30, "…": "…"}, {"name": "Prox15", "type": "proximo", "…": "…"}]}}
```
