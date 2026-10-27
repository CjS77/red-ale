# The SQL backend: how GnuCash 5.17 stores, locks, loads and writes SQLite books

Scope: the DBI backend (`libgnucash/backend/dbi/`) and the SQL object backends (`libgnucash/backend/sql/`) as they
behave for SQLite, plus the engine and GUI code that decides what they do. MySQL and PostgreSQL appear only where
they differ in a way that matters for SQLite. Every behavioural claim cites `5.17:<path>:<line>` (a bare `:<line>`
means the file cited just before). "Oracle" means the GnuCash 5.17 library and Python bindings described in
`CLAUDE.md`. Experiment scripts and outputs were run in a scratch directory and are quoted verbatim, with paths and
long lines shortened (`…`).

Terms follow `design/glossary.md`: *Lock*, *Journal*, *Backup*, *Book feature*, *Read-only session*, *Residue*,
*Backend*. New terms this note needs (*SQL book*, *Schema version*) are listed for the glossary in §13; three clashes
are flagged there.

Oracle setup: as in [`storage-and-safety.md`](storage-and-safety.md), with `GSETTINGS_BACKEND=keyfile` and a scratch
`XDG_CONFIG_HOME`. The SQLite library under the oracle is the system's: libdbi 0.9.0, libdbd-sqlite3 0.9.0,
SQLite 3.45.1. `Session.destroy()` after an SQLite session aborts the oracle with `free(): invalid pointer`; `end()`
has already released the lock by then, so the scripts call `end()` only.

---

## 0. Facts that surprise

1. **Opening an SQLite book writes to it, even with `SESSION_READ_ONLY`.** Loading can set the ISO-dates Book
   feature, create missing tables, rebuild tables whose schema version is older, recover tables left by a failed
   safe-save, and add Imbalance accounts and splits. None of these checks the session mode (§5.4, experiments E3–E9).
2. **The ISO-dates Book feature is set on every load, not on save.** `GncDbiBackend::load` calls
   `gnc_features_set_used` after loading (`5.17:libgnucash/backend/dbi/gnc-backend-dbi.cpp:879-880`), and the change
   goes straight to the database. Save As into a new SQLite file does not set it; the first open does (§3.5).
3. **There is no Save.** Each `commit_edit` writes that one object in its own SQL transaction
   (`5.17:libgnucash/backend/sql/gnc-sql-backend.cpp:580-666`). `qof_session_save` does nothing on an SQL book,
   because the book is never dirty (`5.17:libgnucash/engine/qofsession.cpp:439-440`).
4. **A GnuCash transaction is not one SQL transaction.** Changing two splits gives two `BEGIN … COMMIT` pairs, one
   per split; the transaction row isn't touched at all if only its splits changed (experiment E2). A crash between
   them leaves the database unbalanced.
5. **Every commit deletes and re-inserts all the object's slots,** with new `id`s and new random GUIDs for every
   frame and list (`5.17:libgnucash/backend/sql/gnc-slots-sql.cpp:645-649`, `:580-619`).
6. **Per-table schema versions are upgraded silently on load, and newer ones are ignored silently.** Only the
   `Gnucash` and `Gnucash-Resave` rows of `versions` produce the "too old" and "too new" errors. The GUI's
   upgrade-or-read-only dialog is about those rows, not about tables (§7).
7. **`SESSION_READ_ONLY` skips the lock and nothing else.** Edits made in a read-only session are written to the
   database, even while another process holds the lock (experiment E10).
8. **The lock row is checked by existence only.** Any row in `gnclock` means "locked", whatever its host or PID.
   `SESSION_BREAK_LOCK` deletes every row, and the unlock at the end deletes every row if one of them is ours
   (`5.17:libgnucash/backend/dbi/gnc-dbisqlconnection.cpp:130-153`, `:192-219`).
9. **GnuCash itself loses data in double slots.** It writes doubles with 12 fixed decimals and reads them back with
   about 15 significant digits: `1.234567890123e-13` becomes `0.0`, `1.2345678901234567e+19` becomes
   `1.23456789012346e+19` on the next rewrite (§3.8, experiment E11).
10. **SQL books have no place for scheduled transactions' deferred instances.** Saving an XML book as SQLite drops
    them; nested KVP lists are also not stable through a save (experiment E1b).
11. **No query has `ORDER BY`.** Load order is whatever SQLite returns for `SELECT *`, rowid order in practice. The
    order of a KVP list is its rows' order (§9).
12. **Opening and closing with no edits changes the file anyway.** In normal mode the lock row is inserted and
    deleted, so the SQLite file change counter moves by 2 (header bytes 24–27 and 92–95) even though every row is
    the same (experiment E3).

---

## 1. Files and URIs

- A file is an SQLite book if its first bytes start with `SQLite format 3`, or if it can't be opened (treated as
  new) (`5.17:libgnucash/backend/dbi/gnc-backend-dbi.cpp:1029-1068`). URI schemes and backend choice are in
  [`storage-and-safety.md`](storage-and-safety.md) §1.
- `session_begin` for SQLite (`5.17:libgnucash/backend/dbi/gnc-backend-dbi.cpp:355-459`):
  - a missing file without a create mode is `ERR_FILEIO_FILE_NOT_FOUND` (`:376-384`);
  - an existing file with `SESSION_NEW_STORE` is `ERR_BACKEND_STORE_EXISTS`; with `SESSION_NEW_OVERWRITE` the file
    is unlinked first (`:386-398`);
  - the connection uses the file's directory and basename (`:401-406`);
  - unless read-only, it runs the libdbi 64-bit test (`:428-442`), which creates, fills and drops a `TEMPORARY`
    table `numtest`, so nothing is left in the file (`:1235-1312`);
  - it constructs the connection, which takes the lock and recovers failed safe-saves (§4, §5.2);
  - it sets the journal base name to the book's path (`:456`).
- The file GnuCash creates: page size 4096, UTF-8, no auto-vacuum, `user_version` 0, `application_id` 0, rollback
  journal mode `delete` (header bytes 18–19 are 1, 1: not WAL). SQLite's transient `<book>-journal` file appears
  beside the book during each write transaction. (Oracle, `pragma` on a book from experiment E1.)

## 2. Schema

### 2.1 Column types (`5.17:libgnucash/backend/dbi/gnc-dbiproviderimpl.hpp:55-100`)

| Engine column type | Backend type | SQLite DDL | Source of the mapping |
|---|---|---|---|
| `CT_INT`, `CT_BOOLEAN` | `BCT_INT` | `integer` | `5.17:libgnucash/backend/sql/gnc-sql-column-table-entry.cpp:180-185`, `:216-221` |
| `CT_INT64`, numeric halves | `BCT_INT64` | `bigint` | `…:251-257`, `:578-591` |
| `CT_DOUBLE` | `BCT_DOUBLE` | `float8` | `…:308-313` |
| `CT_STRING` | `BCT_STRING` | `text(<size>)` | `…:132-137` |
| `CT_GUID` and object references | `BCT_STRING`, size 32 | `text(32)` | `…:343-348`, `:109-114` |
| `CT_TIME` | `BCT_DATETIME`, size 19 | `text(19)` | `…:368`, `:414-420` |
| `CT_GDATE` | `BCT_DATE`, size 8 | `text(8)` | `…:459`, `:505-510` |

Flags add ` PRIMARY KEY`, ` AUTOINCREMENT` and ` NOT NULL` in that order (`gnc-dbiproviderimpl.hpp:88-99`). A
`CT_NUMERIC` column `x` becomes `x_num bigint` and `x_denom bigint`, both taking the column's `NOT NULL`
(`gnc-sql-column-table-entry.cpp:578-591`). With these declared types SQLite gives `integer`/`bigint` INTEGER
affinity, `float8` REAL affinity and `text(n)` TEXT affinity; `text(n)` has no length limit in SQLite.

### 2.2 The DDL GnuCash 5.17 writes

Dumped (`sqlite3 .schema`) from the book the oracle wrote in experiment E1, verbatim:

```
CREATE TABLE gnclock ( Hostname varchar(255), PID int );
CREATE TABLE versions(table_name text(50) PRIMARY KEY NOT NULL, table_version integer NOT NULL);
CREATE TABLE books(guid text(32) PRIMARY KEY NOT NULL, root_account_guid text(32) NOT NULL, root_template_guid text(32) NOT NULL);
CREATE TABLE commodities(guid text(32) PRIMARY KEY NOT NULL, namespace text(2048) NOT NULL, mnemonic text(2048) NOT NULL, fullname text(2048), cusip text(2048), fraction integer NOT NULL, quote_flag integer NOT NULL, quote_source text(2048), quote_tz text(2048));
CREATE TABLE accounts(guid text(32) PRIMARY KEY NOT NULL, name text(2048) NOT NULL, account_type text(2048) NOT NULL, commodity_guid text(32), commodity_scu integer NOT NULL, non_std_scu integer NOT NULL, parent_guid text(32), code text(2048), description text(2048), hidden integer, placeholder integer);
CREATE TABLE budgets(guid text(32) PRIMARY KEY NOT NULL, name text(2048) NOT NULL, description text(2048), num_periods integer NOT NULL);
CREATE TABLE budget_amounts(id integer PRIMARY KEY AUTOINCREMENT NOT NULL, budget_guid text(32) NOT NULL, account_guid text(32) NOT NULL, period_num integer NOT NULL, amount_num bigint NOT NULL, amount_denom bigint NOT NULL);
CREATE TABLE sqlite_sequence(name,seq);
CREATE TABLE prices(guid text(32) PRIMARY KEY NOT NULL, commodity_guid text(32) NOT NULL, currency_guid text(32) NOT NULL, date text(19) NOT NULL, source text(2048), type text(2048), value_num bigint NOT NULL, value_denom bigint NOT NULL);
CREATE TABLE transactions(guid text(32) PRIMARY KEY NOT NULL, currency_guid text(32) NOT NULL, num text(2048) NOT NULL, post_date text(19), enter_date text(19), description text(2048));
CREATE INDEX tx_post_date_index ON transactions(post_date);
CREATE TABLE splits(guid text(32) PRIMARY KEY NOT NULL, tx_guid text(32) NOT NULL, account_guid text(32) NOT NULL, memo text(2048) NOT NULL, action text(2048) NOT NULL, reconcile_state text(1) NOT NULL, reconcile_date text(19), value_num bigint NOT NULL, value_denom bigint NOT NULL, quantity_num bigint NOT NULL, quantity_denom bigint NOT NULL, lot_guid text(32));
CREATE INDEX splits_tx_guid_index ON splits(tx_guid);
CREATE INDEX splits_account_guid_index ON splits(account_guid);
CREATE TABLE slots(id integer PRIMARY KEY AUTOINCREMENT NOT NULL, obj_guid text(32) NOT NULL, name text(4096) NOT NULL, slot_type integer NOT NULL, int64_val bigint, string_val text(4096), double_val float8, timespec_val text(19), guid_val text(32), numeric_val_num bigint, numeric_val_denom bigint, gdate_val text(8));
CREATE INDEX slots_guid_index ON slots(obj_guid);
CREATE TABLE recurrences(id integer PRIMARY KEY AUTOINCREMENT NOT NULL, obj_guid text(32) NOT NULL, recurrence_mult integer NOT NULL, recurrence_period_type text(2048) NOT NULL, recurrence_period_start text(8) NOT NULL, recurrence_weekend_adjust text(2048) NOT NULL);
CREATE TABLE schedxactions(guid text(32) PRIMARY KEY NOT NULL, name text(2048), enabled integer NOT NULL, start_date text(8), end_date text(8), last_occur text(8), num_occur integer NOT NULL, rem_occur integer NOT NULL, auto_create integer NOT NULL, auto_notify integer NOT NULL, adv_creation integer NOT NULL, adv_notify integer NOT NULL, instance_count integer NOT NULL, template_act_guid text(32) NOT NULL);
CREATE TABLE lots(guid text(32) PRIMARY KEY NOT NULL, account_guid text(32), is_closed integer NOT NULL);
CREATE TABLE billterms(guid text(32) PRIMARY KEY NOT NULL, name text(2048) NOT NULL, description text(2048) NOT NULL, refcount integer NOT NULL, invisible integer NOT NULL, parent text(32), type text(2048) NOT NULL, duedays integer, discountdays integer, discount_num bigint, discount_denom bigint, cutoff integer);
CREATE TABLE customers(guid text(32) PRIMARY KEY NOT NULL, name text(2048) NOT NULL, id text(2048) NOT NULL, notes text(2048) NOT NULL, active integer NOT NULL, discount_num bigint NOT NULL, discount_denom bigint NOT NULL, credit_num bigint NOT NULL, credit_denom bigint NOT NULL, currency text(32) NOT NULL, tax_override integer NOT NULL, addr_name text(1024), addr_addr1 text(1024), addr_addr2 text(1024), addr_addr3 text(1024), addr_addr4 text(1024), addr_phone text(128), addr_fax text(128), addr_email text(256), shipaddr_name text(1024), shipaddr_addr1 text(1024), shipaddr_addr2 text(1024), shipaddr_addr3 text(1024), shipaddr_addr4 text(1024), shipaddr_phone text(128), shipaddr_fax text(128), shipaddr_email text(256), terms text(32), tax_included integer, taxtable text(32));
CREATE TABLE employees(guid text(32) PRIMARY KEY NOT NULL, username text(2048) NOT NULL, id text(2048) NOT NULL, language text(2048) NOT NULL, acl text(2048) NOT NULL, active integer NOT NULL, currency text(32) NOT NULL, ccard_guid text(32), workday_num bigint NOT NULL, workday_denom bigint NOT NULL, rate_num bigint NOT NULL, rate_denom bigint NOT NULL, addr_name text(1024), addr_addr1 text(1024), addr_addr2 text(1024), addr_addr3 text(1024), addr_addr4 text(1024), addr_phone text(128), addr_fax text(128), addr_email text(256));
CREATE TABLE entries(guid text(32) PRIMARY KEY NOT NULL, date text(19) NOT NULL, date_entered text(19), description text(2048), action text(2048), notes text(2048), quantity_num bigint, quantity_denom bigint, i_acct text(32), i_price_num bigint, i_price_denom bigint, i_discount_num bigint, i_discount_denom bigint, invoice text(32), i_disc_type text(2048), i_disc_how text(2048), i_taxable integer, i_taxincluded integer, i_taxtable text(32), b_acct text(32), b_price_num bigint, b_price_denom bigint, bill text(32), b_taxable integer, b_taxincluded integer, b_taxtable text(32), b_paytype integer, billable integer, billto_type integer, billto_guid text(32), order_guid text(32));
CREATE TABLE invoices(guid text(32) PRIMARY KEY NOT NULL, id text(2048) NOT NULL, date_opened text(19), date_posted text(19), notes text(2048) NOT NULL, active integer NOT NULL, currency text(32) NOT NULL, owner_type integer, owner_guid text(32), terms text(32), billing_id text(2048), post_txn text(32), post_lot text(32), post_acc text(32), billto_type integer, billto_guid text(32), charge_amt_num bigint, charge_amt_denom bigint);
CREATE TABLE jobs(guid text(32) PRIMARY KEY NOT NULL, id text(2048) NOT NULL, name text(2048) NOT NULL, reference text(2048) NOT NULL, active integer NOT NULL, owner_type integer, owner_guid text(32));
CREATE TABLE orders(guid text(32) PRIMARY KEY NOT NULL, id text(2048) NOT NULL, notes text(2048) NOT NULL, reference text(2048) NOT NULL, active integer NOT NULL, date_opened text(19) NOT NULL, date_closed text(19) NOT NULL, owner_type integer NOT NULL, owner_guid text(32) NOT NULL);
CREATE TABLE taxtables(guid text(32) PRIMARY KEY NOT NULL, name text(50) NOT NULL, refcount bigint NOT NULL, invisible integer NOT NULL, parent text(32));
CREATE TABLE taxtable_entries(id integer PRIMARY KEY AUTOINCREMENT NOT NULL, taxtable text(32) NOT NULL, account text(32) NOT NULL, amount_num bigint NOT NULL, amount_denom bigint NOT NULL, type integer NOT NULL);
CREATE TABLE vendors(guid text(32) PRIMARY KEY NOT NULL, name text(2048) NOT NULL, id text(2048) NOT NULL, notes text(2048) NOT NULL, currency text(32) NOT NULL, active integer NOT NULL, tax_override integer NOT NULL, addr_name text(1024), addr_addr1 text(1024), addr_addr2 text(1024), addr_addr3 text(1024), addr_addr4 text(1024), addr_phone text(128), addr_fax text(128), addr_email text(256), terms text(32), tax_inc text(2048), tax_table text(32));
```

- No column has a `DEFAULT`, and there are no foreign keys, `UNIQUE` constraints or triggers. Each `PRIMARY KEY` on a
  text column gets an implicit `sqlite_autoindex_<table>_1`. The explicit indexes are exactly the four above.
- `sqlite_sequence` exists because of the `AUTOINCREMENT` tables; GnuCash's table lists skip it
  (`5.17:libgnucash/backend/dbi/gnc-dbiproviderimpl.hpp:236-247`).
- The `gnclock` DDL is written by hand (`5.17:libgnucash/backend/dbi/gnc-dbisqlconnection.cpp:111-114`); every
  other table comes from its column table through `create_table` (`:447-477`).
- `create_index_ddl` writes `ddl =+ ", "` where it means `+=` (`5.17:libgnucash/backend/dbi/gnc-dbisqlconnection.cpp:489`),
  so a multi-column index would be malformed. Every index GnuCash creates has one column, so it never fires.
- Address columns (`addr_*`, `shipaddr_*`) come from `5.17:libgnucash/backend/sql/gnc-address-sql.cpp:51-68`; owner
  references (`owner_type` integer, `owner_guid` text(32)) from `gnc-owner-sql.cpp`. The address column table
  declares `COL_NNUL` (`gnc-address-sql.cpp:53-67`), but the DDL has no `NOT NULL`: the address columns take the
  flag from the owning address column, not from their sub-columns (`gnc-address-sql.cpp:101-110`).

### 2.3 Tables, versions and their sources

| Table | Version in 5.17 | Column table | Version define |
|---|---|---|---|
| `versions` | not versioned | `5.17:libgnucash/backend/sql/gnc-sql-backend.cpp:74-79` | — |
| `books` | 1 | `gnc-book-sql.cpp:60-70` | `gnc-book-sql.cpp:50-51` |
| `commodities` | 1 | `gnc-commodity-sql.cpp:62-83` | `:52-53` |
| `accounts` | 1 | `gnc-account-sql.cpp:71-93` | `:57-58` |
| `budgets` | 1 | `gnc-budget-sql.cpp:59-68` | `:49-50` |
| `budget_amounts` | 1 | `gnc-budget-sql.cpp:91-…` | `:51-52` |
| `prices` | 3 | `gnc-price-sql.cpp:56-67` | `:50-51` |
| `transactions` | 4 | `gnc-transaction-sql.cpp:85-95` | `:68-69` |
| `splits` | 5 | `gnc-transaction-sql.cpp:104-124` | `:70-71` |
| `slots` | 4 | `gnc-slots-sql.cpp:119-153` | `:54-55` |
| `recurrences` | 2 | `gnc-recurrence-sql.cpp:72-93` | `:48-49` |
| `schedxactions` | 1 | `gnc-schedxaction-sql.cpp:58-82` | `:51-52` |
| `lots` | 2 | `gnc-lots-sql.cpp:58-64` | `:52-53` |
| `billterms` | 2 | `gnc-bill-term-sql.cpp:68-…` | `:65-66` |
| `customers` | 2 | `gnc-customer-sql.cpp:61-…` | `:54-55` |
| `employees` | 2 | `gnc-employee-sql.cpp:60-…` | `:57-58` |
| `entries` | 4 | `gnc-entry-sql.cpp:68-…` | `:57-58` |
| `invoices` | 4 | `gnc-invoice-sql.cpp:62-…` | `:55-56` |
| `jobs` | 1 | `gnc-job-sql.cpp:57-…` | `:50-51` |
| `orders` | 1 | `gnc-order-sql.cpp:57-…` | `:50-51` |
| `taxtables` | 2 | `gnc-tax-table-sql.cpp:72-…` | `:69-70` |
| `taxtable_entries` | 3 | `gnc-tax-table-sql.cpp:94-…` | `:91-92` |
| `vendors` | 1 | `gnc-vendor-sql.cpp:63-…` | `:60-61` |
| `gnclock` | not versioned | hand-written DDL, `5.17:libgnucash/backend/dbi/gnc-dbisqlconnection.cpp:111-114` | — |

All `gnc-*-sql.cpp` paths are under `5.17:libgnucash/backend/sql/`. The `versions` contents of a book GnuCash 5.17
created (experiment E1), in rowid order:

```
Gnucash|5000017
Gnucash-Resave|19920
books|1
commodities|1
accounts|1
budgets|1
budget_amounts|1
prices|3
transactions|4
splits|5
slots|4
recurrences|2
schedxactions|1
lots|2
billterms|2
customers|2
employees|2
entries|4
invoices|4
jobs|1
orders|1
taxtables|2
taxtable_entries|3
vendors|1
```

- `Gnucash` is `gnc_prefs_get_long_version()` = major × 1,000,000 + minor (`5.17:libgnucash/core-utils/gnc-prefs.cpp:122-125`),
  so 5.17 is `5000017`. `Gnucash-Resave` is `GNUCASH_RESAVE_VERSION`, `19920` (`5.17:CMakeLists.txt:37`), unchanged
  since 2011 (commit 7cb436e08d).
- Both are written only when the `versions` table is created (`5.17:libgnucash/backend/sql/gnc-sql-backend.cpp:693-698`)
  and on a full write (`sync`, `:708-716`). An ordinary edit never updates them, so a book created by 4.x and edited
  by 5.17 still says `Gnucash|4000…`.
- Table rows are added in registry order (`:990-1012`) as each table is created (`gnc-sql-object-backend.cpp:74-86`).
  `set_table_version` inserts when it has no row and updates otherwise (`gnc-sql-backend.cpp:754-792`).
- The piecash fixture `fixtures/piecash/simple_sample.sqlite.gnucash` was written by GnuCash 3.0 (`Gnucash|3000001`)
  and has `splits|4`; everything else matches 5.17.

## 3. How values are stored

### 3.1 Strings, integers, booleans

- Strings are written through `dbi_conn_quote_string_copy` (`5.17:libgnucash/backend/dbi/gnc-dbisqlconnection.cpp:538-551`)
  and read as text. A null `char*` leaves the column out of the `INSERT`, so it is NULL
  (`5.17:libgnucash/backend/sql/gnc-sql-column-table-entry.cpp:142-157`); an empty string is `''`.
- Integers and booleans are written as decimal literals (`…:187-193`, `:223-229`). Booleans are `0` or `1`.
- Reading an integer requires libdbi to report the column as `DBI_TYPE_INTEGER`, or the value is skipped
  (`5.17:libgnucash/backend/dbi/gnc-dbisqlresult.cpp:101-108`). libdbi's sqlite3 driver decides the type from the
  declared column type; this note didn't check the driver's source.

### 3.2 GUIDs

32 lower-case hex digits from `guid_to_string`, quoted (`gnc-sql-column-table-entry.cpp:350-364`). Read with
`string_to_guid`; an unparsable value is skipped silently (`:325-341`). Object references (`CT_COMMODITYREF`,
`CT_ACCOUNTREF`, `CT_TXREF`, `CT_LOTREF`, `CT_BUDGETREF`, …) are the referenced object's GUID in the same form
(`:95-114`).

### 3.3 Numerics

Two `bigint` columns, `<name>_num` and `<name>_denom`, written as decimal literals of `gnc_numeric_num` and
`gnc_numeric_denom`, unreduced (`gnc-sql-column-table-entry.cpp:593-634`). On read both halves must be integers;
if either is missing the setter is never called (`:551-576`). The Numeric columns in 5.17:

| Table | Columns |
|---|---|
| `splits` | `value_num/value_denom` (transaction currency), `quantity_num/quantity_denom` (engine param `amount`, `gnc-transaction-sql.cpp:119-120`) |
| `prices` | `value_num/value_denom` |
| `budget_amounts` | `amount_num/amount_denom` |
| `slots` | `numeric_val_num/numeric_val_denom` |
| `billterms` | `discount_num/discount_denom` (nullable) |
| `customers` | `discount_num/_denom`, `credit_num/_denom` |
| `employees` | `workday_num/_denom`, `rate_num/_denom` |
| `entries` | `quantity_num/_denom`, `i_price_num/_denom`, `i_discount_num/_denom`, `b_price_num/_denom` (all nullable) |
| `invoices` | `charge_amt_num/_denom` (nullable) |
| `taxtable_entries` | `amount_num/amount_denom` |

The split column is called `quantity` but the engine property is `amount`: the glossary clash *amount* = split
quantity applies to SQL column names too.

### 3.4 Timestamps (`CT_TIME`, `text(19)`)

- **Written** as `'YYYY-MM-DD HH:MM:SS'` in UTC: `GncDateTime::format_iso8601()`, which takes
  `to_iso_extended_string` of the UTC time, puts a space at index 10 and cuts to 19 characters
  (`5.17:libgnucash/engine/gnc-datetime.cpp:630-636`), quoted (`gnc-sql-column-table-entry.cpp:444-450`). A time
  outside `(MINTIME, MAXTIME)` is written as `NULL` (`:451-455`). Time 0 is inside, so an unset reconcile date is
  `'1970-01-01 00:00:00'`, not NULL (experiment E2).
- **Read** as a string and parsed with `GncDateTime(const std::string&)`. An empty string leaves 0; an unparsable
  one logs `An invalid date … It has been set to 1 January 1970.` and leaves 0 (`:370-412`). A non-string column
  type falls back to libdbi's datetime (`:395-399`), which only MySQL and PostgreSQL produce.
- **Forms the parser accepts** (`5.17:libgnucash/engine/gnc-datetime.cpp:381-416`, `:432-485`):
  - `YYYY-MM-DD HH:MM:SS +0000` (25 characters), UTC;
  - `YYYYMMDDHHMMSS` (14 digits), UTC;
  - otherwise the regexes `^\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}(\.\d{0,9})?\s*([+-]\d{2}(:?\d{2})?)?$` and
    `^\d{14}(\.\d{0,9})?\s*([+-]\d{2}\s*(:?\d{2})?)?$` (`:444-445`). A missing offset means UTC. An offset under an
    hour is discarded as a Windows GLib bug (`:476-484`).
- **The older form** is `YYYYMMDDHHMMSS`. GnuCash 2.6 wrote SQLite timestamps with
  `TIMESPEC_STR_FORMAT "%04d%02d%02d%02d%02d%02d"`; commit 44c51f433b (2017-12-23, "Bug 791848", released in 2.6.20)
  taught 2.6 to read the ISO form and added the Book feature. That is from git history, not the 5.17 tree. 5.17
  reads both through the parser above.
- **Nothing converts old timestamps.** Loading sets the Book feature (§3.5) but rewrites no rows; a compact
  timestamp stays until GnuCash rewrites that row for some other reason (experiment E7).

### 3.5 The ISO-dates Book feature

- Name `ISO-8601 formatted date strings in SQLite3 databases.` (`5.17:libgnucash/engine/gnc-features.h:60`),
  description `Use ISO formatted date-time strings in SQLite3 databases (requires at least GnuCash 2.6.20)`
  (`5.17:libgnucash/engine/gnc-features.cpp:43`).
- Set in `GncDbiBackend<Type>::load` when `Type == DBI_SQLITE`, after the generic load and before the version checks
  (`5.17:libgnucash/backend/dbi/gnc-backend-dbi.cpp:877-880`). It is set on every load, for every session mode.
- `qof_book_set_feature` does nothing if the value is already there; otherwise it begins an edit, sets the slot,
  marks the book dirty and commits (`5.17:libgnucash/engine/qofbook.cpp:1272-1289`). `m_loading` is false by then
  (`5.17:libgnucash/backend/sql/gnc-sql-backend.cpp:340`), so the commit writes the book's slots to the database.
- Not set by `sync`: an XML book saved as SQLite has no such slot until something loads it (experiment E1).
- In the database: a `features` frame row on the book GUID and a string row under it (experiment E3):

```
slots 78|78|'84a3c7eb…'|'features'|9|0|NULL|NULL|'1970-01-01 00:00:00'|'0a530cdf…'|0|1|NULL
slots 79|79|'0a530cdf…'|'features/ISO-8601 formatted date strings in SQLite3 databases.'|4|0|'Use ISO formatted date-time strings in SQLite3 databases (requires at least GnuCash 2.6.20)'|NULL|'1970-01-01 00:00:00'|NULL|0|1|NULL
```

### 3.6 Dates (`CT_GDATE`, `text(8)`)

Written as `'YYYYMMDD'`, zero-padded, only if the GDate is valid; otherwise the column is left out (NULL)
(`gnc-sql-column-table-entry.cpp:512-529`). Read by `stoi` on characters 0–3, 4–5 and 6–7 with no other
validation; NULL or empty leaves the date unset, `00000000` gives a cleared date (`:461-503`).

### 3.7 Slots (`slots`, version 4)

Columns (`5.17:libgnucash/backend/sql/gnc-slots-sql.cpp:119-153`): `id` (autoincrement), `obj_guid` (the owning
object, or the GUID of a parent frame or list), `name` (full path), `slot_type`, then one value column per type.
`slot_type` is `KvpValue::Type` (`5.17:libgnucash/engine/kvp-value.hpp:55-67`):

| `slot_type` | Type | Value column |
|---|---|---|
| 1 | INT64 | `int64_val` |
| 2 | DOUBLE | `double_val` |
| 3 | NUMERIC | `numeric_val_num`, `numeric_val_denom` |
| 4 | STRING | `string_val` |
| 5 | GUID | `guid_val` |
| 6 | TIME64 | `timespec_val` |
| 7 | (placeholder, formerly binary) | none |
| 8 | GLIST | `guid_val` = GUID of the list |
| 9 | FRAME | `guid_val` = GUID of the frame |
| 10 | GDATE | `gdate_val` |

- **Every row fills the unused columns the same way:** `int64_val` 0, `timespec_val` `'1970-01-01 00:00:00'`,
  `numeric_val_num` 0, `numeric_val_denom` 1, and NULL for `string_val`, `double_val`, `guid_val` and `gdate_val`
  unless that is the slot's type. The getters return 0, Time64 0 and `gnc_numeric_zero` for other types
  (`gnc-slots-sql.cpp:290-305`, `:379-389`, `:485-500`); null pointers leave the column out. Experiment E2's insert
  shows it: `INSERT INTO slots(obj_guid,name,slot_type,int64_val,timespec_val,numeric_val_num,numeric_val_denom,gdate_val) VALUES(…,'date-posted',10,0,'1970-01-01 00:00:00',0,1,'20240603')`.
  Reading ignores the filler: each setter checks `value_type` first (`:315`, `:343`, `:374`, `:399`, `:509`, `:539`).
- **Frames and lists:** a frame or list is one row whose `guid_val` is a new random GUID; its children are rows
  with `obj_guid` = that GUID (`:580-620`). A child's `name` is the parent path, `/`, then its key; list items have
  key `""`, so each list item is named `<list>/` and a frame inside a list `<list>//<key>` (experiment E1):

```
33|c080059e…|zz-list|8|0|NULL|NULL|'1970-01-01 00:00:00'|'1c0d82ec…'|0|1|NULL
34|1c0d82ec…|zz-list/|1|3|NULL|NULL|'1970-01-01 00:00:00'|NULL|0|1|NULL
35|1c0d82ec…|zz-list/|4|0|'two'|NULL|'1970-01-01 00:00:00'|NULL|0|1|NULL
36|1c0d82ec…|zz-list/|9|0|NULL|NULL|'1970-01-01 00:00:00'|'17a514a6…'|0|1|NULL
37|17a514a6…|zz-list//k|1|1|NULL|NULL|'1970-01-01 00:00:00'|NULL|0|1|NULL
```

- **Write order:** a frame's slots in its map order (`for_each_slot_temp`; byte order, so `Zupper` sorts before
  `a/b`), depth-first; list items in list order (`:611-615`). Ids ascend in that order.
- **Keys containing `/`** (the fixture's `a/b`) are stored as one name and read back as one key: top-level slots
  are set by their key with `set({key})` (`:197-231`). They survived every experiment here.
- **Every commit rewrites the object's slots:** unless the database is pristine or the object is new, `gnc_sql_slots_save`
  first calls `gnc_sql_slots_delete` (`:645-649`), which recursively deletes child frames and lists (by
  `slot_type in (9, 8)`) and then `DELETE FROM slots WHERE obj_guid = …` (`:658-697`). Then all slots are inserted
  again. Ids are therefore not stable, and frame and list GUIDs change on every commit.
- **Loading:** `SELECT * FROM slots WHERE obj_guid IN (SELECT DISTINCT guid FROM <table>)` per object type
  (`:808-832`), then one `SELECT * FROM slots WHERE obj_guid='<frame guid>'` per frame or list (`:742-761`).
  A row whose `obj_guid` matches no loaded object is skipped ("Silently bail", `:789-790`). No query has `ORDER BY`.

### 3.8 Doubles

- `double_val` is `float8`: REAL affinity, stored as an 8-byte IEEE double (`typeof(double_val)` is `real` for every
  double slot in experiment E1, including text written into it by another program).
- **Written** as `std::setprecision(12) << std::fixed` (`5.17:libgnucash/backend/sql/gnc-sql-column-table-entry.hpp:403-416`),
  so a double keeps only 12 digits after the point: `0.12345678901234567` → `0.123456789012`,
  `1.234567890123e-13` → `0.0`.
- **Read** through libdbi as a double (`5.17:libgnucash/backend/dbi/gnc-dbisqlresult.cpp:126-138`), or as an integer
  or 4-byte float if libdbi reports those (`gnc-sql-column-table-entry.cpp:287-306`). In experiment E11 values came
  back with about 15 significant digits (`1.2345678901234567e+19` → `1.23456789012346e+19`), which fits libdbi
  reading SQLite's text form of the REAL. This is observed; the libdbi source wasn't checked.
- So GnuCash changes a double slot whenever it rewrites the object, and no exact text exists to keep. The XML
  original `0.100000000000000006` is gone once the book is in SQL.

### 3.9 Other per-table encodings worth knowing

- `splits.reconcile_state` is one character, `text(1)` (`5.17:libgnucash/backend/sql/gnc-transaction-sql.cpp:114-116`).
- `accounts.account_type` is the type name string (`ACCOUNT_TYPE_` param, `gnc-account-sql.cpp:76-77`).
- Template accounts are ordinary `accounts` rows under `books.root_template_guid`; `sync` writes the template tree
  after the main tree (`5.17:libgnucash/backend/sql/gnc-sql-backend.cpp:393-405`).
- `recurrences` belong to budgets and scheduled transactions through `obj_guid`, loaded per owner
  (`gnc-recurrence-sql.cpp:314`).
- **No deferred instances.** `schedxactions` has no column for an SX's deferred instances (`gnc-schedxaction-sql.cpp:58-82`),
  and nothing in `libgnucash/backend/sql/` mentions them; the XML backend writes them as `sx:deferredInstance`
  (`5.17:libgnucash/backend/xml/gnc-schedxaction-xml-v2.cpp:66`, `:169`). Experiment E1b shows them lost.

## 4. The lock (`gnclock`)

### 4.1 Taking it (`GncDbiSqlConnection::lock_database`, `5.17:libgnucash/backend/dbi/gnc-dbisqlconnection.cpp:101-169`)

Called from the connection's constructor unless the mode is `SESSION_READ_ONLY` (`:90-93`):

```
BEGIN
if no table named gnclock:
    CREATE TABLE gnclock ( Hostname varchar(255), PID int )       # :111-114, GNC_HOST_NAME_MAX = 255
SELECT * FROM gnclock
if any row:                                                       # :132
    if not break_lock: set ERR_BACKEND_LOCKED; ROLLBACK; fail     # :136-142
    DELETE FROM gnclock                                           # :143, every row, not just the holder's
INSERT INTO gnclock VALUES ('<gethostname()>', '<getpid()>')      # :155-159
COMMIT
```

- The host name is `gethostname` into a zeroed 256-byte buffer, so at most 255 bytes (`:129`, `:155-156`;
  `5.17:libgnucash/backend/dbi/gnc-backend-dbi.hpp:41`). The PID is `getpid()` cast to `int` and written as a
  quoted string; INTEGER affinity stores it as an integer. Oracle: `gnclock 1|'beast'|442380`.
- The host name is inserted with `%s` into a quoted literal, not escaped.
- A failed lock throws, the constructor's caller catches it and returns, and the error stays set
  (`5.17:libgnucash/backend/dbi/gnc-backend-dbi.cpp:444-452`). `ERR_BACKEND_LOCKED` is 7.
- Any row counts as the lock. There is no check of host, PID or age.
- If creating `gnclock` fails, the function returns without a rollback (`:120-125`).

### 4.2 Releasing it (`unlock_database`, `:171-227`)

Runs from the connection's destructor (`:247-255`), that is when the session ends (`5.17:libgnucash/backend/dbi/gnc-backend-dbi.cpp:832-841`)
or the backend is replaced. It does nothing in read-only mode (`:175`). Otherwise, in one transaction, it selects
rows `WHERE Hostname = '<host>' AND PID = '<pid>'`. If there is one it deletes **every** row (`:204-205`) and commits;
if not it rolls back and warns `There was no lock entry in the Lock table`. If there is no `gnclock` table it warns
and returns.

### 4.3 Modes (experiments E4–E6)

| Mode, with a row `('otherhost', 4242)` present | Result |
|---|---|
| `SESSION_NORMAL_OPEN` | `ERR_BACKEND_LOCKED` (7) from `begin`; file unchanged |
| `SESSION_READ_ONLY` | opens with no error; the row is left; nothing locked, nothing unlocked |
| `SESSION_BREAK_LOCK` | the row is deleted, ours inserted, and ours deleted at `end()`; the table is left empty |

The GUI's lock dialog offers "Open Read-Only" (`SESSION_READ_ONLY`) and "Open Anyway" (`SESSION_BREAK_LOCK`)
(`5.17:gnucash/gnome-utils/gnc-file.c:975-982`). After "Open Read-Only" it marks the book read-only, but only after
the load (`:1045-1050`), so load-time writes (§5.4) still happen.

### 4.4 Read-only sessions

`SESSION_READ_ONLY` only sets `m_readonly`, which skips lock and unlock (`gnc-dbisqlconnection.cpp:90-91`, `:175`).
It doesn't mark the book read-only (oracle: `qof_book_is_readonly` returns 0). `GncSqlBackend::commit` refuses
writes only when the *book* is read-only (`5.17:libgnucash/backend/sql/gnc-sql-backend.cpp:589-594`), so an edit
in a read-only session is written, even under another process's lock (experiment E10). The Python docstring says
the mode "will also set a flag on the book that will prevent many elements from being edited"
(`5.17:bindings/python/gnucash_core.py:345-348`); the backend doesn't do that.

## 5. Opening and loading

### 5.1 Sequence

1. `session_begin` (§1), which constructs the connection: lock (§4), then `check_and_rollback_failed_save` (§5.2)
   (`5.17:libgnucash/backend/dbi/gnc-dbisqlconnection.cpp:79-99`).
2. `QofSessionImpl::load` → `GncDbiBackend::load(LOAD_TYPE_INITIAL_LOAD)` (`5.17:libgnucash/engine/qofsession.cpp:198-248`,
   `5.17:libgnucash/backend/dbi/gnc-backend-dbi.cpp:861-900`):
   1. `init_version_info`: `SELECT * FROM versions`, or create the table with the two GnuCash rows if it is missing
      (`5.17:libgnucash/backend/sql/gnc-sql-backend.cpp:676-699`);
   2. `create_tables` for every object backend in registry order (`:211-219`, §7.1);
   3. `GncSqlBackend::load` (`:284-363`), below;
   4. set the ISO-dates feature (§3.5);
   5. the `Gnucash`/`Gnucash-Resave` checks (§7.2).
3. `GncSqlBackend::load`: with `m_loading` true, objects are loaded and every commit just marks them clean
   (`:597-601`). Order: Book, Commodity, Account, Lot, Transaction (`:222-223`), then Bill Term, Tax Table,
   Invoice (`:226-227`), then the rest in registry order (`:230-255`). Oracle trace (experiment E6): `versions`,
   `books`, `commodities`, `accounts`, `lots`, `transactions`, splits by `INNER JOIN`, `billterms`, `taxtables`,
   `invoices`, `budgets` (with `budget_amounts` and `recurrences` per budget), `prices`, `schedxactions` (template
   transactions per template account), `customers`, `employees`, `entries`, `jobs`, `orders`, `vendors`, each
   followed by its slots.
4. With `m_loading` false again: commodities loaded dirty are begun and committed (`:341-346`;
   `gnc-commodity-sql.cpp:158-159`), then every transaction gets `xaccTransBeginEdit` / `xaccTransCommitEdit`
   with the journal disabled (`:351-354`, `:275-281`). Commits write only dirty objects (`:621-625`), but
   `xaccTransCommitEdit` scrubs imbalance (`5.17:libgnucash/engine/Transaction.cpp:1549-1566`), so an unbalanced
   transaction gets an Imbalance split, and maybe a new Imbalance account, written during the load (experiment E9).
5. `qof_book_mark_session_saved` (`:359`).

Load errors other than `ERR_SQL_DB_TOO_OLD`, `ERR_SQL_DB_TOO_NEW` and three XML ones destroy the backend and the
book (`5.17:libgnucash/engine/qofsession.cpp:231-245`). Too old and too new leave the book loaded, with the error.

### 5.2 Recovering a failed safe-save (`5.17:libgnucash/backend/dbi/gnc-dbisqlconnection.cpp:229-245`)

On every open, read-only included, if any table matches `LIKE '%back'`:

- tables matching `%_merge` → fail: `A failed safe-save was detected and rolling it back failed.` (`:236-243`, `:94-98`);
- otherwise `table_operation(recover)` (`:717-733`): for each table matching `%_back`, if the plain-named table
  exists, `CREATE TABLE <t>_merge AS SELECT * FROM <t> UNION SELECT * FROM <t>_back`, empty `<t>`, copy the merge
  back, drop the merge table and the backup (`:621-643`); if not, rename `<t>_back` to `<t>`.

In `LIKE`, `_` matches any character, so a user table such as `feedback` also matches `%_back`. Experiment E8: a
table `zz_back` was renamed to `zz` by a `SESSION_READ_ONLY` open.

### 5.3 Journal on load

The GUI disables the journal around `qof_session_load` (`5.17:gnucash/gnome-utils/gnc-file.c:1039-1043`). The
Python bindings don't, so every oracle load writes a `.log` with a `C` record for each transaction committed while
loading (experiment E3: 63 lines for 8 transactions). This is an oracle artefact; tests must not compare journals
made by the bindings with the GUI's.

### 5.4 Everything a load can write

None of these checks the session mode or the book's read-only flag.

| Write | Source | Experiment |
|---|---|---|
| Lock row insert, then delete at end (normal and break-lock only) | §4 | E3: rows identical, file change counter +2 |
| Recovery of `%_back` tables | `gnc-dbisqlconnection.cpp:94`, `:229-245` | E8 |
| `versions` created with the two GnuCash rows, if missing | `gnc-sql-backend.cpp:693-698` | — |
| Missing tables created, with their `versions` row and indexes | `gnc-sql-object-backend.cpp:77-82` and overrides | E5 |
| Older tables rebuilt by `upgrade_table`, `versions` updated | §7.1 | E4 |
| Dirty commodities committed | `gnc-sql-backend.cpp:341-346` | — |
| Scrubbed transactions (Imbalance splits and accounts) | `gnc-sql-backend.cpp:351-354` | E9 |
| The ISO-dates Book feature, if absent | `gnc-backend-dbi.cpp:879-880` | E3 |

## 6. Writing

### 6.1 Each commit writes through (`GncSqlBackend::commit`, `5.17:libgnucash/backend/sql/gnc-sql-backend.cpp:580-666`)

```
if book is read-only: ERR_BACKEND_READONLY (9); ROLLBACK; return      # :589-594
if m_loading: mark clean; return                                       # :597-601
if type is "PriceDB": mark clean; return                               # :604-609
if not dirty and not being destroyed: return                           # :621-625
BEGIN (or SAVEPOINT savepoint_<n> when nested)                          # :627
object backend's commit                                                # :636-638
error: ROLLBACK, leave dirty | unknown type: ROLLBACK, mark clean      # :639-658
COMMIT; mark session saved; mark clean                                 # :660-663
```

- `GncSqlObjectBackend::commit` picks `DELETE` if destroying, `INSERT` if the database is pristine or the object is
  new (infant), else `UPDATE … WHERE guid = …`, then saves or deletes the slots
  (`5.17:libgnucash/backend/sql/gnc-sql-object-backend.cpp:33-71`).
- An `UPDATE` sets every column the backend knows, the primary key included, and nothing else
  (`gnc-sql-backend.cpp:932-964`). Columns it doesn't know keep their values (experiment E7, U1).
- Transactions (`5.17:libgnucash/backend/sql/gnc-transaction-sql.cpp:584-681`) first make sure the currency is in
  `commodities` (`SELECT guid FROM commodities WHERE guid = …`, inserting it if absent, `gnc-sql-backend.cpp:884-893`),
  then write the row and its slots. Deleting a transaction deletes its slots, every `splits` row with its
  `tx_guid`, and each split's slots (`:512-532`).
- Splits are separate objects with their own commits (`:541-581`); a split with a null GUID gets a new one
  (`:566-570`).
- Nested `BEGIN`s become `SAVEPOINT savepoint_<n>`, released or rolled back by depth
  (`5.17:libgnucash/backend/dbi/gnc-dbisqlconnection.cpp:334-443`).

Experiment E2 (the description of one transaction changed), the whole SQL trace:

```
BEGIN
SQL: SELECT guid FROM commodities WHERE guid = '961203d9…'
SQL: UPDATE transactions SET guid='bf5e53a9…',currency_guid='961203d9…',num='',post_date='2024-06-03 10:59:00',enter_date='2024-06-30 12:00:00',description='To euro account (edited)' WHERE guid = 'bf5e53a9…'
SQL: SELECT * FROM slots WHERE obj_guid='bf5e53a9…' and slot_type in ('9', '8') and not guid_val is null
SQL: DELETE FROM slots WHERE obj_guid = 'bf5e53a9…'
SQL: INSERT INTO slots(obj_guid,name,slot_type,int64_val,timespec_val,numeric_val_num,numeric_val_denom,gdate_val) VALUES('bf5e53a9…','date-posted',10,0,'1970-01-01 00:00:00',0,1,'20240603')
COMMIT
```

Row diff: the `transactions` row changed in place (same rowid); slot 44 deleted and the same slot inserted as 80;
`sqlite_sequence` for `slots` went 79 → 80. Nothing else.

Experiment E2b (both splits of a two-split transaction doubled in value and quantity):

```
BEGIN
SQL: UPDATE splits SET guid='be0e9ff1…',tx_guid='f485efcd…',account_guid='c080059e…',memo='',action='',reconcile_state='y',reconcile_date='2024-06-30 00:00:00',value_num=1000000,value_denom=100,quantity_num=10…
SQL: SELECT * FROM slots WHERE obj_guid='be0e9ff1…' and slot_type in ('9', '8') and not guid_val is null
SQL: DELETE FROM slots WHERE obj_guid = 'be0e9ff1…'
COMMIT
BEGIN
SQL: UPDATE splits SET guid='80dafcc1…',tx_guid='f485efcd…',account_guid='8839ad3e…',memo='',action='',reconcile_state='n',reconcile_date='1970-01-01 00:00:00',value_num=-1000000,value_denom=100,quantity_num=-…
SQL: SELECT * FROM slots WHERE obj_guid='80dafcc1…' and slot_type in ('9', '8') and not guid_val is null
SQL: DELETE FROM slots WHERE obj_guid = '80dafcc1…'
COMMIT
```

Two SQL transactions for one GnuCash transaction, and no write of the `transactions` row.

### 6.2 Save, Save As and safe-save

- **`qof_session_save`** returns at once on a clean book (`5.17:libgnucash/engine/qofsession.cpp:439-440`), which an
  SQL book always is, since every commit marks the session saved (`gnc-sql-backend.cpp:662`). The GUI's Save on an
  SQL book therefore does nothing (`5.17:gnucash/gnome-utils/gnc-file.c:1486`). `sync` on an existing database
  would fail: it assumes an empty database (below).
- **Save As** (`gnc-file.c:1631-1729`): load everything (`qof_session_ensure_all_data_loaded`), begin a new session
  with `SESSION_NEW_STORE` (asking before `SESSION_NEW_OVERWRITE`), `qof_session_swap_data`, mark the book dirty
  (`:1722`), then `qof_session_save` (`:1729`) → `GncSqlBackend::sync` (`5.17:libgnucash/backend/sql/gnc-sql-backend.cpp:475-540`):
  `reset_version_info` (create `versions`, write the two GnuCash rows), mark the database pristine, create every
  table, then in one SQL transaction insert the book, the account tree, the template account tree, the
  transactions, the template transactions, the SXs and each object backend's `write()`; commit, or roll back and
  set `ERR_BACKEND_SERVER_ERR`.
- **`safe_sync`** (`5.17:libgnucash/backend/dbi/gnc-backend-dbi.cpp:945-982`), the GUI's upgrade for a too-old
  database (§7.2): in one SQL transaction, rename every table except `gnclock` to `<t>_back`
  (`5.17:libgnucash/backend/dbi/gnc-dbisqlconnection.cpp:685-695`), drop every index except SQLite's autoindexes
  (`:738-753`; `gnc-dbiproviderimpl.hpp:290-309`), `sync`, commit; then `drop_backup`, which drops each `<t>_back`
  whose `<t>` exists and renames back the ones whose `<t>` doesn't (`gnc-dbisqlconnection.cpp:696-706`). Experiment
  S1 ran it on a book with unknown content:
  - an unknown table survived (renamed away and back, now `CREATE TABLE "zz_custom"` with quotes);
  - an unknown column of `transactions` was lost, with its values;
  - a user index (`zz_idx`) was lost;
  - a `versions` row for the unknown table was lost; `Gnucash` became `5000017`;
  - an orphan slot row was lost;
  - NULL `cusip` and `quote_source` became `''`;
  - every slot id and frame GUID changed, nested lists changed (§9), and a double lost precision.

## 7. Schema versions

### 7.1 Per-table versions: upgraded or ignored, silently

Each object backend's `create_tables` runs on every initial load (`gnc-backend-dbi.cpp:868-875`). The base version
(`5.17:libgnucash/backend/sql/gnc-sql-object-backend.cpp:74-86`) creates a table whose version is 0 (no `versions`
row) and logs `Version mismatch in table …` for any other difference, changing nothing. The overrides for
`billterms`, `lots`, `recurrences`, `customers`, `budgets`, `prices`, `employees`, `slots`, `invoices`,
`transactions`, `splits` and others upgrade older tables with `upgrade_table` (`gnc-sql-backend.cpp:795-816`:
`CREATE TABLE <t>_new`, `INSERT INTO <t>_new SELECT * FROM <t>`, `DROP TABLE <t>`, `ALTER TABLE <t>_new RENAME TO <t>`),
recreate indexes and update `versions`. None of it runs in an SQL transaction. The slots upgrade from 2 adds
`gdate_val` with `ALTER TABLE … ADD COLUMN` instead (`gnc-slots-sql.cpp:872-879`). Newer versions are not handled
by most overrides: `splits` at 6 loads with no message (experiment E4b).

Which upgrades change SQLite DDL: the comments in the overrides say 3→4 for slots and 4→5 for splits are
"Use DATETIME instead of TIMESTAMP in MySQL" (`gnc-slots-sql.cpp:860`, `gnc-transaction-sql.cpp:465`). For SQLite
the table is rebuilt with the same columns. Splits became version 5 in commit e88f31e05c (2019-07-19), first
released in 3.7.

Experiment E4 (`splits` set to 4, `SESSION_READ_ONLY` open), traced:

```
SQL: CREATE TABLE splits_new(guid text(32) PRIMARY KEY NOT NULL, tx_guid text(32) NOT NULL, …
SQL: INSERT INTO splits_new SELECT * FROM splits
SQL: DROP TABLE splits
SQL: ALTER TABLE splits_new RENAME TO splits
SQL: CREATE INDEX splits_tx_guid_index ON splits(tx_guid)
SQL: CREATE INDEX splits_account_guid_index ON splits(account_guid)
SQL: UPDATE versions SET table_version=5 WHERE table_name='splits'
```

The real piecash fixture (`splits|4`) gets the same rebuild from a `SESSION_READ_ONLY` open (experiment E12).
After the rename, `sqlite_master` holds `CREATE TABLE "splits"(…)`, with quotes, at the end of the schema.

`INSERT … SELECT *` copies by position, so an upgrade of a table with an extra column fails or misplaces data; the
errors are not checked (`gnc-sql-backend.cpp:801-815`).

### 7.2 The database-level checks: too old, too new (`5.17:libgnucash/backend/dbi/gnc-backend-dbi.cpp:852-896`)

- `GNUCASH_RESAVE_VERSION (19920) > versions['Gnucash']` → `ERR_SQL_DB_TOO_OLD` (3001). Every GnuCash since 2.x
  writes at least 2,000,000, so this needs a book from before 2.0 or a missing `Gnucash` row (version 0).
- `GNUCASH_RESAVE_VERSION < versions['Gnucash-Resave']` → `ERR_SQL_DB_TOO_NEW` (3002).
- Both are set after the load and the feature write; the book stays loaded (`qofsession.cpp:231-237`).
- Experiments E4c/E4d: `Gnucash-Resave` 19921 gives `[3002]`; `Gnucash` 19919 gives `[3001]`. The library opens
  both without marking the book read-only.

GUI (`5.17:gnucash/gnome-utils/gnc-file.c:1072-1090`, messages `:519-535`): too old asks "This database is from an
older version of GnuCash. Select OK to upgrade it to the current version, Cancel to mark it read-only." OK runs
`qof_session_safe_save` (`:1074-1080`); Cancel, a failed upgrade, or too new marks the book read-only (`:1085-1090`).
Too new warns "It will be marked read-only until you do File->Save As, but data may be lost in writing to the old
version." There is no SQL counterpart of the XML backend's file-version refusal.

## 8. Backups and journal

- **No backup files.** Neither `libgnucash/backend/dbi/` nor `libgnucash/backend/sql/` copies, links or renames the
  file; the only "backup" is `safe_sync`'s `<t>_back` tables, dropped when it succeeds (§6.2). There is no
  retention cleanup for SQL books: `remove_old_files` is the XML backend's
  ([`storage-and-safety.md`](storage-and-safety.md) §7).
- **Journal:** the SQLite `session_begin` sets the journal base name to the book's path
  (`5.17:libgnucash/backend/dbi/gnc-backend-dbi.cpp:456`). The file is opened on the first journal record, named
  `<base>.<gnc_date_timestamp()>.log` (`5.17:libgnucash/engine/TransLog.cpp:158-196`, `:174-176`), so
  `/path/book.gnucash.20261027171404.log`, local time, beside the book. Format and lifecycle are as for XML
  ([`storage-and-safety.md`](storage-and-safety.md) §8). MySQL and PostgreSQL journals go under the user data
  directory, named from `<protocol>_<host>_<user>_<dbname>` (`gnc-backend-dbi.cpp:155-159`, `:769-772`).
- The GUI's Save still ends with `xaccReopenLog()` (`5.17:gnucash/gnome-utils/gnc-file.c:1506`), which closes an
  open journal and starts a new one (`5.17:libgnucash/engine/TransLog.cpp:108-116`), even though nothing was
  written; Save As does the same (`gnc-file.c:1756`). So an SQL book's journals are split at each GUI Save, and
  every change in them is already in the database.

## 9. Row order, unknown content, and what "lossless" means here

- **Load order:** every load is `SELECT *` with no `ORDER BY` (all `load_all` functions in
  `libgnucash/backend/sql/`; experiment E6's trace). SQLite returns rowid order for a plain table scan. Splits come
  from `SELECT splits.* FROM splits INNER JOIN transactions ON splits.tx_guid = transactions.guid`
  (`5.17:libgnucash/backend/sql/gnc-transaction-sql.cpp:256-262`), whose order is up to SQLite's planner.
- **Where order matters to GnuCash:** a KVP list's item order is the order its rows come back (`g_list_append`,
  `gnc-slots-sql.cpp:211-215`); a transaction's split order is the order its splits load. Frames are maps and
  ignore order. Accounts, prices, lots and the rest are found by GUID.
- **Write order:** new rows go to the end of their table (rowid, and `AUTOINCREMENT` ids for slots, recurrences,
  budget amounts and tax table entries). `UPDATE`s keep rowids. Slot rewrites move an object's slots to the end.
- **Unknown tables** are never read, so they survive loads and edits (experiment U1). A table whose name ends in
  `back` or matches `%_merge` is treated as a failed safe-save (§5.2). `safe_sync` renames unknown tables away and
  back (§6.2).
- **Unknown columns of known tables** survive `UPDATE`s (experiment U1: `zz_extra` kept its value through an edit).
  A new row gets NULL there, or fails if the column is `NOT NULL` without a default. `upgrade_table` and
  `safe_sync` drop them.
- **Unknown rows** (a slot whose `obj_guid` matches nothing loaded) are skipped on load and left alone by edits
  (U1). Deleting the object they belong to wouldn't reach them; `safe_sync` drops them.
- **Unknown `versions` rows** are loaded into the version list and ignored (U1); `sync` drops them.
- **Nested KVP lists aren't stable.** Experiment S1 and E1b: after a GnuCash rewrite, the frame inside `zz-list`
  also appeared as a top-level slot `k`, list items appeared again under an empty name, and an empty nested list
  was lost. SQL→XML (E1b) shows the same:

```
>       <slot:key></slot:key>
>       <slot:value type="list">
>         <slot:value type="integer">3</slot:value>
…
<         <slot:value type="list"/>
```

## 10. Experiments

Scripts (left in the scratch directory, not committed):

| Script | What it does |
|---|---|
| `xml_to_sqlite.py`, `convert.py` | Save As between formats with the GUI's recipe (§10.1) |
| `open_close.py` | open in a given mode, load, `end()` |
| `edit_desc.py`, `edit_amount.py`, `edit_account.py` | one edit, with SQL traced at DEBUG |
| `scenario.py`, `run_scn.sh` | copy a base book, mutate it with `sqlite3`, open in a mode, optionally edit, then diff |
| `safe_save.py` | open normally and call `qof_session_safe_save` |
| `dump.py` | every table's rows (rowid first, `quote()` of each column) plus `sqlite_master`, for diffs |
| `bytediff.py` | differing bytes, by page, and the SQLite header fields |

SQL tracing: `gnucash_core_c.qof_log_init_filename_special("stderr")` and
`qof_log_set_level("gnc.backend.dbi", QOF_LOG_DEBUG)` print `GncDbiSqlConnection`'s `DEBUG ("SQL: %s")` lines.
The lock statements use `dbi_conn_queryf` directly and are not traced.

### 10.1 E1: building an SQLite book from an XML one (the fixture builder)

The bindings have `Session.swap_data` and `Session.save`, but not `qof_book_mark_session_dirty`, and `save` is a
no-op on the clean book swapped in (the first attempt wrote an 8 KiB file holding only `gnclock`).
`qof_session_safe_save` should not work either, from the code: `safe_sync` writes the backend's own `m_book`, which
is null in a session that never loaded (`gnc-backend-dbi.cpp:972`; `gnc-sql-backend.cpp:477` returns early); not
run. The recipe that works mirrors `gnc-file.c:1640-1729`:

```python
import ctypes, os
from gnucash import Session, SessionOpenMode, _sw_core_utils
assert _sw_core_utils.gnc_version() == "5.17"
engine = ctypes.CDLL(os.path.expanduser("~/.local/opt/gnucash-5.17/lib/libgnc-engine.so"))
engine.qof_book_mark_session_dirty.argtypes = [ctypes.c_void_p]

old = Session("xml://" + src, SessionOpenMode.SESSION_READ_ONLY)   # begin + load, no lock
new = Session()
new.begin("sqlite3://" + dst, SessionOpenMode.SESSION_NEW_STORE)
new.swap_data(old)                                                 # qof_session_swap_data
engine.qof_book_mark_session_dirty(int(new.book.instance))         # as gnc-file.c:1722
new.save()                                                         # -> GncSqlBackend::sync
assert new.pop_all_errors() == ()
new.end(); old.end()                                               # not destroy(): see the top of this note
```

The same script with the schemes swapped converts SQLite to XML. Results on `fixtures/synthetic/rich.gnucash`:
221,184 bytes, 8 transactions, 17 splits, 77 slots, an empty `gnclock`, no journal (nothing was edited), and **no
ISO-dates feature**; the first load adds it (E3). For a fixture in the state GnuCash 5.17 leaves it, the builder
then opens the file once with `SESSION_READ_ONLY` and `end()`, and deletes the `.log` the bindings leave beside it.

E1b, the same book back to XML and diffed with the original (40 diff lines): the ISO-dates feature added to
`book:slots`; the nested list changed (§9); `1.23456789012345672e+19` became `1.23456789012345999e+19`; and the
SX's `sx:deferredInstance` (`last` 2024-09-01, `rem-occur` 9, `instanceCount` 3) was gone (§3.9).

### 10.2 E3: open and close with no edits (book from E1, no feature yet)

```
c14f4215…  orig.gnucash
c14f4215…  ro.gnucash
c14f4215…  rw.gnucash
(open ro.gnucash SESSION_READ_ONLY, open rw.gnucash SESSION_NORMAL_OPEN, end() each)
c14f4215…  orig.gnucash
51b14b9b…  ro.gnucash
614cf180…  rw.gnucash
-rw-rw-r-- 1 code code   5357 Oct 27 17:14 ro.gnucash.20261027171404.log
-rw-rw-r-- 1 code code   5357 Oct 27 17:14 rw.gnucash.20261027171405.log
```

Both diffs against `orig` are the two `features` slot rows (§3.5) and `sqlite_sequence` 77 → 79. Opened a second
time (the feature now present):

```
SESSION_NORMAL_OPEN: differing bytes 2 pages [1] header offsets [27, 95]
  file change counter: 57 -> 59   version-valid-for: 57 -> 59      rows identical
SESSION_READ_ONLY:   differing bytes 0
```

So with the feature present, a read-only open leaves the file byte-identical, and a normal open changes only the
change counter (two write transactions: lock insert, lock delete).

### 10.3 Scenario results (base: the E3 book with the feature)

| Id | Mutation before opening | Mode | Errors | What changed in the file |
|---|---|---|---|---|
| E4 | `splits` version 4 | read-only | none | `splits` rebuilt (§7.1), `versions` → 5 |
| E4b | `splits` version 6 | normal | none | lock only |
| E4c | `Gnucash-Resave` 19921 | read-only | `[3002]` | nothing |
| E4d | `Gnucash` 19919 | normal | `[3001]` | lock only |
| E5 | `jobs` table and its version row dropped | read-only | none | `jobs` recreated, `versions 25|'jobs'|1` |
| E6 | `gnclock` row `('otherhost',4242)` | normal / read-only / break-lock | `[7]` / none / none | none / none / row gone, table empty |
| E7 | a transaction's `post_date`/`enter_date` in compact form `20240603105900` | normal, edit another tx | none | compact values untouched; read as 2024-06-03 10:59 UTC |
| E7b | same | normal, edit that tx | none | rewritten as `'2024-06-03 10:59:00'` |
| E7c | `post_date` `'2024-06-03 10:59:00 +0200'`, `enter_date` `'garbage'` | normal, edit that tx | warning `An invalid date garbage …` | `'2024-06-03 08:59:00'`; enter date reset to now by the engine |
| E8 | table `zz_back` with one row | read-only | none | renamed to `"zz"` |
| E9 | one split's value and quantity +1/100 | read-only | none | new `Imbalance-USD` account and a split of −1/100 |
| E10 | `gnclock` row present, then an edit | read-only | none | the edit written |
| U1 | unknown table, unknown `transactions` column, orphan slot, unknown `versions` row; then an edit | normal | none | only the edited row and its slots; the unknown content untouched |
| E11 | double slots added, then the account's description edited | normal | none | see §3.8 |
| E12 | none: the piecash fixture as shipped | read-only | none | `splits` rebuilt, `versions` 4 → 5 |
| S1 | U1's result, `Gnucash` 4000005, a user index | normal + `safe_save` | none | see §6.2 |

E11, `printf('%!.17g', double_val)` before and after the rewrite:

```
ab-double-big|real|1.2345678901234567e+19     ->  1.23456789012346e+19
zd-17digits|real|0.12345678901234566          ->  0.123456789012
zd-tiny|real|1.234567890123e-13               ->  0.0
aa-double, ac-double-int, zd-int (7), zd-text ('0.5' stored as REAL)   unchanged
```

## 11. Error codes used here (`5.17:libgnucash/engine/qofbackend.h:57-117`)

| Code | Name | Raised by |
|---|---|---|
| 3 | `ERR_BACKEND_BAD_URL` | connection failure (`gnc-backend-dbi.cpp:417-426`) |
| 7 | `ERR_BACKEND_LOCKED` | a `gnclock` row exists (`gnc-dbisqlconnection.cpp:138`) |
| 8 | `ERR_BACKEND_STORE_EXISTS` | `SESSION_NEW_STORE` on an existing file (`gnc-backend-dbi.cpp:392`) |
| 9 | `ERR_BACKEND_READONLY` | a commit on a read-only book (`gnc-sql-backend.cpp:591`) |
| 11 | `ERR_BACKEND_DATA_CORRUPT` | a malformed transaction, a missing currency, or an existing `_back` at safe-save |
| 12 | `ERR_BACKEND_SERVER_ERR` | SQL failures, a failed `sync`, lock-table errors |
| 17 | `ERR_BACKEND_MISC` | any libdbi error reported to the SQLite error handler (`gnc-backend-dbi.cpp:337-353`) |
| 1003 | `ERR_FILEIO_FILE_NOT_FOUND` | a missing file without a create mode (`gnc-backend-dbi.cpp:378`) |
| 3001 | `ERR_SQL_DB_TOO_OLD` | §7.2 |
| 3002 | `ERR_SQL_DB_TOO_NEW` | §7.2 |
| 3004 | `ERR_SQL_BAD_DBI` | the libdbi 64-bit test failed (`gnc-backend-dbi.cpp:1328-1331`) |
| 3005 | `ERR_SQL_DBI_UNTESTABLE` | the libdbi test couldn't run (`:1323-1326`) |

## 12. Implications for red-ale

These are proposals for the design docs, not GnuCash behaviour.

1. **red-ale's open never writes, which is stricter than GnuCash.** Keep the v0.4 rule that opening and closing
   without a save writes nothing, and say in `sql-fidelity.md` that GnuCash differs (§5.4). In particular red-ale
   never sets the ISO-dates feature, creates tables, upgrades tables, recovers `_back` tables or scrubs on open.
2. **The lock and "byte for byte" conflict.** Writing a `gnclock` row at open and deleting it at close changes the
   file change counter (E3), so "opened and closed without a save, unchanged byte for byte" fails if red-ale locks
   at open. Either compare rows and schema instead of bytes for SQLite (proposed: compare the `dump.py` output and
   `sqlite_master`, and separately require no change at all for read-only sessions), or take the lock only at the
   first save. Locking at open is what keeps GnuCash out while red-ale holds unsaved changes, so the proposal is
   to lock at open and test rows.
3. **Lock row:** exactly GnuCash's: `INSERT INTO gnclock VALUES ('<hostname, ≤255 bytes>', <pid>)` in one
   transaction with the existence check, creating the table with GnuCash's DDL if missing. Any row means locked.
   At close delete only red-ale's own row (`WHERE Hostname = … AND PID = …`), not every row as GnuCash does. Never
   offer break-lock in v0.4.
4. **Books needing GnuCash's load-time repairs open read-only, with the reason:** any `%back`/`%_merge` table, a
   missing table, a `versions` row other than 5.17's, `Gnucash-Resave` ≠ 19920, `Gnucash` < 19920, an unbalanced
   transaction, and any timestamp in a form item 10 doesn't accept. The absence of the ISO-dates feature alone
   need not be one: red-ale sets it in the first save's SQL transaction, as GnuCash's first load would.
5. **Older table versions are common** (E12: piecash's 3.0 book). GnuCash rebuilds them silently. Decide whether
   `splits` 4 and `slots` 3, whose SQLite DDL equals 5.17's, count as compatible for reading and writing without
   an upgrade (proposed: yes for reading; for writing only after a design note says why, since GnuCash will
   still rebuild them on its next open, which is harmless).
6. **Saving** (Decide before code, item 2): GnuCash's write-through is not atomic per transaction (E2b). red-ale's
   proposed single checked SQL transaction per `book.save` is strictly safer and leaves the file in a state GnuCash
   reads the same way. Use `BEGIN IMMEDIATE` so a concurrent writer fails before anything is written.
7. **What a save writes, to look like GnuCash:** `UPDATE` changed rows in place setting only modelled columns
   (unknown columns keep their values, as GnuCash keeps them); for an object whose slots changed, delete and
   re-insert its slots in GnuCash's order with GnuCash's filler values (§3.7); new rows appended. Unchanged objects'
   slots are left alone, including their ids and frame GUIDs. GnuCash itself regenerates them, so neither choice
   is visible to GnuCash; leaving them alone keeps unedited rows identical.
8. **Never write a new row into a table that has an unknown `NOT NULL` column without a default;** opening such a
   book read-only covers it.
9. **Read order:** `ORDER BY rowid` everywhere, splits included, so red-ale's order is deterministic and equals
   what SQLite gives GnuCash for a plain scan. KVP list items must be written in list order, so their rowids
   ascend. Keep each object's slots' row order for unedited objects.
10. **Timestamps:** read the 19-character ISO form; read the 14-digit compact form; treat any other form GnuCash
    accepts (fractions, offsets) or rejects (`garbage`) as a read-only reason rather than reproducing GnuCash's
    parser. Write only ISO, UTC.
11. **Doubles** (Decide before code, item 5): GnuCash itself destroys double precision on rewrite (§3.8), so there
    is no GnuCash behaviour to match beyond "a REAL". Proposed: keep the 8 bytes as read and write them back with
    `sqlite3_bind_double` of the same bits only when the slot's object is rewritten; never convert to text. That
    keeps more than GnuCash does and computes nothing.
12. **Deferred instances and nested lists:** an XML book with SX deferred instances, or KVP lists containing lists
    or frames, can't be saved as SQLite without loss even by GnuCash. "One book, two formats" must exclude those
    from the comparison, and Save As between formats (the missing `features.md` row) must refuse such books.
13. **Fixture builder:** use §10.1's recipe, then one read-only oracle open to add the feature, and remove the
    `.log`. Record the libdbi and SQLite versions with the fixture.
14. **Journal:** `<book path>.<YYYYMMDDhhmmss>.log`, local time, beside the book, as GnuCash does (§8). Since a
    red-ale save is explicit, start a new journal after each save as for XML.
15. **Backups:** GnuCash makes none for SQLite (§8). The proposed pre-save backup via SQLite's backup API goes beyond
    GnuCash; name it like XML backups so GnuCash's XML retention code never mistakes it for its own (it only looks
    at `.gnucash` names matching its pattern, [`storage-and-safety.md`](storage-and-safety.md) §7).
16. **Creating a book:** write the DDL in §2.2 verbatim, the `versions` rows in §2.3's order with `Gnucash|5000017`
    and `Gnucash-Resave|19920`, and the ISO-dates feature, which GnuCash would add on first open anyway.

## 13. Glossary additions and clashes

- *SQL book*: a Book stored in an SQL database; in v0.4 an SQLite file.
- *Schema version*: a row of the `versions` table, either a table's version or one of the two database-level rows
  `Gnucash` and `Gnucash-Resave`.
- **⚠ Lock:** the glossary defines it as the `.LCK` file. For SQL books it is a row in `gnclock`.
- **⚠ Read-only session:** red-ale's refuses every mutation; GnuCash's `SESSION_READ_ONLY` only skips the lock and
  writes edits (§4.4).
- **⚠ Journal:** "a new journal starts after each save" doesn't apply to GnuCash's SQL books, which have no save.

---

## Open questions

1. libdbi's sqlite3 driver wasn't read. Two claims depend on it and are from observation only: that field types come
   from the declared column types (§3.1), and that REALs are read through text with about 15 significant digits
   (§3.8). What GnuCash does with an integer column holding a REAL or text value is untested.
2. Split order inside a transaction from the `INNER JOIN` (§9) was not tested with a book whose split rowids are
   out of transaction order.
3. `xaccTransScrubPostedDate` runs on every loaded transaction (`gnc-transaction-sql.cpp:366`) while `m_loading` is
   true, so its changes are marked clean and may never be written. Not tested.
4. Whether the GUI's "Open Read-Only" path ever writes the ISO-dates feature depends on load order: the book is
   marked read-only after `qof_session_load` (`gnc-file.c:1045-1050`), so it should. Not tested with the GUI.
5. A `%_back` user table (for example `feedback`) during recovery: from the code (§5.2) it is renamed to itself or
   merged with itself; not tested.
6. Concurrency: what GnuCash does when red-ale holds an open SQLite write transaction (`SQLITE_BUSY` through libdbi,
   probably `ERR_BACKEND_MISC`) was not tested.
