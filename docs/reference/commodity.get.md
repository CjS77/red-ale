<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/commodity.get
related: method/commodity.list, method/commodity.update
---
Describes one commodity by its `NAMESPACE::MNEMONIC` name (a bare `USD` names a currency): fullname, exchange code,
fraction (its smallest unit, `100` for cents), quote settings, symbols and how much of the book uses it. The
`displaySymbol` is what GnuCash prints amounts with: the user symbol, else the default symbol, else the mnemonic.

## Examples

```example fixture=reports/test-data
{"method": "commodity.get", "params": {"commodity": "USD"}}
{"result": {"id": "CURRENCY::USD", "mnemonic": "USD", "fullname": "US Dollar", "cusip": "840", "fraction": 100, "isCurrency": true, "builtIn": true, "displaySymbol": "$", "…": "…"}}
```
