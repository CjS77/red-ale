# red-ale-testkit

Test support for red-ale: fixture books, temporary copies, a predictable dispatcher, and the GnuCash *oracle*.

## Role

red-ale promises that GnuCash reads whatever red-ale writes, and agrees with what red-ale computes. This crate is how
that promise is tested.

- **Fixtures.** `fixture` and `xml_fixtures` find the books in [`fixtures/`](../../fixtures): synthetic books and
  books from GnuCash's and piecash's test suites. There is never any personal financial data.
- **Temporary copies.** `TempBook` copies a fixture into a temporary directory, so tests never touch a real book.
- **A predictable dispatcher.** `dispatcher`, `open` and `create` give a `red-ale-rpc` dispatcher with a fixed clock
  and sequential GUIDs, so results are the same on every run.
- **The oracle.** GnuCash 5.17's own engine, run through its Python bindings. `oracle::dump` opens a book in GnuCash
  and prints what GnuCash sees as JSON. `model::dump` prints red-ale's view in the same form, and `diff` lists the
  differences. Every oracle call first checks that it is talking to GnuCash 5.17 exactly, never the system's GnuCash.

- **Synthetic books and memory.** `synthetic` builds books of any size in GnuCash 5.17's format, and `memory`
  measures the heap through `book.open`, `transaction.post` and `book.save`. The `memory` example prints those
  figures for a large book; `tests/memory_budget.rs` holds the budgets of
  [`design/milestone3.md`](../../design/milestone3.md) ("Memory and scale").

## Where it fits

This crate supports testing only, and red-ale never ships it. It depends on all the library crates, and the
end-to-end tests live in its `tests/` directory:

- `oracle_*.rs` tests have GnuCash do something to one copy of a book and red-ale do the same through its API to
  another copy, then compare the results. Examples are an invoice total, a scrub, capital gains and a void.
- The other tests in `tests/` run through the API without GnuCash.

See [`design/testing.md`](../../design/testing.md) for how the layers of tests fit together.

## Usage

A test that changes a book through the API and checks the result with GnuCash:

```rust
use red_ale_testkit::{call, open, oracle_check};
use serde_json::json;

#[test]
fn a_new_account_reads_the_same_in_gnucash() {
    let (book, mut d) = open("synthetic/rich.gnucash"); // a temporary copy, opened for writing
    call(&mut d, "account.create", &json!({ "name": "Savings", "type": "BANK", "parent": "Assets" }));
    let differences = oracle_check(&mut d, &book, &[]); // save, close, and compare with GnuCash's reading
    assert!(differences.is_empty(), "{differences:?}");
}
```

Running the tests:

```sh
cargo test -p red-ale-testkit                    # API tests, no GnuCash needed
cargo test -p red-ale-testkit --features oracle  # also the oracle tests
```

The oracle tests need GnuCash 5.17 built with its Python bindings and installed in `~/.local/opt/gnucash-5.17` (see
[`CLAUDE.md`](../../CLAUDE.md) for the build options). To use another install, set `RED_ALE_ORACLE_PYTHONPATH` to its
`site-packages` directory.
