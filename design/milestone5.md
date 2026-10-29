# Milestone 5: red-ale v0.5

**Status: draft.** The scope comes from [`roadmap.md`](roadmap.md) ("v0.5"), plus the Configuration change, which is
done. "Decide before code" lists what is settled and what is still proposed; the proposed items are settled, and
research notes written, before the reports that depend on them.

**Goal:** red-ale runs GnuCash's ledgers, financial statements, analysis charts and investment reports, translated
from their Scheme into Rhai, with GnuCash's options and figures, on XML and SQLite books alike. Every red-ale process
shares one Configuration file by default.

This file tracks progress towards v0.5. Feature names and versions come from [`features.md`](features.md); when a
feature here is done, set its Ported column there to `0.5`.

## Scope

The 17 open rows the roadmap places in v0.5 (the Register report among them won't be done, "Decided", item 5):

| Area                    | Rows                                                                                                                                                                       |
|-------------------------|----------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| §8 Report system        | Multicolumn reports                                                                                                                                                        |
| §8 Financial statements | Transaction Report, Trial Balance, Cash Flow, Equity Statement, General Journal, General Ledger, Reconciliation Report, Register report, Multicolumn Balance Sheet and P&L |
| §8 Analysis and charts  | Income/expense vs day of week, Cash flow chart, Transaction Breakdown                                                                                                      |
| §8 Investment reports   | Investment Portfolio, Advanced Portfolio, Investment Lots, IFRS cost basis                                                                                                 |

Also in v0.5:

- the default Configuration file ("The Configuration", done);
- the v0.4 follow-ups taken up here ("Carried forward from v0.4");
- distribution, that is the release binaries, `red-ale setup` and the install scripts ("Distribution";  "Decide before code", item 1).

Why these together: every report here reads data red-ale already models, on the v0.2 report engine, and none needs a
change to what red-ale writes. The Transaction Report goes first: its engine (`5.17:gnucash/report/trep-engine.scm`,
2,645 lines) also drives the General Ledger and the Reconciliation Report (`gnc:trep-options-generator` and
`gnc:trep-renderer`), so three reports rest on it. The General Journal is the Register report's renderer in journal
mode over the whole Book (`standard/general-journal.scm:41-118`), and the Transaction Breakdown builds its own table.
Research: [`notes/report-transaction-engine.md`](notes/report-transaction-engine.md),
[`notes/report-statements-charts.md`](notes/report-statements-charts.md) and
[`notes/report-investments.md`](notes/report-investments.md).

**Out:**

- **Tax and business reports, invoices, PDF export** (v0.6), and the Manual (v0.6).
- **Windows builds and installation:** deferred to a later version. `red-ale-service` uses Unix-only file APIs
  (`files.rs`: file modes and `chown`; `lock.rs`: the `.LCK` file's mode), so red-ale doesn't build on Windows today;
  porting them, with Windows' own owner-only permissions, comes first. `red-ale setup` keeps the harnesses' Windows
  paths in its spec for then.
- **Interactive charts:** a v1.0 decision, proposed ❌ ([`roadmap.md`](roadmap.md), "v1.0").
- **Reports GnuCash 5.17 ships that `features.md` had no row for**: the Lot Viewer and the Dashboard (v0.6), the
  Balance Sheet (eguile) and Sample Graphs (❌); "Decided", item 7.

## Compatibility in v0.5

Every v0.1 to v0.4 rule holds. Reports only read: no report changes a Book or makes it dirty, on XML or SQLite. Besides
red-ale's own Configuration file ("The Configuration"), one thing is written differently, and only in XML books GnuCash
didn't write: a transaction whose splits are out of GnuCash's order is written in that order, as GnuCash writes it
after loading the book ("Carried forward from v0.4"). Books GnuCash wrote still round-trip byte for byte.

## The Configuration

| Status | Item                                     | Notes                                                                                                                                                                                                                                                                                                                      |
|:------:|------------------------------------------|----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
|   ✅   | The default Configuration file           | Without `--config`, every process (`exec`, `serve`, `mcp`) uses one file: `~/.config/red-ale/config.json` on Linux (respecting `XDG_CONFIG_HOME`), `~/Library/Application Support/red-ale/config.json` on macOS, `%APPDATA%\red-ale\config.json` on Windows ([`api/configuration.md`](api/configuration.md), "Which file") |
|   ✅   | `--no-config`                            | Memory only, for that process: no Configuration file is read or written. Refused together with `--config`                                                                                                                                                                                                                  |
|   ✅   | A lock around every change               | Each change holds `<file>.lock` from reading the Configuration to writing it back, so two processes changing settings at once keep both. `ConfigStore::save` takes the lock, so a change can't skip it                                                                                                                     |
|   ✅   | The first change makes the directory     | As well as the file                                                                                                                                                                                                                                                                                                        |
|   ✅   | Tests never touch the real Configuration | The binary's tests point `HOME`, `XDG_CONFIG_HOME` and `APPDATA` at a temporary directory                                                                                                                                                                                                                                  |

This corrects a v0.3 design mistake: a process started without `--config` kept its settings in memory, so an agent's
`config.set` was lost when its session ended. Evidence: `red-ale/tests/cli.rs`,
`without_config_every_process_shares_the_default_file` and `no_config_reads_and_writes_no_file`;
`red-ale-service::config` tests `the_default_path_follows_each_platform`, `a_first_save_makes_the_directory` and
`concurrent_changes_are_all_kept`.

## Reports

Translated from GnuCash 5.17's Scheme into Rhai, on the v0.2 engine, as [`reports.md`](reports.md) sets out. Source
files are under `5.17:gnucash/report/reports/`; lines are the file's length at the pin.

| Status | Report                            | Source (GUID)                                                                                                      |      Lines | GnuCash's test               |
|:------:|-----------------------------------|--------------------------------------------------------------------------------------------------------------------|-----------:|------------------------------|
|   ✅   | Transaction Report                | `standard/transaction.scm` (`2fe3b9833af044abb929a88d5a59620f`), on `trep-engine.scm`                              | 36 + 2,645 | `test-transaction.scm`       |
|   ✅   | General Journal                   | `standard/general-journal.scm` (`25455562bd234dd0b048ecc5a8af9e43`), on `register.scm`'s renderer ("Decided", 5) |        125 |                              |
|   ✅   | General Ledger                    | `standard/general-ledger.scm` (`2e22929e5c5b4b769f615a815ef0c20f`)                                                 |        159 |                              |
|   ✅   | Reconciliation Report             | `standard/reconcile-report.scm` (`e45218c6d76f11e7b5ef0800277ef320`)                                               |        115 | `test-transaction.scm`       |
|   ✅   | Transaction Breakdown             | `standard/txn-columns.scm` (`03603f0443494a6fa790b417f35d90d4`), "Transaction Breakdown Report"; not on the engine |        260 |                              |
|   ❌   | Register report                   | `standard/register.scm` (`22104e02654c4adba844ee75a3f8d173`): GUI-only, won't do ("Decided", item 5)             |        674 | `test-register.scm`          |
|   ✅   | Trial Balance                     | `standard/trial-balance.scm` (`216cd0cf6931453ebcce85415aba7082`)                                                  |        876 | `test-trial-balance.scm`     |
|   ✅   | Cash Flow                         | `standard/cash-flow.scm` (`f8748b813fab4220ba26e743aedf38da`)                                                      |        346 | `test-cash-flow.scm`         |
|   ✅   | Equity Statement                  | `standard/equity-statement.scm` (`c2a996c8970f43448654ca84f17dda24`)                                               |        510 | `test-equity-statement.scm`  |
|   ✅   | Multicolumn Balance Sheet and P&L: Balance Sheet (Multicolumn), Income Statement (Multicolumn) | `standard/balsheet-pnl.scm` (`065d5d5a77ba11e8b31e83ada73c5eea`, `0e94fd0277ba11e8825d43e27232c9d4`)               |      1,261 | `test-balsheet-pnl.scm`      |
|   ✅   | Income/expense vs day of week     | `example/daily-reports.scm` (`5e2d129f28d14df881c3e47e3053f604`, `dde49fed4ca940959ae7d01b72742530`)               |        310 |                              |
|   ✅   | Cash flow chart                   | `standard/cashflow-barchart.scm` (`5426e4d987f6444387fe70880e5b28a0`)                                              |        350 | `test-cashflow-barchart.scm` |
|   ✅   | Investment Portfolio              | `standard/portfolio.scm` (`4a6b82e8678c4f3d9e85d9f09634ca89`)                                                      |        292 | `test-portfolios.scm`        |
|   ✅   | Advanced Portfolio                | `standard/advanced-portfolio.scm` (`21d7cfc59fc74f22887596ebde7e462d`)                                             |      1,190 | `test-portfolios.scm`        |
|   ✅   | Investment Lots                   | `standard/investment-lots.scm` (`ab2acc24afd14630a551f98f1a35fa81`)                                                |      2,326 | `test-investment-lots.scm`   |
|   ✅   | IFRS cost basis                   | `standard/ifrs-cost-basis.scm` (`15d5b744176c4625a703720338725291`)                                                |        533 | `test-ifrs-cost-basis.scm`   |
|        | Multicolumn reports               | `standard/view-column.scm` (`d8ba4a2e89e8479ca9f6eccdeb164588`)                                                    |        189 |                              |

The `balsheet-pnl.scm` fixture of v0.2 (`fixtures/reports/balsheet-pnl.gnucash`) already exists. The GUIDs and line
counts above were read from the pinned source for this draft; the research note for each group confirms them and
lists the library functions each report needs.

## Distribution (proposed)

Getting red-ale onto a machine and into the user's agents, so they can run reports from a harness. Not in the roadmap;
proposed for v0.5 because the release workflow is written and `setup` is approved ("Decide before code", item 1).

| Status | Item             | Notes                                                                                                                                                                                                                                               |
|:------:|------------------|-----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
|        | Release binaries | `.github/workflows/release.yml`: four targets (x86_64 and aarch64 for Linux musl and macOS), archives with SHA-256 files and build provenance, on a `vX.Y.Z` tag matching the workspace version; a manual run builds without releasing. Windows is deferred ("Out"). The workflow is in place and also checks and attaches `install.sh`; pushing the first tag, and checking its assets, is the maintainer's step |
|   ✅   | `red-ale setup`  | [`setup.md`](setup.md) (approved): registers `red-ale mcp` with the eleven first-release harnesses, research in [`notes/mcp-harnesses.md`](notes/mcp-harnesses.md). Crate `red-ale-setup`: file adapters for opencode, Claude Desktop, Cursor, VS Code, Antigravity, Devin and Zed; CLI adapters for Claude Code, Codex, Copilot CLI, Gemini and Devin (when `devin` is on `PATH`). Tests: golden files per adapter and sample (`crates/red-ale-setup/tests/adapters.rs`), fake CLIs that record their arguments (`tests/cli.rs`), the command line (`crates/red-ale/tests/setup.rs`). The manual smoke test per harness is still to run before the release |
|   ✅   | Install script | `install.sh`, attached to each release: download the archive for the machine, check its SHA-256, install the binary at a fixed path, run `red-ale setup` ([`setup.md`](setup.md), "Installation"). Tested against a fake release served from `file://` (`crates/red-ale/tests/install.rs`): install, `--no-setup`, arguments after `--`, a checksum that doesn't match |
|        | Version          | The workspace version is still `0.2.0`; the first release sets it to the milestone being released                                                                                                                                                   |

## Carried forward from v0.4

From [`milestone4.md`](milestone4.md), "Follow-ups":

| Status | Follow-up | Notes |
|:------:|-----------|-------|
|   ✅   | XML split order | The XML reader sorts each transaction's splits as GnuCash's load does: a stable sort, value ≥ 0 before value < 0, from the commit that ends each transaction's read, Book and template transactions alike (`5.17:libgnucash/backend/xml/gnc-transaction-xml-v2.cpp:565-572`; `5.17:libgnucash/engine/Transaction.cpp:540-553`, `:1508-1510`). The rule is the core's `sort_splits`, shared with the SQLite reader through `sort_as_splits`, and the element's splits take the same order, so an order alone keeps no original text and original text kept for another reason is written sorted. Every XML fixture is already sorted, so `round_trip.rs` passes unchanged; `splits_another_writer_left_unsorted_are_read_and_written_as_gnucash_sorts_them`, `the_split_sort_keeps_the_file_order_on_each_side`, `original_text_kept_for_a_transaction_has_its_splits_sorted`, `template_transaction_splits_are_sorted_as_they_are_read`; oracle: `oracle_split_order.rs`, `red_ale_saves_unsorted_splits_as_gnucash_saves_them` (red-ale's no-edit save is byte for byte what GnuCash 5.17 writes after load and save) |
|   ✅   | Since Last Run at scale on SQLite | Below. `since_last_run_scale.rs`, `a_since_last_run_creating_5000_transactions_on_an_sqlite_book_is_one_mutation` |
|   ✅   | An SQLite book whose template root has no row | Made in memory, as GnuCash does: `sxtg_book_begin` makes a template root and loading the `books` row gives it the row's GUID (`5.17:libgnucash/engine/SX-book.cpp:113-125`; `5.17:libgnucash/backend/sql/gnc-book-sql.cpp:119-136`). A new scheduled transaction's template account goes under it and only its own row is written (`5.17:libgnucash/engine/Account.cpp:2824-2879`; `5.17:libgnucash/backend/sql/gnc-account-sql.cpp:278-344`): the oracle, given such a book, wrote no row for the root and reopened with the template account under it. red-ale writes the same rows. Neither a refusal nor a new row: GnuCash edits the book without one. `sqlite_template_root.rs`, `the_first_scheduled_transaction_goes_under_a_template_root_with_no_row`; oracle: `gnucash_reads_the_first_scheduled_transaction_of_a_book_whose_template_root_has_no_row` |
|        | Rhai closure bugs | Reported upstream by the maintainer; the conventions stay until a release fixes them |

**Measured: Since Last Run on SQLite.** As in [`milestone4.md`](milestone4.md) ("Done when", "Measured: Since Last
Run"), with the example `memory`: `schedule 10 500` adds ten daily scheduled transactions of 500 occurrences to a copy
of the 100,000-transaction book, and `since-last-run` opens it at the testkit's clock, proposes, and applies with no
choices. The SQLite book is the one `generate-sqlite` writes (108.3 MB). Both books were measured again on this
branch, in release mode; the proposal holds 5,000 instances to create and the apply creates 5,000 transactions (100,000
to 105,000) in one Mutation (Book revision 0 to 1), with a journal of 10,000 records, a `B` and a `C` for each:

| Step                                | Time, XML v0.4 | Time, XML | Time, SQLite | Peak heap, XML v0.4 | Peak heap, XML | Peak heap, SQLite |
|-------------------------------------|---------------:|----------:|-------------:|--------------------:|---------------:|------------------:|
| Open, at rest                       |                |           |              |            174.3 MB |       147.9 MB |          173.7 MB |
| `scheduledTransaction.sinceLastRun` |        47.9 ms |   45.3 ms |      43.4 ms |            213.0 MB |       186.6 MB |          212.4 MB |
| `scheduledTransaction.apply`        |        79.6 ms |   61.4 ms |     106.8 ms |            199.6 MB |       173.2 MB |          205.0 MB |
| After the apply                     |                |           |              |            180.6 MB |       154.2 MB |          180.0 MB |
| `book.save` of the 105,000          |         3.05 s |    3.01 s |     519.5 ms |            188.8 MB |       162.4 MB |          207.7 MB |

Peak resident set 255.3 MB on SQLite and 220.8 MB on XML (248.5 MB in v0.4), within the 564 MB budget. A second run of
each gave the same heap figures and times within 15%. The SQLite save writes only the rows of what the apply added, in
a sixth of the time of the XML save, which writes the whole file; its peak is 27.7 MB above the Book after the apply,
against the XML save's 8.2 MB. The XML figures fell by 26.4 MB because the XML reader now rebuilds each transaction's
split list at its exact length when it sorts it, where the list read from the file had room to spare; measured with
the sort switched off, the XML figures were v0.4's to the tenth of a megabyte. The SQLite reader's split lists still
grow one split at a time ("Follow-ups").

Running it on SQLite found a testkit bug: `add_daily_schedules` and `run` each used a dispatcher whose first session
made the same GUIDs, so the transactions the apply created took the GUIDs of the scheduled transactions and template
transactions made before. SQLite's `transactions` table, which holds both, refused the save; the XML book saved them
without complaint, so the v0.4 XML figures were measured on a book with those duplicates (the figures above are not).
`add_daily_schedules` now uses its own range of sessions (`dispatcher_from`).

## Interfaces

New reports need no new methods: `report.run`, `report.export` and `report.options` reach every registered template.
`redAleApi` becomes 0.5.0 with the first addition, if any (the Multicolumn report's child references may need one,
item 3). `red-ale setup` is a CLI command only ([`setup.md`](setup.md), "What it is not").

| Status | Interface    | Notes                                                                                                                                               |
|:------:|--------------|-----------------------------------------------------------------------------------------------------------------------------------------------------|
|        | JSON-RPC API | `redAleApi` 0.5.0 (started), additive over 0.4.0; the 0.4.0 document is committed beside the others and `api_additive.rs` checks against it. Added so far, to `ReportResult` ([`reports.md`](reports.md) §7): link kinds `split`, `transaction`, `invoice`, `price`, `docLink`; `anchors` cells; `until` (host: `localUntil`) on date cells; `labelLines` on columns; `link` on text blocks; `list` blocks; `alternate` on rows; the `warning` row role; tables with no columns; `rounding` and `numberFormat` on number cells; `exportError` |
|        | MCP server   | Unchanged: the new templates are reached through the report tools                                                                                   |
|        | CLI          | `--no-config` (done); `red-ale setup` if distribution is in                                                                                         |

## Testing

As v0.2 and v0.4 did for reports ([`reports.md`](reports.md) §13):

- **GnuCash's own report tests, ported.** Each `test-*.scm` in the table above is read as text: its book becomes a
  fixture built by the 5.17 oracle through the Python bindings, and its assertions become Rust tests on that fixture. **No test runs Guile**
  ([`milestone4.md`](milestone4.md), "Decided", item 12).
- **Hand-worked cases** for every option and branch the ported tests don't reach, each citing the `.scm` lines it
  follows.
- **Golden bytes:** `report_golden.rs` records every v0.5 report on every fixture book, XML and SQLite, and every
  earlier report's bytes stay unchanged.
- **Exact figures:** where GnuCash computes in doubles (rates of gain and return, Investment Lots' growth rates), red-ale
  computes exactly and records each difference in [`reports.md`](reports.md)'s floating-point table.
- **Reports only read:** a run leaves the Book's revision and the file unchanged, XML and SQLite.

## Order of work

1. ✅ **The default Configuration file.**
2. **The Transaction Report engine** as a library module, then the Transaction Report, General Ledger and
   Reconciliation Report on it; the General Journal on `lib/register`.
3. ✅ **Statements:** Trial Balance, Cash Flow, Equity Statement, Multicolumn Balance Sheet and P&L.
4. ✅ **Transaction Breakdown.**
5. ✅ **Analysis charts:** Income/expense vs day of week, Cash flow chart.
6. **Investment reports:** Investment Portfolio, Advanced Portfolio, IFRS cost basis, Investment Lots.
7. **Multicolumn reports**, last, once there are many reports to combine.
8. **Distribution**, alongside the reports if it is in: the release, `setup` and the install script.
9. **Interfaces and audit.**

## Decide before code

### Decided

1. **Distribution is in v0.5**, as "Distribution" sets out. It doesn't touch the reports, and users can't easily try
   the reports without it.
2. **No `gnucash-cli --report run`.** GnuCash's reports are Guile, and red-ale runs no Guile, in tests either. Report
   figures are checked against GnuCash's own report tests, read as text, and hand-calculated values. The roadmap's
   paragraph is left as written, with this decision recorded here.
3. **Multicolumn reports** (`view-column.scm`) embed other reports. A Multicolumn report's options name its child
   reports by saved configuration GUID, or by template GUID with options; each child is rendered by the engine and
   laid out in the grid GnuCash uses. A child that no longer exists renders a warning in its cell, as a deleted budget
   does. Settled by [`notes/report-statements-charts.md`](notes/report-statements-charts.md) §8.4: `report.run`
   takes no new param. Children are a new option type, `reportPlacement` (GnuCash's `__general`/`report-list`), an
   array of `{report, options?, colspan, rowspan}` in the existing `options` param; the host runs each child into a
   new `grid` block. Views nest to depth 4, at most 100 child runs per run under one deadline; a cycle or a limit
   renders a warning in its cell. The note's open questions are followed as proposed.
4. **The Transaction Report engine** is a library module (`lib/trep-engine.rhai`) with its options, sorting,
   subtotals and the split filter, and the reports on it are thin scripts, as GnuCash's are. The split query
   reuses `book.split_query` and grows only what the engine's filters need (`transaction.find`'s criteria). The
   research ([`notes/report-transaction-engine.md`](notes/report-transaction-engine.md)) found three reports on it,
   not four; its open questions are followed as proposed (the engine builds CSV rows, but no v0.5 template offers
   CSV, as in 5.17; Primary Key "None" keeps split order, a deviation).
5. **The Register report won't be done** (❌). GnuCash runs it from a register window, on that window's query and
   layout (`standard/register.scm`); red-ale has no GUI. Its `features.md` row is ❌. The General Journal, a template
   of its own in GnuCash's Reports menu, delegates to the Register report's renderer in journal mode with a query of
   every non-void transaction (`standard/general-journal.scm:41-118`), so the renderer and options are translated as
   a library, `lib/register`, used by the General Journal only; no Register template is registered.

6. **Investment arithmetic** (settled by [`notes/report-investments.md`](notes/report-investments.md) §7). All four
   reports compute in their scripts; the core supplies reads only (lots, `split.lot`, the stock-split flag, prices in
   any currency, the other split). Advanced Portfolio keeps its own (units, price) list for average, FIFO and LIFO,
   with GnuCash's 10⁻⁸ and SCU roundings, which are exact steps; Investment Lots does its own basis arithmetic; IFRS
   cost basis has no floating point. The core's gains code writes gains transactions, which a report may not. Rates
   of gain and return are exact, rounded half away from zero, and differ from GnuCash's doubles only at ties;
   Investment Lots' annual growth rate is a fractional power, computed in fixed point to 10⁻¹². The note's open
   questions are followed as proposed.
7. **Rows missing from `features.md`** (settled by the same note, §10): the Lot Viewer and the Dashboard get rows for
   v0.6; the Balance Sheet (eguile) and Sample Graphs are ❌.

## Done when

- Every row above is ✅ or ❌, and each ✅ GnuCash feature's Ported column in `features.md` says `0.5`.
- Every report above gives the figures its expected results hold, on XML and SQLite fixtures, and every earlier
  report's golden bytes are unchanged.
- No report changes a Book: the Book's revision and the file are unchanged after every run.
- Every red-ale process without `--config` shares the default Configuration file, `--no-config` reads and writes no
  file, and concurrent changes keep both.
- If distribution is in: a tagged release has the four archives with checksums and provenance; `red-ale setup` passes
  its per-harness tests and a manual smoke test in each first-release harness; the install script installs and sets up
  on Linux and macOS.
- Every JSON-RPC method is an MCP tool with the same name, params schema and hints, and any API change is additive,
  checked by a test.
- v0.1's to v0.4's done-when checks still pass.

## Follow-ups

- **The SQLite reader's split lists.** It pushes each split onto its transaction's list, so a list of two or three
  splits has room for four; the XML reader's exact lists saved 26.4 MB on the 100,000-transaction book ("Carried
  forward from v0.4", "Measured: Since Last Run on SQLite"). Shrinking them is a memory change of its own.
- **Duplicate GUIDs across transactions and template transactions.** An XML save writes a Book in which a
  transaction and a template transaction share a GUID without complaint, though GnuCash keeps both in one collection;
  only the testkit has made one ("Carried forward from v0.4"). Whether a save should refuse it is open.
- **`setup --remove` and an empty servers object.** Removing the red-ale entry also removes the servers object
  (`mcpServers`, `servers`, …) when that leaves it empty, so removing what `setup` added gives back the original. A
  file that already had an empty `"mcpServers": {}` loses that key. Harmless to every Harness known, but not
  byte-exact.
- **`setup` updates through a Harness CLI are remove then add.** If the add fails after the remove, the Harness is
  left with no red-ale entry; the error names the failing command, but the old entry is not restored.
- **Report functions that reuse an engine function's name.** A report script's function is found before the engine's
  inside the libraries it imports too, so a report `fn abs(x)` would replace `abs` there (Rhai 1.27,
  `func/script.rs:377`). The load check refuses clashes between scripts, not with registered or standard-package
  functions; it could, with Rhai's `metadata` feature enabled outside tests. A dependency change of its own.
