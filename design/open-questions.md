# Open questions and decisions

Decisions taken for each version that a later one may revisit, and questions still open. Each says why.

## Decided for v0.1

**Denominators are kept as read.** GnuCash converts each split's value to the currency fraction and its amount to the
account's SCU when it loads a book, so a file holding other denominators changes on its next save in GnuCash
(`notes/engine-accounts-transactions.md` §1.5). red-ale keeps what the file holds, so an unedited entity round-trips
byte for byte; entities red-ale creates or edits are converted as GnuCash would convert them (`ops::commit`). Files
written by GnuCash already match, so normal books are unaffected.

**Unbalanced input is refused.** GnuCash balances a transaction at commit by adding an `Imbalance-<CUR>` split.
red-ale refuses an unbalanced transaction with `unbalanced` instead; the imbalance scrub does GnuCash's repair when
asked.

**English names where GnuCash translates.** Accounts GnuCash creates (`Imbalance-USD`, `Orphan-USD`, gains
accounts), lot titles and posted-document memos use GnuCash's English strings. A book used with a translated GnuCash
gets English names for accounts red-ale creates.

**Unknown quote sources are kept.** A commodity's quote source that GnuCash 5.17 does not know is kept as read.

**The accounting period is red-ale configuration.** In GnuCash it is a user preference, not book data, so red-ale
keeps it in its own configuration rather than adding slots to the user's book.

**One session per server.** A `red-ale serve` process opens at most one book. Several books would need a session
handle in every method; nothing in v0.1 needs it.

**Unrecognised content opens read-only.** Elements and slot types red-ale does not model are kept and written back,
but a book holding content red-ale does not recognise opens read-only, because an edit could leave it inconsistent
with data red-ale cannot see.

**Credit notes, num-field source, trading accounts and the auto read-only threshold open read-only.** They change
what other operations must do (signs, which field is the number, extra balancing splits, which transactions may be
edited), so v0.1 opens such books read-only rather than edit them partly right. *v0.2 implements all four and lifts
these triggers (see [`milestone2.md`](milestone2.md)).*

**Version 1 XML files are refused** with a message to open and save them in GnuCash first. GnuCash 5.17 still reads
them, but no current GnuCash writes them.

**Replay is stricter than GnuCash.** A journal record with an unreadable line is skipped whole, and an unbalanced
replayed transaction is refused unless asked to skip it. Replayed changes are journaled (GnuCash disables the journal
during replay), so they are not lost if the session ends before a save.

**Shutdown without saving.** `red-ale serve` closes the book on Ctrl-C or SIGTERM without saving, as no client asked
it to save; the journal holds the unsaved changes and `book.replayJournal` can restore them.

## Decided for v0.2

**Reports are Rhai scripts.** [Rhai](https://rhai.rs/book/) is pure Rust (MIT or Apache-2.0), can be built with no
floating point (`no_float`) and no clock (`no_time`), has operation and size limits built in, and keeps its maps
ordered, so exact money, sandboxing and deterministic output come from the engine rather than from rules scripts
must follow. Starlark keeps a float type that can't be compiled out, and Lua is a C library whose numbers are
doubles; writing reports in Rust would leave users no way to add their own. Rhai is less familiar than Lua or Python,
but LLMs, directed by people, will write most report scripts, so that matters little. See
[`milestone2.md`](milestone2.md) for how the engine is set up.

**Charts are static SVG.** GnuCash 5.17 draws charts in the browser with Chart.js 2.9.4
(`5.17:gnucash/report/html-chart.scm:532`), about 70 KB of JavaScript after gzip. red-ale draws them as inline SVG in
Rust instead. An exported report then has no script and works offline, and its bytes are the same on every run, so
determinism tests cover charts too. Pixel positions come from `Numeric` with integer arithmetic, so no floating point
enters the renderer. Hover labels use SVG `<title>`; zooming and toggling series are left out. If interactive charts
are wanted later, Chartist (about 13 KB gzipped, MIT) is the lightest library that draws every chart type the reports
use (scatter as a line with points only).

**Reports are translated from GnuCash's Scheme.** Each report `.scm` becomes a `.rhai` with the same name, rather
than a report designed afresh to the same figures. Following GnuCash's own logic line by line is the surest way to
the same figures and options, and makes each report easy to check against its source. The report files are
GPL-2.0-or-later, so translations can be GPL-3.0-or-later; each keeps its source's header and author lines and
cites the file it comes from. Scheme that only builds HTML is not translated.

**Reports are tested against expected results, not against GnuCash.** GnuCash's report code links GTK and needs
GnuCash's GUI modules (`5.17:gnucash/report/CMakeLists.txt:29-34`, `89-91`), so the library-only oracle can't run
reports. For v0.2, expected figures come from GnuCash 5.17's own report tests, rebuilt as red-ale fixtures, and from
cases worked out by hand from the report's Scheme, with citations. Comparing with GnuCash's report output, through
`gnucash-cli --report run` in a full build (possibly in a rootless container), is for a later milestone.

**Time zones work as in GnuCash.** Times are stored in UTC; dates at day-neutral time, 10:59:00 UTC
(`5.17:libgnucash/engine/gnc-datetime.cpp:249-268`); everything else (day boundaries, "today", relative report
dates, same-day prices, backup names) in the process's local time zone, taken from `TZ`, else `/etc/localtime`,
else UTC (`5.17:libgnucash/engine/gnc-timezone.cpp:744-760`). v0.1 used UTC for same-day prices, "today", the find
dialog's day boundaries and the accounting period; v0.2 changes those to local time, so a red-ale server and a
GnuCash in the same zone agree on every date. See [`milestone2.md`](milestone2.md), "Time zones".

**Reports: the design in [`reports.md`](reports.md).** The "Decide before code" items of milestone 2 on reports are
settled there; in short:

- **The report result is data**: sections of tables (rows with roles, depth and staircase level; cells carrying
  exact Rationals with the host's display form), charts as series with axes and labels, and text. It is a published
  language owned by the new crate `red-ale-report` and versioned with `redAleApi`. HTML is rendered from it through
  a style sheet; charts are SVG drawn with integer arithmetic (pie arcs by fixed-point CORDIC).
- **Effects outside the Book get an Effect target.** Effect keeps its three values and drives MCP's hints alone; a
  new field says whether a changing method targets the `book` or the `configuration`. `period.set` was already such
  a method. Saving, renaming and deleting saved report configurations and style sheets target the Configuration.
- **`report.export` returns HTML and writes no file.** A server that wrote to caller-named paths would let a remote
  client write files where it runs; the CLI writes the HTML on its own side.
- **GnuCash's names and GUIDs** for all 19 built-in templates ([`reports.md`](reports.md) §11). A name matching
  several reports is refused, as `gnucash-cli` refuses it.
- **Report arithmetic uses an unbounded Rational** (`num-rational` with `num-bigint`, MIT or Apache-2.0), rounded
  only for display, to the commodity's SCU with banker's rounding, as GnuCash's report renderer rounds.
- **GnuCash preferences reports read** (report currency, reversed-balance accounts, date format, week start) are
  Report settings in the Configuration, with GnuCash's defaults.
- **Saved configurations and style sheets are JSON in the Configuration.** GnuCash's `saved-reports-2.8` and
  `stylesheets-2.0` are Scheme programs and are not read; importing their literal option forms is left open
  (decided and done in v0.3, below).
- **Report fixtures are built by the oracle**: Python scripts make the same engine calls as GnuCash's report tests
  and save XML books, so the fixtures are GnuCash's own files.
- **The budget import in `test-average-balance.scm:4` is unused**: the budget module exports nothing
  (`5.17:gnucash/report/reports/standard/budget.scm:28`), so the port drops it.

## Decided for v0.3

**red-ale fetches quotes itself, without Perl.** GnuCash runs Finance::Quote, a Perl library, as a child process
(`5.17:libgnucash/app-utils/gnc-quotes.cpp:176-240`). red-ale does not depend on Perl; it gets prices from the
quote sources' services in its own Rust code. Books stay compatible with GnuCash:

- **Quote source names are Finance::Quote's.** A commodity's quote source is stored in the book by its Finance::Quote
  name (`alphavantage`, `asx`, `currency`, …; 50 in all, `5.17:libgnucash/engine/gnc-commodity.cpp:180-250`).
  red-ale reads and writes the same names, so GnuCash keeps fetching for the commodity. A source red-ale doesn't
  implement yet is kept as read and reported as unsupported, never changed.
- **Fetched prices use the source string `Finance::Quote`.** GnuCash knows only its fixed list of price sources and
  ranks same-day prices by it (`5.17:libgnucash/engine/gnc-pricedb.h:165-182`). Another string would read as
  `invalid` in GnuCash and lose its rank. **⚠** Here the name means "an online quote", not that Finance::Quote fetched it.
- **Prices are built as GnuCash builds them.** Value read exactly from text, inverted where a currency is quoted the
  other way; type `last`, `nav` or `unknown`; time is the quote's date, or now if it has none; saved through
  `price.add`'s same-day rules (`gnc-quotes.cpp:546-760`).

Which sources v0.3 implements, and how API keys and network access are configured, are for the v0.3 milestone.

**Import and export move to v0.3**: all of `features.md` §7 except online banking, and the CSV imports under §6
"Business import". Two questions came with them, settled in [`milestone3.md`](milestone3.md):

- **Importers without C libraries.** GnuCash reads OFX/QFX through LibOFX, and MT940, MT942, DTAUS and CAMT through
  AqBanking, both C libraries. *Decided:* red-ale links no C library, and those formats are dropped (❌): GnuCash has
  no parsing code of its own for them to translate. The importers red-ale keeps are translated from GnuCash's code.
- **The QIF importer** is Guile Scheme in GnuCash (`5.17:gnucash/import-export/qif-imp/`). *Decided:* translated
  into Rust, as reports are.

Two facts to carry over: Bayesian account matching sets two book features (`5.17:libgnucash/engine/gnc-features.h:58-59`),
and GnuCash keeps CSV import layouts in its state file, not the book
(`5.17:gnucash/import-export/csv-imp/gnc-imp-settings-csv-tx.cpp:147`), so red-ale keeps them in its Configuration.

**GnuCash's saved report configurations and style sheets are imported as data (done).** `report.importConfigurations`
reads `saved-reports-2.8` (else `saved-reports-2.4`) and `report.importStyleSheets` reads `stylesheets-2.0`. The
caller sends the file's text; the server never reads a path. Only the literal forms GnuCash writes are read, by an
s-expression reader with no evaluator; any other form skips its entry with a reason. A configuration keeps GnuCash's
GUID and name and stores only the options that differ from its template's defaults; a template red-ale has not
translated is reported and skipped. Design: [`reports.md`](reports.md) §15; API: [`api/reports.md`](api/reports.md).

## Open

- **Authentication and remote access.** The server listens on localhost with no authentication. Remote access needs
  a design (Unix socket permissions, tokens, TLS) before it is offered.
- **Several books per server.** If clients need it, methods would take a session handle (additive: a new optional
  param).
- **Concurrent clients.** Methods run one at a time. A client cannot hold a multi-method transaction; each method is
  atomic on its own. A batch or script is not atomic as a whole.
- **Locale-dependent collation.** GnuCash orders some ties (transactions with equal date, number and date entered)
  with `g_utf8_collate`, which depends on the locale of the GnuCash that wrote the file. red-ale appends new entities
  and keeps the order it read, which GnuCash accepts on load.
- **Opening a file with no `gnc:book`.** `export.accounts` writes the account tree as GnuCash's Export Accounts does:
  commodities and accounts straight under `gnc-v2`, with no `gnc:book` element
  (`5.17:libgnucash/backend/xml/io-gncxml-v2.cpp:1340-1374`). GnuCash opens such a file as a book; red-ale's
  `book.open` refuses it (`file-format`, "expected one gnc:book, found 0"). Opening it means deciding what red-ale
  writes back on save, since an ordinary save would add a `gnc:book` element; check what GnuCash 5.17 writes first.
  Found at the end of v0.3; until it is decided, a client opens the exported tree in GnuCash or imports it with the
  CSV account importer.
