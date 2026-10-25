# Testing

A feature counts as done when tests prove it at each layer it touches, and GnuCash 5.17 itself agrees with the
result. Tests never touch real books: every test works on a copy in a temporary directory.

## Layers

| Layer       | Where                                             | Proves                                                      |
|-------------|---------------------------------------------------|-------------------------------------------------------------|
| Unit        | `#[cfg(test)]` beside the code                    | domain rules, arithmetic, parsing                           |
| Property    | `proptest`, beside the code                       | invariants: balance, exact arithmetic, rounding as GnuCash  |
| Round-trip  | `crates/red-ale-xml/tests/round_trip.rs`          | canonical books come back byte for byte, both from their original XML and fully regenerated |
| Session     | `crates/red-ale-service/tests/`                   | lock, read-only triggers, SQL refusal, atomic mutations, journal, backups |
| API         | `crates/red-ale-rpc/tests/`, testkit non-oracle tests | each method through the dispatcher, with its error codes |
| CLI         | `crates/red-ale/tests/cli.rs`                     | `exec`, `call`, `serve`, exit codes, scripts                |
| Oracle      | `crates/red-ale-testkit/tests/oracle_*.rs`        | GnuCash 5.17 reads what red-ale wrote and sees the same data, and computes the same answers |

```
cargo test --workspace                              # everything but the oracle
cargo test -p red-ale-testkit --features oracle     # the oracle tests (needs GnuCash 5.17)
cargo clippy --workspace --all-targets --features red-ale-testkit/oracle -- -D warnings
```

## Fixtures

`fixtures/` holds synthetic books and books from GnuCash's and piecash's repositories (see `fixtures/README.md` for
sources and licences). Never personal financial data.

- **Canonical** books are those GnuCash 5.17 loads and saves back byte for byte (`scripts/oracle-resave.py`). They
  must round-trip byte for byte through red-ale.
- `synthetic/rich.gnucash` exercises every core entity and slot type, special characters (including tabs and
  newlines in text), a budget, a scheduled transaction and template transactions.
- `synthetic/business.gnucash` has every business object, including a credit note (it declares the Book feature
  `Credit Notes`); `synthetic/business-editable.gnucash` is the same book without the credit note.
- Some fixtures open read-only by design: `piecash/simple_sample.gnucash` and `piecash/investment.gnucash` use
  trading accounts; `piecash/simple_sample.sqlite.gnucash` is an SQL book (refused);
  `gnucash/currency_tree_xml.gnucash` is a version 1 file (refused).

## The oracle

GnuCash 5.17's engine, through its Python bindings, built from `$GNUCASH_SOURCE` at the pinned tag and installed in
`$HOME/.local/opt/gnucash-5.17` (see `CLAUDE.md`). `red-ale-testkit::oracle` runs it as a subprocess with
`PYTHONPATH` set and `TZ=UTC`; set `RED_ALE_ORACLE_PYTHONPATH` to use another install. Every call first checks that
the oracle reports version 5.17 and fails otherwise: the system GnuCash is never the oracle.

The shared comparison:

- `oracle/dump.py` opens a book read-only in GnuCash and prints GnuCash's view of it as canonical JSON: commodities,
  accounts (with balances), transactions and splits, prices, lots, and business objects.
- `model::dump` builds the same JSON from a red-ale `Book`. Where GnuCash derives a value at load (a currency's quote
  flag, a lot's closed state, an invoice's paid state), `model.rs` reproduces the derivation and cites it.
- `diff` lists the differences by path. Conventions on both sides: amounts as reduced fractions (GnuCash reduces what
  it returns; stored denominators are checked by the byte-level tests), times in epoch seconds, lists sorted by GUID or
  a natural key.
- `oracle_check(&mut dispatcher, &book, ignore)` saves and closes the book, then compares GnuCash's reading of the file
  with red-ale's.

Every fixture is compared as read, after red-ale regenerates every entity, and as a new book red-ale creates.

Feature oracle tests go further: they make GnuCash compute the answer itself (a balance, an invoice total, a due
date, capital gains, the result of a scrub, a void) on one copy of a book, make red-ale do the same through its API on
another copy, and compare.

Known limits of the Python bindings, worked around in `dump.py`: business getters may return raw SWIG pointers
(wrapped in their classes), the GLists of tax table entries and of bill terms come back mistyped (tax tables are
compared by name and entry count; bill terms through the customers, vendors and invoices that use them), and prices,
lots and invoices expose no GUID accessor (they are matched by natural keys).

Business-only edits do not mark a GnuCash book dirty, so a script that must save one also makes a no-op account
rename.
