# Changelog

All notable changes to red-ale are recorded here. The format follows [Keep a Changelog](https://keepachangelog.com),
and red-ale uses [Semantic Versioning](https://semver.org). The JSON-RPC API has its own version (`redAleApi`), which
`api.schema` reports.

## 0.6.3 - 2026-10-30

`redAleApi` is unchanged at 0.6.0.

### Fixed

- The Manual's site asked for its images at root-absolute `/assets/…`, so they 404ed on the GitHub Pages project site
  at `/red-ale/`. `red-ale-manual build` gains a `--base` option, the deploy passes the repository's name, and the
  images are now served from `/red-ale/assets/…`.

## 0.6.2 - 2026-10-30

`redAleApi` is unchanged at 0.6.0.

### Changed

- `red-ale-manual build`, the Manual's static site, is a binary of its own instead of `red-ale manual build`: it is
  built from a checked-out source tree, where the landing page and the images are, and an installed `red-ale` has
  neither.
- The Manual's site now opens on the landing page, and every Manual page wears the landing page's stylesheet, script,
  wordmark and footer. The landing page's version and release status are filled in when it is built, so it can no
  longer show a stale one.
- The landing page's Takeaways are followed by a Fine print section with the software's disclaimers.

## 0.6.1 - 2026-10-29

The first tagged release of 0.6: it ships everything listed under 0.6.0. `redAleApi` is unchanged at 0.6.0.

### Changed

- The release and Manual workflows use `actions/checkout` v7.0.1 and `actions/upload-artifact` v7.0.2, which run on
  Node 24; every action is pinned by commit SHA, and the Linux jobs run on `ubuntu-24.04` instead of `ubuntu-latest`.

## 0.6.0 - Unreleased

Milestone 6 ([`design/milestone6.md`](design/milestone6.md)): the Manual, the Printable Invoice and the Dashboard.
`redAleApi` is 0.6.0, additive over 0.5.0.

### Added

- The Manual ([`design/manual.md`](design/manual.md)), one set of Pages for every door:
  - Reference pages for every method, error code and report template, each with Examples that run as tests;
  - Concept pages, Guides and From GnuCash pages, adapted from GnuCash 5.17's *Tutorial and Concept Guide* and
    *Help Manual* under the GFDL (`docs/legal/`), and a Guide to writing your own report;
  - JSON-RPC: `help.topics`, `help.get` and `help.search`; every error carries `data.help`, the Page that explains
    it, and `api.schema` names each method's Page;
  - MCP: every Page as a resource (`red-ale://manual/<page-id>`), server instructions generated from the Start
    page, and longer tool descriptions (`--tool-descriptions short` for the old ones);
  - CLI: `red-ale help <page or method>`, `red-ale help --search <words>`, `red-ale methods` with Page IDs, and
    `red-ale manual build`, which writes the Manual as a static site, published to GitHub Pages on each release.
- The Printable Invoice, translated from GnuCash's `invoice.scm`, run with `report.run` and an Invoice Number.
  Reports gain the `invoice` and `image` option types, `numberFormat: "exact"`, `class`, `css` and `image` blocks,
  and report scripts can read invoices, entries, owners, orders, billing terms and book options.
- Business book options (company name, address, contact, tax number and the rest) and the Default Invoice Report,
  through `book.options` and `book.setOptions`.
- The Dashboard, GnuCash's Multicolumn View of four charts and the Account Summary, as a built-in read-only saved
  configuration: listed by `report.list` with `readOnly: true`, run by name or GUID, copied with
  `report.saveConfiguration`.

### Changed

- A report script that defines a function with the name of one of the report engine's functions is refused when it
  loads (`report-load`), instead of silently replacing it.
- Opening a MySQL or PostgreSQL book now says red-ale opens XML and SQLite files only, and to save it as one of those
  in GnuCash first: server books are not planned ([`design/roadmap.md`](design/roadmap.md)).

### Fixed

- Saving a book in which a transaction and a template transaction share a GUID is refused (`invalid`,
  `data.reason: "shared-guid"`) instead of writing a book GnuCash would misread.
- `help.search` ranks a method's Page first for the words of its name: "open a book" finds `book.open` before the
  `book-already-open` error.
- Import and Since Last Run proposal IDs count from 1 for each server (`imp-1`, …) and still never repeat while it
  runs.
- `period.dates` with no book open takes today from the server's clock, as it does with a book open.
- A book's `.log` journal is named by the session's clock and time zone, like everything else the session dates.
- `reconcile.setState` takes a date (`YYYY-MM-DD`), stamped at 23:59:59 local time as GnuCash's reconcile window
  stamps it, as well as a timestamp.
- `invoice.dueDate` takes a billing term by name as well as by GUID, as every other method does.
- `import.preview`'s `baseAccount` takes the place of an inline layout's base account. Before, a layout without one
  parsed as if none were given, and the preview failed.

### Not supported

- PDF export: export the report as HTML and convert it (the Guide `guide/report-to-pdf`).
- MySQL and PostgreSQL books.
- The tax reports, and GnuCash's business reports other than the Printable Invoice and the Dashboard: write them as
  a User report (`guide/write-your-own-report`).

### Differences from GnuCash

- The Dashboard is a saved configuration under the menu item's GUID, where GnuCash builds a new Multicolumn View at
  each click; a user changes a copy of it.
- An invoice in a report's options summary is its GUID, where GnuCash prints a pointer that changes at every run.

The pages in [`design/api/`](design/api) and [`design/differences-from-gnucash.md`](design/differences-from-gnucash.md)
list every difference in detail.

### Dependency updates

- pulldown-cmark: (none) => 0.13
- rhai: 1.27.0 => 1.27.0 with the `metadata` feature in `red-ale-report` (before, in its tests only)

## 0.5.0 - Unreleased

Milestone 5 ([`design/milestone5.md`](design/milestone5.md)): ledgers, statements, analysis and investment reports,
the Multicolumn View, and distribution. `redAleApi` is 0.5.0, additive over 0.4.0.

### Added

- `red-ale setup` registers red-ale's MCP server (`red-ale mcp`, with this binary's absolute path) with the agent
  harnesses installed on the machine, and `red-ale setup --remove` takes it out again
  ([`design/setup.md`](design/setup.md)). It shows its plan and asks before changing anything (`--yes` to skip the
  question, `--dry-run` to see each file's exact change); `--list` shows what is installed and set up.
  - Claude Code, Codex, GitHub Copilot CLI, Gemini CLI and Devin (when `devin` is on `PATH`) are set up through
    their own `mcp add` and `mcp remove` commands.
  - opencode, Claude Desktop, Cursor, VS Code, Antigravity, Devin (otherwise) and Zed have their config files
    edited. Only the `red-ale` entry changes: comments, other servers, settings and layout are kept, the file is
    backed up first and replaced atomically, and a file that doesn't parse is left alone. Claude Desktop is
    changed only while it isn't running.
- `install.sh`, attached to each release, installs red-ale at `~/.local/bin/red-ale` after checking the archive's
  SHA-256, then runs `red-ale setup`:
  `curl -fsSL https://github.com/CjS77/red-ale/releases/latest/download/install.sh | sh`.
- Reports translated from GnuCash 5.17, with its options and figures, on XML and SQLite books:
  - ledgers: the Transaction Report (on a library translation of GnuCash's transaction report engine, with its
    filters, sorts, subtotals and Subtotal Table), General Ledger, Reconciliation Report and General Journal;
  - statements: Trial Balance (with the work sheet), Cash Flow, Equity Statement, and the Multicolumn Balance Sheet
    and Income Statement;
  - analysis: Transaction Breakdown, Income and Expenses vs. Day of Week, and the Cash Flow Barchart;
  - investments: Investment Portfolio, Advanced Portfolio (average, FIFO and LIFO basis), Investment Lots and IFRS
    cost basis, computed exactly; rates are rounded half away from zero, and Investment Lots' annual growth rate,
    a fractional power, is computed in fixed point to 10⁻¹²;
  - the Multicolumn View, which lays other reports out in a grid: its children are saved configurations, or templates
    with options, named in its `report-list` option, and each runs in its cell.
- `report.importConfigurations` imports saved Multicolumn Views with their embedded reports.
- Report results gain, additively (`redAleApi` 0.5.0): links to splits, transactions, invoices, prices and document
  links; multi-link cells; multi-row column headers; date ranges in date cells; list and grid blocks; row shading
  and warning rows; GnuCash's share display and half-up rounding on number cells; CSV export errors.

### Changed

- Every red-ale process started without `--config` (`exec`, `serve`, `mcp`) now keeps its settings in one default
  Configuration file, shared by them all: `~/.config/red-ale/config.json` on Linux (or under `$XDG_CONFIG_HOME`),
  `~/Library/Application Support/red-ale/config.json` on macOS, `%APPDATA%\red-ale\config.json` on Windows. Before,
  such a process kept its settings in memory, and an agent's `config.set` was lost when its session ended.
  `--no-config` keeps the old behaviour: memory only, no file read or written.
- Changes to the Configuration from several processes at once are all kept: each change locks `<file>.lock` while it
  reads and writes the file.

### Fixed

- Sorting a book's splits could panic when the book uses split actions for numbers, or with numbers such as `-3`
  and `0`: GnuCash's orders are not total orders. red-ale's are now, agreeing with GnuCash's wherever GnuCash's is
  consistent.
- An XML book whose splits were not in GnuCash's order is read as GnuCash reads it: each transaction's splits sorted,
  non-negative values first. Books GnuCash wrote are unchanged.
- An SQLite book whose `books` row names a template root with no `accounts` row takes its first scheduled transaction
  as GnuCash does, instead of refusing the save.
- A report script's function could silently replace a library function of the same name and arity. red-ale now
  refuses such a report when it loads.

## 0.4.0 - Unreleased

Milestone 4 ([`design/milestone4.md`](design/milestone4.md)): SQLite books, scheduled transactions and budgets, and
their reports. `redAleApi` is 0.4.0, additive over 0.3.0.

### Added

#### SQLite books

- `book.open` opens GnuCash's SQLite books, by path or `sqlite3://`, and `book.save` writes them: only the rows that
  changed, in one SQL transaction, each row read back before the commit, everything rolled back on any failure. Rows,
  tables and columns red-ale doesn't model are kept. GnuCash 5.17 opens every fixture red-ale edited and saved with
  nothing logged and nothing lost.
- red-ale takes and releases GnuCash's `gnclock` lock row, refuses a book GnuCash holds, and refuses to save when
  another program wrote the book or broke the lock (`book-changed`).
- A timestamped backup is made before the first save that writes, and the `.log` journal replays as for XML books.
- `book.create` makes a new SQLite book with GnuCash 5.17's schema (`format: "sqlite"`). `book.info` reports the
  `format` and the `versions` rows.
- A book whose `versions` rows are not 5.17's opens read-only and names the table; red-ale never upgrades a schema.

#### Scheduled transactions

- `scheduledTransaction.create`, `update`, `delete`, `get` and `list`, with every recurrence type GnuCash has and
  template transactions whose amounts may be formulas with variables.
- `scheduledTransaction.occurrences`: the next dates of a schedule, saved or not.
- Since Last Run as a proposal: `scheduledTransaction.sinceLastRun` lists what is due, the caller chooses, and
  `scheduledTransaction.apply` creates the transactions and advances the schedules as one change.
- `scheduledTransaction.fromTransaction` and `scheduledTransaction.loan`, GnuCash's Schedule from Transaction dialog
  and Mortgage and Loan Repayment assistant as one call each.
- Formulas are exact, including GnuCash's financial functions (`pmt`, `ipmt`, `ppmt` and others).

#### Budgets

- `budget.create`, `update`, `copy`, `delete`, `get` and `list`; `budget.setAmounts` and `budget.setNotes`.
- `budget.estimate`, GnuCash's Estimate from past transactions, and `budget.scrubSigns`, its natural-signs scrub.
- The Book option for the default budget.

#### Reports

- Translated from GnuCash 5.17: the Budget Report, Budget Balance Sheet, Budget Income Statement, Budget Profit &
  Loss, Budget Flow, Budget Chart, Future Scheduled Transactions Summary and Balance Forecast.
- The report library reads budgets and the scheduled transactions' cash flow; chart series can set GnuCash's
  `pointRadius`, and tables have footnotes.

#### Interfaces

- Every new method is an MCP tool with the same name, params schema and hints, and reachable from the CLI.
- New error codes: 1013 `book-changed` and 6000 `formula`.

### Fixed

- A lone parenthesised number in a business import file, such as `(5)`, now reads as -5, as GnuCash reads it.
- A saved report configuration whose budget was deleted runs with no budget, as in GnuCash, instead of being refused.

### Not supported

- GnuCash's `cpd_*` and `amort_*` formula functions: they need non-integer powers, which can't be exact.
- MySQL and PostgreSQL books (planned for v0.7).

### Differences from GnuCash

- Since Last Run runs only when asked, never on open, and applies whole or not at all. An instance after a
  postponed one is proposed once, not on every run, and a formula that fails refuses the apply instead of creating
  a transaction with 0.
- Postponing an instance in an SQLite book is refused: GnuCash's SQL backend accepts it and loses it on the next
  load.
- Formulas never pass through floating point. Division by zero and overflow are refused, where GnuCash uses 0.
- The natural-signs scrub runs only through `budget.scrubSigns`; budget edits are refused until it has run on a book
  that needs it. GnuCash runs it from the GUI after every open.
- The loan assistant doesn't copy the defects recorded in `design/notes/engine-scheduled-transactions.md` §6.3, and
  writes formula numbers without grouping so they read the same in every locale.

The pages in [`design/api/`](design/api) list every difference in detail.

### Dependency updates

- rusqlite: (none) => 0.40.2
- rustix: (none) => 1.1.5

## 0.3.0 - Unreleased

Milestone 3 ([`design/milestone3.md`](design/milestone3.md)): memory and scale, import and export, online price
quotes, locales and the Configuration API. `redAleApi` is 0.3.0, additive over 0.2.0.

### Added

#### Memory and scale

- Books of a few hundred thousand transactions open, change and save in little more memory than the book itself:
  the file is read and the save checked as a stream, and original text is kept only where red-ale would write an
  entity differently. On a 100,000-transaction book the peak resident set fell from 3.9 GB to about 200 MB. Nothing
  red-ale writes changes.

#### Import

- `import.preview` and `import.apply`: an import is previewed as an Import proposal (each transaction with the action
  the matcher chose, its candidate matches and scores, its accounts, any imbalance, and whether it is read-only by
  date), changed by the caller, then applied whole as one change, or not at all. `import.discard` drops a proposal.
- CSV transaction import with GnuCash's column types, date and number formats and saved layouts; QIF import,
  translated from GnuCash's Scheme, with its own duplicate finder and account map; CSV account and price import;
  invoices, bills, customers and vendors from GnuCash's business CSV formats.
- GnuCash's import matcher, with its scores and thresholds as settings, and Bayesian account matching that learns
  in GnuCash's own format, so GnuCash and red-ale keep learning from each other's imports.
- `importMap.list` and `importMap.delete`, GnuCash's import map editor.
- Import files arrive in the request with their encoding; the CLI reads them on its side (`--file`, `--encoding`).

#### Export

- `export.csvTree` and `export.csvTransactions`: GnuCash's CSV exports of the account tree and of transactions, in
  the simple and complex layouts. The CLI writes them to a file on its side (`--output`).
- `export.accounts`: a new book with the account tree and commodities and no transactions, as GnuCash's Export
  Accounts writes it.
- `report.exportCsv`: the CSV export of the net worth and category charts.

#### Online quotes

- `price.fetchQuotes` and `price.fetchStatus`: prices and exchange rates fetched in the background, by red-ale's own
  Rust code, from Quote services set in the Configuration (Frankfurter, Alpha Vantage, Twelve Data, Finnhub, Yahoo's
  chart endpoint, or any JSON service). Prices are stored as GnuCash stores Finance::Quote's, and other methods are
  answered while a fetch runs. `red-ale exec price.fetchQuotes` waits for the fetch and exits 2 if a commodity got no
  price, for use from cron.
- Network access is off until the Configuration turns it on. Every URL is a setting; a key is bound to its service's
  host and never appears in a result, log, journal or book.

#### Locales and reports

- A `locale` setting, and a `locale` param on report runs: numbers, dates, day names, currency symbols and the
  first day of the week follow the locale, from glibc's locale data (74 locales). The default stays the C locale,
  under which every report gives the same bytes as in 0.2.
- `report.importConfigurations` and `report.importStyleSheets` read GnuCash's saved report configurations and style
  sheets, as data, never running Scheme.

#### Configuration

- `config.schema`, `config.get` and `config.set` read and change every setting by path, atomically, with no book
  open. Keys are shown only as set or unset. Over MCP, `config.set` refuses keys and the network switch.

#### Interfaces

- Every new method is an MCP tool with the same name, params schema and hints. The JSON-RPC and MCP request size
  limits are settings (64 MiB by default) so a few years of statements fit in one request.

### Fixed

- A book whose `gnc:book` version attribute held an escaped character could not be saved: the attribute was written
  back unescaped and the save's own check refused the file.
- `red-ale serve` and `red-ale mcp` no longer exit with a panic when nobody reads their standard error at shutdown.

### Not supported

- OFX/QFX, MT940, MT942, DTAUS and CAMT import: GnuCash hands these to the C libraries LibOFX and AqBanking, so there
  is no GnuCash code to translate. Banks that offer them nearly always offer CSV.
- Locale collation: names still sort bytewise under every locale.

### Differences from GnuCash

- An import is one proposal, applied whole; GnuCash applies rows one by one from its window. Learning happens only
  when an import is applied.
- An import that leaves a transaction unbalanced gets an `Imbalance-<currency>` split as in GnuCash, and the preview
  and the result say so for each one. Imports ignore the auto read-only threshold as GnuCash's do, and the preview
  flags each transaction dated before it.
- Rows GnuCash would import wrongly or drop silently (unparseable numbers, invalid dates, missing parents) are error
  rows the caller sees, and `import.apply` needs `skipErrors` to leave them out. Bytes invalid in the file's
  encoding are an error, where GnuCash drops them.
- The matcher compares amounts exactly, where GnuCash uses doubles with a tolerance.
- A Quote fetch reports commodities it can't fetch, asks a source's services in turn, times out each request, and
  exits non-zero from the CLI when a price is missing; GnuCash leaves such commodities out silently and exits 0.
- Export Accounts refuses an existing file, where GnuCash asks and overwrites, and `book.open` does not yet open
  the file it writes (GnuCash does).
- A CSV export imported back gives the same transactions, except text with a `"` at its start or end or beside a
  comma, which GnuCash's CSV tokenizer also misreads; red-ale reads it as GnuCash does.

The pages in [`design/api/`](design/api) list every difference in detail.

### Dependency updates

- base64: (none) => 0.22
- encoding_rs: (none) => 0.8
- http-body-util: (none) => 0.1.3
- hyper: (none) => 1.5
- hyper-rustls: (none) => 0.27
- hyper-util: (none) => 0.1.19
- icu_calendar: (none) => 2.4
- icu_datetime: (none) => =2.4.0
- icu_locale_core: (none) => 2.4
- icu_normalizer: (none) => 2.4
- peak_alloc: (none) => 0.3.0
- rustls: (none) => 0.23
- rustls-native-certs: (none) => 0.8
- writeable: (none) => 0.6

## 0.2.0 - 2026-10-26

### Added

#### Interfaces

- `red-ale mcp`, an MCP server over standard input and output for AI agents. Every JSON-RPC method is a tool with the
  same name and params schema, marked read-only or destructive from the method's effect. Errors come back with
  red-ale's error codes.

## 0.1.0 - Unreleased

The first release: milestone 1. It covers every GnuCash feature introduced up to version 2.0 in books and accounts,
transactions, commodities and prices, business, and file storage. It works on XML books written by GnuCash 5.17, and
every feature is reachable through the JSON-RPC API and the command line.

### Added

#### Books and accounts

- Account hierarchy, with GnuCash's standard account types, including A/Receivable and A/Payable.
- Placeholder and hidden accounts, account codes, descriptions and notes.
- The tax-related flag and US tax (TXF) codes. The codes are stored and edited; the TXF report itself comes with
  reports.
- Renumbering an account's sub-accounts.
- Deleting an account while moving its transactions and sub-accounts elsewhere.
- The accounting period and fiscal year end, kept in red-ale's own configuration as GnuCash keeps them in its
  preferences.

#### Transactions

- Double-entry transactions, in any currency, refused unless they balance exactly.
- Transfers between accounts in different commodities, with the exchange rate optionally recorded as a price.
- Transaction notes, date entered, and each split's action and memo.
- Voiding and unvoiding, and reversing transactions.
- Finding transactions with the same criteria as GnuCash's Find dialog.
- Reconcile states, reconciling an account against a statement (including postponing a reconciliation), and the last
  reconcile date.
- The imbalance and orphan scrubs, and Check & Repair over an account, a sub-tree or the whole book.

#### Commodities, prices and investments

- The commodity table and the security editor's operations.
- GnuCash 5.17's table of ISO 4217 currencies, and the fixed rates of the euro's legacy currencies.
- The price database: adding, editing and deleting prices with GnuCash's same-day rules, and removing old prices.
- Quote sources and time zones, stored and edited. Fetching online quotes comes in a later release.
- Lots, with a way to inspect and edit them as GnuCash's lot viewer does.
- Capital gains, worked out per lot and recorded as GnuCash records them.
- Stock splits, with an optional price and cash in lieu, as GnuCash's stock split assistant records them.

#### Business

- Customers, vendors, employees and jobs, with IDs from the book's counters.
- Invoices, bills and expense vouchers, with their entries and the next document number.
- Orders, tax tables and billing terms.
- Posting documents to accounts, and unposting them.
- Processing payments against open documents, and due reminders for bills and invoices.

#### Files and storage

- Reading and writing GnuCash XML books, plain or gzip-compressed. A book written by GnuCash 5.17 and saved by red-ale
  without changes comes back byte for byte.
- GnuCash's `.LCK` lock, always released on exit, including after errors.
- GnuCash's `.log` transaction journal, and replaying a journal to recover unsaved changes.
- Timestamped backups before every save, and deleting old backups and journals after a retention period (30 days by
  default).
- Reading the book's declared features, and opening a book read-only when editing it could damage it: when it uses a
  feature red-ale doesn't implement yet (such as credit notes), trading accounts, an auto read-only threshold, or
  content red-ale doesn't recognise. Any book can also be opened read-only on request.
- Data red-ale doesn't handle yet, such as scheduled transactions and budgets, is kept and written back unchanged.

#### Interfaces

- A JSON-RPC 2.0 API with a versioned JSON Schema (`redAleApi` 0.1.0), stable error codes, and exact amounts that are
  never sent as JSON numbers.
- `red-ale serve`, an HTTP server on localhost.
- `red-ale exec` and `red-ale call`, to run methods from the command line, singly or from a script.

### Not yet supported

- SQLite and other database books are refused, unopened. SQLite support is planned for 0.3.
- Editing scheduled transactions and budgets; import and export; reports (planned for 0.2); online price quotes.
- The MCP server for AI agents (planned for 0.2).

### Differences from GnuCash

- An unbalanced transaction is refused, where GnuCash would add an `Imbalance-<currency>` split. The imbalance scrub
  does GnuCash's repair when asked.
- Accounts red-ale creates, such as `Imbalance-USD` or `Orphaned Gains-USD`, get GnuCash's English names.
- Prices are compared by UTC day, where GnuCash uses the computer's local time zone.

The pages in [`design/api/`](design/api) list every difference in detail.
