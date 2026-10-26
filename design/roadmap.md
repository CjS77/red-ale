# Roadmap to parity

**Status: proposed.** How red-ale gets from v0.3 to feature parity with GnuCash 5.17. v0.1 and v0.2 have their own
milestone files, and v0.3 is partly decided in [`open-questions.md`](open-questions.md); the later versions are
proposals. When a version starts, its milestone file (`milestoneN.md`) takes over from this page for that version.

[`features.md`](features.md) stays the checklist: every feature is a row there, and a version counts as done when its
rows are ✅. This page says which version each open row belongs to, and why in that order.

## Where things stand

v0.2 is done: every row of [`milestone2.md`](milestone2.md) is ✅ and its "Done when" list holds. `features.md` has
183 rows: 94 are ✅ (60 ported in v0.1, 34 in v0.2), 18 are ❌ (won't do), and 71 are open, all placed below.

| Version | Theme                                                             | Open rows | Status                                         |
|---------|-------------------------------------------------------------------|----------:|------------------------------------------------|
| v0.1    | GnuCash ≤ 2.0 features on XML books; JSON-RPC and CLI             |         — | Released ([`milestone1.md`](milestone1.md))    |
| v0.2    | MCP; reports ≤ 1.6 in Rhai; rest of books, transactions, business |         — | Done ([`milestone2.md`](milestone2.md))        |
| v0.3    | Memory and scale; import and export; online quotes                |        11 | Next; [`milestone3.md`](milestone3.md) drafted |
| v0.4    | SQLite books; scheduled transactions and budgets, with reports    |        19 | Proposed                                       |
| v0.5    | Ledgers, statements, analysis and investment reports              |        17 | Proposed                                       |
| v0.6    | The manual; business and tax reports, invoices, PDF export        |        14 | Proposed                                       |
| v0.7    | MySQL and PostgreSQL books, autosave, setup tools                 |         6 | Proposed                                       |
| v1.0    | Parity: every row ✅ or ❌                                        |         4 | Proposed                                       |

## How the order is chosen

1. **Book safety first.** Data red-ale keeps as Residue but cannot edit (scheduled transactions, budgets) and books it
   refuses outright (SQL) come before features that only read. Each one lifts a reason a user's book can't be fully
   used today. A refused SQL book is never touched, so waiting for SQLite costs its users access, not data.
2. **Data before the reports that read it.** Budget reports wait for budgets, the forecast and the scheduled
   transactions summary wait for scheduled transactions.
3. **One engine, many reports.** v0.2 built the report engine and 19 reports on it; v0.5 and v0.6 are mostly
   translation work on it, split so each version is small enough to review.
4. **Common use before niche.** Import comes first; SQLite, which needs no server, follows with scheduled transactions
   and budgets; the server databases and the setup assistants come last.
5. **Every version is safe on its own.** A version ships only with its round-trip and oracle tests passing, and
   every v0.1 rule in `CLAUDE.md` holds throughout.
6. **Every version ships documented.** From v0.6, a method, error code or report counts as done only when its
   Manual page and tested Examples are in, as it already needs its oracle tests ([`manual.md`](manual.md)).

## v0.3: import and export, online quotes

Already decided: [`mission-statement.md`](mission-statement.md) ("Releases"), [`open-questions.md`](open-questions.md)
("Decided for v0.3"), [`milestone2.md`](milestone2.md) ("Out").

| Area               | Rows                                                                                                     |
|--------------------|----------------------------------------------------------------------------------------------------------|
| §7 File import     | QIF, CSV transaction, CSV account, CSV price. OFX/QFX, AqBanking file import and CAMT become ❌        |
| §7 Matching        | Import matcher, Bayesian account matching, Import map editor                                             |
| §7 Export          | CSV export, Export accounts                                                                              |
| §6 Business import | Import invoices and bills, Import customers and vendors                                                  |

### First: memory and scale

The XML Backend holds far more than the Book while a book is open, opened or saved. Measured on synthetic books in
GnuCash's format (2,000 accounts, two or three splits per transaction), through `book.open`, `transaction.post` and
`book.save`:

| Transactions | File (plain) |   Book | Open, at rest | Peak at open | Peak at save | Peak RSS |
|-------------:|-------------:|-------:|--------------:|-------------:|-------------:|---------:|
|      100,000 |       136 MB | 155 MB |        605 MB |       2.4 GB |       2.9 GB |   3.6 GB |
|      200,000 |       272 MB | 310 MB |        1.2 GB |       4.8 GB |       5.8 GB |   7.0 GB |

GnuCash 5.17 opens the 100,000-transaction book in 188 MB. The Book itself is about 1.5 KB per transaction, and
copy-on-write costs about 50 bytes per entity per Mutation; the rest is the Backend. A 4 GB machine stops at about
110,000 transactions today, a size a small business reaches. Bulk import, also in v0.3, makes it worse, so these
three changes come before it:

1. **Keep raw text only where it is needed.** The Residue keeps a parsed copy of every entity's element (about
   4.5 KB per transaction) so unchanged entities are written back byte for byte. Books written by GnuCash 5.17
   already come back byte for byte when regenerated from the Book (the round-trip tests check it), so the Residue
   needs the original text only of entities whose regenerated form differs, and nothing for the rest.
2. **Parse as a stream.** `read_book` builds a DOM of the whole document before translating it, so opening peaks at
   about 17 times the file size. Translating from the tokeniser's events (`quick-xml` already provides them), as
   GnuCash's SAX parser does (`5.17:libgnucash/backend/xml/sixtp.cpp`), keeps the peak near the size of the open
   book.
3. **Stream the save and its check.** `Session::save` serialises the whole file into memory, reads it back into a
   second Book and compares the two. Writing through gzip to the temporary file, and comparing what is read back
   entity by entity as it is parsed, keeps the same guarantee (nothing replaces the old file unless the bytes read
   back as the same Book) without a second copy of the book or of the file.

None of the three changes the file red-ale writes. Every round-trip and oracle test must pass unchanged, and new
tests pin the memory: a fixed-size synthetic book must stay under a budget at rest, at open and at save. With all
three, 4 GB should hold a few million transactions.

That moves the limit to speed. Each Mutation clones the entity maps and scans every transaction for changes, which
took 14 ms at 100,000 transactions and 47 ms at 200,000. That is fine one call at a time, but slow for an importer
posting thousands. `milestone3.md` decides whether importers batch their changes into one Mutation, or Mutations
track what they changed (or use persistent maps) so each one costs only what it touches.

### Also in v0.3

Outside the rows above:

- **Online price quotes** fetched by red-ale's own Rust code, with Finance::Quote's source names and the
  `Finance::Quote` price source string (`open-questions.md`). `features.md` still marks "Online quotes" ❌ because
  red-ale won't run Finance::Quote; that row should gain a note pointing at red-ale's fetcher, or a new row for it.
- **CSV export of the net and category charts**, deferred from v0.2 ([`reports.md`](reports.md) §12).
- **Importing GnuCash's saved report configurations and style sheets**, so users keep the reports they set up in
  GnuCash ([`reports.md`](reports.md) §15). GnuCash keeps them outside the book, as Scheme files in its user data
  directory: `saved-reports-2.8` (falling back to `saved-reports-2.4`, `5.17:gnucash/report/gnc-report.h:35-36`)
  and `stylesheets-2.0` (`5.17:gnucash/report/html-style-sheet.scm:142-143`). The importer reads only the literal
  option forms GnuCash writes (`5.17:gnucash/report/report-core.scm:620-643`) and never evaluates Scheme. Each entry
  becomes a Saved report configuration or Style sheet in red-ale's Configuration.
  - A saved configuration stores only the options that differ from its template's defaults, keyed by the parent
    template's GUID. A configuration whose template red-ale has not translated is reported and skipped.
  - Option values arrive in GnuCash's saved forms: colours as `(r g b a)` lists, dates as `(relative . <name>)`.
    GnuCash writes the accounting-period dates as `start-prev-fin-year` and `end-prev-fin-year`
    (`5.17:libgnucash/engine/gnc-option-date.cpp:311`, `:319`) but reads back only `start-accounting-period` and
    `end-accounting-period` (`5.17:bindings/guile/gnc-optiondb.i:894-897`), so the importer accepts both spellings.
- **Locales.** red-ale formats reports only as GnuCash does under the C locale, the one GnuCash's own report tests
  run under ([`reports.md`](reports.md) §3, §4), so its output differs from that of a user's GnuCash: GnuCash
  prints dates in the `locale` format with the locale's `D_FMT` (`5.17:libgnucash/engine/gnc-date.cpp:61`, `:522`),
  so under en_US the Balance Sheet's date is `12/31/2026` where red-ale prints the C locale's `12/31/26`. v0.3
  makes the locale a setting, with the C locale as the default so existing tests and output are unchanged. A locale
  covers what GnuCash takes from it:
  - the number format: decimal point and grouping (`5.17:libgnucash/core-utils/gnc-locale-utils.c:75-108`). The
    point after a whole amount of a commodity that is not a currency (`5. ACME`) does not depend on the locale:
    5.17 always prints it (`5.17:libgnucash/app-utils/gnc-ui-util.cpp:1253-1297`), while 5.11 and earlier left it
    out (`5.5:libgnucash/app-utils/gnc-ui-util.c:1322`), until commit `5941167132` (Bug 797766, first in 5.12);
  - the `locale` date format, and the ICU interval form for date ranges in titles
    (`5.17:libgnucash/engine/gnc-date.cpp:618-680`);
  - the locale's currency symbol, and the default report currency (`gnc_default_report_currency`,
    `5.17:libgnucash/app-utils/gnc-ui-util.cpp:748-822`);
  - the first day of the week (`weekStart`, `5.17:libgnucash/engine/gnc-date.cpp:195-215`).

[`milestone3.md`](milestone3.md) settles the rest: OFX/QFX, MT940, MT942, DTAUS and CAMT are dropped, since only
LibOFX and AqBanking parse them; the other importers and QIF are translated from GnuCash's code; quote services are
configurable URLs, fetched in the background; and the locale is a Configuration setting, with its data from glibc.

If v0.3 runs long, the memory work ships first as v0.3, and import, export and quotes follow as v0.3.x.

## v0.4: SQLite books, scheduled transactions and budgets

| Area              | Rows                                                                                                                                     |
|-------------------|------------------------------------------------------------------------------------------------------------------------------------------|
| §9 Formats        | SQLite books, ISO dates in SQLite                                                                                                        |
| §4                | Scheduled transactions, Recurrence rules, Formulas and variables, Since Last Run, Schedule from transaction, Mortgage and loan assistant |
| §5                | Budgets, Estimate from history, Budget notes, Natural signs                                                                              |
| §8 Budget reports | Budget Report, Budget Balance Sheet, Budget Income Statement, Budget Flow, Budget Chart                                                  |
| §8 Analysis       | Future scheduled transactions, Balance Forecast                                                                                          |

Why now: scheduled transactions and budgets are Residue today. red-ale keeps them byte for byte but can't create,
edit or run them, so a user who relies on them still needs GnuCash. They are also self-contained in the file format
(template transactions, `gnc:schedxaction`, `gnc:budget`), so they don't disturb the rest of the core.

SQLite books come here rather than in v0.3: nothing in v0.3 depends on them, and with scheduled transactions and
budgets in the model, `red-ale-sql` reads and writes their tables (`schedxactions`, `recurrences`, `budgets`,
`budget_amounts`; `5.17:libgnucash/backend/sql/gnc-schedxaction-sql.cpp:51`, `gnc-recurrence-sql.cpp`,
`gnc-budget-sql.cpp:49-51`) directly instead of keeping them unread and reworking that a version later. Until then red-ale
refuses SQL books before touching them, as v0.1 does.

To settle in `milestone4.md`: `red-ale-sql`'s schema handling (GnuCash's `versions` table and upgrades), the SQL lock
(`gnclock`), the `.log` journal for SQL books (GnuCash's DBI backend keeps one too,
`5.17:libgnucash/backend/dbi/gnc-backend-dbi.cpp:456`), and how an SQL book's rows that red-ale does not model are
kept; formulas are expressions over variables with exact results (GnuCash's `gnc-exp-parser`); Since Last Run
needs "today" and each schedule's last occurrence, which interacts with the auto read-only threshold; the mortgage
assistant is a GUI assistant, ported as one operation as the stock split assistant was.

v0.4 is the largest version on this page. If it runs long, it splits into v0.4 and v0.4.x; `milestone4.md` decides
which half ships first. SQLite first gives SQLite users access sooner; scheduled transactions and budgets first let
`red-ale-sql` map their tables from the start.

## v0.5: ledgers, statements, analysis and investment reports

Every report here reads data red-ale already models, on the v0.2 engine.

| Area                    | Rows                                                                                                                                                                       |
|-------------------------|----------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| §8 Report system        | Multicolumn reports                                                                                                                                                        |
| §8 Financial statements | Transaction Report, Trial Balance, Cash Flow, Equity Statement, General Journal, General Ledger, Reconciliation Report, Register report, Multicolumn Balance Sheet and P&L |
| §8 Analysis and charts  | Income/expense vs day of week, Cash flow chart, Transaction Breakdown                                                                                                      |
| §8 Investment reports   | Investment Portfolio, Advanced Portfolio, Investment Lots, IFRS cost basis                                                                                                 |

The Transaction Report goes first: its engine (`5.17:gnucash/report/trep-engine.scm`) also drives the General
Journal, General Ledger and Reconciliation Report. The Register report runs from a register window in GnuCash; in
red-ale it takes the account and filter as options.

Also in v0.5: checking reports against GnuCash's own output through `gnucash-cli --report run` in a GUI build of the
pinned version, which v0.2 deferred ([`open-questions.md`](open-questions.md), "Reports are tested against expected
results"). By v0.5 there are enough reports for it to pay off.

## v0.6: the manual, business and tax reports, invoices, PDF export

### The manual

Until v0.6, agents and users learn red-ale from `api.schema` (one-line summaries and field descriptions), the MCP
instructions and `red-ale methods`. v0.6 builds the Manual ([`manual.md`](manual.md)) and documents everything from
v0.1 to v0.5 at once:

- the `red-ale-docs` crate, the Manual's common core: Pages generated from `api.schema`, the error table and the
  report registry, completed by hand-written Markdown in `docs/`, with a deterministic offline search;
- `help.topics`, `help.get` and `help.search` through JSON-RPC, and so as MCP tools and through the CLI; `data.help`
  on errors;
- MCP resources (`red-ale://manual/<page-id>`), instructions generated from the Start Page, and richer tool
  descriptions;
- `red-ale help`, and `red-ale manual build` for the static site with `llms.txt`;
- Reference pages and tested Examples for every method, error and report up to v0.6, the Start Page, Concept
  pages, Guides, and the From GnuCash pages for every ported feature;
- **what applies from GnuCash's own documentation**, translated for the API, MCP and the CLI (below).

It changes no file red-ale writes and no existing method's behaviour, so it can run beside the reports. Its Markdown
to HTML path and the PDF export below should share one design. From here on, every version's new methods arrive
with their Pages (rule 6).

The cost of waiting: v0.6 documents five versions' methods at once, and until then a method's doc comments are its
only documentation, so they must stay complete (the workspace's `missing_docs` lint already requires them).

#### From GnuCash's documentation

GnuCash publishes two books (<https://www.gnucash.org/docs.phtml>), whose source is the `Gnucash/gnucash-docs`
repository, tagged `5.17` to match the pin:
- the *Tutorial and Concept Guide* (`C/guide/`): accounting concepts and how-tos;
- the *Help Manual* (`C/manual/`): mostly the GUI, with the parts about transactions, business features, assistants
  and reports in between.

The task is to bring in everything that still applies when there is no GUI, chapter by chapter, and leave out
everything about windows, menus, dialogs, preferences and keyboard use:
- **Concepts** become Concept pages: double entry and the account types, transactions, splits, value and amount,
  reconciliation, currencies and exchange rates, investments, lots and capital gains, depreciation, loans, budgets
  and business features.
- **How-tos** become Guides that do the same task through methods: setting up accounts, the checkbook, credit
  cards, expenses, other assets, loans, investments and their gains, multiple currencies, invoices and payments,
  and the reports.
- **Assistants and dialogs** become From GnuCash pages that name the methods doing their job (the stock split
  assistant is `stock.split`, the Find dialog is `transaction.find`).

A checklist in `docs/` maps each section of both books, at the 5.17 tag, to the Pages that cover it, or marks it
GUI-only, so coverage can be checked and later releases of the books diffed against it. GnuCash's glossary
(`C/guide/gnc-glossary.docbook`) is checked against red-ale's [`glossary.md`](glossary.md) at the same time.

Licence: the books are under the GFDL 1.1 or later, with no Invariant Sections and no Cover Texts
(`C/guide/legal.docbook`, `C/manual/legal.docbook` at `5.17`). red-ale's `docs/` takes the same licence, so Pages
may adapt the books' text; code stays GPL-3.0-or-later. What that asks of the Manual is in
[`manual.md`](manual.md), "Licence".

### Business and tax reports, invoices, PDF export

| Area                | Rows                                                                                                                                                                                        |
|---------------------|---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| §8 Business reports | Customer, vendor and employee reports, Owner report (new), Receivable and payable aging, Customer Summary, Job Report, Printable invoice, Fancy invoice, Easy invoice, Tax invoice, Receipt |
| §8 Tax reports      | US income tax (TXF), German income tax, Income and GST Statement                                                                                                                            |
| §8 Report system    | PDF export                                                                                                                                                                                  |

To settle:

- **eguile.** Tax invoice and Receipt are eguile templates (`5.17:gnucash/report/reports/support/taxinvoice.eguile.scm`,
  `receipt.eguile.scm`): HTML with embedded Scheme. red-ale needs a template form for them (a Rhai template, or a
  report result with a fixed layout) before they can be translated.
- **TXF export.** The US tax report writes a TXF file. That is export, so it follows the v0.3 export design.
- **PDF without a browser.** GnuCash exports PDF by printing its WebKit report view
  (`5.17:gnucash/gnome/gnc-plugin-page-report.cpp:2021`). red-ale needs a pure-Rust way from the report result to
  PDF that keeps the no-floats and same-bytes rules, or the row becomes ❌ with HTML as the export.

## v0.7: server databases, autosave, setup tools

| Area                | Rows                                                                     |
|---------------------|--------------------------------------------------------------------------|
| §9 Formats          | MySQL/MariaDB books, PostgreSQL books                                    |
| §9 Safety           | Autosave                                                                 |
| §10 Setup and tools | New account hierarchy assistant, Account templates, Financial calculator |

The server databases reuse v0.4's `red-ale-sql`; what is new is connecting to a server and its credentials. They
bring the open questions on authentication, remote access and concurrent clients ([`open-questions.md`](open-questions.md),
"Open") to the front, and those need answers before this version starts. Account templates come from GnuCash's
`data/accounts/` trees; check their licence before shipping them. Autosave in a headless server is a timer in the
Configuration, not a GUI preference.

## v1.0: parity

v1.0 is reached when every row of `features.md` is ✅ or ❌. The rows still open after v0.7 need a decision rather
than more work:

- **Online banking: FinTS/HBCI, SEPA transfers, SEPA direct debits.** GnuCash uses AqBanking, a C library. A Rust
  FinTS client is a project of its own, with bank-specific testing red-ale can't do from fixtures. Proposed: ❌,
  unless a maintained Rust FinTS crate exists by then.
- **Interactive charts.** v0.2 chose static SVG ([`open-questions.md`](open-questions.md), "Charts are static SVG").
  Proposed: ❌ for the Chart.js row, keeping static SVG as red-ale's chart feature.

Also for v1.0:

- **The pin.** The parity target stays 5.17 until v1.0. After v1.0, moving to a newer GnuCash release means diffing
  that release against 5.17, adding rows for what changed, and a milestone for the gap.
- **The API.** v1.0 freezes `redAleApi` 1.0.0; until then it changes additively within 0.x.

## Rows missing from `features.md`

5.17 loads three reports that have no row. They should be added before their version starts:

| Report                 | Source                      | Suggested version          |
|------------------------|-----------------------------|----------------------------|
| Balance Sheet (eguile) | `standard/balsheet-eg.scm`  | v0.6, with the eguile work |
| Lot Viewer             | `standard/lot-viewer.scm`   | v0.5                       |
| Sample Graphs          | `example/sample-graphs.scm` | ❌, as Hello World         |

The Dashboard (`standard/dashboard.scm`) is a preset multicolumn report of reports v0.2 ports, so it can sit under
"Multicolumn reports" in v0.5 rather than get its own row.

## Carried forward from v0.2

Known gaps in v0.2, each documented where it happens, to close in v0.3 or record as deviations:

- ✅ The balance-limit check left out sub-accounts in other commodities; it now converts them at the latest price as
  GnuCash does ([`api/accounts.md`](api/accounts.md)). Closed in v0.3.
- GnuCash's business lot scrub is not ported as an operation; tests only check that GnuCash's scrub finds nothing to
  fix in what red-ale writes ([`api/business-posting.md`](api/business-posting.md)). Decided in v0.3: it stays so,
  since business import never runs it; it waits for Check & Repair of books from older GnuCash versions.
- Day boundaries follow zone files exactly, where GnuCash approximates daylight saving with one rule per year
  ([`api.md`](api.md)). Stored dates are unaffected. Decided in v0.3: kept as a deviation, with the reason in `api.md`.
- Nothing pins the API as additive. At the end of v0.2, comparing the v0.1 and v0.2 `api.schema` documents found all
  117 v0.1 methods kept, with their Effects, params, results and error codes, and 29 added; that check was a script,
  not a test. A test against a committed 0.2 schema should keep each later 0.x version additive until v1.0 freezes
  the API.
- Two Rhai limits on closures are worked around by convention in the report library: a closure made inside an
  imported module cannot be called once returned, and a module's methods cannot make closures
  ([`../crates/red-ale-report/README.md`](../crates/red-ale-report/README.md), "Conventions a translator needs").
  The 39 open rows of `features.md` §8 build on the same library from v0.4 on, so settle first whether these are
  Rhai bugs to report and fix upstream or limits of how the engine sets up modules.
