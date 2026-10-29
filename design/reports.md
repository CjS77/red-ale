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
the features `std`, `no_float`, `no_time` and `serde`; `red-ale-report` adds `metadata` (v0.6). Never `unchecked`,
`sync`, `internals`, `debugging` or `decimal`. `cargo deny` must pass with Rhai's dependency tree before the engine
ticket lands.

- `no_float`: no float type exists and a float literal is a syntax error.
- `no_time`: Rhai's `timestamp()` does not exist; time comes only from the run context.
- `serde`: turns the map a template returns into the typed report result (`rhai::serde::from_dynamic`).
- `metadata`: lists the engine's functions with their arity, so a script that would replace one is refused ("One
  name, one function"). Measured when it was added, in release builds: the binary grows by 1.45 MB (49.08 to
  50.53 MB, 3%), Rhai compiles in 10.4 s instead of 8.9 s, and a registry load of the built-in library takes 101 ms
  instead of 96 ms, since every function registered keeps its parameter and return type names. Building the
  standard package once per thread instead of once per engine took back 10 ms of the 15 ms the feature first cost.
- Not `sync`: each run builds its own `Engine` and ASTs on one thread; nothing is shared between runs but the
  standard package, which is immutable once built and built once per thread.

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
- The types and functions of sections 3, 5 and 6 are registered, and `memo()` and `make_regexp()`; nothing else
  is. A `Memo` is a table from integers to values that every copy shares, so a value a function stores is there for
  the next call: Rhai's arrays and maps are copied on assignment, and GnuCash's memoised closures
  (`case-exchange-time-fn`, `report/commodity-utilities.scm:829-887`) need a cache that outlives one call.
  `m.get(k)` (`()` when absent), `m.contains(k)`, `m.set(k, v)` and `m.len()`; it holds at most the map-size limit
  of entries (`map-size`).
- `make_regexp(pattern)` and `make_regexp(pattern, icase)` (v0.5) are Guile's `make-regexp`, with `regexp/icase`
  when `icase`: a `Regexp`, or `()` when the pattern does not compile, where Guile raises
  `regular-expression-syntax`. `re.is_match(text)` is `regexp-exec` finding a match anywhere. The patterns are Rust
  `regex` patterns, as `transaction.find` compiles them (section 14). The Transaction Report engine's account and
  transaction filters use them (`report/trep-engine.scm:2210-2254`).
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
different output.

**Embedded reports** (v0.5). A Multicolumn View's children (section 7, `grid`) each run as a separate engine run with
these limits, operations included, but under **one deadline**: the top-level run's start plus the run time limit, for
the whole tree. A child past any of these limits stops the whole run with `report-limit`, since the limits bound the
caller's request. Two limits of the tree itself render a warning in the child's cell instead, and nothing more runs
there: children nest at most **4 deep** below the top-level run (a view's children are at depth 1), and at most
**100 children** run in one top-level run, at every depth together. So does a **cycle**, a saved configuration
already being run further up the tree, which GnuCash's copied instances cannot form and red-ale's references can
(`notes/report-statements-charts.md` §8.5, Q6). Rhai's data-size limits stand in for a memory limit; the Rational size check keeps one runaway
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
| 3002 | `report-option` | An option value is not valid: unknown section or name, wrong type, not one of the choices, out of range, unknown account or commodity, a style sheet that does not exist, or an option the script made internal. `data`: `{ section, name, reason }` |
| 3003 | `report-result` | The template returned something that is not a valid report result. `data`: `{ report, path, reason, log }` |
| 3004 | `report-load`   | A user script does not compile, or its `templates()` fails, is malformed or repeats a GUID already defined, or it has `templates()` without `options` and `render`, or it and a module it imports define the same function, or it or a module it imports defines a function a call of an engine function would reach ("One name, one function", below). `data`: `{ source, line, column, message }` |

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

### One name, one function

No two scripts of a run may define a function with the same name and number of parameters. A run's scripts are the
template's script and every module it imports, directly or through other modules.

Rhai resolves a call by name and arity. It searches the libraries of the scripts on the call stack, the called
module's own first, but it keeps one function resolution cache for the whole run, keyed by name and arity alone,
and caches a resolution the second time it is looked up. Calling into a module adds the module's library to the
search but starts no new cache (rhai 1.27.0 `src/func/func_call.rs:367-423`, `src/func/script.rs:362-381`). So once
one script has looked a name up twice, every other script that calls that name gets the same function. The Advanced
Portfolio hit this: its own `total_of(env, collector)` ran inside `lib/collectors` in place of that module's private
`total_of(list, commodity)`. That run failed, but a function whose parameters had happened to fit would have given a
wrong figure without an error. No engine setting avoids this, so red-ale refuses the script instead:

- The resolver checks each module it evaluates against the functions of the template's script and of every module
  already imported, private functions included. A clash is an error at the `import` that brought the module in,
  naming both scripts and the function. At load, `templates()` runs the script's top-level imports, so a clashing
  user script is `report-load` and a clashing built-in script is `internal-error`. A module imported inside a
  function is checked when the run reaches it, as `report-script`.
- `templates`, `options` and `render`, which every template script defines, are allowed in two template scripts. The
  engine calls them through the template's own AST, and another script calls them only qualified
  (`transaction_report::render`), which bypasses the cache. A script must not call these three unqualified.

Rhai has a second behaviour. A call finds a script function before the engine's, by name and number of arguments
alone, whatever their types (rhai 1.27.0 `src/func/func_call.rs:818-835`), and a module function that calls a name
its module does not define finds the importing script's function of that name too, because the module's library is
added on top of the caller's and does not replace it (`src/func/script.rs:377`). A method call `x.f(a)` looks for a
script `fn f(a)` that reads `x` as `this` before the engine's `f(x, a)` (`src/parser.rs:2127-2136`). So a report
`fn abs(x)` would run in place of the standard package's `abs` in every script of the run, the libraries included,
and a report `fn len()` in place of every `x.len()`. Seeing a library's calls needs Rhai's `internals`, so red-ale
refuses the definition instead (v0.6):

- **No script defines a function with the name of an engine function of the same arity, or of one more.** The engine's
  functions are the standard package's and the report API's (section 5, `sleep`, `memo`, `make_regexp`, the option
  builder), read once from Rhai's metadata (`metadata` feature, "Build"). The template's script is checked when it is
  compiled and each module when it is imported, private functions included. The error is located at the function:
  Rhai keeps no position for one, so the script's text is searched for `fn <name>(`.
- A user script, or a user module it imports at its top level, is `report-load` at load, and a built-in one is
  `internal-error`. A module imported inside a function is checked when the run reaches it, as `report-script`.
- Arity counts: a report `fn abs(x, y)` replaces nothing and loads. A typed method (`fn int.f()`) is checked as an
  untyped one, which refuses a little more than Rhai would reach.

The Budget Report's `range` of `calc-periods` was the one built-in function this refused: it is `period_range`.
`tests/rhai_closure_limits.rs` pins both behaviours on a bare `Engine`, and `tests/engine_name_clash.rs` tests the
checks.

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
  `menuName`, `menuTip`, `inMenu` (default true), `exportTypes` (GnuCash's `'export-types`; only `["csv"]`, v0.3)
  and `params`, the values that tell one script's templates apart.
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
| `r.round_sigfigs(figs, mode)` (v0.5) | `gnc-numeric-convert` with `GNC-DENOM-SIGFIGS`: to `figs` significant figures with a mode as `round_to` takes, at the denominator `sigfigs_denom` picks (`5.17:libgnucash/engine/gnc-numeric.cpp:468-486`), its exponent capped at 18 as `powten` caps it; a whole part of `figs` digits or more rounds to an integer. The Investment Portfolio's average prices (`standard/portfolio.scm:193-200`) |
| `r.to_decimal(places)` | Text with exactly `places` decimals, rounded half-up; for labels |
| `fixed(r, width, places)` | `width` may be `()` for no padding. Guile's `~w,dF`: rounds half-up to `places` and pads to `width`, dropping the leading `0` of a value below 1 when the width allows no more (`.5`), as `standard/account-piecharts.scm:552-554` relies on |
| `monetary(commodity, r)`, `m.commodity`, `m.amount`, `m + m`, `m - m`, `-m`, `m.round_scu()` | Sums of two commodities are a `report-script` error. `round_scu` rounds to the commodity's SCU with banker's rounding, the `round-scu` of `standard/account-piecharts.scm:497-500` |
| `m.to_string()` | `gnc:monetary->string` (`report/report-utilities.scm:143-146`): the exact value, no rounding, with the commodity's symbol |
| `m.to_string(use_symbol)` (v0.5) | `xaccPrintAmount` with `gnc-commodity-print-info commodity use-symbol`: as `m.to_string()`, the symbol only when `use_symbol`; Investment Lots' "Show mnemonic in amounts" (`standard/investment-lots.scm:808-812`) |

GnuCash crosses back to `gnc_numeric` in a few places (`gnc-numeric-mul` in the exchange functions, `gnc-numeric-convert`
in `gnc:sum-collector-commodity`), and `gnc_scm_to_numeric` fails there when a part passes 64 bits
(`5.17:bindings/guile/gnc-engine-guile.cpp:1709-1727`). red-ale keeps those values exact. Within 64 bits the figures
are the same; beyond it, GnuCash fails or prints an empty cell and red-ale prints the figure (section 14).

### Growth rates

Investment Lots' CAGR raises a ratio of doubles to the power `1/years` (`calculate-cagr`,
`5.17:gnucash/report/reports/standard/investment-lots.scm:447-461`). A fractional power is irrational unless the ratio
is a perfect power, so it is the one report figure that cannot be exact. `growth_rate(ratio, years)` returns
`ratio^(1/years) − 1` as a Rational correctly rounded to 10⁻¹², ties to even; `years` is a Rational, since the report
computes it from day counts. It works as `exp(ln(ratio) / years)` in integer fixed point (`number/growth.rs`), with
enough fraction bits that the power is known within 2^-128 (at least 128 more than the power's whole part, plus guard
bits for the error each step adds). That settles the rounding unless the value lies that close to a half-unit
boundary; then an exact integer check finds a tie on the boundary (so a perfect power that lands on one rounds to
even), and otherwise the power is computed again to within 2^-1024. Whenever the result has twelve decimals or fewer
it is exact: 4 over 2 years is `1`, a ratio of 1 is `0`. A negative ratio, zero years, or a zero ratio over negative
years gives `()`, GnuCash's `#f` or `+inf.0`. `calculate_cagr(basis, end_value, years)` adds `calculate-cagr`'s guards,
which need the basis and end value apart: `()` when the basis is `()`, zero or negative, the end value is negative,
or years is `()` or zero. A power whose whole part would pass the Rational size cap stops the run with the
`number-size` limit; inputs that would need more than 16,384 working bits are a `report-script` error.

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
under the run's Locale as GnuCash reads it (`gnc_localeconv`, `5.17:libgnucash/core-utils/gnc-locale-utils.c:75-108`),
which replaces every empty value and `CHAR_MAX` with a US default. Under the C locale, the default and the one
GnuCash's own report tests use (`setlocale LC_ALL "C"`, `standard/test/test-average-balance.scm:15`):

- grouping by three with `,`, decimal point `.`, minus sign first: `-$1,234.56`;
- a currency (namespace `CURRENCY`) has its symbol first with no space, and as many decimals as its SCU, padded;
- any other commodity has its symbol after the number with a space, and no padding of decimals; the decimal point is
  always printed, so a whole amount of a 1/1000 commodity is `30. FUNDS` (`standard/test/test-balsheet-pnl.scm:247`).
  GnuCash 5.11 and earlier left the point out (`5.5:libgnucash/app-utils/gnc-ui-util.c:1322`). Commit `5941167132`
  (Bug 797766, first in 5.12) began always printing it, to fix a GUI bug: with "automatic decimal point" on, leaving
  a register field holding a whole share amount moved its decimal point. `PrintAmountInternal` also formats reports,
  so their output changed with it, and the commit updated the report tests to the new form. red-ale has no register,
  so the reason does not apply; it prints the point only to match the pin. red-ale will likely leave it out again
  later, as a deviation recorded in §14;
- a remainder that is not a decimal fraction prints as `w + n/d`;
- the symbol is the commodity's *nice symbol*: its user symbol, else the ISO table's symbol, else its mnemonic
  (`5.17:libgnucash/engine/gnc-commodity.cpp:1095-1115`).

Under another Locale (v0.3, [`milestone3.md`](milestone3.md), "Locales"), every report figure takes LC_MONETARY's
`mon_decimal_point`, `mon_thousands_sep` and `mon_grouping`, since GnuCash's report renderers all print with
`monetary` set; only the first character of a separator or point is used, as GnuCash copies one. A currency's symbol
and sign go where the locale's `p_`/`n_cs_precedes`, `sep_by_space` and `sign_posn` put them (fr_FR: `-1 234,56 €`
with NARROW NO-BREAK SPACE; fr_CA brackets a negative number: `(5,00) $`); any other commodity keeps its symbol after
the amount with a space. Number cells with no `places` pad to the locale's `frac_digits`. GnuCash would print the
locale's `currency_symbol` for the currency whose code equals `int_curr_symbol`
(`5.17:libgnucash/engine/gnc-commodity.cpp:1105-1108`), but glibc's `int_curr_symbol` always ends in a space
(`EUR `), so no code equals it and the nice symbol always applies; the locale generator checks that this holds.
Guile's `~w,dF` (`fixed`) does not follow the locale, in GnuCash or here.

## 4. Settings reports read

Some figures depend on GnuCash user preferences, which are not in the Book. red-ale reads them from its
Configuration (section 9), with GnuCash's out-of-the-box values as defaults, and passes them in the run context:

| Setting (`reports.*`)     | GnuCash preference                                     | Default    | Used by |
|---------------------------|--------------------------------------------------------|------------|---------|
| `currency`                | `general.report` `currency-choice-other`/`currency-other`, else the locale's currency (`gnc_default_report_currency`, `5.17:libgnucash/app-utils/gnc-ui-util.cpp:748-822`) | unset: the Locale's currency (`int_curr_symbol`) if the Book has it, else `CURRENCY::USD` (GnuCash's fallback, `:724-745`); USD under C | the default of every "Report's currency" option |
| `reverseBalancedAccounts` | `general` `reversed-accounts-{none,credit,incomeexpense}` (`5.17:libgnucash/app-utils/gnc-ui-util.cpp:128-173`) | `credit`: LIABILITY, PAYABLE, EQUITY, INCOME and CREDIT (not CREDITLINE or TRADING), the schema default (`5.17:gnucash/gschemas/org.gnucash.GnuCash.gschema.xml.in:93-107`) | account rows of the statement tables (`report/html-acct-table.scm:1029-1030`) |
| `dateFormat`              | `general` `date-format` (`5.17:libgnucash/engine/gnc-date.cpp:504-521`) | `locale`   | every printed date (`qof-print-date`) |
| `weekStart`               | the locale's first day of the week (`gnc-start-of-week`, `5.17:libgnucash/engine/gnc-date.cpp:195-215`), which GnuCash asks ICU for | unset: the Locale's, from ICU4X's CLDR data; `sunday` under C | week fractions in averages (`5.17:bindings/guile/date-utilities.scm:244-251`) |
| `accountingPeriod` (existing key, [`api/accounts.md`](api/accounts.md)) | `window.pages.account-tree.summary` | this calendar year | the accounting-period date choices |

`dateFormat` is one of `us` (`%m/%d/%Y`), `uk` (`%d/%m/%Y`), `ce` (`%d.%m.%Y`), `iso` (`%Y-%m-%d`) and `locale`.
`locale` is the Locale's `D_FMT` (`5.17:libgnucash/engine/gnc-date.cpp:61`, `:522`): `%m/%d/%y` under the C locale
GnuCash's report tests use (`standard/test/test-standard-category-report.scm:107-113` reads that form), `%m/%d/%Y`
under en_US (`12/31/2026`), `%d/%m/%Y` under fr_FR. As GnuCash does, the `E`, `O` and `-` modifiers are dropped
before formatting (`normalize_format`, `5.17:libgnucash/engine/gnc-datetime.cpp:517-533`), so `%-d` prints
zero-padded. A date interval in a title is `<from> to <to>`, GnuCash's form for every format but `locale`, where it
asks ICU for the `yMd` interval (`5.17:libgnucash/engine/gnc-date.cpp:618-680`): `1/1/2026 – 12/31/2026` under
en_US. red-ale takes it from ICU4X for every Locale but C, which keeps `<from> to <to>` (section 14).

The Locale of a run is the `locale` param of `report.run` or `report.export`, else the Configuration's `locale`
setting, default `C` ([`api/configuration.md`](api/configuration.md)). Its data, and what is taken from where, are in
[`milestone3.md`](milestone3.md), "Locales", and the `red-ale-locale` crate's documentation.

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
| `book.uses_split_action_for_num()` (v0.5) | bool | `book.info` | `qof-book-use-split-action-for-num-field`: the num field source, which the Transaction Report's option names and Num cells follow |
| `book.company_name()` | string, `""` when unset | | `gnc:company-info` |
| `book.option(path)` (v0.6) | string, integer, Rational, GUID text, Time64 or `()` | `book.options` | `qof-book-get-option` (`gnc:book-get-option-value`, `5.17:bindings/guile/options.scm:85-87`): the slot `options/<path>`, `path` being `"Section/Name"` or deeper (`"Business/Fancy Date Format/custom"`). A double comes back as the exact Rational of its stored text, a frame, list or date as `()`, as does an absent option. The invoice reads the business options and the Fancy Date Format through it |
| `book.invoice(guid)` (v0.6) | Invoice or `()` | `invoice.get` | `gncInvoiceLookup`: any document, invoice, bill, expense voucher or credit note. A GUID naming none, or text that is no GUID, is `()` |
| `book.prices(commodity, currency)` | Price[], newest first, ties by GUID | `price.list` | `gnc-pricedb-get-prices`, forward only (`5.17:libgnucash/engine/gnc-pricedb.cpp:2047-2058`) |
| `book.prices_any_currency(commodity, when, mode)` (v0.5) | Price[], newest first | | `gnc-pricedb-lookup-nearest-in-time-any-currency-t64`, `-nearest-before-any-currency-t64` and `-latest-any-currency` (`5.17:libgnucash/engine/gnc-pricedb.cpp:1955-2001`): for each other commodity `commodity` has a price with, either way round, the one nearest `when`, the newest at or before it, or (`"latest"`) the newest at or before the run's now. Prices come as stored: a script inverts one whose `commodity` is not the one asked for, as the investment reports do. A price at exactly `when` may be listed twice under `"nearest"`, as GnuCash lists it |
| `book.price_rate(commodity, currency, when, mode)` | Rational | `price.nearest`, `price.latest` | `gnc-pricedb-get-{nearest,nearest-before,latest}-price`: 1 for the same commodity; a direct price, inverted when quoted the other way; else an indirect price through a commodity both are priced in; 0 when there is none (`:2408-2527`) |
| `book.price_convert(monetary, currency, when, mode)` | Monetary | | `gnc-pricedb-convert-balance-*`: amount × `price_rate`, rounded to the currency's SCU with banker's rounding; 0 when no price (`:2529-2586`) |
| `book.commodity_splits(accounts, until, commodity, sorted)` | Split[] | | `gnc-get-match-commodity-splits`: non-void splits of `accounts` up to `until` (inclusive) whose transaction currency differs from their account's commodity and, unless `commodity` is `()`, one of the two is `commodity` (`5.17:bindings/engine.i:167-193`). Unsorted, account after account, each in split order (`get-exchange-totals` relies on it, CU:447); `sorted`, all in split order. Callers pass the root's sorted descendants (CU:203-205) or those holding shares (`standard/price-scatter.scm:147-149`) |
| `book.split_query(q)` | Split[], in split order unless `q.sort` says otherwise | `transaction.find` | the split query of `gnc:account-get-trans-type-splits-interval` (`report/report-utilities.scm:692-725`), which the Average Balance report also runs (`example/average-balance.scm:300-345`), and of the Transaction Report engine (`report/trep-engine.scm:2533-2553`): `q` is `#{ accounts, from, to, description: #{ text, cased, regexp }, closing, reconcile, isClosing, sort, unique }`, below |
| `is_euro_currency(c)`, `euro_convert(m, currency)` | bool, Monetary or `()` | `currency.euroConvert` | `gnc-is-euro-currency`, `gnc-convert-to-euro`/`-from-euro` (`5.17:libgnucash/engine/gnc-euro.cpp:31-106`); `()` unless both are euro currencies, as `gnc:exchange-by-euro` (`report/commodity-utilities.scm:635-644`). `c` may be a currency the Book does not hold |
| `book.sx_cashflow(from, to)` (v0.4) | map of account GUID → Rational | | `gnc-sx-all-instantiate-cashflow-all` (`5.17:libgnucash/app-utils/gnc-sx-instance-model.c:1968-1976`): what every enabled scheduled transaction adds to each account from the local date of `from` to that of `to`, both inclusive (`gnc_time64_to_GDate`, `5.17:bindings/guile/expressions.i:27-32`), in the account's commodity, debits positive, computed from the model without creating anything ([`notes/engine-budgets.md`](notes/engine-budgets.md) §4.10). Occurrences count on from the last one, which is not counted again; Deferred instances play no part; formulas are read in the Locale's monetary separators with every variable 0, and what fails counts as 0. An account that no counted template split names is absent; one whose sums cancel is 0 |

`mode` is `"nearest"`, `"before"` or `"latest"`; `when` is a Time64. `"latest"` keeps GnuCash's split behaviour:
the direct lookup takes the newest price even if it is in the future, the indirect one the newest at or before now
(`5.17:libgnucash/engine/gnc-pricedb.cpp:1731-1750`, `:1955-1960`).

The split query's fields, each optional:

- `accounts`: the accounts the splits are in (`xaccQueryAddAccountMatch` with `QOF-GUID-MATCH-ANY`); none finds
  nothing. `from`, `to`: posted at or after, at or before, to the second (`xaccQueryAddDateMatchTT`).
- `description` and `closing`: the transaction's description matches, or (`closing: true`) it is a Closing
  transaction; the two are OR'ed with each other and AND'ed with the rest, as
  `gnc:account-get-trans-type-splits-interval` merges them.
- `reconcile` (v0.5): the states a split may be in, named as `transaction.find`'s `reconcile.states` (`"n"`, `"c"`,
  `"y"`, `"f"`, `"v"`; `xaccQueryAddClearedMatch`, `5.17:libgnucash/engine/Query.cpp:474-503`). Without it every state
  but `v`: void splits match only when asked for, as both GnuCash queries leave them out.
- `isClosing` (v0.5): the transaction is (`true`) or is not (`false`) a Closing transaction, AND'ed
  (`xaccQueryAddClosingTransMatch`, `report/trep-engine.scm:2539-2540`), unlike `closing`.
- `sort` (v0.5): up to three levels, primary first, each `#{ key, ascending }` (`ascending` defaults to true), as
  `qof-query-set-sort-order` and `qof-query-set-sort-increasing` take them. The first level that tells two splits apart
  decides, negated when descending (`5.17:libgnucash/engine/qofquery.cpp:345-374`); ties at every level keep split order.
  A level with key `"none"` compares every two splits equal; with `"none"` first nothing is sorted, which in GnuCash is
  hash order and in red-ale split order (section 14). Without `sort`, split order (`QUERY-DEFAULT-SORT`). The keys,
  each a QOF sort parameter, compared as `SortKey` in `ops::transactions::sort` documents:

  | Key | QOF parameter | Compares |
  |-----|---------------|----------|
  | `accountFullName` | `SPLIT-ACCT-FULLNAME` | the account paths from the top, the first differing accounts' names; the same account is equal, an ancestor less (`Split.cpp:1687-1709`) |
  | `accountCode` | `SPLIT-ACCOUNT`, `ACCOUNT-CODE-` | account codes, `""` when unset |
  | `corrAccountFullName`, `corrAccountCode` | `SPLIT-CORR-ACCT-NAME`, `SPLIT-CORR-ACCT-CODE` | the Split reads of the same names (`Split.cpp:1725-1757`) |
  | `datePosted`, `dateEntered` | `SPLIT-TRANS`, `TRANS-DATE-POSTED` / `-ENTERED` | the transaction's times, to the second |
  | `dateReconciled` | `SPLIT-DATE-RECONCILED` | reconcile times, 0 when never reconciled |
  | `value`, `amount` | `SPLIT-VALUE`, `SPLIT-AMOUNT` | exactly |
  | `num` | `SPLIT-TRANS`, `TRANS-NUM` | the transaction's num by its own compare function, `qof_string_number_compare_func` (`Transaction.cpp:2879-2883`, `qofquerycore.cpp:234-262`): the numbers `strtol` reads at the start (0 when none), then the rest of each string; `"9"` before `"10"` |
  | `action`, `memo` | `SPLIT-ACTION`, `SPLIT-MEMO` | the strings, byte by byte (`g_strcmp0`, `qofquerycore.cpp:194-207`) |
  | `description` | `SPLIT-TRANS`, `TRANS-DESCRIPTION` | the strings, byte by byte |
  | `default` | `QUERY-DEFAULT-SORT` | split order |

- `unique` (v0.5): only the first split of each transaction, in the sorted order (`xaccQueryGetSplitsUniqueTrans`,
  `Query.cpp:67-90`).

### Account

Properties: `guid`, `name`, `fullName`, `type`, `typeString` (the type's label, "Asset", as
`xaccAccountGetTypeStr` gives it), `code`, `description`, `notes`, `placeholder`, `hidden`, `commodity` (Commodity),
`parent` (Account or `()`), `depth` (levels below the root), and from v0.5 `debitLabel` and `creditLabel`: the friendly
names of a debit and a credit in an account of its type ("Deposit" and "Withdrawal" for a bank account), untranslated,
as `gnc-account-get-debit-string` and `-credit-string` give them without the "use formal accounting labels"
preference, which red-ale does not have (`5.17:libgnucash/engine/Account.cpp:148-185`, `:4079-4101`); root and the
legacy bank types get "Debit" and "Credit".

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
| `a.currency_or_parent()` (v0.5) | Commodity or `()` | `gnc_account_get_currency_or_parent` (`Account.cpp:3403-3412`): the account's commodity if it is a currency, else the nearest ancestor's that is |
| `a.compare_order(b)` (v0.5) | -1, 0 or 1 | `xaccAccountOrder` (`Account.cpp:2363-2413`) of any two accounts, siblings or not: code, type, name, GUID, as display order compares siblings. The Transaction Breakdown sorts its account columns by it across the tree (`standard/txn-columns.scm:116-117`), which is not depth-first display order |

### Split, Transaction, Commodity, Price

- **Split**: `guid`, `account`, `transaction`, `amount`, `value`, `memo`, `action`, `reconcile`, `reconcileDate`,
  `balance`, `noclosingBalance` (`xaccSplitGetNoclosingBalance`, `5.17:libgnucash/engine/Split.cpp:1322-1325`),
  `sharePrice`; from v0.5:
  - `otherSplit`: the one other split, or `()` (`xaccSplitGetOtherSplit`, `5.17:libgnucash/engine/Split.cpp:2161-2185`):
    splits in trading accounts and cut splits (with a `lot-split` slot) do not count, and more than one other is `()`;
  - `corrAccountFullName`, `corrAccountCode`: the other split's account's full name and code, or
    `"-- Split Transaction --"` and `"Split"`, untranslated, when the transaction has more than two splits (all
    counted) or no one other split (`xaccSplitGetCorrAccountFullName`, `-Code`, `Split.cpp:1621-1684`; the strings
    at `:1663` and `:1679`). The Transaction Report sorts by these; its Transfer cell reads `otherSplit` instead;
  - `voidFormerAmount`, `voidFormerValue`: what a voided split held, 0 when it never was voided
    (`xaccSplitVoidFormerAmount`, `-Value`, `Split.cpp:2190-2204`);
  - `lot`: the split's Lot, or `()`;
  - `isStockSplit`: its `split-type` slot is `stock-split` (`xaccSplitGetType`, `Split.cpp:2039-2058`), as
    `xaccSplitMakeStockSplit` marks one;
  - `actionNum`: the split's action, or the transaction's num when the book uses split actions for nums
    (`gnc-get-action-num`, `5.17:libgnucash/engine/engine-helpers.c:85-102`), which IFRS cost basis matches its fees
    filter against.
- **Transaction**: `guid`, `currency`, `datePosted` (Time64), `dateEntered`, `num`, `description`, `notes`,
  `isClosing` (the `book_closing` slot), `isVoid`, `splitCount`, `splits()`, and from v0.5 `docLink` (`""` when
  none; `xaccTransGetDocLink`, `5.17:libgnucash/engine/Transaction.cpp:2217-2221`), and `t.order(other)`: −1, 0 or 1
  as `xaccTransOrder` orders two transactions (`Transaction.cpp:1757-1761`, `:1794-1838`), by which Investment Lots
  sorts one day's purchases.
- **Lot** (v0.5): `guid`; `invoice`, `#{ guid, id }` of the business document the lot's `gncInvoice/invoice-guid`
  slot names, or `()` (`gncInvoiceGetInvoiceFromLot`), which the Transaction Report's Invoice column reads; `title`
  and `notes`, `()` when unset (`gnc-lot-get-title`, `-notes`, `5.17:libgnucash/engine/gnc-lot.cpp:444-460`);
  `account`; `splits()`, in the lot's list order (`gnc-lot-get-split-list`, `:424-431`): the order they joined it,
  which red-ale takes as Book order, as loading a file adds them (`gnc_lot_add_split` appends, `:619`). Investment
  Lots reads these and sorts the splits itself; no report reads the engine's lot balance or closure. The General
  Journal's Lot column shows `title`, and nothing for an untitled lot.
- **Commodity**: `id` (`NAMESPACE::MNEMONIC`), `namespace`, `mnemonic`, `fullname`, `fraction` (SCU), `isCurrency`,
  `symbol` (the nice symbol of section 3), `uniqueName`.
- **Price**: `guid`, `commodity`, `currency`, `time`, `source`, `type`, `value`.

"Split order" is `xaccSplitOrder` (`5.17:libgnucash/engine/Split.cpp:1536-1597`) and "display order" is
`xaccAccountOrder` (`Account.cpp:2363-2413`), both as v0.1 implements them, with names compared bytewise
([`api/accounts.md`](api/accounts.md)).

### Budget (v0.4)

Properties: `guid`, `name`, `description`, `numPeriods`, as `BudgetInfo` names them ([`api/budgets.md`](api/budgets.md)).
Periods count from 0; every read takes any period, past the count too, as GnuCash's reports ask
([`notes/engine-budgets.md`](notes/engine-budgets.md) §4.3). Bounds are in the run's local time zone.

| Function | Returns | GnuCash (`5.17:libgnucash/engine/gnc-budget.cpp`) |
|----------|---------|---------|
| `book.budgets()` | Budget[], book order | the budget collection |
| `book.budget(guid)` | Budget or `()` | `gnc-budget-lookup` |
| `book.default_budget()` | Budget or `()` | `gnc-budget-get-default` (`:720-752`): the `Default Budget` option's budget, else the first in book order, where GnuCash takes hash order |
| `b.period_start_date(p)`, `b.period_end_date(p)` | Time64 | `gnc-budget-get-period-start-date`, `-end-date` (`:647-659`): local 00:00:00 of the period's first day, 23:59:59 of its last |
| `b.account_period_value(a, p)` | Rational | `gnc-budget-get-account-period-value` (`:587-597`): 0 when unset or past the count |
| `b.is_account_period_value_set(a, p)` | bool | `gnc-budget-is-account-period-value-set` (`:578-585`) |
| `b.account_period_note(a, p)` | string or `()` | `gnc-budget-get-account-period-note` (`:638-645`) |
| `b.account_period_actual_value(a, p)` | Rational | `gnc-budget-get-account-period-actual-value` (`:661-669`) |

`a` is an Account or its GUID or full name, as elsewhere.

The Future Scheduled Transactions Summary and the Balance Forecast read `book.sx_cashflow`; the summary passes it to
the account table through `get_balance_fn`, as `standard/account-summary.scm:327-336` does. GnuCash's GUI may give a
formula's variable a value its calculator kept (§4.10 step 4 of the note); red-ale reads every variable as 0, as
GnuCash does with no such value.

### Invoice, Entry, Owner, Order, BillTerm (v0.6)

The reads of the Printable invoice (`notes/report-invoice.md` §1.3, §4), with `InvoiceDetail`'s and `EntryInfo`'s
names ([`api/business-documents.md`](api/business-documents.md)). Each figure is exact; a script never computes one.

- **Invoice**: `guid`, `id`, `type` (`invoice`, `bill`, `expenseVoucher` or `creditNote`, as `InvoiceDetail` names
  them), `isCustomerDocument` (the end owner is a customer: GnuCash's `cust-doc?`, `standard/invoice.scm:769-782`),
  `isCreditNote`, `owner` (Owner), `currency` (Commodity or `()`), `posted`, `datePosted` and `dateDue` (Time64, `()`
  unless posted), `terms` (BillTerm or `()`; a posted document's is the frozen copy, which keeps the description),
  `billingId`, `notes`, `postedLot` (Lot or `()`), `postedTransaction` (Transaction or `()`); and
  - `inv.entries()`: Entry[], in document order (`gncEntryCompare`);
  - `inv.totals()`: `#{ subtotal, tax, total, taxes }`, signed as printed on the document (`gncInvoiceGetTotalSubtotal`,
    `-TotalTax`, `-Total`, `-TotalTaxList`, `5.17:libgnucash/engine/gncInvoice.c:917-1038`); `taxes` is
    `[#{ account, value }]` in GnuCash's list order, each the account's unrounded taxes summed, then rounded once.
- **Entry**: `guid`, `date`, `description`, `action`, `notes`, `quantity` (as stored, negated on a credit note),
  `documentQuantity` (as printed, `gncEntryGetDocQuantity`), `invoicePrice`, `billPrice`, `discount`, `discountType`
  (`percent` or `value`), `discountHow` (`preTax`, `sameTime`, `postTax`), `invoiceTaxable`, `billTaxable`,
  `invoiceTaxTable` and `billTaxTable` (the table's GUID or `()`), `order` (Order or `()`); `documentValue` and
  `documentTaxValue`: the value and tax of its document's side, rounded half up to the currency, the tax rounded at
  each step of its sum, negated on a credit note (`gncEntryGetDocValue`, `-DocTaxValue` with `round` true,
  `5.17:libgnucash/engine/gncEntry.c:1431-1442`, `:1535-1546`).
- **Owner**: `kind` (`customer`, `job`, `vendor`, `employee`), `guid`, `id`, `name` (`gncOwnerGetName`: an
  employee's is its address name, `5.17:libgnucash/engine/gncEmployee.c:512-517`), `address`
  (`#{ name, addr1, addr2, addr3, addr4, phone, fax, email }`, `()` for a job), `owner` (a job's customer or vendor,
  else `()`).
- **Order**: `guid`, `id`, `reference`. **BillTerm**: `guid`, `name`, `description`.

`lib/business` translates the owner helpers of `5.17:bindings/guile/business-core.scm:35-105` on top of these.

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
| `book.option(path)` | `Slots::get` of `options/<path>` | v0.6 |
| `book.invoice(guid)`; Invoice `id`, `billingId`, `notes`, `posted`, `datePosted`, `owner`, `currency`, `terms`, `postedLot`, `postedTransaction` | `Book::business.invoices`, the `Invoice` fields | v0.6 |
| Invoice `type`, `isCustomerDocument`, `isCreditNote` | `ops::business::calc::invoice_type` | v0.6 |
| Invoice `dateDue` | `ops::business::posting::date_due` | new (v0.6) |
| `inv.entries()`; Entry fields, `order` | `ops::business::documents::entries_of`; the `Entry` fields | v0.6 |
| `inv.totals()` | `ops::business::calc::document_totals` | v0.6 |
| Entry `documentQuantity`, `documentValue`, `documentTaxValue` | `documents::document_quantity`, `documents::entry_on_credit_note`; `calc::entry_amounts` (`value_rounded`, `tax_rounded`) | v0.6 |
| Owner `name`, `owner`; `address`, `id` | `ops::business::contacts::owner_name`, `end_owner`; the contact's fields | v0.6 |
| `book.prices(commodity, currency)` | `ops::prices::prices(book, commodity, Some(currency), false)`; a date range is a filter on `time`, as the Price Scatterplot filters (`standard/price-scatter.scm:220-229`) | |
| `book.prices_any_currency(…)` | `ops::prices::convert::nearest_any_currency`, `latest_before_any_currency` (at now for `"latest"`) | v0.5 |
| `book.price_rate(…)` | `ops::prices::convert::rate` with `PriceAt::{Nearest, Before, Latest { now }}` | new |
| `book.price_convert(…)` | `ops::prices::convert::convert_balance`, after the Monetary's Rational is made a Numeric (past 64 bits GnuCash fails at the same place, section 3) | new |
| `book.commodity_splits(…)` | `ops::transactions::find::cross_commodity_splits` | new |
| `book.split_query(q)` | `ops::transactions::find::period_splits` with a `PeriodQuery`; its sort `ops::transactions::sort::sort_splits` over `SortLevel`s | new (sort, v0.5) |
| `book.uses_split_action_for_num()` | `book_options::uses_split_action_for_num` | |
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
| Split `otherSplit` | `ops::transactions::other_split` | |
| Split `corrAccountFullName`, `corrAccountCode` | `ops::transactions::corr_account_full_name`, `corr_account_code` | new (v0.5) |
| Split `voidFormerAmount`, `voidFormerValue` | `ops::transactions::void::void_former_amount`, `void_former_value` | new (v0.5) |
| Split `lot`; Lot `invoice` | `Split::lot`, `Book::lot`; `ops::business::payments::lot_invoice` | |
| Lot `title`, `notes`, `account`; `splits()` | `Lot::title`, `Lot::notes`, `Lot::account`; `ops::lots::lot_split_list` | new (v0.5, `lot_split_list`) |
| Split `isStockSplit`, `actionNum` | `lot::is_stock_split`; `book_options::uses_split_action_for_num` | v0.5 |
| `a.currency_or_parent()` | `ops::scrub::currency_or_parent` | v0.5 |
| `a.compare_order(b)` | `ops::accounts::account_order` | v0.5 |
| Split `lot`; Lot `invoice`, `title` | `Split::lot`, `Book::lot`; `ops::business::payments::lot_invoice`; `Lot::title` | |
| Transaction `docLink` | `ops::doc_links::doc_link` | |
| Account `debitLabel`, `creditLabel` | `ops::accounts::debit_label`, `credit_label` | new (v0.5) |
| Transaction `isClosing`, `isVoid` | `Transaction::is_closing`, `Transaction::is_void` | |
| Commodity `symbol` | `ops::commodities::display_symbol` | |
| `ctx.accounting_period()` | `ops::period::period_bounds` | |
| `book.budgets()`, `book.budget(guid)`, `b.account_period_value`, `b.is_account_period_value_set`, `b.account_period_note` | `Book::budgets`, `Book::budget`, `Budget::amount`, `Budget::note` | new (v0.4) |
| `book.default_budget()` | `ops::budgets::default_budget` | new (v0.4) |
| `b.period_start_date(p)`, `b.period_end_date(p)` | `ops::budgets::period_times` | new (v0.4) |
| `b.account_period_actual_value(a, p)` | `ops::budgets::actual` | new (v0.4) |
| `book.sx_cashflow(from, to)` | `ops::scheduled::cash_flow` over `TimeZone::local_date` of each time | new (v0.4) |

Core also has the reads `lib/balances` and clients may want beyond these: `balances_at_dates`
(`gnc:account-get-balances-at-dates`, the sum of amounts at or before each time, which counts the stock splits the
engine skips), `balance_as_of_in_commodity` (`xaccAccountGet{,Noclosing}BalanceAsOfDateInCurrency`, sub-accounts
converted at the price before) and `balance_change` (`xaccAccountGet{,Noclosing}BalanceChangeForPeriod`). No JSON-RPC method is added: every
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
| `ctx.print_time(t, format)` (v0.6) | `gnc-print-time64` (`5.17:libgnucash/engine/gnc-date.cpp:370-385`) under a strftime format, the local date of `t`, month and day names from the Locale. The invoice prints its posted and due dates in the Book's Fancy Date Format with it. Only date directives (`%d %e %m %y %Y %a %A %b %h %B %%`) are known; any other prints as written (section 14) |
| `ctx.print_month_year(t)`, `ctx.print_year(t)` | `gnc:date-get-month-year-string`, `gnc:date-get-year-string`: `gnc-print-time64` with `%B %Y` and `%Y` (`5.17:bindings/guile/date-utilities.scm:151-152`, `:166-167`), month names from the Locale: `March 2024`, `mars 2024` under fr_FR (v0.5) |
| `ctx.print_time_iso(t)` | `gnc-print-time64` with the ISO format, for CSV dates: `YYYY-MM-DD HH:MM:SS` in local time |
| `ctx.print_date_iso(t)` | `gnc-print-time64` with `QOF-DATE-FORMAT-ISO`'s date format, for the charts' CSV dates: `YYYY-MM-DD` in local time (v0.3) |
| `ctx.print_timestamp(t)` | none: the wire form of a Timestamp (RFC 3339, UTC), so a report link can set an absolute date option to an exact time, as the Budget Report's links to the Transaction Report do (`standard/budget.scm:381-382`) (v0.4) |
| `ctx.export_type()` | The `export-type` GnuCash passes to an export thunk (`5.17:gnucash/report/report-core.scm:722`): `"csv"` when the run is for `report.exportCsv`, else `()` (v0.3) |
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
| `budget` | `GncOptionQofInstanceValue` with the budget UI type (`5.17:libgnucash/engine/gnc-optiondb.cpp:657-665`) (v0.4) | the budget's GUID; `null` when the Book has none to default to | Budget or `()` (also for a budget since deleted) |
| `invoice` (v0.6) | `GncOptionQofInstanceValue` with UI `INVOICE` (`5.17:libgnucash/engine/gnc-optiondb.cpp:915-923`), serialized as the GUID (`5.17:libgnucash/engine/gnc-option-impl.cpp:259-284`) | a business document's GUID, or `null` (the default) for none. Any GUID is kept, as a budget's is: a saved configuration may be made with no Book open | Invoice, or `()` for none and for a GUID that names no document of the Book, as GnuCash's failed lookup leaves the option at `'()` |
| `image` (v0.6) | `GncOptionValue<std::string>` with UI `PIXMAP` (`gnc-optiondb.cpp:725-732`) | a string: a picture's file name, which the server never reads (section 8) | string |
| `reportPlacement` | `GncOptionValue<GncOptionReportPlacementVec>`, UI `REPORT_PLACEMENT`: `(id, wide, high)` per child, `id` a report instance of the session (`5.17:libgnucash/engine/gnc-option.hpp:71-72`) (v0.5) | array of `{ report, options?, colspan, rowspan }`, below | the wire form: an array of maps |

**Report placements** are a Multicolumn View's children, GnuCash's `__general`/`report-list`
([`notes/report-statements-charts.md`](notes/report-statements-charts.md) §8.4). `report` is the GUID of a template
or a saved configuration, never a name: names change and clash. `options`, only with a template's GUID, are changes
over that template's defaults as `report.run` takes them; a saved configuration child runs with its own values and is
a reference, following its later changes, where GnuCash copies a report's options into a new instance. `colspan`
and `rowspan` are GnuCash's `wide` and `high`, integers 0 to 100 as its size dialog allows
(`5.17:gnucash/gtkbuilder/dialog-report.glade:5-10`), default 1. An empty `options` is the same as none. The shape is
read with the other values; the children are then checked by the host (below). In the options summary a changed
report-list shows as GnuCash's `disp` shows its list of `(id wide high #f)` (`5.17:bindings/guile/gnc-optiondb.i:304-321`,
`report/html-utilities.scm:302-323`): `<GUID>, <colspan>, <rowspan>, Disabled` for each child, joined by `, `, the
GUID where GnuCash shows an instance id. The `Embedded Report` rows GnuCash lists first (`:339-343`) come with the
view's runs of its children.

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
| `o.budget(…, default)` | `gnc-register-budget-option`; `default` is a Budget or `()`, which reports take from `book.default_budget()` (v0.4) |
| `o.invoice(…, default)` (v0.6) | `gnc-register-invoice-option`; `default` is an Invoice or `()`, which the Printable Invoice passes (`standard/invoice.scm:391-393`) |
| `o.image(…, default)` (v0.6) | `gnc-register-pixmap-option` (`standard/invoice.scm:405-407`) |
| `o.complex_boolean(…, default, callback)`, `o.multichoice_callback(…, default, choices, callback)` | `gnc-register-complex-boolean-option`, `-multichoice-callback-option` (`5.17:bindings/guile/gnc-optiondb.i:1664-1710`): a boolean and a multichoice like any other, saved as one. The callback (a function or `()`) is GnuCash's dialog hook, which in every report only makes other options selectable; red-ale has no dialog and never calls it (v0.4) |
| `o.set_default_section(s)` | `GncOptionDBPtr-set-default-section` |
| `o.set_default(section, name, value)` | `GncOption-set-default-value` (`5.17:bindings/guile/gnc-optiondb.i:1500-1611`): the default and the value both become `value`, given in the form the option's declaration takes its default (`5.17:libgnucash/engine/gnc-option-impl.cpp:87-91`). A value the declaration would have replaced (a key that is not a choice, a number out of range) is refused. GnuCash ignores an option that does not exist; red-ale makes it a `report-script` error, so a misspelt name is found, and the translations leave out GnuCash's calls on options that do not exist (`notes/report-transaction-engine.md` §7, Q6) (v0.5) |
| `o.make_internal(section, name)` | `GncOptionDB::make_internal` (`5.17:libgnucash/engine/gnc-optiondb.cpp:266-272`): the option keeps its value for `render` but leaves `report.options` and the `options` param's schema, and a change to it is refused (`report-option`, reason `internal option`), so it never shows in the options summary. An option that does not exist is a `report-script` error, as for `o.set_default` (v0.5) |
| `o.default_report_currency()` | `gnc-default-report-currency`: the report settings' currency (section 4), since `options()` has no run context |
| `o.report_placement(section, name)` | `gnc-register-report-placement-option` (`5.17:libgnucash/engine/gnc-optiondb.cpp:1101-1112`): default `[]`, key `no_key` as GnuCash's. GnuCash's help is `nodoc_string`, since its dialog gives the option a page of its own; red-ale publishes a help text instead. `o.set_default` takes only `[]` for it (v0.5) |

`lib/options` translates `options-utilities.scm` on top of these (`add_report_date`, `add_date_interval`,
`add_interval_choice`, `add_account_levels`, `add_account_selection`, `add_currency`, `add_price_source`,
`add_plot_size`, `add_marker_choice`, `add_sort_method`, `add_subtotal_view`, `report/options-utilities.scm:44-234`),
with GnuCash's names, keys, help texts, choices and defaults unchanged.

Every template also gets GnuCash's three General options, unless its generator declared them
(`gnc:report-template-new-options`, `report/report-core.scm:298-335`): "Report name" (key `0a`, default the
template name), "Stylesheet" (`0b`, a multichoice over the style sheets, default `Default`) and "Add options
summary" (`0c`, `always` or `never`, default `never`). With `always`, the host appends a table of every option whose
value differs from its default, as `gnc:html-render-options-changed` does (`report/html-utilities.scm:298-345`). An
`invoice` value shows as its GUID: GnuCash's `disp` has no case for one and prints a pointer that differs at every
run (section 14).

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

**Children of a report placement** given by the caller (in `report.run`, `report.export`,
`report.saveConfiguration`, `report.updateConfiguration`) are checked after the values are read
(`Registry::check_children`): each `report` must be a template the registry has or a saved configuration in the
Configuration; `options` with a saved configuration's GUID are refused; a template child's `options` are applied to
that template's options as `report.run` applies them, its own children checked in turn. A failure is `report-option`
on the placement option, its `reason` starting `child <n>: ` (the position, from 0) and, for a child's option,
naming the template and that option. A template child's `options` are then kept as the values that differ from its
template's defaults, in wire form (accounts as GUIDs), as a saved configuration keeps its own. Children a saved
configuration stores are not checked again when it is run or updated: one whose saved configuration was deleted since
renders a warning in its cell when the view runs (`milestone5.md`, "Decided", item 3).

## 7. The report result

The report result is the published language for reports: part of `redAleApi` and versioned with it (`0.2.0`), its
JSON Schema listed in `api.schema` as `ReportResult`. Within a major version it changes only additively, like the
rest of the API. It carries data, not markup: exact figures with their display form, and roles, not styles.

`render()` returns it as a Rhai map of the same shape, without the fields the host fills in (marked *host*). The host
checks it against the schema and fills those fields; a mismatch is a `report-result` error naming the path.

**Class names** (v0.6). Every block but `optionsSummary`, and every cell, may have `class`: names for the report's
own CSS (`css`), as GnuCash's invoice styles its layout by the classes of its `div`s and cells
(`standard/invoice.scm:36-45`). They change no figure.

Beyond the schema, a result is valid when every cell holds exactly one content (`text`, `monetary`, `commodities`,
`number`, `price`, `date` or `anchors`), `places` and `suffix` appear only on number cells and `currency` on every price
cell and nowhere else, `until` only on date cells and not before the date, an `anchors` cell has at least one anchor
and no `link` or `account` of its own, a `docLink` names a document, `labelLines` has at least one line, each row
covers exactly the table's columns (a `null` cell covers one, a cell its `span`), a row's `level` is below the
`levels` of the table's amount columns, `levels` appears only on amount columns, a chart's series have one value per
label (scatter series have `points` instead), `colors` and `links` have one entry per value, a `currency` axis names
its `commodity`, a pie chart has one series, plot sizes are 1–100 % or at least one pixel, and no *host* field is set
by the script. `optionsSummary` blocks come only from the host. A `class` (v0.6) is one or more names separated by
single spaces, each a letter then letters, digits, `-` and `_`; an `image` block's `src` starts `file:///`; an
`exact` number cell has no `places` and no `rounding`.

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
| `export` | Optional (v0.3). Set only when `ctx.export_type()` is `"csv"`: the rows GnuCash hands `gnc:lists->csv` (`report/trep-engine.scm:463-512`), each a list of cells that are a string, an exact number or `null`. The host turns them into the CSV of `report.exportCsv`; `report.run` never carries them |
| `css` | Optional (v0.6). The report's own CSS, which a page appends after the style sheet's: GnuCash's `gnc:html-document-set-style-text!`, which the invoice fills from its Layout › CSS option (`standard/invoice.scm:795`). Text the CSS template refuses (section 8) is left out of the result by the host, with a `debug` log entry naming the reason: the host cannot tell which option fed it, and the figures do not depend on it (`notes/report-invoice.md` §6 Q3) |
| `exportError` | Optional (v0.5). Set only when `ctx.export_type()` is `"csv"`, instead of `export`: why there is nothing to export ("No splits found"), GnuCash's `gnc:html-document-set-export-error` (`report/trep-engine.scm:2524`, `:2605`, `:2641`). `report.exportCsv` answers it with `invalid`, `data.reason: "export-error"` and the message. A draft with both is a `report-result` error |
| `log` | *host*. The report log |

### Blocks

| `kind` | Fields | GnuCash |
|--------|--------|---------|
| `text` | `text`, `role`: `paragraph` (default), `heading`, `note`; `link` (v0.5, optional): what the text leads to | `gnc:make-html-text`, `gnc:html-markup-h3`, `gnc:html-markup-p`; the Multicolumn statements' "Barchart" link (`standard/balsheet-pnl.scm:1079-1083`) |
| `warning` | `title`, `text` | `gnc:html-make-generic-warning` and the no-account and empty-data warnings built on it (`report/html-utilities.scm:350-381`) |
| `table` | below | `<html-table>` |
| `chart` | below | `<html-chart>` |
| `list` (v0.5) | `items: [{ text, account? }]`, an item with `account` naming (and linking to) that account | `gnc:html-markup-ul` (`report/html-text.scm:210-215`): the Cash Flow's selected accounts (`standard/cash-flow.scm:253-262`) |
| `group` | `blocks`, laid out side by side | the one-row layout tables of the two-column Balance Sheet and Income Statement (`standard/balance-sheet.scm:503-513`, `standard/income-statement.scm:483-536`) |
| `grid` (v0.5) | `width` (a Rational, the view's "Number of columns", GnuCash's `table-width`) and `rows: [{ cells: [GridCell] }]`, below | the table of `standard/view-column.scm:51-159` |
| `image` (v0.6) | `src`: `file:///` and a file name, as GnuCash's `make-file-url` writes it (`report/report-utilities.scm:1028-1031`) | `gnc:html-markup-img`: the invoice's `picture` layout choice (`standard/invoice.scm:300-308`). The server never reads the file |
| `optionsSummary` | *host*. `rows: [{ section, name, value }]`; a Multicolumn View's summary starts with one row per child, `section` empty, `name` `Embedded Report` and `value` the child's "Report name" (its GUID when it did not run), as GnuCash lists them first (`report/html-utilities.scm:339-343`) | `gnc:html-render-options-changed` |

**Grid cells** (v0.5). A Multicolumn View's script lays out its children by GnuCash's rules (`notes/report-statements-charts.md`
§8.3) and names each by position; the host runs them (section 2, "Embedded reports"; `engine::children`):

| Field | Meaning |
|-------|---------|
| `rowspan`, `colspan` | 0–100, the child's GnuCash `high` and `wide` |
| `child` | The child's position in the view's report-list, from 0; one that names no child is `report-result` |
| `link` | *host*, optional. A report link that runs the child alone, GnuCash's "Single Report" (`standard/view-column.scm:115-130`): its template, its saved configuration's GUID for a saved configuration child, and a template child's options. Absent when the child names no loaded report. "Edit Options" has no counterpart: `report.options` does that job |
| `result` | *host*. The child's report result, as `report.run` returns it without `version` and `now`, which are the view's: `report`, `title`, `headline`, `options`, `sections`, `log` |
| `warning` | *host*, in place of `result`: `{ title: "Report error", text }` (`standard/view-column.scm:91-97`) when the child's saved configuration or template no longer exists, it fails (`report-script`, `report-option`, `report-result`; the text starts "An error occurred while running the report." as GnuCash's does), it would be deeper than 4 or past the 100th, or it is a saved configuration already being run further up |

Each child runs once, with the view's Book snapshot, `now`, zone, settings and Locale, and no export. A template
child gets its `options` over the template's defaults; a saved configuration child gets its stored values, less its
General/"Stylesheet", which the view's style sheet replaces. A finished cell holds `result` or `warning`, never both,
and a child's sections are validated as the top level's are.

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
default right). `labelLines` (v0.5, optional) is the heading as stacked lines, top first: GnuCash's multi-row column
headers (`gnc:html-table-set-multirow-col-headers!`), which the Transaction Breakdown fills with each account's name
split at its separator (`standard/txn-columns.scm:170-174`). `label` still holds the heading in one line for a client
that reads one. Roles: `label` (account names and row labels), `text`, `amount`, `date`. An `amount` column may
have `levels`, the number of staircase positions an account table uses: GnuCash puts a deeper account's amount
further left and totals furthest right (`report/html-acct-table.scm:870-923`, amount depth at `:1059-1062`). A
row's `level` says which position its amount takes, 0 being the rightmost. The renderer turns levels into
columns; a client may ignore them and read the figure.

**Rows** have a `role`, `cells` (one per column, `null` for empty) and, where they apply:

- `account`: the GUID of the account the row is about;
- `depth`: indentation of the label, counted in levels (`indented-depth` of `report/html-acct-table.scm:749-763`);
- `level`: the amount's staircase position;
- `alternate` (v0.5): for a `data` or `account` row, `true` to shade it as GnuCash's `alternate-row`, `false` as
  `normal-row`. Absent, data and account rows alternate by position. The Transaction Report shades by split, so a
  transaction's several lines share a shade (`report/trep-engine.scm:1967-1977`). On any other role it is a
  `report-result` error.

Row roles: `data` (default), `heading` (a section label such as "Assets"), `account`, `subtotal` ("Total Bank1"),
`total` (a section total: "Total Assets"), `grandTotal` ("Total Liabilities & Equity", "Net income"), `rule` (a
horizontal rule across the table, GnuCash's `gnc:html-table-append-ruler!`), `spacer` and `warning` (v0.5: a warning
inside the table, bold in the negative colour, Investment Lots' warning rows, `standard/investment-lots.scm:1077-1096`). The roles stand for
GnuCash's row and cell markup (`primary-subheading`, `total-label-cell`, `total-number-cell`), which style sheets
colour (section 8).

**Cells** are one of:

| Cell | Fields | Display (*host*) |
|------|--------|------------------|
| text | `text` | the text |
| monetary | `monetary: { amount: Rational, commodity }` | `rounded` (Amount at the commodity's SCU, banker's rounding) and `display`, section 3 |
| commodities | `commodities: [{ monetary, converted }]` | each line's `display` and `convertedDisplay` |
| number | `number: Rational`, `places` (default 2), `suffix`; `rounding` (v0.5): `bankers` (default) or `halfUp`; `numberFormat` (v0.5): `share`, or (v0.6) `exact` | `display`, rounded to `places` with banker's rounding, then the suffix (`"57.14 %"`). With `places` unset, padded to the Locale's `frac_digits` (2 under C), as GnuCash's number renderer. `rounding: "halfUp"` rounds ties away from zero, for the rates GnuCash prints from doubles with Guile's `~,nf` (`standard/advanced-portfolio.scm:914-922`; `3.625` is `3.63`). `numberFormat: "share"` is GnuCash's share print info (`gnc_share_print_info_places`, `5.17:libgnucash/app-utils/gnc-ui-util.cpp:1013-1023`): exactly `places` decimals, the magnitude rounded half up (`:1105-1145`, so ties go away from zero), grouped with the Locale's numeric separators, the point always printed (`1,234.568`, `100.`); it takes no `rounding`. `numberFormat: "exact"` (v0.6) is `xaccPrintAmount` with `gnc_default_print_info(FALSE)` (`gnc-ui-util.cpp:833-860`), which the invoice uses for quantities and percentage discounts (`report/html-style-info.scm:217-218`): at least the Locale's `frac_digits` decimals, no maximum and no rounding, grouped, a remainder that is not decimal as `w + n/d` (`1/8` is `0.125`, `4/3` is `1 + 1/3`, `1/3` is `1/3`, `:1237-1242`); it takes no `places` and no `rounding`, and `suffix` still applies (`2.50 %`) |
| price | `price: Rational`, `currency` | `display` with the currency's price print info |
| date | `date: Time64` (a Timestamp on the wire); `until` (v0.5, optional): the end of the period it starts | `localDate` (`YYYY-MM-DD` in the run's zone) and `display` under `reports.dateFormat`; with `until`, also `localUntil`, and `display` is `<date> to <until>`, each as `qof-print-date` prints it, as the Multicolumn P&L's column headers (`standard/balsheet-pnl.scm:1140-1153`) |
| anchors (v0.5) | `anchors: [{ text, link }]` | none: the texts, one per line. The Transaction Report's invoice cell (`report/trep-engine.scm:1036-1049`) |

A `commodities` cell is GnuCash's nested commodity table for an account holding foreign commodities: one line per
commodity, the native amount and its value in the report currency (`report/html-acct-table.scm:925-948`).

`until` is a Rhai keyword, so a script quotes the key: `#{ date: from, "until": to }`.

Any cell but an `anchors` cell may also have `account` (a GUID: the cell names or links to that account) and `link`
(below). Any cell may have `rule: true` (an accounting rule above the figure, the "Show accounting-style rules" option,
`report/html-acct-table.scm:901-906`), `span` (columns it covers, default 1) and `footnote` (v0.4).

A table may have no columns (v0.5): its rows then have no cells. The Transaction Report makes one when the Amount is
`Hide` and every other column is off (`standard/test/test-transaction.scm:495-501`); the renderer writes an empty
table.

**Footnotes** (v0.4). A table may have `footnotes`, notes numbered from 1, and a cell's `footnote` is the number of
the one it refers to. The renderer writes the number as a superscript after the cell's content, titled with the note,
and lists the notes after the table: GnuCash's footnote collector, which the Budget Report uses for budget notes
(`standard/budget.scm:240-259`). A number outside the table's notes is a `report-result` error.

### Links

A link points at something a client can open, as GnuCash's anchors (`report/html-utilities.scm:82-224`):

| Link | Leads to | GnuCash |
|------|----------|---------|
| `{ "account": "<guid>" }` | the account's register | `gnc:account-anchor-text` (`:85-88`) |
| `{ "report": { "template": "<guid>", "configuration"?: "<guid>", "options": { … } } }` | another report run (drill-down: the charts link a bar or slice to the same report on that account's children); with `configuration` (v0.5), that saved configuration of the template, `options` then changes over its values (a Multicolumn View's "Single Report") | `gnc:make-report-anchor` (`:168-172`); `standard/category-barchart.scm:589-609`, `standard/account-piecharts.scm:474-496`; the Cash Flow Barchart's values to the Cash Flow (`standard/cashflow-barchart.scm:207-217`) |
| `{ "split": "<guid>" }` (v0.5) | the split in its account's register | `gnc:split-anchor-text` (`:90-91`), `gnc:html-split-anchor` (`:195-200`) |
| `{ "transaction": "<guid>" }` (v0.5) | the transaction | `gnc:transaction-anchor-text` (`:93-94`) |
| `{ "invoice": "<guid>" }` (v0.5) | an invoice, bill or voucher | `gnc:invoice-anchor-text` (`:127-128`) |
| `{ "price": "<guid>" }` (v0.5) | a price in the price database | `gnc:price-anchor-text` (`:107-110`), `gnc:html-price-anchor` (`:217-224`) |
| `{ "docLink": "<uri>" }` (v0.5) | a transaction's or invoice's linked document | `gnc:transaction-doclink-anchor-text`, `gnc:invoice-doclink-anchor-text` (`:96-100`) |

GnuCash creates a report instance at render time for a report link; red-ale only describes it, and a client runs it
with `report.run`.
GnuCash's document link names the transaction or invoice and GnuCash opens the document it holds; red-ale's carries
the URI, and the HTML shows only the text (section 8). `lib/html-utilities` builds the cells and anchors as GnuCash's
helpers do: `html_split_anchor`, `html_transaction_anchor`, `html_doclink_anchor`, `html_price_anchor`,
`invoice_anchor` and `html_markup_ul`.

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
`pointRadius` (Rational, v0.4) is GnuCash's key of that name (`standard/balance-forecast.scm:153-154`): 0 draws a
line's markers transparent, so they keep their hover labels; any other radius draws them as usual.

These cover every chart in scope:

| Report | Chart |
|--------|-------|
| Net Worth / Income-Expense Barchart | `bar`, two or three series side by side (`standard/net-charts.scm:316-384`) |
| Net Worth / Income & Expense Linechart | `line`, the same series, `lineWidth` from the option |
| Category bar charts | `bar` or `line` by option, `stacked` by option, one series per account plus "Other" (`standard/category-barchart.scm:528-626`) |
| Account pie charts | `pie`: one series, a value and colour per slice (`standard/account-piecharts.scm:472-558`) |
| Price Scatterplot | `scatter` with a `linear` x axis in steps since the start date (`standard/price-scatter.scm:166-274`) |
| Average Balance | `bar`: Average, Profit, Gain and Loss series (`example/average-balance.scm:386-451`) |
| Budget Chart (v0.4) | `bar` or `line` by option, one chart per account: Actual and Budget series (`standard/budget-barchart.scm:155-222`) |
| Balance Forecast (v0.4) | `line`: Minimum, Balance, Target and Reserve, unfilled, `lineWidth` 1.5, `pointRadius` 3 or 0 (`standard/balance-forecast.scm:234-293`) |

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
  **The one exception** (v0.6) is an `image` block, written only when a script puts one in the result: an
  `<img src="file:///…">`, the src HTML-escaped and otherwise as given (GnuCash quotes it as a Scheme string,
  `report/html-text.scm:227-231`). It may name a local file, but the `file:///` prefix means the page never fetches
  anything from the network, and red-ale never reads the file ([`milestone6.md`](milestone6.md), "Decided", item 4).
  An empty Picture Location gives `file:///`, which a browser shows as a broken image, as in GnuCash.
- **The report's CSS** (v0.6), a result's `css`, goes in the same `<style>` element after the style sheet's CSS, on
  a line of its own, trailing whitespace trimmed, as GnuCash's style sheets append a document's style text
  (`report/html-fonts.scm:162-192`). It passes the CSS template's rule (below) or is not written: a host never puts
  refused CSS in a result, but a client may hand the renderer one from elsewhere.
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
  template therefore styles red-ale's page too. Rows also carry `role-<role>`. Data and account rows take
  `normal-row` and `alternate-row` by position, or as their `alternate` says (v0.5). A `warning` row's text cells
  are `total-label-cell neg`, as Investment Lots marks them (`standard/investment-lots.scm:1090-1095`), and the row
  has no row style.
- Line breaks in text become `<br>`.
- **Tables.** An `amount` column with `levels` becomes that many cell columns; a row's amount goes in the column of
  its `level`, counted from the right, as GnuCash lays out its account tables. A `commodities` cell becomes a small
  nested table, one line per commodity. Cells with `account` carry `data-account="<guid>"`; a `link` becomes
  `data-link` with its JSON, on a cell, a text block's element or each anchor of an `anchors` cell. There are no
  `href`s: GnuCash's `gnc-register:` and report URLs mean nothing outside GnuCash, and a client that embeds the page
  can wire the data attributes itself. A `docLink` becomes nothing: its text shows unlinked, so an exported page
  carries no path to a user's files. An `anchors` cell writes its texts one per line. Columns with `labelLines`
  make the header as many rows as the most lines, a column without them showing its label in the first row. A
  table with no columns has no header and empty rows (v0.5).
- A `list` block is a `<ul>`; an item naming an account carries `data-account` and shows as a link.
- **Class names** (v0.6). A block with a `class` is wrapped in `<div class="…">` whose only child is the block's own
  element, so the invoice's `.entries-table > table` matches; a table's classes join its `table-wrap` wrapper's
  (`<div class="table-wrap entries-table">`). A cell's classes follow its own in its `<td>`'s class list.
- **Negative figures** get the class `neg`, coloured red when the style sheet's "Negative Values in Red" is on
  (`report/html-fonts.scm:116-119`).
- A `group` block is a CSS grid of its blocks side by side, stacking on narrow screens.
- A `grid` block (v0.5) is GnuCash's view table (`standard/view-column.scm:99-155`): a `<table class="grid">` 100 %
  wide, a `<tr>` per row and a `<td>` per cell with its `rowspan` and `colspan` (a `colspan` of 0 written as 1, as
  browsers read it, Q10) and `vertical-align: top`, both as inline styles so the base stylesheet, and every other
  page's bytes, stay as they were. A cell holds the child's headline or title as an `<h3>`, as GnuCash's style sheets
  head an embedded report (`report/stylesheets/plain.scm:199-205`), its sections, and its `link` as a "Single Report"
  note; or its warning as a warning block. The whole page goes through the view's style sheet, never a child's
  (`standard/view-column.scm:84-85`). IDs inside a child continue the cell's position: `chart-0-0-1-0-0-2` is the
  third block of the first section of the child in section 0, block 0, row 1, cell 0.
- An options summary row with an empty `section` (a view's `Embedded Report` rows) shows its `name` alone.

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
- **A Multicolumn View's configuration** stores its children as its changed `__general`/`report-list` value (§6,
  report placements), each child's GUID, spans and, for a template child, its options as they differ from that
  template's defaults: `{ "__general": { "report-list": [{ "report": "…", "colspan": 1, "rowspan": 1 }] } }`. A
  saved configuration it names is kept by GUID, so deleting or renaming that configuration does not touch the view's
  entry (v0.5).
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
- **GnuCash's files are imported as data, never read at start-up.** They are Scheme programs, and red-ale never
  evaluates Scheme. `report.importConfigurations` and `report.importStyleSheets` (v0.3) take their text and read only
  the literal forms GnuCash writes into them (section 15).
- The Book's one reference to a saved configuration, the default invoice report
  (`options/Business/Default Invoice Report`, `5.17:libgnucash/engine/qofbook.cpp:1034-1112`), names the template
  that prints an invoice: `report.run` with that template and the invoice ([`milestone6.md`](milestone6.md)). From
  v0.6 `book.options` reads it as `defaultInvoiceReport` and `book.setOptions` writes it, as GnuCash parses and writes
  the slot ([`api/book-options.md`](api/book-options.md)); no report run reads or writes it. A client printing an
  invoice runs the report it names, and the Printable invoice when it names no invoice report red-ale has, as GnuCash
  falls back (`5.17:gnucash/gnome/dialog-invoice.c:957-963`).

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
| | Future Scheduled Transactions Summary (v0.4) | `47f45d7d6d57b68518481c1fc8d4e4ba` | (top level) | `standard/account-summary.scm:430-435` |
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
| `standard/budget.rhai` (v0.4) | Budget Report | `810ed4b25ef0486ea43bbd3dddb32b11` | Budget | `standard/budget.scm:763-769` |
| `standard/budget-balance-sheet.rhai` (v0.4) | Budget Balance Sheet | `ecc35ea9dbfa4e20ba389fc85d59cb69` | Budget | `standard/budget-balance-sheet.scm:785-791` |
| `standard/budget-income-statement.rhai` (v0.4) | Budget Income Statement | `583c313fcc484efc974c4c844404f454` | Budget | `standard/budget-income-statement.scm:591-598` |
| | Budget Profit & Loss | `e5fa5ce805e840ecbeca4dba3fa4ead9` | Budget | `standard/budget-income-statement.scm:602-609` |
| `standard/budget-flow.rhai` (v0.4) | Budget Flow | `e6e34fa3b6e748debde3cb3bc76d3e53` | Budget | `standard/budget-flow.scm:315-321` |
| `standard/budget-barchart.rhai` (v0.4) | Budget Chart | `415cd38d39054d9e9c4040455290c2b1` | Budget | `standard/budget-barchart.scm:322-328` |
| `standard/balance-forecast.rhai` (v0.4) | Balance Forecast | `321d940d487d4ccbb4bd0467ffbadbf2` | Assets & Liabilities | `standard/balance-forecast.scm:300-306` |
| `standard/equity-statement.rhai` (v0.5) | Equity Statement | `c2a996c8970f43448654ca84f17dda24` | Income & Expense | `standard/equity-statement.scm:501-507` |
| `standard/trial-balance.rhai` (v0.5) | Trial Balance | `216cd0cf6931453ebcce85415aba7082` | Income & Expense | `standard/trial-balance.scm:868-874` |
| `standard/cash-flow.rhai` (v0.5) | Cash Flow | `f8748b813fab4220ba26e743aedf38da` | Income & Expense | `standard/cash-flow.scm:340-346` |
| `standard/cashflow-barchart.rhai` (v0.5) | Cash Flow Barchart | `5426e4d987f6444387fe70880e5b28a0` | Income & Expense | `standard/cashflow-barchart.scm:339-350` |
| `standard/balsheet-pnl.rhai` (v0.5) | Balance Sheet (Multicolumn) | `065d5d5a77ba11e8b31e83ada73c5eea` | Assets & Liabilities | `standard/balsheet-pnl.scm:1242`, `:1245-1251` |
| | Income Statement (Multicolumn) | `0e94fd0277ba11e8825d43e27232c9d4` | Income & Expense | `standard/balsheet-pnl.scm:1243`, `:1253-1259` |
| `standard/transaction.rhai` (v0.5) | Transaction Report | `2fe3b9833af044abb929a88d5a59620f` | (top level) | `standard/transaction.scm:31-36` |
| `standard/portfolio.rhai` (v0.5) | Investment Portfolio | `4a6b82e8678c4f3d9e85d9f09634ca89` | Assets & Liabilities | `standard/portfolio.scm:286-292` |
| `standard/general-ledger.rhai` (v0.5) | General Ledger | `2e22929e5c5b4b769f615a815ef0c20f` | Assets & Liabilities | `standard/general-ledger.scm:149-156` |
| `standard/reconcile-report.rhai` (v0.5) | Reconciliation Report | `e45218c6d76f11e7b5ef0800277ef320` | (top level) | `standard/reconcile-report.scm:106-114` |
| `standard/general-journal.rhai` (v0.5) | General Journal | `25455562bd234dd0b048ecc5a8af9e43` | Assets & Liabilities | `standard/general-journal.scm:116-123` |
| `standard/advanced-portfolio.rhai` (v0.5) | Advanced Portfolio | `21d7cfc59fc74f22887596ebde7e462d` | Assets & Liabilities | `standard/advanced-portfolio.scm:1184-1190` |
| `standard/ifrs-cost-basis.rhai` (v0.5) | IFRS weighted-average cost basis report | `15d5b744176c4625a703720338725291` | Experimental | `standard/ifrs-cost-basis.scm:52`, `:527-533` |
| `standard/investment-lots.rhai` (v0.5) | Investment Lots | `ab2acc24afd14630a551f98f1a35fa81` | Assets & Liabilities | `standard/investment-lots.scm:2320-2326` |
| `standard/txn-columns.rhai` (v0.5) | Transaction Breakdown Report | `03603f0443494a6fa790b417f35d90d4` | Experimental | `standard/txn-columns.scm:33`, `:254-260` |
| `example/daily-reports.rhai` (v0.5) | Income vs. Day of Week | `5e2d129f28d14df881c3e47e3053f604` | Examples | `example/daily-reports.scm:49`, `:291-308` |
| | Expenses vs. Day of Week | `dde49fed4ca940959ae7d01b72742530` | Examples | `example/daily-reports.scm:50`, `:309-310` |
| `standard/view-column.rhai` (v0.5) | Multicolumn View | `d8ba4a2e89e8479ca9f6eccdeb164588` | Multicolumn (menu entry "Custom Multicolumn Report") | `standard/view-column.scm:180-189`; `report/report-core.scm:163` |
| `standard/invoice.rhai` (v0.6) | Printable Invoice | `5123a759ceb9483abf2182d01c140e8d` | Business | `standard/invoice.scm:873`, `:877-885` |

Menu paths are GnuCash's (`report/report-core.scm:157-161`) without the mnemonic underscore in this table; the
scripts keep the strings as written (`"_Assets & Liabilities"`). The Future Scheduled Transactions Summary is the
Account Summary's script with `params.sx`, as GnuCash's two templates share one options generator and renderer
(`standard/account-summary.scm:138`, `:241`); it reads `book.sx_cashflow` (section 5). The two Multicolumn templates
are one script with `params.report_type` (`balsheet` or `pnl`), as GnuCash's share one options generator and renderer
(`standard/balsheet-pnl.scm:1250-1251`, `:1258-1259`); `milestone5.md`'s "Multicolumn Balance Sheet and P&L" is the
feature's name, not a template's. The Printable Invoice is the only invoice template: GnuCash's Easy and Fancy
Invoices, its other two variants (`standard/invoice.scm:887-905`), are ❌ ([`milestone6.md`](milestone6.md),
"Decided", item 2). The two day-of-week templates are one script with `params.account_types` and
`params.reportname`, as GnuCash builds both from one list (`example/daily-reports.scm:291-310`); `features.md`'s
"Income/expense vs day of week" is the feature's name.

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
`5.17:bindings/guile/date-utilities.scm`, TE `report/trep-engine.scm`.

| Report file | Templates | Helpers it uses | Book data and features |
|-------------|-----------|-----------------|------------------------|
| `standard/account-summary.scm` | Account Summary, Future Scheduled Transactions Summary (v0.4) | OU: report-date (date-interval for the summary), account-levels, currency, price-source, subtotal-view. RU: filter-accountlist-type, get-current-account-tree-depth, the commodity collector. CU: case-exchange-fn, case-price-fn. HAT: make-html-acct-table/env/accts, num-rows, get-row-env, the data half of add-account-balances. HU: make-rates-table, no-account warning. DU: time64-end-day-time. `gnc-accounts-and-all-descendants` | Account code, type, description, notes; prices for foreign commodities; company name; the scheduled transactions' cash flow (the summary) |
| `standard/balance-sheet.scm` | Balance Sheet | As Account Summary, plus RU: decompose-accountlist, collector+ and collector-, monetaries-add, accounts-get-comm-total-assets, account-get-comm-value-at-date; CU: sum-collector-commodity, uniform-commodity? | "Use Trading Accounts" (`standard/balance-sheet.scm:266`): a trading line instead of unrealized gains; TRADING accounts; `xaccAccountGetBalanceAsOfDate` |
| `standard/income-statement.scm` | Income Statement, Profit & Loss | OU: date-interval, account-levels, currency, price-source, subtotal-view. RU: decompose-accountlist, accountlist-get-comm-balance-interval-with-closing, account-get-trans-type-balance-interval-with-closing, account-get-trans-type-splits-interval, collectors. CU, HAT and HU as Balance Sheet | Closing transactions (`book_closing` slot) and the closing-entries description pattern; TRADING accounts if selected; voided splits excluded |
| `example/average-balance.scm` | Average Balance | OU: date-interval, interval-choice, currency, price-source, plot-size. RU: accounts-get-commodities, account-get-balances-at-dates, monetaries-add, collectors. CU: case-exchange-time-fn (and everything under it), sum-collector-commodity. DU: make-date-list, deltasym-to-delta, decdate, time64-start/end-day-time. HU: empty-data and no-account warnings | A split query (accounts, dates, non-void); same-day split order follows "Number Field Source" (`5.17:libgnucash/engine/Split.cpp:1548-1557`) |
| `standard/net-charts.scm` | the four net charts | OU: date-interval, interval-choice, currency, price-source, plot-size. RU: account-accumulate-at-dates (no-closing balance), decompose-accountlist, filter-accountlist-type, account-is-inc-exp?, collector-, monetaries-add, accounts-get-commodities, not-all-zeros. CU: case-exchange-time-fn. DU: make-date-list, deltasym-to-delta, day boundaries. HU: warnings, `assign-colors` not used (fixed colours) | Closing transactions excluded through `xaccSplitGetNoclosingBalance` |
| `standard/category-barchart.scm` | the four "Over Time" charts | As net-charts, plus OU: account-levels, sort-method; RU: get-current-account-tree-depth; CU: sum-collector-commodity; DU: date-get-fraction-func and the fraction functions; HU: assign-colors | |
| `standard/account-piecharts.scm` | the five pie charts | OU: report-date or date-interval, currency, price-source, account-levels, plot-size, sort-method. RU: account-get-comm-balance-interval, account-get-comm-balance-at-date, filter-accountlist-type, get-current-account-tree-depth, monetary->string. CU: case-exchange-fn, sum-collector-commodity. DU: fraction functions. HU: assign-colors, warnings | Securities: account commodities and prices |
| `standard/price-scatter.scm` | Price Scatterplot | OU: date-interval, interval-choice, currency, plot-size. RU: account-has-shares?, monetary->string. CU: get-commodity-inst-prices, get-commodity-totalavg-prices, and the euro conversion | STOCK, MUTUAL and CURRENCY accounts with cross-commodity splits; the price database (forward prices only) |
| `standard/balance-forecast.scm` (v0.4) | Balance Forecast | OU: date-interval, interval-choice, currency, price-source, plot-size. RU: filter-accountlist-type, accounts-get-commodities, account-get-balances-at-dates, the commodity collector. CU: case-exchange-time-fn, sum-collector-commodity. DU: make-date-interval-list, deltasym-to-delta, time64-start/end-day-time. HU: no-account and empty-data warnings | `book.sx_cashflow`; each account's first split (`gnc-account-get-earliest-date`, read as the first of `a.splits()`); prices for foreign commodities |
| `standard/equity-statement.scm` (v0.5) | Equity Statement | OU: date-interval, currency, price-source. RU: decompose-accountlist, accounts-get-comm-total-assets, account-get-comm-balance-at-date, accountlist-get-comm-balance-at-date-with-closing, account-get-trans-type-balance-interval-with-closing, accountlist-get-comm-balance-interval-with-closing, collector+ and collector-, monetaries-add, commodity-collector-allzero?. CU: case-exchange-fn (`average-cost` too, for the unrealized gains), case-price-fn, sum-collector-commodity. HAT: add-labeled-amount-line. HU: make-rates-table, no-account warning. DU: time64-previous-day, time64-end-day-time. Its own `account-get-total-flow` (`standard/equity-statement.scm:164-180`) | Closing transactions and the closing-entries pattern; every split of each equity account, void ones included (`xaccAccountGetSplits`), under its transaction's currency; prices for foreign commodities; company name |
| `standard/trial-balance.scm` (v0.5) | Trial Balance | OU: date-interval, account-levels, currency, price-source. RU: decompose-accountlist, account-is-inc-exp?, account-get-trans-type-splits-interval, account-get-comm-balance-at-date, collector+ and collector-, commodity-collector-get-negated, commodity-collector-allzero?, monetaries-add. CU: case-exchange-fn (`average-cost` too, for the unrealized gains), case-price-fn, sum-collector-commodity, uniform-commodity?. HAT: make-html-acct-table/env/accts (flattened, every account shown, no subtotals), gnc-commodity-table. HU: make-rates-table, no-account warning. DU: time64-previous-day, time64-end-day-time. Its own `double-col`, `sum-account-splits` and the work sheet's cells (`standard/trial-balance.scm:138-167`, `:598-660`, `:723-778`), the account table's `get-cell`/`set-cell!` (HAT:830-841) being a per-row array | The adjusting- and closing-entries patterns and closing transactions; the merchandising options' gross adjustment and income summary accounts; prices for foreign commodities; company name. TRADING accounts get no row, though the rates table lists their commodities |
| `standard/cash-flow.scm` (v0.5) | Cash Flow | OU: date-interval, currency, price-source, account-selection. RU: account-full-name<?, accounts-get-children-depth, accounts-get-commodities, account-get-trans-type-splits-interval (through the calculation), collector-. CU: case-exchange-fn, case-exchange-time-fn, case-price-fn, sum-collector-commodity. HU: make-rates-table, no-account warning, `gnc:html-markup-ul` (a `list` block). DU: time64-start/end-day-time. `cash-flow-calc-money-in-out` (`lib/cash-flow-calc`) | Every split of each transaction that touches a selected account in the period, void ones not found, closing ones counted; prices at each split's posted date |
| `standard/balsheet-pnl.scm` (v0.5) | Balance Sheet (Multicolumn), Income Statement (Multicolumn) | OU: date-interval, account-levels, currency, price-source. RU: filter-accountlist-type, decompose-accountlist, accounts-get-commodities(-sorted), account-accumulate-at-dates (as its own loop over each account's splits, recording what each date needs), collector+ and collector-, monetaries-add, and a `monetary+` of its own (`standard/balsheet-pnl.scm:371-384`). CU: case-exchange-time-fn (amounts), case-exchange-fn (unrealized gains and retained earnings), case-price-fn (rates). DU: make-date-list, decdate, deltasym-to-delta, time64-start/end-day-time. HU: make-report-anchor (`make_report_anchor`), no-account warning. Its own walker `add-multicolumn-acct-table` (`:279-607`) and `book-main-currency` (`:804-812`) | Each split's balance, no-closing balance and value under its transaction's currency: closing transactions are left out of the P&L and kept by the balance sheet; the last split at each date (split links); "Use Trading Accounts" (trading accounts in place of unrealized gains); the latest price of each commodity (`has-price?`, `:662-676`) and prices at each column's date; placeholders |
| `standard/cashflow-barchart.scm` (v0.5) | Cash Flow Barchart | OU: date-interval, interval-choice, currency, price-source, plot-size. RU: accounts-get-commodities, not-all-zeros, monetary+. CU: case-exchange-fn, case-exchange-time-fn, sum-collector-commodity. DU: make-date-interval-list, deltasym-to-delta, time64-start/end-day-time. HU: make-report-anchor (`make_report_anchor`), no-account and empty-data warnings. `cash-flow-calc-money-in-out` per interval | As the Cash Flow, per interval |
| `standard/portfolio.scm` (v0.5) | Investment Portfolio | OU: report-date, currency, price-source. RU: filter-accountlist-type, account-get-comm-balance-at-date, the commodity collector. CU: case-exchange-fn. HU: html-account-anchor (an account cell), html-price-anchor (a monetary cell linked to the price), no-account warning. DU: time64-end-day-time, `time64CanonicalDayTime` | Each account's own balance strictly before the end of the Date; prices in any currency nearest to, at or before local noon of the Date, or at or before now (`book.prices_any_currency`), shown as stored; the average-cost and weighted-average exchange functions, their 10,000-unit price rounded to five significant figures (`round_sigfigs`); share counts in the share display |
| `standard/advanced-portfolio.scm` (v0.5) | Advanced Portfolio | OU: report-date, currency. RU: filter-accountlist-type, account-get-comm-balance-at-date, the commodity collector. CU: case-exchange-fn, get-match-commodity-splits-sorted, sum-collector-commodity. HU: html-account-anchor, html-price-anchor (a price cell linked to the price), html-transaction-anchor (a monetary cell linked to the pricing transaction), no-account warning. DU: time64-end-day-time, `time64CanonicalDayTime`. Its own basis list, walk and price choice (`find-price`, `my-exchange-fn`), all in the script ([`notes/report-investments.md`](notes/report-investments.md) §3, §7) | Each account's balance strictly before the end of the Date; every split of each transaction touching it up to the Date, in split order, with the other split (`otherSplit`) and the account types and parents; the parent account's splits; prices in any currency nearest to local noon of the Date or at or before now, inverted where quoted the other way; the share price of the newest split with two commodities; units and prices to 10⁻⁸ and money to a currency's SCU with banker's rounding (`round_to`) where GnuCash's `gnc_numeric` calls round; share counts in the share display; rates as half-up number cells |
| `report/trep-engine.scm` (v0.5) | none: the library module `lib/trep-engine`, which the Transaction Report, General Ledger and Reconciliation Report are built on (`notes/report-transaction-engine.md`) | OU: date-interval, currency, price-source. DU: time64-start/end-day-time, date-get-week, date-get-week-year-string, the month, quarter and year strings. HU: no-account and generic warnings, split, invoice and document-link anchors. RU: the commodity collectors, accounts-get-commodities. CU: case-exchange-time-fn, exchange-by-pricedb-nearest. Guile's `make-regexp` (section 2) | The split query with reconcile states, closing match, the QOF sort keys and one split per transaction; the reads of section 5 marked v0.5 (other split, corresponding account name and code, void former amount, lot and invoice, document link, debit and credit labels, the num field source); reconcile and entered dates; prices for foreign commodities |
| `standard/transaction.scm` (v0.5) | Transaction Report | the engine as it is: `lib/trep-engine` | as `report/trep-engine.scm`; no export type, as in GnuCash (TR:31-36) |
| `standard/general-ledger.scm` (v0.5) | General Ledger | the engine through the Transaction Report's renderer (`import "standard/transaction"`, as GL:142-145 looks it up by GUID), with other defaults and the title "General Ledger" on every document (GL:146) | as the Transaction Report. GnuCash's defaults for "Running Balance" and "Totals" name options the engine does not have and do nothing (`notes/report-transaction-engine.md`, fact 2); the translation leaves them out (§14), so the report shows no balance column and a Grand Total |
| `standard/general-journal.scm` (v0.5) | General Journal | the Register report's options and renderer in journal mode (`lib/register`), with GJ's defaults; the query GJ stores in the Register's query option (GJ:57-71) is built by the script | Every split not in the void state, by date posted and split order, the first of each transaction; each transaction's notes and every one of its splits; the num field source; lot titles; split balances |
| `standard/ifrs-cost-basis.scm` (v0.5) | IFRS weighted-average cost basis report | OU: date-interval, currency. HU: html-split-anchor (the units linked to their split), generic options warning. DU: time64-start-day-time, `time64CanonicalDayTime`. Its own none-aware arithmetic (`M+`, `M-`, `M*`, `M/`, `M-abs`, `:122-149`), sign tests and transaction types (`:164-234`) | Every transaction of the Stock Account, void ones too, in split order by its first split there (`book.split_query` with `unique`); each split's account, amount, value and `actionNum`; the Stock Account's `currency_or_parent`; the price nearest local noon of each posted date, converting 10,000,000 units (`book.price_convert`). No floating point: every figure is exact, the rate rounded to the report currency's SCU by the conversion |
| `standard/investment-lots.scm` (v0.5) | Investment Lots | OU: date-interval, currency, price-source, plot-size. RU: account-get-comm-balance-at-date (`lib/balances`). CU: case-exchange-fn. HU: html-split-anchor (date cells linked to their split), assign-colors. DU: date-year-delta, time64-end-day-time, `time64CanonicalDayTime`. Its own none-aware arithmetic (`gnc-numeric` operations on `#f`), split classifier, lot-statistics collector and warnings (`:499-2069`) | Each selected account's splits up to the End date, void transactions left out, with their lot, amount, value, action and stock-split mark (`isStockSplit`); each lot's title and splits, sorted by date, one day's purchases by `xaccTransOrder` (`t.order`) and its other splits in the reverse of the lot's list order; the account's balance at the End date; one price of its commodity in any currency by the price source, inverted when quoted the other way. Every figure is exact but the CAGR (below); bases, gains and end values are rounded to the report currency's SCU where GnuCash rounds them |
| `example/daily-reports.scm` (v0.5) | Income vs. Day of Week, Expenses vs. Day of Week | OU: date-interval, currency, price-source (default `weighted-average`), plot-size. RU: filter-accountlist-type, accounts-get-commodities. CU: case-exchange-time-fn. HU: assign-colors, empty-data warning (also for no accounts). DU: time64-start/end-day-time. `gnc-accounts-and-all-descendants` | The selected accounts' non-void splits in the period, with their descendants' unless "Include Sub-Accounts" is off; each amount converted at its posted date and added to its local weekday's exact total, no sign reversed (income totals are negative, as GnuCash shows them); prices for the descendants' commodities |
| `standard/txn-columns.scm` (v0.5) | Transaction Breakdown Report | OU: date-interval. HU: html-split-anchor, empty-data and generic warnings. DU: time64-start/end-day-time. The engine's `xaccAccountTypeGetFundamental` (`account_type_fundamental` in `lib/accounts`) and `xaccAccountOrder` (`a.compare_order`, §5). Its own `acct-get-fullname-list` (`standard/txn-columns.scm:59-66`) | One account's non-void splits in the period, the first of each transaction (`book.split_query` with `unique`), filtered by description with `contains`; every split of those transactions, value and amount, signed by `a.reverse_balance`; the accounts' `compare_order` across the tree, types, depths and names; the transaction's num field whatever the num source |
| `standard/invoice.scm` (v0.6) | Printable Invoice (the Easy and Fancy Invoices are ❌) | `lib/business`: BC's owner helpers and `gnc:company-info`. HU: the generic warning. Its own layout renderers (client, company, details, today, picture), `make-entry-table` and `reg-renderer`, with `variant-list`'s `'invoice` defaults written directly | One document (section 5, "Invoice, Entry, Owner, Order, BillTerm"): its owner and the job's, entries and their orders, totals and tax list, terms, posted lot's splits and their transactions' order; the Book's Business and Tax options and its Fancy Date Format (`book.option`) |
| `standard/view-column.scm` (v0.5) | Multicolumn View | None of the helpers: its two options (`o.report_placement`, section 6) and the grid of `render-view` (`standard/view-column.scm:51-159`), with `column-allocs` an array by row. Running the children is the host's (section 2, "Embedded reports") | Nothing of the Book: the children read it |
| `standard/reconcile-report.scm` (v0.5) | Reconciliation Report | the engine with RR's calculated cells, "Funds In" and "Funds Out" (closures passed as `custom_calculated_cells`), and its instructions as `empty_report_message` | as the Transaction Report; the void former amount; amounts never converted. Its "Running Balance" default and `make-internal` are left out as GL's are |

No report needs a Book feature (`features` slot); all only read. The last column lists the Book data their figures
depend on.

### Shared Rhai modules

Each translates the named Scheme file's functions that the reports above use, directly or through each other, and
keeps its source's header and licence (GPL-2.0-or-later, checked at 5.17). Functions keep GnuCash's names with the
`gnc:` prefix dropped and `-` turned into `_`.

| Module | From | Functions |
|--------|------|-----------|
| `lib/collectors` | RU:289-465, :1018; CU:900-919 | `make_value_collector`, `make_commodity_collector` and its methods (`add`, `merge`, `minusmerge`, `format`, `getpair`, `getmonetary`, `getmonetary_strict`, `reset`, `remove_zeros`), `commodity_collector_get_negated`, `commodity_collector_allzero`, `collector_plus`, `collector_minus`, `monetaries_add`, `monetary_plus`, `not_all_zeros`, `sum_collector_commodity`, `uniform_commodity` |
| `lib/accounts` | RU:149-257; `5.17:libgnucash/engine/Account.cpp:4483-4513` | `account_has_shares`, `account_is_stock`, `account_full_name_lt`, `accounts_get_children_depth`, `account_is_inc_exp`, `filter_accountlist_type`, `decompose_accountlist` (a map by kind), `account_get_type_string_plural`, `accounts_get_commodities`, `accounts_get_commodities_sorted`, `get_current_account_tree_depth`, `accounts_and_all_descendants`; `account_type_fundamental` (v0.5), the engine's `xaccAccountTypeGetFundamental`, a fixed table the Transaction Breakdown sorts by |
| `lib/balances` | RU:478-725, :863-881 | `account_get_balances_at_dates`, `account_accumulate_at_dates`, `account_get_comm_balance_at_date`, `account_get_comm_value_interval`, `account_get_comm_value_at_date`, `accounts_get_balance_helper`, `accounts_get_comm_total_assets`, `account_get_balance_interval`, `account_get_comm_balance_interval`, `accountlist_get_comm_balance_interval(_with_closing)`, `accountlist_get_comm_balance_at_date(_with_closing)`, `split_voided`, `accounts_count_splits`, `account_get_trans_type_balance_interval(_with_closing)`, `account_get_trans_type_splits_interval`, `get_assoc_account_balances`, `select_assoc_account_balance`, `get_assoc_account_balances_total` |
| `lib/exchange` | CU:64-887 | `get_match_commodity_splits(_sorted)`, `get_all_commodity_splits`, `exchange_by_euro(_numeric)`, `exchange_if_same`, `get_commodity_totalavg_prices(_internal)`, `get_commoditylist_totalavg_prices`, `get_commodity_inst_prices`, `pricelist_price_find_nearest`, `resolve_unknown_comm`, `get_exchange_totals`, `get_exchange_cost_totals`, `make_exchange_alist`, `make_exchange_cost_alist`, `make_exchange_function`, `exchange_by_pricedb_latest`, `exchange_by_pricedb_nearest`, `exchange_by_pricedb_nearest_before`, `exchange_by_pricealist_nearest`, `case_exchange_fn`, `case_price_fn`, `case_exchange_time_fn`, and `apply_exchange_fn`, `apply_price_fn`, `apply_exchange_time_fn` to call what those return |
| `lib/dates` | DU:123-499 | `leap_year`, `days_in_year`, `days_in_month`, the seven `date_to_*_fraction` functions, `date_get_fraction_func` and `apply_fraction_func`, `moddate`, `incdate`, `decdate`, `incdate_months`, `make_date_list`, `make_date_interval_list` (v0.4), the delta constants and `deltasym_to_delta`, `time64_start_day_time`, `time64_end_day_time`, `time64_previous_day` and `time64_next_day` (v0.5; a local day, so 23 or 25 hours across a change of daylight saving time); `date_to_week`, `date_get_week` and `date_get_week_year_string` (DU:123-125, :169-173, :253-254; `date_get_week` takes a time, as GnuCash's `gnc:time64-get-week` does), `date_year_delta` (DU:201-217, exact where GnuCash divides by `366.0`) and `time64_canonical_day_time` (local noon, `time64CanonicalDayTime`, `5.17:libgnucash/engine/gnc-date.cpp:404-410`) (v0.5) |
| `lib/options` | OU:44-234 | `add_report_date`, `add_date_interval`, `add_interval_choice`, `add_account_levels`, `add_account_selection`, `add_currency`, `add_price_source`, `add_plot_size`, `add_marker_choice`, `add_sort_method`, `add_subtotal_view` |
| `lib/acct-table` | HAT:524-1071 | `make_html_acct_table_env_accts` and `html_acct_table_add_accounts` (the balance calculation, the walk and the post-pass), `num_rows`, `get_row_env`, `add_labeled_amount_line`, `gnc_commodity_table` and `add_account_balances`, which append result rows (role, account, depth, level, cells) instead of HTML; the commodity table becomes a `commodities` cell |
| `lib/budget` (v0.4) | RU:727-856 | `budget_get_start_date`, `budget_get_end_date`, `budget_account_get_net`, `budget_accountlist_get_net`, `budget_account_get_initial_balance`, `budget_accountlist_get_initial_balance`, `get_account_period_rolledup_budget_value`, `budget_account_get_rolledup_net` (the last two take the Book first, for the price database) |
| `lib/html-utilities` | HU:168-381 | `assign_colors` (GnuCash's 18-colour palette), `make_rates_table` (a result table), `make_generic_warning`, `make_generic_simple_warning`, `make_generic_budget_warning`, `make_generic_options_warning`, `make_no_account_warning`, `make_empty_data_warning` (warning blocks); `make_report_anchor` (v0.5), a report link copying the options both templates have, which the caller names besides the three General options every report has, then setting the given ones |
| `lib/cash-flow-calc` (v0.5) | `5.17:gnucash/report/reports/cash-flow-calc.scm:33-106` | `cash_flow_calc_money_in_out`: the money into and out of the selected accounts by other account, as GnuCash's alists of collectors, and the two totals; the trading option works backwards as in 5.17 |
| `lib/business` (v0.6) | `5.17:bindings/guile/business-core.scm:35-105` (BC); `gnc:company-info` (`5.17:bindings/guile/options.scm:409-411`) | `owner_get_name_dep`, `owner_get_address_dep`, `owner_get_owner_id`, on `end_owner` (a job's customer or vendor, else the owner); `company_info`, and `book_string`, a string Book option or `""` |
| `lib/register` (v0.5) | `standard/register.scm:32-634` | What the General Journal reaches: `options` (REG:324-404, without the query option), `reg_renderer(book, opts, ctx, query)` (REG:605-634) with the caller's `book.split_query` map, `make_split_table` in journal mode with its rows, notes rows and totals (REG:166-603), `build_column_used`, `make_heading_list`, `get_num_action`. Left out: the `__reg/query` and `__reg/ledger-type` options and what ledger type changes ("T-Num", balances from the totals, "Net Change"), the non-journal mode (a row per split, the Memo and Transfer columns; `reg_renderer` refuses `__reg/journal` false), and `gnc:register-report-create-internal`. No template is registered for the Register report (`milestone5.md`, "Decided", item 5) |
| `lib/trep-engine` (v0.5) | TE:149-2641 | The options and the renderer. `trep_options_generator`; `trep_splits(book, opts, ctx, hooks)`, the first half, returning `#{ parameters, report_title, subtotal_table, splits, warning, export_error }`; `trep_renderer(book, opts, ctx, hooks)`, the document: the table of `make-split-table`, the Subtotal Table of `make-grid`, and under the CSV export type the export rows or GnuCash's export error. `trep_hooks` names the renderer's keyword arguments; `calc_cell` builds a calculated cell for `custom_calculated_cells`; `account_namestring` and `get_num_action` are GnuCash's helpers of those names. The keylists `sortkey_list`, `date_subtotal_list`, `reconcile_list`, `sign_reverse_list`, `date_sorting_types`, `sortkey_informal_headers`; `sortkey_sortkey`, `sortkey_split_sortvalue`, `sortkey_renderer_fn`, `date_sortvalue`, `date_subtotal_renderer_fn`, `subtotal_enabled`, `custom_sorting`, and `apply_split_sortvalue` and `apply_renderer_fn` to call the renderer-fn and split-sortvalue the parameters hold |

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
- **Budgets in the account table** (v0.4): `report_budget` keeps a zero-balance account whose rolled-up Budget amounts
  are not zero (HAT:749-756); `get_balance_fn` takes a caller's closure `|account, start, end|` (HAT:619-658).
- **Refused**: `account_less_p` #t (GnuCash
  names an undefined function, HAT:573), `account_accumulate_at_dates`'s `split_to_date` (no report in scope uses
  it), and a depth limit of #f without "flatten", which fails in GnuCash too (HAT:655-659).
- **CSV export** (v0.3). net-charts and category-barchart offer CSV through GnuCash's export thunks
  (`standard/net-charts.scm:480-517`, `standard/category-barchart.scm:721-725`). Their templates declare
  `exportTypes: ["csv"]`; under `ctx.export_type() == "csv"` they return the rows of `net-charts.scm:433-445` and
  `category-barchart.scm:674-700` as the result's `export`, and `report.exportCsv` writes them as `gnc:lists->csv`
  does. Numbers are printed as Guile prints `(exact->inexact n)`: the shortest digits that read back as the double
  nearest the exact figure, positional unless below `1.0e-3` or past seven integer digits with more than three
  trailing zeros, computed with integers only (`number/flonum.rs`; checked against Guile 3.0.9 on 40,000 random
  rationals). A run that gives no rows (no accounts, no figures) is GnuCash's "No exportable data"
  (`net-charts.scm:456-457`).
- **The Transaction Report engine** (v0.5). `gnc:trep-renderer`'s keyword arguments are the keys of a `hooks` map
  (`trep_hooks()`); an unknown key is a `report-script` error. Its function values (`custom_split_filter`,
  `split_to_date`) are the caller's closures or function pointers, which the module calls. The keylists' lambdas
  (`sortkey-list`, `date-subtotal-list`, TE:166-349) are functions of the key, and the renderer-fn and
  split-sortvalue a parameter holds (`primary_key_renderer_fn`, …, TE:2393-2424) are one map
  `#{ key, date_subtotal, split_action }`, or `()` for #f, called with `apply_renderer_fn` and
  `apply_split_sortvalue`: a key's two functions are #f together. Parameters keep GnuCash's names with `-` and `/`
  as `_` (`common_currency_original`); GnuCash's `(and flag value)` is `false` or the value. The custom sort
  computes each split's value once and sorts the pairs with Rhai's stable sort, as `stable-sort!` with
  `generic-less?` orders them (TE:2448-2472, :2591-2594); GnuCash's first sort, by date with no date subtotal,
  never orders two splits (the note's fact 4) and is left out, as is a sort by a key whose value is #f. The query's
  sort keys are `book.split_query`'s (section 5). Before the query, `splits` is `()`; after it, `[]` when nothing
  matched, so the table knows which warning it has.
  The table (TE:1052-2063): a calculated cell is a map with GnuCash's keys in snake case (`calc_cell`); its
  `calc_fn` and `friendly_heading_fn` are the names of the default functions (`"converted-amount"`,
  `"original-running-prime"`, `"debit"`, `"bal-bf"`, …) or the caller's closures, and a legacy vector cell
  (TE:1014-1026) is upgraded with its one-argument calc-fn. The six running-total collectors live in a `memo()`,
  which the calc-fns update as GnuCash's closures do; the subtotal collectors are arrays the loop replaces. Sign
  Reverses "none" is #t as a parameter (TE:2345); `(keylist-get-info sign-reverse-list #t 'acct-types)` is
  `(assq-ref (assq-ref …) …)` of a key no entry has, and Guile's `assq-ref` is sloppy (`scm_sloppy_assq`): it gives
  #f, also for the #f alist, so `acc-reverse?` reads the global preference, `gnc-reverse-balance`, exactly as for
  "global" (TE:1072-1080). The engine does the same: "none" follows `reports.reverseBalancedAccounts`. GnuCash's
  report tests cannot tell, as they run with no preferences. The Subtotal Table's hash tables keyed by
  `(sortvalue . label)` pairs (TE:2065-2110) are arrays of keys with an `id` made of both.

### Floating point in GnuCash, exact in red-ale

| Where | GnuCash | red-ale |
|-------|---------|---------|
| Day fraction for daily averages and day steps | `(/ caltime 3600.0)` (DU:258-259); category-barchart rounds the averaging divisor to 6 decimals (`standard/category-barchart.scm:299-319`) | exact seconds / 86,400, then the same 6-decimal rounding of the exact value. Differs only if the double lands on the other side of a rounding boundary |
| Pie percentages | `~2,1f` of a double (`standard/account-piecharts.scm:552-554`) | `fixed(r, 2, 1)` of the exact ratio |
| Price chart tooltip x | `~2,2F` (`standard/price-scatter.scm:260-267`) | `fixed(r, 2, 2)` |
| Category chart percentage cells | `double-to-gnc-numeric` to 1/100 (`report/html-style-info.scm:233-236`) | exact, banker's rounding to 1/100 |
| Chart values | JSON doubles (`5.17:borrowed/guile-json/json/builder.scm:121-124`) | exact Rationals in the result; integer SVG coordinates |
| Balance Forecast's "Reserve amount" and "Target amount above reserve" | Doubles of number-range options (`standard/balance-forecast.scm:109-120`), the Target line their double sum | exact Rationals, the Target line their exact sum |
| Investment Lots' CAGR | `expt` of doubles (`standard/investment-lots.scm:447-461`) | fixed-point power rounded to 10⁻¹² (§3, "Growth rates"); exact where the power is |
| Investment Lots' ROI and the weighted CAGRs of its totals (v0.5) | Doubles: `gnc-numeric-to-double` of the gain over the basis (`standard/investment-lots.scm:474-481`); each sale's CAGR weighted by its amount in doubles (`:1873-1875`) | exact Rationals; a cell rounds half away from zero to one place, `~,1f` as GnuCash prints it |
| Investment Lots' holding period (v0.5) | `gnc:date-year-delta`, a double (`5.17:bindings/guile/date-utilities.scm:201-217`), compared with the double "Long term gains age (years)" (`standard/investment-lots.scm:779-789`) | the exact quotient of days by 366, the option a Rational; a holding of exactly the threshold is short term in both |
| Transaction Report CSV numbers (v0.5) | `(number->string (exact->inexact n))` (TE:496) | as v0.3's chart CSV: the shortest digits of the nearest double, computed exactly (`number/flonum.rs`) |
| Transaction Report Shares column (v0.5) | the amount through `double-to-gnc-numeric … 100 GNC-RND-ROUND` (TE:1212-1218; `report/html-style-info.scm:233-236`) | a number cell: the exact amount rounded to 1/100 with banker's rounding; differs only if the double lands across a rounding boundary |
| Investment Portfolio's and Advanced Portfolio's "Share decimal places" | a double option, `inexact->exact` (`standard/portfolio.scm:99-101`, `standard/advanced-portfolio.scm:367-370`) | a whole Rational; a fraction is a `report-script` error, where GnuCash's conversion to an integer fails |
| Advanced Portfolio's Rate of Gain and Rate of Return | 100 × gain (or return) / money in, each a double (`gnc-numeric-to-double`), printed `~,2f%` (`standard/advanced-portfolio.scm:914-922`, `:928-936`, `:1126-1133`, `:1145-1152`) | the exact rate in a number cell, two places, `%`, `rounding: "halfUp"`; empty when Money In is 0. Differs only at a tie the double misses: $0.29 on $8.00 is 3.625 %, GnuCash `3.62%`, red-ale `3.63%` |
| Advanced Portfolio's division-by-zero messages | `~0,4f` of the units, `~,2f` of the basis (`standard/advanced-portfolio.scm:227`, `:306`) | `fixed` of the exact figure |

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
| `reports/balsheet-pnl.gnucash` | `standard/test/test-balsheet-pnl.scm:51-166`, with the FUNDS commodity declared (GnuCash's test never adds it to the commodity table, `:55-61`) | Balance Sheet, Income Statement, Profit & Loss; the Multicolumn statements; Securities pie and Price Scatterplot (hand-worked) |
| `reports/average-balance.gnucash` | `standard/test/test-average-balance.scm:35-81`, all three transfers | Average Balance |
| `reports/charts-net.gnucash` | book B of `standard/test/test-charts.scm:85-114` | the four net charts |
| `reports/charts-daily.gnucash` | book A and the 101 daily transactions of `standard/test/test-charts.scm:148-200` | Net Worth Barchart, Income/Expense Chart |
| `reports/net-days.gnucash` | `standard/test/test-standard-net-barchart.scm:58-311` (the line chart test uses the same books, `test-standard-net-linechart.scm:55-198`) | the net charts, day steps |
| `reports/category-daily.gnucash`, `-average`, `-multi`, `-liability` | `single-txn-test` (which `asset-test` builds identically), `single-txn-test-average`, `multi-acct-test` and `liability-test` of `standard/test/test-standard-category-report.scm:81-318` | the category bar charts |
| `reports/budget.gnucash` (v0.4) | `create-test-data` and `gnc:create-budget-and-transactions` (`5.17:bindings/guile/test/test-engine-extras.scm:851-884`), as `standard/test/test-budget.scm:66-69` builds them; the budget made through ctypes on `libgnc-engine.so`, its recurrence monthly from 2024-03-01 (the first of the pinned now's month) | the Budget Report; the Budget Income Statement and Budget Balance Sheet |
| `reports/adjusting-entries.gnucash` (v0.5), and its SQLite copy `sqlite/reports/adjusting-entries.gnucash`, saved by the oracle as `scripts/sql-fixtures/build.py` saves every copy | `create-test-data`, `#`, and the three 2003 transfers of `standard/test/test-equity-statement.scm:68-75` (the same as `test-trial-balance.scm:69-76`) | the Equity Statement, the Trial Balance |
| `reports/adjusting-entries-fx.gnucash` (v0.5), and its SQLite copy, saved the same way | `adjusting-entries` and the two foreign transfers of `standard/test/test-trial-balance.scm:170-173` | the Trial Balance's unrealized gains |
| `reports/cash-flow.gnucash` (v0.5), and its SQLite copy, saved the same way | the three groups of `standard/test/test-cash-flow.scm:14-144`, posted on the pinned 2024-03-15 | the Cash Flow and its calculation |
| `reports/cashflow-barchart.gnucash` (v0.5), and its SQLite copy, saved the same way | the three groups of `standard/test/test-cashflow-barchart.scm:51-195`, posted on 2024-03-02 and 03-03 | the Cash Flow Barchart |
| `reports/balsheet-pnl-extras.gnucash` and `-trading` (v0.5), and their SQLite copies, saved the same way | none: `scripts/report-fixtures/balsheet_pnl_extras.py` adds what `balsheet-pnl` lacks: expense accounts, a closing transaction, placeholders, an account with no splits, a priced EUR and an unpriced CHF; the second book with trading accounts | the Multicolumn statements (hand-worked) |
| `reports/stock-test-data.gnucash` (v0.5), and its SQLite copy, saved the same way | `create-stock-test-data` (`5.17:bindings/guile/test/test-engine-extras.scm:952-1059`), as `standard/test/test-portfolios.scm` builds it for both its groups; the 1981-01-01 price is $50 (500 for 10), not the $5 the transaction's description says | the Investment Portfolio; the Advanced Portfolio |
| `reports/ifrs-cost-basis.gnucash` (v0.5), and its SQLite copy, saved the same way | `create-ifrs-cost-basis-test-data` (`standard/test/test-ifrs-cost-basis.scm:54-243`): SPY with SCU 10000 and fifteen transactions in CAD with per-split actions; the price database keeps one price per commodity, currency and day, so the two 2020-06-10 transactions leave one SPY price and one USD price, and the 0/450 stock split a SPY price of 0 | IFRS cost basis |
| `reports/transaction.gnucash` (v0.5) | the trep tree of `standard/test/test-transaction.scm:104-226`, then `create-test-invoice-data` (`5.17:bindings/guile/test/test-engine-extras.scm:487-849`) in the same book; no SQLite copy, as GnuCash cannot save it as SQLite | the Transaction Report; the General Ledger and General Journal (hand-worked) |
| `reports/investment-lots.gnucash` and the four `reports/lots-*.gnucash` (v0.5), and their SQLite copies, saved the same way | books A to E of `standard/test/test-investment-lots.scm:59-369` (`scripts/report-fixtures/investment_lots.py`): A, AAPL and SPY bought and partly sold with no lots; B, a lot of 100 AAPL then a 2-for-1 reverse split (value 0, marked `stock-split`, action "Split"); C, B without the split; D, B and a sale of 25; E, D's equivalent bought as 50 shares. B to E keep their splits in one untitled lot, GUID `1…0`; no prices | Investment Lots |
| `synthetic/business.gnucash` and its SQLite copy `sqlite/synthetic/business.gnucash` (v0.6 for reports) | none: the business fixture of [`testing.md`](testing.md): a customer invoice taxed by a table of two accounts and paid in part, a bill, an expense voucher and an unposted credit note for a job, in euros | the Printable Invoice (hand-worked), and its SQLite runs: `reports/transaction.gnucash` has no SQLite copy |
| `reports/reconcile.gnucash` (v0.5), and its SQLite copy, saved the same way | the reconcile tree of `standard/test/test-transaction.scm:1077-1104`, alone | the Reconciliation Report; the General Ledger (hand-worked) |

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
| Budget Report (v0.4) | `standard/test/test-budget.scm:66-161`: text counts, the Bank, Income and Expenses rows and the footnotes, with every column, `current`..`next`, `last`..`last`, manual 2-4 uncollapsed and accumulated | the defaults; notes without the budget column; collapsed titles; `previous`; relative periods before and after the budget; truncated exact periods; a total with no periods failing as in GnuCash; accumulated columns of each kind; reversed accounts; zero accounts omitted; sub-accounts and flattening; the Transaction Report links; both warnings |
| Budget Balance Sheet (v0.4) | `standard/test/test-budget.scm:186-253`: the text count and the existing, allocated and unallocated assets, total assets, existing, new and total liabilities, existing and new retained earnings, unrealized losses, existing, new and total equity, and total liabilities and equity | the sheet at the budget's start; moving the start; retained losses and the totals alone without new/existing lines; zero retained earnings and gains left out; liability lines signed by the first liability account; the two-column form and rates; both warnings |
| Budget Income Statement (v0.4) | `standard/test/test-budget.scm:163-184`: the text count, Income's budgeted $195.00 and the net income of $105.00 | the defaults; the period range, a single period and a range, with prices at the first reported period's start; truncated and refused ranges; reversed accounts; the four layouts; the warnings; the rates |
| Budget Profit & Loss (v0.4) | none: the test runs the Income Statement's GUID only | the Budget Income Statement's checks against this template; the same statement under its own title; its no-account warning naming the Budget Income Statement |
| Budget Flow (v0.4) | none: GnuCash has no test of it | a single period; budgeted against actual, with an unset amount and a closing transaction; a period past the budget; no reversed signs; totals of a foreign account under the latest and the nearest price, the latter at the end of the next period; selected accounts and a parent left out; both warnings; a period that is not whole failing |
| Budget Chart (v0.4) | none: GnuCash has no test of it | budget against actual series per account, with the running totals in the title; running sum off; bars and lines; manual, first-to-last and previous ranges, exact periods rounded; relative periods before and after the budget; the depth limit and rolled-up budgets; both warnings |
| Equity Statement (v0.5) | `standard/test/test-equity-statement.scm:77-112`: the text count and the seven rows (capital at the start, net income, investments, withdrawals, unrealized losses, increase in capital, capital at the end), on the XML book and its SQLite copy | the options and their order; no accounts; a net loss and a decrease in capital; no unrealized row, then unrealized gains and losses; accounting-style rules; the exchange rates at both dates; each price source, average cost valuing the book at cost; the closing pattern plain, ignoring case, case-sensitive and as a regular expression, and closing transactions whatever their description, a closing loss counted as a withdrawal; a Start Date on a split's day; withdrawals under the transaction's currency; the company name in the title; "Show Foreign Currencies" read nowhere |
| Trial Balance (v0.5) | `standard/test/test-trial-balance.scm:78-179`: the text counts and every column of the current, pre-adjustment and work sheet runs, and the unrealized gains after the foreign transfers, on both books as XML and as SQLite; the work sheet's Net Income read in the Income Statement column (§14) | the options and their order; no accounts; TRADING accounts without rows but in the rates, and only TRADING accounts selected failing; gross adjustment and income summary accounts, with a debit and a credit balance before adjustments; a Net Loss, and no net income (blank, the balance sheet's $0.00) with the grand totals of each branch; the patterns ignoring case, case-sensitive, as a regular expression or not, closing, matching both (taken out twice), empty, and before the start; closing transactions whatever their description; the unrealized gains by each price source and in the work sheet; foreign currencies shown; a foreign zero ($0.00) against no splits (blank); totals converted once (two $1.00 rows, $2.01 total); the levels' indentation and the unrealized row's; hyperlinks off; the company name and the period titles |
| Cash Flow (v0.5) | `standard/test/test-cash-flow.scm:30-146`: the calculation's money-in and money-out alists, each account's collector and the two totals, read through a test template that calls `lib/cash-flow-calc` with identity conversions as the test does, and the same groups through the Cash Flow's rows and totals (worked from `standard/cash-flow.scm:266-317`), on the XML book and its SQLite copy | the options and their order, the trading option's help saying it works backwards; no accounts; a year of `charts-daily`, voids left out; test-data's multi-split with one and two selected splits; every price source at each split's posted date; the weighted average with a currency no selected account holds (a $0.00 row); a foreign report currency; the rates; the Selected Accounts list at depths 1, 2, 3 and all, both suffixes, full names off, and off; sub-accounts joining the selection; a closing transaction counted; a zero split, a voided transaction, one account both in and out; the trading option both ways |
| Balance Sheet (Multicolumn), Income Statement (Multicolumn) (v0.5) | `standard/test/test-balsheet-pnl.scm:493-650`: all 16 checks, the row headers, each column's balances with the conversions and the rates, the date rows both orders, the P&L's monthly columns and its headers, and the rates under each price source (the runs at `:617-650` give the P&L's options to the balance sheet's GUID, which GnuCash renders as a balance sheet), on the XML book and its SQLite copy. GnuCash reads its HTML table's texts; the test rebuilds them from the result: the rate rows back into their title row as `"<one unit> <rate>"`, each cell's texts in `sxpath`'s breadth-first order (an amount shown before its `<small>` original and that original's trailing space, `" to <end>"` after a P&L header's start), and account names with `:` for the fixture's `.` | the options and their order; no accounts under each title form; dual columns and the amount staircase, indented names, placeholders unlinked, split links (none before an account's first split); the P&L leaving the closing transaction out and the balance sheet keeping it; multilevel mode; zero accounts hidden and zero figures omitted in both modes; a depth limit, "Label sections" off (the rates vanish) and "Include totals" off (the Net Income stays); a common currency with an unpriced commodity ("missing", unconverted), unrealized gains, retained earnings unconverted while an income commodity has no price and converted without it; each price source, and the amounts without their originals; `pricedb-latest` ending every P&L header and Transaction Report link today; the Transaction Report links with and without a common currency, the main currency on a tie, and the Barchart links run on their templates; the two weighted averages differing on `balsheet-pnl`; weekly, two-week, swapped, yearly and month-end periods, the overall column first and last; trading accounts in place of unrealized gains; a run changing nothing on both extras books, XML and SQLite |
| Cash Flow Barchart (v0.5) | `standard/test/test-cashflow-barchart.scm:60-196`: money in, out and net per day and in total, read as numbers as the test reads them, and the row and column counts, on the XML book and its SQLite copy | the options and their order; a year of `charts-daily` by month, its colours and totals; no accounts; all zero (no table even with "Show Table"); a hidden non-zero series still drawing the chart; each "Show" option; the values' links to the Cash Flow, with the options both reports have, and running one; a short last interval and swapped dates |
| Transaction Report (v0.5) | `standard/test/test-transaction.scm:241-1050`: every assertion of `transaction.expected.json` (115 table checks: headers, totals, cell and row counts, links, sign reversals, sort orders, date subtotals, running totals, informal headers, both Subtotal Tables, the invoice cell), each expected string also in the exported HTML; the CSV string and its export error (TT:1025-1050) through the engine with the CSV export type, as the report declares none; on the XML book only, as GnuCash cannot save it as SQLite | entered and reconciled dates, Shares and Price cells; document links with links on and off; numbers in a split-action book with Trans Number; a payment paying two invoices (anchors in reverse order); Balance b/f under Common Currency; each condition that turns Balance b/f off; the Subtotal Table with no secondary key, one column, two amount columns, subtotals only; the CSV export errors; custom and legacy calculated cells; Sign Reverses "none" against a global preference; the empty-report message; a run leaving the Book unchanged; test-data's XML and SQLite copies giving the same result |
| Investment Portfolio (v0.5) | `standard/test/test-portfolios.scm:53-78`: AAPL's row under `pricedb-latest`, `pricedb-nearest` on 1980-03-01, and `average-cost` and `weighted-average` on 1980-09-01, on the XML book and its SQLite copy; the null test (`:26`) as the no-account warning on an empty book | the options and their order, the default accounts; the rows, rule and Total, the account and price links; accounts with no shares (no price: $0.00 unlinked) and the shading over shown rows only; `pricedb-before`, `-nearest` and `-latest` on one date, the latest ignoring it; prices shown as stored with every price-database source, a newer price in another currency and one quoted the other way round (`0.025 ABC`, the note's Q9), the values converted through the price database; `nearest` taking the newest of each currency's nearest; no price; the averages' five significant figures ($1,234.60 against a $12,345.67 value); 0, 2 and 3 share places with a half-up tie; no shown rows (the rule alone); a run changing nothing, XML and SQLite |
| IFRS cost basis (v0.5) | `standard/test/test-ifrs-cost-basis.scm:264-357`: the texts of all fifteen rows, in order with empty cells giving none, on the XML book and its SQLite copy; the null test (`:38`) as the options warning and the disclaimer on an empty book | the options and their order; the title, the 28 column labels, the split links, the rate as a price and the disclaimer; a sale and a cover read column by column, the basis kept exact; shading by shorts and by row; commissions not capitalised; monetary cells off (bare numbers, the rate still a price); the Start Date's transactions in and the End Date's out, earlier ones still moving the running figures; the types the test does not reach (dividend, return of capital as a reclassification, notional capital gain, a reverse split, a compensatory dividend, both ERROR rows, "Unknown" for an expensed purchase fee, "Sell" for what a reverse split with cash in lieu would be, a voided transaction); one account for dividends and gains; a rate from the price database rounded to the report currency's cents; the fee filter, another filter and the num field source; a fee-only transaction with units held (empty units) and with none (the `report-script` error); a run changing nothing, XML and SQLite |
| General Ledger (v0.5) | none: GnuCash has no test of it (`notes/report-transaction-engine.md` §4) | the changed defaults and the missing "Running Balance" and "Totals", with no book and in a split-action book; a run on the defaults: account-code groups headed by code and name, two amount columns, no balance column, a Grand Total with a line per commodity; "Account Balance" bringing balances forward, Income reversed by "Credit Accounts" and not by the global preference; the title on a warning whatever the Report name; test-data's XML and SQLite copies giving the same result; a run leaving the Book unchanged |
| Reconciliation Report (v0.5) | `standard/test/test-transaction.scm:1107-1121`: the three subtotals of `reconcile.expected.json` ("Total For Reconciled" $8.00, "Total For Cleared" $29.00, "Total For Unreconciled" $31.00), each also in the exported HTML, on the XML book and its SQLite copy | the whole table of that run; the run on the template's dates (TT:1108-1109; the previous quarter counted from the accounting period's start); void transactions under "Both", grouped first with their former amounts; Common Currency leaving the amounts and headings unconverted; no accounts (the warning, then the instructions); the changed defaults; test-data's XML and SQLite copies giving the same result |
| General Journal (v0.5) | none: GnuCash has no test of it or of the Register report's journal mode | the defaults on the transaction book: every non-void transaction in date order, the closing entry kept, transaction, notes and split rows with their shading, split links, no totals; every column (Num, Shares, Lot, Price, single Amount, Value, Balance of the split the query found) and the four totals and the change in value by commodity; the totals in two columns; a balance reversed by the preference; two transactions on one day in split order; a split-action book (the found split's action, the num on the notes row); `double` off and other debit and credit strings; `__reg/journal` false refused; the options; test-data's XML and SQLite copies giving the same result; a run leaving the Book unchanged |
| Advanced Portfolio (v0.5) | `standard/test/test-portfolios.scm:84-107`: AAPL's row under the average, FIFO and LIFO bases, on the XML book and its SQLite copy; the null test (`:27`) as the no-account warning on an empty book | the options and their order, the default accounts; the rows, the rule over every column and the Total row, the account and price links; the exact rates (−5/8 %) as half-up number cells; the price list off (a transaction's price, `*`, linked to it, found up to the Date or now) and accounts with no price (`**`, $1.00), each note and both a line apart; both price sources on one date; the Display options and share places; prices in another currency, a report-currency price preferred to a newer one, an inverted price linked to the price stored, an unconvertible price falling back to the transaction; the three fee options; the three bases on a sale across pairs; the rate tie (`3.63%`); the 10⁻⁸ price ($9,999,999.99 for $10,000,000.00) and the fee's SCU share ($309.99 against $310.00 in); a sale with nothing held (no money in, empty rates), a gift to charity, a worthless stock; the division by zero after a short sale and on a spin-off with no basis, rows before it kept; income and fees of the parent account by name; reinvested dividends with a sibling holding account, an unrelated one and a negative holding; a purchase in another currency; a run changing nothing, XML and SQLite |
| Investment Lots (v0.5) | `standard/test/test-investment-lots.scm:392-834`: the eleven render-only runs and the option checks on book A; the helper checks against the script's own helpers and the engine's `calculate_cagr` within GnuCash's ranges (`calculate-gain` is not translated; the `is-long-term?` cases under the report's rule, where a year to the day is short term); the reverse-split checks on B and D and the comparisons of B with C and D with E, each read by label and as GnuCash reads them, a column to the right (the plain style sheet's account-name `<th>` counts in the header list); on the XML books and their SQLite copies. With the default window nothing is sold and the default grouping has no Realized Gain column, so GnuCash's comparisons hold trivially; they are repeated over 2020, where they compare real figures. The worked figures of `workedFromCode` for every default run and for D over 2020: a $500 short-term gain, a CAGR of 204.1 %, and the gains-mismatch warning | the options and their order; D's whole table over 2020, its split rows, links and export; the column options (gains and sales by term, ROI, GUID, no grouping, zeros shown, every column off); the Start date's own trades outside the window; the long-term threshold around a leap day, at 0 and at 0.5; several purchases (CAGR from the latest, 399.4 %); a forward split as a purchase; an oversold lot and a sale from an empty lot (blanks, no CAGR); gains splits that match and do not; unassigned splits counted, listed and ignored, and the balance check; "Include only accounts with warnings" with a blank title; the include and display options; the three price sources, an inverted price, a third currency, a future price and none; same-day sales in reverse Book order; the chart's variants; no accounts; a run changing nothing, XML and SQLite |
| Transaction Breakdown Report (v0.5) | none: GnuCash has no test of it; `test/test-stress-options.scm` only renders it | the options and their order; the default run (no account) as the empty-data warning, titled with the "Report name" option, on test-data and an empty book; Bank in 1970–71 on test-data: the columns by account order and fundamental type, their three-row headers, the cells, the multi-split row, the total row and the split links; the 2000 foreign transfers as `commodities` cells; Liabilities and Income negated under `credit`; the description filter, case-sensitive (none left: the warning); the column limit at exactly the count and below it (the "Too many accounts" warning); a two-row header and the closing transaction kept; a void transaction left out; on books built through the API, two splits to one account summed and linked to the first, the columns in `xaccAccountOrder` across the tree rather than depth first, the transaction's num in a split-action book, and TRADING accounts left out; test-data's XML and SQLite copies giving the same result; a run leaving the Book unchanged |
| Printable Invoice (v0.6) | `standard/test/test-invoice.scm:93-261`, the Printable variant (`report_invoice.rs`): each document's last cells, its first entry less the date, the client's name, the texts in the body, the details' first column, the sparse run's first and last cells, and that inv-8 is paid; every option TI sets exists | on `synthetic/business.gnucash`, XML and SQLite: two tax accounts with and without the detailed summary, a partial payment with the Total column on and off, an unposted credit note for a job with its job rows, a bill and an expense voucher; on `reports/transaction.gnucash`: each display column, three and none shown, the minimum number of entries (fractional and 0), every layout choice, the Picture Location (never read: a FIFO), the company details and footers with a Custom Title and the owner's ID, the CSS option and refused CSS, no document and a GUID naming none; References with two orders and a line without one and exact quantities (1/8, 1/3) on a document made through the API; the Fancy Date Format (`red-ale-report/tests/report_invoice.rs`); golden bytes of every document (`report_golden/v0.6.txt`); a run leaving the Book unchanged |
| Multicolumn View (v0.5) | none: GnuCash has no test of its output; `test/test-stress-options.scm:8` renders it with no children | no children (an untitled, empty grid); rows closing at "Number of columns" with a row span taking room below, a span wider than the grid, "Number of columns" 0 and column spans of 0; a template child with options and a saved configuration child, each cell equal to the child run alone, on test-data's XML and SQLite copies, export bytes the same each time and on both; the child's style sheet ignored (one since deleted); "Single Report" links; the `Embedded Report` rows; a deleted saved configuration, failing and no-longer-valid children as warnings in their cells, the others still run; a saved configuration child following its later changes; nested views, a cycle of saved configurations, the depth and count limits; an operations limit in a child stopping the whole run; a run leaving the Book unchanged |
| Income vs. Day of Week, Expenses vs. Day of Week (v0.5) | none: `standard/test/test-charts.scm:71-77` and `:187-190` only render them; both run as checks here, on an empty book (the empty-data warning) and on charts-daily with the calendar year (GnuCash's Accounts value refused, as its validation refuses it) | the options and their order for both templates, menu and tips; 2024 on charts-daily, income negative and the void left out, expenses with a negative day; Income by weekday over 1970's 101 daily transactions, also on the SQLite copy; "Show Totals" off and a "Report name" in the titles but not the series; the warning for only zero totals, for no splits and for no accounts; "Include Sub-Accounts" on and off with a child account; a GBP account at each split's date under `pricedb-nearest` and `weighted-average` (an exact $14.375 printed whole), and no weighted average before any exchange (the warning); a run leaving the Book unchanged, XML and SQLite |

Every check of a figure also runs with `report.export` and checks that the HTML has the same display string.

GnuCash's text counts (`(sxml->table-row-col sxml 1 #f #f)`) read a text more than once when it sits in nested tables:
Guile's `sxpath` applies the rest of the path `// (table 1) // tr // td // *text*` to each node a step selects, and
every table that is its parent's first is selected, so a text inside *k* nested tables is read once for each table,
row below it and cell below that row on its way down, k(k+1)(k+2)/6 times. A foreign account's commodity table is one
table down (`report/html-acct-table.scm:925-948`), and the Budget Income Statement's layout table adds one more
(`standard/budget-income-statement.scm:541-564`). The tests count the result's texts the same way.

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

Collected with every other area's in [`differences-from-gnucash.md`](differences-from-gnucash.md); a new row goes in both.

| What | GnuCash 5.17 | red-ale | Why |
|------|--------------|---------|-----|
| Amounts past 64 bits | Fail at `gnc_scm_to_numeric`: an empty cell or an error (`5.17:bindings/guile/gnc-engine-guile.cpp:1709-1727`) | Exact, printed | Figures within 64 bits are the same |
| Floating point (day fractions, pie percentages, chart values) | Doubles | Exact, rounded as GnuCash displays | No floats anywhere in red-ale |
| Bad option values | Logged and ignored | `report-option` error | A typo never runs a different report |
| A script's `set-default-value` or `make-internal` on an option that does not exist | Ignored (`5.17:bindings/guile/gnc-optiondb.i:1500-1503`; `5.17:libgnucash/engine/gnc-optiondb.cpp:266-272`); the General Ledger and Reconciliation Report rely on it | `report-script` error; the translations leave those calls out | A misspelt name in a user script is found; the figures are the same |
| A Multicolumn View's children (v0.5) | Report instances of the session, made by "Add" with a copy of a saved configuration's options (`report/report-core.scm:389-409`); an unknown id fails the whole view (`standard/view-column.scm:84-85`) | Template or saved configuration GUIDs (§6, report placements); a saved configuration child is a reference; an unknown GUID or invalid child options set by a caller are `report-option` | No sessions of report instances in red-ale; a reference stays put across restarts (`notes/report-statements-charts.md` Q4, Q7) |
| A Multicolumn View's child that cannot run (v0.5) | An unknown report id fails the whole view (`gnc:report-set-stylesheet!` outside the error handler, `standard/view-column.scm:84-85`); a failing child shows "Report error" in its cell (`:87-97`) | Every child that cannot run (gone, failing, too deep, past 100, a cycle) shows "Report error" in its cell; a child past an engine limit stops the run | One stale reference does not hide the other children |
| A Multicolumn View's links (v0.5) | "Edit Options" and "Single Report" under each child (`standard/view-column.scm:115-130`) | "Single Report" only, a report link | `report.options` edits options; red-ale has no instances |
| A saved Multicolumn View inside a saved view (v0.5) | Saved with its report-list of the session's instance ids and none of its children (`report/report-core.scm:567-598`); reloaded, it finds no report by those ids (the whole view fails) or another one | Imported with no children; its report-list is listed as `unsaved-children` | The children were never saved; an empty view is what can be kept (§15) |
| A split query's description pattern that is not a regular expression | The criterion is dropped (`qof_query_string_predicate` returns null, `5.17:libgnucash/engine/qofquerycore.cpp:319-334`) | `report-script` error | A typo never matches every transaction |
| A split query's regular expressions and case folding | POSIX extended expressions; `g_utf8_casefold` | Rust `regex`; lowercase, as `transaction.find` | Same results for patterns both dialects read alike |
| The Transaction Report engine's account and transaction filters (v0.5) | POSIX extended expressions (`make-regexp`, with `regexp/icase` when case-insensitive), `string-contains-ci` (`report/trep-engine.scm:2210-2254`, `:2439-2446`) | Rust `regex` through `make_regexp` (section 2); case-insensitive substrings compare lowercased | As the split query's row |
| Name comparisons in the Transaction Report engine's sorts (v0.5) | Locale collation: `gnc:string-locale<?` of whole full names in the custom sort (`report/trep-engine.scm:2467-2468`), `g_utf8_collate` per path level in the query's account-name sort (`5.17:libgnucash/engine/Split.cpp:1687-1709`) | Bytewise, whole full names in the custom sort and per path level in the query's (`notes/report-transaction-engine.md`, fact 5) | As "Name sorting" |
| A split query sorted with no first key (the Transaction Report's Primary Key "None") | Not sorted: hash order (`5.17:libgnucash/engine/qofquery.cpp:492-497`, `:792-797`) | Split order | Determinism; GnuCash's order is undefined |
| A split query's splits equal at every sort level | The order the query collected them in, hash order (a stable merge sort, `qofquery.cpp:792-797`) | Split order | Determinism; the Transaction Report always sorts by split order third, so its splits never tie |
| Subtotal Table rows or columns with equal sort values and different labels (two accounts with the same code) (v0.5) | Hash order (`report/trep-engine.scm:2086-2090`, `:2118-2119`) | Ordered by label, then as they came | Determinism |
| The General Journal's query (v0.5) | Stored as the default of the Register report's `__reg/query` option (`standard/general-journal.scm:57-79`; `standard/register.scm:327-328`) | Built by the script; there is no query option | No query option type; GnuCash's GUI never changes it for the General Journal |
| `__reg/journal` false in the General Journal (v0.5) | The Register report's non-journal layout (`standard/register.scm:91-102`, `:201-217`) | `report-script` error | Only journal mode is translated (`lib/register`); the option is internal and GnuCash's dialog never shows it |
| General Journal total rows (v0.5) | The label, then the total in one cell spanning from the second column to the amount column (`standard/register.scm:464-471`); `" "` cells under the date, num and balance | The total in its amount column, empty cells before and after; empty cells for `" "` | The result names columns; the figures are the same |
| "Show Account Description" with an Other Account key on a split transaction (v0.5) | A Scheme error: `xaccAccountGetDescription` of no account (`report/trep-engine.scm:1828-1829`) | No description | The report runs |
| The Transaction Report's informal Debit/Credit headers (v0.5) | Debit and Credit under the "use formal accounting labels" preference (`5.17:libgnucash/engine/Account.cpp:4081-4082`) | Always the friendly labels, GnuCash's default | No such preference in red-ale (`notes/report-transaction-engine.md` §7 Q5) |
| Split order with the num field source on | `xaccSplitOrder` is not a total order: two splits of one transaction compare by memo, then action (`5.17:libgnucash/engine/Split.cpp:1559-1597`), splits of different transactions by their actions as nums (`:1548-1556`), so splits A1 < A2 of one transaction by memo, with A2's action below A1's, form a cycle with a split B of another transaction on the same day whose action falls between (A2 < B by date entered, B < A1 by action); the sort's result follows hash order, and Rust's sorts may panic on such a comparator | Two splits of one transaction compare their actions as nums first, then as GnuCash does: a total order (`ops::transactions::order::split_order`). Splits of different transactions, and two of one transaction whose actions are equal as nums, are ordered as GnuCash orders them | A user's book never crashes a sort; the num field source is about ordering across transactions, which is kept |
| Nums (`order_by_int64_or_string`, `5.17:libgnucash/engine/Transaction.cpp:1773-1791`) | Two nums compare by their `strtoull` numbers when both have a non-zero one, else as whole strings, which cycles: `"-3" < "0" < "5" < "-3"` | Nums without a number that sort as text below `"1"` first, then nums with a number by number and rest, then the others as text (`order::num_order`). It differs only for a num with a number that starts below `"1"` (`" 10"`, `"+5"`, `"-3"`, `"05"`) against one without that also starts below `"1"` (`"0"`, `" "`) | A total order; usual nums (`""`, `"1"`, `"10"`, `"abc"`) are ordered as GnuCash orders them |
| The split query's `accountFullName` sort under two sibling accounts of the same name | Equal at that level (`xaccSplitCompareAccountFullNames`, `Split.cpp:1687-1709`), which is not an order once the siblings' sub-accounts are compared too | The names of the levels below decide | A total order |
| Saved configuration name clash | A number is appended on save | `conflict` | The caller gets the name it asked for |
| User text in style sheets | Inserted as raw HTML | Escaped | No injected markup in exported reports |
| CSS style sheet text | Default CSS on `</style` | Refused on `</style`, `@import`, `url(`, `image(`, `image-set(`, `src(`, `\` | Exported pages load nothing |
| Hover labels and chart look | Chart.js, `Intl.NumberFormat` | Static SVG, red-ale's formatting under the Locale | No script; deterministic bytes |
| Drill-down links | Report instances made at render time | Described in the result; no `href` in HTML | Clients run them with `report.run` |
| Document links (v0.5) | A `gnc-register:` link naming the transaction or invoice, opened by GnuCash (`report/html-utilities.scm:96-100`) | The document's URI in the result; text only in HTML | An exported page never points at a user's files |
| Title of the account pie charts | None: the document keeps its empty title (`report/html-document.scm:131`); the chart's title and the warnings' heading name the report | The report name, with its averaging suffix | A result has a title, and its warning blocks leave out the heading that names the report |
| Average Balance, Price Scatterplot and day-of-week pie titles, and the budget reports', the Transaction Report engine's and the Transaction Breakdown's warnings | Untitled documents (`example/average-balance.scm:291`, `standard/price-scatter.scm:146`, `standard/budget.scm:661-671`, `standard/budget-balance-sheet.scm:361-371`, `standard/budget-income-statement.scm:384-410`, `standard/budget-flow.scm:270-281`, `standard/budget-barchart.scm:289-298`; the Budget Chart's charts, `:313-316`; `report/trep-engine.scm:2501-2531`, `:2596-2605`; `standard/txn-columns.scm:143-156`, the title set only for the table at `:246`; `example/daily-reports.scm:144`, `:249-286`) | Titled with the "Report name" option (the budget reports' warnings: with the name the warning gives) | red-ale's warnings leave out the report-name heading GnuCash gives them; the title names the report |
| Unordered sets (`gnc-accounts-and-all-descendants`, which the Cash Flow and the day-of-week reports also use, `standard/cash-flow.scm:159`, `example/daily-reports.scm:175-177`, `:196-198`; the Securities pie) | Hash order | A fixed order | Determinism; no figure changes |
| Name sorting | Locale collation | Bytewise | As v0.1 |
| Date interval in titles under the `locale` format and the C locale | ICU's interval form for en_US_POSIX (`1/1/2026 – 12/31/2026`) | `<from> to <to>` | Output under C stays as it was before Locales |
| ICU data: the first day of the week, date intervals | The ICU the build links (ICU 74, CLDR 44, for the oracle) | ICU4X 2.4's compiled CLDR | Differences only where CLDR changed between them; the multi-day intervals of every supported locale sampled matched ICU 74's. A one-day interval prints ICU4X's `YMD` short pattern (`05.03.2026` under de_DE) where ICU 74 prints its `yMd` pattern (`5.3.2026`) |
| ICU dates outside 1400–9999 | Formatted | `<from> to <to>` | GnuCash's dates cannot hold them (`5.17:libgnucash/engine/gnc-datetime.hpp:240`); ICU4X 2.4's range formatter fails on years that need an era |
| `"%s to %s"` in titles | Translated through gettext | English | red-ale has no translations |
| Category chart series under "Percentage chart" | Fractions, shown ×100 by Chart.js's percent style (`standard/category-barchart.scm:611`, `:622-623`) | Percentages | A `percent` axis shows its values as they are |
| Accounts with no commodity (the trading accounts' parents in a book with no root currency, `5.17:libgnucash/engine/Scrub.cpp:493-516`) in the Balance Sheet, Income Statement, Equity Statement, Trial Balance and the Multicolumn statements | Their zero balance is added under `#f`; the exchange-rates table fails sorting the missing commodity (`report/report-utilities.scm:242-251`) | Left out of the balances and the rates table | The same figures and rates, and the report runs |
| Statement and account-table layout | Spacer and padding cells place amounts in columns (`standard/balance-sheet.scm:444-447`, `standard/equity-statement.scm:422-423`, `report/html-acct-table.scm:870-923`), and narrow indent cells and empty cells the Multicolumn statements' names and amounts (`standard/balsheet-pnl.scm:346-369`); the two-column forms nest tables in one row, as do the Equity Statement's exchange rates, under a header row of the two dates (`standard/equity-statement.scm:473-491`) | Rows with roles, depths and staircase levels; two columns are a `group` of tables, and so are the two rates tables, each titled with its date | The result carries data, not markup; the renderer draws the staircase |
| Trial Balance work sheet: Net Income/Loss figure | In the Adjustments column, by `add-labeled-amount-line`'s padding (HAT:899-901), not the Income Statement column `standard/trial-balance.scm:821-826` names; GnuCash's test reads it there (`standard/test/test-trial-balance.scm:136-138`) | In the Income Statement column | The result names columns; GnuCash's position is an HTML artifact |
| Trial Balance amounts with two or more account columns | Shifted left of their Debit/Credit headers by the same padding | In their columns | The same |
| Trial Balance with only TRADING accounts selected | Fails at `standard/trial-balance.scm:520-523`, reading the columns of a table with no row | `report-script` error saying only TRADING accounts are selected | Fails as GnuCash does, with a reason |
| Trial Balance layout | A spacer row of 60-pixel cells and `th` header rows in the table body (`standard/trial-balance.scm:523-573`) | Column labels "Account Name", "Debit" and "Credit"; the work sheet's five headings a first `heading` row of cells two columns wide | Layout, not data |
| Cash Flow's "Selected Accounts" | A `<ul>` of register links (`standard/cash-flow.scm:253-262`) | A `list` block whose items name their accounts | The result carries data and keeps the links |
| The cash-flow calculation's null-account warning | Printed to standard output (`5.17:gnucash/report/reports/cash-flow-calc.scm:65-67`) | The report log | Scripts have no standard output |
| Multicolumn statements' exchange rates | All rates of a column in one title-row cell, separated by line breaks (`standard/balsheet-pnl.scm:770-797`) | One row per commodity under the "Exchange Rates" heading: the one-unit monetary, then a price cell per column, or `"<symbol> missing"` | A cell holds one content; the figures and texts are the same |
| Multicolumn links' main currency on a tie | Hash order (`standard/balsheet-pnl.scm:804-812`) | The first in the accounts option's order | Determinism |
| A Multicolumn P&L header whose end is before its start (dates given swapped without a period; `pricedb-latest` with a period after now) | `<start> to <end>` (`standard/balsheet-pnl.scm:1140-1153`) | The same words in a text cell, not a date cell with `until` | A period's `until` may not be before its date |
| "Include Trading Accounts in report" of the Cash Flow and the Cash Flow Barchart | Works backwards since commit `2215dfc5e1`: off counts trading splits, on leaves them out (`cash-flow-calc.scm:68-70`); the help says the opposite (`standard/cash-flow.scm:99-101`) | The same figures; the help says it works backwards | Figures match 5.17; revisit if GnuCash fixes it |
| GnuCash's saved-reports and style-sheet files | Evaluated at start-up | Imported on request, as data (section 15) | They are Scheme programs; red-ale never runs Scheme |
| An entry of those files red-ale cannot import | An error while evaluating it | Skipped with its reason; the rest are imported | One entry does not cost the others |
| Advanced Portfolio's rates at a tie (v0.5) | Rounded from a double: 3.625 % prints `3.62%` (`standard/advanced-portfolio.scm:914-922`) | From the exact rate, half away from zero: `3.63%` | No floats ([`notes/report-investments.md`](notes/report-investments.md) Q1) |
| Advanced Portfolio's rates of 1,000 % or more, and under another Locale (v0.5) | Guile's `~,2f`: no grouping, `.` always | A number cell: grouped, with the Locale's separators | Rates are number cells, so clients keep the exact figure (the note's Q3) |
| Advanced Portfolio's investment arithmetic past 64 bits (v0.5) | `gnc_numeric` rounds to fit or fails to `#f` (`5.17:libgnucash/engine/gnc-numeric.cpp:71-85`; `5.17:bindings/guile/gnc-engine-guile.cpp:1729-1734`) | Exact | Same within 64 bits |
| Advanced Portfolio's link to an inverted price (v0.5) | A temporary price not in the book (`gnc-price-invert`, `standard/advanced-portfolio.scm:331-332`, `:900-903`) | The price stored | A link must name a real price |
| Advanced Portfolio's division-by-zero error and rule (v0.5) | Raw HTML before the table; a rule cell 17 columns wide whatever the table has (`standard/advanced-portfolio.scm:51-55`, `:1101-1106`) | A `warning` block titled "Error"; a rule over the table | Layout, not data |
| IFRS cost basis: a transaction with no stock amount and no units held after it (v0.5) | A Guile error, `(< 0 #f)` (`standard/ifrs-cost-basis.scm:417-418`) | `report-script` error naming the transaction | The same failure, with a message (`notes/report-investments.md`, Q11) |
| Transaction Breakdown's `[value] amount` (v0.5) | One cell of text, the value in the transaction's currency in brackets and the amount in the account's commodity linked to the split (`standard/txn-columns.scm:228-235`) | A `commodities` cell of one line: the amount, `converted` the value; the cell linked to the split | The pair is a monetary and its value; the figures are the same |
| IFRS cost basis: a transaction with no stock amount while units are held (v0.5) | A split link around `<boolean> #f`, the fallback rendering of #f (`report/html-document.scm:238-245`, `standard/ifrs-cost-basis.scm:485`) | An empty cell | A figure that is none is an empty cell everywhere else in the report |
| Investment Lots' percentages of 1,000 % or more, and under another Locale (v0.5) | Guile's `~,1f`: no grouping, `.` always (`standard/investment-lots.scm:1141-1162`) | A number cell: grouped, with the Locale's separators (`37,582.1%`) | As Advanced Portfolio's rates: clients keep the exact figure |
| Investment Lots' link to an inverted price (v0.5) | A temporary price not in the book (`gnc-price-invert`, `standard/investment-lots.scm:620-636`, `:944-962`) | The price stored | A link must name a real price |
| Investment Lots: a sale in the window from a lot with no purchase (v0.5) | A Guile error: `gnc:date-year-delta` of `'()` (`standard/investment-lots.scm:1859-1863`) | No CAGR: an empty cell, the rest of the row as computed | The report runs; the CAGR is undefined as when the basis is (`notes/report-investments.md`, Q6) |
| Investment Lots: an untitled lot with "Warn if a lot has a blank title" (v0.5) | A Guile error: `string-trim` of the `#f` that `gnc-lot-get-title` gives (`standard/investment-lots.scm:1577-1580`) | The blank-title warning | An untitled lot is blank; the report runs |
| Investment Lots: an account total's shares when the account has no lot (v0.5) | Printed with the default print info of a commodity of `'()`: the locale's default currency (`standard/investment-lots.scm:810-813`) | Printed in the report's currency | red-ale has no default-currency preference; the report's currency stands in |
| Investment Lots' chart with no account selected (v0.5) | A chart it never set up, with no data (`standard/investment-lots.scm:2266-2298`) | No chart | There is nothing to draw |
| The Printable Invoice's page (v0.6) | No title, and one `div.main-table` holding a two-column table (`standard/invoice.scm:797-869`) whose entries table has rows of different widths: *n* cells for an entry, 1 + max(3, *n*) for a total, max(3, *n*) for a payment (`:601-606`, `:670-678`) | An empty title; the main table's rows as blocks in order: a heading of class `invoice-title`, a group per layout row (the right-hand block's class starting `div-align-right`), the entries table and the notes. The entries table declares 1 + max(3, *n*) columns, the shown ones headed, and pads shorter rows with empty cells at the end | The report result has no layout tables and every row covers its table; the figures sit in the columns GnuCash's browser puts them in |
| An invoice option in the options summary (v0.6) | `#<swig-pointer …>`, different at every run: `disp` has no case for an invoice (`report/html-utilities.scm:302-323`) | The document's GUID | The same bytes for the same run |
| The invoice's picture (v0.6) | `<img src=…>` with the address quoted as a Scheme string (`report/html-text.scm:227-231`) | The address HTML-escaped | Every text on a page is HTML-escaped (§8) |
| An invoice whose owner is not a job, with Job Details on (v0.6) | `gncJobGetID` of the null job `gncOwnerGetJob` gives (`standard/invoice.scm:238-239`; `5.17:libgnucash/engine/gncOwner.c:376-381`); GnuCash's own test shows no job rows for such documents (`standard/test/test-invoice.scm:85-90`) | No job, so no job rows | As GnuCash's test runs show |
| `ctx.print_time` with a time directive (v0.6) | `gnc-print-time64` prints any `strftime` directive (`5.17:libgnucash/engine/gnc-date.cpp:370-385`) | Date directives only; any other is printed as written | Reports print dates: the Book's Fancy Date Format, which the invoice reads, is built from day, month and year (`5.17:gnucash/gnome-utils/gnc-date-format.c:458-461`) |

## 15. Importing GnuCash's report files, and what is left for later

### Importing saved configurations and style sheets (v0.3)

GnuCash keeps saved report configurations in `saved-reports-2.8`, or `saved-reports-2.4` from older versions
(`5.17:gnucash/report/gnc-report.h:35-36`), and style sheets in `stylesheets-2.0`
(`5.17:gnucash/report/html-style-sheet.scm:142-143`), and evaluates them at start-up. They are Scheme programs, but
GnuCash writes them from a few fixed forms with literal values. `report.importConfigurations` and
`report.importStyleSheets` ([`api/reports.md`](api/reports.md)) read those forms as data, as an anti-corruption layer
(`crates/red-ale-report/src/import/`): nothing in them is evaluated, called or looked up.

- **The reader** (`import/sexp.rs`) reads what Guile's `write` prints for the values GnuCash saves: lists and dotted
  pairs; strings with Guile's escapes (`\n \t \" \\ \a \b \v \f \r \0 \xHH \uHHHH \UHHHHHH`, and a
  backslash before a newline); plain symbols and `#{…}#` symbols; integers, decimals, exponents and `n/d`, held
  exactly; `#t`, `#f`, `#true`, `#false`; `'` for quote; `;` comments. Everything else is an error: vectors,
  characters, `#|…|#` comments, quasiquote and unquote, `+inf.0` and `+nan.0`, nesting deeper than 64. Text the
  reader cannot read ends the file with one `malformed` entry; the entries before it are kept.
- **The forms** (`import/forms.rs`) are matched exactly: a saved configuration as
  `gnc:report-template-serialize-internal` writes it (`5.17:gnucash/report/report-core.scm:620-643`, the same since
  2.6), a style sheet as `gnc:save-style-sheet-options` does (`html-style-sheet.scm:145-166`), and each option as
  `gnc:generate-restore-forms` does (`5.17:bindings/guile/options.scm:130-172`), with the 5.17 setter
  (`gnc:option-set-value`) or the 2.x one (`(gnc:option-setter option)`). Any other form is `malformed` for its entry.
- **The values** (`import/values.rs`) are turned into the wire form of the option they set (section 6) and applied as
  a client's would be, so the same checks hold. They are those `save_scm_value` writes
  (`5.17:bindings/guile/gnc-optiondb.i:1216-1342`) and those older versions wrote:

| Option | Written as | Read as |
|--------|------------|---------|
| boolean | `#t`, `#f` | itself |
| string, text, font | `"…"` | itself |
| colour | `'(r g b a)`, components `0.0`–`255.0` | `#rrggbb`, each component truncated as GnuCash does; `#rrggbbaa` unless the alpha is `ff` |
| multichoice | `'symbol`, `'"string"` or `'3` | the choice with that key |
| list | `'(a b)`, `'#f` with none | the choices' keys |
| number range | `'2.0` (5.17), `2` (4.x) | the exact number |
| plot size | `'(percent . 50)`, `'(pixels . 500)` | the number; above 100 is pixels, as GnuCash decides (`gnc-optiondb.i:1376-1384`) |
| date | `'(absolute . <seconds>)`, 2.x `'(absolute <seconds> . <ns>)`; `'(relative . <name>)` | the absolute time, or the relative date. Both spellings of the accounting period's (`start-prev-fin-year`, which 5.17 writes, and `start-accounting-period`) are accepted |
| account list, account | `'("<guid>" …)`, `'""` when empty; `"<guid>"` | the GUIDs, kept whether or not the open book has them |
| currency | `"USD"` (5.17), `'(commodity-scm "CURRENCY" "USD")` (4.x; `ISO4217` read as `CURRENCY`) | the currency |
| commodity | `'(commodity-scm "<namespace>" "<mnemonic>")` | the commodity |
| budget (v0.4) | `(gnc-budget-lookup "<guid>" (gnc-get-current-book))` (`5.17:bindings/guile/options.scm:150-160`), matched exactly, never called | the GUID, kept whether or not the open book has it |

  A value of any other shape for its option is `malformed`; one of the right shape that the option refuses is
  `invalid-option`.
- **A saved configuration** keeps GnuCash's GUID and name and configures the template its `'parent-type` names. It
  stores only the options that differ from the template's defaults, as GnuCash saves (`options.scm:141`), with
  defaults computed without a Book so the result does not depend on the book open. An option the template does not
  have is reported (`unknown-option`) and left out, as GnuCash's `(if o …)` leaves it. A template red-ale has not
  translated is reported (`untranslated`) and the entry skipped; importing it again once the template exists works.
  A General/"Stylesheet" naming a style sheet red-ale does not have is kept (section 8).
- **A saved Multicolumn View** (v0.5) holds its children as embedded reports (`gnc:report-serialize-embedded`,
  `5.17:gnucash/report/report-core.scm:567-618`): among its option forms, a `__general`/`report-list` of the saving
  session's report instance ids with each child's spans, `'((<id> <wide> <high> #f) …)`; then, for each child in
  placement order, a `(let ((options (gnc:report-template-new-options/report-guid "<template>" "<name>"))) <option
  forms> (set! new-embedded-report-ids (append new-embedded-report-ids (list
  (gnc:restore-report-by-guid-with-custom-template #f "<template>" "<name>" "<custom template>" options)))))` form;
  then one form putting the new instances' ids in place of the saved ones, written even with no children. All three
  are matched exactly as data (`import/forms.rs`), with 5.17's last form or 3.x and 4.x's
  (`4.14:gnucash/report/report-core.scm:586-600`). Each embedded report becomes a child `{ report: <its template>,
  options, colspan, rowspan }`, its spans from the report-list at its position: the copy of the options GnuCash makes
  when a child is added (`:389-409`), made explicit. The custom template, the saved configuration the copy was made
  from, is dropped: the options are complete without it. A child's options are imported as an entry's are
  (`unknown-option` in `ignored` with the child's position, a value red-ale refuses `invalid-option`), then checked as
  `report.saveConfiguration` checks a child. A report-list and embedded reports that differ in number, or any other
  form, are `malformed`. An embedded report whose template red-ale has not translated skips the whole entry as
  `untranslated`, naming the child, so importing again once it is translated adds the view whole
  (`notes/report-statements-charts.md`, Q5). Only a view's own children are saved: a child that is itself a view
  keeps a report-list of instance ids and no embedded reports, which GnuCash reloads as unknown or other reports. It
  imports with no children, and its report-list is listed in `ignored` as `unsaved-children`.
- **A style sheet** naming a predefined one holds the user's changes to it; any other name is a user style sheet.
  The image options red-ale does not render are kept (`not-rendered`), as `report.createStyleSheet` keeps them.
- **Nothing is overwritten.** An entry whose GUID (configurations) or name (style sheets) the Configuration already
  has is skipped as `exists`, so a second import adds nothing and edits made in red-ale survive; a configuration
  whose name another has is `conflict`. Everything imported is written in one atomic save; `dryRun` writes nothing.

Tests: `crates/red-ale-report/src/import/` (the reader, with property tests that no input panics; every value form)
and `crates/red-ale-rpc/tests/report_import.rs` (hand-written files in 5.17 and older forms, skips, nothing
evaluated, and an imported configuration running as the same options set by hand; a saved Multicolumn View with a
template child, one copied from a saved configuration and a nested view, running cell for cell as its children
alone, and its untranslated, malformed and refused children).

### Left for later

- **Comparing with GnuCash's report output** through `gnucash-cli --report run` in a GUI build, as
  `open-questions.md` defers.
- **Interactive charts**, should anyone want them (`open-questions.md`, "Charts are static SVG").
