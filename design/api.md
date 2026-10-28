# API: the JSON-RPC published language

red-ale is controlled entirely through JSON-RPC 2.0. The same methods are reached three ways, all through one
transport-free dispatcher (`red-ale-rpc::Dispatcher`), so every door runs the same code:

- **HTTP:** `red-ale serve` (default `127.0.0.1:7878`; keep it on localhost, as there is no authentication yet);
- **CLI:** `red-ale exec --book <file> <method> [params]` runs methods in process; `red-ale call <method> [params]`
  calls a running server;
- **MCP:** `red-ale mcp` serves every method as an MCP tool over stdio, with the method's name, its params schema and
  hints from its Effect. Errors come back as tool results with `isError: true` carrying the error object below. See
  [`mcp.md`](mcp.md).

The authoritative list of methods, with every params and result schema, is the schema document: `red-ale schema`,
or the `api.schema` method. This page gives the conventions; `api/` holds one page per area.

## Versioning

`api.schema` reports `redAleApi` (`0.1.0` in v0.1, `0.2.0` in v0.2, `0.3.0` in v0.3). Within a major version the API changes only additively: new
methods, new optional params, new result fields. A method, param, result field or error code, once released, keeps its
name and meaning.

From 0.2 on, a test holds each later 0.x version to this until v1.0 freezes the API
(`crates/red-ale-rpc/tests/api_additive.rs`): it compares `api.schema` with each released document committed beside
it (`tests/schemas/red-ale-api-0.2.0.json`, `red-ale-api-0.3.0.json`). Every earlier method must keep its Effect and
Effect target; its params may widen their types and add optional fields but not lose, retype or newly require one;
its result may add fields but not lose, widen or make optional one; every earlier error code keeps its name. Schemas are compared place by place after
resolving `$ref`s, so renamed `$defs` and changed descriptions don't count.

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

From `redAleApi` 0.2.0 each method also declares its **effect target**, the `target` field of its entry in the
schema document: what a `write` or `destructive` method changes, `book` or `configuration` (`null` for `read`).
Every method that changes something targets `book`, except `config.set` ([`api/configuration.md`](api/configuration.md)),
`period.set`, `docLink.setPathHead` and the saved-configuration and style-sheet methods of
[`api/reports.md`](api/reports.md), which change only red-ale's Configuration, work with no book open and never make
a book dirty. The MCP hints come from the effect
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
| Recurrence    | `{"mult": 1, "periodType": "month", "start": "2025-01-31", "weekendAdjust": "none"}`: `periodType` one of `once`, `day`, `week`, `month`, `endOfMonth`, `nthWeekday`, `lastWeekday`, `year`; `weekendAdjust` `none` (the default), `back` or `forward`. Read as stored; see [`api/scheduled-transactions.md`](api/scheduled-transactions.md) and [`api/budgets.md`](api/budgets.md) for what each area writes. From 0.4.0. |

Value and amount keep GnuCash's meanings (see the glossary): a split's *value* is in the transaction currency, its
*amount* in the account's commodity.

### Dates, times and the local time zone

red-ale handles time zones as GnuCash 5.17 does ([`notes/time-zones.md`](notes/time-zones.md)), so a red-ale server
and a GnuCash in the same zone agree on every date:

- **The local time zone** is the server process's, read once at start-up in GnuCash's order: `TZ` as a zone file
  (IANA name or path, `TZDIR` respected), else `TZ` as a POSIX string **with GnuCash's (Boost's) reversed sign**
  (`XYZ11` is UTC+11), else `/etc/localtime`, else UTC. A `TZ` that GnuCash would reject is rejected the same way.
- **A date given as a Date** (a posted date, a price's day, a due date, a find dialog's "before") is stored at the
  day-neutral time: 10:59:00 UTC, moved to 11:59 UTC in zones west of UTC−10 and to 09:59 UTC in zones east of
  UTC+13, so the stored instant is on that day in the local zone.
- **A Date derived from an instant** is the local date, except a posted date with no `date-posted` slot, which is the
  instant's UTC date, as GnuCash reads it.
- **Day boundaries** are local: a day starts at 00:00:00 and ends at 23:59:59 local time. A time in a daylight-saving
  gap is moved an hour later; an ambiguous one takes the later instant.
- **"Today"** is the local date of the session's clock (the server's without a book).
- Timestamps on the wire stay UTC (RFC 3339 with `Z`); only the meaning of Dates depends on the zone.

Where v0.1 used UTC days instead, v0.2 uses local ones: same-day prices, "today" and the find dialog's day
boundaries, the accounting period, balances "as of" a day, lot dates and price dates. Under `TZ=UTC` nothing
changes.

Deviations, each pinned by `oracle_time_zones.rs` or the zone tests:

- red-ale follows a zone file's transitions exactly. GnuCash approximates the file with one rule per year
  ([`notes/time-zones.md`](notes/time-zones.md) §1), which puts local midnight an hour late on the autumn change in
  zones whose rules are in UT (most of Europe) and drops daylight saving from 2037. Stored dates are unaffected:
  day-neutral times and local noons agree. Kept as a deviation (decided for v0.3): the zone file is the true local
  time, and GnuCash's fit is a side effect of turning each year into one Boost rule
  (`TimeZoneProvider::parse_file`, `5.17:libgnucash/engine/gnc-timezone.cpp:635-720`), not a rule a user relies on.
  Matching it would mean porting that fit to put day boundaries an hour wrong once a year, and it changes only which
  day an instant in that hour belongs to (a balance "as of" a day, a same-day price lookup), never what is written.
- GnuCash's `GncDate` "today" reads the C library's zone, which reads a POSIX `TZ` string with the opposite sign to
  GnuCash's own; red-ale has one zone.
- A `TZ` naming a file that is not TZif aborts GnuCash; red-ale reads it as a POSIX string, then falls back as
  GnuCash does.

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
| 1004   | `sql-book`          | The book is a database server's (`mysql://`, `postgres://`, `postgresql://`; v0.7), or `book.create` was asked for an SQLite book, which red-ale cannot write yet; refused before anything is touched. |
| 1005   | `unsupported-uri`   | A URI scheme other than a path, `file://` or `xml://`.                    |
| 1006   | `read-only`         | The book is open read-only (`data.reason`).                               |
| 1007   | `file-exists`       | `book.create` on an existing file.                                        |
| 1008   | `io-error`          | A file operation failed.                                                  |
| 1009   | `file-format`       | Not a readable XML book (or journal), including GnuCash version 1 files.  |
| 1010   | `save-verification` | The file written did not read back as the same book; it was removed and the book on disk is unchanged. |
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
| 4000   | `import-file`       | An import file can't be read as its format at all (`data.file`, `data.line`, `data.reason`); see [`api/import.md`](api/import.md). From 0.3.0. |
| 5000   | `network-disabled`  | A method needs the network and the Configuration has it off; nothing was sent ([`api/quotes.md`](api/quotes.md)). From 0.3.0. |
| 6000   | `formula`           | A scheduled transaction's formula does not parse, names a function red-ale does not compute, or can't be evaluated (`data.reason`, `data.formula`, `data.position`, where it is); see [`api/scheduled-transactions.md`](api/scheduled-transactions.md). From 0.4.0. |

## Book methods

| Method               | Effect | Does                                                                                   |
|----------------------|--------|----------------------------------------------------------------------------------------|
| `book.open`          | read   | Open a book (`path`, `readOnly`): XML, or SQLite (a file or `sqlite3://`), read-only for now. Takes an XML book's `.LCK` lock unless read-only. |
| `book.create`        | write  | Create an empty book (root account only) and open it; gzip-compressed unless `compressed: false`. |
| `book.info`          | read   | Path, GUIDs, read-only state and reasons, dirty, compression, journal, Book features, `options` (`useTradingAccounts`), counts. |
| `book.save`          | write  | Back up the previous file, write the book (atomically), rotate the journal, apply retention. `compressed` changes the file's compression. |
| `book.close`         | write  | Release the lock. Refuses unsaved changes unless `save` or `discard`.                  |
| `book.replayJournal` | write  | Replay a GnuCash `.log` journal: transactions created, updated or deleted by GUID.     |
| `book.closeBooks`    | write  | Close the books: zero income and expense into equity at a date ([`api/close-book.md`](api/close-book.md)). Not `book.close`. |
| `book.options`       | read   | The num field source, auto read-only threshold, trading accounts and default budget Book options ([`api/book-options.md`](api/book-options.md), [`api/trading-accounts.md`](api/trading-accounts.md), [`api/budgets.md`](api/budgets.md)). |
| `book.setOptions`    | write  | Change them as GnuCash's book options dialog does, setting `Number Field Source` when the num field source is turned on. |

A book opens **read-only**, and says why in `readOnlyReasons`, when editing it could break it: it declares a Book
feature red-ale does not implement (`unsupported-features`), holds content red-ale can keep but not edit safely
(`unrecognised-content`), its SQL schema is not GnuCash 5.17's (`schema-version`), its format can't be written
yet (`writing-not-supported`: SQLite books, until saving them lands), or the caller asked (`requested`). A read-only
book is never locked and never written.
Since v0.2 the num field source (and its Book feature `Number Field Source`) and an auto read-only threshold no longer
make a book read-only; the threshold makes old transactions read-only instead
([`api/book-options.md`](api/book-options.md)), and the reason `auto-read-only` is no longer given. Nor do trading
accounts: commits keep their trading splits ([`api/trading-accounts.md`](api/trading-accounts.md)), and the reason
`trading-accounts` is no longer given.

Replay differs from GnuCash in two ways, both to protect the book: a record with an unreadable line is skipped
whole, and an unbalanced replayed transaction is refused (or skipped with `skipInvalid`) instead of getting an
`Imbalance-<CUR>` split. Replayed changes are journaled like any other mutation.

## Areas

Each page lists its methods, params and results, the GnuCash behaviour they match (cited to the 5.17 source), and
deliberate deviations.

| Page                                                       | Methods                                                     |
|------------------------------------------------------------|-------------------------------------------------------------|
| [`api/accounts.md`](api/accounts.md)                       | `account.*`, `period.*`                                     |
| [`api/transactions.md`](api/transactions.md)               | `transaction.*`, `docLink.*`                                |
| [`api/reconcile.md`](api/reconcile.md)                     | reconcile states, reconciling against a statement, auto-clear |
| [`api/scrub.md`](api/scrub.md)                             | imbalance and orphan scrubs, Check & Repair                 |
| [`api/trading-accounts.md`](api/trading-accounts.md)       | trading accounts: the option, trading splits at every commit |
| [`api/commodities.md`](api/commodities.md)                 | `commodity.*`, `currency.*`                                 |
| [`api/prices.md`](api/prices.md)                           | `price.*`                                                   |
| [`api/lots.md`](api/lots.md)                               | `lot.*`, capital gains, stock splits                        |
| [`api/business-contacts.md`](api/business-contacts.md)     | customers, vendors, employees, jobs, counters, tax tables, billing terms |
| [`api/business-documents.md`](api/business-documents.md)   | invoices, bills, expense vouchers, entries, orders          |
| [`api/business-posting.md`](api/business-posting.md)       | totals, post, unpost, payments, due reminders               |
| [`api/close-book.md`](api/close-book.md)                   | `book.closeBooks`: closing the books                        |
| [`api/book-options.md`](api/book-options.md)               | `book.options`, `book.setOptions`: num field source, auto read-only threshold |
| [`api/reports.md`](api/reports.md)                         | `report.*`: list, options, run, export, saved configurations, style sheets |
| [`api/import.md`](api/import.md)                           | `import.*`, `importMap.*`: CSV, QIF and business import, the matcher, Import maps (v0.3) |
| [`api/export.md`](api/export.md)                           | `export.*`: CSV export of the account tree and transactions, Export accounts (v0.3) |
| [`api/quotes.md`](api/quotes.md)                           | `price.fetchQuotes`, `price.fetchStatus`: online quotes and Quote services (v0.3) |
| [`api/configuration.md`](api/configuration.md)             | `config.*`: the Configuration's schema, every setting, reading and changing them by path |
| [`api/budgets.md`](api/budgets.md)                         | `budget.*`: budgets, amounts and notes, estimate, natural signs, the default budget (v0.4) |
| [`api/scheduled-transactions.md`](api/scheduled-transactions.md) | `scheduledTransaction.*`: schedules, formulas, Since Last Run, from a transaction, the loan assistant (v0.4) |

## CLI

```
red-ale exec --book books/home.gnucash account.create name=Checking type=BANK parent=Assets commodity=USD
red-ale exec --book books/home.gnucash transaction.post description=Rent datePosted=2024-07-01 \
    'splits:=[{"account": "Assets:Checking", "value": "-750.00"}, {"account": "Expenses:Rent", "value": "750.00"}]'
red-ale exec --book books/home.gnucash --script changes.txt     # one "method {json}" per line
red-ale exec --config red-ale.json config.set 'changes:={"import.settings.matchThreshold": 2}'   # no book needed
red-ale mcp --book books/home.gnucash                            # MCP over stdio, for an AI agent
red-ale call --url http://127.0.0.1:7878 book.info
```

Params are one JSON object, or `key=value` pairs (the value is a string, which suits amounts: `amount=12.50`) and
`key:=json` pairs (`placeholder:=true`). `exec` saves when the methods succeed and changed the book (`--no-save` to
skip), and saves nothing if any method fails. Without `--book`, `exec` runs the methods with no book open, as the
Configuration methods need none. Results go to standard output as JSON; errors go to standard error as
`{"error": …}` with exit status 2. Usage errors exit with status 1. `exec` (with one method, not `--script`) and `call`
take `--output <file>`, which writes the result's `html` to the file on the caller's side and prints
`{title, path, bytes}` instead ([`api/reports.md`](api/reports.md), "CLI").
