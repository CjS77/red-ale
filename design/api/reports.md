# API: reports

Listing, describing, running and exporting reports; saved report configurations; style sheets. Design:
[`../reports.md`](../reports.md), which this page refers to by section. Engine, result and renderer:
`crates/red-ale-report`; methods: `crates/red-ale-rpc/src/methods/reports.rs`; Configuration:
`crates/red-ale-service/src/config.rs`.

Reports only read the Book: `report.run` and `report.export` work on books open read-only, never make a book dirty
and never write the journal. Saved report configurations and style sheets live in red-ale's Configuration, as
GnuCash keeps them in files outside the book (`5.17:gnucash/report/gnc-report.h:35-36`); the methods that change
them have the **Effect target** `configuration` ([`../reports.md`](../reports.md) §10) and work with no book open.

Tests: `crates/red-ale-report/tests/` (engine, every report against its fixtures),
`crates/red-ale-rpc/tests/reports.rs` (methods and errors), `crates/red-ale-rpc/tests/report_import.rs` (importing
GnuCash's files) and `crates/red-ale-rpc/tests/report_chart_csv.rs` (CSV export).

## Types

`ReportRef`: a GUID (a template or a saved configuration), or a name. A name is looked up among template names and
saved configuration names; several matches give `conflict` with `data.matches`, as `gnucash-cli` asks for the GUID
when a name is ambiguous (`5.17:gnucash/report/report-core.scm:914-929`).

`ReportInfo`:

| Field | Meaning |
|-------|---------|
| `guid`, `name` | The template's GUID and name (GnuCash's, for built-in reports), or the saved configuration's |
| `kind` | `builtin`, `user` or `saved` |
| `template` | For `saved`: the GUID of the template it configures |
| `script` | The script that defines it: `standard/balance-sheet.rhai`, or `./` and a user script's path in the report directory (`./mine/report.rhai`), the source name errors and the log use. For `saved`: its template's script, or `null` when that template is not loaded |
| `menuPath`, `menuName`, `menuTip`, `inMenu` | GnuCash's menu fields, kept for clients that build menus. A saved configuration has none (`[]`, `null`, `null`, `false`): GnuCash lists custom reports in their own dialog |
| `exportTypes` | The exports offered besides HTML (v0.3): `["csv"]` for the four net charts, the four category charts and saved configurations of them (`report.exportCsv`), else `[]` |

`OptionInfo`: `{ section, name, key, type, help, default, value, … }` with the fields of each option type
([`../reports.md`](../reports.md) §6). `value` is the configured value: the default for a template, the saved value
for a saved configuration.

`Options` (a param): `{ "<section>": { "<name>": <value> } }`, only the options to change; wire forms in §6.

`ReportResult`: the published language of §7, listed in `api.schema`.

## Methods

| Method | Effect | Target | Does |
|--------|--------|--------|------|
| `report.list` | read | | Every template and saved configuration |
| `report.options` | read | | A report's options, defaults and values, and the JSON Schema of its `options` param |
| `report.run` | read | | Run a report: the report result |
| `report.export` | read | | Run a report and render it as HTML through a style sheet |
| `report.exportCsv` | read | | Run a net or category chart and return its data as CSV (v0.3) |
| `report.saveConfiguration` | write | configuration | Save a report's options under a new name |
| `report.updateConfiguration` | write | configuration | Change options of a saved configuration |
| `report.renameConfiguration` | write | configuration | Rename a saved configuration |
| `report.deleteConfiguration` | destructive | configuration | Delete a saved configuration |
| `report.importConfigurations` | write | configuration | Import GnuCash's saved report configurations from the text of `saved-reports-2.8` (v0.3) |
| `report.importStyleSheets` | write | configuration | Import GnuCash's style sheets from the text of `stylesheets-2.0` (v0.3) |
| `report.styleSheets` | read | | Style sheet templates and style sheets, with their options |
| `report.createStyleSheet` | write | configuration | Create a style sheet from a template |
| `report.updateStyleSheet` | write | configuration | Change a style sheet's options |
| `report.deleteStyleSheet` | destructive | configuration | Delete a user style sheet, or reset a predefined one |

### `report.list` (read)

No params → `{ reports: ReportInfo[], problems: [{ source, code, message, line }] }`. Built-in templates, user
templates and saved configurations, sorted by name (bytewise), then GUID: GnuCash's `-R list` sorts by the locale
(`5.17:gnucash/report/report-core.scm:833-842`). Templates with `inMenu: false` are listed too, with the flag.
`problems` lists user scripts that failed to load (code `report-load`, or `report-limit` for one past a compile-time
limit), so one bad file does not hide the others, and the files the loader left out of the report directory
(`report-load`): a symbolic link that leads outside it, a file over 1 MiB or not UTF-8, files past the 1,000th, or
`source: "."` when the directory itself cannot be read. Needs no book.

### `report.options` (read)

`{ report: ReportRef }` → `{ report: ReportInfo, options: OptionInfo[], paramsSchema }`. Options come in GnuCash's
order: sections by name, options by key (§6). Defaults that depend on the Book (account lists) are computed from
the open book, or from an empty book when none is open.

### `report.run` (read)

| Param | Type | Meaning |
|-------|------|---------|
| `report` | ReportRef | Required |
| `options` | Options? | Changes over the template's defaults, or over the saved configuration's values |
| `now` | Timestamp? | The time to run the report for; default the session's clock. Relative dates ("end of this month") and "today" follow it |
| `locale` | string? | The Locale numbers and dates are formatted in for this run (v0.3): a supported POSIX name (`fr_FR`, `fr_FR.UTF-8`, `C`) or `environment`; default the Configuration's `locale` setting ([`configuration.md`](configuration.md)). It also sets the defaults of an unset `reports.currency` and `reports.weekStart` ([`../reports.md`](../reports.md) §3, §4). A name that is not supported is invalid params |

→ `ReportResult`. Needs an open book (`no-book`). Errors: `not-found` (`data.kind: "report"`), `conflict` (an
ambiguous name), `report-option`, `report-script`, `report-limit`, `report-result`. A user script that no longer
loads defines no template, so naming one of its templates is `not-found`, and running a saved configuration of one is
`not-found` with `data.id` the template's GUID; `report.list` gives the script's `report-load` under `problems`.

### `report.export` (read)

`{ report, options?, now?, styleSheet?: string, locale?: string }` → `{ title, html }`. Runs the report as `report.run` does and
renders it through the style sheet named by `styleSheet`, else by the report's General/"Stylesheet" option (§8).
The HTML is one self-contained document: no script, no external resource, the same bytes for the same book, options,
`now` and settings. **Nothing is written to disk**: the server never writes to a path a caller names. The CLI's
`--output` writes the `html` on the caller's side. Errors as `report.run`, and `report-option` for an unknown
`styleSheet`.

### `report.exportCsv` (read)

`{ report, options?, now? }` → `{ title, csv }`. GnuCash's CSV export of a chart: the report's export thunk run
with export type `csv` (`5.17:gnucash/report/reports/standard/net-charts.scm:480-517`,
`5.17:gnucash/report/reports/standard/category-barchart.scm:721-725`). Offered by the reports whose `exportTypes`
holds `csv`: Net Worth Barchart, Net Worth Linechart, Income/Expense Chart, Income & Expense Linechart, Income Over
Time, Expense Over Time, Assets Over Time, Liabilities Over Time, and saved configurations of them. The report runs
as `report.run` runs it; the rows are the chart's data, whatever the Display options show.

| Report | Header | Each row |
|--------|--------|----------|
| Net and income/expense charts (`net-charts.scm:433-445`) | `Date`, `Assets`, `Liabilities`, `Net Worth`, or `Date`, `Income`, `Expense`, `Net Profit` | The date, then the two series and their difference as computed: a liability or expense total is not negated as the chart draws it |
| Category charts (`category-barchart.scm:674-700`) | `Date`, each series (`Other` among them when `Maximum Bars` folds the rest), `Grand Total` when there is more than one series | The date, each series' amount (averaged under `Show Average`, never a percentage), and the row's sum when there is more than one series |

Dates are ISO `%Y-%m-%d` whatever the date format. The text is what `gnc:lists->csv` writes
(`5.17:gnucash/report/trep-engine.scm:463-512`): each row padded with empty cells to the longest, text quoted with
`"` doubled, a number as Guile prints `(exact->inexact n)` (`5053.0`, `-27.0`, `796.8524633707785`, `1.0e-4`), rows
joined by `\n` with none after the last. red-ale prints the double nearest the exact figure, with the shortest
digits that read back as it, as Guile does, and computes it without floating point. `title` is the report's title:
empty for the charts, which GnuCash leaves untitled.

Errors as `report.run`, and `invalid` with `data.reason`:
- `no-export` (`data.exportType: "csv"`): the report offers no CSV export;
- `no-exportable-data`: GnuCash's "No exportable data" (`net-charts.scm:456-457`): no accounts selected, or no
  figures in the period (the chart's empty-data warning).

Nothing is written to disk; the CLI's `--output` writes the `csv` on the caller's side.

### `report.saveConfiguration` (write, configuration)

`{ name, report: ReportRef, options?: Options }` → `ReportInfo` (`kind: "saved"`, a new GUID). The options are
those of `report` with `options` applied; what is stored is only what differs from the template's defaults
(`5.17:bindings/guile/options.scm:141`). When `report` is itself a saved configuration, the new one configures the
same template. A name already used by a saved configuration is `conflict` (GnuCash appends a number,
`5.17:gnucash/report/report-core.scm:515-532`). `report-option` for an invalid value; an account given by full name
needs an open book to resolve, a GUID does not.

### `report.updateConfiguration` (write, configuration)

`{ configuration: ReportRef, options: Options }` → `ReportInfo`. Sets the given options; an option given as `null`
returns to the template's default. This is GnuCash's "Save Report Configuration" over an existing one, which keeps its
GUID and name (`5.17:gnucash/report/report-core.scm:705-719`). `not-found` unless `configuration` is a saved
configuration.

The `configuration` param of this method, `renameConfiguration` and `deleteConfiguration` is a GUID or name looked
up among saved configurations only, so a template of the same name does not make it ambiguous; no match is
`not-found` with `data.kind: "configuration"`.

### `report.renameConfiguration` (write, configuration)

`{ configuration: ReportRef, name }` → `ReportInfo`. `conflict` when the name is taken, as GnuCash's dialog refuses
it (`5.17:gnucash/gnome/dialog-custom-report.c:462-494`).

### `report.deleteConfiguration` (destructive, configuration)

`{ configuration: ReportRef }` → `{ deleted: ReportInfo }` (`gnc:delete-report`,
`5.17:gnucash/report/report-core.scm:811-818`).

### `report.importConfigurations` (write, configuration)

`{ text, dryRun?: bool }` → `{ dryRun, imported: [{ line, configuration: ReportInfo, options: Options, ignored }],
skipped: [{ line, name?, guid?, template?, reason, message }] }`. `text` is the content of GnuCash's
`saved-reports-2.8`, or of `saved-reports-2.4` when there is no 2.8 file
(`5.17:gnucash/report/gnc-report.h:35-36`), from its user data directory. **The server never reads a path**: the
caller sends the text, and the CLI's `--file` reads it on the caller's side. The text is read as data and never
evaluated ([`../reports.md`](../reports.md) §15).

Each entry (`gnc:report-template-serialize-internal`, `5.17:gnucash/report/report-core.scm:620-643`) becomes a saved
configuration with GnuCash's GUID and name, configuring the template its `'parent-type` names and storing only the
options that differ from that template's defaults (`5.17:bindings/guile/options.scm:141`), read without a Book.
`options` is what is stored, in wire form. `ignored` lists options the entry sets that the template does not have
(`reason: "unknown-option"`), which GnuCash ignores too (`options.scm:130-138`). Entries are imported in file order,
each on its own: one skipped does not stop the others, and everything imported is written in one save of the
Configuration. With `dryRun` nothing is written.

| `reason` | The entry is skipped when |
|----------|---------------------------|
| `malformed` | It is not a form GnuCash writes, or a value is not a literal of its option's kind; text that cannot be read at all ends the file with one `malformed` entry |
| `untranslated` | Its template is not one red-ale has: a report red-ale has not translated, or a user report that is not loaded |
| `exists` | A saved configuration with its GUID is in the Configuration or earlier in the file, so importing twice adds nothing |
| `conflict` | Another saved configuration has its name |
| `invalid-option` | A value red-ale refuses, as `report-option` would |

An option form is accepted with the setter 5.17 and 3.x/4.x write or the one 2.x wrote. The values each option
kind accepts, and how they map to red-ale's wire forms, are in [`../reports.md`](../reports.md) §15. A
General/"Stylesheet" naming a style sheet red-ale does not have is kept, and renders with the base look until that
style sheet is imported or created (§8).

### `report.importStyleSheets` (write, configuration)

`{ text, dryRun?: bool }` → `{ dryRun, imported: [{ line, styleSheet, ignored }], skipped }`, with `skipped` as
above. `text` is the content of GnuCash's `stylesheets-2.0` (`5.17:gnucash/report/html-style-sheet.scm:142-166`).
An entry naming a predefined style sheet (`Default`, `Easy`, …) holds the user's changes to it; any other name is a
user style sheet. `ignored` lists the image options red-ale keeps but does not render (`not-rendered`, §8) and options
the template does not have (`unknown-option`). Skip reasons: `malformed`; `untranslated` (a style sheet template
red-ale does not have); `exists` (the Configuration already has an entry for that name, so edits made in red-ale
are kept); `invalid-option`; `unchanged` (a predefined style sheet at its template's defaults: nothing to keep).

### `report.styleSheets` (read)

No params → `{ templates: [{ name, options: OptionInfo[] }], styleSheets: [{ name, template, predefined,
options: OptionInfo[] }] }`. The templates are Plain, CSS, Easy, Fancy, Footer and Head or Tail; the predefined style
sheets are `Default`, `CSS-based stylesheet (experimental)`, `Easy`, `Technicolor`, `Footer` and `Head or Tail`
(§8). Style sheets are sorted by name, bytewise.

### `report.createStyleSheet` (write, configuration)

`{ name, template, options?: Options }` → the style sheet. `conflict` when a style sheet has that name (`data.kind`,
`data.name`); `not-found` for an unknown template (`data.kind: "styleSheetTemplate"`); `report-option` for an invalid
value, including CSS text that §8 refuses (`</style`, `@import`, `url(`, …). Values for the image options red-ale
leaves out are kept in the Configuration.

### `report.updateStyleSheet` (write, configuration)

`{ name, options: Options }` → the style sheet; `null` returns an option to its default. Works on predefined style
sheets too, as GnuCash lets the user edit `Default`. The Configuration entry is merged with the changes, never
replaced, so image options it holds survive. `not-found` (`data.kind: "styleSheet"`) for an unknown name.

### `report.deleteStyleSheet` (destructive, configuration)

`{ name }` → `{ deleted, reset }`. A user style sheet is deleted (`reset: false`); a predefined one goes back to its
template's defaults (`reset: true`), which is what GnuCash does at its next start
(`5.17:gnucash/report/gnc-report.cpp:81-111`). Saved configurations naming a deleted style sheet render with the
base look (§8).

## Errors

The report range of [`../api.md`](../api.md):

| Code | Name | Raised by |
|------|------|-----------|
| 3000 | `report-script` | `run`, `export`: the script failed |
| 3001 | `report-limit` | `run`, `export`: a limit of §2 was passed (`data.limit`) |
| 3002 | `report-option` | any method taking `options`, `export`'s `styleSheet`, style sheet methods (the import methods report it per entry as `invalid-option`) |
| 3003 | `report-result` | `run`, `export`: the script returned an invalid result (`data.path`) |
| 3004 | `report-load` | `list` reports it under `problems` for each user script that does not load |

## CLI

```
red-ale exec --book books/home.gnucash report.list
red-ale exec --book books/home.gnucash --read-only report.run report="Balance Sheet" \
    'options:={"General": {"Balance Sheet Date": {"absolute": "2024-12-31"}}}'
red-ale exec --book books/home.gnucash --read-only report.export report="Balance Sheet" --output balance-sheet.html
red-ale exec --book books/home.gnucash --read-only report.exportCsv report="Net Worth Barchart" --output net-worth.csv
red-ale call report.saveConfiguration name="Year-end balance sheet" report="Balance Sheet" \
    'options:={"Accounts": {"Levels of Subaccounts": 2}}'
red-ale exec report.importConfigurations dryRun:=true --file ~/.local/share/gnucash/saved-reports-2.8
red-ale exec report.importStyleSheets --file ~/.local/share/gnucash/stylesheets-2.0
```

`--output <file>` (on `exec` with one method, and on `call`) writes the `html` or `csv` field of the result to the
file and prints `{ title, path, bytes }` instead of the result; a result with neither is a usage error (exit status
1), and `--output` with `--script` is refused. `--file <file>` (on `exec` with one method, and on `call`) reads the
file on the caller's side into the `text` param; giving `text` as well is a usage error. With `exec`, report methods never save the book: they never make it dirty.

## Implementation

- **The Configuration.** `red-ale-service::reports` reads and writes the `reports` keys of [`../reports.md`](../reports.md)
  §9 as plain values, keeping fields it does not know at every level; `red-ale-rpc` gives them their meaning (the
  report settings deserialize into `ReportSettings`, with GnuCash's defaults for what is unset). A malformed key is
  `invalid`. A relative `reports.directory` is taken from the Configuration file's directory.
- **The report directory** is read at every report method (`load_report_scripts`): every `*.rhai` below it, sorted by
  path, following a symbolic link only when it resolves inside the directory.
- **The registry** is built from the built-in library and those scripts, and cached: one entry for the process,
  reused while the user scripts' paths and texts and `reports.limits` are what it was built from, and rebuilt when any
  of them changes. Built-in scripts are compiled in, so nothing else can change it.
- **Reading the Book.** `report.run` and `report.export` use `Context::query` only, take now from the session's
  clock unless `now` is given, and the zone from the session. `report.options` and the configuration methods work
  with no book open: options are declared over an empty Book and read with no Book, so accounts can then be given only
  by GUID.
- **A saved configuration naming a deleted style sheet** still applies: that name is added to the choices of its
  General/"Stylesheet" option, and `report.export` renders it with the base look and the `missing_style_sheet` log
  entry (§8).
- **New GUIDs** for saved configurations come from the session's environment, or, with no book open, from the one a
  new session would get.

## Deviations from GnuCash

| What | GnuCash 5.17 | red-ale | Why |
|------|--------------|---------|-----|
| Where configurations and style sheets live | `saved-reports-2.8`, `stylesheets-2.0` (Scheme) | The Configuration (JSON); GnuCash's files are imported as data, never run | red-ale never runs Scheme |
| CSV export target | A file the GUI writes | CSV returned; only the CLI writes, on its side | The server never writes caller-named paths |
| Name clash on save | A number is appended | `conflict` | The caller gets the name it asked for |
| Invalid option value | Logged, option unchanged | `report-option` | A typo never runs a different report |
| `gnucash-cli --report run` failure | Exit status 0, error on standard error (`5.17:gnucash/gnucash-commands.cpp:234-250`) | An error response; the CLI exits with status 2 | Failures are visible |
| Export target | A file the GUI or CLI writes | HTML returned; only the CLI writes, on its side | The server never writes caller-named paths |

See [`../reports.md`](../reports.md) §14 for differences in the reports themselves.
