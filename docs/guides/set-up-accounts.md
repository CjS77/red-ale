<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: guide/set-up-accounts
title: Set up accounts
summary: Build a chart of accounts for a household (assets, credit cards, income, expenses), change it, renumber it, move and delete accounts, and enter opening balances.
related: method/account.create, method/account.update, method/account.renumber, method/account.cascade, method/account.move, method/account.delete, method/account.openingBalanceAccount, method/export.accounts, method/transaction.post, method/account.types, gnucash/not-in-red-ale, concept/loans-and-interest, concept/other-assets-and-depreciation, gnucash/file-menu, gnucash/account-menu, concept/chart-of-accounts
from: guide/ch_basics.docbook#basics-expt-acct
---
The chart of accounts is the skeleton of the book: every transaction moves value between its accounts. A household
usually needs somewhere to keep money (assets), somewhere that records what is owed (liabilities), and categories for
money coming in (income) and going out (expenses). This guide builds a small chart, adjusts it, and starts it off with
the balances you have today.

## Steps

1. **Plan the tree.** Accounts nest: `Assets:Current Assets:Checking`. A parent that only groups its children is
   usually a placeholder, which cannot take transactions itself. [account.types](method/account.types) lists the types
   (`BANK`, `CASH`, `CREDIT`, `ASSET`, `LIABILITY`, `INCOME`, `EXPENSE`, `EQUITY`, `STOCK`, and so on); a child's type
   must suit its parent's.
2. **Create the parents, then the children** with [account.create](method/account.create). `parent` is the full name
   of the parent (the root by default) and `commodity` the currency; a child takes its parent's commodity if you leave
   it out.
3. **Edit** with [account.update](method/account.update): name, code, description, notes, colour, hidden, placeholder.
   You cannot change the commodity of an account that already has splits, or the type to one the parent does not
   allow.
4. **Number the accounts** with [account.renumber](method/account.renumber): give a subtree a `prefix` and an
   `interval`, and its accounts get codes `1-100`, `1-200` and so on.
5. **Set one property down a whole subtree** with [account.cascade](method/account.cascade), for example a colour or
   `hidden`.
6. **Move** an account to another parent with [account.move](method/account.move), with its sub-accounts and
   transactions.
7. **Delete** with [account.delete](method/account.delete). An account that has transactions is refused unless you say
   where they go: `transactions: {"moveTo": "<account>"}` moves its splits, or you can delete the transactions.
   Sub-accounts are moved up or deleted the same way.
8. **Enter opening balances.** The money you already have when you start is recorded as a transaction against an Equity
   account called Opening Balances. [account.openingBalanceAccount](method/account.openingBalanceAccount) finds it.
   Create it with `openingBalance: true` if there is none, then post one transaction per opening balance with
   [transaction.post](method/transaction.post): a positive value on the asset account and the same amount, negative, on
   Opening Balances. For a credit card or loan, reverse the signs.
9. **Keep a copy of the chart** with [export.accounts](method/export.accounts), which writes the accounts and their
   commodities to a file you can import into another book.

## Example

A small household chart, with an opening balance.

```example now=2024-07-01T12:00:00Z
{"method": "account.create", "params": {"name": "Assets", "type": "ASSET", "commodity": "USD", "placeholder": true}}
{"result": {"fullName": "Assets", "placeholder": true, "…": "…"}}
{"method": "account.create", "params": {"name": "Checking", "type": "BANK", "parent": "Assets"}}
{"result": {"fullName": "Assets:Checking", "commodity": "CURRENCY::USD", "…": "…"}}
{"method": "account.create", "params": {"name": "Wallet", "type": "CASH", "parent": "Assets"}}
{"result": {"fullName": "Assets:Wallet", "…": "…"}}
{"method": "account.create", "params": {"name": "Expenses", "type": "EXPENSE", "commodity": "USD", "placeholder": true}}
{"result": {"fullName": "Expenses", "…": "…"}}
{"method": "account.create", "params": {"name": "Food", "type": "EXPENSE", "parent": "Expenses"}}
{"result": {"fullName": "Expenses:Food", "…": "…"}}
{"method": "account.update", "params": {"account": "Assets:Wallet", "description": "Cash on hand"}}
{"result": {"fullName": "Assets:Wallet", "description": "Cash on hand", "…": "…"}}
{"method": "account.renumber", "params": {"account": "Assets", "prefix": "1", "interval": 100}}
{"result": [{"account": "…", "fullName": "Assets:Wallet", "code": "1-100"}, {"account": "…", "fullName": "Assets:Checking", "code": "1-200"}]}
{"method": "account.openingBalanceAccount", "params": {"currency": "USD"}}
{"result": {"account": null, "foundBy": null, "featureUsed": false}}
{"method": "account.create", "params": {"name": "Opening Balances", "type": "EQUITY", "commodity": "USD", "openingBalance": true}}
{"result": {"fullName": "Opening Balances", "…": "…"}}
{"method": "transaction.post", "params": {"datePosted": "2024-07-01", "description": "Opening balance", "splits": [{"account": "Assets:Checking", "value": "1250.00"}, {"account": "Opening Balances", "value": "-1250.00"}]}}
{"result": {"balanced": true, "…": "…"}}
{"method": "account.move", "params": {"account": "Assets:Wallet", "parent": "Assets:Checking"}}
{"result": {"fullName": "Assets:Checking:Wallet", "…": "…"}}
{"method": "account.delete", "params": {"account": "Assets:Checking:Wallet"}}
{"result": {"deletedAccounts": ["…"], "…": "…"}}
{"method": "account.balance", "params": {"account": "Assets:Checking"}}
{"result": {"balances": {"commodity": "CURRENCY::USD", "balance": {"num": "125000", "denom": "100"}, "…": "…"}, "…": "…"}}
```
