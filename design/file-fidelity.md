# File fidelity

How `red-ale-xml` keeps an XML book intact. The rules come from CLAUDE.md ("File compatibility: rules that are never
broken"); this document says how the Backend meets them. GnuCash's format is described in
[`notes/xml-format-core.md`](notes/xml-format-core.md) and [`notes/xml-format-business.md`](notes/xml-format-business.md).

## The target: byte-identical round-trips

GnuCash's own test suite loads two books and saves them again, and requires the output to equal the input byte for
byte (`5.17:libgnucash/backend/xml/test/gtest-load-save-files.cpp:80`). A book written by GnuCash 5.17 is therefore
in a canonical form, and red-ale holds itself to the same test: **open a 5.17 book, save it with no edits, and the
decompressed bytes must not change.**

Books written by older versions are not canonical; GnuCash itself rewrites them on save. For those, the test is the
oracle's: GnuCash 5.17 must read red-ale's output and find the same data it finds in the original.

## Three layers

1. **DOM.** The file is read as a stream (gzip undone, UTF-8 checked and line ends normalised as it goes), and each
   part of the book, one price of the price database or one other child of `gnc:book`, is built as a small DOM,
   translated, and dropped, as GnuCash's SAX parser handles one part at a time (`5.17:libgnucash/backend/xml/sixtp.cpp`).
   No DOM of the whole file and no copy of its text is held. Character references are resolved; formatting whitespace
   (whitespace-only text beside child elements) is dropped; text content is kept exactly. An element written
   `<a></a>` keeps an empty text node and `<a/>` has none, because libxml2 serialises them differently and GnuCash
   produces both.
2. **Serialiser.** `Element::dump` writes a DOM exactly as libxml2 2.9's `xmlElemDump` does with formatting on: two
   spaces per level, capped at 60; children indented only when none is text; markup characters escaped; non-ASCII and
   control characters (except tab and newline) as `&#xHH;` references with upper-case hex. The envelope (XML
   declaration, `gnc-v2` namespaces, `gnc:book` wrapper, `gnc:count-data`, the price database wrapper) is written as
   GnuCash's `fprintf` calls write it. Passing both GnuCash test books through parse and dump gives the same bytes.
3. **Mapping.** Each known element becomes a core entity. What the mapping does not consume is Residue.

## Residue

The Backend returns, beside the `Book`, an `XmlResidue` the Session carries untouched and hands back on save. It holds:

- **Layout:** every part of `gnc:book` in the order it was read: book id and slots, each commodity, the price
  database, each account, each transaction, the template transactions, each scheduled transaction, each budget, each
  business object. Unknown parts are kept as DOM in their place.
- **The Book as read:** an `Arc` clone of the Book read (or, after a save, of the Book saved), so this costs one
  pointer per entity.
- **Originals:** the text of each entity whose generated form differs from the element it was read from, and nothing
  for the rest. Each entity is generated as it is read and compared with its element; accounts are compared once every
  lot is read, since an account is written with its lots. A book GnuCash 5.17 wrote keeps no originals at all.
- **Unknown children:** child elements of a known entity that the mapping did not consume, with the name of the known
  sibling they followed.
- Nothing out of scope since v0.4: budgets, scheduled transactions and the template accounts and template
  transactions of `gnc:template-transactions` are in the core model, read and generated like the other entities. The
  template block is written only when the template root has descendants, as GnuCash writes it
  (`5.17:libgnucash/backend/xml/io-gncxml-v2.cpp:1179-1199`). A legacy `sx:freqspec` opens the book read-only and keeps
  its scheduled transaction's original text ([`notes/xml-format-core.md`](notes/xml-format-core.md) §8.5).
- **Compression:** whether the file was gzipped.

## Writing

The writer walks the layout:

- An entity that still exists, equals the entity as read (`Arc::ptr_eq`, else `==`) and has an original is written
  from its **original text**. Every other entity, the unedited ones of a book GnuCash 5.17 wrote included, is
  **generated** from the model, as GnuCash 5.17's generator for that type would write it, which for an unedited one
  gives the bytes it was read as. An unedited book therefore cannot lose anything.
- Unknown children of an entity make its generated form differ, so its original text keeps them. They are also
  reported as unrecognised content, which opens the book read-only (`read_only_triggers`), so such an entity is never
  regenerated.
- A deleted entity is skipped. New entities are generated and appended at the end of their section; an empty section
  gets them at GnuCash's canonical position (commodities, price database, accounts, transactions, template
  transactions, scheduled transactions, budgets, then business objects in plugin registry order).
- Commodities are kept sorted by namespace then mnemonic, GnuCash's own write order
  (`5.17:libgnucash/backend/xml/io-gncxml-v2.cpp:1037`).
- `gnc:count-data` lines are recomputed with GnuCash's rules, from the Book.

An account's lots are written inside the account (`act:lots`), so an account counts as changed when it or any of its
lots changed.

**A transaction and a template transaction never share a GUID in a file red-ale writes** (v0.6). GnuCash keeps both
in the Book's one transaction collection (`5.17:libgnucash/engine/Transaction.cpp:473-478`), where the second read of
a GUID replaces the first (`5.17:libgnucash/engine/qofid.cpp:95-106`), and an SQL book keeps both in one table
(`5.17:libgnucash/backend/sql/gnc-transaction-sql.cpp:68`). red-ale's model keeps them in two maps, so a file with
such a pair opens and both are read, but `book.save` refuses the Book before anything is written, no backup
included: `invalid` with `data` `{ "reason": "shared-guid", "guid" }`. The session goes on, and a Mutation that
removes one of the two lets it save. A Mutation that would create such a pair is refused the same way, on XML and SQL
alike; GUIDs come from the session's Env, so only a test or a file another writer made can bring one. Only red-ale's
testkit has made one, before v0.5 (`milestone5.md`, "Measured: Since Last Run on SQLite").
`Book::shared_transaction_guids` costs a lookup per template transaction. Tests: `crates/red-ale-service/tests/shared_guid.rs`,
`an_xml_book_read_with_a_shared_guid_opens_but_does_not_save_until_it_is_removed`,
`a_mutation_giving_a_transaction_a_template_transaction_s_guid_is_refused_on_xml`; `crates/red-ale-rpc/tests/dispatch.rs`,
`a_save_of_a_shared_transaction_guid_is_invalid`. Template accounts and Book accounts, and template splits and Book
splits, share collections and SQL tables the same way and are not checked yet ([`milestone6.md`](milestone6.md),
"Follow-ups").

### Proving the generators

Writing unchanged entities from their originals hides generator bugs, so a second test path forces **every** entity
to be generated from the model and requires the same bytes for every GnuCash 5.17 fixture. Both paths must pass.

## References from schedules and budgets

Scheduled transactions refer to accounts by GUID, through the `sched-xaction/account` slot of their template splits,
and budgets through their Slots' keys. GnuCash deletes such an account and leaves the references dangling; the core
refuses to delete it (or a sub-account it would delete with it) and names the scheduled transactions and budgets. A
commodity renamed is renamed in template accounts and template transactions too; one they use is not deleted.

## Compression

A book is written back the way it was read: gzipped if it was gzipped. `book.save` can override this. GnuCash itself
compresses by default (see [`notes/storage-and-safety.md`](notes/storage-and-safety.md)); red-ale keeps the user's
choice instead, which GnuCash reads either way.

## What is not kept

- Formatting of books not written by GnuCash 5.17: the output uses 5.17's layout, as GnuCash's own save would.
- XML comments and processing instructions, which GnuCash never writes and ignores on read.
- `gnc:count-data` values from the original, which GnuCash only uses for progress reporting.
