# Reports

How red-ale runs reports: where the engine lives, how Rhai is set up, the numbers scripts compute with, the query
API they read the Book through, options, the report result, rendering, saved report configurations, and the plan for
translating each GnuCash 5.17 report and testing it. Scope and requirements are in [`milestone2.md`](milestone2.md),
"Report scripting"; the methods are in [`api/reports.md`](api/reports.md); the terms are in
[`glossary.md`](glossary.md), "Reports".

Citations are to the 5.17 tree. In them, `report/` stands for `5.17:gnucash/report/`, `standard/` for
`5.17:gnucash/report/reports/standard/` and `example/` for `5.17:gnucash/report/reports/example/`.

## In short

- A **report script** is a `.rhai` file defining one or more **report templates**. Built-in scripts translate
  GnuCash's `.scm` files and are compiled into the binary; user scripts load from the **report directory** named in
  the Configuration.
- A template declares its **report options** and builds a **report result**: data (sections, tables, charts) with
  exact figures. `report.run` returns the result as JSON; `report.export` returns HTML rendered from it through a
  **style sheet**, with charts as inline SVG and no script.
- Scripts compute with **Rational**, an exact rational with no size limit, and read the Book only through the
  read-only **query API**. They cannot reach a Mutation, a file, the network, the environment or the clock.
- **Saved report configurations** and user style sheets live in red-ale's Configuration, never in the Book. Methods
  that change them have the **Effect target** `configuration`.
- The engine and renderer are a new bounded context, the crate `red-ale-report`, which depends only on
  `red-ale-core`.

## 1. Engine boundaries

### The crate

`red-ale-report` holds everything about reports except I/O and the wire:

| Module    | Job                                                                                                    |
|-----------|--------------------------------------------------------------------------------------------------------|
| `number`  | `Rational` and `Monetary`, display rounding and GnuCash's amount formatting (section 3)                |
| `query`   | The query API: read-only handles over a Book snapshot (section 5)                                      |
| `options` | Option declarations, values, validation and the published option schema (section 6)                    |
| `result`  | The report result types, their validation and their JSON form (section 7)                              |
| `engine`  | The Rhai engine: setup, limits, module resolver, the template registry, running a template (section 2) |
| `library` | The built-in `.rhai` files, compiled in with `include_str!`                                            |
| `render`  | HTML and SVG from a report result and a style sheet (section 8)                                        |

The renderer depends only on `result` and `number`, never on the engine or the query API, so a result a client got
as JSON renders the same way.

The engine's Rust API is small: `Registry::load(library, user_scripts, limits)` compiles the built-in and user
scripts and collects their templates (`templates()`, `find(reference)`, `problems()` for the user scripts that did
not load); `Registry::options(template, run_context)` declares a template's options for `report.options`; and
`Registry::run(template, changes, configuration, run_context)` runs one. The registry is plain data, shared between
threads; each call builds its own engine over its own snapshot of the Book.

The crate depends on `red-ale-core`, `rhai`, `num-bigint`, `num-rational`, `num-traits`, `num-integer`, `serde`,
`serde_json`, `schemars`, `chrono` (RFC 3339 timestamps on the wire, as `red-ale-rpc` writes them) and
`serde_path_to_error` (the path of a `report-result` mismatch). It does no I/O: script sources, the report settings and the time zone come in as
values. It denies `clippy::float_arithmetic`, as core and the backends do, and has `#![forbid(unsafe_code)]`.

`red-ale-rpc` gains the `report.*` methods and depends on `red-ale-report`. `red-ale-service` gains the
Configuration keys of section 9 and the loader for the report directory, which reads the `.rhai` files and hands
their text to the engine, so file access stays where it already is. `red-ale-report` never depends on
`red-ale-service`, so it cannot reach a Session, the journal or the lock.

### Read-only by construction

- A report runs on a **snapshot**: the method takes `&Book` through `Context::query` and clones it. A Book clone
  copies one `Arc` per entity ([`architecture.md`](architecture.md), "Mutations are atomic by copy-on-write"), and
  the snapshot is held in an `Rc` that only the query handles see.
- Every Mutation in core takes `&mut Book`. The report crate never holds one: query handles return values and `&`
  references, and `Book` has no interior mutability. A script has no path to a Mutation, and the compiler checks it.
- Report methods never call `Context::mutate`. Only `Session::try_mutate` marks the Session dirty and writes the
  journal (`crates/red-ale-service/src/session.rs`), so running a report can do neither.
- Tests (section 13) run every built-in template on every report fixture and check that the Book compares equal to
  its state before, `book.info` reports `dirty: false`, and no journal file appeared.

### Context map

```
   red-ale-core ─────────────► red-ale-report    customer/supplier: core supplies the Book and its reads
        │                           │
        ▼                           ▼
   red-ale-service ──────────► red-ale-rpc       report.* methods: snapshot, settings, run, render
   (Configuration, report directory)
```

- **core → report:** customer/supplier. The query API exposes core's reads under glossary names. A read a report
  needs that core lacks (the indirect price lookup, balances accumulated at dates) is added to core, not written in
  the report crate.
- **report → rpc:** the report result is a published language owned by `red-ale-report` (its types derive `serde`
  and `schemars`). rpc returns it unchanged from `report.run` and lists its schema in `api.schema`.
- **GnuCash's Scheme → Rhai:** a translation, kept close enough to compare line by line (`milestone2.md`,
  "Translating the reports").

## 2. The Rhai engine

### Build

`rhai` 1.x (1.27.0 when this was written), pinned in `[workspace.dependencies]` with `default-features = false` and
the features `std`, `no_float`, `no_time` and `serde`. Never `unchecked`, `sync`, `internals`, `debugging` or
`decimal`. `cargo deny` must pass with Rhai's dependency tree before the engine ticket lands.

- `no_float`: no float type exists and a float literal is a syntax error.
- `no_time`: Rhai's `timestamp()` does not exist; time comes only from the run context.
- `serde`: turns the map a template returns into the typed report result (`rhai::serde::from_dynamic`).
- Not `sync`: each run builds its own `Engine` and ASTs on one thread; nothing is shared between runs.

### The engine for a run

`Engine::new_raw()`, which has no resolver and no `print` target, plus Rhai's `StandardPackage` (strings, arrays,
maps, logic and integer arithmetic; its time and float functions are compiled out by the features above). Then:

- `disable_symbol("eval")`, so a script cannot compile code at run time. `sleep`, a function and not a symbol, is
  overridden by one that fails, so a script cannot hold a thread while using no operations.
- `set_strict_variables(true)` and `set_fail_on_invalid_map_property(true)`, so a misspelt variable or map key is an
  error rather than a silent `()`.
- `on_print` and `on_debug` append to the **report log** (`{ level: "print" | "debug", text, source, line }`)
  instead of standard output, which is the MCP stdio channel. The log comes back in the result, and in the `data`
  of a script error. It holds at most the string-length limit in bytes of text; past that one last entry says the
  log was cut.
- `set_module_resolver` with red-ale's resolver (below).
- The types and functions of sections 3, 5 and 6 are registered, and `memo()`; nothing else is. A `Memo` is a
  table from integers to values that every copy shares, so a value a function stores is there for the next call:
  Rhai's arrays and maps are copied on assignment, and GnuCash's memoised closures (`case-exchange-time-fn`,
  `report/commodity-utilities.scm:829-887`) need a cache that outlives one call. `m.get(k)` (`()` when absent),
  `m.contains(k)`, `m.set(k, v)` and `m.len()`; it holds at most the map-size limit of entries (`map-size`).
- The limits below are set, and `on_progress` stops the run once the run time limit passes.

### Limits and their errors

Defaults; each can be changed in the Configuration (`reports.limits`, section 9).

| Limit              | How                          | Default              | `data.limit`       |
|--------------------|------------------------------|----------------------|--------------------|
| Operations         | `set_max_operations`         | 1,000,000,000        | `operations`       |
| Run time           | `on_progress`, host clock    | 120 s                | `time`             |
| Call depth         | `set_max_call_levels`        | 64                   | `call-depth`       |
| Expression depth   | `set_max_expr_depths`        | 128 global, 64 in functions | `expression-depth` |
| String length      | `set_max_string_size`        | 1 MiB                | `string-size`      |
| Array length       | `set_max_array_size`         | 10,000,000           | `array-size`       |
| Map size           | `set_max_map_size`           | 1,000,000            | `map-size`         |
| Modules            | `set_max_modules`            | 64                   | `modules`          |
| Variables          | `set_max_variables`          | 10,000               | `variables`        |
| Functions          | `set_max_functions`, per script | 1,000             | `functions`        |
| Rational size      | red-ale's own check          | 4,096 bits per part  | `number-size`      |

The operation limit is deterministic. The run time limit is not, but it only turns a run into an error, never into
different output. Rhai's data-size limits stand in for a memory limit; the Rational size check keeps one runaway
computation from doing the same. GnuCash's exact sums stay far below 4,096 bits.

The Rational size check raises Rhai's `ErrorDataTooLarge`, so, like every other limit, a script's `try … catch` cannot
catch it. The expression-depth and function limits also apply when a script compiles; a user script that passes one
at load time is listed with the scripts that did not load, as `report-limit`. Rhai checks the map size when maps are
merged (`m += other`), assigned to a property or returned, but not when `m[key] = v` adds a key; the operation limit
bounds that case.

New error codes, in a range for reports (added to [`api.md`](api.md)):

| Code | Name            | When                                                                                         |
|------|-----------------|----------------------------------------------------------------------------------------------|
| 3000 | `report-script` | The script failed at run time: `throw`, a type error, division by zero, Rhai's `ErrorArithmetic`, an unknown function, variable or module. `data`: `{ report, source, line, column, message, log }` |
| 3001 | `report-limit`  | The script passed a limit: `ErrorTooManyOperations`, `ErrorTerminated`, `ErrorStackOverflow`, `ErrorDataTooLarge`, `ErrorTooManyModules`, `ErrorTooManyVariables`, a compile-time depth or function limit, or the Rational size check. `data`: `{ report, source, limit, max, log }`; `source` is the script, or `null` when not known |
| 3002 | `report-option` | An option value is not valid: unknown section or name, wrong type, not one of the choices, out of range, unknown account or commodity, or a style sheet that does not exist. `data`: `{ section, name, reason }` |
| 3003 | `report-result` | The template returned something that is not a valid report result. `data`: `{ report, path, reason, log }` |
| 3004 | `report-load`   | A user script does not compile, or its `templates()` fails, is malformed or repeats a GUID already defined, or it has `templates()` without `options` and `render`. `data`: `{ source, line, column, message }` |

A built-in script that fails to compile, declares a malformed or repeated template, or returns an invalid result is a
bug: `internal-error` (-32603, the JSON-RPC code), and the tests of section 13 catch it before release.

A script with no `templates()` function is a module that other scripts import, not a failure to load.

### The module resolver

Rhai's file resolver, which would let `import` read any path, is replaced by red-ale's, which knows only the
scripts handed to the engine:

- `import "lib/<name>"` loads a built-in shared module (section 12), for example `lib/exchange`.
- `import "standard/<name>"` and `import "example/<name>"` load a built-in report script.
- `import "./<path>"` loads a user script relative to the importing one, inside the report directory. `..`,
  absolute paths, a path that leaves the directory after resolving symlinks, and `./` from a built-in script are
  refused as "module not found" (`report-script`). An import that comes back to a script still loading is refused
  the same way.

A user script's source name is its path below the report directory with `./` in front (`./mine/report.rhai`); a
built-in's is its path in the library (`standard/balance-sheet.rhai`). Errors, the log and `TemplateInfo.script`
use these names. The engine never touches the file system: the service reads the directory, resolves symlinks and
leaves out what escapes it, and hands the engine path and text pairs.

The service's loader reads the report directory (`reports.directory`, section 9) each time a report method runs:
every `*.rhai` file below it, at most 1,000 files of at most 1 MiB each. Edits are seen without a restart.

### Now, today and the time zone

The run context carries:

- **now**: the Session's clock (`Env::now`, so tests use `FixedEnv`), or the optional `now` param of `report.run`
  and `report.export`, which lets a caller reproduce an earlier run exactly;
- **the local time zone**, as `milestone2.md`, "Time zones", defines it. The engine never reads the environment;
- the **report settings** (section 9) and the **accounting period**.

Relative date options ("start of this month", "end of accounting period") are resolved by the host from now, the
zone and the accounting period before the script sees them, as `gnc-relative-date-to-time64` does
(`5.17:libgnucash/engine/gnc-option-date.cpp:518-597`). A script sees Time64 values only. The same Book, options,
now, zone and settings give the same result, byte for byte.

### The shape of a report script

A script is a Rhai module with three functions:

```rhai
// The header, author and licence lines of the .scm it translates, then red-ale's, then
// "Translated from 5.17:gnucash/report/reports/standard/balance-sheet.scm".
import "lib/options" as opts_util;
import "lib/acct-table" as acct_table;

fn templates() {
    [#{ guid: "c4173ac99b2b448289bf4d11c731af13", name: "Balance Sheet",
        menuPath: ["_Assets & Liabilities"], params: #{} }]
}

fn options(params, o, book) {
    o.string("General", "Report Title", "a", "Title for this report.", "Balance Sheet");
    opts_util::add_report_date(o, "General", "Balance Sheet Date", "c");
    // ...
}

fn render(params, book, opts, ctx) {
    let report_date = ctx.end_of_day(opts.get("General", "Balance Sheet Date"));
    // ...
    #{ title: title, sections: [#{ blocks: [table] }] }
}
```

- `templates()` returns the template records: `guid` (32 lowercase hex digits), `name`, and optionally `menuPath`,
  `menuName`, `menuTip`, `inMenu` (default true) and `params`, the values that tell one script's templates apart.
  GnuCash passes these as arguments to a shared options generator and renderer, for example
  `standard/account-piecharts.scm:573-600`; Rhai functions do not capture, so they come in as `params`.
- `options(params, o, book)` declares the options through the builder `o` (section 6). `book` is the open Book, or
  an empty Book with only a root account when none is open, so defaults can be computed from it as GnuCash's
  generators do.
- `render(params, book, opts, ctx)` returns the report result as a map (section 7).

The engine adds GnuCash's three General options to every template (section 6).

## 3. Numbers

### Rational and Monetary

GnuCash hands every amount to Scheme as an exact, reduced rational (`gnc_numeric_to_scm` divides numerator by
denominator, `5.17:bindings/guile/gnc-engine-guile.cpp:1729-1734`), and the report helpers add and multiply them with
Scheme's unbounded arithmetic: the commodity collector adds with plain `+` (`report/report-utilities.scm:289-296`)
and never rounds. A 64-bit `Numeric` would overflow where GnuCash prints a figure, so reports get their own number:

- **Rational**: an exact rational with arbitrary-precision numerator and denominator, always reduced, denominator
  positive. Implemented as `num_rational::BigRational` (`Ratio<num_bigint::BigInt>`) from `num-rational` 0.4 and
  `num-bigint` 0.4, both `MIT OR Apache-2.0`, which `deny.toml` allows. They are the most used bigint crates and
  `num-traits` is already in Rhai's tree. `malachite` (LGPL-3.0) is not on the allow-list; `dashu` is faster but
  younger, and report sums are small.
- **Monetary**: a Rational with a Commodity, GnuCash's `gnc:monetary` (`5.17:bindings/guile/gnc-numeric.scm:81-91`).

A Rational never enters the Book. Values come out of the query API as Rationals (`Numeric` converts exactly) and go
into the report result as Rationals; the wire form is `{ "num": "…", "denom": "…" }` with decimal strings of any
length.

Operations registered for scripts:

| Operation | Meaning |
|-----------|---------|
| `rational(n)`, `rational(n, d)`, `rational("12.34")`, `rational("1234/100")` | Construct from an integer, a pair, or text (the forms the wire accepts). There is no float to construct from |
| `+ - * /`, unary `-`, `==`, `!=`, `<`, `<=`, `>`, `>=` | Exact, mixing Rational and integer freely. Division by zero is a `report-script` error, as `(/ x 0)` is in Scheme |
| `abs`, `sign`, `is_zero`, `is_integer`, `num`, `denom`, `min`, `max` | `num` and `denom` return Rationals, since either may pass 64 bits |
| `r.round()` | To the nearest integer, ties to even: Scheme's `round` on an exact number, which `standard/category-barchart.scm:309` uses |
| `r.round_to(denom, mode)` | `gnc-numeric-convert`: to a denominator with a GnuCash rounding mode: `"round"` (banker's, `GNC-RND-ROUND`), `"half-up"`, `"half-down"`, `"floor"`, `"ceiling"`, `"truncate"`, `"promote"` (`5.17:libgnucash/engine/gnc-numeric.h:91-97`) |
| `r.to_decimal(places)` | Text with exactly `places` decimals, rounded half-up; for labels |
| `fixed(r, width, places)` | `width` may be `()` for no padding. Guile's `~w,dF`: rounds half-up to `places` and pads to `width`, dropping the leading `0` of a value below 1 when the width allows no more (`.5`), as `standard/account-piecharts.scm:552-554` relies on |
| `monetary(commodity, r)`, `m.commodity`, `m.amount`, `m + m`, `m - m`, `-m`, `m.round_scu()` | Sums of two commodities are a `report-script` error. `round_scu` rounds to the commodity's SCU with banker's rounding, the `round-scu` of `standard/account-piecharts.scm:497-500` |
| `m.to_string()` | `gnc:monetary->string` (`report/report-utilities.scm:143-146`): the exact value, no rounding, with the commodity's symbol |

GnuCash crosses back to `gnc_numeric` in a few places (`gnc-numeric-mul` in the exchange functions, `gnc-numeric-convert`
in `gnc:sum-collector-commodity`), and `gnc_scm_to_numeric` fails there when a part passes 64 bits
(`5.17:bindings/guile/gnc-engine-guile.cpp:1709-1727`). red-ale keeps those values exact. Within 64 bits the figures
are the same; beyond it, GnuCash fails or prints an empty cell and red-ale prints the figure (section 14).

### Display rounding and formatting

A figure is rounded only to be shown, never before it is used again. The host fills in the display form of every cell
(section 7); scripts do not format money.

- **Monetary cells**: GnuCash's monetary renderer rounds to the commodity's SCU with `GNC-HOW-RND-ROUND`, banker's
  rounding, then prints with the commodity's print info (`report/html-style-info.scm:224-231`;
  `5.17:libgnucash/engine/gnc-rational-rounding.hpp:197-209`). Note the commodity's SCU, not the account's.
- **Price cells** (the exchange-rates table): `gnc-price-print-info` of the report currency: decimal places from the
  currency's fraction, no forced rounding (`5.17:libgnucash/app-utils/gnc-ui-util.cpp:1025-1057`). A rate that is not
  a terminating decimal prints as `$480 + 85/104`, as GnuCash's own test expects
  (`standard/test/test-balsheet-pnl.scm:335`).
- **Number cells**: rounded to two places with banker's rounding, GnuCash's number renderer
  (`report/html-style-info.scm:233-236`), which goes through a double there and exactly here.

The text form is `xaccSPrintAmount` and `PrintAmountInternal` (`5.17:libgnucash/app-utils/gnc-ui-util.cpp:1083-1430`)
under the conventions GnuCash falls back to when no locale is set (`5.17:libgnucash/core-utils/gnc-locale-utils.c:75-108`),
which are those of its own report tests (`setlocale LC_ALL "C"`, `standard/test/test-average-balance.scm:15`):

- grouping by three with `,`, decimal point `.`, minus sign first: `-$1,234.56`;
- a currency (namespace `CURRENCY`) has its symbol first with no space, and as many decimals as its SCU, padded;
- any other commodity has its symbol after the number with a space, and no padding of decimals; the decimal point is
  always printed, so a whole amount of a 1/1000 commodity is `30. FUNDS` (`standard/test/test-balsheet-pnl.scm:247`);
- a remainder that is not a decimal fraction prints as `w + n/d`;
- the symbol is the commodity's *nice symbol*: its user symbol, else the ISO table's symbol, else its mnemonic
  (`5.17:libgnucash/engine/gnc-commodity.cpp:1095-1115`). GnuCash also prefers the locale's own symbol for the
  locale's currency; red-ale has no locale, and the ISO table gives the same `$` for USD.

Formatting is red-ale's only locale. Clients that want another take the exact `amount` and the `rounded` form from
the cell and format them themselves.

## 4. Settings reports read

Some figures depend on GnuCash user preferences, which are not in the Book. red-ale reads them from its
Configuration (section 9), with GnuCash's out-of-the-box values as defaults, and passes them in the run context:

| Setting (`reports.*`)     | GnuCash preference                                     | Default    | Used by |
|---------------------------|--------------------------------------------------------|------------|---------|
| `currency`                | `general.report` `currency-choice-other`/`currency-other`, else the locale's currency (`gnc_default_report_currency`, `5.17:libgnucash/app-utils/gnc-ui-util.cpp:748-822`) | `CURRENCY::USD` (GnuCash's fallback, `:724-745`) | the default of every "Report's currency" option |
| `reverseBalancedAccounts` | `general` `reversed-accounts-{none,credit,incomeexpense}` (`5.17:libgnucash/app-utils/gnc-ui-util.cpp:128-173`) | `credit`: LIABILITY, PAYABLE, EQUITY, INCOME and CREDIT (not CREDITLINE or TRADING), the schema default (`5.17:gnucash/gschemas/org.gnucash.GnuCash.gschema.xml.in:93-107`) | account rows of the statement tables (`report/html-acct-table.scm:1029-1030`) |
| `dateFormat`              | `general` `date-format` (`5.17:libgnucash/engine/gnc-date.cpp:504-521`) | `locale`   | every printed date (`qof-print-date`) |
| `weekStart`               | the locale's first day of the week (`gnc-start-of-week`, `5.17:libgnucash/engine/gnc-date.cpp:195-215`) | `sunday` | week fractions in averages (`5.17:bindings/guile/date-utilities.scm:244-251`) |
| `accountingPeriod` (existing key, [`api/accounts.md`](api/accounts.md)) | `window.pages.account-tree.summary` | this calendar year | the accounting-period date choices |

`dateFormat` is one of `us` (`%m/%d/%Y`), `uk` (`%d/%m/%Y`), `ce` (`%d.%m.%Y`), `iso` (`%Y-%m-%d`) and `locale`.
red-ale has no locale; `locale` means what GnuCash prints under the C locale its report tests use, `%m/%d/%y`
(`standard/test/test-standard-category-report.scm:107-113` reads that form). A date interval in a title is
`<from> to <to>`, GnuCash's form for every format but `locale`, where it asks ICU instead
(`5.17:libgnucash/engine/gnc-date.cpp:618-680`).

Book options come through the query API instead, since they are in the Book: "Use Trading Accounts"
(`5.17:libgnucash/engine/qofbook.cpp:921-928`), the business company name (`5.17:bindings/guile/options.scm:409-411`)
and the fiscal year end (`fy_end`).

GnuCash's own report tests run with no preferences at all, so they reverse no accounts
(`5.17:libgnucash/app-utils/gnc-ui-util.cpp:128-149`): their fixtures set `reverseBalancedAccounts: "none"`.

## 5. The query API

Scripts read the Book through handles registered as Rhai types. Names are the glossary's; properties carry the field
names of the JSON-RPC read results (`AccountInfo`, `PriceInfo`, …); functions are snake case. A function mirrors a
read method where one exists; the rest are reads GnuCash's reports need that no client has asked for yet. Each is
listed with the GnuCash call it stands for, so a translation maps one to one.

Every handle holds the Book snapshot; none can change it. Lists come back as Rhai arrays in a fixed order; nothing
comes back in hash order.

### Book

| Function | Returns | Mirrors | GnuCash |
|----------|---------|---------|---------|
| `book.root()` | Account | `account.tree` | `gnc-get-current-root-account` |
| `book.account(ref)` | Account or `()` | `account.get` | lookup by GUID or full name |
| `book.accounts()` | Account[], book order | `account.list` | |
| `book.accounts_of_types(types)` | Account[], depth first in display order | | `gnc:filter-accountlist-type` over `gnc-account-get-descendants-sorted` (`report/report-utilities.scm:178-181`), the default of most account options |
| `book.commodity(id)` | Commodity or `()` | `commodity.get` | |
| `book.uses_trading_accounts()` | bool | `book.info` | `qof-book-use-trading-accounts` |
| `book.company_name()` | string, `""` when unset | | `gnc:company-info` |
| `book.prices(commodity, currency)` | Price[], newest first, ties by GUID | `price.list` | `gnc-pricedb-get-prices`, forward only (`5.17:libgnucash/engine/gnc-pricedb.cpp:2047-2058`) |
| `book.price_rate(commodity, currency, when, mode)` | Rational | `price.nearest`, `price.latest` | `gnc-pricedb-get-{nearest,nearest-before,latest}-price`: 1 for the same commodity; a direct price, inverted when quoted the other way; else an indirect price through a commodity both are priced in; 0 when there is none (`:2408-2527`) |
| `book.price_convert(monetary, currency, when, mode)` | Monetary | | `gnc-pricedb-convert-balance-*`: amount × `price_rate`, rounded to the currency's SCU with banker's rounding; 0 when no price (`:2529-2586`) |
| `book.commodity_splits(accounts, until, commodity, sorted)` | Split[] | | `gnc-get-match-commodity-splits`: non-void splits of `accounts` up to `until` (inclusive) whose transaction currency differs from their account's commodity and, unless `commodity` is `()`, one of the two is `commodity` (`5.17:bindings/engine.i:167-193`). Unsorted, account after account, each in split order (`get-exchange-totals` relies on it, CU:447); `sorted`, all in split order. Callers pass the root's sorted descendants (CU:203-205) or those holding shares (`standard/price-scatter.scm:147-149`) |
| `book.split_query(q)` | Split[] in split order | `transaction.find` | the split query of `gnc:account-get-trans-type-splits-interval` (`report/report-utilities.scm:692-725`), which the Average Balance report also runs (`example/average-balance.scm:300-345`): `q` is `#{ accounts, from, to, description: #{ text, cased, regexp }, closing }`; void splits never match (both GnuCash queries leave them out); `description` and `closing` are OR'ed, as there |
| `is_euro_currency(c)`, `euro_convert(m, currency)` | bool, Monetary or `()` | `currency.euroConvert` | `gnc-is-euro-currency`, `gnc-convert-to-euro`/`-from-euro` (`5.17:libgnucash/engine/gnc-euro.cpp:31-106`); `()` unless both are euro currencies, as `gnc:exchange-by-euro` (`report/commodity-utilities.scm:635-644`). `c` may be a currency the Book does not hold |

`mode` is `"nearest"`, `"before"` or `"latest"`; `when` is a Time64. `"latest"` keeps GnuCash's split behaviour:
the direct lookup takes the newest price even if it is in the future, the indirect one the newest at or before now
(`5.17:libgnucash/engine/gnc-pricedb.cpp:1731-1750`, `:1955-1960`).

### Account

Properties: `guid`, `name`, `fullName`, `type`, `typeString` (the type's label, "Asset", as
`xaccAccountGetTypeStr` gives it), `code`, `description`, `notes`, `placeholder`, `hidden`, `commodity` (Commodity),
`parent` (Account or `()`), `depth` (levels below the root).

| Function | Returns | GnuCash |
|----------|---------|---------|
| `a.children()`, `a.descendants()` | Account[], book order | `gnc-account-get-children`, `gnc-account-get-descendants` |
| `a.children_sorted()`, `a.descendants_sorted()` | Account[], display order (`account.tree` with `sorted`) | `gnc-account-get-children-sorted`, `-descendants-sorted` (`5.17:libgnucash/engine/Account.cpp:2957-2961`, `:3040-3045`) |
| `a.tree_depth()` | integer, counting the account itself as 1 | `gnc-account-get-tree-depth` (`:3017-3029`) |
| `a.splits()` | Split[] in split order | `xaccAccountGetSplits` |
| `a.splits_between(from, to)`, `a.splits_between(from, to, descendants)` | Split[], both ends inclusive; `()` leaves an end open; with `descendants` true, the sub-accounts' splits too | `gnc-account-foreach-split-between-dates` (`5.17:libgnucash/engine/Account.hpp:51-85`) |
| `a.balance_as_of(t)` | Rational | `xaccAccountGetBalanceAsOfDate`: splits posted strictly before `t` (`Account.cpp:3505-3524`); `account.balance` |
| `a.accumulate_at(times, what)` | (Rational or `()`)[] | `gnc-account-accumulate-to-dates` (`5.17:bindings/engine.i:196-236`): for each time, `what` (`"balance"`, `"noclosingBalance"`, `"clearedBalance"` or `"reconciledBalance"`) of the last split posted at or before it, `()` before the first split |
| `a.reverse_balance(ctx)` | bool | `gnc-reverse-balance` under `reports.reverseBalancedAccounts` |

### Split, Transaction, Commodity, Price

- **Split**: `guid`, `account`, `transaction`, `amount`, `value`, `memo`, `action`, `reconcile`, `reconcileDate`,
  `balance`, `noclosingBalance` (`xaccSplitGetNoclosingBalance`, `5.17:libgnucash/engine/Split.cpp:1322-1325`),
  `sharePrice`.
- **Transaction**: `guid`, `currency`, `datePosted` (Time64), `dateEntered`, `num`, `description`, `notes`,
  `isClosing` (the `book_closing` slot), `isVoid`, `splitCount`, `splits()`.
- **Commodity**: `id` (`NAMESPACE::MNEMONIC`), `namespace`, `mnemonic`, `fullname`, `fraction` (SCU), `isCurrency`,
  `symbol` (the nice symbol of section 3), `uniqueName`.
- **Price**: `guid`, `commodity`, `currency`, `time`, `source`, `type`, `value`.

"Split order" is `xaccSplitOrder` (`5.17:libgnucash/engine/Split.cpp:1536-1597`) and "display order" is
`xaccAccountOrder` (`Account.cpp:2363-2413`), both as v0.1 implements them, with names compared bytewise
([`api/accounts.md`](api/accounts.md)).

### Core reads behind the query API

Each function reads the Book through one `red_ale_core` read; the report crate adds no Book logic of its own. Reads
marked *new* were added for reports; `oracle_report_reads.rs` checks them against GnuCash 5.17 on one book, with the
num field source off and on (`crates/red-ale-testkit/tests/oracle_report_reads.rs`).

| Query function | Core read | |
|----------------|-----------|---|
| `book.root()`, `book.account(ref)`, `book.accounts()` | `Book::root`, `Book::account`, `Book::account_by_full_name`, `Book::accounts` | |
| `book.accounts_of_types(types)` | `ops::accounts::descendants_sorted` of the root, filtered by type | new |
| `book.commodity(id)` | `ops::commodities::lookup` | |
| `book.uses_trading_accounts()` | `book_options::uses_trading_accounts` | |
| `book.company_name()` | `book_options::company_name` (`options/Business/Company Name`) | new |
| `book.prices(commodity, currency)` | `ops::prices::prices(book, commodity, Some(currency), false)`; a date range is a filter on `time`, as the Price Scatterplot filters (`standard/price-scatter.scm:220-229`) | |
| `book.price_rate(…)` | `ops::prices::convert::rate` with `PriceAt::{Nearest, Before, Latest { now }}` | new |
| `book.price_convert(…)` | `ops::prices::convert::convert_balance`, after the Monetary's Rational is made a Numeric (past 64 bits GnuCash fails at the same place, section 3) | new |
| `book.commodity_splits(…)` | `ops::transactions::find::cross_commodity_splits` | new |
| `book.split_query(q)` | `ops::transactions::find::period_splits` with a `PeriodQuery` | new |
| `is_euro_currency`, `euro_convert` | `currencies::is_euro_currency`, `ops::commodities::euro_convert` | |
| `a.children()`, `a.descendants()` | `Book::children`, `Book::descendants` | |
| `a.children_sorted()`, `a.descendants_sorted()` | `ops::accounts::sorted_children`, `ops::accounts::descendants_sorted` | new (descendants) |
| `a.tree_depth()`, `depth` | `ops::accounts::tree_depth`, `ops::accounts::depth` | new |
| `a.splits()` | `ops::balances::account_splits` | new |
| `a.splits_between(from, to)` | `ops::balances::account_splits_between` (with descendants for `gnc:account-get-comm-value-interval`) | new |
| `a.balance_as_of(t)` | `ops::balances::balance_as_of(…, Total::Balance)` | new |
| `a.accumulate_at(times, what)` | `ops::balances::running_totals_at` with `Total::Balance` or `Total::Noclosing`; `accumulate_to_dates` for any other element | new |
| `a.reverse_balance(ctx)` | none: a report setting (section 4), not Book data; stays in `red-ale-report` | |
| Split `balance`, `noclosingBalance` | `ops::balances::running_balances` (`RunningSplit::running`) | new |
| Split `sharePrice` | `ops::transactions::find::share_price` | |
| Transaction `isClosing`, `isVoid` | `Transaction::is_closing`, `Transaction::is_void` | |
| Commodity `symbol` | `ops::commodities::display_symbol` | |
| `ctx.accounting_period()` | `ops::period::period_bounds` | |

Core also has the reads `lib/balances` and clients may want beyond these: `balances_at_dates`
(`gnc:account-get-balances-at-dates`, the sum of amounts at or before each time, which counts the stock splits the
engine skips), `balance_as_of_in_commodity` (`xaccAccountGet{,Noclosing}BalanceAsOfDateInCurrency`, sub-accounts
converted at the price before), `balance_change` (`xaccAccountGet{,Noclosing}BalanceChangeForPeriod`) and the
any-currency lookups `nearest_any_currency` and `latest_before_any_currency`. No JSON-RPC method is added: every
query function that mirrors a method mirrors one that exists.

Behaviour the oracle confirms and translations must keep:

- **As of is strictly before; at dates is at or before.** `balance_as_of(t)` takes splits posted before `t`
  (`Account.cpp:3505-3518`), `accumulate_at` splits posted at or before each time (`5.17:bindings/engine.i:195-221`).
  `accumulate_at` walks the times in the order given with a cursor that only moves forward, as GnuCash's does.
- **The num field source reorders a day.** With it on, split actions stand in for nums in split order
  (`Split.cpp:1548-1557`), which changes the running balances of splits posted at the same time, never a balance
  between days.
- **A skipped stock split reads as zero.** The engine skips a stock split while the balance is zero or when it would
  make it zero, and never sets that split's running totals (`Account.cpp:2284-2290`); its `balance` reads 0, and so
  does `balance_as_of` a time when it is the last split before.
- **Latest is two lookups.** A direct latest price may lie in the future; an indirect one is the newest at or before
  now, which the host passes in (`PriceAt::Latest { now }`), never the clock.

### Time

The run context `ctx` gives time and formatting, all in the local time zone of the run:

| Function | GnuCash |
|----------|---------|
| `ctx.now`, `ctx.today()` | `current-time`, `gnc:get-today` |
| `ctx.start_of_day(t)`, `ctx.end_of_day(t)` | `gnc:time64-start-day-time`, `gnc:time64-end-day-time`: local 00:00:00 and 23:59:59 (`5.17:bindings/guile/date-utilities.scm:479-493`) |
| `ctx.local(t)` | `gnc-localtime`: `#{ year, month, day, hour, minute, second, weekday, yearday }`, counted as people count them, not as `struct tm` does: `month` 1–12, `weekday` 1 (Sunday) to 7, `yearday` 1–366 |
| `ctx.make_time(fields)` | `gnc-mktime` with `isdst = -1`, fields normalised C-style (31 January + 1 month is 3 March) |
| `ctx.print_date(t)`, `ctx.print_date_interval(from, to)` | `qof-print-date`, `gnc-date-interval-format`, under `reports.dateFormat` |
| `ctx.print_time_iso(t)` | `gnc-print-time64` with the ISO format, for CSV dates: `YYYY-MM-DD HH:MM:SS` in local time |
| `ctx.accounting_period()` | `#{ start, end }` as Time64: `gnc-accounting-period-fiscal-start`/`-end` |
| `ctx.report_currency()`, `ctx.week_start()` | the settings of section 4; the week starts on day 1 (Sunday) to 7, numbered as `weekday` |

Calendar stepping (`incdate`, `incdate-months`, `gnc:make-date-list`, the date fractions) is Scheme in GnuCash
(`5.17:bindings/guile/date-utilities.scm`) and is translated into `lib/dates` on top of these.

## 6. Options

### Model

An option is identified by **section and name**, GnuCash's English strings (`OptionClassifier`,
`5.17:libgnucash/engine/gnc-option-impl.hpp:69-76`). It also has a **key** (GnuCash's sort tag), help text, a type,
a default and a value. red-ale keeps GnuCash's order: sections sorted by name, options within a section by key
(`5.17:libgnucash/engine/gnc-optiondb.cpp:143-148`, `:191-194`); equal keys keep declaration order, where GnuCash's
unstable sort leaves them in no defined order (the pie charts' "Show Totals" and "Show Percents" share key `b`,
`standard/account-piecharts.scm:138-145`).

Option types used by the reports in scope and the style sheets, and their wire values:

| Type | GnuCash | Value on the wire | In a script |
|------|---------|-------------------|-------------|
| `boolean` | `GncOptionValue<bool>` | `true` / `false` | bool |
| `string`, `text`, `font` | `GncOptionValue<std::string>`; font is a Pango description (`report/html-fonts.scm:53-98`) | string | string |
| `multichoice` | `GncOptionMultichoiceValue`, keys of type symbol, string or number (`5.17:bindings/guile/gnc-optiondb.i:623-655`) | the key: a string, or an integer for number keys (account levels `1`–`6`) | string or integer |
| `list` | the same with several keys selected | array of keys; empty reads as the default (`gnc-optiondb.i:1073-1081`) | array |
| `numberRange` | `GncOptionRangeValue<double>`: value, min, max, step (`5.17:libgnucash/engine/gnc-option-impl.hpp:410-439`) | decimal string (`"1.5"`) or integer | Rational |
| `plotSize` | `GncOptionRangeValue<int>`, 10–65535; above 100 means pixels (`gnc-optiondb.i:1094-1108`) | `{ "percent": n }` (10–100) or `{ "pixels": n }` (101–65535), since GnuCash stores only the number; or a bare integer read GnuCash's way | `#{ unit, value }` |
| `color` | hex string `rrggbb` or `rrggbbaa` (`gnc-optiondb.i:1326-1344`) | that string, a leading `#` accepted | string |
| `date` | `GncOptionDateValue` (`gnc-option-impl.hpp:1045-1165`) | `{ "absolute": Date or Timestamp }` or `{ "relative": name }` | Time64 (resolved) |
| `accountList` | `GncOptionAccountListValue`: GUIDs, allowed types, multi-select (`5.17:libgnucash/engine/gnc-option-impl.cpp:384-444`) | array of AccountRef (GUID or full name); written as GUIDs | Account[] |
| `accountSel` | `GncOptionAccountSelValue` | AccountRef | Account |
| `currency`, `commodity` | `GncOptionCommodityValue`, stored by namespace and mnemonic (`gnc-option-impl.hpp:215-259`) | CommodityId | Commodity |
| `internal` | `gnc_register_internal_option` | string or bool | |

**Numbers in ranges.** Every number range registered from Scheme is a double (`gnc-optiondb.i:1717`); only one in
scope is not a whole number, net-charts' "Line Width", 1.5 in steps of 0.1 (`standard/net-charts.scm:130-133`).
red-ale holds them as Rationals and reads a decimal string exactly, so a range never needs a float. The step is not
enforced, as in GnuCash.

**Dates.** An absolute Date means local 10:59:00 of that day, the day-neutral time GnuCash's date widget gives
(`gnc_tm_set_day_neutral`, `5.17:gnucash/gnome-utils/gnc-date-edit.c:1062-1066`); a Timestamp is taken as given.
Relative names are GnuCash's Scheme names (`gnc-optiondb.i:828-899`, table in
`5.17:libgnucash/engine/gnc-option-date.cpp:73-323`): `today`, `one-week-ago`, … `start-this-month`,
`end-this-month`, `start-prev-month`, … `start-cal-year`, `end-prev-year`, `start-accounting-period`,
`end-accounting-period`. The C++ storage names of the last two, `start-prev-fin-year` and `end-prev-fin-year`
(`gnc-option-date.cpp:311`, `:319`), are accepted as aliases, since they are what GnuCash writes into saved reports.
A date option has a UI of `absolute`, `relative` or `both` and, for relative values, a set of allowed names; a value
outside the set is refused, as GnuCash's `validate` refuses it (`gnc-option-impl.hpp:1093-1098`).

**Defaults that depend on the Book** (account lists) are computed by the script's `options` function from the Book
it is given, as GnuCash's generators compute them from the current book when the options are made. An account-list
option with an empty default and allowed types defaults to the first account of an allowed type in display order
(`gnc-option-impl.cpp:415-444`).

**Defaults from preferences** come from the report settings (section 4): `add_currency` defaults to
`ctx.report_currency()`. A default GnuCash takes from the locale takes the C locale's value: the Price
Scatterplot's "Price of Commodity" defaults to `gnc-locale-default-iso-currency-code` (`standard/price-scatter.scm:63-67`),
which is `USD`.

### Declaring options

The builder passed to `options()` mirrors GnuCash's `gnc-register-*-option` functions
(`5.17:libgnucash/engine/gnc-optiondb.cpp:626-1112`). Each takes section, name, key and help text, then:

| Builder call | GnuCash |
|--------------|---------|
| `o.boolean(s, n, k, help, default)` | `gnc-register-simple-boolean-option` |
| `o.string(…, default)`, `o.text(…, default)`, `o.font(…, default)` | `gnc-register-string-option`, `-text-option`, `-font-option` |
| `o.color(…, "rrggbb")` | `gnc-register-color-option` |
| `o.multichoice(…, default, [[key, label, help?], …])` | `gnc-register-multichoice-option` |
| `o.list(…, defaults, [[key, label, help?], …])` | `gnc-register-list-option` |
| `o.number_range(…, default, min, max, step)` | `gnc-register-number-range-option`; numbers as integers or decimal strings |
| `o.plot_size(…, default)` | `gnc-register-number-plot-size-option` |
| `o.date(…, default, #{ ui, relative })` | `gnc-register-date-option-set`; `default` is a relative date's name or a Time64. It is an argument, not a map key, because `default` is a reserved word in Rhai |
| `o.start_date(…, both)`, `o.end_date(…, both)` | `gnc-register-start-date-option`, `-end-date-option`: UI `both` (`relative` when `both` is false), the eight start or end choices, default the accounting period's start or end (`gnc-optiondb.cpp:1052-1098`) |
| `o.account_list(…, default, #{ types, multiple })` | `gnc-register-account-list-option`, `-limited-option` |
| `o.account_sel(…, default, #{ types })` | `gnc-register-account-sel-limited-option` |
| `o.currency(…, default)`, `o.commodity(…, default)` | `gnc-register-currency-option`, `-commodity-option` |
| `o.internal(…, default)` | `gnc_register_internal_option` |
| `o.set_default_section(s)` | `GncOptionDBPtr-set-default-section` |
| `o.default_report_currency()` | `gnc-default-report-currency`: the report settings' currency (section 4), since `options()` has no run context |

`lib/options` translates `options-utilities.scm` on top of these (`add_report_date`, `add_date_interval`,
`add_interval_choice`, `add_account_levels`, `add_account_selection`, `add_currency`, `add_price_source`,
`add_plot_size`, `add_marker_choice`, `add_sort_method`, `add_subtotal_view`, `report/options-utilities.scm:44-234`),
with GnuCash's names, keys, help texts, choices and defaults unchanged.

Every template also gets GnuCash's three General options, unless its generator declared them
(`gnc:report-template-new-options`, `report/report-core.scm:298-335`): "Report name" (key `0a`, default the
template name), "Stylesheet" (`0b`, a multichoice over the style sheets, default `Default`) and "Add options
summary" (`0c`, `always` or `never`, default `never`). With `always`, the host appends a table of every option whose
value differs from its default, as `gnc:html-render-options-changed` does (`report/html-utilities.scm:298-345`).

In a script, `opts.get(section, name)` returns the value (dates already turned into Time64, as
`gnc:date-option-absolute-time` does, `5.17:bindings/guile/options.scm:335-340`), and `opts.raw(section, name)` the
wire form. An unknown section or name is a `report-script` error.

### The published schema and setting options

`report.options` returns, for a template or saved configuration, every option in order:

```json
{ "section": "General", "name": "Start Date", "key": "aa", "type": "date",
  "help": "Start of reporting period.",
  "ui": "both",
  "relative": ["today", "start-this-month", "start-prev-month", "start-current-quarter", "start-prev-quarter",
               "start-cal-year", "start-prev-year", "start-accounting-period"],
  "default": { "relative": "start-accounting-period" },
  "value": { "relative": "start-accounting-period" } }
```

with `choices` (`[{ key, label, help }]`) for multichoice and list options, `min`, `max`, `step` (as Rationals)
for number ranges, `accountTypes` and `multiple` for account options. `value` differs from `default` only for a
saved configuration. It also returns `paramsSchema`, a JSON Schema (2020-12) of the `options` param for this
template, built from the declarations, so an MCP client can check its call before making it.

A client sets options in `report.run`, `report.export` and `report.saveConfiguration` as an object by section and
name, giving only what it changes:

```json
{ "options": { "General": { "Start Date": { "absolute": "2024-01-01" },
                            "End Date": { "relative": "end-this-month" } },
               "Accounts": { "Levels of Subaccounts": 2 } } }
```

Absent options keep their default (or a saved configuration's value). Every given value is checked against its
option before the script runs; the first failure is a `report-option` error and nothing runs. GnuCash logs bad
values and leaves the option unchanged (`gnc-optiondb.i:1492-1497`); red-ale refuses, so a typo never runs a
different report than the caller asked for.

## 7. The report result

The report result is the published language for reports: part of `redAleApi` and versioned with it (`0.2.0`), its
JSON Schema listed in `api.schema` as `ReportResult`. Within a major version it changes only additively, like the
rest of the API. It carries data, not markup: exact figures with their display form, and roles, not styles.

`render()` returns it as a Rhai map of the same shape, without the fields the host fills in (marked *host*). The host
checks it against the schema and fills those fields; a mismatch is a `report-result` error naming the path.

Beyond the schema, a result is valid when every cell holds exactly one content (`text`, `monetary`, `commodities`,
`number`, `price` or `date`), `places` and `suffix` appear only on number cells and `currency` on every price cell and
nowhere else, each row covers exactly the table's columns (a `null` cell covers one, a cell its `span`), a row's
`level` is below the `levels` of the table's amount columns, `levels` appears only on amount columns, a chart's series
have one value per label (scatter series have `points` instead), `colors` and `links` have one entry per value, a
`currency` axis names its `commodity`, a pie chart has one series, plot sizes are 1–100 % or at least one pixel, and
no *host* field is set by the script. `optionsSummary` blocks come only from the host.

### Top level

```json
{
  "version": "0.2.0",
  "report": { "guid": "c4173ac99b2b448289bf4d11c731af13", "name": "Balance Sheet", "configuration": null },
  "title": "Acme Ltd Balance Sheet 12/31/24",
  "headline": null,
  "now": "2025-01-15T09:30:00Z",
  "options": { "General": { "Balance Sheet Date": { "relative": "end-accounting-period" } } },
  "sections": [ { "title": null, "blocks": [ … ] } ],
  "log": []
}
```

| Field | Meaning |
|-------|---------|
| `version` | *host*. The `redAleApi` version the result was published with, `0.2.0` |
| `report` | *host*. The template's GUID and name, and the saved configuration's GUID when one was run |
| `title` | The document title (GnuCash's `gnc:html-document-set-title!`); the HTML `<title>` and, without a headline, the heading |
| `headline` | Optional heading when it differs from the title (`gnc:html-document-set-headline!`) |
| `now` | *host*. The time the report was run for |
| `options` | *host*. Every option's value, in wire form, so the result says exactly what produced it |
| `sections` | Ordered sections, each an optional `title` and ordered `blocks` |
| `log` | *host*. The report log |

### Blocks

| `kind` | Fields | GnuCash |
|--------|--------|---------|
| `text` | `text`, `role`: `paragraph` (default), `heading`, `note` | `gnc:make-html-text`, `gnc:html-markup-h3`, `gnc:html-markup-p` |
| `warning` | `title`, `text` | `gnc:html-make-generic-warning` and the no-account and empty-data warnings built on it (`report/html-utilities.scm:350-381`) |
| `table` | below | `<html-table>` |
| `chart` | below | `<html-chart>` |
| `group` | `blocks`, laid out side by side | the one-row layout tables of the two-column Balance Sheet and Income Statement (`standard/balance-sheet.scm:503-513`, `standard/income-statement.scm:483-536`) |
| `optionsSummary` | *host*. `rows: [{ section, name, value }]` | `gnc:html-render-options-changed` |

### Tables

```json
{
  "kind": "table",
  "title": null,
  "columns": [
    { "label": "Account title", "role": "label" },
    { "label": "Balance", "role": "amount", "levels": 3 }
  ],
  "rows": [
    { "role": "heading", "cells": [ { "text": "Assets" }, null ] },
    { "role": "account", "account": "4a1c…", "depth": 1, "level": 1,
      "cells": [ { "text": "Bank", "account": "4a1c…" },
                 { "monetary": { "amount": { "num": "2186", "denom": "1" }, "commodity": "CURRENCY::USD" },
                   "rounded": { "num": "2186", "denom": "1" }, "display": "$2,186.00" } ] },
    { "role": "total", "level": 0,
      "cells": [ { "text": "Total Assets" }, { "monetary": …, "display": "$116,006.33", "rule": true } ] }
  ]
}
```

**Columns** have a `label` (empty for none), a `role`, and an optional `align` (`left`, `right`, `center`; amounts
default right). Roles: `label` (account names and row labels), `text`, `amount`, `date`. An `amount` column may
have `levels`, the number of staircase positions an account table uses: GnuCash puts a deeper account's amount
further left and totals furthest right (`report/html-acct-table.scm:870-923`, amount depth at `:1059-1062`). A
row's `level` says which position its amount takes, 0 being the rightmost. The renderer turns levels into
columns; a client may ignore them and read the figure.

**Rows** have a `role`, `cells` (one per column, `null` for empty) and, where they apply:

- `account`: the GUID of the account the row is about;
- `depth`: indentation of the label, counted in levels (`indented-depth` of `report/html-acct-table.scm:749-763`);
- `level`: the amount's staircase position.

Row roles: `data` (default), `heading` (a section label such as "Assets"), `account`, `subtotal` ("Total Bank1"),
`total` (a section total: "Total Assets"), `grandTotal` ("Total Liabilities & Equity", "Net income"), `rule` (a
horizontal rule across the table, GnuCash's `gnc:html-table-append-ruler!`) and `spacer`. The roles stand for
GnuCash's row and cell markup (`primary-subheading`, `total-label-cell`, `total-number-cell`), which style sheets
colour (section 8).

**Cells** are one of:

| Cell | Fields | Display (*host*) |
|------|--------|------------------|
| text | `text` | the text |
| monetary | `monetary: { amount: Rational, commodity }` | `rounded` (Amount at the commodity's SCU, banker's rounding) and `display`, section 3 |
| commodities | `commodities: [{ monetary, converted }]` | each line's `display` and `convertedDisplay` |
| number | `number: Rational`, `places` (default 2), `suffix` | `display`, rounded to `places` with banker's rounding, then the suffix (`"57.14 %"`) |
| price | `price: Rational`, `currency` | `display` with the currency's price print info |
| date | `date: Time64` (a Timestamp on the wire) | `localDate` (`YYYY-MM-DD` in the run's zone) and `display` under `reports.dateFormat` |

A `commodities` cell is GnuCash's nested commodity table for an account holding foreign commodities: one line per
commodity, the native amount and its value in the report currency (`report/html-acct-table.scm:925-948`).

Any cell may also have `account` (a GUID: the cell names or links to that account), `link` (below), `rule: true` (an
accounting rule above the figure, the "Show accounting-style rules" option, `report/html-acct-table.scm:901-906`) and
`span` (columns it covers, default 1).

### Links

A link points at something a client can open: `{ "account": "<guid>" }` (GnuCash's register link,
`report/html-utilities.scm:82-88`), or `{ "report": { "template": "<guid>", "options": { … } } }` (GnuCash's
drill-down: the charts link a bar or slice to the same report on that account's children,
`standard/category-barchart.scm:589-609`, `standard/account-piecharts.scm:474-496`). GnuCash creates a report
instance at render time for a report link; red-ale only describes it, and a client runs it with `report.run`.

### Charts

```json
{
  "kind": "chart",
  "type": "bar",
  "title": ["Net Worth Barchart", "01/01/24 to 12/31/24"],
  "width": { "percent": 100 }, "height": { "percent": 100 },
  "stacked": false, "grid": true, "legend": true,
  "xAxis": { "label": "", "scale": "category" },
  "yAxis": { "label": "USD", "format": "currency", "commodity": "CURRENCY::USD" },
  "labels": ["01/01/24", "02/01/24", "03/01/24"],
  "series": [
    { "label": "Assets", "color": "#0074D9", "values": [ { "num": "2500", "denom": "1" }, … ],
      "fill": true, "link": { "report": { … } } }
  ],
  "tooltip": { "grouped": true, "nonZeroOnly": true }
}
```

| Field | Meaning | GnuCash (`report/html-chart.scm`) |
|-------|---------|-----------------------------------|
| `type` | `bar`, `line`, `pie` or `scatter` | `set-type!` (`:289-291`) |
| `title` | lines of the title | `set-title!` (`:296-297`) |
| `width`, `height` | a plot size value | `set-width!`, `set-height!`, default 100 % (`:190-193`) |
| `stacked` | bars stack, or lines accumulate | `set-stacking?!` (`:392-394`) |
| `grid` | grid lines | `set-grid?!` (`:396-398`) |
| `legend` | show the series (or slice) labels | the Chart.js legend, on the right (`:205-206`) |
| `xAxis`, `yAxis` | `label`; `scale`: `category` (one slot per label), `linear` (numeric x, for scatter); `format` of y values: `currency`, `percent` or `number`; `commodity` for currency | `set-x-axis-label!`, `set-y-axis-label!`, `set-axes-display!`, currency ISO and format style (`:173-178`, `:389-401`) |
| `labels` | category labels (bar, line), slice labels (pie) | `set-data-labels!` (`:299-300`) |
| `series` | below | `add-data-series!` (`:370-384`) |
| `tooltip` | `grouped` (default true, GnuCash's `index` mode, `:220-221`): one tooltip for all series at a label; `nonZeroOnly`: leave zero values out of it | `set-tooltip-mode!`, `set-tooltip-non-zero-only!` (`:309-328`) |

A series has `label`, `color` (`#rrggbb`), `values` (Rational or `null`, one per label), and optionally `colors` (one
per value: a pie's slices), `fill` (area under a line, or a filled bar), `lineWidth` (Rational), `stack` (series with
the same stack name stack together; GnuCash's `stack` key), `kind` (`bar` or `line`, to mix them), `link` (the whole
series) or `links` (one per value). A `scatter` series has `points: [{ x: Rational, y: Rational, label }]` instead of
`values`, and `line` (`#rrggbb` or `null`): the price chart joins its points with a line of another colour
(`standard/price-scatter.scm:269-274`). `marker` is `circle`, `square`, `diamond`, `cross` or `plus`.

These cover every chart in scope:

| Report | Chart |
|--------|-------|
| Net Worth / Income-Expense Barchart | `bar`, two or three series side by side (`standard/net-charts.scm:316-384`) |
| Net Worth / Income & Expense Linechart | `line`, the same series, `lineWidth` from the option |
| Category bar charts | `bar` or `line` by option, `stacked` by option, one series per account plus "Other" (`standard/category-barchart.scm:528-626`) |
| Account pie charts | `pie`: one series, a value and colour per slice (`standard/account-piecharts.scm:472-558`) |
| Price Scatterplot | `scatter` with a `linear` x axis in steps since the start date (`standard/price-scatter.scm:166-274`) |
| Average Balance | `bar`: Average, Profit, Gain and Loss series (`example/average-balance.scm:386-451`) |

### Hover labels

The host fills in each value's display form under the y axis's format: `displays` on a series (one per value, `null`
for a missing value) and `display` on a scatter point. The renderer, which has no Book, builds each hover label from
them and writes it into an SVG `<title>`. The text follows GnuCash's tooltip: the label,
then `series : value` lines (`report/html-chart.scm:433-449`), with the value in the display form of section 3 for a
`currency` axis, two decimals and `%` for `percent`, and the exact decimal for `number`. GnuCash formats these in the
browser with `Intl.NumberFormat` from doubles; red-ale formats the exact value its own way, so hover text is
red-ale's, not GnuCash's.

## 8. Rendering

`render::html(result, style_sheet, context) -> String` turns a finished report result into one self-contained HTML
document. `style_sheet` is an `Option<&StyleSheet>` (`None` is the base look, below); `context` is the run's local
time zone and `reports.dateFormat`, which the header and footer dates need. Report styling is red-ale's own: the
figures must match GnuCash's, the markup need not.

### The document

- `<!DOCTYPE html>`, `<html lang="en">`, `<meta charset="utf-8">`, the `title`, one `<style>` element, then the body.
  No `<script>`, no external stylesheet, font, image or URL of any kind, so the page works offline and leaks nothing.
- Every piece of text from the Book, an option or a script is HTML-escaped. GnuCash inserts some text raw
  (preparer, footer and comments in the style sheets, `report/html-text.scm:239-245`); red-ale never does.
- **Deterministic bytes.** Elements and attributes are written in a fixed order, numbers in a fixed form, IDs from
  the block's position (`chart-0-2`: section 0, block 2; `chart-0-2-1` inside a group), never from a counter shared
  between runs. The run's `now` is the only time printed. Exporting twice from the same Book, options, now and
  settings gives the same bytes; a test checks it.
  The page carries no version, so it is the same across releases, except for Head or Tail's version line.
- **The look.** One base stylesheet: the body font from the style sheet's Fonts options, numbers right-aligned with
  `font-variant-numeric: tabular-nums` and no wrapping, labels indented by `depth`, a thin rule above subtotals and a
  double rule above totals and grand totals, headings for sections, generous but compact spacing, and print rules
  (no backgrounds, tables kept together). Style sheet options set the values it uses.
- **Class names are GnuCash's** where one exists: `number-cell`, `total-number-cell`, `text-cell`,
  `total-label-cell`, `date-cell`, `neg`, and the row styles `normal-row`, `alternate-row`, `primary-subheading`,
  `secondary-subheading` and `grand-total` (`report/stylesheets/plain.scm:104-167`). CSS written for GnuCash's CSS
  template therefore styles red-ale's page too. Rows also carry `role-<role>`.
- Line breaks in text become `<br>`.
- **Tables.** An `amount` column with `levels` becomes that many cell columns; a row's amount goes in the column of
  its `level`, counted from the right, as GnuCash lays out its account tables. A `commodities` cell becomes a small
  nested table, one line per commodity. Cells with `account` carry `data-account="<guid>"`; a `link` becomes
  `data-link` with its JSON. There are no `href`s: GnuCash's `gnc-register:` and report URLs mean nothing outside
  GnuCash, and a client that embeds the page can wire the data attributes itself.
- **Negative figures** get the class `neg`, coloured red when the style sheet's "Negative Values in Red" is on
  (`report/html-fonts.scm:116-119`).
- A `group` block is a CSS grid of its blocks side by side, stacking on narrow screens.

### Style sheets

A **style sheet** is a named set of presentation options from a **style sheet template**. red-ale has GnuCash
5.17's five templates and its six predefined style sheets (`report/stylesheets/plain.scm:211-218`,
`report/stylesheets/css.scm:222-228`, `report/stylesheets/footer.scm:378-398`,
`report/stylesheets/head-or-tail.scm:524-530`):

| Style sheet (predefined) | Template | Options red-ale keeps |
|--------------------------|----------|-----------------------|
| `Default` | Plain | General: Background Color, Enable Links; Colors: Alternate Table Cell Color; Tables: Table cell spacing, Table cell padding, Table border width; Fonts |
| `CSS-based stylesheet (experimental)` | CSS | General: CSS |
| `Easy` | Easy | General: Preparer, Prepared for, Show preparer info, Enable Links, Footer; Colors: Background Color, Text Color, Link Color, Table Cell Color, Alternate Table Cell Color, Subheading/Subtotal Cell Color, Sub-subheading/total Cell Color, Grand Total Cell Color; Tables; Fonts |
| `Technicolor` | Fancy | as Easy |
| `Footer` | Footer | as Easy |
| `Head or Tail` | Head or Tail | General: Preparer, Prepared for, Show preparer info, Show receiver info, Show date, Show time in addition to date, Show GnuCash Version, Enable Links, Additional Comments, and the five "… at bottom" options; Colors and Tables as Easy; Fonts |

Names, keys, types and defaults are 5.17's, including Head or Tail's misspelt subheading default `"eexe8xaa"`
(`report/stylesheets/head-or-tail.scm:168-172`): an invalid colour sets no colour, which is what the browser does
with GnuCash's page. The Fonts section is `register-font-options` (`report/html-fonts.scm:100-142`): Title, Account
link, Number cell, Negative Values in Red, Number header, Text cell, Total number cell, Total label cell, Centered
label cell. GnuCash's default font family is the GTK one, or Arial without a GUI (`report/gnc-report.cpp:302-331`);
red-ale's is `Noto Sans` with the generic `sans-serif` after it, as `font-name-to-style-info` writes it
(`report/html-fonts.scm:53-98`).

**Left out**, by milestone 2's scope: every image option (Background Pixmap, Background Tile, Heading Banner, Logo)
and Heading Alignment, which only places the banner. A saved style sheet carrying one keeps it in the Configuration
but it has no effect.

**What the options do in red-ale's look:**

- Colours set the page background and text, the backgrounds of `heading` and `subtotal` rows (Subheading/Subtotal),
  `total` rows (Sub-subheading/total), `grandTotal` rows (Grand Total) and alternate `data` rows. GnuCash's colour
  meanings are kept; its table markup is not.
- Table spacing, padding and border widths become the table's `border-spacing`, cell `padding` and `border` in
  pixels.
- Fonts map to the CSS of the table cells they name, as `html-fonts.scm` maps them.
- "Enable Links" off removes the `data-account` and `data-link` attributes.
- Preparer, Prepared for, the date (the run's `now` under `reports.dateFormat`) and the footer or comments go in a
  header or footer block, worded as GnuCash words them ("Prepared by: ", "Date: ", "Report Creation Date: "). An
  empty name or text is left out, where GnuCash prints an empty label. Head or Tail's time is `HH:MM:SS` and the
  numeric UTC offset (`+0100`), standing for `%X %Z`: red-ale knows the zone's rules, not its abbreviation.
  "Show GnuCash Version" shows `red-ale <version>`.
- Plain colours subheading and grand-total rows with its Background Color, as GnuCash does
  (`report/stylesheets/plain.scm:104-167`). Colours are written as `gnc:color->html` writes them, the first six
  digits (`5.17:bindings/guile/options.scm:72-77`).
- A font option's family keeps only letters, digits, spaces, `-`, `_` and `.`, and a size that is not a number is
  dropped, so no option can write other CSS.
- The CSS template's text is appended to the base stylesheet; the template has no Fonts or Tables, so the base
  look's stand in. Text containing `</style`, `@import`, `url(`, `image(`, `image-set(`, `src(` or a backslash (an
  escape could spell any of them) is refused (`report-option`), ignoring ASCII case: GnuCash falls back to its
  default CSS on `</style` (`report/stylesheets/css.scm:204-209`), and red-ale also keeps an exported page from
  loading anything.
- Values for the left-out image options are accepted and ignored when a style sheet is built or changed; the
  Configuration keeps whatever it was given.

The "Stylesheet" option names a style sheet; `report.export` can also override it for one export. A name that no
longer exists (a saved configuration whose style sheet was deleted) renders with the base look and no style-sheet
options, as GnuCash renders a report with no style sheet when the lookup fails (`report/report-core.scm:457-462`),
and says so in the log: the host appends `render::missing_style_sheet(name)`, a `debug` entry, before rendering.
The base look is the Plain template's defaults with no colours.

The style sheets a report can name are `StyleSheets::from_configuration(entries)`: the six predefined ones, then each
`reports.styleSheets` entry in order, an entry with a predefined name replacing it, a later entry replacing an
earlier one of the same name. Lookup is exact by name. Lists sort by name bytewise.

### Charts as SVG

Each chart is an inline `<svg>` with a `viewBox`, drawn from the chart block with integer arithmetic only:

- **Size.** A pixel plot size sets the `viewBox` and the `width` and `height`. A percent size uses a 1000 × 600
  `viewBox` scaled to that percentage of the container's width, keeping the aspect; a percent height shortens the
  `viewBox` to that share of 600. Layout never works in less than 320 × 200; a smaller pixel size scales that down.
- **Coordinates** are computed as exact Rationals and rounded half-even to hundredths of a unit, then written as
  integers or with two decimals (`123.45`). The y range is the data's minimum and maximum (and zero for bars),
  widened to "nice" tick steps of 1, 2 or 5 × 10ⁿ, chosen exactly. A `linear` x axis does the same with x.
- **Bars** are `<rect>`s, side by side per label or stacked (positive values up, negative down, as Chart.js stacks
  them). Stacked, bars with the same `stack` share a column and different stacks stand side by side; unstacked, each
  bar series has its own column. Bars are always filled, as Chart.js fills them; `fill` applies to lines.
  **Lines** are `<polyline>`s with a marker per point, broken at a missing value; stacked lines add up in series
  order and a filled one is shaded down to the line below it (or to zero). **Scatter** points are markers, joined in
  data order when the series has a `line` colour.
- **Pie slices** are `<path>` arcs from 12 o'clock clockwise, Chart.js's start. A slice's size is its value's share
  of the total of absolute values, as Chart.js sizes them; zero and missing values have no slice, and one slice of
  everything is a `<circle>`. The angle of each boundary is the exact cumulative fraction of the total; its
  sine and cosine come from a fixed-point CORDIC on `i64` (Q2.61, 62 iterations) and are rounded to hundredths. No
  float is involved, and every platform gives the same bytes. Slice labels sit in the legend, as in GnuCash's chart.
- **Ticks** show exact, grouped numbers (`2,500`, `0.5`), with `%` on a `percent` axis; a `currency` axis names its
  currency in its label rather than on every tick, since the renderer has no Book to format money with.
- **Text.** Axis labels, ticks, the title and the legend are `<text>` elements. Their widths are estimated as three
  fifths of the font size per character, so layout needs no font metrics. Labels that would not fit are cut with
  `…`, category labels that would overlap are thinned to every k-th, and a legend longer than the plot ends with
  "+N more".
- **Hover labels** are `<title>` children of each bar, point or slice (section 7).
- **Colours** are the series colours; GnuCash's chart area background `#fffdf6` and grid lines are kept
  (`report/html-chart.scm:203-204`).

There is no zooming, no toggling of series and no drill-down click; links are in the JSON result.

## 9. Saved report configurations and the Configuration

GnuCash keeps report settings outside the book, in files in the user's data directory: saved report configurations
in `saved-reports-2.8` (`SAVED_REPORTS_FILE`, `report/gnc-report.h:35-36`) and style sheets in `stylesheets-2.0`
(`report/html-style-sheet.scm:142-143`; `report/gnc-report.cpp:90`), both Scheme code that GnuCash evaluates at
start-up (`report/gnc-report.cpp:55-112`). red-ale keeps the same things in its Configuration, the JSON file of
`--config` ([`api/accounts.md`](api/accounts.md), "The accounting period"), under one key:

```json
{
  "accountingPeriod": { "start": "year", "end": "year" },
  "reports": {
    "directory": "/home/me/.local/share/red-ale/reports",
    "currency": "CURRENCY::USD",
    "reverseBalancedAccounts": "credit",
    "dateFormat": "locale",
    "weekStart": "sunday",
    "limits": { "operations": 1000000000, "seconds": 120 },
    "saved": [
      { "guid": "6f1e0c9d2b7a4e3f8a5b1c2d3e4f5a6b", "name": "Balance Sheet (USD, 2 levels)",
        "template": "c4173ac99b2b448289bf4d11c731af13",
        "options": { "Accounts": { "Levels of Subaccounts": 2 } } }
    ],
    "styleSheets": [
      { "name": "Footer", "template": "Footer",
        "options": { "General": { "Preparer": "A. Bookkeeper" } } }
    ]
  }
}
```

- **A saved report configuration** is a template's options saved under a new name and GUID, GnuCash's *custom
  report* (`report/report-core.scm:126-145`). As in GnuCash (`report/report-core.scm:620-643`;
  `5.17:bindings/guile/options.scm:141`), it stores only the options that differ from the template's defaults, so
  it follows a template's later defaults for the rest. It runs the template it names; it cannot change its code.
- **Names** are unique among saved configurations. GnuCash appends a number to a clashing name on save
  (`report/report-core.scm:515-532`) and refuses a clash on rename; red-ale refuses both (`conflict`), so a client
  always gets the name it asked for.
- **Style sheets**: the six predefined ones always exist. An entry under `styleSheets` with a predefined name holds
  changes to it; one with a new name is a user style sheet of the named template. Deleting a predefined style sheet
  resets it to its template's defaults, which is what GnuCash does at its next start, since it recreates the
  predefined sheets before loading the saved ones (`report/gnc-report.cpp:81-111`).
- **Writing** is atomic (the store writes beside the file and renames, `config.rs`) and keeps keys red-ale does not
  know. Entries are kept in the order they were added. GnuCash rewrites its files in hash order without a temporary
  file (`report/report-core.scm:744-755`; `report/html-style-sheet.scm:145-150`).
- **GnuCash's files are not read.** They are Scheme programs; reading them would mean evaluating Scheme. Importing
  their option forms (`gnc:option-set-value` calls on literal values) is possible later (section 15).
- The Book's one reference to a saved configuration, the default invoice report
  (`options/Business/Default Invoice Report`, `5.17:libgnucash/engine/qofbook.cpp:1034-1112`), belongs to invoice
  printing, which is out of scope; red-ale keeps the slot as it keeps every slot.

`limits` holds any of the limits of section 2 under the names in `data.limit` (`seconds` for `time`).

## 10. Effects outside the Book

Effect said what a method does to the Book. Saving a report configuration changes no Book, and v0.1 already had a
method like that: `period.set` writes the Configuration and is declared `write`. Making the target explicit keeps
Effect meaning one thing and MCP's hints unchanged:

- **Effect** keeps its three values and its meaning of *how much* a method changes: `read`, `write`,
  `destructive`. MCP's `readOnlyHint` (`read`) and `destructiveHint` (`destructive`) still come from it alone. The MCP
  specification's read-only hint means the tool does not change its environment; writing the Configuration does,
  so those methods are not read-only.
- **Effect target**, new, says *what* a non-read method changes: `book` or `configuration`. It is a field of every
  method in the schema document (`null` for `read` methods), added in `redAleApi` 0.2.0. Every v0.1 method that
  changes something targets `book`, except `period.set`, which targets `configuration`.
- Report methods: `report.list`, `report.options`, `report.run`, `report.export` and `report.styleSheets` are
  `read`; `report.saveConfiguration`, `report.updateConfiguration`, `report.renameConfiguration`,
  `report.createStyleSheet` and `report.updateStyleSheet` are `write` / `configuration`; `report.deleteConfiguration`
  and `report.deleteStyleSheet` are `destructive` / `configuration` ([`api/reports.md`](api/reports.md)).
- Configuration methods work with no Book open, never make a Book dirty and never touch the journal.

**`report.export` returns the HTML and writes nothing.** The server never writes to a path a caller names: over
HTTP or MCP that would let a client write files where the server runs. The CLI writes the returned text to the file
given with `--output`, on the caller's side, which is what milestone 2 asks of the CLI.

## 11. Report names and identities

Built-in templates keep GnuCash 5.17's names and GUIDs, so a name or GUID given to `gnucash-cli --report run`
names the same report in red-ale. All names are GnuCash's English ones.

| Script | Template name | GUID | Menu | Source |
|--------|---------------|------|------|--------|
| `standard/account-summary.rhai` | Account Summary | `3298541c236b494998b236dfad6ad752` | (top level) | `standard/account-summary.scm:423-428` |
| `standard/balance-sheet.rhai` | Balance Sheet | `c4173ac99b2b448289bf4d11c731af13` | Assets & Liabilities | `standard/balance-sheet.scm:527-533` |
| `standard/income-statement.rhai` | Income Statement | `0b81a3bdfd504aff849ec2e8630524bc` | Income & Expense | `standard/income-statement.scm:566-572` |
| | Profit & Loss | `8758ba23984c40dea5527f5f0ca2779e` | Income & Expense | `standard/income-statement.scm:576-582` |
| `example/average-balance.rhai` | Average Balance | `d5adcc61c62e4b8684dd8907448d7900` | Examples | `example/average-balance.scm:488-494` |
| `standard/net-charts.rhai` | Net Worth Barchart | `cbba1696c8c24744848062c7f1cf4a72` | Assets & Liabilities | `standard/net-charts.scm:469`, `:473-519` |
| | Income/Expense Chart (menu: Income & Expense Barchart) | `80769921e87943adade887b9835a7685` | Income & Expense | `standard/net-charts.scm:470` |
| | Net Worth Linechart | `d8b63264186b11e19038001558291366` | Assets & Liabilities | `standard/net-charts.scm:468` |
| | Income & Expense Linechart | `e533c998186b11e1b2e2001558291366` | Income & Expense | `standard/net-charts.scm:512` |
| `standard/category-barchart.rhai` | Income Over Time (menu: Income Chart) | `44f81bee049b4b3ea908f8dac9a9474e` | Income & Expense | `report/report.scm:63-66`; `standard/category-barchart.scm:708-744` |
| | Expense Over Time (Expense Chart) | `b1f15b2052c149df93e698fe85a81ea6` | Income & Expense | |
| | Assets Over Time (Asset Chart) | `e9cf815f79db44bcb637d0295093ae3d` | Assets & Liabilities | |
| | Liabilities Over Time (Liability Chart) | `faf410e8f8da481fbc09e4763da40bcc` | Assets & Liabilities | |
| `standard/account-piecharts.rhai` | Income Accounts (Income Piechart) | `e1bd09b8a1dd49dd85760db9d82b045c` | Income & Expense | `standard/account-piecharts.scm:602-642` |
| | Expense Accounts (Expense Piechart) | `9bf1892805cb4336be6320fe48ce5446` | Income & Expense | |
| | Assets (Asset Piechart) | `5c7fd8a1fe9a4cd38884ff54214aa88a` | Assets & Liabilities | |
| | Securities (Security Piechart) | `e9418ff64f2c11e5b61d1c7508d793ed` | Assets & Liabilities | |
| | Liabilities (Liability Piechart) | `3fe6dce77da24c66bdc8f8efdea7f9ac` | Assets & Liabilities | |
| `standard/price-scatter.rhai` | Price Scatterplot | `1d241609fd4644caad765c95be20ff4c` | Assets & Liabilities | `standard/price-scatter.scm:281-288` |

Menu paths are GnuCash's (`report/report-core.scm:157-161`) without the mnemonic underscore in this table; the
scripts keep the strings as written (`"_Assets & Liabilities"`). The Future Scheduled Transactions Summary
(`47f45d7d6d57b68518481c1fc8d4e4ba`, `standard/account-summary.scm:430-435`) is not defined: it needs scheduled
transactions.

**Looking a report up.** Methods take `report`: a GUID, or a name. A GUID matches a template or a saved
configuration. A name matches template names and saved configuration names; more than one match is a `conflict`
whose `data.matches` lists them, as `gnucash-cli` refuses a name that matches several reports and asks for the GUID
(`report/report-core.scm:874-878`, `:914-929`). No match is `not-found` (`data.kind: "report"`).

**User reports** may not reuse a built-in GUID or each other's: the first definition wins, in the order built-in,
then user files by path, and the others are reported by `report.list` with `report-load`, as GnuCash keeps the first
and complains (`report/report-core.scm:219-220`).

## 12. Inventory and translation plan

### The reports

Abbreviations: RU `report/report-utilities.scm`, CU `report/commodity-utilities.scm`, OU
`report/options-utilities.scm`, HAT `report/html-acct-table.scm`, HU `report/html-utilities.scm`, DU
`5.17:bindings/guile/date-utilities.scm`.

| Report file | Templates | Helpers it uses | Book data and features |
|-------------|-----------|-----------------|------------------------|
| `standard/account-summary.scm` | Account Summary | OU: report-date, account-levels, currency, price-source, subtotal-view. RU: filter-accountlist-type, get-current-account-tree-depth, the commodity collector. CU: case-exchange-fn, case-price-fn. HAT: make-html-acct-table/env/accts, num-rows, get-row-env, the data half of add-account-balances. HU: make-rates-table, no-account warning. DU: time64-end-day-time. `gnc-accounts-and-all-descendants` | Account code, type, description, notes; prices for foreign commodities; company name |
| `standard/balance-sheet.scm` | Balance Sheet | As Account Summary, plus RU: decompose-accountlist, collector+ and collector-, monetaries-add, accounts-get-comm-total-assets, account-get-comm-value-at-date; CU: sum-collector-commodity, uniform-commodity? | "Use Trading Accounts" (`standard/balance-sheet.scm:266`): a trading line instead of unrealized gains; TRADING accounts; `xaccAccountGetBalanceAsOfDate` |
| `standard/income-statement.scm` | Income Statement, Profit & Loss | OU: date-interval, account-levels, currency, price-source, subtotal-view. RU: decompose-accountlist, accountlist-get-comm-balance-interval-with-closing, account-get-trans-type-balance-interval-with-closing, account-get-trans-type-splits-interval, collectors. CU, HAT and HU as Balance Sheet | Closing transactions (`book_closing` slot) and the closing-entries description pattern; TRADING accounts if selected; voided splits excluded |
| `example/average-balance.scm` | Average Balance | OU: date-interval, interval-choice, currency, price-source, plot-size. RU: accounts-get-commodities, account-get-balances-at-dates, monetaries-add, collectors. CU: case-exchange-time-fn (and everything under it), sum-collector-commodity. DU: make-date-list, deltasym-to-delta, decdate, time64-start/end-day-time. HU: empty-data and no-account warnings | A split query (accounts, dates, non-void); same-day split order follows "Number Field Source" (`5.17:libgnucash/engine/Split.cpp:1548-1557`) |
| `standard/net-charts.scm` | the four net charts | OU: date-interval, interval-choice, currency, price-source, plot-size. RU: account-accumulate-at-dates (no-closing balance), decompose-accountlist, filter-accountlist-type, account-is-inc-exp?, collector-, monetaries-add, accounts-get-commodities, not-all-zeros. CU: case-exchange-time-fn. DU: make-date-list, deltasym-to-delta, day boundaries. HU: warnings, `assign-colors` not used (fixed colours) | Closing transactions excluded through `xaccSplitGetNoclosingBalance` |
| `standard/category-barchart.scm` | the four "Over Time" charts | As net-charts, plus OU: account-levels, sort-method; RU: get-current-account-tree-depth; CU: sum-collector-commodity; DU: date-get-fraction-func and the fraction functions; HU: assign-colors | |
| `standard/account-piecharts.scm` | the five pie charts | OU: report-date or date-interval, currency, price-source, account-levels, plot-size, sort-method. RU: account-get-comm-balance-interval, account-get-comm-balance-at-date, filter-accountlist-type, get-current-account-tree-depth, monetary->string. CU: case-exchange-fn, sum-collector-commodity. DU: fraction functions. HU: assign-colors, warnings | Securities: account commodities and prices |
| `standard/price-scatter.scm` | Price Scatterplot | OU: date-interval, interval-choice, currency, plot-size. RU: account-has-shares?, monetary->string. CU: get-commodity-inst-prices, get-commodity-totalavg-prices, and the euro conversion | STOCK, MUTUAL and CURRENCY accounts with cross-commodity splits; the price database (forward prices only) |

No report needs a Book feature (`features` slot); all only read. The last column lists the Book data their figures
depend on.

### Shared Rhai modules

Each translates the named Scheme file's functions that the reports above use, directly or through each other, and
keeps its source's header and licence (GPL-2.0-or-later, checked at 5.17). Functions keep GnuCash's names with the
`gnc:` prefix dropped and `-` turned into `_`.

| Module | From | Functions |
|--------|------|-----------|
| `lib/collectors` | RU:289-465, :1018; CU:900-919 | `make_value_collector`, `make_commodity_collector` and its methods (`add`, `merge`, `minusmerge`, `format`, `getpair`, `getmonetary`, `getmonetary_strict`, `reset`, `remove_zeros`), `commodity_collector_get_negated`, `commodity_collector_allzero`, `collector_plus`, `collector_minus`, `monetaries_add`, `monetary_plus`, `not_all_zeros`, `sum_collector_commodity`, `uniform_commodity` |
| `lib/accounts` | RU:149-257 | `account_has_shares`, `account_is_stock`, `account_full_name_lt`, `accounts_get_children_depth`, `account_is_inc_exp`, `filter_accountlist_type`, `decompose_accountlist` (a map by kind), `account_get_type_string_plural`, `accounts_get_commodities`, `accounts_get_commodities_sorted`, `get_current_account_tree_depth`, `accounts_and_all_descendants` |
| `lib/balances` | RU:478-725, :863-881 | `account_get_balances_at_dates`, `account_accumulate_at_dates`, `account_get_comm_balance_at_date`, `account_get_comm_value_interval`, `account_get_comm_value_at_date`, `accounts_get_balance_helper`, `accounts_get_comm_total_assets`, `account_get_balance_interval`, `account_get_comm_balance_interval`, `accountlist_get_comm_balance_interval(_with_closing)`, `accountlist_get_comm_balance_at_date(_with_closing)`, `split_voided`, `accounts_count_splits`, `account_get_trans_type_balance_interval(_with_closing)`, `account_get_trans_type_splits_interval`, `get_assoc_account_balances`, `select_assoc_account_balance`, `get_assoc_account_balances_total` |
| `lib/exchange` | CU:64-887 | `get_match_commodity_splits(_sorted)`, `get_all_commodity_splits`, `exchange_by_euro(_numeric)`, `exchange_if_same`, `get_commodity_totalavg_prices(_internal)`, `get_commoditylist_totalavg_prices`, `get_commodity_inst_prices`, `pricelist_price_find_nearest`, `resolve_unknown_comm`, `get_exchange_totals`, `get_exchange_cost_totals`, `make_exchange_alist`, `make_exchange_cost_alist`, `make_exchange_function`, `exchange_by_pricedb_latest`, `exchange_by_pricedb_nearest`, `exchange_by_pricedb_nearest_before`, `exchange_by_pricealist_nearest`, `case_exchange_fn`, `case_price_fn`, `case_exchange_time_fn`, and `apply_exchange_fn`, `apply_price_fn`, `apply_exchange_time_fn` to call what those return |
| `lib/dates` | DU:176-493 | `leap_year`, `days_in_year`, `days_in_month`, the seven `date_to_*_fraction` functions, `date_get_fraction_func` and `apply_fraction_func`, `moddate`, `incdate`, `decdate`, `incdate_months`, `make_date_list`, the delta constants and `deltasym_to_delta`, `time64_start_day_time`, `time64_end_day_time` |
| `lib/options` | OU:44-234 | `add_report_date`, `add_date_interval`, `add_interval_choice`, `add_account_levels`, `add_account_selection`, `add_currency`, `add_price_source`, `add_plot_size`, `add_marker_choice`, `add_sort_method`, `add_subtotal_view` |
| `lib/acct-table` | HAT:524-1071 | `make_html_acct_table_env_accts` and `html_acct_table_add_accounts` (the balance calculation, the walk and the post-pass), `num_rows`, `get_row_env`, `add_labeled_amount_line`, `gnc_commodity_table` and `add_account_balances`, which append result rows (role, account, depth, level, cells) instead of HTML; the commodity table becomes a `commodities` cell |
| `lib/html-utilities` | HU:226-381 | `assign_colors` (GnuCash's 18-colour palette), `make_rates_table` (a result table), `make_generic_warning`, `make_generic_simple_warning`, `make_generic_budget_warning`, `make_generic_options_warning`, `make_no_account_warning`, `make_empty_data_warning` (warning blocks) |

Notes for the translators:

- **Keep the alist mechanics.** The collector conses a new commodity onto the front and `merge` walks the other list
  from its front, so merging reverses order (RU:347-368); `resolve-unknown-comm` adds entries at the front and
  `assoc` takes the first (CU:313-432). Which rate wins, and the order of a multi-commodity cell, depend on it.
  Collectors are Rhai arrays of `[commodity, Rational]` pairs for this reason, not maps, which would sort by key.
- **Two "weighted average"s.** Single-date reports use a book-wide volume ratio (`make-exchange-alist`, CU:599-607);
  time-series reports use a running average matched to the nearest date, later on ties (CU:281-290, :829-876).
- **Missing prices give 0 silently** in every path but `make-exchange-alist`, which divides by zero
  (CU:606); red-ale raises `report-script` there, as Guile raises an error.
- **Euro first.** Every exchange function tries the fixed euro rates before anything else, so EUR to EUR rounds to
  cents (CU:635-644, :663-758).
- **Fixed orders where GnuCash has none.** `gnc-accounts-and-all-descendants` returns a hash set
  (`5.17:bindings/engine.i:248-254`); `accounts_and_all_descendants` returns each given account followed by its
  descendants in display order, duplicates dropped. The Securities pie sums in a hash table
  (`standard/account-piecharts.scm:257-283`); red-ale keeps first-appearance order, so equal slices sort stably. No
  figure changes.
- **Locale collation.** Sorting by name or code uses `gnc:string-locale<?`; red-ale compares bytewise, as v0.1 does.
- **Dead code stays out**: names RU, DU and HAT export but never define (`gnc:accounts-get-comm-total-profit`,
  `gnc:account-code-less-p`, …) and `price-scatter.scm`'s unused `dates-list` (`standard/price-scatter.scm:141-144`).
- **Progress calls** (`gnc:report-starting`, `-percent-done`, `-finished`, RU:619-647) are dropped.
- **Function values are maps.** A closure made inside an imported Rhai module cannot be called once it leaves the
  module, so the exchange, price and fraction functions GnuCash returns as lambdas are maps (`#{ exchange_fn:
  "pricedb-nearest", book, date }`), called with `apply_exchange_fn`, `apply_price_fn`, `apply_exchange_time_fn`
  and `apply_fraction_func`. Each also takes a caller's closure. `case_exchange_time_fn` memoises its price tables
  by day in a `Memo` (§2), shared by every copy of the map.
- **Collectors are arrays with methods.** `c.add(comm, amt)`, `c.merge(other)` and the other actions are methods on
  the array that change it in place; `make_commodity_collector()` is `[]`. A value collector is its Rational total.
- **Keyword arguments and alists are maps**: `account_accumulate_at_dates(acc, dates, #{ split_to_elt, nosplit_elt })`,
  transaction types `#{ str, cased, regexp, closing }`, the account table's environment and parameters with
  GnuCash's keys (`display_tree_depth`, `parent_account_balance_mode`). A missing key, `()` and false read as #f.
- **The Book and run context are arguments** where GnuCash reads the current book or preferences: the split
  queries, `case_exchange_fn` and its kin, `make_html_acct_table_env_accts`.
- **Warnings drop the report id**: `make_no_account_warning(title)`; the options link it builds needs GnuCash's
  report instances. `add_labeled_amount_line` drops the colspans and markups, which GnuCash ignores or which are
  HTML, and takes a row role for the row markup.
- **Refused**: the account table's `report_budget` (budgets are outside v0.2), `account_less_p` #t (GnuCash
  names an undefined function, HAT:573), `account_accumulate_at_dates`'s `split_to_date` (no report in scope uses
  it), and a depth limit of #f without "flatten", which fails in GnuCash too (HAT:655-659).
- **CSV export.** net-charts and category-barchart offer CSV through GnuCash's export thunks; that is export, which
  moves to v0.3 with the rest of `features.md` §7 (`milestone2.md`, "Out"). Their "Show table" data tables are kept.

### Floating point in GnuCash, exact in red-ale

| Where | GnuCash | red-ale |
|-------|---------|---------|
| Day fraction for daily averages and day steps | `(/ caltime 3600.0)` (DU:258-259); category-barchart rounds the averaging divisor to 6 decimals (`standard/category-barchart.scm:299-319`) | exact seconds / 86,400, then the same 6-decimal rounding of the exact value. Differs only if the double lands on the other side of a rounding boundary |
| Pie percentages | `~2,1f` of a double (`standard/account-piecharts.scm:552-554`) | `fixed(r, 2, 1)` of the exact ratio |
| Price chart tooltip x | `~2,2F` (`standard/price-scatter.scm:260-267`) | `fixed(r, 2, 2)` |
| Category chart percentage cells | `double-to-gnc-numeric` to 1/100 (`report/html-style-info.scm:233-236`) | exact, banker's rounding to 1/100 |
| Chart values | JSON doubles (`5.17:borrowed/guile-json/json/builder.scm:121-124`) | exact Rationals in the result; integer SVG coordinates |

## 13. Testing reports

As decided in [`milestone2.md`](milestone2.md), "Testing reports": figures from GnuCash 5.17's own report tests,
hand-worked cases where they don't reach, and property tests. Tests live in `crates/red-ale-report/tests/` (engine
and reports) and `crates/red-ale-rpc/tests/reports.rs` (methods).

### Fixture books

GnuCash's report tests build their books in code with `test-engine-extras.scm`
(`5.17:bindings/guile/test/test-engine-extras.scm`): `create-test-data` (`:389-480`), `env-transfer` (`:199-223`),
`env-create-multisplit-transaction` (`:239-292`) and friends. red-ale rebuilds each book **with the oracle**: a
Python script per test under `scripts/report-fixtures/` makes the same engine calls through GnuCash 5.17's bindings
(`xaccTransSetDate`, `xaccTransVoid`, the price each foreign transfer adds as a side effect, `:279-286`) and saves an
XML book under `fixtures/reports/`. The book is then GnuCash's own, written by the pinned version, and the scripts
can be re-run to check it.

- The scripts run with `TZ=UTC`, since price times and absolute option dates are local midnight while posted dates
  are 10:59 UTC (`5.17:libgnucash/engine/gnc-date.cpp:1262-1265`, `:1323-1328`).
- Books dated "this month" or "this year" are built for a pinned now, **2024-03-15 12:00 UTC**, and the tests run
  the reports with that `now`.
- Each script cites the test it rebuilds and keeps its licence notice; the four tests without one
  (test-account-summary, test-balsheet-pnl, test-charts, test-average-balance) are cited with GnuCash's licence.
- Report tests use the settings GnuCash's tests run with, which have no preferences behind them:
  `reverseBalancedAccounts: "none"`, currency USD, an accounting period of `today` to `today` (with no preferences
  the period choice reads as 0, today, `5.17:libgnucash/engine/gnc-accounting-period.c:60-98`), and the
  date format each test sets (ISO for the Balance Sheet, Income Statement and chart tests, `locale` for the
  category report's day test).
- Every assertion becomes a named check (account or row label × column → display string), keeping GnuCash's
  expected string exactly (`"$116,006.33"`, `"30. FUNDS"`, `"$480 + 85/104"`). GnuCash's tests read text at HTML
  table positions (`5.17:gnucash/report/test/test-report-extras.scm:96-109`), which red-ale's markup does not share.

| Fixture | Rebuilt from | Used by |
|---------|--------------|---------|
| `reports/test-data.gnucash` | `create-test-data` (`test-engine-extras.scm:389-480`), GBP user symbol `#` (`standard/test/test-account-summary.scm:26-33`) | Account Summary; pie charts (hand-worked) |
| `reports/balsheet-pnl.gnucash` | `standard/test/test-balsheet-pnl.scm:51-166`, with the FUNDS commodity declared (GnuCash's test never adds it to the commodity table, `:55-61`) | Balance Sheet, Income Statement, Profit & Loss; Securities pie and Price Scatterplot (hand-worked) |
| `reports/average-balance.gnucash` | `standard/test/test-average-balance.scm:35-81`, all three transfers | Average Balance |
| `reports/charts-net.gnucash` | book B of `standard/test/test-charts.scm:85-114` | the four net charts |
| `reports/charts-daily.gnucash` | book A and the 101 daily transactions of `standard/test/test-charts.scm:148-200` | Net Worth Barchart, Income/Expense Chart |
| `reports/net-days.gnucash` | `standard/test/test-standard-net-barchart.scm:58-311` (the line chart test uses the same books, `test-standard-net-linechart.scm:55-198`) | the net charts, day steps |
| `reports/category-daily.gnucash`, `-average`, `-multi`, `-liability` | `single-txn-test` (which `asset-test` builds identically), `single-txn-test-average`, `multi-acct-test` and `liability-test` of `standard/test/test-standard-category-report.scm:81-318` | the category bar charts |

GUIDs are fixed and readable (`a000…0001` for the first account made, and so on), entered dates and void times are
the pinned now plus one second per transaction, and books are written uncompressed, so a script gives the same bytes
on every run; an oracle test checks that the committed books are what the scripts build. Next to each book,
`<name>.expected.json` lists every assertion of the test that uses it: template, the options in effect, the cells
GnuCash reads and the exact expected strings, with the test line (see `fixtures/README.md`).

### Per report

| Report | Expected figures from 5.17 | Hand-worked cases (each cites the Scheme lines it follows) |
|--------|----------------------------|------------------------------------------------------------|
| Account Summary | `standard/test/test-account-summary.scm:54-64`: account order; Bank $2,186.00; GBP Bank #608.00 and $912.00; Wallet $20.00; Income-GBP -#612.00 and -$918.00 | depth limit with summarize, flatten and truncate; parent subtotals; omit zero figures; exchange-rates table; reversed signs under `credit` |
| Balance Sheet | `standard/test/test-balsheet-pnl.scm:168-360`: totals for each price source, the multilevel and recursive runs, foreign currencies and the rates table, zero-balance options | two-column and non-US layouts; trading accounts on (trading line) and off (unrealized gains); retained losses |
| Income Statement | `standard/test/test-balsheet-pnl.scm:362-476`: revenue for each price source, multilevel and recursive runs, rates table | closing-entries pattern (plain, case-sensitive, regular expression); two-column and expense-first layouts; trading section |
| Profit & Loss | none: GnuCash's "pnl" test runs the Income Statement's GUID (`standard/test/test-balsheet-pnl.scm:19`) | the Income Statement's checks run against this template, plus its title |
| Average Balance | `standard/test/test-average-balance.scm:66-99`: average, maximum, minimum, profit, gain and loss, with and without "Exclude transactions between selected accounts" | other step sizes; a foreign-currency account under each price source; no splits in range |
| Net charts | `standard/test/test-charts.scm:116-146` (monthly, closing transaction excluded), `:201-238` (daily); `test-standard-net-barchart.scm:86-309` and `test-standard-net-linechart.scm:83-197` (day steps, closing test) | chart series values and colours; line width; series hidden by the Display options |
| Category bar charts | `standard/test/test-standard-category-report.scm:107-113` (daily amounts), `:149-207` (averages for every step and averaging pair), `:241-259` (depth, percentages), `:290-300` (assets), `:330-343` (credit-card liability) | stacking, "Maximum Bars" and the "Other" series, the sort methods, two currencies |
| Account pie charts | none: `test-charts.scm` and `test-stress-options.scm` only check that they render | every template on `test-data` and `balsheet-pnl`: slices, "Other" with "Maximum Slices" 2, percentages, totals, averaging, Securities by commodity |
| Price Scatterplot | none: the tests only reach the "Identical commodities" warning (`standard/test/test-charts.scm:35`, `:148-190`) | FUNDS in USD on `balsheet-pnl` for the three price sources, inverted, and each warning |

Every check of a figure also runs with `report.export` and checks that the HTML has the same display string.

### The budget module in `test-average-balance.scm`

`test-average-balance.scm:4` loads `(gnucash reports standard budget)`. That module exports nothing
(`standard/budget.scm:28-37`); loading it only registers the Budget Report template (`:763-769`). Every symbol the
test uses comes from the engine, report and test-helper modules. The import is an unused leftover, so the port
drops it and needs nothing from budgets.

### Properties and safety

- **Property tests** (`proptest`) on generated books: a Balance Sheet's "Total Assets" equals "Total Liabilities &
  Equity" as multi-commodity collectors, before any rounding; a pie chart's unrounded slices sum to its unrounded
  total; a category chart's "Grand Total" column equals the sum of its series; reports never fail on a book red-ale
  can open.
- **Read-only**: every template on every fixture leaves the Book equal to before, not dirty, and the journal
  absent (section 1).
- **Deterministic**: two exports of each template with the same Book, options and `now` give the same bytes, and the
  HTML contains no `<script`.
- **Sandboxed**: test scripts show that a float literal does not compile; `timestamp()` does not exist; `import`
  of an absolute path, of `../x` or of a symlink out of the report directory fails; `eval` is unknown; `print` lands
  in the log, not on standard output; each limit of section 2 raises `report-limit` with its `data.limit`; a script
  has no function that returns a mutable Book.

## 14. Deviations from GnuCash

| What | GnuCash 5.17 | red-ale | Why |
|------|--------------|---------|-----|
| Amounts past 64 bits | Fail at `gnc_scm_to_numeric`: an empty cell or an error (`5.17:bindings/guile/gnc-engine-guile.cpp:1709-1727`) | Exact, printed | Figures within 64 bits are the same |
| Floating point (day fractions, pie percentages, chart values) | Doubles | Exact, rounded as GnuCash displays | No floats anywhere in red-ale |
| Bad option values | Logged and ignored | `report-option` error | A typo never runs a different report |
| A split query's description pattern that is not a regular expression | The criterion is dropped (`qof_query_string_predicate` returns null, `5.17:libgnucash/engine/qofquerycore.cpp:319-334`) | `report-script` error | A typo never matches every transaction |
| A split query's regular expressions and case folding | POSIX extended expressions; `g_utf8_casefold` | Rust `regex`; lowercase, as `transaction.find` | Same results for patterns both dialects read alike |
| Saved configuration name clash | A number is appended on save | `conflict` | The caller gets the name it asked for |
| User text in style sheets | Inserted as raw HTML | Escaped | No injected markup in exported reports |
| CSS style sheet text | Default CSS on `</style` | Refused on `</style`, `@import`, `url(`, `image(`, `image-set(`, `src(`, `\` | Exported pages load nothing |
| Hover labels and chart look | Chart.js, `Intl.NumberFormat` | Static SVG, red-ale's formatting | No script; deterministic bytes |
| Drill-down links | Report instances made at render time | Described in the result; no `href` in HTML | Clients run them with `report.run` |
| Title of the account pie charts | None: the document keeps its empty title (`report/html-document.scm:131`); the chart's title and the warnings' heading name the report | The report name, with its averaging suffix | A result has a title, and its warning blocks leave out the heading that names the report |
| Unordered sets (`gnc-accounts-and-all-descendants`, the Securities pie) | Hash order | A fixed order | Determinism; no figure changes |
| Name sorting | Locale collation | Bytewise | As v0.1 |
| Date interval in titles under the `locale` format | ICU interval format | `<from> to <to>` | No locale |
| CSV export of the net and category charts | Offered | Not in v0.2 | Export is v0.3 |
| GnuCash's saved-reports and style-sheet files | Read at start-up | Not read | They are Scheme programs |

## 15. Left for later

- **Importing GnuCash's saved configurations and style sheets.** Their option forms are simple literal calls
  (`report/report-core.scm:620-643`); an importer that reads only those forms, never evaluating Scheme, would let
  users keep them. It needs a design of its own; planned for v0.3 ([`roadmap.md`](roadmap.md)).
- **Comparing with GnuCash's report output** through `gnucash-cli --report run` in a GUI build, as
  `open-questions.md` defers.
- **Interactive charts**, should anyone want them (`open-questions.md`, "Charts are static SVG").
