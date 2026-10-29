<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/price.update
related: method/price.get, method/price.add, error/conflict
---
Edits a price as the price editor does: its value, time, source, type, commodity or currency. Changing the commodity,
currency or time re-adds the price, so the same-day rules apply against the others. Where GnuCash would then drop the
edited price itself (a better-ranked same-day price exists, or an identical one), red-ale refuses with `conflict` and
changes nothing. `type: null` clears the type.

## Examples

```example fixture=reports/test-data
{"method": "price.update", "params": {"price": "e0000000000000000000000000000001", "value": "1.55", "type": "last"}}
{"result": {"price": {"guid": "e0000000000000000000000000000001", "type": "last", "value": {"num": "155", "denom": "100"}, "…": "…"}, "replaced": null}}
```
