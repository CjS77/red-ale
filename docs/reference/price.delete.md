<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/price.delete
related: method/price.removeOld, method/price.list
---
Deletes one price and returns it. To thin out many old prices at once, use
[price.removeOld](method/price.removeOld) instead.

## Examples

```example fixture=reports/test-data
{"method": "price.delete", "params": {"price": "e0000000000000000000000000000000"}}
{"result": {"guid": "e0000000000000000000000000000000", "…": "…"}}
{"method": "price.list", "params": {}}
{"result": [{"guid": "e0000000000000000000000000000001", "…": "…"}]}
```
