# SQLite fixtures

SQLite copies of the canonical XML fixtures, written by GnuCash 5.17 itself. Each `sqlite/<dir>/<name>` is the XML
book `fixtures/<dir>/<name>` saved as SQLite by `scripts/sql-fixtures/build.py`, which follows GnuCash's own Save As
(`5.17:gnucash/gnome-utils/gnc-file.c:1640-1729`: a new `sqlite3://` session, `swap_data`, save) and then opens the
result once read-only, as a user's first load would, so each book carries the ISO-dates Book feature GnuCash adds on
load (`5.17:libgnucash/backend/dbi/gnc-backend-dbi.cpp:879-880`). GnuCash never opens the fixtures themselves: the
script works on copies in a temporary directory.

`MANIFEST.json` records, for each book, its XML source and that source's SHA-256, and the oracle's GnuCash, libdbi
and SQLite versions.

```
TZ=UTC scripts/sql-fixtures/build.py                  # into fixtures/sqlite
TZ=UTC scripts/sql-fixtures/build.py /tmp/sqlite      # elsewhere
```

The script finds the bindings through `RED_ALE_ORACLE_PYTHONPATH`, else the 5.17 install under `$HOME`, and refuses
any GnuCash but 5.17. A rebuild gives the same rows except the GUIDs GnuCash makes afresh on every Save As: each
commodity's and each frame or list slot's `guid_val`. red-ale reads neither into the Book (commodities are known by
namespace and mnemonic, frames by their place in the tree), so a rebuilt book reads to the same Book.

`piecash/simple_sample.sqlite.gnucash` is not here: it is piecash's original SQLite book, written by GnuCash 3.0, and
lives in `fixtures/piecash/`.

## What the SQLite books lose

GnuCash's SQL backend cannot hold everything its XML holds, so each book reads to its XML original less these
(`crates/red-ale-sql/tests/fixtures.rs`, `design/sql-fidelity.md` §8):

- the deferred instances of scheduled transactions (no table);
- the Slots of addresses (no owner GUID for them);
- the digits of a double slot past the twelfth decimal (GnuCash writes `%.12f` text into a `REAL` column);
- commodities nothing uses: Save As writes only commodities that an account, transaction, price, invoice, employee
  or vendor refers to.

And the SQLite books hold one thing more: rows for currencies the XML writer leaves out.

## Licence

Each book keeps the licence of the XML book it was made from:

| Directory | Licence |
|-----------|---------|
| `gnucash/`, `reports/` | GPL-2.0-or-later, as GnuCash (see `fixtures/README.md`) |
| `piecash/` | MIT, see `piecash/LICENSE` |
| `synthetic/` | as red-ale |
