# QIF files from GnuCash's repository

Copied unchanged from GnuCash 5.17 (`design/GNUCASH_REV`), `doc/examples/`. GnuCash is free software under the GNU
General Public License, version 2 or (at your option) any later version; these files are distributed under the same
terms. They are example exports kept for GnuCash's documentation and testing; the `Money95*_fr.qif` files are
ISO-8859-1, the rest ASCII.

| File | What it exercises |
|------|-------------------|
| `abc.qif`, `abc-all.qif` | A bank account; `abc-all` adds classes, categories and an account list |
| `bogus.qif` | Categories with `^^` record ends, accounts and transactions |
| `cbb-export.qif` | A bank export with check numbers and memos (CBB) |
| `divx.qif` | `!Option:AutoSwitch`, an opening balance to the account itself, splits, `Oth A` and investment transfers with miscx categories |
| `every.qif` | Every section of a Quicken export: classes, categories, accounts, bank and investment transactions |
| `ms-money.qif` | Microsoft Money's export: an "Opening Balance" transaction names the account |
| `Money95bank_fr.qif`, `Money95invst_fr.qif`, `Money95mfunds_fr.qif`, `Money95stocks_fr.qif` | French Microsoft Money 95 exports, ISO-8859-1 |
| `price.qif` | Securities, investment actions, `U` amounts and a `!Type:Prices` list with fractions and a bad line |
| `quicktest.qif` | A Quicken export with an empty record (a transaction without a date) and padded investment fields |
| `swipe.qif` | Investment transactions with values padded by spaces |
| `web.qif` | A bank's web export whose dates run into the text after them (`03/25/099Share Withdrawal`) |

`crates/red-ale-import/tests/qif_read.rs` reads every file.
