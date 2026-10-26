# Architecture

## Bounded contexts and crates

Each bounded context is one crate with its own model. A crate uses another only through its public API.

| Crate | Context | Job | May depend on |
|-------|---------|-----|---------------|
| `red-ale-core` | Bookkeeping domain | The Book model and its invariants, and every domain operation (post, void, scrub, lots, capital gains, invoices, payments). Pure: no I/O, no async, no file formats. | — |
| `red-ale-xml` | XML file format | Anti-corruption layer: parses an XML Book file into a core `Book` plus the Residue it does not model, and writes them back as GnuCash 5.17 does. | core |
| `red-ale-service` | Book sessions | Use cases over an open Book: open, lock, read-only triggers, atomic mutations, journal, save, backups, replay. | core, xml, locale |
| `red-ale-rpc` | Published language | The JSON-RPC 2.0 methods, their schemas and error codes, and a transport-free dispatcher; plus the `jsonrpsee` server. Holds the open book's Quote fetches and stores their prices. | core, service, report, locale, export, quotes |
| `red-ale-mcp` | MCP adapter | Conformist to the MCP specification: one tool per JSON-RPC method, served over stdio, each call run through the `Dispatcher`. No logic of its own ([`mcp.md`](mcp.md)). | rpc |
| `red-ale` | Binary | `red-ale serve` (the JSON-RPC server), `red-ale mcp` (the MCP server) and the CLI front end. The only crate that uses `anyhow`. | rpc, mcp, service |
| `red-ale-report` | Reports (v0.2) | The Rhai report engine and its sandbox, the read-only query API, report options, the report result (a published language), and the HTML and SVG renderer with its style sheets. Built-in report scripts are compiled in. Pure: no I/O; script sources and settings come in as values. | core, locale |
| `red-ale-import` | Import (v0.3) | The importers translated from GnuCash (CSV, QIF, business CSV), the generic import matcher, Import maps and the Import proposal. Pure: no I/O; file content, Import settings and the Book come in as values, and the service applies a proposal as one Mutation. | core, locale |
| `red-ale-export` | Export (v0.3) | The CSV exports of the account tree and transactions, translated from GnuCash (`design/api/export.md`). Pure: no I/O; reads the Book and returns text. Numbers print through the report renderer's `xaccPrintAmount` translation under a Locale. Export accounts, which writes a file, is the XML backend's writer and the service's use case. | core, report, locale |
| `red-ale-locale` | Locales (v0.3) | The Locale table generated from glibc's locale sources (number format, `D_FMT`, day and month names) and GnuCash's view of it (`gnc_localeconv`); the first day of the week and date intervals from ICU4X. Pure data and lookups: the environment comes in as a function. | — |
| `red-ale-quotes` | Online quotes (v0.3) | The only crate that reaches the network ([`api/quotes.md`](api/quotes.md)). `format`: one parser per Quote service response format, pure functions from an answer's bytes to a quote record, with numbers read as text (`json_text`); `plan`: the requests for a commodity from the Configuration's Quote services and sources, keys bound to their hosts (`secret`); `fetch`: runs a Quote fetch's requests over HTTP on its own thread and tokio runtime. Which commodities are fetched and how a quote record becomes a Price are pure and live in `red-ale-core` (`ops::quotes`). | core, service |
| `red-ale-testkit` | Testing | Fixture paths, temporary books, a deterministic dispatcher, the GnuCash oracle harness, and a Quote service on 127.0.0.1 serving recorded answers. Used only by tests. | core, xml, service, rpc |

The core reaches the outside world only through `Env`: the clock, fresh GUIDs and the Local time zone. The zone is a
value (`TimeZone`) built from data, never from the environment; `red-ale-service` reads `TZ`, `TZDIR` and the zone
files once per process, in GnuCash's order, and hands the zone to every session's `Env`. Tests pin a zone with
`FixedEnv::with_zone`.

`red-ale-report` arrives in v0.2 (see [`reports.md`](reports.md)); `red-ale-rpc` depends on it for the `report.*`
methods, and `red-ale-service` reads the report directory and keeps saved report configurations in the
Configuration. `red-ale-sql` arrives in v0.4 as a second Backend beside `red-ale-xml`.

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
                                     red-ale-mcp → red-ale mcp (stdio, conformist to MCP)
```

- **XML file ⇄ core:** anti-corruption layer. File quirks (element names, `quantity` for amount, slot paths used only
  by the file) stay in `red-ale-xml`.
- **core ⇄ service:** customer/supplier. The service decides *when*; core decides *what is valid*.
- **service ⇄ rpc:** the rpc crate translates wire types to core types and back; it holds no domain rules.
- **rpc ⇄ clients:** published language (see [`api.md`](api.md)).
- **rpc ⇄ MCP:** conformist. `red-ale-mcp` adopts the MCP specification's model as it is and maps the published
  language onto it: method to tool, Effect to annotations, error object to an `isError` result (see [`mcp.md`](mcp.md)).
- **core → report:** customer/supplier. The report engine reads a snapshot of the Book through core's read
  operations; reads that reports need and core lacks are added to core. **report → rpc:** the report result is a
  published language owned by `red-ale-report`, served unchanged by `report.run`.
- **Quote services ⇄ core:** anti-corruption layer. Each service's answer is read by its format's parser in
  `red-ale-quotes` into a quote record, the fields Finance::Quote hands GnuCash; no provider's field name reaches
  `red-ale-core`, whose price building (translated from `gnc-quotes.cpp`) sees only quote records.

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

### Quote fetches run beside the Book

A Quote fetch (`price.fetchQuotes`) must not hold up other methods while services answer slowly, and must work the
same in `red-ale serve`, `red-ale mcp` and `red-ale exec`, which have different runtimes or none. So its requests run
on a thread of their own with a single-threaded tokio runtime (`red_ale_quotes::fetch`), independent of whatever
started them; the dispatcher keeps only a handle, and dropping the handle cancels the fetch without waiting for the
thread. When the requests are done, the thread records their outcomes in the open book's registry of Quote fetches
(`red_ale_rpc::quote_fetches`), and the prices are added in one Mutation under the dispatcher's lock, in turn with
every other method:

- the HTTP and MCP servers install a notifier, holding only a weak reference to the dispatcher, that stores the
  prices as soon as the requests are done;
- every method call first stores a finished fetch's prices, so no door ever sees a Book without them;
- `red-ale exec` waits for the fetch, then stores them.

The registry belongs to the open book: closing the book drops it, which cancels a running fetch, and nothing from it
is stored. The fetch thread never waits for the dispatcher while holding anything the dispatcher waits for, so a
method running when a fetch ends simply goes first.

### Errors

Library crates return typed errors (`thiserror`). `red-ale-rpc` maps each to a stable JSON-RPC error code listed in
[`api.md`](api.md). A malformed file is an error, never a panic.

## Dependencies

All licences must be GPL-3.0 compatible; `deny.toml` enforces the allow-list.

| Crate | Used by | Why |
|-------|---------|-----|
| `thiserror` | all libraries | typed errors |
| `indexmap` | core | ordered entity collections |
| `chrono` | core, xml, service, report | calendar arithmetic and formatting (no clock in core) |
| `jiff` | core | time zone rules from TZif data and POSIX strings, with no I/O (default features off: no system zone lookup, no bundled database); the service reads the files |
| `quick-xml` | xml | XML tokenising; red-ale builds its own small DOM and serialiser to match libxml2's output |
| `flate2` | xml | gzip, through the pure-Rust `miniz_oxide` |
| `serde`, `serde_json`, `schemars` | rpc, report | wire types, the report result and their JSON Schema |
| `jsonrpsee` | rpc | JSON-RPC 2.0 server over HTTP |
| `tokio` | rpc, mcp, binary, quotes | async runtime for the servers, and each Quote fetch's own single-threaded runtime |
| `rmcp` | mcp | the official MCP SDK: protocol types and the stdio transport (Apache-2.0) |
| `clap`, `anyhow` | binary | CLI |
| `rhai` | report | report scripting (`no_float`, `no_time`; see [`reports.md`](reports.md)) |
| `num-bigint`, `num-rational`, `num-traits`, `num-integer` | report | the unbounded exact Rational of report arithmetic |
| `serde_path_to_error` | report | the path of a mismatch in a report result |
| `icu_calendar`, `icu_datetime`, `icu_locale_core`, `writeable` | locale | ICU4X with its compiled CLDR data (`Unicode-3.0`): the first day of the week and the interval form of date ranges, which GnuCash asks ICU for. `icu_datetime`'s range formatter is behind its `unstable` feature, so it is pinned to an exact version |
| `hyper`, `hyper-util`, `http-body-util` | quotes | HTTP/1.1 GET for Quote fetches (client only, no redirects). Chosen over `reqwest` and `ureq` because `jsonrpsee` already brings exactly this stack into the build (its HTTP client is `hyper-util`'s), so the fetcher adds no crate to `Cargo.lock`; `ureq` would add a second, blocking HTTP stack, and `reqwest` a layer over the same one |
| `hyper-rustls`, `rustls`, `rustls-native-certs` | quotes | HTTPS with the system's trust store. rustls's crypto provider is `ring`, with default features off so `aws-lc-rs` is never built. **`ring` is not pure Rust:** it compiles C and assembly with `cc` (licence `Apache-2.0 AND ISC`). Every production rustls provider does (`aws-lc-rs` is C, built with CMake; the pure-Rust `rustls-rustcrypto` is not production-ready), so `ring` is the least bad choice: smaller than `aws-lc-rs`, no CMake, and already linked for `red-ale call`'s HTTPS through `jsonrpsee`. No system C library is linked |
| `regex` | core, rpc | GnuCash's `GncNumeric(std::string)` and `GncDate` patterns; the `--namespace` pattern of a Quote fetch |
| `proptest`, `tempfile` | tests | property tests and temp directories |
