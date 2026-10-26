# API: export

CSV export of the account tree and of transactions, and Export Accounts: a new book holding the account tree and
commodities. Scope: [`../milestone3.md`](../milestone3.md) ("Export", "Compatibility in v0.3"). Terms:
[`../glossary.md`](../glossary.md), "Import and export".

**Exports only read.** No export changes the Book, makes it dirty or writes the journal, so every export works on a
book open read-only. CSV export returns its text and writes no file, as `report.export` returns HTML; the CLI writes
the file on its side. Export accounts writes a new book file, as `book.create` does, and refuses a path that exists.

A file red-ale exports as CSV imports back to the same transactions with the `GnuCash Export Format` CSV import
layout ([`import.md`](import.md)), and to the same accounts with the CSV account importer.

## What GnuCash does

### Choices

GnuCash's CSV export assistant has three entries (`5.17:gnucash/ui/gnc-plugin-csv-export.ui:6-17`): the account tree,
transactions, and the active register. The choices reach the export code in `CsvExportInfo`
(`5.17:gnucash/import-export/csv-exp/assistant-csv-export.h:69-100`):

| Choice | Default | Source |
|--------|---------|--------|
| Separator: comma, colon, semicolon, or any custom string | comma | `assistant-csv-export.c:203-236`, `:282-296`, `:304-315` |
| Use quotes: quote every field | off | `:244-253`, `:304-315` |
| Simple layout (transactions only) | off: the complex layout | `:260-275`, `:875-883` |
| Accounts (transactions only), several | none selected | `:360-376`, `:422`, `:894-900` |
| Date range (transactions only): all, or from earliest/a date/today to latest/a date/today | all | `:483-537`, `:618-653`, `:929-930` |

A chosen start date is the start of that local day, an end date its end (`get_filter_times`, `:483-515`). The
register export runs the register's own query instead, with its filter and sort.

### Writing a line

`gnc_csv_add_line` (`5.17:gnucash/import-export/csv-exp/csv-export-helpers.cpp:48-84`) quotes a field when "Use
quotes" is on, or when it contains the separator string, `"`, LF or CR; a `"` inside is doubled; newlines stay raw
inside the quotes. Every line, header included, ends in CRLF (`:38-44`). The file is UTF-8 with no byte-order mark
(`5.17:libgnucash/core-utils/gnc-filepath-utils.cpp:1343-1348`). GnuCash's own tests pin this
(`5.17:gnucash/import-export/csv-exp/test/test-csv-export-helpers.cpp`): `A,B,C,,D` with CRLF; `"A","B","C","","D"`
with quotes on; `""""` for a lone `"`; `",D"` for a field holding the separator.

### The account tree

The header (`5.17:gnucash/import-export/csv-exp/csv-tree-export.cpp:59-64`) is `Type`, `Full Account Name`,
`Account Name`, `Account Code`, `Description`, `Account Color`, `Notes`, `Symbol`, `Namespace`, `Hidden`, `Tax
Info`, `Placeholder`, all translated. Each row (`:82-95`):

| Column | Value |
|--------|-------|
| Type | The type's name, untranslated: `BANK`, `CASH`, … (`5.17:libgnucash/engine/Account.cpp:4235-4264`) |
| Full Account Name, Account Name | The Full name with the account separator, and the name |
| Account Code, Description, Account Color, Notes | The text, or empty |
| Symbol, Namespace | The commodity's mnemonic and namespace |
| Hidden, Tax Info, Placeholder | `T` or `F` (`:74`) |

Every account but the root, depth first, siblings in Display order (`gnc_account_get_descendants_sorted`, `:72`;
`xaccAccountOrder`, `Account.cpp:2363-2413`), so parents come before children.

### Transactions

The header follows the Book's num field source in two of its names only (`Transaction Number` for `Number`,
`Number/Action` for `Action`); the data does not (`5.17:gnucash/import-export/csv-exp/csv-transactions-export.cpp:347-388`,
`:91-96`, `:139-144`).

| Layout | Columns |
|--------|---------|
| Simple (11) | Date, Account Name, Number, Description, Full Category Path, Reconcile, Amount With Sym, Amount Num., Value With Sym, Value Num., Rate/Price |
| Complex (18) | Date, Transaction ID, Number, Description, Notes, Commodity/Currency, Void Reason, Action, Memo, Full Account Name, Account Name, Amount With Sym, Amount Num., Value With Sym, Value Num., Reconcile, Reconcile Date, Rate/Price |

The simple layout writes one line per transaction, for its split in the exported account; the complex layout one line
per split, every line repeating the transaction's fields, splits in stored order, splits in other accounts included
(`:210-253`, `:290-315`). How each column is formed (`:55-206`):

| Column | Value |
|--------|-------|
| Date, Reconcile Date | `qof_print_date_buff` in the date format preference (`5.17:libgnucash/engine/gnc-date.cpp:502-523`, `:574-595`), default `locale` (the locale's `D_FMT`). Reconcile Date only for `y` |
| Transaction ID | The GUID, 32 hex digits |
| Number, Description, Notes, Void Reason, Action, Memo | The text, or empty |
| Account Name | Simple layout: the split account's **full** name, despite the header. Complex: the name; Full Account Name the full name |
| Full Category Path | The other split's account full name, or `-- Split Transaction --` (translated) when there is not exactly one other split (`xaccSplitGetOtherSplit`, `5.17:libgnucash/engine/Split.cpp:2161-2185`) |
| Commodity/Currency | The transaction currency's unique name, `CURRENCY::USD` |
| Reconcile | `n`, `c`, `y`, `f` or `v`, translated (`5.17:libgnucash/app-utils/gnc-ui-util.cpp:469-486`) |
| Amount With Sym, Amount Num. | The amount (a void transaction's former amount), printed with the account's print info (places from the account's SCU); Num. without symbol or grouping (`gnc-ui-util.cpp:919-974`) |
| Value With Sym, Value Num. | The value (or former value), printed with the transaction currency's print info |
| Rate/Price | The share price, value/amount (`xaccSplitGetSharePrice`, `Split.cpp:1995-2027`), printed with the price print info of the split's **account commodity**, with grouping. A void transaction gives 0 in the simple layout, and former value/former amount to 6 significant figures in the complex one |

Numbers are printed by `xaccPrintAmount` (`gnc-ui-util.cpp:1082-1444`), with the locale's monetary decimal point,
grouping, symbol and sign position. A whole amount of a commodity printed with no minimum places ends in a bare point
(`10.`), as in reports ([`../reports.md`](../reports.md) §3).

**Which transactions** (`:263-334`, `:394-408`): for each selected account in selection order, its own splits (not
its sub-accounts') posted in the range, inclusive, sorted by Split order. Each transaction is written once, under the
first account it is met in; the set of written transactions is marked before splits with no account and trading
splits are skipped, so a transaction first met through such a split is never written (`:277-288`). Trading splits are
left out unless the exported account is itself a trading account.

### Export accounts

File ▸ Export ▸ Export Accounts (`5.17:gnucash/gnome/gnc-plugin-basic-commands.c:430-445`) calls `gnc_file_do_export`
(`5.17:gnucash/gnome-utils/gnc-file.c:1312-1443`):

1. The path gets `.gnucash` if it lacks it (`:1341`), and may not be in GnuCash's user data directory or be the open
   book's own file (`:1362-1383`).
2. A new session with **no book** begins on it with `SESSION_NEW_STORE`, which takes the `.LCK` lock and fails when
   the file exists (`5.17:libgnucash/backend/xml/gnc-xml-backend.cpp:128-133`, `:333-341`); the GUI then asks to
   overwrite.
3. `qof_session_export` (`5.17:libgnucash/engine/qofsession.cpp:676-682`, `:530-554`) calls the backend's
   `export_coa`. The XML backend writes the file with `g_fopen(path, "w")`: **uncompressed**, with no temporary file and
   no backup (`gnc-xml-backend.cpp:343-355`). The SQL backends have no `export_coa`, so exporting to SQLite writes
   nothing and reports success (`5.17:libgnucash/engine/qof-backend.hpp:250`).

What is written (`gnc_book_write_accounts_to_xml_filehandle_v2`, `5.17:libgnucash/backend/xml/io-gncxml-v2.cpp:1340-1374`):
the `gnc-v2` header of a full save; `count-data` for commodities and accounts; the commodities, as a full save writes
them; and the accounts, root first, each as a full save writes it (GUID, type, commodity, SCU, code, description,
every slot, parent) **except its lots** (`5.17:libgnucash/backend/xml/gnc-account-xml-v2.cpp:146-158`). There is **no
`<gnc:book>` element**: no book GUID, no book slots, no prices, transactions, template transactions, scheduled
transactions, budgets or business objects. GnuCash reads such a file as an old-style book whose book is implicit
(`io-gncxml-v2.cpp:715-731`). The open book is only read.

## What red-ale does

### Text conventions

Both CSV exports write as `gnc_csv_add_line` does: the same quoting, CRLF after every line, UTF-8 with no byte-order
mark. The header strings and the `-- Split Transaction --` placeholder are GnuCash's English msgids, as red-ale has no
translations; the reconcile letters are `n c y f v`. Dates follow `reports.dateFormat`, the setting that stands for
GnuCash's date format preference ([`../reports.md`](../reports.md) §9), and numbers follow the Locale
([`../milestone3.md`](../milestone3.md), "Locales"): under the default `C` locale, `$1,234.56`-style output, as
GnuCash prints with `gnc_localeconv`'s defaults (`5.17:libgnucash/core-utils/gnc-locale-utils.c:76-109`). The account
separator is `:`, GnuCash's default (`5.17:gnucash/gschemas/org.gnucash.GnuCash.gschema.xml.in:18-19`). Number
printing is the report renderer's `xaccPrintAmount` translation, shared, not a second copy, with the print infos the
exporter asks for (`gnc_account_print_info`, `gnc_commodity_print_info` and `gnc_price_print_info`,
`5.17:libgnucash/app-utils/gnc-ui-util.cpp:833-1063`). The Locale is the Configuration's `locale` setting, or the
`locale` param of `export.csvTransactions` for one call, as for `report.export`; the account tree prints no numbers or
dates, so it takes none.

The CSV exporters live in the `red-ale-export` crate (translated from `5.17:gnucash/import-export/csv-exp/`), which
reads the Book and returns text; every number and date it writes goes through one type, `Formats`, which holds the
Locale and the date format.

The output is deterministic: the same book, params and settings give the same bytes.

### `export.csvTree` (read)

| Param | Type | Default | Meaning |
|-------|------|---------|---------|
| `separator` | string | `,` | Any non-empty string, as GnuCash's custom separator allows |
| `quotes` | bool | false | Quote every field |

→ `{ csv, rows }`: the text and the number of account rows, the header not counted. Accounts in Display order, which red-ale compares
bytewise where GnuCash collates names by locale (as everywhere, [`../reports.md`](../reports.md) §14). `invalid` for
an empty separator.

### `export.csvTransactions` (read)

| Param | Type | Default | Meaning |
|-------|------|---------|---------|
| `accounts` | AccountRef[] | | Required, at least one, in the order to process them; a transaction goes under the first one it has a split in. Sub-accounts are not included unless listed, as in GnuCash |
| `from`, `to` | Date? | earliest, latest | From local 00:00:00 of `from` to local 23:59:59 of `to`, inclusive |
| `layout` | string | `complex` | `simple` or `complex` |
| `separator` | string | `,` | As above |
| `quotes` | bool | false | As above |
| `locale` | string? | the `locale` setting | The Locale for this call: a supported locale name or `environment` |

→ `{ csv, transactions, lines }`: the text, the transactions written and the lines written, the header not counted.
Which transactions, their order, the columns and their formats are GnuCash's, above,
including the trading-split rule. `not-found` for an unknown account; `invalid` for none, an empty separator, or `from`
after `to`; `invalid-params` for an unknown Locale.

GnuCash's register export (a register's query) has no method: a client gets the same transactions from
`transaction.find` and exports their accounts, or asks for one account.

### `export.accounts` (read)

| Param | Type | Meaning |
|-------|------|---------|
| `path` | string | The new book file, a path or `xml://` URI. `.gnucash` is added when the name does not end in it (case matters, as in `gnc_uri_add_extension`, `5.17:libgnucash/engine/gnc-uri-utils.c:413-429`) |

→ `{ path, accounts, commodities }`: the file written, and the account elements (the root's included) and commodity
elements it holds. red-ale writes what `gnc_book_write_accounts_to_xml_filehandle_v2` writes, above, through the XML
backend's own writers, so each account and commodity element is the one a full save would write, without lots, and
the accounts come in the order a full save writes them (root first for a book GnuCash wrote). The file is
uncompressed, as GnuCash's is.

- **A new file only.** An existing path is `file-exists`, as `book.create` refuses one; there is no overwrite.
  Paths are the client's own, as for `book.create`, and the path of the open book is refused (`invalid`). A path whose
  `.LCK` file exists is `book-locked`; the path is checked again once the lock is held.
- **Written safely**, unlike GnuCash: through a temporary file in the same directory and a rename, holding the new
  file's `.LCK` lock while it writes and releasing it afterwards, on error paths too. No backup is made, since there
  was no file, and no journal: the export does not touch the open book's journal (GnuCash moves its journal beside the
  exported file, `gnc-xml-backend.cpp:148`; `5.17:libgnucash/engine/TransLog.cpp:120-176`).
- **Checked before it is kept**, as a save is: the temporary file is read back and every commodity and account in it
  must equal the open Book's, with no lots and nothing else; otherwise the call fails with `save-verification` and
  leaves no file. Such a file has no `gnc:book`, so red-ale's `book.open` does not read it; GnuCash does.
- **The open book is unchanged and not dirty.** Works on a read-only book.
- **The oracle opens the result**: GnuCash 5.17 reads the exported file with the same accounts, GUIDs, slots and
  commodities, and no transactions.

The effect is `read`: the open Book and the Configuration are unchanged. Like `book.create`, the method creates a
file the client names; unlike it, it never touches an existing one.

## Methods

| Method | Effect | Does |
|--------|--------|------|
| `export.csvTree` | read | The account tree as CSV text |
| `export.csvTransactions` | read | Transactions of some accounts and dates as CSV text, simple or complex layout |
| `export.accounts` | read | A new book file with the account tree and commodities, no transactions |

MCP marks all three read only. No new error codes.

## CLI

```
red-ale exec --book books/home.gnucash --read-only export.csvTree --output accounts.csv
red-ale exec --book books/home.gnucash --read-only export.csvTransactions \
    'accounts:=["Assets:Checking"]' from=2024-01-01 to=2024-12-31 --output checking-2024.csv
red-ale exec --book books/home.gnucash --read-only export.accounts path=books/chart-of-accounts.gnucash
```

`--output` writes the result's `csv` field to the file on the caller's side, as it writes a report's `html`
([`reports.md`](reports.md), "CLI"), and prints the result's other fields with `path` and `bytes`.

**Large exports over HTTP.** The CSV text travels in one JSON-RPC response. `red-ale serve` sends responses of up to
the Configuration's `jsonRpc.maxResponseBytes`, 256 MiB by default ([`configuration.md`](configuration.md)), rather
than jsonrpsee's 10 MB, which a few years of transactions in the complex layout pass; a larger export fails with an
error and the book is untouched. `red-ale call` accepts a response of any size, and `exec` and MCP have no limit.

## Deviations from GnuCash

| What | GnuCash 5.17 | red-ale | Why |
|------|--------------|---------|-----|
| Header text and placeholders | Translated | English msgids | red-ale has no translations |
| Sibling order in the tree export | Names by locale collation | Bytewise | As every red-ale sort ([`../reports.md`](../reports.md) §14) |
| Export to an existing file | Overwrites after asking | `file-exists` | Never overwrite a file the client may need |
| How the exported book is written | In place, no temporary file; write errors after opening ignored (`gnc-xml-backend.cpp:343-355`) | Temporary file and rename; any error fails the call and leaves no file | A half-written book looks like a book |
| Exporting accounts to SQLite | Writes nothing, reports success | Not offered (SQL books come in v0.4) | |
| The open book's journal during Export accounts | Moved beside the exported file | Untouched | The journal belongs to the open book |
| The register export | From a register's query | No method | red-ale has no registers; `export.csvTransactions` covers it by account |

## Tests

- GnuCash's line-writer cases (`test-csv-export-helpers.cpp`) as unit tests.
- A synthetic book exported in both layouts, compared with committed expected files, under the `C` locale and one
  other. Done: `crates/red-ale-export/tests/csv.rs` builds a book by hand and pins every line, citing the 5.17 lines
  each follows (`de_DE` as the other Locale); `crates/red-ale-rpc/tests/export.rs` compares `synthetic/rich.gnucash`'s
  tree and both layouts with `crates/red-ale-rpc/tests/export/*.csv`.
- **Round trips:** (with the CSV importers) the complex export imported back with the `GnuCash Export Format` layout gives the same
  transactions; the tree export imported back with the account importer gives the same accounts.
- **Read only:** each export on a read-only book; the book is not dirty and the journal is unchanged afterwards.
- **Oracle:** GnuCash 5.17 opens the file `export.accounts` wrote, logging nothing, and finds the source book's
  accounts, commodities and slots, and no transactions, prices or lots (`crates/red-ale-testkit/tests/oracle_export.rs`,
  on three fixtures).
