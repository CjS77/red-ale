<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: concept/value-and-amount
title: Value and amount
summary: A Split has a value, in the Transaction currency, and an amount, in its Account's Commodity. Both are exact rationals; values sum to zero per Transaction. GnuCash's quantity is red-ale's amount.
related: concept/transactions-and-splits, concept/commodities-and-currencies, concept/prices-and-exchange-rates, method/transaction.post, method/transaction.transfer
---
A Split has two sizes, and mixing them up is the commonest mistake with multiple currencies or securities.

- Its **value** is in the **Transaction currency**: the one currency every Split of the Transaction shares. The values
  of a Transaction's Splits sum to exactly zero.
- Its **amount** is in the **Commodity of the Split's Account**: euros in a euro account, shares in a stock account.

**⚠** GnuCash's word *amount* means exactly this quantity, not a generic number, and its files call it `quantity`.
Elsewhere in this Manual a *value* is always in the Transaction currency.

When an Account holds the Transaction currency, value and amount are equal, and red-ale fills in the amount from the
value if you leave it out. When the Account's Commodity differs, you give both, and their ratio is the price the
Split implies:

```
price = value / amount
```

Buying 10 shares of AAPL for 1,500.00 USD from a USD bank Account is a USD Transaction with two Splits:

| Account | Value (USD) | Amount |
|---------|-------------|--------|
| Broker (AAPL) | 1500.00 | 10 shares |
| Bank (USD) | -1500.00 | -1500.00 USD |

Transferring 100.00 USD to a euro Account at 0.92 is a USD Transaction too: the euro Account's Split has value 100.00
and amount 92.00. [`transaction.transfer`](method/transaction.transfer) works that out from a rate;
[`transaction.post`](method/transaction.post) takes the amounts as you give them.

```example
{"method": "account.create", "params": {"name": "Bank", "type": "BANK", "commodity": "USD"}}
{"result": "…"}
{"method": "account.create", "params": {"name": "Euro account", "type": "BANK", "commodity": "EUR"}}
{"result": "…"}
{"method": "transaction.post", "params": {"datePosted": "2024-07-01", "description": "To euros", "currency": "USD", "splits": [{"account": "Bank", "value": "-100.00"}, {"account": "Euro account", "value": "100.00", "amount": "92.00"}]}}
{"result": "…"}
{"method": "account.balance", "params": {"account": "Euro account"}}
{"result": {"balances": {"commodity": "CURRENCY::EUR", "balance": {"num": "9200", "denom": "100"}, "…": "…"}, "…": "…"}}
```

Both are kept; neither is derived from the other when a book is saved, so a change of rate later does not silently
rewrite what you recorded.

## Exact numbers

Amounts, values, prices and rates are *exact rationals*, a numerator over a denominator, never floating point. On the
wire send one as a decimal string (`"12.34"`), a rational string (`"1234/100"`) or an object
(`{"num": "1234", "denom": "100"}`); red-ale answers with the object. A JSON number is refused. This holds in every
door: JSON-RPC, MCP and the CLI.

An amount is held to its Commodity's **smallest commodity unit** (SCU): 100 for USD (cents), 1 for JPY, 10000 for a
typical share. When red-ale records a figure it rounds half up to the currency's fraction (values) or the Account's
SCU (amounts), and refuses a non-zero figure that would round to zero. GnuCash reduces fractions when it reads them
back (`-617/50`) but stores what was written (`-1234/100`), and red-ale does the same; compare figures by value, not by
their text.

## Signs

Both value and amount use the raw sign: positive is a debit, negative a credit
([Double entry](concept/double-entry)). Selling shares gives the stock Account a negative value and a negative amount,
and the bank Account that receives the proceeds a positive value and a positive amount.
