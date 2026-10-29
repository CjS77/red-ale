<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: gnucash/preferences-and-book-options
title: Preferences and book options
summary: GnuCash's Edit > Preferences become Configuration settings read with config.get and changed with config.set; File > Properties (Book Options) become book.options and book.setOptions, with counters in counter.list and counter.set. Preferences about windows, registers and printing have no counterpart.
related: concept/configuration, concept/closing-the-books-and-accounting-period, concept/importing-and-matching, concept/prices-and-exchange-rates, gnucash/file-menu, method/config.get, method/config.set, method/config.schema, method/book.options, method/book.setOptions, guide/set-up-business, guide/configure-red-ale
from: manual/ch_Customize.docbook#set-prefs
---
GnuCash keeps two kinds of settings. **Preferences** belong to the person and live in GSettings (dconf, the registry or
a plist). **Book options** belong to the book and live in the file. red-ale keeps the same split: the
**Configuration** is a file of the person's settings, and the book options stay in the book, where GnuCash reads them.
See [Configuration](concept/configuration).

## Edit > Preferences

[`config.schema`](method/config.schema) lists every setting with its type and default, [`config.get`](method/config.get)
reads one or all, and [`config.set`](method/config.set) changes several at once, all or none. They work with no book open.

| GnuCash tab | red-ale |
|---|---|
| Accounting Period | `accountingPeriod.start`, `.end` and `.fiscalYearEnd`. [`period.get`](method/period.get), [`period.set`](method/period.set) and [`period.dates`](method/period.dates) read, change and work out the dates ([Closing the books and the accounting period](concept/closing-the-books-and-accounting-period)). |
| Accounts | Not provided. Colours, columns and the Account tree's filter are display. `excludeHidden` is a parameter of `account.list` and `account.tree` |
| Business | The invoice and bill defaults are book options (below). Reminder days and the like are not provided |
| General | Not provided: red-ale does not autosave, warn before saving or show a Tip of the Day. The date and number conventions are under Numbers, Date, Time |
| Import | `import.settings.*` for the matcher's thresholds, `import.csvLayouts.*` for saved CSV layouts and `import.qifAccountMap` for QIF ([Importing and matching](concept/importing-and-matching)) |
| Numbers, Date, Time | `locale` for the number, date and currency conventions (`C` by default, or a POSIX name such as `fr_FR`), `defaultCurrency`, and `reports.dateFormat` for reports |
| Online Banking | Not supported ([Not in red-ale](gnucash/not-in-red-ale)) |
| Online Quotes | `quotes.network` (off by default), `quotes.services.*` and `quotes.sources.*` ([Prices and exchange rates](concept/prices-and-exchange-rates)) |
| Printing | Not provided: export the HTML of a report and print it from a browser |
| Register, Register Defaults | Not provided: there is no register |
| Reports | `reports.directory` (your own scripts), `reports.currency`, `reports.weekStart`, `reports.reverseBalancedAccounts` and `reports.limits.*` ([Reports](concept/reports)) |
| Scheduled Transactions | Not provided. red-ale never runs scheduled transactions on its own; you ask with `scheduledTransaction.sinceLastRun` and then `.apply` ([Tools](gnucash/tools-menu)) |
| Windows | Not provided |

Document links have one setting, `docLinks.pathHead`, which [`docLink.pathHead`](method/docLink.pathHead) reads and
[`docLink.setPathHead`](method/docLink.setPathHead) changes: it is the folder that relative links start from. It is in
the Configuration, never in the book.

red-ale has no GSettings. A setting you leave alone has its default. A key in the Configuration file that red-ale does
not know is kept and listed in `unknown`.

## File > Properties (Book Options)

[`book.options`](method/book.options) reads the options in the book and [`book.setOptions`](method/book.setOptions)
changes them. An option you leave out is left alone.

| GnuCash tab | red-ale |
|---|---|
| Accounts: Use Trading Accounts | `useTradingAccounts` ([Trading accounts](concept/trading-accounts)) |
| Accounts: Day Threshold for Read-Only Transactions | `autoReadOnlyDays`; 0 turns it off ([Read-only books](concept/read-only-books)) |
| Accounts: Use Split Action Field for Number | `splitActionForNum`. Turning it on adds the Book feature `Number Field Source` ([Books and files](concept/books-and-files)) |
| Budgeting: Default Budget | `defaultBudget` ([Budgets](concept/budgets)) |
| Business: company name, address, contact, phone, fax, email, website, company ID, tax number | `companyName`, `companyAddress`, `companyContactPerson`, `companyPhoneNumber`, `companyFaxNumber`, `companyEmailAddress`, `companyWebsiteUrl`, `companyId`, `taxNumber` |
| Business: Default Invoice Report (and the layout, title and footer of invoices) | `defaultInvoiceReport` ([Business](gnucash/business-menu)) |
| Counters | [`counter.list`](method/counter.list), [`counter.set`](method/counter.set). **⚠** The counter is the last number issued; the next document gets one more |

Options that red-ale does not know are kept in the book as they are, and written back unchanged.
