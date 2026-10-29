<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: guide/credit-cards
title: Credit cards
summary: Record charges, payments, refunds and interest on a credit card as a liability account, and know what the sign of its balance means.
related: method/account.create, method/transaction.post, method/account.balance, method/reconcile.start, method/reconcile.finish, concept/account-types
from: guide/ch_cc.docbook#chapter_cc
---
A credit card is money you owe, so it belongs in a liability account of type `CREDIT`, not in your assets. Every charge
makes the debt larger; every payment makes it smaller. Keeping the card as an account of its own lets you see what you
owe at any moment and reconcile it against each monthly statement.

## Signs

A `CREDIT` account has a credit balance when you owe money, which is a negative number in the book's raw sign. A charge
is a credit to the card (`-`) and a debit to the expense (`+`). A payment is the reverse: a debit to the card (`+`) and
a credit to the bank (`-`). So a balance of `-185.00` means you owe 185.00.

## Steps

1. **Create the card account** with [account.create](method/account.create), type `CREDIT`, in the card's currency.
   Create an expense account for the interest the card charges.
2. **Record each charge** with [transaction.post](method/transaction.post): the expense `+`, the card `-`.
3. **Record a refund** as the opposite of a charge: the card `+`, the expense `-`.
4. **Record interest and fees** from the statement: the interest expense `+`, the card `-`.
5. **Record a payment**: the card `+`, the bank account `-`.
6. **Check what you owe** with [account.balance](method/account.balance).
7. **Reconcile** against the statement as in [Reconcile an account](guide/reconcile-an-account), with the statement's
   ending balance given in the card's raw sign (negative for money owed).

## Example

```example now=2024-07-31T12:00:00Z
{"method": "account.create", "params": {"name": "Checking", "type": "BANK", "commodity": "USD"}}
{"result": "…"}
{"method": "account.create", "params": {"name": "Visa", "type": "CREDIT", "commodity": "USD"}}
{"result": "…"}
{"method": "account.create", "params": {"name": "Groceries", "type": "EXPENSE", "commodity": "USD"}}
{"result": "…"}
{"method": "account.create", "params": {"name": "Card interest", "type": "EXPENSE", "commodity": "USD"}}
{"result": "…"}
{"method": "transaction.post", "params": {"datePosted": "2024-07-03", "description": "Supermarket", "splits": [{"account": "Groceries", "value": "120.00"}, {"account": "Visa", "value": "-120.00"}]}}
{"result": {"balanced": true, "…": "…"}}
{"method": "transaction.post", "params": {"datePosted": "2024-07-10", "description": "Supermarket", "splits": [{"account": "Groceries", "value": "80.00"}, {"account": "Visa", "value": "-80.00"}]}}
{"result": {"balanced": true, "…": "…"}}
{"method": "transaction.post", "params": {"datePosted": "2024-07-12", "description": "Returned goods", "splits": [{"account": "Visa", "value": "30.00"}, {"account": "Groceries", "value": "-30.00"}]}}
{"result": {"balanced": true, "…": "…"}}
{"method": "transaction.post", "params": {"datePosted": "2024-07-20", "description": "Interest", "splits": [{"account": "Card interest", "value": "15.00"}, {"account": "Visa", "value": "-15.00"}]}}
{"result": {"balanced": true, "…": "…"}}
{"method": "account.balance", "params": {"account": "Visa"}}
{"result": {"balances": {"commodity": "CURRENCY::USD", "balance": {"num": "-18500", "denom": "100"}, "…": "…"}, "…": "…"}}
{"method": "transaction.post", "params": {"datePosted": "2024-07-25", "description": "Card payment", "splits": [{"account": "Visa", "value": "100.00"}, {"account": "Checking", "value": "-100.00"}]}}
{"result": {"balanced": true, "…": "…"}}
{"method": "account.balance", "params": {"account": "Visa"}}
{"result": {"balances": {"balance": {"num": "-8500", "denom": "100"}, "…": "…"}, "…": "…"}}
```
