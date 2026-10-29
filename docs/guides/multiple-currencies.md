<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: guide/multiple-currencies
title: Multiple currencies
summary: Keep an account in a foreign currency, buy that currency, spend it, and record the exchange rate, with each split's value in the transaction currency and its amount in the account's.
related: method/account.create, method/price.add, method/transaction.post, method/transaction.transfer, method/account.balance, method/price.list, method/currency.list
---
If you hold money in a foreign currency, give it an account in that currency. The book then keeps two numbers for every
split, and they are not the same thing:

- the split's **value** is in the *transaction's currency*, the one the transaction as a whole is in;
- the split's **amount** is in the *account's commodity*, which can be another currency.

For a purchase of 100.00 EUR for 108.50 USD, the euro account's split has an amount of 100.00 (euros) and a value of
108.50 (dollars); the dollar account's split has both equal to 108.50. The values sum to zero, and the ratio of value
to amount is the exchange rate.

## Steps

1. **Create the account** with [account.create](method/account.create), with `commodity` set to the currency's code
   (`EUR`). The currencies GnuCash knows are in [currency.list](method/currency.list); you do not create them.
2. **Say which currency the transaction is in.** [transaction.post](method/transaction.post) uses the first split's
   account commodity unless you set `currency`, so when the first split is in the foreign currency, pass `currency`
   explicitly. A split whose account is in another commodity than the transaction needs its `amount`.
3. **Buy the currency**: post the transaction in your home currency, with the euro split carrying both `value` and
   `amount`. For a simple move between your own accounts, [transaction.transfer](method/transaction.transfer) with a
   `rate` does the same, and records the rate.
4. **Record the rate** for reports with [price.add](method/price.add), so that balances can be shown in one currency.
   `transaction.post` records a price from a priced split when you pass `recordPrice: true`.
5. **Spend the currency**: a transaction in USD whose euro split has `value: -54.25` and `amount: -50.00` takes 50.00 EUR
   out of the euro account for a 54.25 USD expense.
6. **Check** with [account.balance](method/account.balance). A balance is in the account's own commodity; the
   [price.list](method/price.list) shows the rates the book has.

If your book uses trading accounts, see [Use trading accounts](guide/use-trading-accounts).

## Example

```example now=2024-07-15T12:00:00Z
{"method": "account.create", "params": {"name": "Checking", "type": "BANK", "commodity": "USD"}}
{"result": "…"}
{"method": "account.create", "params": {"name": "Euro Bank", "type": "BANK", "commodity": "EUR"}}
{"result": {"commodity": "CURRENCY::EUR", "…": "…"}}
{"method": "account.create", "params": {"name": "Travel", "type": "EXPENSE", "commodity": "USD"}}
{"result": "…"}
{"method": "transaction.post", "params": {"datePosted": "2024-07-02", "description": "Buy euros", "currency": "USD", "splits": [{"account": "Euro Bank", "value": "108.50", "amount": "100.00"}, {"account": "Checking", "value": "-108.50"}]}}
{"result": {"currency": "CURRENCY::USD", "balanced": true, "splits": [{"accountName": "Euro Bank", "value": {"num": "10850", "denom": "100"}, "amount": {"num": "10000", "denom": "100"}, "…": "…"}, {"accountName": "Checking", "…": "…"}], "…": "…"}}
{"method": "price.add", "params": {"commodity": "EUR", "currency": "USD", "value": "1.0850", "date": "2024-07-02"}}
{"result": {"outcome": "added", "…": "…"}}
{"method": "transaction.post", "params": {"datePosted": "2024-07-10", "description": "Hotel in Lyon", "currency": "USD", "splits": [{"account": "Travel", "value": "54.25"}, {"account": "Euro Bank", "value": "-54.25", "amount": "-50.00"}]}}
{"result": {"balanced": true, "…": "…"}}
{"method": "account.balance", "params": {"account": "Euro Bank"}}
{"result": {"balances": {"commodity": "CURRENCY::EUR", "balance": {"num": "5000", "denom": "100"}, "…": "…"}, "…": "…"}}
```
