# API: the JSON-RPC published language

red-ale is controlled entirely through JSON-RPC 2.0. The same methods are reached three ways, all through one
transport-free dispatcher (`red-ale-rpc::Dispatcher`), so every door runs the same code:

- **HTTP:** `red-ale serve` (default `127.0.0.1:7878`; keep it on localhost, as there is no authentication yet);
- **CLI:** `red-ale exec --book <file> <method> [params]` runs methods in process; `red-ale call <method> [params]`
  calls a running server;
- **MCP** (v0.2): each tool maps to one method and shares its params schema.

The authoritative list of methods, with every params and result schema, is the schema document: `red-ale schema`,
or the `api.schema` method. This page gives the conventions; `api/` holds one page per area.

## Versioning

`api.schema` reports `redAleApi` (`0.1.0` in v0.1, `0.2.0` in v0.2). Within a major version the API changes only additively: new
methods, new optional params, new result fields. A method, param, result field or error code, once released, keeps its
name and meaning.

## Requests

- **Params are by name**: a JSON object. Absent params read as `{}`. Positional (array) params are refused with
  `invalid-params`. Unknown param names are refused, so a typo never passes silently.
- Names are camelCase. Methods are namespaced by bounded context: `book.open`, `account.create`,
  `transaction.post`, `price.add`, `invoice.post`.
- Batches and notifications work as JSON-RPC 2.0 defines them.
- **One book per server.** `book.open` fails with `book-already-open` while one is open.

## Effects

Every method declares its effect in the schema; MCP's `readOnlyHint` and `destructiveHint` come from it.

| Effect        | Meaning                                                   |
|---------------|-----------------------------------------------------------|
| `read`        | Reads only.                                               |
| `write`       | Adds or changes data.                                     |
| `destructive` | Deletes or overwrites data (delete, remove old prices, …) |

From `redAleApi` 0.2.0 each method also declares its **effect target**: what a `write` or `destructive` method
changes, `book` or `configuration` (`null` for `read`). Every method that changes something targets `book`, except
`period.set` and the saved-configuration and style-sheet methods of [`api/reports.md`](api/reports.md), which change
only red-ale's Configuration, work with no book open and never make a book dirty. The MCP hints come from the effect
alone (see [`reports.md`](reports.md) §10).

Every mutation is **atomic**: it runs on a copy of the book, the copy's changed transactions are checked (balanced,
accounts exist), and only then does the copy replace the book. A failed mutation leaves nothing behind. Mutations
change the book in memory and the journal; `book.save` writes the file.

## Wire types

| Type          | Form                                                                                                  |
|---------------|-------------------------------------------------------------------------------------------------------|
| Amount        | Written `{"num": "1234", "denom": "100"}`. Read from that, or a decimal string `"12.34"` (the digits set the denominator: `"12.30"` is 1230/100), or a rational string `"1234/100"`. **JSON numbers are refused**, so no amount passes through a float. |
| GUID          | 32 hexadecimal digits.                                                                                |
| Date          | `YYYY-MM-DD`: a calendar date, as GnuCash's posted dates are.                                         |
| Timestamp     | RFC 3339, written in UTC with `Z`; read with any offset.                                              |
| CommodityId   | GnuCash's unique name `NAMESPACE::MNEMONIC` (`CURRENCY::USD`, `NASDAQ::AAPL`). A bare `USD` names a currency. |
| AccountRef    | A GUID, or a full name with `:` between levels (`Assets:Current Assets:Checking`).                    |

Value and amount keep GnuCash's meanings (see the glossary): a split's *value* is in the transaction currency, its
*amount* in the account's commodity.

## Errors

Errors are JSON-RPC error objects with a stable `code`, a human `message` and structured `data`. The JSON-RPC range
is used only for protocol errors.

| Code   | Name                | When                                                                      |
|--------|---------------------|---------------------------------------------------------------------------|
| -32700 | `parse-error`       | The request is not JSON.                                                  |
| -32600 | `invalid-request`   | Not a JSON-RPC 2.0 request.                                               |
| -32601 | `method-not-found`  | No such method.                                                           |
| -32602 | `invalid-params`    | Params do not match the method's schema (unknown name, wrong type, a JSON number as an amount). |
| -32603 | `internal-error`    | A bug in red-ale.                                                         |
| 1000   | `no-book`           | No book is open.                                                          |
| 1001   | `book-already-open` | A book is already open in this server.                                    |
| 1002   | `book-locked`       | Another program holds the book's lock (`data.path` is the lock file).     |
| 1003   | `cannot-lock`       | The lock file could not be created.                                       |
| 1004   | `sql-book`          | The book is an SQL book (SQLite file or database URI); v0.1 refuses it before touching it. |
| 1005   | `unsupported-uri`   | A URI scheme other than a path, `file://` or `xml://`.                    |
| 1006   | `read-only`         | The book is open read-only (`data.reason`).                               |
| 1007   | `file-exists`       | `book.create` on an existing file.                                        |
| 1008   | `io-error`          | A file operation failed.                                                  |
| 1009   | `file-format`       | Not a readable XML book (or journal), including GnuCash version 1 files.  |
| 1010   | `save-verification` | The bytes about to be written did not read back as the same book; nothing was written. |
| 1011   | `backup-failed`     | The backup could not be made; nothing was written.                        |
| 1012   | `unsaved-changes`   | `book.close` on a book with unsaved changes, without `save` or `discard`. |
| 2000   | `not-found`         | An entity named in the request does not exist (`data.kind`, `data.id`).   |
| 2001   | `invalid`           | The request is not valid for the book as it stands.                       |
| 2002   | `unbalanced`        | A transaction would not balance (`data.imbalance`, `data.currency`).      |
| 2003   | `conflict`          | The entity exists but may not be changed this way.                        |
| 2004   | `arithmetic`        | Exact arithmetic overflowed or was undefined.                             |
| 3000   | `report-script`     | A report script failed at run time (`data.source`, `data.line`, `data.message`, `data.log`). |
| 3001   | `report-limit`      | A report script passed a limit (`data.limit`: `operations`, `time`, `call-depth`, …). |
| 3002   | `report-option`     | A report option value is not valid (`data.section`, `data.name`, `data.reason`). |
| 3003   | `report-result`     | A report script returned an invalid report result (`data.path`).          |
| 3004   | `report-load`       | A user report script does not load (`data.source`, `data.line`).          |

## Book methods

| Method               | Effect | Does                                                                                   |
|----------------------|--------|----------------------------------------------------------------------------------------|
| `book.open`          | read   | Open an XML book (`path`, `readOnly`). Takes the `.LCK` lock unless read-only.         |
| `book.create`        | write  | Create an empty book (root account only) and open it; gzip-compressed unless `compressed: false`. |
| `book.info`          | read   | Path, GUIDs, read-only state and reasons, dirty, compression, journal, Book features, counts. |
| `book.save`          | write  | Back up the previous file, write the book (atomically), rotate the journal, apply retention. `compressed` changes the file's compression. |
| `book.close`         | write  | Release the lock. Refuses unsaved changes unless `save` or `discard`.                  |
| `book.replayJournal` | write  | Replay a GnuCash `.log` journal: transactions created, updated or deleted by GUID.     |

A book opens **read-only**, and says why in `readOnlyReasons`, when editing it could break it: it declares a Book
feature v0.1 does not implement (`unsupported-features`), uses trading accounts (`trading-accounts`), sets an auto
read-only threshold (`auto-read-only`), holds content red-ale can keep but not edit safely (`unrecognised-content`),
or the caller asked (`requested`). A read-only book is never locked and never written.

Replay differs from GnuCash in two ways, both to protect the book: a record with an unreadable line is skipped
whole, and an unbalanced replayed transaction is refused (or skipped with `skipInvalid`) instead of getting an
`Imbalance-<CUR>` split. Replayed changes are journaled like any other mutation.

## Areas

Each page lists its methods, params and results, the GnuCash behaviour they match (cited to the 5.17 source), and
deliberate deviations.

| Page                                                       | Methods                                                     |
|------------------------------------------------------------|-------------------------------------------------------------|
| [`api/accounts.md`](api/accounts.md)                       | `account.*`, `period.*`                                     |
| [`api/transactions.md`](api/transactions.md)               | `transaction.*`                                             |
| [`api/reconcile.md`](api/reconcile.md)                     | reconcile states, reconciling against a statement           |
| [`api/scrub.md`](api/scrub.md)                             | imbalance and orphan scrubs, Check & Repair                 |
| [`api/commodities.md`](api/commodities.md)                 | `commodity.*`, `currency.*`                                 |
| [`api/prices.md`](api/prices.md)                           | `price.*`                                                   |
| [`api/lots.md`](api/lots.md)                               | `lot.*`, capital gains, stock splits                        |
| [`api/business-contacts.md`](api/business-contacts.md)     | customers, vendors, employees, jobs, tax tables, billing terms |
| [`api/business-documents.md`](api/business-documents.md)   | invoices, bills, expense vouchers, entries, orders          |
| [`api/business-posting.md`](api/business-posting.md)       | totals, post, unpost, payments, due reminders               |
| [`api/reports.md`](api/reports.md)                         | `report.*`: list, options, run, export, saved configurations, style sheets |

## CLI

```
red-ale exec --book books/home.gnucash account.create name=Checking type=BANK parent=Assets commodity=USD
red-ale exec --book books/home.gnucash transaction.post description=Rent datePosted=2024-07-01 \
    'splits:=[{"account": "Assets:Checking", "value": "-750.00"}, {"account": "Expenses:Rent", "value": "750.00"}]'
red-ale exec --book books/home.gnucash --script changes.txt     # one "method {json}" per line
red-ale call --url http://127.0.0.1:7878 book.info
```

Params are one JSON object, or `key=value` pairs (the value is a string, which suits amounts: `amount=12.50`) and
`key:=json` pairs (`placeholder:=true`). `exec` saves when the methods succeed and changed the book (`--no-save` to
skip), and saves nothing if any method fails. Results go to standard output as JSON; errors go to standard error as
`{"error": …}` with exit status 2. Usage errors exit with status 1.
