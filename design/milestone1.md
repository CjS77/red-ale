# Milestone 1: red-ale v0.1

**Goal:** every GnuCash feature up to and including 2.0, outside the excluded areas, controllable through the red-ale API,
on XML books written by GnuCash 5.17.

This file tracks progress towards v0.1. Feature names and versions come from [`features.md`](features.md); when a
feature here is done, set its Ported column there to `0.1`.

**Progress: 0 of 60 features, 0 of 2 interfaces.**

| Status  | Meaning                                                                           |
|:-------:|-----------------------------------------------------------------------------------|
| (blank) | Not started                                                                       |
|   🚧    | In progress                                                                       |
|   ✅    | Done: implemented, reachable through the API, round-trip and oracle tests passing |

GnuCash names some features after its dialogs (Security editor, Price editor, Lot viewer, Find transactions). In red-ale
each is a set of API operations that does the same job.

## Scope

**In:** features introduced in GnuCash 2.0 or earlier, from §1 Books and accounts, §2 Transactions, §3 Commodities,
prices and investments, §6 Business and §9 Files and storage of `features.md`. Two later safety features are added (see "Added for
compatibility").

**Out:**

- §4 Scheduled transactions, §5 Budgets, §7 Import and export.
- §8 Reports. They arrive in v0.2, rebuilt on red-ale's own report scripting. Guile is never supported.
- §10 Automation and tools. The GnuCash command line is not ported. red-ale's CLI is a front end for the API instead (see "Interfaces").
- The new account hierarchy assistant and account templates (§10). New books start with an empty account tree.
- Online price quotes (§3), even though they date from before 1.4. They come in a later milestone.
- §11 User interface. It's desktop-only.
- The MCP server. It ships in v0.2, as a thin adapter over the v0.1 JSON-RPC operations.
- SQLite books. They arrive in v0.3. **v0.1 reads and writes XML books only.**
- Everything else introduced after 2.0, including MySQL and PostgreSQL books.

## Compatibility in v0.1

Out of scope never means lost. A v0.1 book must still open in GnuCash 5.17 unchanged.

- **Any XML book written by GnuCash 5.17 opens.** Data from features outside v0.1, such as slots, account colours,
  trading accounts, budgets, scheduled transactions and business documents v0.1 doesn't model, is kept as read and
  written back unchanged.
- **Refuse SQL books.** A SQLite book (a file starting `SQLite format 3`) or a database URI is refused with a clear
  error before anything else happens. v0.1 never opens or locks it, and never writes to it.
- **Open read-only when editing could break the book**, and say why:
    - the book declares a feature in its `features` slot that v0.1 doesn't implement (for example credit notes or num
      field source);
    - the book enables trading accounts;
    - the book sets an auto read-only threshold.
- **Round-trip:** for every fixture, open then save with no edits gives a book GnuCash 5.17 considers identical.

## 1. Books and accounts

### Account structure

| Status | Feature                          | Introduced | Epic | Notes                                         |
|:------:|----------------------------------|:----------:|------|-----------------------------------------------|
|        | Account hierarchy                |   ≤ 1.4    |      |                                               |
|        | Standard account types           |   ≤ 1.4    |      |                                               |
|        | A/Receivable and A/Payable types |    1.8     |      |                                               |
|        | Placeholder accounts             |   1.6.7    |      |                                               |
|        | Hidden accounts                  |    2.0     |      |                                               |
|        | Account codes                    |   ≤ 1.4    |      |                                               |
|        | Account description and notes    |   ≤ 1.4    |      |                                               |
|        | Tax-related flag                 |    1.6     |      |                                               |
|        | US tax (TXF) codes               |    1.6     |      | Codes stored and edited; the TXF report is §8 |
|        | Renumber sub-accounts            |    2.0     |      |                                               |
|        | Delete with move                 |    1.8     |      |                                               |

### Book settings

| Status | Feature           | Introduced | Epic | Notes |
|:------:|-------------------|:----------:|------|-------|
|        | Accounting period |    2.0     |      |       |

## 2. Transactions

### Entry

| Status | Feature                     | Introduced | Epic | Notes                                              |
|:------:|-----------------------------|:----------:|------|----------------------------------------------------|
|        | Double-entry transactions   |   ≤ 1.4    |      |                                                    |
|        | Multi-currency transactions |    1.6     |      |                                                    |
|        | Exchange rates on transfer  |    1.8     |      |                                                    |
|        | Transaction notes           |    1.6     |      |                                                    |
|        | Date entered                |   ≤ 1.4    |      |                                                    |
|        | Split action and memo       |   ≤ 1.4    |      |                                                    |
|        | Void and unvoid             |    1.8     |      | Void status follows the void timestamp, as in 5.17 |
|        | Reverse transaction         |    2.0     |      |                                                    |
|        | Find transactions           |   ≤ 1.4    |      |                                                    |

### Reconciliation

| Status | Feature                       | Introduced | Epic | Notes |
|:------:|-------------------------------|:----------:|------|-------|
|        | Reconcile states              |   ≤ 1.4    |      |       |
|        | Reconcile against a statement |   ≤ 1.4    |      |       |
|        | Last reconcile date           |    1.6     |      |       |

### Data integrity

| Status | Feature         | Introduced | Epic | Notes                                               |
|:------:|-----------------|:----------:|------|-----------------------------------------------------|
|        | Imbalance scrub |    1.6     |      | Match 5.17: zero-value imbalance splits are removed |
|        | Orphan scrub    |    1.6     |      |                                                     |
|        | Check & Repair  |   1.6.3    |      |                                                     |

## 3. Commodities, prices and investments

### Commodities

| Status | Feature                | Introduced | Epic | Notes                                                                   |
|:------:|------------------------|:----------:|------|-------------------------------------------------------------------------|
|        | Commodity table        |    1.6     |      |                                                                         |
|        | ISO 4217 currencies    |    1.6     |      | Use the 5.17 table, including the KRW fraction fix and BGN's retirement |
|        | Security editor        |    1.6     |      |                                                                         |
|        | Euro legacy currencies |   ≤ 1.4    |      |                                                                         |

### Prices

| Status | Feature                    | Introduced | Epic | Notes                                                                  |
|:------:|----------------------------|:----------:|------|------------------------------------------------------------------------|
|        | Price database             |    1.6     |      |                                                                        |
|        | Price editor               |    1.6     |      |                                                                        |
|        | Remove old prices          |    2.0     |      |                                                                        |
|        | Quote source and time zone |    1.6     |      | Stored and edited only; fetching quotes is later. Keep unknown sources |

### Investments

| Status | Feature               | Introduced | Epic | Notes                                           |
|:------:|-----------------------|:----------:|------|-------------------------------------------------|
|        | Lots                  |    1.8     |      |                                                 |
|        | Lot viewer            |    2.0     |      |                                                 |
|        | Capital gains         |    2.0     |      | Lot repair skips stock-split splits, as in 5.17 |
|        | Stock split assistant |    1.6     |      |                                                 |

## 6. Business

### Contacts

| Status | Feature   | Introduced | Epic | Notes |
|:------:|-----------|:----------:|------|-------|
|        | Customers |    1.8     |      |       |
|        | Vendors   |    1.8     |      |       |
|        | Employees |    1.8     |      |       |
|        | Jobs      |    1.8     |      |       |

### Documents

| Status | Feature            | Introduced | Epic | Notes                                       |
|:------:|--------------------|:----------:|------|---------------------------------------------|
|        | Invoices and bills |    1.8     |      | Includes assigning the next document number |
|        | Expense vouchers   |   1.8.2    |      |                                             |
|        | Orders             |    1.8     |      |                                             |
|        | Tax tables         |    1.8     |      |                                             |
|        | Billing terms      |    1.8     |      |                                             |
|        | Post to accounts   |    1.8     |      |                                             |
|        | Unpost             |    1.8     |      |                                             |

### Payments

| Status | Feature         | Introduced | Epic | Notes |
|:------:|-----------------|:----------:|------|-------|
|        | Process payment |    1.8     |      |       |
|        | Due reminders   |    2.0     |      |       |

## 9. Files and storage

### Formats

| Status | Feature         | Introduced | Epic | Notes                       |
|:------:|-----------------|:----------:|------|-----------------------------|
|        | XML file format |    1.6     |      | Read and write as 5.17 does |
|        | Compressed XML  |    1.8     |      |                             |

### Safety

| Status | Feature               | Introduced | Epic | Notes                                            |
|:------:|-----------------------|:----------:|------|--------------------------------------------------|
|        | Book locking          |   ≤ 1.4    |      | `.LCK` file; always released, including on error |
|        | Transaction journal   |   ≤ 1.4    |      |                                                  |
|        | Replay journal        |   1.8.5    |      |                                                  |
|        | Backups and retention |    1.8     |      |                                                  |

### Added for compatibility

These come from after 2.0, but v0.1 can't open 5.17 books safely without them.

| Status | Feature        | Introduced | Epic | Notes                                                            |
|:------:|----------------|:----------:|------|------------------------------------------------------------------|
|        | Book features  |    2.6     |      | Read the `features` slot; trigger read-only for unknown features |
|        | Open read-only |    2.6     |      | Used for every read-only trigger above                           |

## Interfaces

These are red-ale's own work, not GnuCash features. Each feature above counts as done only once both reach it.

| Status | Interface    | Notes                                                                                                    |
|:------:|--------------|----------------------------------------------------------------------------------------------------------|
|        | JSON-RPC API | Every v0.1 feature as a JSON-RPC 2.0 method, with a versioned schema the v0.2 MCP tools reuse            |
|        | CLI          | A front end for the API, so every v0.1 feature can be scripted from a shell. Not a port of `gnucash-cli` |

## Done when

- Every row above is ✅, and each one's Ported column in `features.md` says `0.1`.
- Round-trip and oracle tests pass for every fixture, including books written by GnuCash 5.17 that use features outside
  v0.1.
- Every read-only trigger, and the refusal of SQL books, has a test.
