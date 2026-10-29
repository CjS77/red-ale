<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: guide/write-your-own-report
title: Write your own report
summary: Write a User report as a .rhai script in the report directory, check that report.list shows it, then run it with report.run and export it with report.export.
related: method/config.set, method/config.get, method/report.list, method/report.run, method/report.export, method/report.options, error/report-script, error/report-load, concept/report-scripts, gnucash/reports-menu
from: guide/ch_reports.docbook#rpt_grp_sampcust
---
GnuCash comes with reports of its own, and red-ale has translated most of them. Others, such as GnuCash's business reports
and the sample reports you may have written for it, are left for you to write. A **User report** is a Rhai script, a file
ending in `.rhai` in the **report directory**. red-ale loads it each time a report method runs, lists it beside the
built-in reports, and runs it in a sandbox: it can read the open book but cannot change it, read other files, reach the
network or ask the clock.

You can write one yourself, or ask an AI agent to. Either way the steps are the same, and the script is an ordinary text
file. There is no method that takes a script's text, so the file must be put in the report directory by something that can
write files on the machine where red-ale runs: you, your editor, or an agent with file access.

## Steps

1. **Choose the report directory** and tell red-ale where it is, with [config.set](method/config.set):
   `{"changes": {"reports.directory": "/home/me/red-ale-reports"}}`. [config.get](method/config.get) with the path
   `reports.directory` reads it back. Scripts may be in sub-directories; at most 1,000 files of at most 1 MiB are read.
2. **Write the script.** Save the one below as `asset-balances.rhai` in that directory. A script defines three functions:
   `templates()` names the reports it provides, `options()` declares the options a user can set, and `render()` builds the
   report from the book.
3. **Check that it loaded.** Call [report.list](method/report.list). The new report is in `reports` with `"kind": "user"`
   and its script's path. If the script has a mistake, it is not listed; instead `problems` names the file, line and
   column and says what is wrong ([report-load](error/report-load)).
4. **Run it** with [report.run](method/report.run), by name. You get the report result as data, with exact figures. A fault
   at run time is a [report-script](error/report-script) error with the line and column. Edit the file and run it again:
   there is nothing to restart.
5. **Export it** with [report.export](method/report.export) for a page you can read or print.
6. **Share the options.** An option you declare shows up in [report.options](method/report.options) and can be set in
   `options`, like those of the built-in reports. Save a favourite set with `report.saveConfiguration`
   ([Run and export a report](guide/run-and-export-a-report)).

## A minimal script

This report lists every asset account that can hold transactions, with its balance now:

```rhai
// Asset Balances: every asset account that can hold transactions, with its balance now.

fn templates() {
    [#{ guid: "5a1e0c9d2b7a4e3f8a5b1c2d3e4f5a6b", name: "Asset Balances", menuPath: ["_Custom"] }]
}

fn options(params, o, book) {
    o.string("General", "Report Title", "a", "Title for this report.", "Asset Balances");
}

fn render(params, book, opts, ctx) {
    let rows = book.accounts()
        .filter(|a| a.type == "ASSET" && !a.placeholder)
        .map(|a| #{ cells: [#{ text: a.fullName }, #{ number: a.balance_as_of(ctx.now), places: 2 }] });

    #{
        title: opts.get("General", "Report Title"),
        sections: [#{ blocks: [#{
            kind: "table",
            columns: [#{ label: "Account", role: "label" }, #{ label: "Balance", role: "amount" }],
            rows: rows
        }] }]
    }
}
```

- The `guid` is any 32 lowercase hex digits you make up, and it must be unique among the reports. Keep it when you edit
  the script, so saved configurations still find it.
- `book.accounts()` returns the book's accounts. `a.balance_as_of(ctx.now)` is an exact rational, and the `number` cell
  shows it to two places. Money is never a floating-point number in a script.
- `render()` returns a map of the report result: a `title` and `sections` of `blocks` (tables, text, charts).

Running it on a book with a bank account holding 2,186.00 gives a table whose rows look like this (shown, not run, since
an Example cannot put a file in the directory):

```json
{"role": "data", "cells": [{"text": "Root:Asset:Bank"}, {"number": {"num": "2186", "denom": "1"}, "places": 2, "display": "2,186.00"}]}
```

The engine's whole interface, with the reads a script can make, the option types, the arithmetic and the limits, is on
the Report scripts concept page. The built-in reports are written the same way and are worked examples.

## Example

A directory that does not exist yet is reported under `problems`. Setting the directory and listing the reports works on any book. A script in the directory would appear in this list:
`"kind": "builtin"` marks the reports that come with red-ale, and a User report has `"kind": "user"`.

```example fixture=reports/test-data now=2024-03-15T12:00:00Z
{"method": "config.set", "params": {"changes": {"reports.directory": "reports"}}}
{"result": {"values": {"reports.directory": "reports"}, "clearedKeys": [], "configFile": null}}
{"method": "report.list", "params": {}}
{"result": {"reports": [{"name": "Account Summary", "kind": "builtin", "…": "…"}, "…"], "problems": "…"}}
{"method": "report.run", "params": {"report": "Account Summary"}}
{"result": {"title": "…", "report": {"name": "Account Summary", "…": "…"}, "sections": "…", "…": "…"}}
{"method": "report.export", "params": {"report": "Account Summary"}}
{"result": {"title": "…", "html": "…"}}
```
