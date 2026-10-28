# Milestone 4: red-ale v0.4

**Status: in progress.** The scope comes from [`roadmap.md`](roadmap.md) ("v0.4"). What GnuCash 5.17 does is in three
research notes, every claim cited to the source at the pin:
[`notes/sql-backend.md`](notes/sql-backend.md), [`notes/engine-scheduled-transactions.md`](notes/engine-scheduled-transactions.md)
and [`notes/engine-budgets.md`](notes/engine-budgets.md). The "Decide before code" items were settled at the start
of the work ("Decided", below), from the *Proposed* answers of the draft and what the research found.

**Goal:** red-ale opens, edits and saves SQLite books without loss, with GnuCash's lock, journal and Book features;
creates, edits and runs scheduled transactions, with formulas, variables and Since Last Run, as GnuCash does;
creates and edits budgets; and runs the budget reports, the Future Scheduled Transactions Summary and the Balance
Forecast. All of it on XML and SQLite books alike.

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

Also in v0.4: the Mutation walk and the other v0.3 follow-ups taken up here ("Carried forward from v0.3").

Why these together: scheduled transactions and budgets are Residue today. red-ale keeps them byte for byte but can't
create, edit or run them, so a user who relies on them still needs GnuCash. SQL books are refused outright. Each of
the three lifts a reason a user's book can't be fully used ([`roadmap.md`](roadmap.md), "How the order is chosen",
rule 1). With the model work in the same version, `red-ale-sql` maps the `schedxactions`, `recurrences`, `budgets`
and `budget_amounts` tables from the start.

**Out:**

- **MySQL/MariaDB and PostgreSQL books** (v0.7). `red-ale-sql` is written so a second SQL dialect fits beside
  SQLite, but only SQLite connects in v0.4. Database URIs (`mysql://`, `postgres://`) are still refused with
  `sql-book` before anything is touched.
- **Autosave: won't do (❌).** Every operation is atomic, so a timer adds nothing a client can't do with
  `book.save`; unsaved changes are in the journal ([`features.md`](features.md), §9 "Safety").
- **Attached documents.** [`milestone3.md`](milestone3.md) ("Out") moved them to v0.4; they move out again ("Decided",
  item 8). GnuCash has no attachments, only document links, which v0.2 already ports (`docLink.*`).
- **Running Since Last Run on open.** GnuCash's GUI runs it when a book opens, by default (a `HOOK_BOOK_OPENED`
  handler, `5.17:gnucash/gnome/dialog-sx-editor.c:1893-1899`; the `dialogs.sxs.since-last-run.show-at-file-open`
  setting, default true, `5.17:gnucash/gschemas/org.gnucash.GnuCash.dialogs.sxs.gschema.xml.in:15-19`), and creates
  auto-create instances at once (`5.17:gnucash/gnome/dialog-sx-since-last-run.c:1433`). red-ale is headless and never
  changes a book on open: a client asks for Since Last Run.
- **Converting between XML and SQLite** (GnuCash's Save As). `features.md` has no row for it; see "Rows missing from
  `features.md`".
- **Books with legacy frequency specs** (`sx:freqspec`). GnuCash 5.17 reads them and rewrites them as recurrences,
  and never writes them ([`notes/xml-format-core.md`](notes/xml-format-core.md) §8.5). red-ale opens such a book
  read-only, with the reason.

## Compatibility in v0.4

Out of scope still never means lost, and every v0.1, v0.2 and v0.3 rule holds. Additions:

- **Opening an SQLite book never writes to it.** GnuCash's load writes even in a read-only session: it sets the
  ISO-dates Book feature, creates missing tables, rebuilds tables at older versions, recovers leftover `<table>_back`
  tables and adds Imbalance splits ([`notes/sql-backend.md`](notes/sql-backend.md) §0, §5.4). red-ale does none of
  these. A read-only open leaves the file byte for byte as it was. A normal open writes only the `gnclock` row, which
  moves SQLite's file change counter, so after a normal open and a close with no edits every row is as it was but the
  file's bytes are not (§10.2).
- **Rows, columns and tables red-ale doesn't model are never touched.** A save writes only rows of entities that
  changed, and only the columns GnuCash 5.17 defines. A book with an unknown table, an unknown column of a known table,
  or a value of a storage class GnuCash never writes in that column opens read-only, as unrecognised XML content does
  ([`open-questions.md`](open-questions.md), "Unrecognised content opens read-only").
- **Schema versions are respected, never upgraded.** GnuCash silently upgrades older per-table versions on every
  load, read-only included, and ignores newer ones (`5.17:libgnucash/backend/sql/gnc-sql-object-backend.cpp:74-86`);
  its GUI treats only the database-level `Gnucash` and `Gnucash-Resave` rows of `versions` as too old or too new
  (`5.17:libgnucash/backend/dbi/gnc-backend-dbi.cpp:882-896`, `5.17:gnucash/gnome-utils/gnc-file.c:1074-1090`).
  red-ale opens any book whose `versions` rows differ from 5.17's read-only, names the tables, and never upgrades.
- **The SQL lock is GnuCash's.** `gnclock (Hostname varchar(255), PID int)`; any row at all means locked
  (`ERR_BACKEND_LOCKED`), whatever its host or PID (`5.17:libgnucash/backend/dbi/gnc-dbisqlconnection.cpp:101-169`).
  red-ale never opens for writing a book with a `gnclock` row, writes its host name and PID as GnuCash does, and
  deletes only its own row on every exit path, error paths included. It never copies `SESSION_BREAK_LOCK`, which
  deletes every row, nor GnuCash's unlock, which deletes every row if one is its own (`:171-227`).
- **Book features are set as GnuCash sets them.** GnuCash sets `ISO-8601 formatted date strings in SQLite3
  databases.` on every SQLite load (`5.17:libgnucash/backend/dbi/gnc-backend-dbi.cpp:879-880`). red-ale writes only
  ISO timestamps and sets the feature in its first save of an SQLite book that lacks it, not on open. Creating the
  first budget sets `Use natural signs in budget amounts` (`5.17:gnucash/gnome/gnc-plugin-budget.c:181-183`), and
  deleting the last unsets it, as GnuCash's budget page does (`5.17:gnucash/gnome/gnc-plugin-page-budget.cpp:944-948`).
  Both features are already in `SUPPORTED_FEATURES` (`crates/red-ale-service/src/session.rs:24`).
- **The journal covers SQL books too.** GnuCash keeps one for SQL books (`5.17:libgnucash/backend/dbi/gnc-backend-dbi.cpp:456`),
  named `<book path>.<YYYYMMDDhhmmss>.log` in local time (`5.17:libgnucash/engine/TransLog.cpp:174-176`). red-ale
  writes one beside an SQLite book as beside an XML one, and `book.replayJournal` works on both.
- **Template transactions are not transactions of the Book.** The template accounts and template transactions of
  scheduled transactions live apart from the Account tree: no balance, report, find query, import match, scrub or
  export sees them.
- **Transactions made from a schedule look as GnuCash makes them:** the template's description, num, notes, memos
  and actions; posted at the Day-neutral time; a `date-posted` slot; and `from-sched-xaction`, the scheduled
  transaction's GUID, and no other slot ([`notes/engine-scheduled-transactions.md`](notes/engine-scheduled-transactions.md)
  §4.6; `5.17:libgnucash/app-utils/gnc-sx-instance-model.c:1405-1409`).
- **Accounts used by schedules or budgets can't be deleted silently.** GnuCash doesn't check
  ([`notes/engine-accounts-transactions.md`](notes/engine-accounts-transactions.md) §3.2), and keeps a deleted
  account's budget amounts ([`notes/engine-budgets.md`](notes/engine-budgets.md) §1.7). red-ale keeps its stricter
  rule ([`file-fidelity.md`](file-fidelity.md), "References from schedules and budgets"), now checked in the model: deleting
  such an Account is refused, naming the schedules and budgets that use it.
- **Budget order is kept as read.** GnuCash's own budget write order is not reproducible
  ([`notes/xml-format-core.md`](notes/xml-format-core.md) §7.6), and GnuCash ignores it on read.
- **Since Last Run is one Mutation.** Every transaction it creates and every schedule it advances applies together,
  or not at all.
- **Nothing is changed on open that GnuCash's GUI changes on open:** not the template numerics
  (`5.17:gnucash/gnome-utils/gnc-file.c:1117-1131`), not budget signs (`5.17:gnucash/gnome-utils/gnc-file.c:753-793`).

## SQLite books

| Status | Item                        | Notes                                                                                                         |
|:------:|-----------------------------|---------------------------------------------------------------------------------------------------------------|
|        | SQLite books                | `red-ale-sql`: read and write every table red-ale models                                                       |
|        | ISO dates in SQLite         | Read both timestamp forms; write ISO; set the Book feature on the first save                                   |
|        | The Backend seam            | One interface in `red-ale-service` for opening, reading, saving and locking, with XML and SQLite behind it     |
|        | Schema versions             | Any `versions` row other than 5.17's opens read-only, naming the tables                                        |
|        | The SQL lock                | `gnclock`, taken at open and released on every exit path                                                       |
|        | Saving                      | One SQL transaction per save, read back and compared before it commits                                         |
|        | Journal and backups         | The `.log` journal as for XML; a backup copy before the first save of a session                               |

**The Backend seam.** `red-ale-service` today opens, saves and locks XML books directly. A `Backend` trait in
`red-ale-service` puts XML and SQLite behind one interface, so the Session, the journal, read-only triggers and every
use case are the same for both formats. The Residue is each Backend's own type; only the Backend reads it. The XML
Backend moves behind the seam first, with no change to any byte it writes.

**Reading.** `red-ale-sql` translates each table into the core model, an anti-corruption layer as `red-ale-xml` is:
column names and SQL quirks never reach the core. The schema, column types and encodings are in
[`notes/sql-backend.md`](notes/sql-backend.md) §2–§3. Amounts are `<name>_num` and `<name>_denom` `bigint` pairs,
written unreduced (`5.17:libgnucash/backend/sql/gnc-sql-column-table-entry.cpp:578-634`), read as integers only; a
value of another storage class is unrecognised content. Rows are read in `rowid` order: GnuCash's queries have no
`ORDER BY`, so `rowid` is what SQLite gives it for a plain table scan (§9), and KVP list items keep their row order.
Budget amounts are stored twice in SQLite, in `budget_amounts` and in `slots`
([`notes/engine-budgets.md`](notes/engine-budgets.md) §6); a book where the two disagree opens read-only.

**Timestamps.** 5.17 writes `YYYY-MM-DD HH:MM:SS` in UTC (`5.17:libgnucash/engine/gnc-datetime.cpp:630-636`); books
written before the ISO feature hold `YYYYMMDDHHMMSS`, also UTC. GnuCash reads both (`gnc-datetime.cpp:381-416`,
`:432-485`) and converts a row only when it rewrites it. red-ale does the same: it reads both, keeps unchanged rows
as they are, and writes ISO.

**Saving.** GnuCash has no Save for SQL books: each commit is written straight away in its own SQL transaction
(`5.17:libgnucash/backend/sql/gnc-sql-backend.cpp:580-666`), and `qof_session_save` does nothing
(`5.17:libgnucash/engine/qofsession.cpp:439-440`). red-ale keeps changes in memory and in the journal until
`book.save` for both formats ("Decided", item 2). A save:

1. diffs the Book against the Book as read (`red_ale_core::diff`);
2. writes the changed rows in one SQL transaction: for an entity whose slots changed, its slot rows are deleted and
   inserted again, as GnuCash does (`5.17:libgnucash/backend/sql/gnc-slots-sql.cpp:645-649`);
3. reads the written entities back inside that transaction and compares them with the Book;
4. commits only if they match, and rolls back otherwise.

**Backups.** GnuCash makes none for SQL books ([`notes/sql-backend.md`](notes/sql-backend.md) §8). red-ale copies
the SQLite file to a timestamped backup beside it before the first save of a session, named and retained as XML
backups are, using SQLite's backup API so the copy is consistent. This goes beyond GnuCash but changes nothing GnuCash
reads.

**Creating.** `book.create` makes XML books by default, as v0.1 decided (a missing path is never taken as SQLite;
[`notes/storage-and-safety.md`](notes/storage-and-safety.md) §15, item 9). It gains a `format` param (`xml` or
`sqlite`) to create an SQLite book with GnuCash 5.17's schema and `versions` rows.

**Double slots.** GnuCash stores them in `double_val float8` and writes them with 12 fixed decimals, losing precision
itself (`5.17:libgnucash/backend/sql/gnc-sql-column-table-entry.hpp:403-416`; [`notes/sql-backend.md`](notes/sql-backend.md)
§3.8). red-ale keeps each double's eight bytes as read and writes them back unchanged; a double red-ale itself sets is
written as GnuCash writes it. No float is computed with.

**Data SQLite can't hold.** There is no column for a scheduled transaction's deferred instances, and GnuCash loses
them ([`notes/sql-backend.md`](notes/sql-backend.md) §0). In an SQLite book, postponing an instance is refused with
the reason.

## Scheduled transactions

| Status | Feature                     | Notes                                                                                                     |
|:------:|-----------------------------|-----------------------------------------------------------------------------------------------------------|
|   ✅   | Scheduled transactions      | In the core model, with their template accounts and template transactions; `scheduledTransaction.*`       |
|   ✅   | Recurrence rules            | Every period type, multiplier and weekend adjustment, with next occurrences as GnuCash computes them      |
|   ✅   | Formulas and variables      | Template split amounts as formulas over variables, evaluated exactly                                      |
|   ✅   | Since Last Run              | Two calls, as import: a proposal of due instances, then apply                                             |
|   ✅   | Schedule from transaction   | A scheduled transaction made from an existing Transaction                                                 |
|   ✅   | Mortgage and loan assistant | One operation that builds a loan's repayment schedule, as the stock split assistant became `stock.split` |

What GnuCash does is in [`notes/engine-scheduled-transactions.md`](notes/engine-scheduled-transactions.md); its §10
lists the implications this section adopts.

**The model.** A Scheduled transaction holds every field the XML writer writes (§1.1;
[`notes/xml-format-core.md`](notes/xml-format-core.md) §8.2), as read: its Recurrences' raw fields, its deferred
instances in file order, and both an end date and a count if a file has both. Template accounts and template
transactions are their own aggregate, under the Template root, never in the Account tree. Each template split's
`sched-xaction` Frame (the real account, the credit and debit formulas and numerics) is a value object (§1.3).

**Recurrences** are ported from `5.17:libgnucash/engine/Recurrence.cpp:202-381`, not redesigned, and computed from
the stored fields, since GnuCash's reader doesn't normalise them (§2.1). The normalisation of `recurrenceSet` applies
only to recurrences red-ale creates. GnuCash's `test_some` cases (`5.17:libgnucash/engine/test/test-recurrence.c:361-399`)
are ported exactly and `test_all` (`:212-252`) as a property test, with the oracle's `recurrenceNextInstance`, which
starts no Guile, giving expected dates.

**Formulas and variables.** v0.3's translation of GnuCash's expression parser
(`crates/red-ale-import/src/business/expr.rs`) moves into `red-ale-core`, which computes instances, and gains
assignments, compound assignments, `(n)` negation, string arguments and `:`-separated function calls (§3.1–§3.4).
Variables come only from the instance's bindings (§3.5); the user's global `expressions-2.0` table is not consulted.
Function calls are Guile procedures in GnuCash, called with doubles, with the result rounded half up to 12
significant figures (`5.17:libgnucash/app-utils/gnc-exp-parser.c:288-360`). red-ale never runs Guile: it implements
the financial functions of `5.17:bindings/guile/fin.scm` exactly ("Decided", item 6), and refuses any other function
name with a reason.

**Since Last Run** works as import does, in two calls, on the same rules ([`api/import.md`](api/import.md)):

- `scheduledTransaction.sinceLastRun` proposes the due instances as GnuCash builds them (§4.2): postponed instances
  first, then those to create up to today plus the days to create in advance, then reminders up to that plus the
  days to remind. Each carries its state (GnuCash's `TO_CREATE`, `REMINDER`, `POSTPONED`, `IGNORED`, `CREATED`;
  `5.17:libgnucash/app-utils/gnc-sx-instance-model.h:52-60`) and the variables it needs. "Today" is the local day of
  the Env clock.
- `scheduledTransaction.apply` takes the proposal's ID with only the caller's choices (create, postpone, skip, a
  reminder; variable values), checks GnuCash's rule that a reminder can't come before any other state (§4.3), creates
  the transactions and advances each schedule as GnuCash does (§4.4, §4.7), in one Mutation.
- The server holds the proposal by ID with the Book revision it was made at, and refuses it at any other
  (`conflict`), as `import.apply` does. `scheduledTransaction.discard` drops one.
- Three deviations, each documented in `api/scheduled-transactions.md` ("Decided", item 11): red-ale does not
  propose again instances a postponed one already led to be created (GnuCash's duplicates, §4.8); an instance whose
  formula fails is refused, creating nothing, where GnuCash creates it with zero amounts and does not advance (§4.8);
  and instances dated before the auto read-only threshold are flagged and need the caller's explicit choice, where
  GnuCash ignores the threshold (`5.17:libgnucash/app-utils/gnc-sx-instance-model.c:1453-1457`).

**Schedule from transaction** is GnuCash's dialog (`5.17:gnucash/gnome/dialog-sx-from-trans.cpp`) as one operation,
`scheduledTransaction.fromTransaction` (§5): named after the description, starting at the next occurrence after the
transaction's date, never ending unless given an end, one template transaction whose split values become formulas.
red-ale writes the formulas locale-free (`1234.50`) and sets the numerics too, where GnuCash writes the locale's
grouped form and no numerics.

**The loan assistant** becomes `scheduledTransaction.loan` (§6;
`5.17:gnucash/gnome/assistant-loan.cpp:2838-3046`): the loan's principal, rate, term, payment frequency and accounts
in, the scheduled transactions GnuCash's assistant builds out, with the same `pmt`, `ppmt` and `ipmt` formulas
(`:2500-2528`) and instance count (`:2863-2865`), so GnuCash computes the same repayments from them. Defects the note
records (§6.3) are not copied.

## Budgets

| Status | Feature               | Notes                                                                                                   |
|:------:|-----------------------|---------------------------------------------------------------------------------------------------------|
|   ✅   | Budgets               | In the core model; `budget.*` to create, list, get, update, copy and delete                             |
|   ✅   | Estimate from history | `budget.estimate`: fills amounts for chosen accounts from past transactions, as GnuCash's dialog does   |
|   ✅   | Budget notes          | A note per account and period                                                                           |
|   ✅   | Natural signs         | Amounts kept in natural signs; older books scrubbed only when asked                                     |

What GnuCash does is in [`notes/engine-budgets.md`](notes/engine-budgets.md).

**The model.** A Budget has a GUID, name, description, number of periods and a Recurrence that gives the periods, and
keeps its amounts and notes in its Slots: amounts at `<account-guid>/<period>`, notes at `notes/<account-guid>/<period>`
(`5.17:libgnucash/engine/gnc-budget.cpp:494-508`). An unset amount has no slot; an explicit zero is stored as `0/1`;
amounts are stored as given, neither rounded nor reduced (§1.2). Lowering the number of periods hides amounts beyond
it but keeps them (§1.3). Period keys sort as strings (`0`, `10`, `2`). Period dates and actuals follow §1.4–§1.5.
The Book option `options/Budgeting/Default Budget` names the default budget; when it is unset or stale, red-ale picks
the first budget in book order, where GnuCash's choice is hash order (§1.6). Copying names the copy `Copy of <name>`
and, as GnuCash does, drops the notes (§1.7).

**Natural signs.** Since 3.8 budget amounts are stored with their natural signs, and the Book feature says so.
GnuCash's GUI scrubs a book after every open: it guesses each budget's sign convention from its summed income and
expense amounts, flips accordingly and sets the feature; it unsets the feature when there are no budgets
(`5.17:libgnucash/engine/ScrubBudget.c:105-156`, `:193-216`; §2). red-ale never changes a book on open. A book with
budgets and without the feature can be read, but budget edits are refused until `budget.scrubSigns` runs GnuCash's
scrub as an explicit Mutation.

**Estimate from history** follows GnuCash's dialog (`5.17:gnucash/gnome/gnc-plugin-page-budget.cpp:955-1092`): the
accounts, a start date, significant digits (1–12, default 1) and whether to use the average. History period *i* fills
budget period *i*; with the average, the whole range's change divided by the number of periods fills every period.
Rounding is half up to significant digits, never left of the decimal point, and denominators are left unreduced
(§3.4). It overwrites set amounts, so its Effect is *destructive*.

## Reports

| Status | Report                                | Source (GUID)                                                                         |
|:------:|---------------------------------------|---------------------------------------------------------------------------------------|
|   ✅   | Budget Report                         | `standard/budget.scm` (`810ed4b25ef0486ea43bbd3dddb32b11`)                            |
|        | Budget Balance Sheet                  | `standard/budget-balance-sheet.scm` (`ecc35ea9dbfa4e20ba389fc85d59cb69`)              |
|        | Budget Income Statement               | `standard/budget-income-statement.scm` (`583c313fcc484efc974c4c844404f454`)           |
|        | Budget Profit & Loss                  | the same file (`e5fa5ce805e840ecbeca4dba3fa4ead9`), under the Budget Income Statement row |
|        | Budget Flow                           | `standard/budget-flow.scm` (`e6e34fa3b6e748debde3cb3bc76d3e53`)                       |
|        | Budget Chart                          | `standard/budget-barchart.scm` (`415cd38d39054d9e9c4040455290c2b1`)                   |
|        | Future Scheduled Transactions Summary | `standard/account-summary.scm:430-435` (`47f45d7d6d57b68518481c1fc8d4e4ba`)           |
|        | Balance Forecast                      | `standard/balance-forecast.scm` (`321d940d487d4ccbb4bd0467ffbadbf2`)                  |

All are translated from GnuCash's Scheme into Rhai, on the v0.2 engine, as [`reports.md`](reports.md) sets out. The
report library needs what [`notes/engine-budgets.md`](notes/engine-budgets.md) §4.11 lists: budget reads (periods,
amounts, notes, actuals), a scheduled-transaction cash-flow read (§4.10: occurrences after the last one, both range
ends inclusive, deferred instances ignored, formulas with every variable at 0), the account table's `report_budget`
and `get_balance_fn`, `make_date_interval_list`, and the budget, complex-boolean and callback option types. Reports
still only read.

Expected results come from GnuCash's `test-budget.scm`, which checks the Budget Report, Budget Income Statement and
Budget Balance Sheet, rebuilt as fixtures by the oracle as v0.2 did ([`reports.md`](reports.md) §13), and from
hand-worked cases for the rest, each citing the 5.17 lines it follows (§5).

## Also in v0.4

| Status | Item                                   | Notes                                                                                                   |
|:------:|----------------------------------------|---------------------------------------------------------------------------------------------------------|
|   ✅   | Mutations walk their maps in order     | `red_ale_core::diff::changes`, used by `Session::try_mutate` and the trading-account commit; a property test checks it against lookups |
|   ✅   | Clippy after `cargo fmt`               | The formatting commit left pedantic lints failing; fixed without changing behaviour                     |
|        | The `sql-book` message                 | `crates/red-ale-service/src/error.rs:41` says SQLite arrives in v0.3. SQLite files no longer get it; for database URIs it names v0.7 |

## Carried forward from v0.3

From [`milestone3.md`](milestone3.md), "Follow-ups":

| Status | Follow-up                                 | Notes                                                                                             |
|:------:|-------------------------------------------|---------------------------------------------------------------------------------------------------|
|   ✅   | Mutation cost                             | Above                                                                                             |
|   ✅   | A loopback guard for quote tests          | `Dispatcher::restrict_quotes_to_loopback`, set by every testkit dispatcher; a runtime switch, not a cargo feature ([`api/quotes.md`](api/quotes.md)) |
|        | Fuzzing                                   | New targets for the SQL row readers and formulas with variables (see "Testing")                   |
|        | Rhai closure bugs                         | Report upstream; the conventions stay until a release fixes them                                  |
|        | `ring` is C                               | Unchanged; SQLite adds a second C library ("Decided", item 3)                                     |
|        | Opening an exported account tree          | Not in v0.4; still open in [`open-questions.md`](open-questions.md)                               |
|        | GnuCash's QIF account map file            | Not in v0.4                                                                                       |

## Interfaces

Each feature above counts as done only once it is reachable through JSON-RPC, MCP and the CLI.

| Status | Interface    | Notes                                                                                                                         |
|:------:|--------------|-------------------------------------------------------------------------------------------------------------------------------|
|        | JSON-RPC API | `scheduledTransaction.*` and `budget.*`; `book.open` takes SQLite files and `sqlite3://`; `book.create` takes `format`. `redAleApi` becomes `0.4.0`, additive over `0.3.0`: the 0.3.0 document is committed beside the 0.2.0 one, and `api_additive.rs` checks against it |
|        | MCP server   | Every new method as a tool. Deletes, `budget.estimate` and `budget.scrubSigns` are destructive; `scheduledTransaction.sinceLastRun`, `scheduledTransaction.occurrences`, `budget.get` and `budget.list` read only |
|        | CLI          | Every new method through `exec` and `call`                                                                                    |

The designs go in `api/scheduled-transactions.md` and `api/budgets.md`, and the SQL Backend's in `sql-fidelity.md`
beside [`file-fidelity.md`](file-fidelity.md).

## Testing

Every feature follows [`testing.md`](testing.md). The oracle reads and writes SQLite books
(`Session("sqlite3://<path>")`). **No test runs Guile** ("Decided", item 12): where a result needs it (formula
functions, the instance model's amounts, the forecast reports), expected values are hand-worked from the 5.17 source.

- **SQLite fixtures.** For every canonical fixture the oracle builds an SQLite copy with the recipe of
  [`notes/sql-backend.md`](notes/sql-backend.md) §10.1: open the XML read-only, begin a `sqlite3://` session with
  `SESSION_NEW_STORE`, swap the data, mark the book dirty through `ctypes`, save, then open once read-only so the
  ISO-dates feature is set, and delete the journal the bindings leave.
- **SQLite round-trips.** A read-only open leaves the file byte for byte as it was; a normal open and close leaves
  every row as it was; an edit and save is opened by GnuCash 5.17 with nothing logged and compared whole, and every
  row red-ale doesn't model is unchanged.
- **One book, two formats.** The XML and SQLite copies of each fixture read to equal Books, apart from what SQLite
  can't hold (deferred instances).
- **Locks.** red-ale refuses a book GnuCash holds, GnuCash refuses one red-ale holds, and red-ale's `gnclock` row goes
  on every exit path, with a test for each error path.
- **Schema versions.** A book with any `versions` row other than 5.17's opens read-only and names the table.
- **Recurrences.** GnuCash's `test_some` and `test_all`, ported, with the oracle's `recurrenceNextInstance` (through
  `ctypes`, no Guile) checking a table of dates.
- **Since Last Run, formulas and the loan.** GnuCash's `test-sx.cpp` and `test-exp-parser.c` cases rebuilt as red-ale
  tests where they need no Guile, and hand-worked cases otherwise; property tests that next occurrences never go
  backwards and that Since Last Run twice on the same day creates nothing; GnuCash 5.17 opening every book Since Last
  Run wrote, and seeing the transactions and schedule state expected.
- **Budgets.** The bindings have no budget functions; oracle tests read budgets back by calling `libgnc-engine.so`
  through `ctypes` ([`notes/engine-budgets.md`](notes/engine-budgets.md) §7).
- **Fuzzing.** New targets for the SQL row readers (an SQLite file is outside input) and for formulas with variables.

## Order of work

1. ✅ **The Mutation walk.**
2. **The glossary and the designs**: the terms below, `api/scheduled-transactions.md`, `api/budgets.md` and
   `sql-fidelity.md`.
3. **Budgets and recurrences in the core model and the XML Backend**, then **scheduled transactions and template
   transactions**, lifted from Residue into the model. Every round-trip test passes unchanged.
4. **Recurrence arithmetic and formulas** in the core.
5. **Budget operations** and **scheduled transaction operations**: CRUD, Since Last Run, from transaction, the loan.
6. **The report library additions**, then **the reports**.
7. **The Backend seam**, with the XML Backend behind it.
8. **`red-ale-sql`**: reading (books open read-only), the lock, writing and saving, the journal and backups.
9. **Interfaces and the audit.**

## Decide before code

### Decided

1. **Order.** Scheduled transactions and budgets first, `red-ale-sql` after, so it maps every table from its first
   line. Both are in v0.4.
2. **Saving SQL books** at `book.save`, in one SQL transaction read back and compared before it commits. Writing
   each Mutation through, as GnuCash does, would change what `book.save` and closing without a save mean for one
   format only.
3. **The SQLite library** is `rusqlite` with `bundled`, so the SQLite version is pinned, not the system's. It compiles
   C, a second C library in the build beside `ring`, recorded as `ring` is ([`architecture.md`](architecture.md),
   "Dependencies"); pure-Rust SQLite engines are not yet ready to trust with a user's books.
4. **Schema versions.** A book whose table rows in `versions` differ from 5.17's, or whose `Gnucash-Resave` row is not
   19920, opens read-only; the `Gnucash` row only has to lie between 19920 and 5.17's, since ordinary edits never update
   it ([`sql-fidelity.md`](sql-fidelity.md)). red-ale never upgrades.
5. **Double slots in SQLite** keep their eight bytes as read and are written back unchanged.
6. **Financial functions** are computed exactly whenever their powers are whole numbers (then `(1+r)^n` is rational),
   rounded half up to 12 significant figures as GnuCash rounds a function's result, then to the SCU: `pmt`, `ipmt`,
   `ppmt`, `futureValue`, `computeInterestIncrement`, `periodic_rate` and `foobar`. A non-whole power is refused with a
   reason. `cpd_*` and `amort_*` are refused too: they round to the cent with Scheme's half-even `round` on doubles,
   which red-ale can't yet reproduce at exact ties ([`notes/engine-scheduled-transactions.md`](notes/engine-scheduled-transactions.md)
   §7.4, H9).
7. **Natural signs.** Budget edits are refused until `budget.scrubSigns`; the feature is set with the first budget and
   unset with the last, as GnuCash does.
8. **Attached documents** are out of v0.4, with a design doc before any version takes them.
9. **Glossary.** New terms enter [`glossary.md`](glossary.md) before code: *Scheduled transaction*, *Schedule*,
   *Recurrence*, *Occurrence*, *Instance* and its states, *Deferred instance*, *Template account*, *Template
   transaction*, *Formula*, *Variable*, *Since Last Run*, *Budget*, *Budget period*, *Budget amount*, *Budget note*,
   *Natural signs*, *SQL book*, *Schema version*. Clashes flagged: *Template transaction* is not *Report template*; a
   *Budget period* comes from the budget's Recurrence and is not the *Accounting period*; a *Schedule* is a scheduled
   transaction's list of Recurrences; GnuCash calls every entity a *QofInstance*, while a scheduled transaction's
   *Instance* is one occurrence of it; GnuCash's code says *SX*, red-ale says *scheduled transaction*.
10. **Method names:** `scheduledTransaction.*` and `budget.*`, in the style of `billTerm.*` and `taxTable.*`.
11. **Since Last Run deviations:** no duplicates after a postponement, failing formulas refused, the read-only
    threshold flagged (above).
12. **No Guile in tests either** (the user's decision): results that need Guile are hand-worked.

## Rows missing from `features.md`

- **Save As between formats.** GnuCash's Save As writes an open book to XML or SQLite. There is no row. Proposed: add
  one under §9 "Formats" and place it once both Backends exist. Converting is safe only for what both formats model,
  since one Backend's Residue cannot be written by the other.

## Done when

- Every row above is ✅ or ❌, and each ✅ row's Ported column in `features.md` says `0.4`.
- An SQLite book GnuCash 5.17 wrote is unchanged byte for byte after a read-only open, and row for row after a normal
  open and close. Opened, edited and saved, GnuCash 5.17 opens it with no warnings and no lost data, and every table,
  column and row red-ale doesn't model is unchanged.
- red-ale never opens for writing an SQLite book GnuCash holds, GnuCash sees red-ale's lock, and red-ale releases it
  on every exit path, error paths included.
- An SQLite book with a `versions` row other than 5.17's opens read-only and says which.
- Every canonical fixture reads to the same Book as XML and as SQLite.
- Every recurrence type gives the occurrences GnuCash's own tests and the oracle expect. Since Last Run creates the
  transactions GnuCash would (dates, amounts, Slots) and advances each schedule as GnuCash does, and run twice on the
  same day it creates nothing.
- Formulas are exact, and the loan assistant's schedules give repayments that match GnuCash's to the SCU in the
  hand-worked cases.
- Budget amounts and notes red-ale wrote are read back by GnuCash 5.17, and every book declares only the features
  GnuCash would have set.
- Every report above gives the figures its expected results hold.
- The memory budgets of [`milestone3.md`](milestone3.md) still hold on the 100,000-transaction book, and a Since Last
  Run creating 5,000 transactions is one Mutation.
- Every JSON-RPC method is an MCP tool with the same name, params schema and hints, and `redAleApi` 0.4.0 is additive
  over 0.3.0, checked by a test.
- v0.1's, v0.2's and v0.3's done-when checks still pass.

## Follow-ups

- **Split order of XML books GnuCash didn't write.** GnuCash sorts each transaction's splits as it loads a book,
  non-negative values first and otherwise in order (`5.17:libgnucash/engine/Transaction.cpp:1508-1510`), for XML and SQL
  alike. red-ale's SQLite reader does the same; its XML reader keeps the file's order, which differs only for files GnuCash
  did not write. A ticket of its own.
- **A saved report configuration whose budget was deleted.** [`reports.md`](reports.md) §6 says the budget option then
  reads as `()`; `report.run` refuses it with `report-option` ("unknown budget") instead, so such a configuration
  can't run.
- **The `sql_file` fuzz target** (mutated SQLite fixtures opened read-only), named in [`sql-fidelity.md`](sql-fidelity.md).
