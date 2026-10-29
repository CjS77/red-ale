<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: concept/chart-of-accounts
title: The chart of accounts
summary: The Account tree under the root Account, which GnuCash calls the chart of accounts - names, full names, placeholders, hidden Accounts, codes and how to lay one out.
related: concept/account-types, concept/transactions-and-splits, method/account.tree, method/account.list, method/account.get, method/account.create, guide/set-up-accounts
from: guide/ch_basics.docbook#basics-accounts2
---
GnuCash calls the whole set of Accounts a book holds its **chart of accounts**; red-ale's term is the **Account
tree**, the hierarchy under the root Account. Think of a table of contents for your finances: the main branches are
whole groups (`Assets`, `Expenses`), the leaves are the individual bank accounts and expense categories. A summary
report usually shows only the branches.

An Account keeps track of what you own, owe, spend or receive. A book holds any number of them, and each can hold
sub-accounts to any depth. Accounts stand for real accounts that send you statements (checking, a credit card, a loan)
and for categories of money (groceries, salary).

## Names

An Account has a name, unique among its siblings and never containing `:`. Its **full name** joins the names of its
ancestors with `:`, leaving out the root: `Assets:Bank:Checking`. Methods take an Account as its GUID or its full name.
red-ale refuses a new name containing `:`; if a book already holds one, [`account.list`](method/account.list) lists it
under `nameViolations`.

## Looking at the tree

- [`account.tree`](method/account.tree) returns the tree, or the part under one Account, as nested nodes. With `sorted`
  the siblings are in the order GnuCash displays them: by code, then type, then name.
- [`account.list`](method/account.list) returns the same Accounts flat, in the order the book stores them.
- [`account.get`](method/account.get) returns one.

## Placeholders, hidden Accounts and codes

- A **placeholder** groups others and accepts no Splits. Make the branches of the tree placeholders so that nothing is
  posted to `Expenses` itself.
- A **hidden** Account is kept but left out of everyday listings: use it for an Account you no longer use but whose
  history you keep. `excludeHidden` leaves hidden Accounts out of the listing methods.
- An **Account code** is free text used to sort and number Accounts. A common scheme gives leaves codes ending in
  non-zero digits and their parents codes with more zeros (`1010` Checking under `1000` Assets).
  [`account.renumber`](method/account.renumber) numbers an Account's children in sequence.

An Account also carries a description, notes, a colour, a tax-related flag, balance limits and the Accounts it
remembers for roles such as dividends. [`account.update`](method/account.update) sets them, and
[`account.cascade`](method/account.cascade) applies colour, placeholder or hidden to an Account and everything below it.

## Laying one out

Start from the five kinds in [Account types](concept/account-types): top-level branches for `Assets`, `Liabilities`,
`Income`, `Expenses` and `Equity`, with your real accounts as leaves under the first two and your categories under the
rest. Keep it as detailed as you will use: Transactions can move to another Account later. red-ale has no
starting-hierarchy assistant and no Account templates; build the tree with [`account.create`](method/account.create),
import one (see [Importing and matching](concept/importing-and-matching)), or copy a book's tree to a new book with
[`export.accounts`](method/export.accounts).
