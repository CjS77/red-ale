<h1>
  <img src="assets/logo.webp" alt="" height="96"><img src="assets/wordmark.svg" alt="red ale" height="96">
</h1>

red-ale reads and edits [GnuCash](https://www.gnucash.org) books without the GnuCash program. It has no windows: you
drive it from the command line or from your own programs over [JSON-RPC](https://www.jsonrpc.org/specification).
Anything red-ale saves, GnuCash 5.17 opens with no warnings and no lost data.

Use it to script your bookkeeping, to build tools on top of your GnuCash books, or to let other programs (including AI
agents, from v0.2) record transactions for you.

The name: **red** for Rust, **ale** for the accounting equation *Assets = Liabilities + Equity*.

## What it can do

Version 0.1 covers the GnuCash features from version 2.0 and earlier:

- accounts, transactions, voids, reversals, transfers and reconciling;
- commodities, currencies, prices, lots, capital gains and stock splits;
- customers, vendors, employees, jobs, invoices, bills, expense vouchers, orders, tax tables, billing terms and
  payments;
- the checks and repairs from GnuCash's Check & Repair;
- GnuCash's backups and `.log` journal, and its lock, so two programs never write the same book.

Not in 0.1:

- SQLite and other database books. red-ale refuses them and leaves them untouched.
- Editing scheduled transactions and budgets, which are kept unchanged.
- Reports, import and export.
- Online price quotes.

Out of scope never means lost: anything red-ale doesn't handle is written back exactly as it was read.

## Install

You need [Rust](https://rustup.rs) (rustup picks the right toolchain version for you), git and a C compiler such as
gcc or clang.

```sh
git clone https://github.com/CjS77/red-ale.git
cd red-ale
cargo install --path crates/red-ale
```

This puts the `red-ale` command in `~/.cargo/bin`. Check it works:

```sh
red-ale --version
```

## Walkthrough

This walkthrough builds a small household book from scratch. The examples pipe output through
[jq](https://jqlang.org) to keep it short; jq is optional.

### 1. Create a book

```sh
mkdir red-ale-demo && cd red-ale-demo
red-ale exec --create --book home.gnucash book.info
```

`exec` opens a book, runs one method on it, saves if anything changed, and closes it. Here it creates `home.gnucash`
and prints a description of it: an empty book with only a root account.

### 2. Add accounts

Add one account by giving the method name followed by its params:

```sh
red-ale exec --book home.gnucash account.create name=Assets type=ASSET commodity=USD placeholder:=true
```

Params are written `key=value` for text, and `key:=value` for anything that isn't text, such as `true`, a number or
a list.

For more than one change, put the methods in a script, one per line, as the method name followed by its params in
JSON. Save this as `setup.txt`:

```text
# Accounts for a simple household book
account.create {"name": "Checking", "type": "BANK", "parent": "Assets"}
account.create {"name": "Income", "type": "INCOME", "commodity": "USD", "placeholder": true}
account.create {"name": "Salary", "type": "INCOME", "parent": "Income"}
account.create {"name": "Expenses", "type": "EXPENSE", "commodity": "USD", "placeholder": true}
account.create {"name": "Groceries", "type": "EXPENSE", "parent": "Expenses"}
```

Then run it:

```sh
red-ale exec --book home.gnucash --script setup.txt
```

A script is all or nothing: if one line fails, the script stops and nothing is saved.

```sh
red-ale exec --book home.gnucash account.list | jq -r '.accounts[].fullName'
```

```text
Assets
Assets:Checking
Income
Income:Salary
Expenses
Expenses:Groceries
```

Accounts are named by their full name, with `:` between levels, or by their GUID.

### 3. Record transactions

A transaction has two or more splits, one per account it touches. The values must add up to zero:

```sh
red-ale exec --book home.gnucash transaction.post description="Pay day" datePosted=2024-07-01 \
    'splits:=[{"account": "Assets:Checking", "value": "2500.00"}, {"account": "Income:Salary", "value": "-2500.00"}]'

red-ale exec --book home.gnucash transaction.post description="Corner shop" datePosted=2024-07-03 \
    'splits:=[{"account": "Expenses:Groceries", "value": "42.17"}, {"account": "Assets:Checking", "value": "-42.17"}]'
```

A positive value is a debit and a negative one a credit, as in GnuCash.

red-ale refuses a transaction that doesn't balance, and says by how much:

```json
{
  "error": {
    "code": 2002,
    "message": "transaction is unbalanced by 1.00 USD",
    "data": { "imbalance": "1.00", "currency": "USD" }
  }
}
```

Amounts are always written as strings (`"42.17"`), never as bare numbers, so they are exact to the cent. A bare number
is refused.

### 4. Look at the results

```sh
red-ale exec --book home.gnucash account.balance account=Assets:Checking | jq -c .balances.balance
```

```json
{"num":"245783","denom":"100"}
```

red-ale gives amounts as fractions, as GnuCash stores them: 245783/100 is 2,457.83.

```sh
red-ale exec --book home.gnucash transaction.list account=Assets:Checking \
    | jq -r '.transactions[] | "\(.datePosted)  \(.description)"'
```

```text
2024-07-01  Pay day
2024-07-03  Corner shop
```

Now open `home.gnucash` in GnuCash: the same accounts, transactions and balances are there.

### 5. Run it as a server

`exec` opens and closes the book each time. To keep a book open and send it many requests, run the server:

```sh
red-ale serve --book home.gnucash
```

In another terminal, use `call` to send it methods:

```sh
red-ale call book.info
red-ale call price.add commodity=EUR currency=USD value=1.08 date=2024-07-01
red-ale call book.save
```

Changes made through the server stay in memory until you call `book.save`. Any program that speaks JSON-RPC over HTTP
can do the same. With curl:

```sh
curl -s http://127.0.0.1:7878 -H 'Content-Type: application/json' \
    -d '{"jsonrpc": "2.0", "id": 1, "method": "account.balance", "params": {"account": "Assets:Checking"}}'
```

Stop the server with Ctrl-C. It closes the book and releases the lock, but **does not save**. Unsaved changes are kept
in the book's `.log` journal, and `book.replayJournal` can bring them back.

The server listens on 127.0.0.1 only and has no password. Don't expose it to a network.

## Using your own GnuCash books

- **Try it on a copy first.** red-ale backs up a book before every save, but a copy costs nothing.
- **Close the book in GnuCash first.** red-ale won't edit a book that GnuCash, or another red-ale, has open.
- **Some books open read-only.** If a book uses something red-ale can't yet edit safely (credit notes, trading
  accounts, the auto read-only threshold, or anything it doesn't recognise), it opens read-only and `book.info` says
  why under `readOnlyReasons`. To open any book read-only on purpose, add `--read-only`.
- **Backups and journals.** Each save keeps the previous file as `<book>.<timestamp>.gnucash` and starts a new
  `<book>.<timestamp>.log`, as GnuCash does. Files older than 30 days are deleted at the next save; change that with
  `--retention-days` or `--keep-backups-forever`.

## Learn more

- `red-ale help` prints red-ale's Manual: `red-ale help transaction.post` shows what a method is for, with examples
  as `red-ale exec` command lines, and `red-ale help --search <words>` finds the Page for a question.
- `red-ale methods` lists every method, with what it does and whether it changes the book.
- `red-ale schema` prints every method's params and results as JSON Schema.
- `red-ale mcp` serves the same methods to AI agents as MCP tools; [`design/mcp.md`](design/mcp.md) shows how to set
  up a client.
- [`design/api.md`](design/api.md) explains the API's conventions and error codes, and [`design/api/`](design/api)
  has a page for each area, with notes on where red-ale and GnuCash differ.
- [`design/glossary.md`](design/glossary.md) defines the accounting terms, such as *value* and *amount*.

## Code layout

| Crate | What it does |
|-------|--------------|
| [`red-ale-core`](crates/red-ale-core) | The bookkeeping model (books, accounts, transactions, prices, lots, business objects) and every rule that keeps it consistent. |
| [`red-ale-xml`](crates/red-ale-xml) | Reads and writes GnuCash XML files, translating between GnuCash's format and the core model. |
| [`red-ale-service`](crates/red-ale-service) | Opens a book for editing: the lock, read-only checks, all-or-nothing changes, the journal, saving and backups. |
| [`red-ale-rpc`](crates/red-ale-rpc) | The JSON-RPC API: every method, its params and error codes, and the HTTP server. |
| [`red-ale-mcp`](crates/red-ale-mcp) | The MCP server: every method as a tool that AI agents can call. |
| [`red-ale`](crates/red-ale) | The `red-ale` command: runs the server, or calls methods from the command line. |
| [`red-ale-testkit`](crates/red-ale-testkit) | Test fixtures, temporary copies of books, and the harness that checks red-ale's results against GnuCash itself. |

## Licence

GPL-3.0-or-later. See [`LICENSE`](LICENSE).
