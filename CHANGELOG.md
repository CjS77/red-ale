# Changelog

All notable changes to red-ale are recorded here. The format follows [Keep a Changelog](https://keepachangelog.com),
and red-ale uses [Semantic Versioning](https://semver.org). The JSON-RPC API has its own version (`redAleApi`), which
`api.schema` reports.

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
