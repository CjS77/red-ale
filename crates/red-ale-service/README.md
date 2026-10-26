# red-ale-service

Opens a book for editing and keeps it safe while it is open.

## Role

A `Session` is one open book file. This crate does everything that has to happen around the bookkeeping itself:

- **Opening.** It refuses SQL books (SQLite files and database URIs) before touching them. It takes GnuCash's
  `.LCK` lock, so red-ale and GnuCash never write the same book at once.
- **Read-only checks.** A book opens read-only, with the reason recorded, when editing it could damage it:
    - it declares a GnuCash feature red-ale doesn't implement yet, such as credit notes;
    - it uses trading accounts;
    - it sets an auto read-only threshold;
    - it holds content red-ale can keep but not edit safely.

  A read-only session never takes the lock and never writes.
- **All-or-nothing changes.** `Session::mutate` runs a change on a copy of the book. The copy replaces the book only if
  the change succeeds and every transaction it touched still balances. A failed change leaves nothing behind.
- **The journal.** Every change to a transaction is recorded in GnuCash's `.log` journal format, as GnuCash does, so
  unsaved transactions can be recovered by red-ale's `book.replayJournal` or by GnuCash's *Replay GnuCash .log file*.
- **Saving.** Before replacing the file, a save checks that the new bytes read back as the same book. It then keeps
  the previous file as a timestamped backup, writes the new one, starts a new journal and deletes old backups after the
  retention period. All of this matches what GnuCash does.

## Where it fits

This is the application layer: it runs the use cases that `red-ale-core` defines, against files that `red-ale-xml`
reads and writes.

```text
red-ale-rpc  ->  red-ale-service  ->  red-ale-core  (the operations)
                                  ->  red-ale-xml   (the file)
```

It holds no bookkeeping rules of its own. The JSON-RPC methods in `red-ale-rpc` are thin wrappers that open a
`Session` and call `mutate` with a core operation.

## What's inside

| Module    | Contents                                                                                  |
|-----------|-------------------------------------------------------------------------------------------|
| `session` | `Session`, `Settings`, `ReadOnlyReason`, and the read-only checks                         |
| `lock`    | The `.LCK` lock file                                                                      |
| `journal` | Writing the `.log` journal                                                                |
| `replay`  | Replaying a journal onto a book                                                           |
| `files`   | Backups, the safe replace step, and deleting old files                                    |
| `config`  | red-ale's own settings that GnuCash keeps outside the book, such as the accounting period |
| `reports` | The Configuration's `reports` keys, and the loader that reads user report scripts          |

## Usage

Open a book, make a change, save and close:

```rust
use red_ale_core::AccountType;
use red_ale_core::ops::accounts::{AccountEdit, create_account};
use red_ale_service::{Session, Settings};

let mut session = Session::open("home.gnucash", false, Settings::default ()) ?; // takes the lock
if session.is_read_only() {
println ! ("read-only: {:?}", session.read_only_reasons());
return Ok(());
}

// Runs on a copy of the book, which replaces it only if this succeeds.
let savings = session.mutate( | book, env| {
let edit = AccountEdit { name: Some("Savings".into()), account_type: Some(AccountType::Bank), ..AccountEdit::default() };
let parent = book.account_by_full_name("Assets").map_or(book.root, | a | a.guid);
create_account(book, env, parent, & edit)
}) ?;

session.save(None) ?;    // backs up the old file, writes the new one, starts a new journal
session.close(false) ?;  // releases the lock
```

`Session::create` makes a new, empty book. `Settings` sets how long backups are kept and where red-ale's own
configuration lives.
