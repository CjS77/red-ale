<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/commodity.create
related: method/commodity.update, method/account.create, error/invalid, error/conflict
---
Creates a security (a stock, fund or other non-currency commodity), with the security editor's checks. Give a
`namespace` (an exchange such as `NASDAQ`), a `mnemonic` (the ticker) and a `fullname`; `fraction` is the smallest unit
(default 10000), and `quoteFlag` and `quoteSource` set it up for online prices. Currencies cannot be created: the
`CURRENCY` namespace is refused with `invalid`, and an existing name with `conflict`. Names with `::` are refused
because they would make `NAMESPACE::MNEMONIC` ambiguous.

## Examples

```example
{"method": "commodity.create", "params": {"namespace": "NASDAQ", "mnemonic": "AAPL", "fullname": "Apple Inc", "fraction": 10000}}
{"result": {"id": "NASDAQ::AAPL", "fullname": "Apple Inc", "fraction": 10000, "isCurrency": false, "builtIn": false, "displaySymbol": "AAPL", "…": "…"}}
{"method": "commodity.create", "params": {"namespace": "CURRENCY", "mnemonic": "ZZZ", "fullname": "Nope"}}
{"error": {"code": 2001, "message": "…", "data": {"help": "error/invalid"}}}
```
