<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: concept/report-scripts
title: Report scripts
summary: User reports - how to write your own report as a .rhai script in the Configuration's report directory. The shape of a script (templates, options, render), option types, the query API's main reads, exact Rational arithmetic, the report result's blocks, limits, library modules and load errors.
related: concept/reports, concept/configuration, concept/business-documents, method/report.list, method/report.options, method/report.run, method/report.export, method/config.set
from: manual/ch_Reports.docbook#report-create
---
red-ale ports the reports GnuCash itself relies on and leaves the rest for you to write: the business and tax reports,
the Easy and Fancy invoices, and anything particular to your own books. A report you write is a **User report**: a
**Report script**, a `.rhai` file, that defines one or more report templates. Rhai is a small scripting language that
looks like Rust. red-ale never runs Guile, GnuCash's Scheme; its own reports are GnuCash's Scheme translated into Rhai,
and they serve as worked examples (below).

**⚠** A User report is not GnuCash's "custom report", which is a [saved configuration](concept/reports) of an existing
template.

A script reads the book only through a read-only query API. It cannot change the book, open a file, reach the network
or read the clock. No method takes a script's text: **the script is a file on the machine where red-ale runs**, so an
agent that writes reports needs file access there.

## Where scripts live

Set the **report directory** in the Configuration:

```
red-ale exec config.set 'changes:={"reports.directory": "/home/me/reports"}'
```

`reports.directory` is relative to the Configuration file if it is not absolute (see
[The Configuration](concept/configuration)). red-ale reads every `*.rhai` file below it, at most 1,000 files of at most
1 MiB each, each time a report method runs, so edits show without a restart. Symbolic links that lead outside the
directory are ignored. A script with no `templates()` function is a library module that other scripts import.

[`report.list`](method/report.list) lists your templates with `kind` `user` and the script that defines them
(`./mine/report.rhai`). A script that does not load, for example because it does not compile or repeats a GUID, defines no
template; `report.list` gives its error, with the file, line and column, in `problems` (error
[`report-load`](error/report-load)), so one bad file does not hide the others. Naming one of its templates is `not-found`.

## The shape of a script

A script is three functions:

```rhai
fn templates() {
    [#{ guid: "0f2d5e3a9b1c4d6e8a7b3c5d9e1f2a4b", name: "Bank balances", menuPath: ["_Assets & Liabilities"] }]
}

fn options(params, o, book) {
    o.date("General", "Balance Date", "a", "Balances at the end of this day.", "today", #{});
}

fn render(params, book, opts, ctx) {
    let at = ctx.end_of_day(opts.get("General", "Balance Date"));
    let rows = [];
    for a in book.accounts_of_types(["BANK"]) {
        let balance = monetary(a.commodity, a.balance_as_of(at + 1));
        rows.push(#{ account: a.guid, cells: [#{ text: a.fullName, account: a.guid }, #{ monetary: balance }] });
    }
    #{
        title: "Bank balances",
        sections: [#{ blocks: [#{
            kind: "table",
            columns: [#{ label: "Account", role: "label" }, #{ label: "Balance", role: "amount" }],
            rows: rows
        }] }]
    }
}
```

- `templates()` returns the template records: a `guid` of 32 lowercase hex digits (make your own and keep it: saved
  configurations refer to it), a `name`, and optionally `menuPath`, `menuName`, `menuTip`, `inMenu`, `exportTypes` and
  `params`, values that tell templates of one script apart.
- `options(params, o, book)` declares the options through the builder `o`. `book` is the open book, or an empty one when
  none is open, so defaults can be computed from it.
- `render(params, book, opts, ctx)` returns the report result as a map. `opts.get(section, name)` is an option's value,
  with dates already as times; `ctx` gives time and formatting.

red-ale adds GnuCash's three General options (*Report name*, *Stylesheet*, *Add options summary*) to every template.
A misspelt variable or map key is an error, and a function may not have the name of an engine function of the same arity.

## Options

`o.boolean`, `o.string`, `o.text`, `o.font`, `o.color`, `o.multichoice`, `o.list`, `o.number_range`, `o.plot_size`, `o.date`,
`o.start_date` and `o.end_date`, `o.account_list`, `o.account_sel`, `o.currency`, `o.commodity`, `o.budget`, `o.internal`,
and two for the Printable Invoice: **`o.invoice`**, an option whose value is a business document's GUID (a script sees an
Invoice, or `()` for none) and **`o.image`**, a picture's file name that the HTML refers to but the server never reads. Each
takes a section, a name, a sort key, a help text and a default. The wire forms are those of `report.options`
([Reports](concept/reports)). The built-in library module `lib/options` has the common ones GnuCash's reports share, such
as the report date, the date interval and the account levels.

## What a script can read

All handles are read-only, and lists come back in a fixed order, never in hash order.

- **Book**: `root()`, `account(ref)`, `accounts()`, `accounts_of_types(types)`, `commodity(id)`, `prices(...)`,
  `price_rate(...)`, `price_convert(...)`, `split_query(q)` for splits by account and date, budgets and, from v0.6,
  **`book.option("Business/Company Name")`**, which reads an option stored in the book, and `book.invoice(guid)`.
- **Account**: names, type, commodity, `children()`, `descendants_sorted()`, `splits()`, `splits_between(from, to)`,
  `balance_as_of(t)`.
- **Split** and **Transaction**: amount, value, memo, action, reconcile state, dates, description, `splits()`.
- **Commodity** and **Price**.
- **Invoice**, **Entry**, **Owner**, **Order** and **BillTerm** (v0.6), the reads the Printable Invoice uses: a document's
  type, owner and addresses, terms, dates, Entries (quantity, price, discount, taxes), totals, posted Transaction and Lot.
  Each figure is exact and computed by red-ale; a script never recomputes one.
- **ctx**: `now`, `today()`, `start_of_day(t)`, `end_of_day(t)`, `local(t)`, `print_date(t)`, `print_time(t, format)`
  (date directives only, `%d %m %Y %B ...`) and the report settings (`report_currency()`, `week_start()`,
  `accounting_period()`). The clock is the run's `now`, not the machine's.

## Exact arithmetic

Money in reports is a **Rational**, an exact fraction of any size. `rational("12.34")` and `rational(1234, 100)` make one;
`+ - * /` and the comparisons are exact. There is no float: a float literal is a syntax error. A **Monetary** is a
Rational with a Commodity (`monetary(commodity, r)`); adding two Commodities is an error. Round only to show a figure:
`r.round_to(denom, mode)`, `m.round_scu()`, `r.to_decimal(places)`. Dividing by zero is a `report-script` error.

## The report result

`render` returns a map with a `title` and `sections`, each with `blocks`. The kinds are `text` (with a `role` of `paragraph`,
`heading` or `note`), `warning`, `table`, `chart`, `list`, `group` (blocks side by side), `grid` (a Multicolumn View's cells)
and, from v0.6, **`image`** (a `src` starting `file:///`). A table has `columns` and `rows` whose `cells` are one of
`text`, `monetary`, `commodities`, `number`, `price`, `date` or `anchors`. From v0.6:

- **`class`** on any block or cell, one or more CSS class names for the report's own style, and **`css`** on the result,
  the CSS itself (red-ale leaves out text its CSS template would refuse and logs why);
- **`numberFormat: "exact"`** on a `number` cell, to print the figure unrounded.

The host checks the result and fills in display text, so a malformed result is `report-result` and names the path
(`sections[0].blocks[0].text`). Whatever the script `print`s or `debug`s is returned in the result's `log`.

## Limits

A script runs under limits that make a runaway script an error, not a hung server: 1,000,000,000 operations, 120
seconds, call depth 64, a string of 1 MiB, 10,000,000 array items, and so on. Change them under `reports.limits.*` in the
Configuration. Passing one is [`report-limit`](error/report-limit); a script error is
[`report-script`](error/report-script); a bad option value, [`report-option`](error/report-option). A tree of reports that
embed reports shares one time limit.

## Using the library

`import "lib/exchange" as exchange;` loads a built-in module (`lib/accounts`, `lib/balances`, `lib/dates`, `lib/options`,
`lib/exchange`, `lib/html-utilities` and others). `import "standard/<name>"` and `import "example/<name>"` load a built-in
report. `import "./other"` loads another script of yours, inside the report directory; `..` and absolute paths are refused.
No two scripts in a run may define a function with the same name and number of parameters.

## Worked examples

The built-in reports are ordinary scripts, translated line by line from GnuCash's Scheme and commented with where each
line comes from. In red-ale's source they are in `crates/red-ale-report/scripts/`:

- `standard/account-summary.rhai`, `standard/balance-sheet.rhai`: tables of accounts and balances;
- `standard/transaction.rhai`: filters, sorting and subtotals;
- `standard/invoice.rhai`: Invoices, Entries, options of type `invoice` and `image`, `class` and `css`;
- `example/average-balance.rhai`, `example/daily-reports.rhai`: GnuCash's own example reports, including charts;
- `lib/*.rhai`: the shared modules.

Run yours with [`report.run`](method/report.run), render it with [`report.export`](method/report.export), and save a set of
options with `report.saveConfiguration`.
