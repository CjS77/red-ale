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
| `engine`   | The Rhai engine for a run, its limits and module resolver; the template `Registry`; running a template; `Memo` |
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

## Shared Rhai modules

`scripts/lib/` translates the Scheme helpers GnuCash's reports share (`design/reports.md` §12). Import them as
`import "lib/exchange" as exchange;`. Names are GnuCash's with `gnc:` dropped and `-` turned into `_`; arguments
keep GnuCash's order, with the Book (and the run context `ctx` where GnuCash reads preferences) added first.

| Module | Exports |
|--------|---------|
| `lib/collectors` | `make_value_collector`, `make_commodity_collector` (an array with the methods `add`, `merge`, `minusmerge`, `format`, `getpair`, `getmonetary`, `getmonetary_strict`, `reset`, `remove_zeros`), `commodity_collector_get_negated`, `commodity_collector_allzero`, `collector_plus`, `collector_minus`, `monetaries_add`, `monetary_plus`, `not_all_zeros`, `sum_collector_commodity`, `uniform_commodity` |
| `lib/accounts` | `account_has_shares`, `account_is_stock`, `account_full_name_lt`, `accounts_get_children_depth`, `account_is_inc_exp`, `filter_accountlist_type`, `decompose_accountlist`, `account_get_type_string_plural`, `accounts_get_commodities(_sorted)`, `get_current_account_tree_depth`, `accounts_and_all_descendants` |
| `lib/balances` | `account_get_balances_at_dates`, `account_accumulate_at_dates`, `account_get_comm_balance_at_date`, `account_get_comm_value_interval`, `account_get_comm_value_at_date`, `accounts_get_balance_helper`, `accounts_get_comm_total_assets`, `account_get_balance_interval`, `account_get_comm_balance_interval`, `accountlist_get_comm_balance_interval(_with_closing)`, `accountlist_get_comm_balance_at_date(_with_closing)`, `split_voided`, `accounts_count_splits`, `account_get_trans_type_balance_interval(_with_closing)`, `account_get_trans_type_splits_interval`, `get_assoc_account_balances`, `select_assoc_account_balance`, `get_assoc_account_balances_total` |
| `lib/exchange` | the commodity splits and price lists (`get_match_commodity_splits(_sorted)`, `get_all_commodity_splits`, `get_commodity_totalavg_prices(_internal)`, `get_commoditylist_totalavg_prices`, `get_commodity_inst_prices`, `pricelist_price_find_nearest`), the exchange tables (`resolve_unknown_comm`, `get_exchange_totals`, `get_exchange_cost_totals`, `make_exchange_alist`, `make_exchange_cost_alist`), the exchanges (`exchange_by_euro(_numeric)`, `exchange_if_same`, `make_exchange_function`, `exchange_by_pricedb_latest`, `_nearest`, `_nearest_before`, `exchange_by_pricealist_nearest`), `case_exchange_fn`, `case_price_fn`, `case_exchange_time_fn`, and `apply_exchange_fn`, `apply_price_fn`, `apply_exchange_time_fn` |
| `lib/dates` | `leap_year`, `days_in_year`, `days_in_month`, `date_to_{year,halfyear,quarter,month,twoweek,week,day}_fraction`, `date_get_fraction_func`, `apply_fraction_func`, `moddate`, `incdate`, `decdate`, `incdate_months`, `make_date_list`, the constants `SecDelta` … `NinetyDayDelta`, `deltasym_to_delta`, `time64_start_day_time`, `time64_end_day_time` |
| `lib/options` | `add_report_date`, `add_date_interval`, `add_interval_choice`, `add_account_levels`, `add_account_selection`, `add_currency`, `add_price_source`, `add_plot_size`, `add_marker_choice`, `add_sort_method`, `add_subtotal_view` |
| `lib/acct-table` | `make_html_acct_table_env_accts`, `html_acct_table_add_accounts`, `num_rows`, `get_row_env`, `add_labeled_amount_line`, `gnc_commodity_table`, `add_account_balances` (result rows: role, account, depth, level, cells) |
| `lib/html-utilities` | `assign_colors`, `make_rates_table` (a table block), `make_generic_warning`, `make_generic_simple_warning`, `make_generic_budget_warning`, `make_generic_options_warning`, `make_no_account_warning`, `make_empty_data_warning` (warning blocks) |

Conventions a translator needs:

- **Function values are maps.** A closure made inside an imported module cannot be called after it is returned, so
  what GnuCash returns as a lambda (`case-exchange-fn`, `case-price-fn`, `case-exchange-time-fn`,
  `date-get-fraction-func`) is a map, called with `exchange::apply_exchange_fn(fx, foreign, domestic)`,
  `apply_price_fn(pf, commodity)`, `apply_exchange_time_fn(fx, foreign, domestic, date)` or
  `dates::apply_fraction_func(f, t)`. These, and every function that takes one, also accept a closure of the
  caller's.
- **Collectors change in place**: `c.add(commodity, amount)`, `c.merge(other)`. They are association-list arrays, so
  a new commodity goes to the front and merging reverses order, as in GnuCash.
- **Keyword arguments, transaction types and the account table's environment are maps** with GnuCash's keys
  (`#{ str: "Closing Entries", closing: true }`, `#{ display_tree_depth: 2, depth_limit_behavior: "flatten" }`);
  a missing key, `()` and false read as #f. Symbols are strings.
- **No closures inside a module's methods.** Rhai caches a method's resolution for the run; called from another
  module after its own module called it, the method runs without its module's environment, and a closure made in
  it is "not found" (`anon$…`). lib/collectors' methods loop for this reason (`tests/lib_collectors.rs`).
- `memo()` makes a `Memo`, a table from integers to values shared by its copies; `case_exchange_time_fn` keeps its
  daily price tables in one.

## Adding a built-in script

Put the file below `scripts/`, in `standard/` or `example/` for a report and `lib/` for a shared module, as GnuCash
lays them out, and add its path to `BUILTIN` in `src/library/mod.rs`:

```rust
const BUILTIN: &[(&str, &str)] = scripts!["lib/dates.rhai", "standard/balance-sheet.rhai"];
```

A built-in template with a GnuCash name or GUID must have both, as `GNUCASH_TEMPLATES` lists them; a test checks it.

Regenerate the result schema snapshot after a deliberate change with
`UPDATE_SNAPSHOTS=1 cargo test -p red-ale-report --test result_schema`, and review the diff.
