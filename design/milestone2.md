# Milestone 2: red-ale v0.2

**Goal:** an MCP server over every red-ale operation; GnuCash's reports from 1.6 or earlier, translated into Rhai;
the remaining features of books and accounts, transactions and business; user currency symbols; and time zones
handled as GnuCash 5.17 handles them. All on XML books written by GnuCash 5.17.

This file tracks progress towards v0.2. Feature names and versions come from [`features.md`](features.md); when a
feature here is done, set its Ported column there to `0.2`.

## Scope

The two main features for milestone 2 are

- the MCP server;
- the report scripting engine, with the reports below translated to it (see "Report scripting" and "Interfaces").

In addition the following features from `features.md`:

- all outstanding features in §1 Books and accounts, including trading accounts, num field source and the auto
  read-only threshold;
- all outstanding features in §2 Transactions;
- all outstanding features in §6 Business, including credit notes, except business import;
- user currency symbols from §3.

**Out:**

- Reports introduced after GnuCash 1.6.0: the Transaction Report (1.6.1), Trial Balance, Cash Flow, Equity
  Statement, General Journal, General Ledger, Income/expense vs day of week, and every later report.
- Reports that need features red-ale has not ported: the Register report (GnuCash runs it only from a register
  window) and the Future Scheduled Transactions Summary that `standard/account-summary.scm:432` defines beside Account
  Summary. Budget and scheduled-transaction reports stay out with §4 and §5.
- Logo and background images in style sheets.
- All import and export: §7, and the CSV imports under §6 "Business import". They move to v0.3, with the CSV export
  that the net and category charts offer ([`reports.md`](reports.md) §12).
- The stock transaction assistant (won't do).

## Compatibility in v0.2

Out of scope still never means lost, and every v0.1 rule holds, with these changes:

- **Four read-only triggers are lifted**, each once its feature is ✅: trading accounts, num field source, credit
  notes and the auto read-only threshold. Until then a book using one still opens read-only.
- **Book features are set and read as GnuCash does, and no others**: `Credit Notes` and `Number Field Source`
  (`5.17:libgnucash/engine/gnc-features.h:54-55`) and the opening balance account feature (`:64`). A book declaring
  a feature red-ale doesn't implement still opens read-only.
- **Settings GnuCash keeps outside the book stay outside it.** Saved report configurations go in red-ale's
  Configuration.
- **Reports only read.** Every report method has the effect `read`, and reports run on books open read-only.

## Time zones

red-ale handles time zones as GnuCash 5.17 does (decided; see [`open-questions.md`](open-questions.md)):

- **Times are stored in UTC**, written to XML as `YYYY-MM-DD HH:MM:SS +0000`
  ([`notes/xml-format-core.md`](notes/xml-format-core.md) §4).
- **Dates are stored at day-neutral time**, 10:59:00 UTC, so a date reads as the same calendar day in every zone from
  UTC−10 to UTC+13; zones further out are moved to stay in that range (`5.17:libgnucash/engine/gnc-datetime.cpp:249-268`).
  Posted dates and prices entered by date use it. A posted date with no `date-posted` slot is read as its UTC date,
  as GnuCash reads it (`5.17:libgnucash/engine/Transaction.cpp:2271-2285`).
- **Everything else uses the local time zone**: day start and end (local 00:00:00 and 23:59:59,
  `5.17:libgnucash/engine/gnc-date.cpp:1366-1395`), "today", relative report dates such as "start of this month"
  (`5.17:libgnucash/engine/gnc-option-date.cpp:519`), report end dates (`standard/balance-sheet.scm:261-262`), which
  prices fall on the same day, and backup and journal file names.
- **The local zone comes from the environment**, in GnuCash's order: the `TZ` variable, else `/etc/localtime`, else
  UTC (`5.17:libgnucash/engine/gnc-timezone.cpp:744-760`).
- The wire types don't change: dates are `YYYY-MM-DD`, timestamps RFC 3339 in UTC.

v0.1 used UTC in three places where GnuCash uses local time. Business documents already use local time.

| Status | Change                                                     | Where v0.1 differs                                              |
|:------:|------------------------------------------------------------|-----------------------------------------------------------------|
|   ✅   | Same-day prices decided in local time                      | [`api/prices.md`](api/prices.md), "Same day, source precedence" |
|   ✅   | "Today" and the find dialog's day boundaries in local time | [`api/transactions.md`](api/transactions.md), "Times"           |
|   ✅   | Accounting period turned into local 00:00:00 and 23:59:59  | [`api/accounts.md`](api/accounts.md), accounting period         |

## Report scripting

GnuCash's reports are Guile Scheme, and red-ale never runs Guile. Each report here is translated from its 5.17
Scheme file into a Rhai script, keeping its options, logic and figures.

### Translating the reports

- **One `.rhai` per `.scm`.** Every report file cited under "8. Reports" becomes a Rhai file with the same base name
  and GnuCash's directory: `standard/balance-sheet.scm` becomes `standard/balance-sheet.rhai`,
  `example/average-balance.scm` becomes `example/average-balance.rhai`. Only report templates whose features are
  ported are translated.
- **Shared helpers become Rhai modules.** Logic the reports call from `report-utilities.scm`,
  `commodity-utilities.scm`, `options-utilities.scm` and `html-acct-table.scm` (which accounts and subtotals a table
  shows) is translated as modules, loaded through the engine's module resolver. Scheme that only builds HTML
  (`html-document.scm`, `html-table.scm` and the like) is not translated; red-ale's renderer draws the report result.
- **Translated, not transliterated.** Scheme idioms become plain Rhai, but option names, defaults and figures stay
  as 5.17 has them. A deliberate difference, such as an exact figure where GnuCash uses floating point, is
  commented where it happens.
- **Licence.** Every file listed here is GPL-2.0-or-later (checked at 5.17), so its translation can be
  GPL-3.0-or-later. Each `.rhai` keeps its source's header, author lines and licence notice, adds red-ale's, and
  cites its source as `5.17:gnucash/report/<path>`.

### Requirements

- **Exact money.** Scripts have no floating point. Amounts are exact, with GnuCash's arithmetic and rounding. Where
  GnuCash computes a figure in floating point, red-ale computes it exactly and rounds it as GnuCash displays it. Any
  figure that can't match this way is listed in the report's design notes.
- **Wide enough for GnuCash's sums.** GnuCash hands amounts to Scheme as exact fractions
  (`5.17:bindings/guile/gnc-engine-guile.cpp:1730-1733`), and the report helpers add and multiply them with Scheme
  arithmetic, which has no size limit. Report arithmetic needs a wider rational than the 64-bit `Numeric`, rounding
  to the commodity's SCU only for display, so a report never fails with `arithmetic` where GnuCash prints a figure.
- **Read-only access to the Book**, through a query API that uses the glossary's names and mirrors the JSON-RPC read
  methods. A script can't reach a Mutation.
- **Sandboxed and deterministic.** No files, network, environment or clock. The report date and "now" are passed in,
  so the same Book, options and time give the same output, byte for byte.
- **Bounded.** Limits on operations, memory and run time. A script that passes a limit stops with a stable error code.
- **Options declared by the script**, in a schema red-ale publishes, using GnuCash's option types and its default
  values for each report.
- **A structured result.** A report returns sections, tables, rows with exact amounts, and chart series: the
  published language for reports, versioned with the API. HTML is rendered from that result through a style sheet, so
  clients and agents get data, not markup.
- **GnuCash's identities.** Built-in reports keep 5.17's template names and GUIDs, so a report named for
  `gnucash-cli` names the same report in red-ale.
- **Built-in and user reports.** Built-in reports are compiled into the binary. User reports load from a directory set
  in the Configuration.

### Engine: Rhai

Reports are [Rhai](https://rhai.rs/book/) scripts (decided; see [`open-questions.md`](open-questions.md)). Rhai is
pure Rust under MIT or Apache-2.0, which `deny.toml` already allows. It meets the requirements above once red-ale
configures it this way:

- **Exact money:** built with the [`no_float` and `no_time` features](https://rhai.rs/book/start/features.html), so a
  script can't make a floating-point number or read the clock. red-ale's exact amount type is registered as a custom
  type.
- **Sandboxed:** the engine starts with no access to anything outside itself except what red-ale registers, which is
  only the read-only query API. Two defaults are replaced: the file module resolver, which would let `import` read
  scripts from disk, gives way to one holding only built-in and configured reports; and `print` and `debug`, which
  write to standard output (the MCP stdio channel), go to the report's log instead.
- **Bounded:** [operation, call-depth and data-size limits](https://rhai.rs/book/safety/) are built in. The
  `unchecked` feature, which turns them off, is never enabled.
- **Deterministic:** Rhai's object maps are ordered by key.

LLMs, directed by people, will write most report scripts, so how familiar the language is to people matters less
than how safe it is. How reports use Rhai (the query API, options and result) goes in `design/reports.md`, before any
report code.

## 1. Books and accounts

### Account structure

| Status | Feature                    | Introduced | Epic | Notes                                                                                     |
|:------:|----------------------------|:----------:|------|-------------------------------------------------------------------------------------------|
|   ✅   | Trading account type       |    2.4     |      | With "Use trading accounts"                                                               |
|   ✅   | Account colours            |    2.4     |      |                                                                                           |
|   ✅   | Balance limits             |    5.0     |      |                                                                                           |
|   ✅   | Opening balance account    |    4.3     |      | Sets the opening balance account book feature (`gnc-features.h:64`)                       |
|   ✅   | Associated accounts        |    5.6     |      | Stored and edited only: in 5.17 only the stock transaction assistant (won't do) uses them |
|   ✅   | Cascade account properties |    4.0     |      | Needs Account colours                                                                     |

### Book settings

| Status | Feature                     | Introduced | Epic | Notes                                                                               |
|:------:|-----------------------------|:----------:|------|-------------------------------------------------------------------------------------|
|   ✅   | Number counters and formats |   2.4.1    |      | v0.1 already assigns the next document number                                       |
|   ✅   | Auto read-only threshold    |    2.6     |      | Lifts a v0.1 read-only trigger                                                      |
|   ✅   | Num field source            |    2.6     |      | Lifts a v0.1 read-only trigger; sets `Number Field Source` (`gnc-features.h:55`)    |
|   ✅   | Use trading accounts        |    2.4     |      | Lifts a v0.1 read-only trigger; every Mutation keeps trading splits as GnuCash does |
|   ✅   | Close book                  |   2.2.4    |      |                                                                                     |

## 2. Transactions

| Status | Feature        | Introduced | Epic | Notes                                                 |
|:------:|----------------|:----------:|------|-------------------------------------------------------|
|   ✅   | Document links |    2.6     |      | Link to a file or URL                                 |
|   ✅   | Auto-clear     |    2.4     |      | Finds the splits that make up a given cleared balance |

## 3. Commodities

| Status | Feature               | Introduced | Epic | Notes                     |
|:------:|-----------------------|:----------:|------|---------------------------|
|   ✅   | User currency symbols |    2.6     |      | Used by report formatting |

## 6. Business

| Status | Feature                           | Introduced | Epic | Notes                                                                                                                              |
|:------:|-----------------------------------|:----------:|------|------------------------------------------------------------------------------------------------------------------------------------|
|   ✅   | Credit notes                      |    2.6     |      | Lifts a v0.1 read-only trigger; sets `Credit Notes` (`gnc-features.h:54`)                                                           |
|   ✅   | Duplicate document                |   2.4.1    |      |                                                                                                                                    |
|   ✅   | Document links                    |    4.2     |      |                                                                                                                                    |
|   ✅   | Payment matching and pre-payments |    2.6     |      | v0.1 already makes lot links and pre-payment lots ([`api/business-posting.md`](api/business-posting.md)); scope what's left first |
|   ✅   | Payment from transaction          |    2.6     |      |                                                                                                                                    |

## 8. Reports

Only reports introduced in GnuCash 1.6.0 or earlier whose features are ported. All report paths are under
`gnucash/report/reports/`.

### Report system

| Status | Feature                     | Introduced | Epic | Notes                                                                                                                            |
|:------:|-----------------------------|:----------:|------|----------------------------------------------------------------------------------------------------------------------------------|
|   ✅   | Report framework            |   ≤ 1.4    |      | Templates with options, listed and run through the API                                                                           |
|   ✅   | Style sheets                |    1.6     |      | 5.17's Plain, CSS, Easy, Fancy, Footer and Head or Tail, in red-ale's own clean, professional look. No logo or background images |
|   ✅   | Saved report configurations |   1.8.10   |      | Kept in red-ale's Configuration                                                                                                  |
|   ✅   | HTML export                 |    1.6     |      | Rendered from the report result through a style sheet; charts as inline SVG                                                      |

Report styling is red-ale's own: clean and professional, without copying GnuCash's markup. The figures are what must
match.

### Financial statements

| Status | Feature            | Introduced | Epic | Notes                                                                                                                         |
|:------:|--------------------|:----------:|------|-------------------------------------------------------------------------------------------------------------------------------|
|   ✅   | Account Summary    |    1.6     |      | The Account Summary template only (`standard/account-summary.scm:425`)                                                        |
|   ✅   | Balance Sheet      |    1.6     |      | `standard/balance-sheet.scm:529`. Reads the trading accounts option (`:266`)                                                  |
|   ✅   | Income Statement   |    1.6     |      | Income Statement and Profit & Loss (`standard/income-statement.scm:568-578`); trading accounts section (`:88`)                |

### Analysis and charts

Charts are static SVG, drawn by red-ale's HTML renderer from the series in the report result (decided; see
[`open-questions.md`](open-questions.md)). The page has no JavaScript, so an exported report works offline and is the
same bytes on every run. Pixel positions are worked out with integer arithmetic, so the renderer uses no floating
point either. Hover labels are SVG `<title>` elements.

| Status | Feature                     | Introduced | Epic | Notes                                                                                                                  |
|:------:|-----------------------------|:----------:|------|------------------------------------------------------------------------------------------------------------------------|
|   ✅   | Average Balance             |   ≤ 1.4    |      | `example/average-balance.scm`. 5.17 files it as an example but loads it (`reports/reports.scm:51`)                     |
|   ✅   | Net worth and income charts |    1.6     |      | Net Worth Barchart and Linechart, Income/Expense Chart, Income & Expense Linechart (`standard/net-charts.scm:475-511`) |
|   ✅   | Account bar charts          |    1.6     |      | Income, Expense, Assets and Liabilities Over Time (`standard/category-barchart.scm:67-70`)                             |
|   ✅   | Account pie charts          |    1.6     |      | Income Accounts, Expense Accounts, Assets, Securities, Liabilities (`standard/account-piecharts.scm:59-63`)            |
|   ✅   | Price chart                 |    1.6     |      | Price Scatterplot (`standard/price-scatter.scm:283`)                                                                   |

## Interfaces

These are red-ale's own work, not GnuCash features. Each feature above counts as done only once it is reachable
through JSON-RPC, MCP and the CLI.

| Status | Interface        | Notes                                                                                                                                                                                                                     |
|:------:|------------------|---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
|   ✅   | MCP server       | `red-ale mcp` over stdio, on `rmcp`. One tool per JSON-RPC method, named as the method and sharing its params schema; `readOnlyHint` and `destructiveHint` come from the method's Effect. No logic of its own. Tools only |
|   ✅   | JSON-RPC API     | Methods for every feature above, including reports (list, options, run, export, saved configurations). `redAleApi` becomes `0.2.0`, additive over `0.1.0`                                                                 |
|   ✅   | CLI              | Every new method through `exec` and `call`; report HTML written to a file                                                                                                                                                 |
|   ✅   | Report scripting | Rhai, set up as above. Every report in this file is written in it, and users can add their own                                                                                                                            |

MCP tool names can be the method names unchanged: the 2025-11-25 specification allows letters, digits, `_`, `-` and
`.` ([tools, "Tool Names"](https://modelcontextprotocol.io/specification/2025-11-25/server/tools)). Some clients are
stricter than the specification.

## Testing

Every new feature outside §8 follows [`testing.md`](testing.md): round-trip and oracle tests, with GnuCash 5.17
opening what red-ale wrote. The oracle needs no GUI for these.

### Testing reports

v0.2 doesn't run GnuCash's reports to compare against (decided; see [`open-questions.md`](open-questions.md)). Every
`.rhai` report is tested by running it on a fixture book, a synthetic XML book in `fixtures/`, and checking its
figures against expected results:

- **Expected figures from GnuCash's own report tests.** 5.17 tests the reports in this file on books it builds in
  code (`5.17:gnucash/report/reports/standard/test/`): `test-account-summary.scm`, `test-balsheet-pnl.scm`
  (Balance Sheet and Income Statement), `test-average-balance.scm`, `test-charts.scm` and the
  net and category chart tests. red-ale rebuilds the same books as fixture books and checks the same figures. Each
  ported expectation cites the test it comes from, and the fixture keeps that file's GPL notice.
  `test-average-balance.scm` loads the budget report module (`:4`); what it uses from it must be checked before
  porting.
- **Hand-worked cases** where those tests don't reach, such as options GnuCash doesn't test. Each expected figure is
  worked out from the report's 5.17 Scheme and cites the lines it follows.
- **Property tests** on what every report must satisfy: a balance sheet balances, a pie chart's slices add up to its
  total.

Checking against GnuCash's own report output waits for a later milestone. It needs a GnuCash build that includes
the GUI, since the report code links GTK (`5.17:gnucash/report/CMakeLists.txt:29-34`).

## Order of work

1. **Time zones**, since day boundaries and same-day prices change v0.1 behaviour that later work relies on.
2. **Trading accounts**, before the Balance Sheet and Income Statement, which read them.
3. **Account colours** before Cascade account properties; **user currency symbols** before report formatting.
4. **Report scripting** (`design/reports.md`, the engine, the shared helpers) before the reports themselves.

## Decide before code

1. **The report result**: the shape of sections, tables, rows and series, in `design/reports.md`. Styling is
   red-ale's own (see "Report system").
2. **Effects outside the Book.** Effect describes what a method does to the Book. Saving a report configuration
   changes the Configuration only, and needs a name in the glossary.
3. **Report names.** As in v0.1, red-ale uses GnuCash's English names for report titles and labels.
4. **Payment matching and pre-payments**: what v0.1 does not already cover.

## Done when

- Every row above is ✅, and each one's Ported column in `features.md` says `0.2`.
- Books using trading accounts, num field source, credit notes or an auto read-only threshold open for editing, and
  GnuCash 5.17 opens what red-ale writes to them with no warnings and no lost data.
- Every new feature outside §8 passes its round-trip and oracle tests.
- Day boundaries, "today" and same-day prices match GnuCash 5.17 under `TZ` values at both extremes (for example
  UTC−11 and UTC+14) and with `TZ` unset.
- Every report `.scm` cited in this file, and the shared helpers it relies on, has a `.rhai` translation.
- Each report runs on at least one fixture book and gives the expected figures under "Testing reports": those ported
  from GnuCash 5.17's report tests, and hand-worked cases for its default options and for each option that changes a
  figure.
- Every JSON-RPC method is an MCP tool with the same name and params schema and hints that match its Effect; one test
  lists both and compares them.
- Running every report leaves the Book unchanged and not dirty, and writes nothing to the journal.
- Exporting a report twice from the same Book, options and time gives the same bytes, charts included, and the HTML
  holds no script.
- Tests show that a report script can't change the Book, read a file or the clock, use floating point, or run past
  its limits.
- v0.1's done-when checks still pass.
