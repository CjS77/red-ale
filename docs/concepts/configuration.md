<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: concept/configuration
title: The Configuration
summary: red-ale's own settings file, for what GnuCash keeps in preferences and user files, against Book options, which live in the book. Where it is, how to read and change a Setting by path, secrets, and the settings you are likely to want.
related: concept/books-and-files, concept/closing-the-books-and-accounting-period, concept/report-scripts, method/config.get, method/config.set, method/config.schema, method/book.options, method/book.setOptions
from: manual/ch_Customize.docbook#book-options
---
GnuCash keeps settings in two places. **Preferences** (Edit > Preferences) belong to the person using GnuCash and are kept
outside the book. **Book options** (File > Properties) belong to the book and travel with it. red-ale keeps the same split.

| | Kept in | Examples | Methods |
|---|---|---|---|
| The **Configuration** | red-ale's own JSON settings file | Accounting period, report currency, import settings, quote services, saved report configurations, the report directory | `config.get`, `config.set`, `config.schema`, and the shortcuts `period.*` and `docLink.setPathHead` |
| **Book options** | The Book, as Slots GnuCash reads | Use trading accounts, the auto read-only threshold, the split-action number field, the default budget, business details | `book.options`, `book.setOptions`, `counter.*` |

**⚠** Do not call the Configuration a "preference file of the book": it never goes into a book. A preference that GnuCash
holds, such as the accounting period, is in red-ale's Configuration. GUI-only preferences (window layout, register look,
the Tip of the Day) have nothing to configure.

## Where it is

Every red-ale process reads one Configuration file:

| Started with | Configuration |
|---|---|
| neither option | The default file, shared by every process started this way: `~/.config/red-ale/config.json` on Linux (`$XDG_CONFIG_HOME/red-ale/config.json` if set), `~/Library/Application Support/red-ale/config.json` on macOS |
| `--config <file>` | That file, for this process |
| `--no-config` | Memory only, for this process: nothing is read or written |

The file is read at every use, so a change made by hand or by another process takes effect at once. A change takes a lock
on `<file>.lock` while it reads and rewrites the file, so two processes do not lose each other's changes; the file is
written whole and renamed into place, and readable by its owner only.

## Settings

A **Setting** is one value of the Configuration, named by its **setting path**, the keys joined with `.`:
`accountingPeriod.start`, `import.settings.matchThreshold`, `quotes.services.alphavantage.key`. Paths are typed on command
lines, so there is no escaping.

- [`config.schema`](method/config.schema) returns the JSON Schema of every setting: its type, default and description.
  `format`s such as `period-bound`, `commodity-id` and `decimal` are checked.
- [`config.get`](method/config.get) reads one setting by `path`, or the whole Configuration with defaults filled in. It
  works with no book open. It also lists `unknown` keys in the file (typos, kept and ignored) and `problems` with values
  that do not match the schema.
- [`config.set`](method/config.set) changes one or more by path, validated against the schema, **all or none**. Setting a
  path to `null` restores its default.

```example book=none
{"method": "config.get", "params": {"path": "accountingPeriod.start"}}
{"result": {"path": "accountingPeriod.start", "value": "year", "unknown": [], "problems": [], "configFile": null}}
```

Some Settings are owned by other methods: saved report configurations (`reports.saved`) and style sheets
(`reports.styleSheets`) change only through the `report.*` methods, so `config.set` refuses them.

### Secrets and the network

A **secret**, such as a quote service's API key, is never shown: `config.get` says only `{"set": true}` or
`{"set": false}`, and no key appears in a result, error, log or journal. Settings that only a person may change are
refused through MCP: `quotes.network`, which lets quote fetches reach the network (off by default, and the only network
red-ale uses), and each quote service's `key` and `keyEnv`. Changing them over JSON-RPC, the CLI or by editing the file
is allowed. A key is also tied to its service's host: pointing a service at a new host clears its key.

## Settings you are likely to want

| Setting | Meaning |
|---|---|
| `accountingPeriod.start`, `.end`, `.fiscalYearEnd` | The accounting period ([Closing the books and the accounting period](concept/closing-the-books-and-accounting-period)) |
| `reports.directory` | Where your own report scripts are ([Report scripts](concept/report-scripts)) |
| `reports.currency`, `reports.dateFormat`, `reports.weekStart`, `reports.reverseBalancedAccounts`, `reports.limits.*` | What reports read where GnuCash reads a preference |
| `locale` | Number, date and currency conventions (`C` by default, or a POSIX name such as `fr_FR`) |
| `defaultCurrency` | GnuCash's default currency: what currencies are quoted in |
| `docLinks.pathHead` | The folder relative document links start from |
| `import.settings.*`, `import.csvLayouts.*`, `import.qifAccountMap` | The matcher's thresholds and saved layouts ([Importing and matching](concept/importing-and-matching)) |
| `quotes.network`, `quotes.services.*`, `quotes.sources.*` | Online quotes ([Prices and exchange rates](concept/prices-and-exchange-rates)) |

## Book options

[`book.options`](method/book.options) reads the options kept in the book, and [`book.setOptions`](method/book.setOptions)
changes them as GnuCash's Book Options dialog does. An absent option is left alone.

| Option | Meaning |
|---|---|
| `useTradingAccounts` | Keep multi-currency Transactions balanced through trading Accounts ([Trading accounts](concept/trading-accounts)) |
| `splitActionForNum` | Use a Split's action field as its number. Turning it on adds the Book feature `Number Field Source` |
| `autoReadOnlyDays` | A number of days; Transactions older than that become read-only (0 turns it off) |
| `defaultBudget` | The Budget reports start from ([Budgets](concept/budgets)) |
| `companyName`, `companyAddress`, `companyContactPerson`, `companyPhoneNumber`, `companyFaxNumber`, `companyEmailAddress`, `companyWebsiteUrl`, `companyId`, `taxNumber` | The business details the Printable Invoice prints |
| `defaultInvoiceReport` | The invoice report Print Invoice runs |

Document numbers are kept separately: **counters** hold the last number issued for each kind of document or owner
(`gncInvoice`, `gncCustomer`, ...), with a format such as `%.6li`. [`counter.list`](method/counter.list) and
[`counter.set`](method/counter.set) read and change them. **⚠** GnuCash's book options call a counter the *previous*
number: the next ID is one more.
