# red-ale

A Rust reimplementation of GnuCash with (eventually) feature parity. It runs headless and is controlled entirely through JSON-RPC,
with an MCP server so AI agents can drive it. 

The name: 
* **red** because Rust, 
* **ale** from the accounting formula `Assets = Liabilities + Equity`.

## Mission

1. **Feature parity with GnuCash**, measured against one pinned upstream release (see "GnuCash as upstream").
2. **Everything controllable over JSON-RPC 2.0.** No operation exists only inside the code or a UI.
3. **An MCP server** that exposes the same operations to AI agents.
4. **Drop-in file compatibility.** A user copies their existing `.gnucash` file, red-ale opens and edits it, and GnuCash
   opens the result with no warnings and no lost data. This goal outranks the other three: a feature that risks a
   user's books does not ship.

**Status: greenfield, design phase.** Design documents live in `design/` and come before code. Start at
`design/README.md` (reading order), then `design/mission-statement.md`, `design/glossary.md`,
`design/features.md` (the checklist against upstream) and `design/open-questions.md`. Create any that are missing
before writing code that depends on them.

## File compatibility: rules that are never broken

The GnuCash file is the user's financial record. Treat every write to it as irreversible.

- **Never lose data.** Anything red-ale does not understand (unknown XML elements, KVP slots, SQL tables or columns,
  book features) is read, kept and written back unchanged. Round-tripping a file with no edits must give back a file
  GnuCash considers identical.
- **Money is exact.** GnuCash stores amounts as rationals (`num/denom`, 64-bit). Never use `f32`/`f64` for amounts,
  prices or rates anywhere: not in the domain, not on the wire, not in tests. Keep the original denominator; rounding
  follows the commodity's SCU (smallest commodity unit) and GnuCash's rounding rules.
- **Value is not quantity.** A split's *value* is in the transaction's currency; its *quantity* (GnuCash: amount) is in
  the account's commodity. Every transaction's split values sum to exactly zero. Both are kept; neither is derived from
  the other on save.
- **Keep identity.** GUIDs, the order of entities in the file, and timestamps (including their time zone handling) are
  kept as read.
- **Respect the lock.** XML books use a `.LCK` file beside them; SQL books use the `gnclock` table. Never open for writing
  a book that GnuCash or another red-ale holds, and always release the lock on exit, including on error paths.
- **Respect book features.** A book declares the features it relies on in its `features` slot. If a file declares a
  feature red-ale does not implement, open it read-only and say why. Never quietly drop a feature or add one GnuCash
  would not recognise.
- **Back up like GnuCash.** Match GnuCash's timestamped backup and `.log` journal behaviour for XML books before
  overwriting.
- **Tests never touch real books.** Work on copies in a temp directory. Never commit personal financial data. Fixtures
  are synthetic or come from GnuCash's own test files.
- **Extensions must round-trip.** Anything beyond parity (for example richer derivatives support) is stored in a way
  GnuCash keeps and ignores, such as KVP slots, and gets a design doc first.

Both backends are in scope: XML (gzipped or plain) and SQL (SQLite first; PostgreSQL and MySQL later).

## GnuCash as upstream

- **One pinned release is the parity target: GnuCash 5.17**, recorded in `design/GNUCASH_REV` (tag, then commit). Pin
  released tags only, never the tip of `stable`.
- **`$GNUCASH_SOURCE` is the GnuCash checkout**, kept at the pinned tag. Set it in your shell profile
  (`git clone https://github.com/Gnucash/gnucash "$GNUCASH_SOURCE"`). If it is unset, ask rather than guess a path.
  Read pinned source with `git -C "$GNUCASH_SOURCE" show 5.17:<path>` when the working tree is on another revision.
- **Claims about GnuCash behaviour or file format must be checked against its source and cited** as
  `tag:path/to/file.cpp:line`. Mark anything taken only from the wiki or docs as docs-only. The format is defined by
  `libgnucash/backend/xml/` and `libgnucash/backend/sql/`, not by the wiki.
- **No Guile, ever.** GnuCash's reports are Guile Scheme. red-ale never embeds or runs Guile. From v0.2 each report
  is translated from its GnuCash `.scm` file into a Rhai script (`.rhai`), keeping its options, logic and figures.
  Reports, and from v0.3 the importers (translated from GnuCash's C, C++ and, for QIF, Scheme), are the exceptions
  to "port only when the behaviour can't be matched any other way" (see "Licensing", `design/milestone2.md` and
  `design/milestone3.md`).
- **GnuCash is the test oracle.** Compatibility tests write a book with red-ale, open it with GnuCash, and compare. The
  oracle must run the pinned version; if its version differs from the pin, the tests say so and fail.
  - The oracle is GnuCash 5.17: the library and Python bindings, with no GUI. It is installed in
    `$HOME/.local/opt/gnucash-5.17`, built from `$GNUCASH_SOURCE` in a separate build directory. XML and SQLite both
    work.
  - To use it, set `PYTHONPATH=$HOME/.local/opt/gnucash-5.17/lib/python3.12/site-packages`. `LD_LIBRARY_PATH` isn't
    needed because the bindings carry their own library path. Never run oracle scripts with `python3 -I`, which ignores
    `PYTHONPATH`.
  - Check the version before anything else: `from gnucash import _sw_core_utils; _sw_core_utils.gnc_version()` must
    return `5.17`. The system GnuCash (5.5, `/usr/lib/python3/dist-packages/gnucash`) loads silently when `PYTHONPATH`
    is missing. It is never the oracle.
  - Open books with `Session("xml://<path>")` or `Session("sqlite3://<path>")`, using
    `SessionOpenMode.SESSION_NEW_OVERWRITE` to create and `SESSION_READ_ONLY` to inspect. Always call `end()` so the
    lock is released.
  - GnuCash reduces fractions when it returns them (`GetBalance()` gives `-617/50`) but stores what was written
    (`1234/100`). Compare amounts by value, and compare stored denominators separately.
  - Saving writes a `<book>.<timestamp>.log` journal beside the book. A warning that `pref_transformations.xml` is
    missing is expected from a build with no GUI.
  - Rebuild after moving the pin: check out the new tag, then configure with
    `-DCMAKE_INSTALL_PREFIX=$HOME/.local/opt/gnucash-<tag> -DWITH_GNUCASH=OFF -DWITH_PYTHON=ON -DWITH_SQL=ON
    -DWITH_AQBANKING=OFF -DWITH_OFX=OFF`, then `ninja` and `ninja install`. GnuCash's own `ninja check` needs the
    `en_US`, `en_GB` and `fr_FR` UTF-8 locales.
- **Licensing:** red-ale is **GPL-3.0-or-later** (`LICENSE`). Every crate sets `license = "GPL-3.0-or-later"`.
  - GnuCash is GPL-2.0 or GPL-3.0 (most files GPL-2.0-or-later), so its code may be ported under GPL-3.0. Check the
    header of each file you port. Keep the original copyright lines, and cite the source file in the ported code.
  - Port only when the behaviour can't be matched any other way. Rust code designed from our notes in `design/` is the
    default. Reports and importers are the exceptions: they are translated from GnuCash's source.
  - Test files taken from GnuCash's repository keep their licence notice.
  - Every dependency must be GPL-3.0 compatible (MIT, Apache-2.0, BSD, ISC, MPL-2.0, Zlib, and so on).
    `cargo deny` enforces the allow-list in `deny.toml`.
  - Programs that use red-ale (Naomi, for one) talk to it only through JSON-RPC or MCP. Never ship a red-ale crate as a
    library for them to link.

## Domain language

Domain-Driven Design, strategic ideas first: code that says what bookkeeping means, parts with one clear job, rules that
are hard to break by accident.

- **Ubiquitous language.** `design/glossary.md` names things, in docs and code alike: one name per concept, defined once.
  A new concept enters the glossary before the code. **GnuCash's terms come first** (Book, Account, Transaction, Split,
  Commodity, Price, Lot, Slot, Scheduled Transaction, Budget, Invoice, Owner). Flag every clash in the glossary,
  for example:
  - GnuCash *amount* means a split's quantity, not a generic number;
  - a *currency* is a commodity in the `CURRENCY` namespace;
  - `STOCK` is an account type, not warehouse inventory;
  - *value* always means transaction-currency value.
- **Bounded contexts**, each with its own model and crate; don't reach into another's internals. Proposed, to be settled
  in `design/`:
  - `red-ale-core`: the domain model and its invariants. Pure: no I/O, no async, no serde for file formats.
  - `red-ale-xml`, `red-ale-sql`: the file backends.
  - `red-ale-service`: use cases and the book session (open, lock, edit, commit, save).
  - `red-ale-rpc`: the JSON-RPC server and its schema.
  - `red-ale-mcp`: the MCP server.
  - `red-ale`: the binary.
  - `red-ale-testkit`: fixtures, builders and the GnuCash oracle harness.
- **Context map.**
  - The GnuCash file format is an external model. Each backend is an anti-corruption layer that translates to and from
    the core model, so file quirks never leak into the domain.
  - The JSON-RPC schema is a published language: versioned, documented, and changed only additively within a major
    version.
  - The MCP server conforms to the MCP spec and is a thin adapter over the same use cases as JSON-RPC, with no logic of
    its own.
- **Tactical patterns where they earn their place:**
  - value objects as newtypes that are valid on construction (`Guid`, `Numeric`, `CommodityRef`);
  - entities by GUID;
  - a Transaction with its Splits as the aggregate that keeps itself balanced;
  - domain events in the past tense (`TransactionPosted`).

  Keep domain logic pure, with I/O at the edges.

## JSON-RPC and MCP

- **JSON-RPC 2.0**, with methods namespaced by context: `book.open`, `account.create`, `transaction.post`,
  `price.add`, and so on.
- Every mutation is atomic: it applies completely or not at all, and leaves the book balanced.
- Errors use stable, documented codes.
- **Amounts on the wire are exact.** Send them as rational `{ "num": "...", "denom": "..." }` or decimal strings, never
  as JSON numbers.
- **One operation, two doors.** Each MCP tool maps to a JSON-RPC method and shares its input schema. Mark destructive
  tools with MCP's `destructiveHint`. Read-only tools say so.
- **Safe by default.** Listen on localhost or a Unix socket unless configured otherwise. Authentication and
  remote access are design questions; don't add them ad hoc.
- Use `rmcp` (the official MCP Rust SDK) and `jsonrpsee`, unless `design/` decides otherwise.

## Rust conventions

- Edition 2024; toolchain pinned in `rust-toolchain.toml`; one Cargo workspace with dependency versions held in
  `[workspace.dependencies]`.
- Formatting follows `rustfmt.toml`: max width 140, 4-space indents, no tabs. Run `cargo fmt` before committing.
- Prefer iterators and combinators over `for` loops where they read as well.
- Use inline format arguments: `format!("bad split {guid}")`.
- Keep functions short and doing one thing; split long ones into named helpers.
- Libraries return typed errors with `thiserror`; only the binary uses `anyhow`.
- No `unwrap`, `expect` or `panic!` outside tests. A malformed file is an error, never a crash.
- `#![forbid(unsafe_code)]` in every crate.
- Workspace lints: `clippy::all` and `clippy::pedantic` warn; `clippy::float_arithmetic` is denied in core and the
  backends.
- Public items have doc comments. Comments say *why*, not what.
- Output must be deterministic: no `HashMap` iteration order in anything that is written to disk or the wire.

## Commands

```
cargo build --workspace
cargo test --workspace                         # unit, property and round-trip tests
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p red-ale-testkit --features oracle # tests that run real GnuCash (slower)
cargo deny check && cargo audit                 # licences and advisories
```

Before a PR: tests and clippy are clean, and any change to a backend passes the oracle tests.

## Testing

- **Round-trip tests** for every backend and fixture: open, save without changes, compare, then open in GnuCash.
- **Property tests** (`proptest`) on core invariants: balanced transactions, exact arithmetic, and rounding that
  matches GnuCash.
- **Parity tests:** each item in `design/features.md` links to the tests that prove it.
- Bugs get a failing test first.

## Workflow

Use the Kanban board to plan, track and review work.

- **Tickets are small and self-contained.** A PR's code (excluding docs and tests) should be under 400 lines and never
  over 1000; past 500, look for a split.
- **Each feature is an Epic.** Its description says what the feature does and why, so context isn't lost across small
  tickets. Parity features name the GnuCash behaviour they match, with citations.
- **Reviews:** re-read the mission and the relevant design docs, then ask:
  - Does this clearly move us toward the mission?
  - Does it fit the design?
  - Could it damage a user's book?
  - What code should be *removed*? The only bug-free code is code never written. Code that is not in the ticket's scope
    comes out, even if useful; argue for it in a new ticket.
- **Commits** follow Conventional Commits (`feat(sql): read scheduled transactions`). Collect dependency changes
  under a "Dependency updates" heading, one line each: `crate: old => new`.
