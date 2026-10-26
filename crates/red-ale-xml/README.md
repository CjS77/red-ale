# red-ale-xml

Reads and writes GnuCash XML book files.

## Role

This crate turns the bytes of a `.gnucash` XML file (plain or gzip-compressed) into a `red_ale_core::Book`, and turns
a `Book` back into bytes that GnuCash 5.17 reads with no warnings.

GnuCash's file is the user's financial record, so the crate's first job is to lose nothing:

- **Anything red-ale doesn't model is kept.** Unknown elements, slots, scheduled transactions, budgets and other
  out-of-scope data go into a *residue* (`XmlResidue`) and are written back unchanged.
- **Unchanged books come back byte for byte.** Entities are generated from the model, and the serialiser matches
  libxml2's output, the library GnuCash writes with, so they come out as GnuCash 5.17 would write them. An entity whose
  generated form differs from what was read (written by another program or an older GnuCash) keeps its original text
  in the residue, and is written from it until it is edited.
- **Identity and order are kept.** GUIDs, the order of entities in the file, denominators and timestamps stay as read.
- **Version 1 files are refused**, with a message to open and save them in GnuCash first.

A malformed file gives an `XmlError`, never a crash.

Books are read and written as streams, so memory stays near the size of the Book: `read_book_from` builds one part
of the book at a time from the tokeniser's events and drops it once translated, and `write_book_to` hands the file to
its sink (through gzip, if compressed) a chunk at a time. `read_back` reads a written file the same way and compares
each entity with the Book that was written, without building a second one.

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
| `book`     | `read_book`, `write_book`, `write_book_to`, `write_book_text`, and `XmlResidue`              |
| `read`     | The streaming reader: `read_book_from`, and `read_back`, which checks a written file entity by entity |
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
