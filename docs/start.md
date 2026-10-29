<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: start
title: red-ale Manual
summary: red-ale keeps GnuCash books and is driven entirely through JSON-RPC, MCP and its CLI. Open a book with book.open, change it with the other methods, save it with book.save, and send amounts as exact strings, never JSON numbers. Look things up with help.search and help.get.
related: method/book.open, method/account.create, method/transaction.post, method/account.balance
---
red-ale is a reimplementation of GnuCash that runs without a window. It reads and writes GnuCash's own files, XML
or SQLite, so a book red-ale has edited opens in GnuCash with nothing lost, and the other way round. Everything it
does is a JSON-RPC method, reached through three doors that run the same code:

- **JSON-RPC 2.0** over HTTP, from `red-ale serve`;
- **MCP**, from `red-ale mcp`: every method is a tool of the same name, for AI agents;
- **the CLI**: `red-ale exec --book <file> <method> '<params>'` runs one method on a book and saves it.

## Opening a book

One book is open at a time. [`book.open`](method/book.open) opens an existing book by its path and takes its lock,
so GnuCash and red-ale never write it at once; [`book.create`](method/book.create) makes a new, empty one. Some books
open read-only, and `readOnlyReasons` says why: a book that uses a GnuCash feature red-ale does not implement is
never written.

## Changing it

Each method that changes the book applies completely or not at all, and leaves every transaction balanced: a
transaction whose split values don't sum to zero is refused with [`unbalanced`](error/unbalanced). Changes are kept
in memory and in GnuCash's `.log` journal until the book is saved.

## Saving

[`book.save`](method/book.save) writes the book, after backing up the previous file as GnuCash does.
[`book.close`](method/book.close) releases the lock; it refuses to drop unsaved changes unless you say `save` or
`discard`. `red-ale exec` saves for you when its methods succeed.

## Amounts on the wire

Amounts are exact. Send one as a decimal string (`"12.34"`), a rational string (`"1234/100"`) or an object
(`{"num": "1234", "denom": "100"}`); red-ale answers with the object. A JSON number is refused, so no amount ever
passes through floating point.

A split has two sizes, as in GnuCash: its *value*, in the transaction's currency, and its *amount* (GnuCash's
*quantity*), in its account's commodity. For a split in an account of the transaction's currency they are equal, and
red-ale fills in the amount from the value.

## Where to look next

- `help.topics` lists every Page of this Manual, and `help.search` finds Pages for a question in your own words.
- Each method has a Reference page, `method/<name>`, with what it is for and Examples: start with
  [`account.create`](method/account.create), [`transaction.post`](method/transaction.post) and
  [`account.balance`](method/account.balance).
- `api.schema` gives every method's params and result as JSON Schema.
