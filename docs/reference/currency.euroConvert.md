<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/currency.euroConvert
related: method/currency.list, method/transaction.transfer
---
Converts between the euro and a currency it replaced, at the fixed rates fixed when the euro was introduced: to euros
divides by the rate, from euros multiplies, rounding half away from zero to the currency's fraction. One side must be
`EUR`. GnuCash returns 0 for a currency with no euro rate; red-ale refuses with `invalid`, so a zero is never taken for
an answer. For other pairs use the price database ([price.nearest](method/price.nearest)).

## Examples

```example
{"method": "currency.euroConvert", "params": {"amount": "100.00", "from": "EUR", "to": "DEM"}}
{"result": {"amount": {"num": "19558", "denom": "100"}, "rate": {"num": "195583", "denom": "100000"}}}
{"method": "currency.euroConvert", "params": {"amount": "100.00", "from": "USD", "to": "GBP"}}
{"error": {"code": 2001, "message": "…", "data": {"help": "error/invalid"}}}
```
