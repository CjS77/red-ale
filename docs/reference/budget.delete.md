<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/budget.delete
related: method/budget.create, method/budget.copy
---
Deletes a budget with all its amounts and notes. Deleting the last budget unsets the book feature `Use natural signs in
budget amounts`, as GnuCash does (`featureUnset` says so). The book option `Default Budget` is left alone even if it
names the deleted budget; a stale one falls back to the first budget.

## Examples

```example
{"method": "budget.create", "params": {"name": "2024"}}
{"result": "…"}
{"method": "budget.delete", "params": {"budget": "00ed0000000000000000000000000001"}}
{"result": {"deleted": true, "featureUnset": true}}
```
