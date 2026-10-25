# API: reports

Listing, describing, running and exporting reports; saved report configurations; style sheets. Design:
[`../reports.md`](../reports.md), which this page refers to by section. Engine, result and renderer:
`crates/red-ale-report`; methods: `crates/red-ale-rpc/src/methods/reports.rs`; Configuration:
`crates/red-ale-service/src/config.rs`.

Reports only read the Book: `report.run` and `report.export` work on books open read-only, never make a book dirty
and never write the journal. Saved report configurations and style sheets live in red-ale's Configuration, as
GnuCash keeps them in files outside the book (`5.17:gnucash/report/gnc-report.h:35-36`); the methods that change
them have the **Effect target** `configuration` ([`../reports.md`](../reports.md) §10) and work with no book open.

Tests: `crates/red-ale-report/tests/` (engine, every report against its fixtures) and
`crates/red-ale-rpc/tests/reports.rs` (methods and errors).

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
| `script` | The script that defines it: `standard/balance-sheet.rhai`, or a user script's path relative to the report directory |
| `menuPath`, `menuName`, `menuTip`, `inMenu` | GnuCash's menu fields, kept for clients that build menus |

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
| `report.saveConfiguration` | write | configuration | Save a report's options under a new name |
| `report.updateConfiguration` | write | configuration | Change options of a saved configuration |
| `report.renameConfiguration` | write | configuration | Rename a saved configuration |
| `report.deleteConfiguration` | destructive | configuration | Delete a saved configuration |
| `report.styleSheets` | read | | Style sheet templates and style sheets, with their options |
| `report.createStyleSheet` | write | configuration | Create a style sheet from a template |
| `report.updateStyleSheet` | write | configuration | Change a style sheet's options |
| `report.deleteStyleSheet` | destructive | configuration | Delete a user style sheet, or reset a predefined one |

### `report.list` (read)

No params → `{ reports: ReportInfo[], problems: [{ source, code, message, line }] }`. Built-in templates, user
templates and saved configurations, sorted by name (bytewise), then GUID: GnuCash's `-R list` sorts by the locale
(`5.17:gnucash/report/report-core.scm:833-842`). Templates with `inMenu: false` are listed too, with the flag.
`problems` lists user scripts that failed to load (code `report-load`), so one bad file does not hide the others.
Needs no book.

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

→ `ReportResult`. Needs an open book (`no-book`). Errors: `not-found` (`data.kind: "report"`), `conflict` (an
ambiguous name), `report-option`, `report-script`, `report-limit`, `report-result`, `report-load` (the user script
no longer loads).

### `report.export` (read)

`{ report, options?, now?, styleSheet?: string }` → `{ title, html }`. Runs the report as `report.run` does and
renders it through the style sheet named by `styleSheet`, else by the report's General/"Stylesheet" option (§8).
The HTML is one self-contained document: no script, no external resource, the same bytes for the same book, options,
`now` and settings. **Nothing is written to disk**: the server never writes to a path a caller names. The CLI's
`--output` writes the `html` on the caller's side. Errors as `report.run`, and `report-option` for an unknown
`styleSheet`.

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

### `report.renameConfiguration` (write, configuration)

`{ configuration: ReportRef, name }` → `ReportInfo`. `conflict` when the name is taken, as GnuCash's dialog refuses
it (`5.17:gnucash/gnome/dialog-custom-report.c:462-494`).

### `report.deleteConfiguration` (destructive, configuration)

`{ configuration: ReportRef }` → `{ deleted: ReportInfo }` (`gnc:delete-report`,
`5.17:gnucash/report/report-core.scm:811-818`).

### `report.styleSheets` (read)

No params → `{ templates: [{ name, options: OptionInfo[] }], styleSheets: [{ name, template, predefined,
options: OptionInfo[] }] }`. The templates are Plain, CSS, Easy, Fancy, Footer and Head or Tail; the predefined style
sheets are `Default`, `CSS-based stylesheet (experimental)`, `Easy`, `Technicolor`, `Footer` and `Head or Tail`
(§8). Style sheets are sorted by name, bytewise.

### `report.createStyleSheet` (write, configuration)

`{ name, template, options?: Options }` → the style sheet. `conflict` when a style sheet has that name; `not-found`
for an unknown template; `report-option` for an invalid value, including CSS text containing `</style`, `@import` or
`url(`.

### `report.updateStyleSheet` (write, configuration)

`{ name, options: Options }` → the style sheet; `null` returns an option to its default. Works on predefined style
sheets too, as GnuCash lets the user edit `Default`.

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
| 3002 | `report-option` | any method taking `options`, `export`'s `styleSheet`, style sheet methods |
| 3003 | `report-result` | `run`, `export`: the script returned an invalid result (`data.path`) |
| 3004 | `report-load` | `run`, `export`, `options` on a user report whose script no longer loads; `list` reports it under `problems` |

## CLI

```
red-ale exec --book books/home.gnucash report.list
red-ale exec --book books/home.gnucash --read-only report.run report="Balance Sheet" \
    'options:={"General": {"Balance Sheet Date": {"absolute": "2024-12-31"}}}'
red-ale exec --book books/home.gnucash --read-only report.export report="Balance Sheet" --output balance-sheet.html
red-ale call report.saveConfiguration name="Year-end balance sheet" report="Balance Sheet" \
    'options:={"Accounts": {"Levels of Subaccounts": 2}}'
```

`--output <file>` writes the `html` field of the result to the file and prints `{ title, path, bytes }` instead of
the result. With `exec`, report methods never save the book.

## Deviations from GnuCash

| What | GnuCash 5.17 | red-ale | Why |
|------|--------------|---------|-----|
| Where configurations and style sheets live | `saved-reports-2.8`, `stylesheets-2.0` (Scheme) | The Configuration (JSON) | red-ale never runs Scheme |
| Name clash on save | A number is appended | `conflict` | The caller gets the name it asked for |
| Invalid option value | Logged, option unchanged | `report-option` | A typo never runs a different report |
| `gnucash-cli --report run` failure | Exit status 0, error on standard error (`5.17:gnucash/gnucash-commands.cpp:234-250`) | An error response; the CLI exits with status 2 | Failures are visible |
| Export target | A file the GUI or CLI writes | HTML returned; only the CLI writes, on its side | The server never writes caller-named paths |

See [`../reports.md`](../reports.md) §14 for differences in the reports themselves.
