# red-ale-xml

Reads and writes GnuCash XML book files.

## Role

This crate turns the bytes of a `.gnucash` XML file (plain or gzip-compressed) into a `red_ale_core::Book`, and turns
a `Book` back into bytes that GnuCash 5.17 reads with no warnings.

GnuCash's file is the user's financial record, so the crate's first job is to lose nothing:

- **Anything red-ale doesn't model is kept.** Unknown elements, slots, scheduled transactions, budgets and other
  out-of-scope data go into a *residue* (`XmlResidue`) and are written back unchanged.
- **Unchanged books come back byte for byte.** Entities that weren't edited are written from their original elements.
  The serialiser matches libxml2's output, the library GnuCash writes with, so even regenerated entities come out as
  GnuCash would write them.
- **Identity and order are kept.** GUIDs, the order of entities in the file, denominators and timestamps stay as read.
- **Version 1 files are refused**, with a message to open and save them in GnuCash first.

A malformed file gives an `XmlError`, never a crash.

## Where it fits

In domain-driven design terms, this crate is an *anti-corruption layer*. GnuCash's file format is an outside model with
its own quirks: element names, value encodings and slot conventions. This crate translates between that model and
`red-ale-core`, so none of those quirks leak into the bookkeeping code.

```text
.gnucash file  <->  red-ale-xml  <->  red-ale-core (Book)
```

It depends only on `red-ale-core`. It does no locking, backups or journaling; those belong to `red-ale-service`, which
is the only crate that calls this one in normal use. An SQLite backend, `red-ale-sql`, will sit beside it from v0.4.

## What's inside

| Module     | Contents                                                                                     |
|------------|----------------------------------------------------------------------------------------------|
| `book`     | `read_book`, `write_book`, `write_book_text`, and `XmlResidue`                               |
| `envelope` | The XML declaration, the `gnc-v2` root and its namespaces, and the book wrapper              |
| `entities` | Commodities, prices, accounts, lots, transactions and splits                                 |
| `business` | Customers, vendors, employees, jobs, invoices, entries, orders, tax tables and billing terms |
| `values`   | Shared encodings: GUIDs, numerics, timestamps, dates, commodity references and slots         |
| `dom`      | A small XML tree, and the serialiser that writes it as libxml2 does                          |

## Usage

Read a book, look inside, and write it back:

```rust
let bytes = std::fs::read("home.gnucash") ?;
let (book, residue) = red_ale_xml::read_book( & bytes) ?;
println!("{} accounts, {} transactions", book.accounts.len(), book.transactions.len());

// Nothing changed, so the same text comes back.
let text = red_ale_xml::write_book_text( & book, & residue);
assert_eq!(text, red_ale_xml::book::decode(&bytes)?.0);

// Bytes ready for disk, compressed if the original was.
let out = red_ale_xml::write_book( & book, & residue, residue.compressed) ?;
```

To edit a real book, go through `red-ale-service` instead: it takes GnuCash's lock and makes a backup before writing.
