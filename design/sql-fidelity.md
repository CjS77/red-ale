# SQL fidelity

How `red-ale-sql` keeps an SQLite book intact. The rules come from CLAUDE.md ("File compatibility: rules that are never
broken") and [`milestone4.md`](milestone4.md) ("Compatibility in v0.4", "SQLite books", "Decided" items 2–5); this
document says how the SQLite Backend meets them. What GnuCash 5.17 does is in
[`notes/sql-backend.md`](notes/sql-backend.md) ("the note" below, cited by section);
[`file-fidelity.md`](file-fidelity.md) is the XML counterpart.

## The target: nothing written that GnuCash would not write

An SQLite book has no byte-identical form to aim for: GnuCash rewrites rows in place, appends new ones, and its own
open writes to the file (the note §0, §5.4). The targets are:

- **A read-only open leaves the file byte for byte as it was.** GnuCash's does not (the ISO-dates feature, table
  upgrades, `_back` recovery and Imbalance scrubs all run in `SESSION_READ_ONLY`, the note §5.4).
- **A normal open and a close with no save leave every row as it was.** Only the `gnclock` row is inserted and
  deleted, which moves SQLite's file change counter (header bytes 24–27 and 92–95; the note §10.2).
- **A save writes only the rows of entities that changed**, in one SQL transaction, as GnuCash 5.17 would write those
  rows, and nothing else. Every table, column and row red-ale doesn't model is untouched.
- **GnuCash 5.17 reads the result with nothing logged** and finds the Book red-ale saved.

## Layers

1. **Connection.** `rusqlite` over the bundled SQLite ("Dependencies"). The Backend opens the file itself with
   explicit flags, never `SQLITE_OPEN_CREATE` on an existing book, and sets only per-connection state:
   `busy_timeout` 5 s, `trusted_schema = OFF`, `SQLITE_DBCONFIG_DEFENSIVE` on, and no extension loading. It never
   changes `journal_mode`, `synchronous`, `page_size` or any other setting stored in the file.
2. **Schema check.** `sqlite_master` and `PRAGMA table_xinfo` against GnuCash 5.17's schema (§7), and the `versions`
   table against 5.17's rows. What differs is a finding (§1).
3. **Rows.** Pure decoders and encoders between a row's cells and the core model: no I/O, so they can be fuzzed and
   property-tested without a database. Table names, column names, `_num`/`_denom` pairs, timestamp forms and slot rows
   stay in this layer, as element names stay in `red-ale-xml`.

**One reading for both formats.** Each column is read into the value the XML reader gives for the element GnuCash's
XML writer writes from the same engine value, so the same engine state reads to the same Book from either format. For
example GnuCash's XML writer leaves out an empty memo (`5.17:libgnucash/backend/xml/gnc-transaction-xml-v2.cpp:75-82`),
so `splits.memo = ''` reads as no memo; a zero reconcile date is not written either (`:106-107`), so
`'1970-01-01 00:00:00'` reads as no reconcile date. Where the core distinguishes values the engine does not (no memo
and an empty one), an SQL book can hold only the engine's value. The **stored form** of an entity is the entity after
writing it and reading it back; the save's read-back compares stored forms (§4).

## 1. What is kept

### Tables

| Table | red-ale |
|-------|---------|
| `books`, `commodities`, `accounts`, `lots`, `transactions`, `splits`, `slots`, `prices` | Read into the Book; written for changed entities |
| `schedxactions`, `recurrences`, `budgets`, `budget_amounts` | The same, since v0.4 models scheduled transactions and budgets |
| `billterms`, `customers`, `employees`, `entries`, `invoices`, `jobs`, `orders`, `taxtables`, `taxtable_entries`, `vendors` | The same |
| `gnclock` | Read for the lock; only red-ale's own row is ever inserted or deleted (§5) |
| `versions` | Read and checked; never written to an existing book |
| `sqlite_sequence` | SQLite's own; it moves only as `AUTOINCREMENT` inserts move it, as with GnuCash |
| `sqlite_stat1`, `sqlite_stat4` | SQLite's own (`ANALYZE`); ignored, never written |

### The SQL Residue

What the Backend keeps beside the Book, carried by the Session untouched (as `XmlResidue` is,
[`file-fidelity.md`](file-fidelity.md) "Residue"):

- **Commodity GUIDs.** SQL rows refer to commodities by GUID, which the core does not model (a commodity is its
  namespace and mnemonic, as in XML). The Residue maps each `CommodityRef` to its row's GUID.
- **Orphan owners.** The owner GUIDs of `slots`, `splits`, `recurrences`, `budget_amounts` and `taxtable_entries` rows
  that belong to nothing read. GnuCash leaves such rows itself (a removed split's slots, §4), skips them on load and
  never touches them (the note §9, experiment U1); red-ale does the same, with one exception in §4.
- **Versions** as read, for `book.info`.
- **The data version** (`PRAGMA data_version`) after the open, to detect a write by another program (§4).

The Book as read is not in the Residue: the Session keeps it for both formats (§2). Nothing else needs keeping,
because unchanged rows are never rewritten: compact timestamps, NULL-versus-empty strings and the ids and frame GUIDs
of slot rows stay as they are until their entity changes. Double slots need no Residue either: their eight bytes and
their text are in one-to-one correspondence (§3, "Doubles").

### What opens read-only

Content red-ale can read but not edit safely opens read-only, as unrecognised XML does
([`open-questions.md`](open-questions.md), "Unrecognised content opens read-only"). Each finding names the table, the
`rowid` and the column.

| Finding | Reason | Why |
|---------|--------|-----|
| A table, view, trigger or index not in 5.17's schema, other than `sqlite_*` | `unrecognised-content` | Unknown data, or code that would run on red-ale's writes |
| A table matching GnuCash's `LIKE '%back'` or `%_merge` | `unrecognised-content` | A failed safe-save GnuCash would recover on open (the note §5.2); red-ale never recovers |
| A known table with a column added, missing, or of another declared type, `NOT NULL`, primary key or `AUTOINCREMENT` | `unrecognised-content` | A new row could fail or lose the column (the note §9) |
| A missing 5.17 table or index, `gnclock` included | `unrecognised-content` | GnuCash would create it on open (the note §5.4); its absence means another program wrote the file |
| A `versions` row other than 5.17's (below) | `schema-version` | Decided item 4: never upgraded |
| A cell of a storage class GnuCash never writes in that column, or a value the column's reader refuses (§3) | `unrecognised-content` | It could not be written back |
| Slot rows that don't form a tree: a child whose name is not its parent's path plus `/`, a frame or list GUID owned twice, a cycle, `slot_type` 7 or unknown | `unrecognised-content` | The Frame read would not be the rows |
| Slot rows owned by an entity the core gives no Slots (a price) | `unrecognised-content` | They could not be kept |
| `budget_amounts` disagreeing with the budget's amount Slots (§3) | `unrecognised-content` | Decided in "SQLite books": the two copies must agree |
| `accounts.hidden` or `placeholder` disagreeing with the account's `hidden` or `placeholder` Slot (§3) | `unrecognised-content` | The same duplication |
| More than one `books` row; a budget without exactly one recurrence; a transaction with splits in both the account tree and the template tree; an account reachable from neither root; a reference to a GUID with no row | `unrecognised-content` | The Book could not be built as GnuCash builds it |
| `PRAGMA quick_check` fails (normal opens only) | `unrecognised-content` | Writing into a damaged file could spread the damage |
| A Book feature red-ale does not implement | `unsupported-features` | As for XML (the Session's rule) |

**Schema versions.** Every table row of `versions` must equal 5.17's (the note §2.3) and `Gnucash-Resave` must be
`19920`. The `Gnucash` row is not a schema version: it records the release that last wrote the whole database
(created it, Save As, or safe-save) and an ordinary edit never updates it
(`5.17:libgnucash/backend/sql/gnc-sql-backend.cpp:693-698`, `:708-716`), so a book created by 4.x and used with
5.17 still says `4000…`. It must be at least `19920`, as GnuCash's too-old check requires
(`5.17:libgnucash/backend/dbi/gnc-backend-dbi.cpp:882-896`), and at most `5000017`, since a book a later GnuCash
created may hold what 5.17 does not know. The piecash fixture (`splits|4`, written by 3.0) therefore opens read-only,
with `schema-version` naming `splits`.

**Not findings.** A missing ISO-dates Book feature: red-ale sets it in its first save (§4), as GnuCash's first load
would. An unbalanced transaction: GnuCash scrubs it on load (the note §5.1), red-ale reads it as it is, as it does in
XML, and `scrub.imbalance` repairs it when asked. Orphan rows (above).

**Refused outright,** before a Book is built, with `file-format`:

- a book in write-ahead-log mode (header bytes 18–19 = 2): GnuCash never sets it, and even reading one can write
  beside it and checkpoint into it;
- a read-only open of a book with a hot journal (`<book>-journal` left by a crash): rolling it back writes the file.
  A normal open lets SQLite roll it back, which restores the last committed state, as any GnuCash open would;
- a file SQLite cannot read as a database.

A book whose `gnclock` holds a row cannot be opened for writing (`book-locked`, §5), as for XML; it can be opened
read-only.

## 2. The Backend seam

`red-ale-service` today opens, locks and saves XML books itself. v0.4 puts both formats behind one trait, so the
Session, the journal, the read-only rule and every use case are the same for both. `red-ale-xml` and `red-ale-sql`
don't depend on the service; the trait and its two implementations (thin adapters over the two crates) live in
`red-ale-service::backend`. The Residue is each Backend's own type and only that Backend reads it.

```rust
/// The format of a book file. `book.info` reports it; `book.create` takes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// GnuCash's XML, gzipped or plain (`red-ale-xml`).
    Xml,
    /// An SQLite database (`red-ale-sql`).
    Sqlite,
}

impl Format {
    /// The format named by a URI scheme (`xml://`, `sqlite3://`), else the one the file's first bytes say, as GnuCash
    /// decides (`5.17:libgnucash/backend/dbi/gnc-backend-dbi.cpp:1029-1068`).
    /// (Until the SQLite Backend lands, `of(path)` only sniffs; see "The XML Backend moves first".)
    pub fn of(spec: &str, path: &Path) -> Result<Self, ServiceError>;
}

/// Whether a session may write.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Access {
    ReadOnly,
    ReadWrite,
}

/// The Session's read-only rule: from the Book just read and what its Backend found, every reason it must open
/// read-only. A Backend calls it while it can still decide not to lock.
pub type ReadOnlyRule<'a> = &'a dyn Fn(&Book, &[ReadOnlyReason]) -> Vec<ReadOnlyReason>;

/// A book a Backend has opened.
pub struct Opened {
    /// The Book as read.
    pub book: Book,
    /// Why the session is read-only; empty when it may write, in which case the Backend holds the lock.
    pub read_only: Vec<ReadOnlyReason>,
    /// The open file with its Residue and lock.
    pub backend: Box<dyn Backend>,
}

/// Open an existing book, taking its lock only if `access` is `ReadWrite` and `rule` finds no reason.
pub fn open(path: &Path, format: Format, access: Access, rule: ReadOnlyRule<'_>) -> Result<Opened, ServiceError>;

/// Create a new, empty book and open it for writing.
pub fn create(path: &Path, format: Format, settings: &Settings, env: &mut dyn Env) -> Result<Opened, ServiceError>;

/// What a save may change about the file.
pub struct SaveOptions {
    /// XML only: write gzipped or plain. `Some` on an SQLite book is `invalid-params`.
    pub compress: Option<bool>,
}

/// What a save did.
pub struct Saved {
    /// The backup made before writing, if any.
    pub backup: Option<PathBuf>,
    /// XML only: whether the file was written gzipped.
    pub compressed: Option<bool>,
}

/// One open book file: its Residue, its lock and how to save it. Dropping it releases its lock.
pub trait Backend: fmt::Debug + Send {
    /// The file's format.
    fn format(&self) -> Format;

    /// The book file. Its journal and backups are named from it.
    fn path(&self) -> &Path;

    /// True while this Backend holds the book's lock.
    fn holds_lock(&self) -> bool;

    /// Release the lock now, reporting any failure. Releases only this Backend's own lock; does nothing if none is held.
    fn release_lock(&mut self) -> Result<(), ServiceError>;

    /// Refuse a Mutation whose result this format cannot hold without loss (§8). Called for every Mutation, before
    /// the Session replaces its Book.
    fn check_storable(&self, before: &Book, after: &Book) -> Result<(), ServiceError>;

    /// The Book as this format requires it to be saved, if that differs from `book`: SQLite sets the ISO-dates Book
    /// feature (§4). XML returns `None`.
    fn prepare_save(&self, book: &Book) -> Option<Book>;

    /// Save `book`, given `saved`, the Book as read or last saved. Writes nothing unless everything it wrote
    /// reads back as `book`.
    fn save(&mut self, saved: &Book, book: &Book, options: &SaveOptions, env: &mut dyn Env) -> Result<Saved, ServiceError>;

    /// Names of Residue sections that refer to `to`. Empty for SQLite: unknown content opens read-only.

    /// XML only: whether the file is gzipped (`book.info`).
    fn compressed(&self) -> Option<bool>;

    /// XML only: the Residue Export Accounts writes from. Export Accounts writes XML whatever the book's format and
    /// uses `XmlResidue::new_book` for an SQLite book, so every account is generated.
    fn xml_residue(&self) -> Option<&XmlResidue>;
}
```

**What the Session keeps.** Its `residue` and `lock` fields become one `backend: Box<dyn Backend>`, and it gains
`saved: Book`, the Book as read or last saved (an `Arc` clone, one pointer per entity). Everything else is as now:
the Book, the read-only reasons, the journal, the dirty flag, the settings, the `Env`, the revision and the Import
proposals. `try_mutate` calls `check_storable` before it commits; `save` calls `prepare_save` (when the Book is dirty),
then `save`, then sets `saved` to the Book, rotates the journal and applies retention, for both formats.
`read_only_triggers` takes the Backend's findings (`&[ReadOnlyReason]`) instead of an `XmlResidue`; the rule the
Session passes adds `Requested` first for a read-only open, so `Opened::read_only` is complete and "empty" means "the
Backend holds the lock". With the SQLite Backend, `ReadOnlyReason` gains `SchemaVersions(Vec<String>)` with the code
`schema-version`. A Book `prepare_save` returns replaces the Session's Book only once the save succeeds, and does not
move the revision: it is not a Mutation and touches nothing an Import proposal reads.

`Session::residue()` is gone (deletion and rename checks now read the model: budgets and scheduled transactions are in the Book); callers use
`compressed() -> Option<bool>` (`book.info`), `xml_residue() -> Option<&XmlResidue>` (Export Accounts) and
`format()`. `SaveReport::compressed` is `Option<bool>`; the wire still says `false` for `None` until the SQLite
ticket decides `book.info` and `book.save`'s results.

**The XML Backend moves first, with no byte changing.**
`XmlBackend { path, residue: XmlResidue, lock: Option<BookLock> }` does what `Session::open`, `save` and `close` do
today, in the same order: lock, read, release the lock if the rule finds a reason; write a temporary file, read it
back, back up, rename. Its `saved` argument equals the snapshot its Residue already holds and is unused at first.
`check_storable` always passes; `prepare_save` returns `None`. Its `create` locks and builds the empty Book and a new
book's Residue but writes nothing: `Session::create` marks the Book dirty and saves it, as before, so the first write
goes through the same replace, journal rotation and retention as every save. Until the SQLite Backend exists,
`Format::of` takes only the path and sniffs its first bytes (`resolve_path` still refuses `sqlite3://` with
`sql-book`), and `open` and `create` with `Format::Sqlite` give `sql-book`. The commit that adds the seam changes no
test except imports, constructors and one accessor (`residue().compressed` to `compressed()`), and every round-trip test, both writer paths included ([`file-fidelity.md`](file-fidelity.md),
"Proving the generators"), passes unchanged.

**`red-ale-sql`'s own surface**, which the SQLite adapter wraps:

```rust
/// Who holds a lock: the `gnclock` row. The service fills it from the host name and process id, so this crate makes
/// no system calls of its own and tests can play GnuCash's part.
pub struct LockOwner {
    pub hostname: String,
    pub pid: i64,
}

/// What the schema check and the row readers found; each entry makes the book read-only.
pub struct Findings {
    pub unrecognised: Vec<String>,
    pub schema_versions: Vec<String>,
}

/// An open SQLite book: the connection, the lock row if held, the SQL Residue, and whether this session has made
/// its backup.
pub struct SqliteBook { /* private */ }

/// Read the book. With an owner, reads inside `BEGIN IMMEDIATE` and inserts the lock row only if `writable` agrees.
pub fn open(path: &Path, owner: Option<&LockOwner>, writable: &dyn Fn(&Book, &Findings) -> bool)
    -> Result<(Book, SqliteBook, Findings), SqlError>;

/// Write a new book with GnuCash 5.17's schema (§7) and lock it.
pub fn create(path: &Path, book: &Book, owner: &LockOwner, env: &mut dyn Env) -> Result<SqliteBook, SqlError>;

impl SqliteBook {
    /// Why `after` can't be stored in an SQLite book, if it can't (§8).
    pub fn unstorable(before: &Book, after: &Book) -> Option<String>;
    /// Write the rows that differ between `saved` and `book` in one checked transaction (§4).
    pub fn save(&mut self, saved: &Book, book: &Book, env: &mut dyn Env) -> Result<(), SqlError>;
    /// Copy the database to `dest` with SQLite's online backup API, without this session's lock row (§6).
    pub fn backup_to(&self, dest: &Path) -> Result<(), SqlError>;
    /// Delete this session's lock row, and only it (§5).
    pub fn release_lock(&mut self) -> Result<(), SqlError>;
}
```

The core is untouched by the seam: `red-ale-sql` depends on `red-ale-core` alone, and uses the `Env` for every GUID
it makes.

**Paths and URIs.** A plain path or `file://` is sniffed by its first bytes (`SQLite format 3\0`), as GnuCash does;
`xml://` is XML and `sqlite3://` is SQLite, and a scheme that contradicts the content is `file-format`. Database URIs
(`mysql://`, `postgres://`, `postgresql://`) are still refused with `sql-book` before anything is touched, whose
message now names v0.7.

## 3. Reading

A normal open runs inside one `BEGIN IMMEDIATE` transaction (§5); a read-only open opens the file with
`SQLITE_OPEN_READONLY` and reads inside one deferred `BEGIN`, so either way the Book comes from one consistent
snapshot even if GnuCash writes meanwhile. Every table is read with explicit column names and `ORDER BY rowid`, the
order SQLite gives GnuCash's plain scans (the note §9), so entities enter the Book's maps in the file's order. Rows
are decoded as they are stepped and not kept, so the memory budgets of [`milestone3.md`](milestone3.md) hold.

### Tables to the model

| Table | Becomes | Notes |
|-------|---------|-------|
| `books` | `Book.guid`, `Book.root`, the Template root | Exactly one row. The Book's Slots are its slot rows |
| `commodities` | `Book.commodities` | Namespace and mnemonic make the `CommodityRef`; the GUID goes to the Residue. GnuCash writes a commodity only when something uses it (`5.17:libgnucash/backend/sql/gnc-sql-backend.cpp:884-893`; `sync` writes none, `:492-493`). So the `template` commodity every book has gets a row only once a scheduled transaction's template account uses it (`5.17:libgnucash/engine/SchedXaction.cpp:398-401`; `gnc-account-sql.cpp:318`); when it has none, the reader adds it as `Book::new_empty` does |
| `accounts` | `Book.accounts` and the template accounts | Rows reachable from `root_account_guid` by `parent_guid` are the Account tree; rows reachable from `root_template_guid` are template accounts, under the Template root, never in the tree ([`milestone4.md`](milestone4.md), "The model") |
| `lots` | `Book.lots` | `is_closed` is a cache, not data (below) |
| `transactions`, `splits` | `Book.transactions` and the template transactions | A transaction whose splits are all in template accounts is a template transaction; one with splits in both trees is a finding. Splits come from `SELECT … FROM splits ORDER BY rowid`, grouped by `tx_guid` |
| `prices` | `Book.prices` | Commodity and currency by GUID through the Residue |
| `schedxactions` | Scheduled transactions | `template_act_guid` names the template account. Deferred instances: none (§8) |
| `recurrences` | A scheduled transaction's Schedule, a budget's Recurrence | By `obj_guid`, in `rowid` order, which is the Schedule's order (GnuCash appends in result order, `5.17:libgnucash/backend/sql/gnc-recurrence-sql.cpp:348-361`). A budget's must be exactly one: GnuCash warns at none or several and takes the first (`:323-345`) |
| `budgets`, `budget_amounts` | Budgets | Amounts and notes come from the budget's Slots; `budget_amounts` is checked against them (below) |
| `billterms` … `vendors` | `Book.business` | Owner references are `owner_type` (GnuCash's `GncOwnerType`) and `owner_guid`; addresses are the `addr_*` and `shipaddr_*` columns (`5.17:libgnucash/backend/sql/gnc-address-sql.cpp:51-68`); `taxtable_entries` by `taxtable`, in `rowid` order |
| `slots` | Every entity's Slots | Read whole, in `rowid` order, and built into Frames by owner (below) |

**Split order.** A transaction's split order is the order its splits load. GnuCash loads them through an
`INNER JOIN` with no `ORDER BY` (`5.17:libgnucash/backend/sql/gnc-transaction-sql.cpp:256-262`): whether SQLite scans
`splits` or walks `splits_tx_guid_index` per transaction, one transaction's splits come in `rowid` order, which is
what red-ale reads. A test with split `rowid`s out of transaction order confirms it (the note's open question 2).

### Values

| Column type (§2.1 of the note) | Read as | Refused (a finding) |
|---|---|---|
| `integer`, `bigint` | INTEGER, within the engine field's range | any other storage class; out of range |
| booleans (`integer`) | 0 or 1; `lots.is_closed` also −1 | other values |
| `text(n)` | TEXT, valid UTF-8 | BLOB, INTEGER, REAL; invalid UTF-8 (read lossily for the read-only Book) |
| NULL | only where GnuCash's DDL lets it be, and read as GnuCash's reader leaves the field (the setter is not called) | NULL in a column GnuCash always fills |
| GUIDs and references (`text(32)`) | 32 lower-case hex digits, as `guid_to_string` writes (the note §3.2) | anything else, including upper case, which GnuCash's reader would skip silently |
| Numerics (`x_num`, `x_denom`) | two INTEGERs, unreduced, as `Numeric` accepts them from XML | a missing half; a REAL or TEXT half |
| Timestamps (`text(19)`) | `YYYY-MM-DD HH:MM:SS` or `YYYYMMDDHHMMSS`, both UTC; NULL and `''` as time 0, as GnuCash reads them (`5.17:libgnucash/backend/sql/gnc-sql-column-table-entry.cpp:376-399`) | every other form, even those GnuCash's parser accepts (fractions, offsets: the note §3.4), and those it turns into 1970 |
| Dates (`text(8)`) | `YYYYMMDD`; NULL, `''` and `00000000` as an unset date (the note §3.6) | anything else |
| Doubles (`float8`) | REAL (below) | any other storage class |
| Enumerations in text (`account_type`, `recurrence_period_type`, `recurrence_weekend_adjust`, `reconcile_state`, …) | the names GnuCash writes | unknown names |

A cell that is refused still gives the Book a value, the one GnuCash's reader gives (the engine's default when its
setter is skipped), so a read-only session can show the book. A row whose primary key is refused is left out of the
Book.

**Timestamps** in compact form are kept as they are: a row is converted to ISO only when red-ale rewrites it, as GnuCash
does (the note §3.4, experiments E7 and E7b).

**Doubles.** `double_val` holds an 8-byte IEEE double. The reader turns it into the text GnuCash's XML writer gives
the same double, `%24.18g` with the spaces stripped ([`notes/xml-format-core.md`](notes/xml-format-core.md) §4), which
is what `SlotValue::Double` holds. Eighteen significant digits identify every finite double, so the text parses back
(correctly rounded) to the same eight bytes, and an unchanged double is written back bit for bit (Decided item 5).
This is formatting, not arithmetic: no float is computed with. The formatter is red-ale's own and is
property-tested against C's `%.18g` (§9).

**Slots.** `slots` is read once, whole, and each owner's rows are built into a Frame: a row with `slot_type` 9 (frame)
or 8 (list) owns the rows whose `obj_guid` is its `guid_val`; a child's key is its `name` less its parent's path and
`/`; list items have the key `""` and keep their `rowid` order; top-level keys are taken whole, so a key holding `/`
stays one key (the note §3.7). Only the value column the `slot_type` names is read; the filler in the others is
ignored, as GnuCash ignores it (`5.17:libgnucash/backend/sql/gnc-slots-sql.cpp:315`, `:343`, `:374`, `:399`, `:509`,
`:539`). Rows whose owner matches nothing read, and the rows under them, are orphans.

**Columns that duplicate a Slot.** Two pieces of data are stored twice in an SQL book, once in a column and once in
`slots`, and the core keeps only the Slot:

- **Budget amounts.** GnuCash writes one `budget_amounts` row per amount Slot of an account in the Account tree and a
  period below `num_periods` (`5.17:libgnucash/backend/sql/gnc-budget-sql.cpp:251-289`), while the Slots hold every
  amount, hidden periods and deleted accounts included ([`notes/engine-budgets.md`](notes/engine-budgets.md) §6). The
  two agree when the rows are exactly that set, with equal numerators and denominators.
- **`accounts.hidden` and `placeholder`.** The engine keeps both in the account's Slots ("Table slot exists, but in
  KVP in memory & xml", `5.17:libgnucash/engine/Account.cpp:116-117`; `:4107-4116`, `:4178-4187`), true when the Slot
  is the string `true` or a non-zero integer (`:2563-2577`); the SQL backend also writes them as columns
  (`5.17:libgnucash/backend/sql/gnc-account-sql.cpp:91-92`). They agree when the column (NULL counting as 0) equals
  the Slot's truth.

Disagreement can come only from an outside edit, and which copy GnuCash would follow depends on its load order (for
budgets, [`notes/engine-budgets.md`](notes/engine-budgets.md) §6), so either is a finding.

**Lots' `is_closed`** is the engine's cache of whether the lot's balance is zero: −1 (unknown), 0 or 1
(`LOT_CLOSED_UNKNOWN`, `5.17:libgnucash/engine/gnc-lot.cpp:96`). GnuCash writes it from the `is-closed` property
(`5.17:libgnucash/backend/sql/gnc-lots-sql.cpp:64`; `gnc-lot.cpp:149-150`), which adding or removing a split resets
to −1 (`:620-623`, `:642`) and which is recomputed from the splits on first use (`:367-373`). It is not in the
model; the reader accepts the three values.

## 4. Writing a save

GnuCash has no Save for SQL books: each commit writes through in its own SQL transaction, a GnuCash transaction
whose two splits change being two SQL transactions (the note §6.1, experiment E2b). red-ale saves at `book.save`, in
one transaction (Decided item 2):

1. **Nothing to do.** A save of a Book that is not dirty writes nothing and makes no backup.
2. **Backup**, before the first save of the session that writes (§6). If it fails, nothing is written.
3. **`BEGIN IMMEDIATE`**, so a concurrent writer fails before anything is written.
4. **Check the file is still red-ale's.** `gnclock` must hold exactly red-ale's row, and `PRAGMA data_version` must
   be what it was after the open (or the last save): SQLite changes it when another connection commits. Either
   failing means another program wrote the book or broke the lock (GnuCash's "Open Anyway" deletes every row,
   the note §4.3; a GnuCash read-only session writes its edits regardless of the lock, the note §4.4), and the save is refused
   with `book-changed`, writing nothing.
5. **Diff** the Book against the Book as saved: `red_ale_core::diff::changes` for each entity map, and by
   `CommodityRef` for commodities.
6. **Write** the rows of each change (below), table by table in GnuCash's registry order, additions and changes in
   the Book's order, then deletions. Every statement's row count is checked against what it should touch: an
   `UPDATE … WHERE guid = ?` must change one row, a slot deletion exactly the rows the owner's Frame was stored as.
7. **Read back** every entity written, through the same decoders, inside the transaction, and compare its stored form
   with the Book's; check no row of a deleted entity remains.
8. **`COMMIT`** only if every step passed; otherwise `ROLLBACK`, and the file is as it was. A failed `COMMIT` is
   rolled back too. A mismatch is `save-verification`, as for XML.

### Which rows an entity writes

| Entity | Added | Changed | Removed |
|--------|-------|---------|---------|
| Book | (only by `book.create`, §7) | its slot rows, if its Slots changed; the `books` row never changes | never |
| Commodity | `INSERT` with a new GUID from the `Env` | `UPDATE`; slot rows if its Slots changed | `DELETE`, with its slot rows |
| Account, template account | `INSERT` | `UPDATE`; slot rows if changed | `DELETE`, with its slot rows |
| Lot | `INSERT`, `is_closed` −1 | `UPDATE`, `is_closed` −1; slot rows if changed | `DELETE`, with its slot rows |
| Transaction, template transaction | `INSERT` the transaction, then its splits in order | `UPDATE` the transaction row only if its own columns changed; each split `INSERT`, `UPDATE` or `DELETE` as it changed; slot rows of each that changed | `DELETE` the transaction, its splits and all their slot rows (`5.17:libgnucash/backend/sql/gnc-transaction-sql.cpp:512-532`) |
| Price | `INSERT` | `UPDATE` | `DELETE` |
| Scheduled transaction | `INSERT`, its recurrences, its slot rows | `UPDATE`; recurrences if its Schedule changed; slot rows if changed | `DELETE`, with its recurrences and slot rows |
| Budget | `INSERT`, its recurrence, amounts, slot rows | `UPDATE`; the recurrence if changed; `budget_amounts` and slot rows if its Slots or `num_periods` changed | `DELETE`, with recurrence, amounts and slot rows (`5.17:libgnucash/backend/sql/gnc-budget-sql.cpp:411-421`) |
| Tax table | `INSERT`, its entries | `UPDATE`; entries if they changed | `DELETE`, with entries and slot rows |
| Other business objects | `INSERT` | `UPDATE`; slot rows if changed | `DELETE`, with slot rows |

- **Columns.** An `INSERT` lists every column of GnuCash's column table for the table except an `AUTOINCREMENT` `id`,
  which SQLite assigns; an `UPDATE` sets every one of them, the primary key included, as GnuCash's does
  (`5.17:libgnucash/backend/sql/gnc-sql-backend.cpp:932-964`). A writable book has no other columns (§1). Values are
  bound, never spliced into SQL text, and written as GnuCash writes them: timestamps `YYYY-MM-DD HH:MM:SS` in UTC, or
  NULL outside GnuCash's `(MINTIME, MAXTIME)` (`5.17:libgnucash/backend/sql/gnc-sql-column-table-entry.cpp:444-455`;
  `5.17:libgnucash/engine/gnc-datetime.hpp:55-56`); dates `YYYYMMDD`, or NULL when unset (`…cpp:512-529`);
  numerics unreduced (`…cpp:593-634`); booleans 0 or 1; absent strings in `NOT NULL` columns as `''`.
- **Dependent rows** (an owner's recurrences, budget amounts, tax-table entries, slots) are deleted and inserted again
  whole when they change, as GnuCash does (`5.17:libgnucash/backend/sql/gnc-recurrence-sql.cpp:255-273`,
  `gnc-budget-sql.cpp:252-289`, `gnc-tax-table-sql.cpp:386-406`). Budget amounts are written in GnuCash's order: the
  Account tree's descendants depth first, periods ascending, set amounts only.
- **Commodities used but not stored.** A written row that refers to a commodity with no row (a transaction's
  currency, an account's or a price's commodity, a business object's currency) first gets that commodity inserted,
  from the Book or the ISO table, as GnuCash's `save_commodity` does
  (`5.17:libgnucash/backend/sql/gnc-sql-backend.cpp:884-893`; callers `gnc-account-sql.cpp:318`,
  `gnc-transaction-sql.cpp:613`, `gnc-price-sql.cpp:189-190`, `gnc-invoice-sql.cpp:210`, `gnc-employee-sql.cpp:185`,
  `gnc-vendor-sql.cpp:164`).
- **A renamed commodity** (a new namespace or mnemonic) is one removed and one added to the diff: its row is deleted,
  a new row with a fresh GUID is inserted, and every row referring to it changed with it. GnuCash keeps the GUID;
  nothing outside the rows red-ale writes can refer to it in a writable book.
- **Split order.** New split rows go to the end of `splits`, so they read back last in their transaction. When the
  Book's split order for a changed transaction would not read back that way (a split added before others, or
  splits reordered), all its split rows are deleted and inserted again in order, keeping their GUIDs and leaving
  their slot rows alone, which are keyed by GUID.
- **Lots.** A lot is written when it changed, and its `is_closed` set to −1 whenever a split entering, leaving or
  changing in it was written, so GnuCash recomputes it, as it does after its own split changes (§3).
- **Removed splits** take their slot rows with them. GnuCash's split commit deletes the split's row but not its slots
  (`5.17:libgnucash/backend/sql/gnc-transaction-sql.cpp:575-578`), leaving orphans; red-ale doesn't add to them.
- **Orphans that would be adopted.** Before inserting an entity, red-ale deletes the orphan rows keyed by its GUID
  (slots, recurrences, budget amounts, tax-table entries), since GnuCash would read them as the new entity's. This
  happens only when a GUID comes back, as when a journal replay brings back a split GnuCash removed from its
  transaction. All other orphans are never touched.

### Slots

For an entity whose Slots changed, its slot rows are deleted, frames and lists recursively, and inserted again, as
GnuCash does (`5.17:libgnucash/backend/sql/gnc-slots-sql.cpp:645-649`, `:658-697`):

- in GnuCash's order: a Frame's slots in key byte order, depth first, list items in list order
  (`for_each_slot_temp`, `:593`, `:653`; `:611-615`), so `id`s ascend in that order and list items read back in order;
- each frame and list row gets a **new GUID**, as GnuCash's `guid_new()` gives one on every write (`:583`, `:601`),
  taken from the session's `Env`, so tests are deterministic; children are owned by it;
- every row filled as GnuCash fills it: `int64_val` 0, `timespec_val` `'1970-01-01 00:00:00'`, `numeric_val_num` 0,
  `numeric_val_denom` 1, and NULL in `string_val`, `double_val`, `guid_val` and `gdate_val`, except the column of the
  row's own type (the note §3.7);
- doubles bound as the REAL their text gives: for a double read from the file, its eight bytes. A double red-ale sets
  is written as GnuCash writes one, a REAL in `double_val`; red-ale binds its exact value, where GnuCash's text keeps
  12 decimals (`5.17:libgnucash/backend/sql/gnc-sql-column-table-entry.hpp:403-416`). For every double red-ale sets
  today, the whole number of days of the auto read-only threshold, the two are the same REAL.

An entity whose Slots did not change keeps its slot rows, `id`s and frame GUIDs, where GnuCash would rewrite them on
every commit. Neither is visible to GnuCash (implication 7 of the note), and leaving them keeps unedited rows
identical.

### The ISO-dates Book feature

GnuCash sets `ISO-8601 formatted date strings in SQLite3 databases.` on every SQLite load
(`5.17:libgnucash/backend/dbi/gnc-backend-dbi.cpp:879-880`). red-ale writes only ISO timestamps and sets the feature,
with GnuCash's description (`5.17:libgnucash/engine/gnc-features.cpp:43`), in the first save that writes anything,
never on open: `prepare_save` adds it to the Book when it is missing, the Session keeps that Book, and the save
writes the Book's slot rows with it, as GnuCash's own feature commit does (the note §3.5). It is not a Mutation:
no proposal reads Book features, so the revision does not move.

### What is never written

`versions`; the schema (no `CREATE`, `ALTER` or `DROP` on an existing book); other programs' `gnclock` rows; tables
and rows red-ale doesn't model; orphan rows (but for adoption, above); rows of unchanged entities; file settings
("Layers").

## 5. The lock

The lock is GnuCash's (`5.17:libgnucash/backend/dbi/gnc-dbisqlconnection.cpp:101-227`; the note §4): a row in
`gnclock (Hostname varchar(255), PID int)`, and any row at all means locked, whatever its host or PID.

**Taking it.** A normal open reads the whole book inside one `BEGIN IMMEDIATE`, as GnuCash takes its lock inside one
transaction with the existence check (`:105-107`). In that transaction red-ale checks `gnclock`, reads the book, runs
the read-only rule, and only if the session will be writable inserts its row, then commits. So the existence check,
the read and the insert are atomic, and a book that opens read-only because of what it holds is never written at all.
A row already there ends the open with `book-locked`, rolled back, the file untouched; the error's `data` names the
book and the holder's host and PID. An open that fails for any reason before the commit leaves nothing behind.
`SQLITE_BUSY` after the 5 s timeout (another program is mid-write) is also `book-locked`; a file red-ale may not
write is `cannot-lock`.

**What is written.** `INSERT INTO gnclock VALUES (?, ?)` with the host name and the process ID as GnuCash writes
them: `gethostname`, at most 255 bytes (`:129`, `:155-156`), and the PID, which SQLite stores as an INTEGER as it
stores GnuCash's quoted `'%d'` (`:157-159`). The service supplies both (`rustix::system::uname().nodename()`,
`std::process::id()`); binding them, where GnuCash splices the host name into SQL unescaped, gives the same row.
red-ale never creates `gnclock`: a book without it opens read-only (§1). It never offers `SESSION_BREAK_LOCK` (v0.4).

**Holding it.** The connection stays open for the session, so the lock row and `data_version` belong to it. An idle
connection holds no SQLite lock, so GnuCash can still open the book read-only.

**Releasing it,** on `book.close`, when a session turns out read-only, and on drop (unwinding included), on every exit
path, error paths included: in one transaction, `DELETE FROM gnclock WHERE Hostname = ? AND PID = ?`, which must
delete one row. GnuCash deletes every row once it finds its own (`:192-219`); red-ale deletes only its own. If its row
is gone (the lock was broken) it deletes nothing and the close still succeeds. A failed save rolls back and leaves the
lock held; the session goes on.

**Read-only sessions** take no lock, don't look at `gnclock`, and open the file with `SQLITE_OPEN_READONLY`, so they
work while GnuCash holds the book.

A killed process leaves its row, as a crashed GnuCash does; the error names the holder so the user can clear it
(GnuCash's "Open Anyway" does).

## 6. Journal and backups

**Journal.** As for XML: `<book path>.<YYYYMMDDhhmmss>.log`, local time, beside the book
(`5.17:libgnucash/backend/dbi/gnc-backend-dbi.cpp:456`; `5.17:libgnucash/engine/TransLog.cpp:174-176`), opened at
the first record and rotated after each save and at close. It holds the session's unsaved transaction changes;
`book.replayJournal` reads it on either format. GnuCash's SQL journals hold changes already written (each commit
writes through) and are split at every GUI Save even though nothing is saved (the note §8); red-ale's mean the same
for both formats.

**Backup.** GnuCash makes none for SQL books (the note §8). Before the first save of a session that writes, red-ale
copies the book to `<book path>.<YYYYMMDDhhmmss>.gnucash`, the name and local-time stamp of XML backups:

- with SQLite's online backup API, all pages in one step, so the copy is one consistent state of the database;
- into a temporary file beside the book, from which red-ale's own lock row is deleted, so the backup opens in GnuCash
  unlocked; then synced, given the book's mode and group as XML saves do, and linked to its name without replacing an
  existing file, waiting for the next second as XML backups do;
- before `BEGIN IMMEDIATE`, so a failed backup writes nothing to the book.

Later saves in the session make none: the backup and the journals cover them. Retention deletes old backups and
journals of an SQLite book as for XML, by the same name pattern and modification time; GnuCash itself never cleans
up beside SQL books, and only ever looks at XML books' siblings.

## 7. Creating

`book.create` makes XML by default ([`notes/storage-and-safety.md`](notes/storage-and-safety.md) §15, item 9);
`format: "sqlite"`, or a `sqlite3://` path, makes an SQLite book. `format: "xml"` with `sqlite3://`, or `compressed`
with SQLite, is `invalid-params`; an existing path is `file-exists`.

The book is written into a temporary file beside the target, in one transaction, then linked to its name without
replacing anything, so a half-made book is never seen. The statements, in this order, are GnuCash 5.17's DDL as the
oracle wrote it (the note §2.2), so `sqlite_master` lists the objects in GnuCash's order (`sqlite_sequence` appears
with the first `AUTOINCREMENT` table):

```sql
CREATE TABLE gnclock ( Hostname varchar(255), PID int );
CREATE TABLE versions(table_name text(50) PRIMARY KEY NOT NULL, table_version integer NOT NULL);
CREATE TABLE books(guid text(32) PRIMARY KEY NOT NULL, root_account_guid text(32) NOT NULL, root_template_guid text(32) NOT NULL);
CREATE TABLE commodities(guid text(32) PRIMARY KEY NOT NULL, namespace text(2048) NOT NULL, mnemonic text(2048) NOT NULL, fullname text(2048), cusip text(2048), fraction integer NOT NULL, quote_flag integer NOT NULL, quote_source text(2048), quote_tz text(2048));
CREATE TABLE accounts(guid text(32) PRIMARY KEY NOT NULL, name text(2048) NOT NULL, account_type text(2048) NOT NULL, commodity_guid text(32), commodity_scu integer NOT NULL, non_std_scu integer NOT NULL, parent_guid text(32), code text(2048), description text(2048), hidden integer, placeholder integer);
CREATE TABLE budgets(guid text(32) PRIMARY KEY NOT NULL, name text(2048) NOT NULL, description text(2048), num_periods integer NOT NULL);
CREATE TABLE budget_amounts(id integer PRIMARY KEY AUTOINCREMENT NOT NULL, budget_guid text(32) NOT NULL, account_guid text(32) NOT NULL, period_num integer NOT NULL, amount_num bigint NOT NULL, amount_denom bigint NOT NULL);
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

Then the rows:

- `versions`, in this order (the note §2.3): `Gnucash|5000017`, `Gnucash-Resave|19920`, `books|1`, `commodities|1`,
  `accounts|1`, `budgets|1`, `budget_amounts|1`, `prices|3`, `transactions|4`, `splits|5`, `slots|4`,
  `recurrences|2`, `schedxactions|1`, `lots|2`, `billterms|2`, `customers|2`, `employees|2`, `entries|4`,
  `invoices|4`, `jobs|1`, `orders|1`, `taxtables|2`, `taxtable_entries|3`, `vendors|1`;
- the `books` row, then the Book's slot rows: the same empty Book `book.create` makes for XML, with the ISO-dates
  feature, which GnuCash would add on its first open (implication 16 of the note);
- the root account (`Root Account`, `ROOT`, no commodity, `commodity_scu` 0, `non_std_scu` 0, `hidden` and
  `placeholder` 0) and the template root (`Template Root`), as GnuCash's `sync` writes the two trees
  (`5.17:libgnucash/backend/sql/gnc-sql-backend.cpp:393-405`);
- red-ale's lock row.

No `PRAGMA` is set: the bundled SQLite's defaults are what GnuCash's books have (page size 4096, UTF-8, no
auto-vacuum, rollback journal `delete`, `user_version` and `application_id` 0; the note §1), and a test checks each.
The session's first save makes a backup as any other session's does.

## 8. What SQLite can't hold

Some data has no place in an SQL book, or GnuCash can't read it back from one. red-ale refuses the Mutation that would
need it, through `check_storable`, with the reason, rather than lose it at save:

- **Deferred instances of a scheduled transaction.** `schedxactions` has no column for them, and GnuCash's SQL
  backend drops them (the note §3.9, experiment E1b). Postponing an instance in an SQLite book is refused; so is any
  other Mutation that would leave a scheduled transaction with deferred instances.
- **Lists inside lists, and frames inside lists, in Slots.** GnuCash writes them as rows correctly but does not read
  them back as written (the note §9, experiments S1 and E1b). Setting such a value in an SQLite book is refused. One
  read from the file is kept as read and written back with the same structure when its entity changes, so GnuCash
  sees what it saw before.
- **Times outside `(MINTIME, MAXTIME)`.** GnuCash writes them NULL and reads them as 1970
  (`5.17:libgnucash/backend/sql/gnc-sql-column-table-entry.cpp:451-455`).
- **Strings containing NUL**, which GnuCash's C strings would cut short.
- **A NaN double**, which SQLite stores as NULL.

Converting a book between formats (GnuCash's Save As) is not in v0.4 ([`milestone4.md`](milestone4.md), "Out"); when
it comes, it is refused for any book holding these.

## 9. Testing

As [`testing.md`](testing.md) sets out, on copies in temporary directories, never on a user's book.

- **Fixtures.** For every canonical fixture the oracle builds an SQLite copy with the recipe of the note §10.1: open
  the XML read-only, `SESSION_NEW_STORE` an `sqlite3://` session, swap the data, mark the book dirty through
  `ctypes`, save, open once read-only so the ISO-dates feature is set, and delete the journal the bindings leave. The
  script is committed with its output, which records the libdbi and SQLite versions under the oracle; an oracle test
  rebuilds them and requires the same rows. The piecash SQLite book stays as shipped, the case of an older schema.
- **Read-only round-trip.** Opening read-only and closing leaves the file byte for byte as it was (a hash), for every
  fixture, and creates no file beside it.
- **Normal round-trip.** Opening for writing and closing leaves every row as it was: the dump of every table (`rowid`
  then `quote()` of each column) and of `sqlite_master` is equal, and the file differs only in header bytes 24–27 and
  92–95.
- **Edits.** For each entity kind, an addition, an edit and a deletion are saved, then opened by GnuCash 5.17 with its
  log captured: nothing is logged at warning or above, the Book GnuCash reads equals red-ale's, and every row of every
  table and column red-ale doesn't model, and every orphan row, is unchanged.
- **Writing as GnuCash writes.** The oracle makes the same edit to a second copy, and the two dumps are equal once
  slot `id`s and frame GUIDs are numbered in order of appearance: the same rows changed, with the same values.
- **Save atomicity.** A test hook in `red-ale-sql` fails the n-th statement of a save, for every n, and the file's
  rows are unchanged afterwards; a mismatched read-back (a corrupting hook) is refused with `save-verification`.
- **One book, two formats.** The XML and SQLite copies of each fixture read to equal Books, apart from what GnuCash's
  own conversion changes: deferred instances and nested lists (§8), doubles beyond 12 decimals (the note §3.8) and
  commodities neither reader counts as data (unused currencies, the `template` commodity).
- **Locks.** red-ale refuses a book GnuCash holds; GnuCash refuses one red-ale holds; red-ale's row goes on close, on
  a session that turns out read-only, on a failed open, on a failed save followed by close, and on drop; a row left
  by "another host" stays. GnuCash's "Open Anyway" while red-ale holds the book, and an edit by a GnuCash read-only
  session, each make red-ale's next save fail with `book-changed`, writing nothing.
- **Read-only findings.** One test per row of the table in §1, each naming what it found; a `versions` row changed
  down and up for every table, `Gnucash-Resave` 19921, `Gnucash` 19919 and 5000018.
- **Values.** Both timestamp forms read; an edited row is rewritten in ISO and its untouched neighbours are not; every
  other form is a finding. A property test that every finite double's bits survive the reader's text and the
  writer's parse, and that the text equals C's `%.18g` (Python's `'%.18g' %`, which needs no GnuCash).
- **Backups.** The backup holds the rows before the save and no lock row, opens in GnuCash 5.17 unlocked, is made
  once per session, and is deleted by retention like an XML backup.
- **Creating.** A created book's `sqlite_master` and `versions` equal those of the oracle's E1 book, its pragmas are
  GnuCash's, and GnuCash 5.17 opens it with nothing logged.
- **Scale.** The 100,000-transaction book as SQLite within [`milestone3.md`](milestone3.md)'s memory budgets, and a
  one-transaction save of it writing only that transaction's rows.
- **Fuzzing.** An SQLite file is outside input. `sql_rows` feeds arbitrary cells (NULL, INTEGER, REAL, TEXT, BLOB)
  to every table's row decoder and arbitrary slot rows to the Frame builder; `sql_file` opens mutated copies of the
  fixtures read-only. Neither may panic, and every refusal is a finding or an error.

## 10. Dependencies

**`rusqlite`** with `default-features = false` and the features `bundled` (compile SQLite in, so its version is
pinned, not the system's: Decided item 3) and `backup` (the online backup API). At the time of writing that is
`rusqlite` 0.40.2 with `libsqlite3-sys` 0.38.2, bundling SQLite 3.53.2. The default features are left off: `cache`
(`hashlink`) isn't needed, since a save prepares each statement once, and `ffi-sqlite-wasm-rs` is for wasm only.

**Licences against `deny.toml`.** `rusqlite` and `libsqlite3-sys` are MIT; their dependencies (`bitflags`,
`fallible-iterator`, `fallible-streaming-iterator`, `smallvec`, and at build time `cc`, `pkg-config`, `vcpkg`) are
MIT or Apache-2.0. All are on the allow-list, so `cargo deny check` passes with no exception. SQLite itself is in the
public domain (the blessing at the top of `sqlite3.h`); it is source inside `libsqlite3-sys`, which cargo-deny judges
by that crate's MIT licence, and public-domain code is compatible with GPL-3.0-or-later.

**C code.** `bundled` compiles SQLite's C amalgamation with `cc`: a second C library in the build beside `ring`,
recorded as `ring` is ([`architecture.md`](architecture.md), "Dependencies"). No system SQLite is linked. Its compile
options include extension loading, which red-ale never turns on (no `load_extension` feature, and
`SQLITE_DBCONFIG_ENABLE_LOAD_EXTENSION` stays off). Pure-Rust SQLite engines are not yet ready to trust with a
user's books (Decided item 3).

**`rustix`** (feature `system`) gives the lock row's host name, as `gethostname` does, without `unsafe` in red-ale.
It is already in `Cargo.lock` through `tempfile`; its licence (Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT)
is on the allow-list.

## API changes

Additive within `redAleApi` 0.4.0:

- `book.open` takes SQLite files and `sqlite3://`; `book.create` takes `format` (`xml`, `sqlite`); `book.info` gives
  `format` and, for SQLite, the `versions` rows; `book.save`'s `compressed` is `invalid-params` on an SQLite book.
- `readOnlyReasons` gains `schema-version`.
- `book-locked` on an SQLite book gives `data.path` (the book) and `data.holder` (`hostname`, `pid`).
- A new error, `book-changed` (1013): another program wrote the SQL book or broke its lock since red-ale opened it
  (or last saved it); nothing was written.
- `save-verification` on an SQLite book means the transaction was rolled back and the book is unchanged.
