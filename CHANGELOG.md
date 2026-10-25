# Changelog

All notable changes to red-ale are recorded here. The format follows [Keep a Changelog](https://keepachangelog.com),
and red-ale uses [Semantic Versioning](https://semver.org). The JSON-RPC API has its own version (`redAleApi`), which
`api.schema` reports.

## 0.2.0 - Unreleased

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
