# Open questions and decisions

Decisions taken for v0.1 that a later version may revisit, and questions still open. Each says why.

## Decided for v0.1

**Denominators are kept as read.** GnuCash converts each split's value to the currency fraction and its amount to the
account's SCU when it loads a book, so a file holding other denominators changes on its next save in GnuCash
(`notes/engine-accounts-transactions.md` §1.5). red-ale keeps what the file holds, so an unedited entity round-trips
byte for byte; entities red-ale creates or edits are converted as GnuCash would convert them (`ops::commit`). Files
written by GnuCash already match, so normal books are unaffected.

**Unbalanced input is refused.** GnuCash balances a transaction at commit by adding an `Imbalance-<CUR>` split.
red-ale refuses an unbalanced transaction with `unbalanced` instead; the imbalance scrub does GnuCash's repair when
asked.

**English names where GnuCash translates.** Accounts GnuCash creates (`Imbalance-USD`, `Orphan-USD`, gains
accounts), lot titles and posted-document memos use GnuCash's English strings. A book used with a translated GnuCash
gets English names for accounts red-ale creates.

**Unknown quote sources are kept.** A commodity's quote source that GnuCash 5.17 does not know is kept as read.

**The accounting period is red-ale configuration.** In GnuCash it is a user preference, not book data, so red-ale
keeps it in its own configuration rather than adding slots to the user's book.

**One session per server.** A `red-ale serve` process opens at most one book. Several books would need a session
handle in every method; nothing in v0.1 needs it.

**Unrecognised content opens read-only.** Elements and slot types red-ale does not model are kept and written back,
but a book holding content red-ale does not recognise opens read-only, because an edit could leave it inconsistent
with data red-ale cannot see.

**Credit notes, num-field source, trading accounts and the auto read-only threshold open read-only.** They change
what other operations must do (signs, which field is the number, extra balancing splits, which transactions may be
edited), so v0.1 opens such books read-only rather than edit them partly right.

**Version 1 XML files are refused** with a message to open and save them in GnuCash first. GnuCash 5.17 still reads
them, but no current GnuCash writes them.

**Replay is stricter than GnuCash.** A journal record with an unreadable line is skipped whole, and an unbalanced
replayed transaction is refused unless asked to skip it. Replayed changes are journaled (GnuCash disables the journal
during replay), so they are not lost if the session ends before a save.

**Shutdown without saving.** `red-ale serve` closes the book on Ctrl-C or SIGTERM without saving, as no client asked
it to save; the journal holds the unsaved changes and `book.replayJournal` can restore them.

## Open

- **Authentication and remote access.** The server listens on localhost with no authentication. Remote access needs
  a design (Unix socket permissions, tokens, TLS) before it is offered.
- **Several books per server.** If clients need it, methods would take a session handle (additive: a new optional
  param).
- **Concurrent clients.** Methods run one at a time. A client cannot hold a multi-method transaction; each method is
  atomic on its own. A batch or script is not atomic as a whole.
- **Locale-dependent collation.** GnuCash orders some ties (transactions with equal date, number and date entered)
  with `g_utf8_collate`, which depends on the locale of the GnuCash that wrote the file. red-ale appends new entities
  and keeps the order it read, which GnuCash accepts on load.
