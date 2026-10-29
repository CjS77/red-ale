<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: guide/transfer-between-accounts
title: Transfer between accounts
summary: Move an amount from one account to another with transaction.transfer, including a transfer between accounts in different currencies at an exchange rate.
related: method/transaction.transfer, method/transaction.post, method/price.add, method/account.balance
---
Moving money between two of your own accounts (bank to savings, bank to cash, one currency to another) is the most
common transaction, and [transaction.transfer](method/transaction.transfer) does it in one call. It is GnuCash's
Transfer dialog: you name the two accounts and an amount, and red-ale builds the two splits.

## Steps

1. **Pick the accounts.** `from` is the account the amount leaves; it must hold a currency. `to` is the account it
   reaches.
2. **Give the amount**, in the `from` account's currency, as an exact string. Add `date`, `description`, `num`, `notes`
   or `memo` as needed.
3. **Same currency?** That is all. The transaction has two splits of the same value.
4. **Different currencies?** Give the `rate`: units of the `to` commodity per unit of the `from` currency. The `from`
   account loses 100.00 USD and the `to` account, in GBP, gains 80.00 at a rate of `0.80`. Or give `toAmount`, the
   amount received, and the rate is worked out from it. Giving both is refused if they disagree.
5. **The price is remembered.** Unless you pass `recordPrice: false`, the rate is added to the price database, as the
   dialog does, so reports can convert later. See [Multiple currencies](guide/multiple-currencies).
6. **Check** with [account.balance](method/account.balance).

For anything the Transfer dialog cannot express, such as a transfer with a fee, use
[transaction.post](method/transaction.post) with all the splits.

## Example

```example fixture=reports/test-data
{"method": "transaction.transfer", "params": {"from": "Root:Asset:Bank", "to": "Root:Asset:Wallet", "amount": "40.00", "date": "2024-06-09", "description": "Cash for the weekend"}}
{"result": {"transaction": {"description": "Cash for the weekend", "balanced": true, "…": "…"}, "…": "…"}}
{"method": "transaction.transfer", "params": {"from": "Root:Asset:Bank", "to": "Root:Asset:GBP Bank", "amount": "100.00", "date": "2024-06-10", "description": "Top up", "rate": "0.80"}}
{"result": {"transaction": {"currency": "CURRENCY::USD", "description": "Top up", "splits": [{"accountName": "Root:Asset:GBP Bank", "value": {"num": "10000", "denom": "100"}, "amount": {"num": "8000", "denom": "100"}, "…": "…"}, {"accountName": "Root:Asset:Bank", "value": {"num": "-10000", "denom": "100"}, "…": "…"}], "…": "…"}, "…": "…"}}
```
