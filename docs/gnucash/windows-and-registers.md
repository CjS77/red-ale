<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: gnucash/windows-and-registers
title: Windows and registers
summary: GnuCash's main windows mapped to methods - the Account tree window is account.tree, an Account register is transaction.list and transaction.post, the Reconcile window is reconcile.*, and the report, scheduled transaction, budget and business windows are their own methods. Tabs, toolbars and summary bars have no counterpart.
related: concept/chart-of-accounts, concept/transactions-and-splits, concept/reconciliation, gnucash/transaction-menu, gnucash/account-menu, gnucash/reports-menu, method/account.tree, method/transaction.list, method/reconcile.start
from: manual/ch_GUIMenus.docbook#gui-windows
---
red-ale has no windows. Each window of GnuCash is a view of the book, and a view is a method that returns the same
information. The menus of each window are on their own pages; this page says where each window went.

| GnuCash window | red-ale |
|---|---|
| Account tree (the main window, its Accounts page) | [`account.tree`](method/account.tree) gives the tree with balances; [`account.list`](method/account.list) a flat list; [`account.get`](method/account.get) one Account; [`account.balance`](method/account.balance) a balance, as of a date if you like. The menus are on [Accounts](gnucash/account-menu). `account.balance` includes the cleared and reconciled balances. Column choices are not modelled |
| Account register (General Journal, Receivable, Payable and the others) | [`transaction.list`](method/transaction.list) for an Account and a date range, [`transaction.find`](method/transaction.find) for a search, [`transaction.post`](method/transaction.post) to enter. Entry and columns are on [Transaction](gnucash/transaction-menu) |
| Reconcile window | [`reconcile.start`](method/reconcile.start), [`reconcile.status`](method/reconcile.status), [`reconcile.setState`](method/reconcile.setState), [`reconcile.finish`](method/reconcile.finish), [`reconcile.postpone`](method/reconcile.postpone). The Reconcile Information dialog is [`reconcile.info`](method/reconcile.info) and [`reconcile.setInfo`](method/reconcile.setInfo) ([Reconciliation](concept/reconciliation)) |
| Report window | [`report.run`](method/report.run), [`report.export`](method/report.export) ([Reports](gnucash/reports-menu)) |
| Scheduled Transaction Editor, Since Last Run | `scheduledTransaction.*` ([Tools](gnucash/tools-menu)) |
| Budget window | `budget.*` ([Tools](gnucash/tools-menu)) |
| Price Database, Security Editor | `price.*` and `commodity.*` ([Tools](gnucash/tools-menu)) |
| Lots window | `lot.*` ([Tools](gnucash/tools-menu)) |
| Customer, Vendor, Employee, Job, Invoice and Bill windows | `customer.*`, `vendor.*`, `employee.*`, `job.*` and `invoice.*` ([Business](gnucash/business-menu)) |
| Find Transaction window | [`transaction.find`](method/transaction.find) |
| Preferences, Book Options | [Preferences and book options](gnucash/preferences-and-book-options) |

## What has no counterpart

- **Tabs, panes, toolbars, status bar, summary bar** and the *Window* menu.
- **The saved window state.** GnuCash writes which pages were open into a `.gcm` file so that it can restore them.
  red-ale does not.
- **Account tree filters and column choices.** Ask for what you want: `excludeHidden`, or filter the result yourself.
- **Register styles** (Basic Ledger, Auto-Split, Transaction Journal), autocompletion, the double-line mode, sort and
  filter by. A list returns every Split in date order for you to filter.
- **The summary bar** of Profit and Total Assets: run the [Balance Sheet](report/balance-sheet) or
  [Profit & Loss](report/profit-loss) for the same figures.
- **Help.** The Manual you are reading is available through `help.topics`, `help.search` and `help.get`.
