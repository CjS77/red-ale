<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: concept/trading-accounts
title: Trading accounts
summary: With the book option Use Trading Accounts on, every Transaction that crosses Commodities also records what was bought and sold in TRADING Accounts, so amounts balance per Commodity as well as values. What the option does, and what to expect.
related: concept/value-and-amount, concept/commodities-and-currencies, concept/account-types, method/book.options, method/book.setOptions, method/transaction.post
from: guide/ch_currency.docbook#trading_acct_concepts
---
A Transaction in two Commodities balances by *value* (in the Transaction currency) but not by *amount*: you spent 100
USD and received 92 EUR, so the amounts, being in different Commodities, sum to nothing in particular. **Trading
accounts** make the books balance per Commodity too.

In accounting terms they are specialised equity Accounts that record changes in income which come from Transactions
between two or more currencies (or securities and currencies). In GnuCash they are Accounts of type `TRADING`.
Extended, the accounting equation reads:

```
Assets = Liabilities + Equity + (Income - Expenses) + Trading
```

## The option

Trading accounts are off by default. Switch them on with the book option **Use trading accounts**
(`useTradingAccounts`), read by [`book.options`](method/book.options) and set by
[`book.setOptions`](method/book.setOptions). Changing it alters no Transaction: an existing Transaction gets trading
Splits the next time it is changed.

## What it does

With the option on, every Transaction that is created or changed is kept **balanced with trading accounts**: its
non-trading Split values sum to zero; its trading Split values sum to zero; and, per Commodity, the amounts of its
Splits sum to zero. When a Transaction does not already balance that way, red-ale removes its trading Splits and adds
new ones, in an Account for each Commodity under `Trading:<namespace>:<mnemonic>`:

```
Trading
  CURRENCY
    EUR
    USD
```

They are created when needed. For the Transaction above, a Split in `Trading:CURRENCY:USD` and one in
`Trading:CURRENCY:EUR` make both Commodities' amounts balance.

**⚠** *Trading splits* are added and removed by red-ale itself, as GnuCash does, and are not entries of your own. Do not post to a Trading
Account by hand; a change to the Transaction replaces them. `balanced` in a Transaction's result follows the option:
without it, balanced means only that the values sum to zero.

The Account tree lists these Accounts like any other, but their balances measure currency gains and losses, not money
you hold. Reports can show them.

## If you do not use them

Leave the option off and record foreign-currency Transactions by hand: transfer money into the foreign Account at the
rate you got, and record gains and losses as ordinary Transactions with an income or expense Account. Trading accounts earn their place when you buy and sell across currencies often
and want the gains and losses worked out for you.

A book that uses trading accounts opens for editing: red-ale applies the same rule as GnuCash on every change, in the
same Accounts with the same figures.
