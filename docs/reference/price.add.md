<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/price.add
related: method/price.list, method/price.update, method/price.latest
---
Adds a price: `value` units of `currency` per unit of `commodity` on a `date` (or at an exact `time`), kept exactly as
given, so `"1.0850"` is stored as 10850/10000. The `source` defaults to `user:price-editor`, the price editor's, and
must be one of GnuCash's eleven source strings (`Finance::Quote`, `user:price`, `user:xfer-dialog`, …) so that
GnuCash reads it back the same.

A same-day price of the pair (in either direction) is handled as GnuCash does. One with a better-ranked source blocks
the new one: `outcome` is `rejected` and `existing` names it. Otherwise it is replaced (`outcome` `replaced`, with the
old GUID in `replaced`). Adding the same price twice therefore replaces the first.

## Examples

```example
{"method": "price.add", "params": {"commodity": "EUR", "currency": "USD", "value": "1.0850", "date": "2024-06-28"}}
{"result": {"outcome": "added", "price": {"commodity": "CURRENCY::EUR", "currency": "CURRENCY::USD", "date": "2024-06-28", "source": "user:price-editor", "value": {"num": "10850", "denom": "10000"}, "…": "…"}, "replaced": null, "existing": null}}
{"method": "price.add", "params": {"commodity": "EUR", "currency": "USD", "value": "1.0900", "date": "2024-06-28", "source": "Finance::Quote"}}
{"result": {"outcome": "rejected", "price": null, "replaced": null, "existing": "…"}}
```
