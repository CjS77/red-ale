<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/currency.list
related: method/commodity.list, method/currency.euroConvert
---
Lists GnuCash 5.17's 227 built-in ISO 4217 currencies in code order, with each one's code, name, ISO numeric code
(`exchangeCode`), fraction, symbol and, for the euro and the currencies with a fixed rate to it, `euroRate`. Every
book can use any of them without defining it first. [commodity.list](method/commodity.list) lists these and the
book's own securities together.

## Examples

```example
{"method": "currency.list", "params": {}}
{"result": [{"id": "CURRENCY::ADF", "code": "ADF", "fullname": "Andorran Franc", "exchangeCode": "950", "fraction": 100, "symbol": "₣", "euroRate": null}, "…"]}
```
