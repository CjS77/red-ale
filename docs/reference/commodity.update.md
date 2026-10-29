<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/commodity.update
related: method/commodity.get, method/commodity.create
---
Edits a security (any field, including its namespace and mnemonic) or a currency's quote flag, quote time zone and
symbol. Fields you leave out are kept; `null` clears `cusip`, `quoteSource`, `quoteTz` or `userSymbol`. A currency's
other fields are fixed and a change is refused with `invalid`.

Renaming a security changes every account, transaction and price that names it. Changing a commodity's `fraction`
does not re-round amounts already stored.

## Examples

```example
{"method": "commodity.create", "params": {"namespace": "NASDAQ", "mnemonic": "AAPL", "fullname": "Apple Inc"}}
{"result": "…"}
{"method": "commodity.update", "params": {"commodity": "NASDAQ::AAPL", "fullname": "Apple Inc.", "quoteFlag": true, "quoteSource": "yahoo_json"}}
{"result": {"id": "NASDAQ::AAPL", "fullname": "Apple Inc.", "quoteFlag": true, "quoteSource": "yahoo_json", "…": "…"}}
{"method": "commodity.update", "params": {"commodity": "EUR", "fullname": "Euro, renamed"}}
{"error": {"code": 2001, "message": "…", "data": {"help": "error/invalid"}}}
```
