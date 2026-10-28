# Test fixtures

Synthetic or third-party test books only; never personal financial data. Tests copy a fixture into a temporary
directory before opening it, so these files are never written.

| Directory | Contents | Licence |
|-----------|----------|---------|
| `gnucash/` | Books from GnuCash's own repository (see its README) | GPL-2.0-or-later, as GnuCash |
| `synthetic/` | Books generated with the GnuCash 5.17 oracle by `scripts/fixtures/`, and one report rendered by GnuCash 5.5 (see below) | as red-ale |
| `piecash/` | Books from [piecash](https://github.com/sdementen/piecash/tree/master/gnucash_books), converted from SQLite to XML by the oracle with `scripts/sqlite-to-xml.py`; `simple_sample.sqlite.gnucash` is the unconverted SQLite original | MIT, see `piecash/LICENSE` |
| `csv/` | Import files from GnuCash's CSV importer tests (see its README) | GPL-2.0-or-later, as GnuCash |
| `reports/` | The books of GnuCash 5.17's report tests, rebuilt in GnuCash's engine by `scripts/report-fixtures/`, and the tests' expected figures (see below) | GPL-2.0-or-later, as GnuCash |
| `qif/` | QIF files from GnuCash's `doc/examples/` (see its README) | GPL-2.0-or-later, as GnuCash |
| `sqlite/` | GnuCash 5.17's SQLite copies of the canonical XML books, made by `scripts/sql-fixtures/build.py` (see `sqlite/README.md`) | as the XML book each was made from |
| `gnucash-userdata/` | A `saved-reports-2.8` and a `stylesheets-2.0` written by hand exactly as GnuCash 5.17 writes them (`5.17:gnucash/report/report-core.scm:620-643`, `5.17:gnucash/report/html-style-sheet.scm:145-166`), since the oracle has no GUI to save them | as red-ale |

## Canonical books

A book is *canonical* when GnuCash 5.17 loads and saves it back byte for byte (`scripts/oracle-resave.py`). These
are canonical, and red-ale must round-trip them byte for byte:

- `gnucash/sample1.gnucash`, `gnucash/bug-746937-sx-split-order.gnucash`
- `synthetic/business.gnucash` (every business object type, including a credit note, so it declares the Credit
  Notes feature and red-ale v0.1 opens it read-only), `synthetic/business-editable.gnucash` (the same book made with
  `--no-credit-note`: the job's invoice is an ordinary invoice and the book is writable), `synthetic/rich.gnucash` (every core entity and slot
  type, special characters, a budget, a scheduled transaction and template transactions)
- every `piecash/*.gnucash` except the `.sqlite` one
- every `reports/*.gnucash`

The other `gnucash/` books were written by older GnuCash versions; GnuCash itself rewrites them on save, so they are
checked with the oracle rather than byte for byte.

## GnuCash 5.5 output

`synthetic/rich-balance-report.html` is the Balance Sheet of `synthetic/rich.gnucash` as GnuCash **5.5**'s GUI
rendered it, not 5.17's: the oracle has no GUI and runs no reports, and no 5.17 GUI build exists. It differs from
what 5.17 prints, for example `5 ACME` where 5.17 prints `5. ACME` (`design/reports.md` §3). No test compares with it
as 5.17's output; replace it with 5.17 output if a 5.17 GUI build becomes available.

## Report books

`reports/` holds the books GnuCash 5.17's report tests build in code, rebuilt by one script per test in
`scripts/report-fixtures/` with GnuCash's own engine (see `design/reports.md` §13). Each script replays the Scheme
calls of the test and of `5.17:bindings/guile/test/test-engine-extras.scm` (ported in `engine_extras.py`), saves the
book, then reopens it read-only in GnuCash and checks the account balances the test's figures imply. Books whose
dates depend on now are built for 2024-03-15 12:00:00 UTC. GUIDs are fixed (`b…0` the book, `a…N` the Nth account
made, `c…` transactions, `d…` splits, `e…` prices, `f…` budgets), so a rebuild gives the same bytes.

| Book | Script | Rebuilt from (GnuCash 5.17) | Licence of the source |
|------|--------|-----------------------------|-----------------------|
| `test-data` | `account_summary.py` | `create-test-data` (`bindings/guile/test/test-engine-extras.scm:389-480`) and `gnucash/report/reports/standard/test/test-account-summary.scm:26-33` | GPL-2.0-or-later (notice in test-engine-extras.scm; test-account-summary.scm has none and is cited under GnuCash's licence) |
| `balsheet-pnl` | `balsheet_pnl.py` | `standard/test/test-balsheet-pnl.scm:51-166`, with FUNDS added to the commodity table | GPL-2.0-or-later (no notice; GnuCash's licence) |
| `average-balance` | `average_balance.py` | `standard/test/test-average-balance.scm:35-81` | GPL-2.0-or-later (no notice; GnuCash's licence) |
| `charts-net`, `charts-daily` | `charts.py` | books B and A of `standard/test/test-charts.scm:85-114`, `:148-200` | GPL-2.0-or-later (no notice; GnuCash's licence) |
| `net-days` | `net_barchart.py` | `standard/test/test-standard-net-barchart.scm:58-311`; its first three groups are the book of `test-standard-net-linechart.scm` | GPL-2.0-or-later (notice kept) |
| `category-daily`, `-average`, `-multi`, `-liability` | `category_report.py` | `standard/test/test-standard-category-report.scm:81-318` | GPL-2.0-or-later (notice kept) |
| `budget` | `budget.py` | `standard/test/test-budget.scm:66-69`: `create-test-data` and `gnc:create-budget-and-transactions` (`bindings/guile/test/test-engine-extras.scm:851-884`), the budget made through ctypes on `libgnc-engine.so` (the bindings do not wrap it), GUID `f…0` | GPL-2.0-or-later (notice kept) |
| `adjusting-entries`, `-fx` | `adjusting_entries.py` | `create-test-data`, GBP's `#` and the three 2003 transfers of `standard/test/test-trial-balance.scm:54-76` (= `test-equity-statement.scm:54-75`); `-fx` adds the two foreign transfers of `test-trial-balance.scm:170-173` | GPL-2.0-or-later (notice kept) |
| `cash-flow` | `cash_flow.py` | `standard/test/test-cash-flow.scm:14-144`, its three groups dated 2024-03-15 | GPL-2.0-or-later (no notice; GnuCash's licence) |
| `cashflow-barchart` | `cash_flow.py` | `standard/test/test-cashflow-barchart.scm:51-195`, its three groups dated 2024-03-02 and -03 | GPL-2.0-or-later (notice kept) |
| `balsheet-pnl-extras`, `-trading` | `balsheet_pnl_extras.py` | no GnuCash test: a small book for hand-worked Multicolumn Balance Sheet and P&L checks (expense accounts, a closing transaction, placeholders, an account with no splits, priced EUR and unpriced CHF), the second with trading accounts made by GnuCash's own commit; its figures are in the script's docstring | as red-ale |
| `stock-test-data` | `stock_test_data.py` | `create-stock-test-data` and its `stock-split` helper (`bindings/guile/test/test-engine-extras.scm:893-1059`), as `standard/test/test-portfolios.scm:52`, `:84` builds it | GPL-2.0-or-later (notice in test-engine-extras.scm kept; test-portfolios.scm has none and is cited under GnuCash's licence) |
| `ifrs-cost-basis` | `ifrs_cost_basis.py` | `standard/test/test-ifrs-cost-basis.scm:54-243` | GPL-2.0-or-later (no notice; GnuCash's licence) |
| `investment-lots`, `lots-reverse-split`, `lots-baseline`, `lots-reverse-split-sale`, `lots-postsplit-baseline` | `investment_lots.py` | books A to E of `standard/test/test-investment-lots.scm:59-369`; B to E keep their AAPL splits in one lot, GUID `1…0`, and A's unbalanced transactions add an Imbalance-USD account, given fixed GUIDs after each commit | GPL-2.0-or-later (no notice; GnuCash's licence) |

Beside each book, `<book>.expected.json` lists every assertion of the GnuCash tests that run on it, for the report
translators to turn into named checks: `settings` (now, time zone, locale, date format, report currency, user
symbols), `accounts` (full name or role to GUID), and `runs`, each with the template name and GUID, the options in
effect (`"Section/Name": value`, the test's cumulative changes from the template's defaults), and `checks`. A check
names the test's assertion and its line, the cells GnuCash reads (`select`: table, row and column as
`sxml->table-row-col` counts them, or `count` for a number of cells), the exact `expected` strings and their
`meaning`. `renderOnly` lists assertions that only check a report renders. Runs marked `"inV02Inventory": false` use
templates outside v0.2's list (`design/reports.md` §12). `test-cash-flow.scm` never renders the report: it reads
`cash-flow-calc-money-in-out`'s result, so `cash-flow.expected.json`'s checks name the result item they `read`, and
each run adds the equivalent Cash Flow `report`, worked from the report's source rather than GnuCash output. Checks
with `"readAs": "number"` compare numbers the test parses from the cells, with the strings they imply under
`implied`. The `balsheet-pnl-extras` books have no `expected.json`: no GnuCash test runs on them.

```
scripts/report-fixtures/build-all.sh               # every book into fixtures/reports, with its checks
scripts/report-fixtures/build-all.sh /tmp/reports  # elsewhere
```

The driver sets `TZ=UTC` and `PYTHONPATH` (from `RED_ALE_ORACLE_PYTHONPATH`, or the 5.17 install under `$HOME`);
a single script runs the same way: `TZ=UTC PYTHONPATH=… scripts/report-fixtures/charts.py [out-dir]`.

## Regenerating

```
export PYTHONPATH=$HOME/.local/opt/gnucash-5.17/lib/python3.12/site-packages
python3 scripts/fixtures/business_make_book.py /tmp/biz.gnucash        # then zcat to fixtures/synthetic/business.gnucash
python3 scripts/fixtures/business_make_book.py /tmp/biz2.gnucash --no-credit-note   # business-editable.gnucash
python3 scripts/fixtures/rich_create.py /tmp/rich.xml && python3 scripts/fixtures/rich_augment.py /tmp/rich.xml > /tmp/aug.xml
scripts/oracle-resave.py /tmp/aug.xml fixtures/synthetic/rich.gnucash
scripts/sqlite-to-xml.py <piecash book> fixtures/piecash/<name>.gnucash
```
