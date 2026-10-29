<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: guide/configure-red-ale
title: Configure red-ale
summary: Change red-ale's own settings (accounting period, report directory, network) with config.set, and the settings stored in the book with book.setOptions.
related: method/config.get, method/config.set, method/config.schema, method/book.options, method/book.setOptions, method/period.get, method/period.set, method/period.dates, error/invalid-params
---
There are two places where settings live, and it matters which you change.

- **The Configuration** belongs to you and your machine: the accounting period that reports default to, the folder of your
  User reports, whether the network may be used for online quotes, import settings and limits. It is a JSON file (by
  default `~/.config/red-ale/config.json`), shared by every book you open. This is what GnuCash calls *preferences*.
- **Book options** belong to the book and travel with it: the company's name and details, whether to use trading accounts,
  the auto read-only threshold, which source the number field uses, the default budget. GnuCash stores them in the file,
  so GnuCash sees them too.

## Steps

1. **See what can be set** with [config.schema](method/config.schema): every setting, its type, default and
   description. [config.get](method/config.get) returns the current values; give a `path` such as `quotes.network` for one.
   Secrets (such as quote service keys) are shown only as set or unset.
2. **Change settings** with [config.set](method/config.set): `changes` maps a path to its new value, and `null` restores
   a default. The whole call is checked first: a bad path or value changes nothing and the error names it
   ([invalid-params](error/invalid-params)). Neither call needs a book to be open. Over MCP the network setting cannot be
   changed.
3. **Set the accounting period** with [period.set](method/period.set): `start` and `end`, each a relative choice (`year`,
   `previous-month`, `fiscal-year`, ...) or a date, and `fiscalYearEnd` as `MM-DD` for the fiscal-year choices.
   [period.get](method/period.get) reads it, and [period.dates](method/period.dates) shows the first and last day it
   works out to for today, or a date you give. Reports use it for relative dates.
4. **Point to your report scripts** with `reports.directory` ([Write your own report](guide/write-your-own-report)).
5. **Allow the network** only if you want online quotes ([Record prices and commodities](guide/record-prices)):
   `{"changes": {"quotes.network": true}}`.
6. **Change book options** with [book.setOptions](method/book.setOptions) and read them with
   [book.options](method/book.options). Set only the ones you want to change. For the business options, see
   [Set up the business features](guide/set-up-business). The changes are made in memory and kept when you save the book.
7. **Use another file for a run** with `red-ale --config <file>` or `--no-config` on the command line, which keeps the
   settings in memory for that process only.

## Example

```example fixture=reports/test-data now=2024-07-01T12:00:00Z
{"method": "config.schema", "params": {}}
{"result": {"title": "red-ale Configuration", "type": "object", "properties": {"…": "…"}, "…": "…"}}
{"method": "config.set", "params": {"changes": {"quotes.network": true}}}
{"result": {"values": {"quotes.network": true}, "clearedKeys": [], "configFile": null}}
{"method": "config.get", "params": {"path": "quotes.network"}}
{"result": {"path": "quotes.network", "value": true, "…": "…"}}
{"method": "period.set", "params": {"start": "fiscal-year", "end": "fiscal-year", "fiscalYearEnd": "03-31"}}
{"result": {"start": "fiscal-year", "end": "fiscal-year", "fiscalYearEnd": "03-31", "…": "…"}}
{"method": "book.setOptions", "params": {"autoReadOnlyDays": 30}}
{"result": {"autoReadOnlyDays": 30, "readOnlyThreshold": "2024-06-01", "…": "…"}}
{"method": "book.options", "params": {}}
{"result": {"autoReadOnlyDays": 30, "useTradingAccounts": false, "…": "…"}}
```
