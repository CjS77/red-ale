# Test fixtures

Synthetic or third-party test books only; never personal financial data. Tests copy a fixture into a temporary
directory before opening it, so these files are never written.

| Directory | Contents | Licence |
|-----------|----------|---------|
| `gnucash/` | Books from GnuCash's own repository (see its README) | GPL-2.0-or-later, as GnuCash |
| `synthetic/` | Books generated with the GnuCash 5.17 oracle by `scripts/fixtures/` | as red-ale |
| `piecash/` | Books from [piecash](https://github.com/sdementen/piecash/tree/master/gnucash_books), converted from SQLite to XML by the oracle with `scripts/sqlite-to-xml.py`; `simple_sample.sqlite.gnucash` is the unconverted SQLite original | MIT, see `piecash/LICENSE` |

## Canonical books

A book is *canonical* when GnuCash 5.17 loads and saves it back byte for byte (`scripts/oracle-resave.py`). These
are canonical, and red-ale must round-trip them byte for byte:

- `gnucash/sample1.gnucash`, `gnucash/bug-746937-sx-split-order.gnucash`
- `synthetic/business.gnucash` (every business object type, including a credit note, so it declares the Credit
  Notes feature and red-ale v0.1 opens it read-only), `synthetic/business-editable.gnucash` (the same book made with
  `--no-credit-note`: the job's invoice is an ordinary invoice and the book is writable), `synthetic/rich.gnucash` (every core entity and slot
  type, special characters, a budget, a scheduled transaction and template transactions)
- every `piecash/*.gnucash` except the `.sqlite` one

The other `gnucash/` books were written by older GnuCash versions; GnuCash itself rewrites them on save, so they are
checked with the oracle rather than byte for byte.

## Regenerating

```
export PYTHONPATH=$HOME/.local/opt/gnucash-5.17/lib/python3.12/site-packages
python3 scripts/fixtures/business_make_book.py /tmp/biz.gnucash        # then zcat to fixtures/synthetic/business.gnucash
python3 scripts/fixtures/business_make_book.py /tmp/biz2.gnucash --no-credit-note   # business-editable.gnucash
python3 scripts/fixtures/rich_create.py /tmp/rich.xml && python3 scripts/fixtures/rich_augment.py /tmp/rich.xml > /tmp/aug.xml
scripts/oracle-resave.py /tmp/aug.xml fixtures/synthetic/rich.gnucash
scripts/sqlite-to-xml.py <piecash book> fixtures/piecash/<name>.gnucash
```
