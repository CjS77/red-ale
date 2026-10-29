<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/price.get
related: method/price.list, method/price.update
---
Describes one price by GUID: the commodity, the currency, the time and local date, the source, the type and the value.
GUIDs come from [price.list](method/price.list) and [price.add](method/price.add).

## Examples

```example fixture=reports/test-data
{"method": "price.get", "params": {"price": "e0000000000000000000000000000001"}}
{"result": {"guid": "e0000000000000000000000000000001", "commodity": "CURRENCY::GBP", "currency": "CURRENCY::USD", "date": "2000-02-15", "source": "user:xfer-dialog", "type": "test", "value": {"num": "3", "denom": "2"}, "…": "…"}}
```
