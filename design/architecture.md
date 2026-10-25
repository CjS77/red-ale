# Architecture

## Bounded contexts and crates

Each bounded context is one crate with its own model. A crate uses another only through its public API.

| Crate | Context | Job | May depend on |
|-------|---------|-----|---------------|
| `red-ale-core` | Bookkeeping domain | The Book model and its invariants, and every domain operation (post, void, scrub, lots, capital gains, invoices, payments). Pure: no I/O, no async, no file formats. | — |
| `red-ale-xml` | XML file format | Anti-corruption layer: parses an XML Book file into a core `Book` plus the Residue it does not model, and writes them back as GnuCash 5.17 does. | core |
| `red-ale-service` | Book sessions | Use cases over an open Book: open, lock, read-only triggers, atomic mutations, journal, save, backups, replay. | core, xml |
| `red-ale-rpc` | Published language | The JSON-RPC 2.0 methods, their schemas and error codes, and a transport-free dispatcher; plus the `jsonrpsee` server. | core, service, report |
| `red-ale` | Binary | `red-ale serve` (the JSON-RPC server) and the CLI front end. The only crate that uses `anyhow`. | rpc, service |
| `red-ale-report` | Reports (v0.2) | The Rhai report engine and its sandbox, the read-only query API, report options, the report result (a published language), and the HTML and SVG renderer with its style sheets. Built-in report scripts are compiled in. Pure: no I/O; script sources and settings come in as values. | core |
| `red-ale-testkit` | Testing | Fixture paths, temporary books, a deterministic dispatcher, and the GnuCash oracle harness. Used only by tests. | core, xml, service, rpc |

`red-ale-mcp` arrives in v0.2 as a thin adapter over `red-ale-rpc`'s dispatcher. `red-ale-report` arrives in v0.2
(see [`reports.md`](reports.md)); `red-ale-rpc` depends on it for the `report.*` methods, and `red-ale-service` reads
the report directory and keeps saved report configurations in the Configuration. `red-ale-sql` arrives in v0.3 as a
second Backend beside `red-ale-xml`.

## Context map

```
           GnuCash XML file (external model)
                       │
                       ▼
   ┌────────────── red-ale-xml ──────────────┐  anti-corruption layer
   │ DOM ⇄ core Book  +  Residue (kept raw)   │
   └──────────────────────────────────────────┘
                       │ core::Book
                       ▼
   red-ale-service  (Session: lock, journal, backups, atomic use cases)
                       │
                       ▼
   red-ale-rpc  (published language: JSON-RPC 2.0, versioned schema)
          │                              │
          ▼                              ▼
   red-ale serve (HTTP, localhost)   red-ale CLI (in-process or client)
                                     red-ale-mcp (v0.2, conformist to MCP)
```

- **XML file ⇄ core:** anti-corruption layer. File quirks (element names, `quantity` for amount, slot paths used only
  by the file) stay in `red-ale-xml`.
- **core ⇄ service:** customer/supplier. The service decides *when*; core decides *what is valid*.
- **service ⇄ rpc:** the rpc crate translates wire types to core types and back; it holds no domain rules.
- **rpc ⇄ clients:** published language (see [`api.md`](api.md)).
- **core → report:** customer/supplier. The report engine reads a snapshot of the Book through core's read
  operations; reads that reports need and core lacks are added to core. **report → rpc:** the report result is a
  published language owned by `red-ale-report`, served unchanged by `report.run`.

## Main decisions

### The core owns the model; the Backend owns the file

`core::Book` holds every entity red-ale models: commodities, accounts, transactions with their splits, prices, lots,
and the business objects. The Book's own Slots and each entity's Slots are kept whole, as a Frame, so slots red-ale
does not interpret are never lost.

Everything else in the file is Residue, kept by the Backend beside the Book (see [`file-fidelity.md`](file-fidelity.md)):
scheduled transactions, template transactions, budgets, and any element a later GnuCash adds. The Session carries the
Book and the Residue together; only the Backend reads the Residue.

### Order is kept

Entities keep the order they were read in (`IndexMap`), and new ones are appended to their section. GnuCash does not
care about order on load, and a file written by GnuCash 5.17 then gives a byte-identical round-trip.

### Mutations are atomic by copy-on-write

Entities sit behind `Arc`, so cloning a Book costs one pointer copy per entity. A Mutation clones the Book, applies
the change to the clone through `Arc::make_mut`, checks the Book's invariants, and only then replaces the Session's
Book. A failed Mutation leaves nothing behind.

### Money is exact

`core::Numeric` is a 64-bit rational with GnuCash's arithmetic and rounding modes, using 128-bit intermediates. The
crate denies `clippy::float_arithmetic`. A `double` slot is stored as the text it was read as and never computed with.

### Determinism

No `HashMap` is iterated for output. Slots are a `BTreeMap` (GnuCash writes them sorted by key with `strcmp`, which
is byte order). Collections are `IndexMap`.

### One Session per server

A `red-ale serve` process has at most one open Session. `book.open` fails while one is open. This keeps v0.1 simple;
see [`open-questions.md`](open-questions.md).

### Reports only read

A report runs on a clone of the Book, held where only the read-only query handles can see it. `red-ale-report` never
holds a `&mut Book` and does not depend on `red-ale-service`, so it can reach neither a Mutation nor the journal;
report methods use `Context::query`, never `mutate`, so a Session is never made dirty by a report. Tests check all
three on every built-in report (see [`reports.md`](reports.md) §1).

### Errors

Library crates return typed errors (`thiserror`). `red-ale-rpc` maps each to a stable JSON-RPC error code listed in
[`api.md`](api.md). A malformed file is an error, never a panic.

## Dependencies

All licences must be GPL-3.0 compatible; `deny.toml` enforces the allow-list.

| Crate | Used by | Why |
|-------|---------|-----|
| `thiserror` | all libraries | typed errors |
| `indexmap` | core | ordered entity collections |
| `chrono` | core, xml, service | calendar arithmetic and formatting (no clock in core) |
| `quick-xml` | xml | XML tokenising; red-ale builds its own small DOM and serialiser to match libxml2's output |
| `flate2` | xml | gzip, through the pure-Rust `miniz_oxide` |
| `serde`, `serde_json`, `schemars` | rpc, report | wire types, the report result and their JSON Schema |
| `jsonrpsee` | rpc | JSON-RPC 2.0 server over HTTP |
| `tokio` | rpc, binary | async runtime for the server |
| `clap`, `anyhow` | binary | CLI |
| `rhai` | report | report scripting (`no_float`, `no_time`; see [`reports.md`](reports.md)) |
| `num-bigint`, `num-rational` | report | the unbounded exact Rational of report arithmetic |
| `proptest`, `tempfile` | tests | property tests and temp directories |
