# red-ale-report

Reports: the numbers they compute with, the options they take and the result they return.

## Role

GnuCash's reports are Guile Scheme. red-ale translates each one into a Rhai script that declares its options and
returns a structured **report result**, rendered to HTML on request (`design/reports.md`). This crate holds everything
about reports except I/O and the wire:

- **Exact numbers.** `Rational` is an exact rational of any size, so a report never fails where GnuCash's unbounded
  Scheme arithmetic prints a figure. A size cap turns a runaway computation into an error. `Monetary` pairs a Rational
  with a commodity.
- **GnuCash's figures, as GnuCash prints them.** Amounts are rounded only for display, to the commodity's SCU with
  banker's rounding, and printed as GnuCash prints them in the C locale its report tests use: `$116,006.33`,
  `30. FUNDS`, `$480 + 85/104`.
- **Options with GnuCash's semantics.** Every option type the reports and style sheets use, ordered and defaulted as
  GnuCash does. A bad value is refused with `report-option`, never quietly ignored. Relative dates are resolved through
  `red-ale-core`'s time zone functions.
- **A published result.** The report result carries data, not markup: sections, tables with row roles, exact figures
  with their display form, and charts. Its JSON Schema is part of the API and checked against a snapshot.

It does no I/O. Script sources, the report settings and the time zone come in as values.

## Where it fits

A bounded context of its own. `red-ale-core` supplies the Book and its reads; the report result is a published
language that `red-ale-rpc` returns unchanged.

```text
red-ale-core  ->  red-ale-report  ->  red-ale-rpc (report.* methods)
```

It never depends on `red-ale-service`, so it cannot reach a Session, the journal or the lock.

## What's inside

| Module     | Contents                                                                                               |
|------------|--------------------------------------------------------------------------------------------------------|
| `number`   | `Rational`, `Monetary`, `SizeCap`, `Rounding`; display rounding and amount formatting, `fixed`         |
| `options`  | `OptionDb` and the option types; setting values from JSON; `report.options` info and params schema      |
| `result`   | `ReportDraft` (what a script returns), `ReportResult`, validation, display filling, the JSON Schema     |
| `settings` | `ReportSettings`: report currency, reversed balances, date format, first day of the week                |
| `error`    | `ReportError` with the kinds `report-script`, `report-limit`, `report-option`, `report-result`, `report-load` |
| `limits`   | The limits of a report run and their defaults                                                           |
| `wire`     | Wire forms of GUIDs, commodities and timestamps                                                          |
| `engine`   | The Rhai engine for a run, its limits and module resolver; the template `Registry`; running a template   |
| `query`    | The query API: read-only Rhai handles over a snapshot of the Book                                        |
| `library`  | The built-in `.rhai` scripts, compiled in from `scripts/`                                                |

The HTML renderer will join these modules.

## Usage

Declare options, apply a client's changes, and finish a script's draft into a result:

```rust
use red_ale_report::options::{Classifier, OptionDb, ValueContext};
use red_ale_report::result::{DisplayContext, ReportDraft, RunInfo};

let mut db = OptionDb::new();
db.end_date(Classifier::new("General", "Balance Sheet Date", "c", "Select a date to report on."), true)?;
db.add_general_options("Balance Sheet", &["Default"])?;
db.apply(&params["options"], &ValueContext { book: Some(&book), zone: &zone })?;

let draft = ReportDraft::from_json(&script_output)?;
let result = draft.finish(run_info, &DisplayContext { book: &book, zone: &zone, date_format })?;
```

Load the built-in and user scripts once, then list, describe and run templates:

```rust
use red_ale_report::engine::{Registry, RunContext, ScriptSource};
use red_ale_report::library::Library;
use red_ale_report::limits::Limits;

let user = vec![ScriptSource { path: "mine/report.rhai".into(), text }];
let registry = Registry::load(&Library::builtin(), &user, Limits::default())?;
let failed = registry.problems(); // user scripts that did not load: report-load or report-limit
let template = registry.find("Balance Sheet")?; // Lookup::NotFound or Lookup::Ambiguous(guids)
let ctx = RunContext { book: &book, now, zone: &zone, settings: &settings, accounting_period: &period, style_sheets: &["Default"] };
let db = registry.options(template, &ctx)?; // report.options
let result = registry.run(template, &[&saved, &params["options"]], None, &ctx)?;
```

The registry holds only script texts and template records, so it can be shared between threads; every call builds
its own engine. Nothing in it reads files: the service hands in each user script's path below the report directory
and its text.

## Adding a built-in script

Put the file below `scripts/`, in `standard/` or `example/` for a report and `lib/` for a shared module, as GnuCash
lays them out, and add its path to `BUILTIN` in `src/library/mod.rs`:

```rust
const BUILTIN: &[(&str, &str)] = scripts!["lib/dates.rhai", "standard/balance-sheet.rhai"];
```

A built-in template with a GnuCash name or GUID must have both, as `GNUCASH_TEMPLATES` lists them; a test checks it.

Regenerate the result schema snapshot after a deliberate change with
`UPDATE_SNAPSHOTS=1 cargo test -p red-ale-report --test result_schema`, and review the diff.
