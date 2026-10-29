<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/transaction.transfer
related: method/transaction.post, method/price.list, error/invalid
---
GnuCash's Transfer dialog as a call: moves `amount` from one account to another in a two-split transaction. The
`from` account must hold a currency. When the two accounts are in different commodities, give the `rate` (units of
the `to` commodity per unit of the from currency) or the `toAmount` received, or leave both out and red-ale uses the
price database's price for the date; with no price at all the transfer is refused. The rate is recorded as a price
unless `recordPrice` is false.

The result gives the transaction, the amount received, the rate and what happened to the price (`created`,
`updated` or `kept`). The same account on both sides, a placeholder account, a zero amount and a zero or negative rate
are refused with `invalid`.

## Examples

Moving 100.00 USD to a pound account at 0.80:

```example fixture=reports/test-data
{"method": "transaction.transfer", "params": {"from": "Root:Asset:Bank", "to": "Root:Asset:GBP Bank", "amount": "100.00", "date": "2024-06-10", "description": "Top up", "rate": "0.80"}}
{"result": {"transaction": {"currency": "CURRENCY::USD", "description": "Top up", "splits": [{"accountName": "Root:Asset:GBP Bank", "value": {"num": "10000", "denom": "100"}, "amount": {"num": "8000", "denom": "100"}, "…": "…"}, {"accountName": "Root:Asset:Bank", "value": {"num": "-10000", "denom": "100"}, "…": "…"}], "…": "…"}, "toAmount": {"num": "8000", "denom": "100"}, "rate": {"num": "80", "denom": "100"}, "price": {"action": "created", "guid": "…"}}}
```
