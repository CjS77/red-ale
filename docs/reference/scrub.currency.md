<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/scrub.currency
related: method/scrub.checkAndRepair, method/transaction.update
---
Gives a transaction whose currency is not a currency (a security, say) the most common currency of its splits, and
re-values its splits to match. Scope it with `transaction`, or `account` and `subaccounts`; with neither it covers the
whole book. A clean book gives an empty `repairs` list. To change a transaction's currency on purpose, use
[transaction.update](method/transaction.update).

## Examples

```example fixture=gnucash/taxreport
{"method": "scrub.currency", "params": {}}
{"result": {"repairs": []}}
```
