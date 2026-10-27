# Milestone 4: red-ale v0.4

**Status: draft.** Proposed from [`roadmap.md`](roadmap.md) ("v0.4"), [`architecture.md`](architecture.md) and
[`features.md`](features.md). Nothing here is decided until the items under "Decide before code" are settled; items
marked *Proposed* are suggestions for that review.

Claims about GnuCash with a citation come from the design documents and notes already checked against 5.17. Claims
marked **(to check)** have not yet been checked against the source at the pin; each must be, and cited, before code
depends on it.

**Goal:** red-ale opens, edits and saves SQLite books as GnuCash 5.17 does, with the same lock, journal and Book
features; creates, edits and runs scheduled transactions, with formulas, variables and Since Last Run, as GnuCash
does; creates and edits budgets; and runs the budget reports, the Future Scheduled Transactions Summary and the
Balance Forecast. All of it on XML and SQLite books alike.

This file tracks progress towards v0.4. Feature names and versions come from [`features.md`](features.md); when a
feature here is done, set its Ported column there to `0.4`.

## Scope

The 19 open rows the roadmap places in v0.4:

| Area              | Rows                                                                                                                                     |
|-------------------|------------------------------------------------------------------------------------------------------------------------------------------|
| §9 Formats        | SQLite books, ISO dates in SQLite                                                                                                        |
| §4                | Scheduled transactions, Recurrence rules, Formulas and variables, Since Last Run, Schedule from transaction, Mortgage and loan assistant |
| §5                | Budgets, Estimate from history, Budget notes, Natural signs                                                                              |
| §8 Budget reports | Budget Report, Budget Balance Sheet, Budget Income Statement, Budget Flow, Budget Chart                                                  |
| §8 Analysis       | Future scheduled transactions, Balance Forecast                                                                                          |

Also in v0.4: the Mutation walk carried forward from v0.3, which Since Last Run needs (see "Also in v0.4"), and the
v0.3 follow-ups taken up here ("Carried forward from v0.3").

Why these together: scheduled transactions and budgets are Residue today. red-ale keeps them byte for byte but can't
create, edit or run them, so a user who relies on them still needs GnuCash. SQL books are refused outright. Each of
the three lifts a reason a user's book can't be fully used ([`roadmap.md`](roadmap.md), "How the order is chosen",
rule 1). Putting the model work in the same version lets `red-ale-sql` map the `schedxactions`, `recurrences`,
`budgets` and `budget_amounts` tables from the start (`5.17:libgnucash/backend/sql/gnc-schedxaction-sql.cpp:51`,
`gnc-recurrence-sql.cpp`, `gnc-budget-sql.cpp:49-51`) instead of keeping them unread and reworking that later.

**Out:**

- **MySQL/MariaDB and PostgreSQL books** (v0.7). `red-ale-sql` is written so a second SQL dialect fits beside
  SQLite, but only SQLite connects in v0.4. Database URIs (`mysql://`, `postgres://`) are still refused with
  `sql-book` before anything is touched.
- **Autosave: won't do (❌).** Every operation is atomic, so a timer adds nothing a client can't do with
  `book.save`; unsaved changes are in the journal ([`features.md`](features.md), §9 "Safety").
- **Attached documents.** [`milestone3.md`](milestone3.md) ("Out") moved them to v0.4. *Proposed:* they move out
  again, to a version the roadmap names. GnuCash has no attachments, only document links, which v0.2 already ports
  (`docLink.*`). Storing the files goes beyond parity and needs its own design doc first, and v0.4 is already the
  largest version on the roadmap. See "Decide before code", item 8.
- **Running Since Last Run on open.** GnuCash's GUI can run it when a book opens, by preference **(to check)**.
  red-ale is headless and never changes a book on open: a client asks for Since Last Run.
- **Converting between XML and SQLite** (GnuCash's Save As). `features.md` has no row for it; see "Rows missing from
  `features.md`".
- **Books with legacy frequency specs** (`sx:freqspec`). GnuCash 5.17 reads them and rewrites them as recurrences,
  and never writes them ([`notes/xml-format-core.md`](notes/xml-format-core.md) §8.5). red-ale opens such a book
  read-only, with the reason.

## Compatibility in v0.4

Out of scope still never means lost, and every v0.1, v0.2 and v0.3 rule holds. Additions:

- **An SQLite book is the user's record as much as an XML one.** Opening one and closing it without a save writes
  nothing to it. A save writes only the rows of entities that changed, and changes only the columns red-ale models.
- **Rows, columns and tables red-ale doesn't model are never touched.** The SQL Backend's Residue is mostly what it
  leaves alone: unknown tables, unknown columns of known tables, and rows of entity types red-ale doesn't model. A
  book holding any of these opens read-only, as unrecognised XML content does ([`open-questions.md`](open-questions.md),
  "Unrecognised content opens read-only").
- **Schema versions are respected, never upgraded.** GnuCash records each table's version in the `versions` table.
  GnuCash's GUI offers to upgrade an older schema and opens the book read-only if the user declines, and opens a newer
  one read-only (`5.17:gnucash/gnome-utils/gnc-file.c:1074-1090`). red-ale opens any book whose table versions differ
  from 5.17's read-only, says which tables differ, and never upgrades.
- **The SQL lock is GnuCash's.** SQL books are locked by a row in the `gnclock` table, not a `.LCK` file
  ([`../CLAUDE.md`](../CLAUDE.md), "Respect the lock"). red-ale never opens for writing a book with a `gnclock` row,
  writes its own row as GnuCash does **(to check: the columns and what GnuCash writes in them)**, and removes it on
  every exit path, error paths included. It never deletes a row it did not write.
- **Book features are set as GnuCash sets them.** GnuCash's SQLite backend sets
  `ISO-8601 formatted date strings in SQLite3 databases.` on save (`5.17:libgnucash/backend/dbi/gnc-backend-dbi.cpp:880`).
  red-ale writes ISO dates and sets the feature the same way. Creating the first budget sets
  `Use natural signs in budget amounts`, as GnuCash's new-budget command does
  (`5.17:gnucash/gnome/gnc-plugin-budget.c:181-183`). Both are already in `SUPPORTED_FEATURES`
  (`crates/red-ale-service/src/session.rs:24`), so nothing opens read-only that did not before.
- **The journal covers SQL books too.** GnuCash's DBI backend keeps a `.log` journal for SQL books
  (`5.17:libgnucash/backend/dbi/gnc-backend-dbi.cpp:456`); red-ale writes one beside an SQLite book as it does beside
  an XML one, and `book.replayJournal` works on both.
- **Template transactions are not transactions of the Book.** The template accounts and the template transactions
  of scheduled transactions live apart from the Account tree: no balance, report, find query, import match, scrub or
  export ever sees them.
- **Transactions made from a schedule look as GnuCash makes them,** with the same Slots (a reference back to the
  scheduled transaction, **(to check: `from-sched-xaction`)**), dates and split values.
- **Accounts used by schedules or budgets can't be deleted silently.** Today the Session refuses to delete an Account
  whose GUID appears in out-of-scope Residue ([`file-fidelity.md`](file-fidelity.md), "References to out-of-scope
  data"). GnuCash doesn't check ([`notes/engine-accounts-transactions.md`](notes/engine-accounts-transactions.md)
  §3.2). With scheduled transactions and budgets in the model, the check moves into the core and
  stays: deleting such an Account is refused, and says which schedules and budgets use it.
- **Budget order is kept as read.** GnuCash's own budget write order is not reproducible
  ([`notes/xml-format-core.md`](notes/xml-format-core.md) §7.6), and GnuCash ignores it on read. red-ale keeps the
  order it read and appends new budgets.
- **Since Last Run is one Mutation.** Every transaction it creates and every schedule it advances applies together,
  or not at all.

## SQLite books

| Status | Item                         | Notes                                                                                                             |
|:------:|------------------------------|-------------------------------------------------------------------------------------------------------------------|
|        | SQLite books                 | `red-ale-sql`: read and write every table red-ale models, as GnuCash 5.17's DBI backend does                      |
|        | ISO dates in SQLite          | Read both timestamp forms; write ISO and set the Book feature (`gnc-backend-dbi.cpp:880`)                         |
|        | A Backend seam (*Proposed*)  | One interface in `red-ale-service` for opening, reading, saving and locking, with XML and SQLite behind it         |
|        | Schema versions              | Read `versions`; any table at a version other than 5.17's opens read-only, with the tables named                  |
|        | The SQL lock                 | `gnclock`, taken at open and released on every exit path                                                          |
|        | Saving                       | One SQL transaction per save, checked before it commits (below)                                                   |
|        | Journal and backups          | The `.log` journal as for XML; a backup copy before the first save (*Proposed*, below)                            |

**The Backend seam.** `red-ale-service` today opens, saves and locks XML books directly. *Proposed:* a `Backend`
trait in `red-ale-service` with two implementations, so the Session, the journal, read-only triggers and every use
case are the same for both formats. The Residue is the Backend's own type; only the Backend reads it, as now. The
XML Backend moves behind the seam first with no change to any byte it writes.

**Reading.** Each table is translated into the core model by the Backend, an anti-corruption layer as `red-ale-xml`
is: column names and SQL quirks never reach the core. Amounts are two 64-bit integer columns per Numeric
**(to check: column names per table)** and are read as integers only; a value stored with another SQLite storage
class (a `REAL` written by another program, say) is an error, never converted. Entities are read in a fixed order
(*Proposed:* `rowid`), so the Book's order, and so everything red-ale writes and reports, is deterministic.

**Timestamps.** Before the ISO Book feature, GnuCash wrote SQLite timestamps in another form **(to check: the form
and the code that reads both)**. red-ale reads both, writes only the ISO form, and sets the feature on save as
GnuCash does.

**Saving.** GnuCash's SQL backend writes each change to the database as it is committed **(to check:
`gnc-sql-backend.cpp`)**, so GnuCash's SQL books have no Save. red-ale's API keeps changes in memory, and in the
journal, until `book.save` ([`api.md`](api.md)). *Proposed:* keep that for SQLite too, so both formats behave the
same for clients and closing without a save discards changes for both. A save then:

1. diffs the Book against the Book as read (the same walk the Mutation needs, "Also in v0.4");
2. writes the changed rows in one SQL transaction;
3. reads them back inside that transaction and compares them with the Book entity by entity;
4. commits only if they match, and rolls back otherwise.

That keeps the guarantee XML saves give: nothing replaces what was there unless it reads back as the same Book. The
other choice, writing each Mutation through as GnuCash does, is "Decide before code", item 2.

**Backups.** GnuCash makes timestamped backups of XML books on save. For SQL books it makes none **(to check)**.
*Proposed:* before the first save of a session, red-ale copies the SQLite file to a timestamped backup beside it,
named and retained as XML backups are (SQLite's own backup API, so the copy is consistent). This goes beyond GnuCash
but changes nothing GnuCash reads.

**Creating.** `book.create` makes XML books by default, as v0.1 decided (a missing path is never taken as SQLite;
[`notes/storage-and-safety.md`](notes/storage-and-safety.md) §15, item 9). It gains a `format` param (`xml` or
`sqlite`) to create an SQLite book with GnuCash 5.17's schema.

**Double slots.** A `double` Slot is kept as the text it was read as and never computed with
([`architecture.md`](architecture.md), "Money is exact"). In SQLite it is a `REAL` column **(to check: the `slots`
table)**, so there is no text to keep. "Decide before code", item 5.

## Scheduled transactions

| Status | Feature                     | Notes                                                                                                     |
|:------:|-----------------------------|-----------------------------------------------------------------------------------------------------------|
|        | Scheduled transactions      | In the core model, with their template accounts and template transactions; `scheduledTransaction.*`       |
|        | Recurrence rules            | Every period type, multiplier and weekend adjustment, with next occurrences as GnuCash computes them      |
|        | Formulas and variables      | Template split amounts as formulas over variables, evaluated exactly                                      |
|        | Since Last Run              | Two calls, as import: a proposal of due instances, then apply                                             |
|        | Schedule from transaction   | A scheduled transaction made from an existing Transaction                                                 |
|        | Mortgage and loan assistant | One operation that builds a loan's repayment schedule, as the stock split assistant became `stock.split` |

**The model.** A Scheduled transaction holds what the XML writer writes
([`notes/xml-format-core.md`](notes/xml-format-core.md) §8.2, `5.17:libgnucash/backend/xml/gnc-schedxaction-xml-v2.cpp:80-197`):
its GUID, name, enabled, auto-create and notify flags, days to create and remind in advance, instance count, start,
last occurrence, an end date or a number of occurrences with those remaining, its template account, its schedule
(a list of Recurrences), its deferred instances and its Slots. Its template transactions sit under the Template root,
in commodity `template:template`. Each template split carries a `sched-xaction` Frame naming the real account and the
credit and debit formulas (§8.1). The core keeps them apart from the Account tree (see "Compatibility").

**Recurrences** have a multiplier, a period type (`once`, `day`, `week`, `month`, `end of month`, `nth weekday`,
`last weekday`, `year`; `5.17:libgnucash/engine/Recurrence.cpp:44-52`), a start date and a weekend adjustment
(§8.4). Budgets use the same type. Next occurrences are translated from `Recurrence.cpp`: the end-of-month,
nth-weekday and weekend rules have edge cases that only the source settles, which is the case for porting rather
than redesigning.

**Formulas and variables.** v0.3 translated GnuCash's expression parser for business import
(`crates/red-ale-import/src/business/expr.rs`, from `5.17:libgnucash/app-utils/gnc-exp-parser.c:459-597`), without
variables, assignments or functions. *Proposed:* move it into `red-ale-core`, since the core computes instances and
cannot depend on `red-ale-import`, and add variables and assignments as GnuCash's parser has them. Function calls are
the hard part: GnuCash evaluates them in Guile (the same file's comment), and the loan assistant writes formulas
that call financial functions **(to check: `pmt`, `ipmt`, `ppmt` and their Scheme source)**. red-ale never runs
Guile, so those functions are translated into Rust, and any other function call is refused with a reason. GnuCash
computes them in floating point **(to check)**; red-ale must give the same figures after rounding to the SCU without
floats. "Decide before code", item 6.

**Since Last Run** works as import does, in two calls, on the same rules ([`api/import.md`](api/import.md)):

- `scheduledTransaction.sinceLastRun` proposes every instance due from each schedule's last occurrence up to today,
  plus its days in advance: those to create, reminders, and postponed (deferred) ones, each with the variables it
  needs **(to check: the instance states in `gnc-sx-instance-model`)**. Auto-create schedules' instances are
  proposed as *create*. "Today" is the local day of the Env clock.
- `scheduledTransaction.apply` takes the proposal's ID with only the caller's choices: create, postpone or skip per
  instance, and variable values. It creates the transactions and advances each schedule's last occurrence, instance
  count, remaining occurrences and deferred instances as GnuCash does, in one Mutation.
- The server holds the proposal by ID with the Book revision it was made at, and refuses it at any other
  (`conflict`), as `import.apply` does. `scheduledTransaction.discard` drops one.
- Instances dated before the auto read-only threshold are flagged with the threshold date, as import flags them.
  Whether GnuCash's Since Last Run checks the threshold **(to check)** decides whether they are created or refused.

**Schedule from transaction** is GnuCash's dialog as one operation, `scheduledTransaction.fromTransaction`: the
template is the Transaction's splits with their amounts as formulas, and the caller gives the schedule
**(to check: `dialog-sx-from-trans.c`)**.

**The loan assistant** becomes `scheduledTransaction.loan`: the loan's principal, rate, term and accounts in, the
scheduled transactions GnuCash's assistant would build out, with the same formulas, so GnuCash computes the same
repayments from them **(to check: `assistant-loan.cpp`)**.

## Budgets

| Status | Feature               | Notes                                                                                                   |
|:------:|-----------------------|---------------------------------------------------------------------------------------------------------|
|        | Budgets               | In the core model; `budget.*` to create, list, get, update and delete                                   |
|        | Estimate from history | `budget.estimate`: fills amounts for chosen accounts from past transactions, as GnuCash's dialog does   |
|        | Budget notes          | A note per account and period                                                                           |
|        | Natural signs         | Amounts kept in natural signs; books from before 3.8 handled as below                                   |

**The model.** A Budget has a GUID, name, description, number of periods and a Recurrence that gives the periods,
and keeps its amounts and notes in its Slots: amounts at `<account-guid>/<period>`, notes at
`notes/<account-guid>/<period>` (`5.17:libgnucash/engine/gnc-budget.cpp:495-508`;
[`notes/xml-format-core.md`](notes/xml-format-core.md) §8.3). Period keys sort as strings (`0`, `10`, `2`), and the
writer keeps that order. The Book option `options/Budgeting/Default Budget` names the default budget
([`notes/storage-and-safety.md`](notes/storage-and-safety.md) §11). Setting many amounts is one call
(`budget.setAmounts`), atomic as every Mutation is.

**Natural signs.** Since 3.8, budget amounts are stored with their natural signs, and the Book feature says so.
GnuCash's GUI scrubs older books on open: it flips the signs and sets the feature if budgets exist, and unsets it if
there are none (`5.17:libgnucash/engine/ScrubBudget.c:193-216`, called from `5.17:gnucash/gnome-utils/gnc-file.c:768`).
red-ale never changes a book on open. *Proposed:* a book with budgets and without the feature can be read, but
budget edits are refused with a clear error until `budget.scrubSigns` runs GnuCash's scrub, as an explicit
Mutation. red-ale never unsets the feature. "Decide before code", item 7.

**Estimate from history** follows GnuCash's estimate dialog: the period's actuals from a chosen start, per account,
with its rounding options **(to check: `gnc-plugin-page-budget.cpp`)**. It overwrites the amounts it fills, so its
Effect is *destructive*.

## Reports

| Status | Report                                    | Source                                                                         |
|:------:|-------------------------------------------|--------------------------------------------------------------------------------|
|        | Budget Report                             | `standard/budget.scm` ([`reports.md`](reports.md), "The budget module")       |
|        | Budget Balance Sheet                      | **(to check: file)**                                                           |
|        | Budget Income Statement                   | **(to check: file)**                                                           |
|        | Budget Flow                               | **(to check: file)**                                                           |
|        | Budget Chart                              | **(to check: file)**                                                           |
|        | Future Scheduled Transactions Summary     | A preset of the Account Summary, `47f45d7d6d57b68518481c1fc8d4e4ba` (`standard/account-summary.scm:430-435`) |
|        | Balance Forecast                          | **(to check: file)**                                                           |

All are translated from GnuCash's Scheme into Rhai, on the v0.2 engine, as [`reports.md`](reports.md) sets out.
They need two additions:

- **The query API reads budgets and future instances**: a budget's periods, amounts and notes, and the instances a
  schedule would create in a date range, computed without creating them. Reports still only read.
- **The HTML account table's `report_budget`** is refused today because budgets were outside v0.2
  ([`reports.md`](reports.md), the library conventions); v0.4 translates it.

Expected results come from GnuCash's own budget and forecast report tests where they exist **(to check:
`gnucash/report/reports/standard/test/`)**, rebuilt as fixtures by the oracle as v0.2 did
([`reports.md`](reports.md) §13), and from hand-worked cases citing the 5.17 lines they follow.

## Also in v0.4

| Status | Item                                   | Notes                                                                                                   |
|:------:|----------------------------------------|---------------------------------------------------------------------------------------------------------|
|        | Mutations walk their maps in order     | `transaction_changes` (`crates/red-ale-service/src/session.rs`) walks the old and new maps in order, as [`milestone3.md`](milestone3.md) ("Decided") measured: about a hundred times faster at finding changes. First, since Since Last Run may create many transactions and the SQL save uses the same walk |
|        | The `sql-book` message                 | `crates/red-ale-service/src/error.rs:41` says SQLite arrives in v0.3. SQLite files no longer get it; for database URIs it names v0.7 |

## Carried forward from v0.3

From [`milestone3.md`](milestone3.md), "Follow-ups":

| Status | Follow-up                                 | Proposed                                                                                          |
|:------:|-------------------------------------------|---------------------------------------------------------------------------------------------------|
|        | Mutation cost                             | In v0.4, first (above)                                                                            |
|        | A loopback guard for quote tests          | In v0.4: small, and makes a network call from a test impossible rather than unlikely              |
|        | Fuzzing in CI                             | In v0.4, with the new targets (see "Testing")                                                     |
|        | Rhai closure bugs                         | Report upstream in v0.4; the conventions stay until a release fixes them                          |
|        | `ring` is C                               | Unchanged; SQLite may add a second C library ("Decide before code", item 3)                       |
|        | Opening an exported account tree          | Not in v0.4; still open in [`open-questions.md`](open-questions.md)                               |
|        | GnuCash's QIF account map file            | Not in v0.4                                                                                       |

## Interfaces

These are red-ale's own work, not GnuCash features. Each feature above counts as done only once it is reachable
through JSON-RPC, MCP and the CLI.

| Status | Interface    | Notes                                                                                                                         |
|:------:|--------------|-------------------------------------------------------------------------------------------------------------------------------|
|        | JSON-RPC API | `scheduledTransaction.*` and `budget.*`; `book.open` takes SQLite files and `sqlite3://`; `book.create` takes `format`. `redAleApi` becomes `0.4.0`, additive over `0.3.0`: the 0.3.0 document is committed beside the 0.2.0 one, and `api_additive.rs` checks against it |
|        | MCP server   | Every new method as a tool. `scheduledTransaction.delete`, `budget.delete` and `budget.estimate` are destructive; `scheduledTransaction.sinceLastRun`, `scheduledTransaction.occurrences`, `budget.get` and `budget.list` read only |
|        | CLI          | Every new method through `exec` and `call`                                                                                    |

Method names are *Proposed* ("Decide before code", item 10). The designs go in `api/scheduled-transactions.md` and
`api/budgets.md`, and the SQL Backend's in a new `sql-fidelity.md` beside [`file-fidelity.md`](file-fidelity.md).

## Testing

Every feature follows [`testing.md`](testing.md). The oracle reads and writes SQLite books
(`Session("sqlite3://<path>")`), so SQLite gets the same round-trip and oracle tests as XML.

- **SQLite round-trips.** For every canonical fixture, the oracle writes an SQLite copy **(to check: how the Python
  bindings save a loaded book to `sqlite3://`)**. red-ale opens it and closes it without a save, and the file is
  unchanged byte for byte; it opens, edits and saves it, and GnuCash 5.17 opens the result with nothing logged,
  compares it whole with what it expects, and finds every row red-ale doesn't model unchanged.
- **One book, two formats.** The XML and SQLite copies of each fixture read to equal Books.
- **Locks.** red-ale refuses a book GnuCash holds, GnuCash refuses one red-ale holds, and red-ale's `gnclock` row
  goes on every exit path (a test for each error path, as for the `.LCK` file).
- **Schema versions.** A book with any table at another version opens read-only and names the table.
- **Recurrences and Since Last Run.** GnuCash's own tests, rebuilt as red-ale tests **(to check: `test-recurrence.c`,
  `test-schedxaction.c`, and the Since Last Run tests in `app-utils/test/`)**, each citing the test it ports; property
  tests that next occurrences never go backwards and that Since Last Run twice on the same day creates nothing; and
  the oracle opening every book Since Last Run wrote.
- **Budgets.** GnuCash reads back the amounts and notes red-ale wrote, through the bindings where they reach budgets
  **(to check)**, else from the saved file.
- **Fuzzing.** New targets for the SQL row translators (an SQLite file is outside input) and for formulas with
  variables, each seeded from the fixtures and run in CI with the existing ones.

## Order of work

1. **The Mutation walk**, since Since Last Run and the SQL save both lean on it.
2. **The glossary and the designs**: the terms below, `api/scheduled-transactions.md`, `api/budgets.md` and
   `sql-fidelity.md`.
3. **The Backend seam**, with the XML Backend moved behind it and every test passing unchanged.
4. **Scheduled transactions and budgets in the core and the XML Backend**, lifted from Residue into the model. Every
   round-trip test passes unchanged: a book GnuCash wrote still comes back byte for byte.
5. **Scheduled transaction operations**: recurrences, formulas, Since Last Run, from transaction, then the loan
   assistant.
6. **Budget operations**: amounts, notes, natural signs, estimate.
7. **The reports.**
8. **`red-ale-sql`**: reading (books open read-only), then the lock, writing and saving, the journal and backups.

If v0.4 runs long and splits, *Proposed:* items 1–7 ship as v0.4 and item 8 as v0.4.x ("Decide before code",
item 1).

## Decide before code

1. **Which half ships first if v0.4 splits.** The roadmap leaves this here. *Proposed:* scheduled transactions and
   budgets first. They help every XML user at once, and XML is GnuCash's default format; with them in the model,
   `red-ale-sql` maps every table from its first line; and the SQL Backend is the riskiest work for a user's book, so
   it gains from not being rushed. The cost: SQLite users wait one more release, losing access but no data, since a
   refused SQL book is never touched.
2. **Saving SQL books.** At `book.save`, in one checked SQL transaction (*Proposed*, "SQLite books"), or each
   Mutation written through as GnuCash does. Writing through matches GnuCash but changes what `book.save` and
   closing without a save mean for one format only.
3. **The SQLite library.** `rusqlite` with its bundled SQLite compiles C, a second C library in the build beside
   `ring` ([`architecture.md`](architecture.md), "Dependencies"). The pure-Rust SQLite engines are not yet ready to
   trust with a user's books. *Proposed:* `rusqlite` with `bundled`, so the SQLite version is pinned and not the
   system's, recorded as `ring` is. Check its licence (`libsqlite3-sys`: MIT; SQLite: public domain) against
   `deny.toml`.
4. **Schema versions.** *Proposed:* any table version other than 5.17's opens read-only; red-ale never upgrades.
   Decide whether an older schema that 5.17 would upgrade gets a method later.
5. **Double slots in SQLite.** The `REAL` column has no text form to keep. Either the SQL Backend keeps the value's
   eight bytes as read and writes them back unchanged, converting to the core's text form only for display, or a book
   holding a double Slot opens read-only. Either way no float is computed with.
6. **Financial functions in formulas.** The Financial calculator is ❌, so only the functions formulas call are
   translated, and only as formulas need them. How red-ale gets GnuCash's figures for `pmt`-style functions, which
   GnuCash computes in floating point, without floats: exact rationals (exact for whole numbers of periods) rounded
   to the SCU, checked against hand-worked cases from the Scheme source, with any case where GnuCash's double result
   rounds differently found and recorded as a deviation.
7. **Natural signs.** *Proposed* in "Budgets": budget edits refused until `budget.scrubSigns`, and the feature never
   unset. Check what GnuCash 5.17 does with a book whose budgets were all deleted.
8. **Attached documents.** *Proposed:* out of v0.4, with a design doc before any version takes them.
9. **Glossary.** New terms enter [`glossary.md`](glossary.md) before code: *Scheduled transaction*, *Schedule*,
   *Recurrence*, *Occurrence*, *Instance* and its states, *Deferred instance*, *Template account*, *Template
   transaction*, *Formula*, *Variable*, *Since Last Run*, *Budget*, *Budget period*, *Budget amount*, *Budget note*,
   *Natural signs*, *SQL book*, *Schema version*. Flag the clashes:
   - *Template transaction* is not *Report template*;
   - a *Budget period* comes from the budget's Recurrence and is not the *Accounting period*;
   - a *Schedule* is a scheduled transaction's list of Recurrences, not the scheduled transaction;
   - GnuCash calls every entity a *QofInstance*; a scheduled transaction's *Instance* is one occurrence of it;
   - GnuCash's code says *SX*; red-ale writes *scheduled transaction* in docs and `ScheduledTransaction` in code.
10. **Method names.** *Proposed:* `scheduledTransaction.*` and `budget.*`, in the style of `billTerm.*` and
    `taxTable.*`. `sx.*` is shorter but is GnuCash's code abbreviation, not its user term.

## Rows missing from `features.md`

- **Save As between formats.** GnuCash's Save As writes an open book to XML or SQLite. There is no row. *Proposed:*
  add one under §9 "Formats" and place it once both Backends exist. Converting is safe only for what both formats
  model, since one Backend's Residue cannot be written by the other: a book carrying Residue would be refused.

## Done when

- Every row above is ✅ or ❌, and each ✅ row's Ported column in `features.md` says `0.4`.
- An SQLite book GnuCash 5.17 wrote, opened and closed without a save, is unchanged byte for byte. Opened, edited and
  saved, GnuCash 5.17 opens it with no warnings and no lost data, and every table, column and row red-ale doesn't
  model is unchanged.
- red-ale never opens for writing an SQLite book GnuCash holds, GnuCash sees red-ale's lock, and red-ale releases it
  on every exit path, error paths included.
- An SQLite book with a table at a version other than 5.17's opens read-only and says which.
- Every canonical fixture reads to the same Book as XML and as SQLite.
- Every recurrence type gives the occurrences GnuCash's own tests expect. Since Last Run creates the transactions
  GnuCash would (dates, amounts, Slots) and advances each schedule as GnuCash does, and run twice on the same day it
  creates nothing.
- Formulas are exact, and the loan assistant's schedules give repayments that match GnuCash's to the SCU.
- Budget amounts and notes red-ale wrote are read back by GnuCash 5.17, and every book declares only the features
  GnuCash would have set.
- Every report above gives the figures its expected results hold.
- The memory budgets of [`milestone3.md`](milestone3.md) still hold on the 100,000-transaction book, and a Since Last
  Run creating 5,000 transactions is one Mutation.
- Every JSON-RPC method is an MCP tool with the same name, params schema and hints, and `redAleApi` 0.4.0 is
  additive over 0.3.0, checked by a test.
- v0.1's, v0.2's and v0.3's done-when checks still pass.

## Follow-ups

None yet.
