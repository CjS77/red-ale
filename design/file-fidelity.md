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

1. **DOM.** `dom::parse_document` reads the whole file into a small DOM. Character references are resolved;
   formatting whitespace (whitespace-only text beside child elements) is dropped; text content is kept exactly. An
   element written `<a></a>` keeps an empty text node and `<a/>` has none, because libxml2 serialises them
   differently and GnuCash produces both.
2. **Serialiser.** `Element::dump` writes a DOM exactly as libxml2 2.9's `xmlElemDump` does with formatting on: two
   spaces per level, capped at 60; children indented only when none is text; markup characters escaped; non-ASCII and
   control characters (except tab and newline) as `&#xHH;` references with upper-case hex. The envelope (XML
   declaration, `gnc-v2` namespaces, `gnc:book` wrapper, `gnc:count-data`, the price database wrapper) is written as
   GnuCash's `fprintf` calls write it. Passing both GnuCash test books through parse and dump gives the same bytes.
3. **Mapping.** Each known element becomes a core entity. What the mapping does not consume is Residue.

## Residue

The Backend returns, beside the `Book`, an `XmlResidue` the Session carries untouched and hands back on save. It holds:

- **Layout:** every part of `gnc:book` in the order it was read: book id and slots, each commodity, the price
  database, each account, each transaction, out-of-scope sections, each business object. Unknown parts are kept as
  DOM in their place.
- **Originals:** the DOM element each entity was read from, and the entity as read (an `Arc` clone of the Book read,
  so this costs one pointer per entity).
- **Unknown children:** child elements of a known entity that the mapping did not consume, with the name of the known
  sibling they followed.
- **Out-of-scope sections**, kept as DOM: `gnc:template-transactions`, `gnc:schedxaction`, `gnc:budget`, and any
  business object type v0.1 does not model.
- **Compression:** whether the file was gzipped.

## Writing

The writer walks the layout:

- An entity that still exists and equals its original (`Arc::ptr_eq`, else `==`) is written from its **original DOM**.
  An unedited book is therefore written from its own elements and cannot lose anything.
- An entity that changed is **generated** from the model, as GnuCash 5.17's generator for that type would write it;
  unknown children are re-inserted after the sibling they followed.
- A deleted entity is skipped. New entities are generated and appended at the end of their section; an empty section
  gets them at GnuCash's canonical position (commodities, price database, accounts, transactions, template
  transactions, scheduled transactions, budgets, then business objects in plugin registry order).
- Commodities are kept sorted by namespace then mnemonic, GnuCash's own write order
  (`5.17:libgnucash/backend/xml/io-gncxml-v2.cpp:1037`).
- `gnc:count-data` lines are recomputed with GnuCash's rules, from the Book and the out-of-scope sections.

An account's lots are written inside the account (`act:lots`), so an account counts as changed when it or any of its
lots changed.

### Proving the generators

Writing unchanged entities from their originals hides generator bugs, so a second test path forces **every** entity
to be generated from the model and requires the same bytes for every GnuCash 5.17 fixture. Both paths must pass.

## References to out-of-scope data

Budgets and scheduled transactions refer to accounts by GUID. Deleting an account those sections mention would leave
GnuCash with dangling references, so the Session refuses to delete any account or commodity whose GUID or mnemonic
appears in out-of-scope Residue, and says which section holds it.

## Compression

A book is written back the way it was read: gzipped if it was gzipped. `book.save` can override this. GnuCash itself
compresses by default (see [`notes/storage-and-safety.md`](notes/storage-and-safety.md)); red-ale keeps the user's
choice instead, which GnuCash reads either way.

## What is not kept

- Formatting of books not written by GnuCash 5.17: the output uses 5.17's layout, as GnuCash's own save would.
- XML comments and processing instructions, which GnuCash never writes and ignores on read.
- `gnc:count-data` values from the original, which GnuCash only uses for progress reporting.
