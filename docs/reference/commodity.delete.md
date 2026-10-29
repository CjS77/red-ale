<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/commodity.delete
related: method/commodity.create, method/price.removeOld, error/conflict
---
Deletes a security that nothing uses. It is refused with `conflict` while an account, a transaction (as its currency)
or a business object uses it, or while it has prices, unless `deletePrices` is true, which deletes those prices with it.
GnuCash's dialog would leave the prices of a deleted currency dangling; red-ale will not. Currencies cannot be deleted.

## Examples

```example
{"method": "commodity.create", "params": {"namespace": "NASDAQ", "mnemonic": "AAPL", "fullname": "Apple Inc"}}
{"result": "…"}
{"method": "commodity.delete", "params": {"commodity": "NASDAQ::AAPL"}}
{"result": {"deletedPrices": []}}
{"method": "commodity.delete", "params": {"commodity": "USD"}}
{"error": {"code": 2001, "message": "…", "data": {"help": "error/invalid"}}}
```
