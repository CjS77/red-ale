<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: gnucash/account-menu
title: Accounts and the Actions menu
summary: GnuCash's Edit and Actions menus for Accounts mapped to methods - New Account, Edit Account, Delete Account, Cascade, Renumber Subaccounts, Transfer, Reconcile, Auto clear, Check and Repair, View Lots. Find Account, Open Account and the New Account Hierarchy assistant have no counterpart.
related: concept/chart-of-accounts, concept/account-types, concept/reconciliation, concept/imbalance-and-scrubs, method/account.create, method/account.update, method/account.delete, method/reconcile.start, method/scrub.checkAndRepair
from: manual/ch_GUIMenus.docbook#AccTree-actions-menu
---
The Account tree window's **Edit** and **Actions** menus are about Accounts. Scheduled transactions, budgets, stock splits
and the other assistants are on [Tools and scheduled transactions](gnucash/tools-menu) and
[Stock assistants](gnucash/stock-assistants).

## Edit menu

| GnuCash | red-ale |
|---|---|
| Edit Account | [`account.update`](method/account.update): name, type, commodity, code, description, notes, placeholder, hidden, colour, tax flag, balance limits, associated Accounts. A type may change only to a compatible one when the Account has Splits. [`account.get`](method/account.get) reads what is there |
| Delete Account... | [`account.delete`](method/account.delete). An Account with Transactions needs a choice: delete them (whole Transactions go) or `moveTo` another Account of the same type. The same for a sub-account, of which at most one is allowed. GnuCash, given no choice, deletes the Splits and leaves `Imbalance` ones; red-ale never does |
| Cascade Account Properties... | [`account.cascade`](method/account.cascade): colour, placeholder or hidden on an Account and all below it |
| Renumber Subaccounts... | [`account.renumber`](method/account.renumber): a prefix, an interval and a number of digits |
| Find Account | [`account.list`](method/account.list) or [`account.tree`](method/account.tree), filtered by you |
| Open Account, Open Subaccount | [`transaction.list`](method/transaction.list) for that Account and a date range; for its sub-accounts too, list each. [`account.balance`](method/account.balance) gives its balance, with `includeSubaccounts` for the subtree's |
| Find... | [`transaction.find`](method/transaction.find) |
| Cut, Copy, Paste | Not provided. Move an Account with [`account.move`](method/account.move) |
| Preferences, Style Sheets | [From GnuCash: Preferences and book options](gnucash/preferences-and-book-options), [Reports](gnucash/reports-menu) |
| Tax Report Options | [`account.update`](method/account.update) with `taxRelated` and the US tax-code fields. The flag and codes are kept so GnuCash's tax reports still work on the book; red-ale has no tax report |

## Actions menu

| GnuCash | red-ale |
|---|---|
| New Account... | [`account.create`](method/account.create). Give `name`, `type` and `commodity`; `parent` defaults to the root. `openingBalance` adds the opening balance Transaction against the opening-balance Account ([`account.openingBalanceAccount`](method/account.openingBalanceAccount)). It also takes the Account's SCU, notes, colour, balance limits and the Accounts it remembers for dividends and gains |
| New Account Hierarchy... | **Not provided.** There are no Account templates ([File menu](gnucash/file-menu)) |
| Transfer... | [`transaction.transfer`](method/transaction.transfer), with a `rate` or `toAmount` when the Accounts' Commodities differ |
| Reconcile... | [`reconcile.start`](method/reconcile.start), `reconcile.status`, `reconcile.finish` and `reconcile.postpone`: see [Reconciliation](concept/reconciliation) |
| Auto clear... | [`reconcile.findAutoClear`](method/reconcile.findAutoClear), then [`reconcile.autoClear`](method/reconcile.autoClear) |
| Stock Split... | [`stock.split`](method/stock.split): see [Stock assistants](gnucash/stock-assistants) |
| View Lots... | [`lot.list`](method/lot.list), [`lot.get`](method/lot.get); see [Tools](gnucash/tools-menu) |
| Check & Repair > Check & Repair Account | [`scrub.checkAndRepair`](method/scrub.checkAndRepair) with `account` |
| Check & Repair > Check & Repair Subaccounts | The same with `subaccounts: true` |
| Check & Repair > Check & Repair All | The same with no Account |
| Reset Warnings... | Not provided. red-ale asks nothing |
| Rename Page | Not provided |
| Online Actions (Get Balance, Get Transactions, Issue SEPA Transaction...) | **Not supported.** See [Not in red-ale](gnucash/not-in-red-ale) |
| Scheduled Transactions, Budget | [Tools and scheduled transactions](gnucash/tools-menu) |

**Check & Repair** runs only when you ask. GnuCash also scrubs a book as it loads; red-ale does not
([Imbalance, orphans and scrubs](concept/imbalance-and-scrubs)). Add `lots: true` to include the Lot scrub. Each repair is
reported in `repairs`.

## Other things the New Account dialog did

| GnuCash | red-ale |
|---|---|
| Security/Currency Select... | `commodity` as `USD` or `NASDAQ::AAPL`; make a new security first with [`commodity.create`](method/commodity.create) ([Commodities and currencies](concept/commodities-and-currencies)) |
| Get Online Quotes (on a security) | `quoteFlag` and `quoteSource` on the Commodity; see [Prices and exchange rates](concept/prices-and-exchange-rates) |
| Placeholder, Hidden | `placeholder`, `hidden` |
| Show hidden accounts, Filter by... | `excludeHidden` on `account.list` and `account.tree` |
