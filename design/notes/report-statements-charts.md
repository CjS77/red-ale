# Report note: statements, analysis charts and the Multicolumn report (GnuCash 5.17)

Scope: the v0.5 reports of [`../milestone5.md`](../milestone5.md) that do **not** run on the Transaction Report
engine: Trial Balance, Cash Flow, Equity Statement, Multicolumn Balance Sheet and P&L, Transaction Breakdown,
Income/expense vs day of week, Cash flow chart, and Multicolumn reports. For each: its templates and options, its
computation, the library it needs against what red-ale has, what the report result cannot yet express, GnuCash's tests
and the fixtures to port them, and the deviations to record. The Transaction Report engine and the four reports on it
are another note's subject.

Citations are `5.17:<path>:<line>`; a bare `:<line>` refers to the file last named. `standard/` stands for
`5.17:gnucash/report/reports/standard/`, `example/` for `5.17:gnucash/report/reports/example/`, `report/` for
`5.17:gnucash/report/`. Abbreviations as [`../reports.md`](../reports.md) §12: RU `report/report-utilities.scm`, CU
`report/commodity-utilities.scm`, OU `report/options-utilities.scm`, HAT `report/html-acct-table.scm`, HU
`report/html-utilities.scm`, DU `5.17:bindings/guile/date-utilities.scm`. Test helpers are in
`5.17:bindings/guile/test/test-engine-extras.scm` (TEE) and `report/test/test-report-extras.scm`.

**Method.** Everything here was read from the 5.17 source. No Scheme was run: GnuCash's report tests are read as
text, and figures quoted as "worked" were computed by hand from the cited lines. Where a test's expected figure was
worked through, the note says so; it confirms the reading of the Scheme, not GnuCash's output.

---

## Facts that surprise

1. **The Cash Flow's "Include Trading Accounts in report" works backwards.** Off (the default) counts trading splits;
   on leaves them out (`5.17:gnucash/report/reports/cash-flow-calc.scm:68-70`), since commit `2215dfc5e1` (§3.1).
2. **The Trial Balance's work-sheet Net Income lands in the Adjustments column** in GnuCash's HTML, and GnuCash's own
   test asserts it there (`standard/test/test-trial-balance.scm:136-138`); the Scheme asks for the Income Statement
   column (§2.3).
3. **The Trial Balance leaves TRADING accounts out** of every column though they are in its default selection, and
   fails outright when only TRADING accounts are selected (§2.3).
4. **The Equity Statement counts a closing entry as a withdrawal** when it closes a loss: every debit to an equity
   account is one (§4.2). Its "Show Foreign Currencies" option is read by nothing.
5. **The Multicolumn P&L leaves closing transactions out; the Multicolumn Balance Sheet keeps them** (§5.3). Under
   `pricedb-latest` with a common currency, every P&L column header ends today.
6. **"Income vs. Day of Week" shows income as negative** amounts, labels included (`Monday - -$109.00`) (§7.2).
7. **A Multicolumn report's children are copies, not references,** in GnuCash: adding a saved configuration copies its
   options; and only one level of nesting survives a save (§8.2).
8. **GnuCash's Trial Balance and Equity Statement tests build the same book** (`create-test-data` plus three 2003
   transfers), so one fixture serves both (§2.5, §4.4).
9. **The Multicolumn Balance Sheet and P&L need no new fixture** for GnuCash's assertions: v0.2's
   `balsheet-pnl.expected.json` already holds all of them (§5.5).
10. **The Transaction Breakdown's default run is a warning:** its Account option has no default (§6.1).

## 1. Inventory

| Report (milestone row) | File | Lines | Template | GUID | Menu | GnuCash's test |
|------------------------|------|------:|----------|------|------|----------------|
| Trial Balance | `standard/trial-balance.scm` | 876 | Trial Balance | `216cd0cf6931453ebcce85415aba7082` | Income & Expense | `test-trial-balance.scm` |
| Cash Flow | `standard/cash-flow.scm` (+ `reports/cash-flow-calc.scm`, 107) | 346 | Cash Flow | `f8748b813fab4220ba26e743aedf38da` | Income & Expense | `test-cash-flow.scm` (the calculation only) |
| Cash flow chart | `standard/cashflow-barchart.scm` | 350 | Cash Flow Barchart | `5426e4d987f6444387fe70880e5b28a0` | Income & Expense | `test-cashflow-barchart.scm` |
| Equity Statement | `standard/equity-statement.scm` | 510 | Equity Statement | `c2a996c8970f43448654ca84f17dda24` | Income & Expense | `test-equity-statement.scm` |
| Multicolumn Balance Sheet and P&L | `standard/balsheet-pnl.scm` | 1,261 | Balance Sheet (Multicolumn) | `065d5d5a77ba11e8b31e83ada73c5eea` | Assets & Liabilities | `test-balsheet-pnl.scm` |
| | | | Income Statement (Multicolumn) | `0e94fd0277ba11e8825d43e27232c9d4` | Income & Expense | |
| Transaction Breakdown | `standard/txn-columns.scm` | 260 | Transaction Breakdown Report | `03603f0443494a6fa790b417f35d90d4` | Experimental | none |
| Income/expense vs day of week | `example/daily-reports.scm` | 310 | Income vs. Day of Week | `5e2d129f28d14df881c3e47e3053f604` | Examples | render only (`test-charts.scm`) |
| | | | Expenses vs. Day of Week | `dde49fed4ca940959ae7d01b72742530` | Examples | |
| Multicolumn reports | `standard/view-column.scm` | 189 | Multicolumn View (menu: Custom Multicolumn Report) | `d8ba4a2e89e8479ca9f6eccdeb164588` | Multicolumn | none |

GUIDs and line counts match `milestone5.md`'s table. Every file is GPL-2.0-or-later (headers checked). Menu names are
`report/report-core.scm:157-163` without the mnemonic underscore. `test-stress-options.scm` renders every one of these
templates under its boolean and multichoice options, on an empty and a populated book (`:62-94`, `:237-264`).

---

## 2. Trial Balance (`standard/trial-balance.scm`)

### 2.1 Inventory

"Trial Balance", `'version 1`, GUID `216cd0cf6931453ebcce85415aba7082`, menu `_Income & Expense`, no menu name or tip
(`:869-872`; `report/report-core.scm:158`). 876 lines, GPL-2.0-or-later, by David Montenegro, borrowed from
balance-sheet.scm and pnl.scm, with no copyright line (`:3`, `:26-47`). Default section Display (`:275`).

Dead code left out of the translation: `period-for` (`:365`), `ws-col` (`:565`), the commented-out "Include accounts
with zero total balances" option (`:264-267`; `show-zb-accts?` fixed at `#t`, `:321`) and its help (`:115-117`), and
the progress calls.

### 2.2 Options (`:170-277`)

| Section | Name | Key | Type | Default | Notes | Line |
|---------|------|-----|------|---------|-------|------|
| General | Report Title | a | string | "Trial Balance" | | `:174-176` |
| General | Start of Adjusting/Closing | ca | date, start choices (`add-date-interval`) | start-accounting-period | | `:180-182` |
| General | Date of Report | cb | date, end choices | end-accounting-period | | `:180-182` |
| General | Report variation | d | multichoice | `current` | `current` "General journal exact balances", `pre-adj` "No adjusting/closing entries", `work-sheet` "Full end-of-period work sheet" | `:184-190` |
| Accounts | Accounts | a | accountList | BANK CASH CREDIT ASSET LIABILITY STOCK MUTUAL CURRENCY PAYABLE RECEIVABLE EQUITY INCOME EXPENSE TRADING, display order | | `:193-204` |
| Accounts | Levels of Subaccounts | b | multichoice all, 1–6 (`add-account-levels`, OU:79-94) | **1** | | `:205-207` |
| Merchandising | Gross adjustment accounts. | c | accountList, any type | none | the name ends with a period | `:210-216` |
| Merchandising | Income summary accounts | d | accountList, any type | none | | `:218-222` |
| Commodities | Report's currency | a | currency | report currency setting | | `:225-227` |
| Commodities | Price Source | b | multichoice | **`average-cost`** | the five sources | `:229-231` |
| Commodities | Show Foreign Currencies | c | boolean | #f | | `:233-235` |
| Commodities | Show Exchange Rates | d | boolean | #f | | `:237-239` |
| Entries | Adjusting Entries pattern | a | string | "Adjusting Entries" | | `:245-247` |
| Entries | Adjusting Entries pattern is case-sensitive | b | boolean | #f | | `:248-250` |
| Entries | Adjusting Entries Pattern is regular expression | c | boolean | #f | capital P as written | `:251-253` |
| Entries | Closing Entries pattern | d | string | "Closing Entries" | | `:254-256` |
| Entries | Closing Entries pattern is case-sensitive | e | boolean | #f | | `:257-259` |
| Entries | Closing Entries Pattern is regular expression | f | boolean | #f | | `:260-262` |
| Display | Display accounts as hyperlinks | e | boolean | #t | | `:270-272` |

No callbacks. An empty "Accounts" value reads as its default (`5.17:libgnucash/engine/gnc-option-impl.cpp:410-413`),
so the no-account warning comes only when the default itself is empty.

### 2.3 Computation

**Dates** (`:296-303`): the adjusting/closing window starts at 23:59:59 of the day before "Start of
Adjusting/Closing" (`gnc:time64-previous-day`, DU:495-496) and ends at 23:59:59 of "Date of Report". `exchange-fn`
and `price-fn` are `gnc:case-exchange-fn` and `gnc:case-price-fn` of the price source at the end (`:362-364`).

**Title** (`:367-378`): `current`: `"${company-name} ${report-title} ${date}"` with the end date; `pre-adj` and
`work-sheet`: `"… For Period Covering ${start} to ${end}"`. The company name or "" (`:295`), so an unnamed book's
title begins with a space. No headline.

**Warning:** no accounts gives the no-account warning only (`:380-389`), the title still set. The only warning.

**Accounts** (`:325-358`): `gnc:decompose-accountlist` (RU:188-205); `all-accounts` = asset kinds, liability kinds,
EQUITY, then INCOME reversed onto EXPENSE. **TRADING accounts are dropped**: the decomposition's TRADING bucket is never
used. The Merchandising lists are decomposed the same way and matter only for accounts that are also rows.

**Account table** (`:502-517`): `gnc:make-html-acct-table/env/accts` over `all-accounts`, with start `#f`, end the end
date, `display-tree-depth` the level (or `#f` for `all`), `depth-limit-behavior 'flatten`,
`parent-account-subtotal-mode #f`, `zero-balance-mode 'show-leaf-acct`, `account-label-mode` anchor or name. So every
selected account has one row, zero balances included, no subtotal rows, display order depth first (HAT:609, :660-801);
`account-bal` is the account's own balance, splits posted up to the end date inclusive (HAT:628-651). `account-cols`
comes from row 0 (`:520-521`). With no rows (only TRADING accounts selected) `account-cols` is `#f` and `:523` fails.

**Per row** (`:582-660`). Two split queries over `all-accounts` from start to end (RU:692-725; non-void, inclusive):
closing splits (description contains the Closing pattern per its case and regexp options, **or** a closing
transaction), and adjusting splits (description contains the Adjusting pattern; the flag plays no part). An empty
pattern matches every description; a transaction matching both is subtracted twice. `sum-account-splits`
(`:598-608`) adds the amounts of the splits whose account is the row's own (with `pos?`, only positive ones).

| Value | Definition |
|-------|-----------|
| `closing`, `adjusting` | the row's closing and adjusting sums |
| `is?` | the account is an income summary account |
| `ga-or-is?` | a gross adjustment account, or `is?` |
| `pos-adjusting`, `neg-adjusting` | when `ga-or-is?`: the positive adjusting amounts, and `adjusting − pos-adjusting` |
| `pre-closing-bal` | `curr-bal − closing` |
| `pre-adjusting-bal` | `pre-closing-bal − adjusting` |
| `atb` | not `is?`: `pre-closing-bal`; `is?` with `pre-adjusting-bal` a credit: the pair (`pos-adjusting`, `neg-adjusting + pre-adjusting-bal`); else (`pos-adjusting + pre-adjusting-bal`, `neg-adjusting`) (`:625-633`) |

Cells per row (`:645-658`): 0 pre-adjustments = `pre-adjusting-bal`; 1 adjustments = the pair when `ga-or-is?`, else
`adjusting`; 2 ATB = `atb`; 3 IS = `atb` for INCOME/EXPENSE or `is?` accounts, else 4 BS = `atb`; 5 = `curr-bal`.

**Sign: `double-col`** (`:138-167`). The amount is the collector converted (`gnc:sum-collector-commodity`, banker's
rounding to the SCU). Negative is a **credit**, else a debit (zero counts as a debit); the figure shown is always the
absolute value. The entry is blank when the collector holds only the report currency (or nothing) and converts to 0;
with a foreign commodity it is a commodity table under "Show Foreign Currencies", else the converted sum, so a foreign
balance that sums or converts to 0 prints `$0.00`, not blank. Collectors keep zero entries (RU:346-351), so a foreign
account with splits summing to 0 shows `$0.00` and one with no splits shows blank.

**Columns by variant:**

| Variant | Columns | From |
|---------|---------|------|
| `current` | Account Name, Debit, Credit | `curr-bal`, closing and adjusting entries included |
| `pre-adj` | Account Name, Debit, Credit | `pre-adjusting-bal` |
| `work-sheet` | Account Name, then Debit/Credit for Trial Balance (`pre-adjusting-bal`), Adjustments, Adjusted Trial Balance, Income Statement, Balance Sheet | cells 0–4 |

Totals (`add-line`, `:413-427`): a credit row is `minusmerge`d into the credit total, anything else `merge`d into the
debit total. The label is indented by the row's `indented-depth`; the global one is reused by the unrealized row
(`:677`), so that label takes the last account row's indentation.

**Unrealized gains** (`:473-500`, `:685-714`): with `average-cost` (the default) never a row. Otherwise the book
balance (`gnc:account-get-comm-balance-at-date … #f` over `all-accounts`: strictly **before** the end, where the
table's balances include it) converted by the price source minus the same converted at `average-cost`; the row exists
when the negated gain collector is not all zero. "Unrealized Gains" in Credit when it converts negative, else
"Unrealized Losses" in Debit; it counts in the totals and, in the work sheet, in ATB and BS on its side, not in
Adjustments or IS.

**Work sheet cells** (`:723-778`): an unset cell (IS of a BS account or the reverse) is blank and adds nothing; a net
collector goes to Debit or Credit by `double-col`; a gross pair shows each side's absolute entry, blank when zero, the
first merged into the debit total and the second `minusmerge`d into the credit total.

**Totals row** (`:781-798`): role "primary-subheading", **no label text**; each total the converted absolute
(`tot-abs-amt-cell`, `:446-459`), converted once per commodity, so it can differ from the sum of rounded rows.

**Work sheet only** (`:799-850`): `net-is` = IS debits − IS credits, `net-bs` = BS debits − BS credits. Negative
`net-is` is "Net Income" with |`net-is`| meant for IS Debit; otherwise "Net Loss", meant for IS Credit; zero gives "Net
Loss" with a blank figure. `net-bs` negative goes to BS Debit, else BS Credit; a zero BS net prints `$0.00`. Then an
unlabelled grand-totals row with IS and BS cells only, each side showing the larger column total.

**Two placement quirks of GnuCash's HTML.** Both come from `gnc:html-table-add-labeled-amount-line!` padding with
`amt-depth − floor(tbl-width / 2)` (HAT:899-901), which assumes the amount sits right of the middle:

- The work sheet's Net Income/Loss figure (`:821-826`, which asks for the IS column) lands in **Adjustments Debit**
  (Net Income) or Adjustments Credit (Net Loss). GnuCash's test asserts it: `$760.00` in column 4
  (`standard/test/test-trial-balance.scm:136-138`), while the IS debit column (`:152-154`) does not hold it. The BS net
  figure, placed with `gnc:html-table-set-cell!` (`:827-830`), is in its column.
- With `account-cols` ≥ 2 (Levels 2 or more, or `all` on a deep tree), every Trial Balance amount moves left of its
  Debit/Credit header.

red-ale puts each figure in the column the Scheme names (§12).

**Header rows** (`:523-573`): a spacer row of 60-pixel cells (layout); in the work sheet a row with "Trial Balance",
"Adjustments", "Adjusted Trial Balance", "Income Statement", "Balance Sheet" each over its Debit column; then "Account
Name", Debit, Credit (and four more pairs). They are body rows, not column headers.

**After the table**: with "Show Exchange Rates", the rates table over the *option's* accounts (`:857-861`), so TRADING
accounts' commodities are listed though their rows are not.

### 2.4 Library and result fit

Present: the OU helpers, `filter_accountlist_type`/`book.accounts_of_types`, `decompose_accountlist`,
`account_is_inc_exp`, the collector operations, `sum_collector_commodity`, `uniform_commodity`, `case_exchange_fn`
(every source), `case_price_fn`, `account_get_trans_type_splits_interval` (empty pattern matches all,
`crates/red-ale-core/src/ops/transactions/find.rs:419-423`), `account_get_comm_balance_at_date`, the account table
with flatten, `show-leaf-acct` and anchors, `gnc_commodity_table`, `make_no_account_warning`, `make_rates_table`, day
bounds, `ctx.print_date`, `book.company_name()`, unary minus on Monetary.

Gaps: `time64_previous_day` (and `time64_next_day`, DU:495-499) in `lib/dates`, shared with the Equity Statement;
`gnc:html-acct-table-get-cell`/`-set-cell!` (HAT:830-841), used only here, become a per-row array in the script;
full-width Debit/Credit rows are built in the script, since `add_labeled_amount_line` makes staircase rows.

Result: fits today. A `label` column and `amount` columns without `levels`; role `account` rows; the work sheet's
group headings as a first `heading` row of `span: 2` text cells; the unrealized row `data`; the totals row `total`
with a `null` label; Net Income/Loss `total`; grand totals `grandTotal`; foreign amounts as `commodities` cells of
absolute values; the rates table a second block. The spacer row is dropped.

### 2.5 GnuCash's test (`standard/test/test-trial-balance.scm`, 179 lines, GPL-2.0-or-later notice)

Book: `create-test-data` (TEE:389-480), GBP user symbol `#` (`:65-67`), then the same three 2003 transfers as the
Equity Statement's test (`:69-76`): 2003-06-30 Expenses → Equity 111 "Closing Entries", flagged closing; 2003-06-01
Expenses → Equity 33 "Adjusting Entries"; 2003-07-01 Income → Equity −2500 "Adjusting Entries". Before the last run
only, two foreign transfers (`:170-173`): 2000-01-15 GBP Bank → Bank 10 GBP / 14 USD "GBP 10 to USD 14"; 2000-02-15
Bank → GBP Bank 9 USD / 8 GBP "USD 9 to GBP 8". The second price, GBP in USD 9/8, replaces `create-test-data`'s 3/2 of
the same day (a price of the same source on the same day replaces the old one,
`5.17:libgnucash/engine/gnc-pricedb.cpp:1056-1071`). C locale (`:35`). Price Source `pricedb-nearest` for every run
(`:63`).

| Run | Options | Lines |
|-----|---------|-------|
| current | defaults + pricedb-nearest | `:78-94` |
| pre-adj | + Start of Adjusting/Closing 2000-01-01, variation `pre-adj` | `:96-115` |
| work-sheet | + variation `work-sheet` | `:117-166` |
| unrealized | variation `current`, after the two 2000 transfers | `:175-179` |

| Line | Check (table 1) | Expected |
|------|-----------------|----------|
| `:79-81` | all texts | 22 |
| `:83-86` | column 1 | "Root" "Asset" "Bank" "GBP Bank" "Wallet" "Liabilities" "Income" "Income-GBP" "Expenses" "Equity" "Unrealized Losses" |
| `:88-90` | column 2 | "$2,186.00" "$912.00" "$20.00" "$120.00" "$1.00" "$3,239.00" |
| `:92-94` | column 3 | "$3.00" "$73.00" "$918.00" "$2,245.00" "$3,239.00" |
| `:100-102` | pre-adj texts | 22 |
| `:104-107` | pre-adj column 1 | as `:84-85` |
| `:109-111` | pre-adj column 2 | "$2,186.00" "$912.00" "$20.00" "$264.00" "$111.00" "$1.00" "$3,494.00" |
| `:113-115` | pre-adj column 3 | "$3.00" "$2,573.00" "$918.00" "$3,494.00" |
| `:119-121` | work-sheet texts | 58 |
| `:123-126` | work-sheet column 1 | as above + "Net Income" |
| `:128-130` | TB debits (col 2) | as `:110` |
| `:132-134` | TB credits (col 3) | as `:114` |
| `:136-138` | adj debits (col 4) | "$2,500.00" "$2,500.00" "$760.00" (the last is Net Income, misplaced) |
| `:140-142` | adj credits (col 5) | "$33.00" "$2,467.00" "$2,500.00" |
| `:144-146` | ATB debits (col 6) | "$2,186.00" "$912.00" "$20.00" "$231.00" "$1.00" "$3,350.00" |
| `:148-150` | ATB credits (col 7) | "$3.00" "$73.00" "$918.00" "$2,356.00" "$3,350.00" |
| `:152-154` | IS debits (col 8) | "$231.00" "$231.00" "$991.00" |
| `:156-158` | IS credits (col 9) | "$73.00" "$918.00" "$991.00" "$991.00" |
| `:160-162` | BS debits (col 10) | "$2,186.00" "$912.00" "$20.00" "$1.00" "$3,119.00" "$3,119.00" |
| `:164-166` | BS credits (col 11) | "$3.00" "$2,356.00" "$2,359.00" "$760.00" "$3,119.00" |
| `:177-179` | second row from the bottom | "Unrealized Gains" "$3.25" |

The "= $401" in two test names (`:92`, `:113`) is stale. Worked from §2.3: own balances Bank 2186, GBP Bank #608 ×
3/2 = 912, Wallet 20, Liabilities −3, Income −73, Income-GBP −#612 → −918, Expenses 120, Equity −2245; unrealized −1
(USD +5, GBP −4 at 3/2; average cost 5/4 gives 0); pre-adjustment Expenses 264, Equity 111, Income −2573; ATB Expenses
231, Equity −2356; IS 231 against 991, Net Income 760; BS 3119 against 2359; last run USD +10, GBP −6 at 9/8 = 3.25
against 0 at cost 5/3, so a gain. red-ale's checks map `:136-138`'s third value to the Net Income row's **IS Debit**
cell, and the text counts to non-blank cells.

**Fixture.** The book is the Equity Statement's (§4.4), so **one** script, `adjusting_entries.py`, imports
`create_test_data` from `account_summary.py`, sets `#`, adds the three 2003 transfers (`SetIsClosingTxn` on the first)
and saves `reports/adjusting-entries.gnucash` for both reports; it then adds the two 2000 transfers with
`env_transfer_foreign` and saves `reports/adjusting-entries-fx.gnucash` for the unrealized run. Balances to check:
Bank 2186, GBP Bank 608, Income −73, Expenses 120, Equity −2245; then Bank 2191, GBP Bank 606, one GBP/USD price on
2000-02-15 of 9/8. No date depends on now.

**Other tests.** `test-stress-options.scm` renders every variation, price source and boolean (`:63-94`, `:237-264`):
a red-ale property on the budget fixture. Nothing under `report/test/` names it.

**Hand-worked branches:** gross adjustment accounts (both Adjustments sides, `:618-621`, `:649-651`); income summary
accounts in both `atb` branches and a non-income `is?` account in IS (`:625-633`, `:655`); Net Loss; zero net income
(blank IS figure, BS `$0.00`); negative `net-bs` and the grand-total branches (`:835-850`); unrealized gains in the work
sheet and each price source (none under `average-cost`); patterns case-sensitive, regular expression, empty, a flagged
transaction with another description, the description without the flag, one matching both (subtracted twice),
entries before the start; foreign currencies shown, a zero-sum foreign account (`$0.00`) against one with no splits
(blank), a missing price; rates including a TRADING commodity; TRADING accounts left out, and only TRADING selected (the
failure); Levels 2 and `all` (indentation, the unrealized row's depth); hyperlinks off; a company name; the `pre-adj`
title; totals differing from rounded rows; a split at exactly the end time (in the table, not in the unrealized sum);
an empty book.

## 3. Cash Flow and Cash flow chart (`standard/cash-flow.scm`, `standard/cashflow-barchart.scm`)

Both templates share one computation, `cash-flow-calc-money-in-out`, in its own module
`5.17:gnucash/report/reports/cash-flow-calc.scm` (CFC below; moved out of `cash-flow.scm` by commit `b84ad7ab7e`,
2020-06-08). Its only callers are `standard/cash-flow.scm:235-241` and `standard/cashflow-barchart.scm:226-232`;
`standard/test/test-cash-flow.scm` also calls it directly (`:42`, `:85`, `:119`).

### 3.1 The shared calculation (CFC:33-106)

107 lines, "copyright 2015 Peter Broadberry, copyright 2019 Christopher Lam", GPL-2.0-or-later (CFC:1-24).

Inputs, an alist (CFC:34-40): `accounts`, `from-date-t64`, `to-date-t64`, `report-currency`,
`include-trading-accounts`, and `to-report-currency`, a closure `(currency amount date) → amount`.

1. **Splits.** `gnc:account-get-trans-type-splits-interval accounts '() from to` (CFC:45-46; RU:692-725): splits of
   `accounts` whose transaction is posted in `[from, to]`, both inclusive; voided splits never match (RU:709-710); no
   description or closing filter, so **closing transactions count**; an empty list gives no splits; split order.
2. **Walk.** For each found split, walk **all** splits `s` of its transaction, in the transaction's split order
   (CFC:55-96):
   1. `s-report-value` = `to-report-currency (transaction currency) |value of s| (posted date)` (CFC:59-63), computed
      before any test.
   2. No account: write `WARNING: s-account is NULL for split: <guid>` to stdout, skip (CFC:65-67).
   3. Skip when: the option is **on** and `s`'s account is TRADING; or `s`'s account is one of `accounts`; or `s` is
      already counted (CFC:68-73).
   4. Value negative: **money in** from `s`'s account: that account's collector in the `money-in` alist (a new key goes
      to the front, `assoc-set!`), `s` marked counted, `s-report-value` added under the report currency to the total
      and to the account (CFC:74-84). Value positive: the same as **money out** (CFC:85-95). Zero: nothing, and no
      row.
3. **Each split once.** Only counted splits are remembered, so a transaction walked twice (several of its splits in
   selected accounts) adds nothing the second time. Every non-zero split of a matching transaction outside the
   selected accounts counts exactly once. red-ale may key the set by split GUID; no figure changes.
4. **Amounts are positive**; the sign only picks the direction, and one account can be in both.
5. **Result** (CFC:100-106): the two account lists in alist order (reverse first appearance), the two alists of
   `(account collector)`, and the two total collectors. Nothing is rounded.

**`to-report-currency`** in both callers (`standard/cash-flow.scm:217-232`, `standard/cashflow-barchart.scm:177-201`):
`gnc:case-exchange-time-fn price-source report-currency commodity-list to-date 0 0` (CU:829-887), with
`commodity-list` the selected accounts' commodities other than the report currency (RU:235-240). Each split converts
at **its own posted date**:

| Price source | One split's conversion | Source |
|---|---|---|
| `pricedb-nearest` (default) | euro, else same commodity, else `gnc-pricedb-convert-balance-nearest-price-t64` at the posted time | CU:708-721 |
| `pricedb-before` | the same with the nearest-before price | CU:723-739 |
| `pricedb-latest` | the latest price; the date is ignored | CU:686-706, :877 |
| `average-cost` | `make-exchange-cost-alist` up to the end of the posted day, memoised by day | CU:857-864 |
| `weighted-average` | `get-commoditylist-totalavg-prices commodity-list` to the end of the posted day, nearest entry | CU:865-874, :741-758 |

The pricedb paths round to the report currency's SCU inside the conversion; a transaction in the report currency
stays exact (`exchange-if-same`, CU:655-657). Euro currencies go through the fixed rates first (CU:635-644), rounding
half away from zero (`5.17:libgnucash/engine/gnc-euro.cpp:80-105`). Under `weighted-average` the price list holds only
the selected accounts' commodities, so a transaction in another currency converts to **0** and its account still gets
a row of 0. A missing price gives 0.

**Figures.** Each collector becomes one amount through `gnc:sum-collector-commodity coll report-currency
exchange-fn` (CU:900-910), with `exchange-fn` = `gnc:case-exchange-fn price-source report-currency to-date`: the
identity here except the euro path, then banker's rounding to the SCU. An empty collector gives 0.

**The trading option works backwards.** The skip rule is `(and include-trading-accounts (eqv? type TRADING))`
(CFC:68-70): with "Include Trading Accounts in report" **off** (the default) trading splits are counted as money in or
out; **on**, they are skipped. The help text says the opposite (`standard/cash-flow.scm:99-101`,
`standard/cashflow-barchart.scm:90-92`). Clean-up commit `2215dfc5e1` (2019) inverted it; before, the rule was
`(or include-trading-accounts (not (eq? type TRADING)))`. 5.15 to 5.17 all have the inverted rule. red-ale matches
5.17 (open question 4).

### 3.2 Cash Flow

**Inventory.** "Cash Flow", GUID `f8748b813fab4220ba26e743aedf38da`, menu `_Income & Expense`, no menu tip
(`:340-346`; `report/report-core.scm:158`). 346 lines, GPL-2.0-or-later, by Herbert Thoma (`:1-28`).

**Options** (`:57-106`; default section General, `:104`; no callbacks):

| Section | Name | Key | Type | Default | Line | Does |
|---|---|---|---|---|---|---|
| General | Start Date, End Date | aa, ab | date (`add-date-interval`) | accounting period | `:61-63` | start of day, end of day (`:139-146`) |
| General | Report's currency | b | currency | report currency setting | `:66-68` | target of every conversion |
| General | Price Source | c | multichoice (`add-price-source`) | pricedb-nearest | `:70-72` | the exchange functions of §3.1 |
| General | Show Exchange Rates | d | boolean | #f | `:74-76` | the rates table |
| General | Show Full Account Names | e | boolean | #t | `:78-80` | full name or name, in rows and list |
| General | Show Selected Accounts | f | boolean | #t | `:82-84` | the "Selected Accounts" list and the first ruler |
| Accounts | Account Display Depth | aa | multichoice all, 1–6 | 2 | `:87-90` (`add-account-selection`, OU:96-121) | depth of the list only |
| Accounts | Always show sub-accounts | ab | boolean | #f | OU:106-110 | adds every descendant (`:158-160`) |
| Accounts | Account | ac | accountList, any type | BANK CASH ASSET STOCK MUTUAL in display order | `:91-95` | the selected accounts |
| Accounts | Include Trading Accounts in report | b | boolean | #f | `:99-101` | inverted, §3.1 |

**Computation.**

1. Accounts: with "Always show sub-accounts", `gnc-accounts-and-all-descendants` (a hash set,
   `5.17:bindings/engine.i:241-254`); then sorted by `gnc:account-full-name<?`, locale collation (RU:160-161)
   (`:158-161`).
2. Title `<Report name> - <gnc-date-interval-format from to>` (`:186-190`), set even before the warning.
3. No accounts: the no-account warning only (`:192`, `:331-334`).
4. Selected Accounts list (`:194-215`): depth `all` is `gnc:accounts-get-children-depth` (RU:163-168); accounts at
   depth ≤ that, as account links, `"~a and subaccounts"` when at that depth with children and "Always show
   sub-accounts" on, `"~a and selected subaccounts"` when it is off, else `"~a"`.
5. CFC over the sorted accounts (`:235-241`); both result lists sorted again by full name (`:242-249`; a stable sort).
6. The document: with "Show Selected Accounts", the text "Selected Accounts", the list and a ruler (`:253-263`); then
   one two-column table with no headers: heading "Money into selected accounts comes from", a row per account (link,
   its collector summed), `grand-total` "Money In"; ruler; heading "Money out of selected accounts goes to", rows,
   "Money Out"; ruler; "Difference" = `gnc:collector- in out` summed (`:266-317`), so `round(in − out)`; then the
   rates table with "Show Exchange Rates" (`:323-327`; HU:257-274; `case-price-fn` at `to`, `:152`).
7. All figures are positive except Difference. No empty-data warning: an empty period shows $0.00 three times.

### 3.3 Cash flow chart

**Inventory.** "Cash Flow Barchart" (the features list's "Cash flow chart"), GUID `5426e4d987f6444387fe70880e5b28a0`
(`cashflow-barchart-uuid`, `:340-341`), menu `_Income & Expense`, tip "Shows a barchart with cash flow over time"
(`:343-350`). 350 lines, GPL-2.0-or-later, by Jose Marino (`:1-28`).

**Options** (`:63-119`; default section General; no callbacks):

| Section | Name | Key | Type | Default | Line |
|---|---|---|---|---|---|
| General | Start Date, End Date | aa, ab | date (`add-date-interval`) | accounting period | `:67-69` |
| General | Step Size | b | multichoice (`add-interval-choice`, OU:62-74) | MonthDelta | `:71-72` |
| General | Report's currency | c | currency | report currency setting | `:74-75` |
| General | Price Source | d | multichoice | pricedb-nearest | `:77-79` |
| Accounts | Accounts | a | accountList, any type, help "Report on these accounts." | BANK CASH ASSET STOCK MUTUAL | `:82-88` |
| Accounts | Include Trading Accounts in report | b | boolean | #f (inverted) | `:90-92` |
| Display | Show Money In, Show Money Out, Show Net Flow | a, b, c | boolean | #t each | `:95-105` |
| Display | Show Table | d | boolean | #f | `:107-109` |
| Display | Plot Width, Plot Height | ea, eb | plotSize | 100 % each | `:112-114` |

No "Always show sub-accounts": only the listed accounts.

**Computation.**

1. From start of day, to end of day (`:144-151`); intervals `gnc:make-date-interval-list` (DU:323-349) with the step
   (`:165-168`): `[d, next − 1 s]`, the last ending at `to`; months step from the start date; swapped dates are
   swapped back with a warning in the log.
2. No accounts: the no-account warning (`:176`, `:329-332`).
3. Per interval (`:222-241`): CFC on the interval; money in and out each `sum-collector-commodity`, rounded per
   interval; net = in − out of the rounded figures (`gnc:monetary+`, RU:460-465; `gnc:monetary-neg`,
   `5.17:bindings/guile/gnc-numeric.scm:91-94`).
4. Totals: sums of the rounded per-interval figures (`:244-252`).
5. Zero test (`:289-299`): `gnc:not-all-zeros` over in, out and net **whether shown or not**; all zero gives the
   empty-data warning and no table. Otherwise the chart, even with every visible series zero or none shown.
6. Chart (`:256-285`): `bar` (the default type), not stacked; title `[Report name, interval format]`; y axis label the
   currency's mnemonic, currency format; labels `qof-print-date` of each interval's **start** (`:186-188`); series
   "Money In" `#0074D9`, "Money Out" `#FF4136`, "Net Flow" `#2ECC40`, each when its option is on.
7. Links (`:207-217`): every value links to the Cash Flow (`f8748b…`) with the chart's values copied for every option
   the Cash Flow also has by section and name (`gnc:make-report-anchor`, HU:168-185) — Report name, Stylesheet, Add
   options summary, Report's currency, Price Source, Include Trading Accounts — then Start Date and End Date set to the
   interval and Accounts/**Account** to the accounts. The linked Cash Flow is therefore titled "Cash Flow Barchart -
   …".
8. Table (with data and "Show Table", `:301-326`): text "Overview:" (h3), headers "Date" and the shown series, a row
   per interval (date text, monetaries), then "Total".

### 3.4 Library and result fit

| Need | Used by | GnuCash | red-ale today |
|---|---|---|---|
| The calculation | both | CFC:33-106 | **missing**: `lib/cash-flow-calc` (`cash_flow_calc_money_in_out(settings)`, settings a map, alists as `[account, collector]` arrays with prepend) |
| Split query, collectors, `sum_collector_commodity`, `case_exchange_fn`, `case_price_fn`, `case_exchange_time_fn`, `accounts_get_commodities`, `account_full_name_lt`, `accounts_get_children_depth`, `accounts_and_all_descendants`, the OU helpers, day bounds, `make_date_interval_list`, `deltasym_to_delta`, `monetary_plus`, `not_all_zeros`, rates table, both warnings | both | RU, CU, OU, DU, HU | present |
| A report link carrying another template's options (copy the names the target declares) | chart | HU:168-185 | a private helper in `category-barchart.rhai`; move a general one to `lib/html-utilities` |
| A list of account links | Cash Flow | `report/html-text.scm:210-215` | **no result form** (below) |

Result: the table, chart, links per value and warnings fit today. The "Selected Accounts" `<ul>` of account links does
not: a text block has no links (category-barchart joined its list into one text, `scripts/standard/category-barchart.rhai:328`,
losing them). Proposed: an additive `list` block, `items: [{ text, account? }]`, which category-barchart's negative
accounts list could also use; failing that, a one-column table of `account` rows.

### 3.5 GnuCash's tests

**`standard/test/test-cash-flow.scm`** (146 lines, no licence notice; cite under GnuCash's licence). It calls CFC with
identity conversions (`:26-28`) and never renders the report. Each of its three groups makes a structure (`:14-19`):
"Root" (ASSET) › "Asset" › "Bank", "Wallet", and "Expenses" (EXPENSE), USD; the groups share one book, so it ends with
three "Root" trees. Dates: `to` = 23:59:59 today, `from` = `to` − 1 day; transactions on today
(`env-create-transaction`, TEE:118-142: credit `+aaa`, debit `−aaa`).

| Test | Transactions | Accounts | Line | Expected |
|---|---|---|---|---|
| one-tx-in | Bank +100 / Expenses −100 | Bank | `:53-54` | money-out alist `()` |
| | | | `:55-59` | Expenses' in collector 100 |
| | | | `:60-64`, `:65-69` | in total 100, out total 0 |
| one-tx-skip | Bank +100 / Wallet −100 | Wallet, Bank | `:95-102` | both alists `()`, both totals 0 |
| both-way | Bank +100 / Expenses −100; Expenses +50 / Bank −50 | Wallet, Bank | `:137-144` | Expenses in 100, out 50; totals 100 and 50 |

As red-ale checks: the Cash Flow on each group's accounts (by GUID) for 2024-03-15, full names on, USD (account full
names use `.`, the engine default separator, `5.17:libgnucash/engine/Account.cpp:58`): group 1 "Root.Expenses"
$100.00 in, Money In $100.00, Money Out $0.00, Difference $100.00 (worked); group 2 no rows and $0.00; group 3
"Root.Expenses" $100.00 in and $50.00 out, Difference $50.00 (worked).

**`standard/test/test-cashflow-barchart.scm`** (196 lines, GPL-2.0-or-later notice). C locale (`:32`), no date format
set, so dates print `%m/%d/%y`. Structure (`:51-57`): "Root" (ASSET) › "Asset" › "Bank", "Wallet"; "Expenses";
"Income"; three groups in one book. `date-0` = start of this month, `date-1`, `date-2` the next two days. Every run:
Show Table, Start date-0, End date-2, DayDelta, pricedb-nearest, USD, Accounts Wallet and Bank. Cells are read as
numbers by stripping all but digits, `-` and `.` (`:43-49`); data rows exclude the header.

| Test | Transactions | Line | Expected |
|---|---|---|---|
| in-txn | d1 Bank +1 / Income; d2 Wallet +5 / Income | `:85-99` | every row in = net, out = 0; column 2 `0 1 5 6`; 4 columns; 4 rows |
| out-txn | d1 Bank +100 / Income, Expenses +1 / Bank; d2 Wallet +100 / Income, Expenses +5 / Wallet | `:133-147` | in − out = net; column 3 `0 1 5 6`; 4 columns; 4 rows |
| null-txn | in-txn plus d1 Bank +20 / Wallet −20 | `:181-195` | as in-txn |

Strings under the pinned now (date-0 = 2024-03-01): dates `03/01/24`, `03/02/24`, `03/03/24`, `Total`; in-txn and
null-txn Money In `$0.00 $1.00 $5.00 $6.00`, Money Out all `$0.00`, Net as In; out-txn In `$0.00 $100.00 $100.00
$200.00`, Out `$0.00 $1.00 $5.00 $6.00`, Net `$0.00 $99.00 $95.00 $194.00`. red-ale checks the strings.

**Fixtures.** Neither book exists. One script, `cash_flow.py`, builds `reports/cash-flow.gnucash` (three groups,
2024-03-15) and `reports/cashflow-barchart.gnucash` (three groups, d1 = 2024-03-02, d2 = 2024-03-03), with
`engine_extras.py`'s `env_create_account_structure_alist`, `env_create_transaction`, `start_this_month`,
`incdate_days`; both depend on now, fixed by the pinned 2024-03-15 and `TZ=UTC`.

**Render-only.** `test-charts.scm` renders the chart with defaults (`:71-77`) and on book A (`:163-190`); its case
`'cashflow-barchart-test` asserts nothing (`:245-246`); `cash-flow.scm` is imported (`:7`) but never run.
`charts-daily.expected.json` already lists the chart under `renderOnly`. `test-stress-options.scm` renders both.

**Hand-worked cases** (figures worked from the fixture scripts; recheck when written): on `charts-daily`, the Cash Flow
over 2024 for Bank and Liability: in Root.Equity $3.00, Root.Expenses $11.00, Root.Income $37.00 (the void 10 left
out), Money In $51.00; out Root.Expenses $50.00; Difference $1.00; the list shows only Root.Liability at depth 2. The
chart monthly over 2024: In 3, 29, 0, 0, 0, 0, 11, 0, 8, 0, 0, 0; Out 8, 27, 15, then 0; totals 51, 50, 1. On
`test-data`, GBP Bank over 2000: in from Bank $9.00; out to Bank $14.00 (GBP 10 at 1.4, nearest 2000-01-15);
Difference −$5.00; with `pricedb-latest` out is $15.00. Branches: no accounts (both); all zero (no table even with Show
Table); a hidden non-zero series still draws the chart; each Show flag off; transfers between selected accounts; a
multi-split with two selected splits (test-data's 1971 transaction: Bank alone gives Expenses 80 and Wallet 20 out;
Bank and Wallet give Expenses 80 only); one account in and out; voids and zero splits; the trading option both ways in
a trading book; every price source at each posted date; weighted-average with an unheld currency (a $0.00 row); "Always
show sub-accounts" on and off; depth 1, 2, all and both suffixes; full names off; list off; rates; row order; the
link's options; a short last interval and swapped dates; a closing transaction counted; a EUR report currency; a split
with no account (the log line).

## 4. Equity Statement (`standard/equity-statement.scm`)

### 4.1 Inventory and options

One template, "Equity Statement", GUID `c2a996c8970f43448654ca84f17dda24`, menu `_Income & Expense` (`:501-507`;
`report/report-core.scm:158`). 510 lines, GPL-2.0-or-later (`:29-44`). The default section is Accounts (`:160`).

| Section | Name | Key | Type | Default | Line |
|---------|------|-----|------|---------|------|
| General | Report Title | a | string | "Equity Statement" | `:100-102` |
| General | Start Date, End Date | ca, cb | date (`add-date-interval`, OU:54-59; `5.17:bindings/guile/options.scm:330-334`) | start and end of the accounting period | `:105-107` |
| Accounts | Accounts | a | accountList, any type | every account of the types BANK CASH CREDIT ASSET LIABILITY STOCK MUTUAL CURRENCY PAYABLE RECEIVABLE EQUITY INCOME EXPENSE TRADING, in display order | `:110-121` |
| Commodities | Report's currency | a | currency (`add-currency`, OU:124-130) | the report currency setting | `:124-126` |
| Commodities | Price Source | b | multichoice (`add-price-source`, OU:133-144): average-cost, weighted-average, pricedb-before, pricedb-nearest, pricedb-latest | pricedb-nearest | `:128-130` |
| Commodities | Show Foreign Currencies | c | boolean | #t | `:132-134` |
| Commodities | Show Exchange Rates | d | boolean | #f | `:136-138` |
| Display | Show accounting-style rules | f | boolean | #f | `:141-143` |
| Entries | Closing Entries pattern | a | string | "Closing Entries" | `:149-151` |
| Entries | Closing Entries pattern is case-sensitive | b | boolean | #f | `:152-154` |
| Entries | Closing Entries Pattern is regular expression | c | boolean | #f | `:155-157` |

"Show Foreign Currencies" is declared and never read: the renderer reads every other option (`:197-230`) and no
other line names `optname-show-foreign`. Every figure is one amount in the report's currency. No option enables or
disables another.

### 4.2 Computation

Dates (`:202-210`): **start** is 23:59:59 of the day *before* the Start Date (`gnc:time64-previous-day`, DU:495-496,
then `time64-end-day-time`); **end** is 23:59:59 of the End Date. The printed start is the Start Date option itself.

Accounts are split by type with `gnc:decompose-accountlist` (RU:188-208): assets (every asset-like type), liabilities,
equity, and income + expense + trading together (`:233-243`). An empty selection gives the no-account warning and
nothing else (`:285-293`).

Every balance is a commodity collector of amounts in each account's own commodity:

| Figure | Function | Line |
|--------|----------|------|
| start, end asset balance | `gnc:accounts-get-comm-total-assets` with `gnc:account-get-comm-balance-at-date acct date #f` (children excluded; `xaccAccountGetBalanceAsOfDate`, strictly before the time) | `:272-276`, `:296-302` |
| start, end liability balance | the same over liabilities (signs as stored: negative for a normal liability) | `:304-310` |
| start, end equity balance | the same over equity | `:330-336` |
| pre-start, pre-end retained earnings | `gnc:accountlist-get-comm-balance-at-date-with-closing` over income/expense/trading at start and at end: every non-void split up to the date, closing ones included (RU:612-613, :672-682) | `:312-318` |
| income/expense closing | `gnc:account-get-trans-type-balance-interval-with-closing` over income/expense/trading with the closing pattern, start to end: non-void splits in the interval whose transaction's description contains the pattern (case and regexp per the options) **or** is a closing transaction (the `book_closing` flag) (RU:692-725) | `:320-322` |
| net income | income/expense closing − the income/expense change over the interval with closing entries | `:324-328` |
| equity closing | the closing-pattern sum over the equity accounts, start to end | `:361-363` |
| withdrawals | `account-get-total-flow 'in`: for each selected equity account, each split of `xaccAccountGetSplits` (no query: void splits are read, with amount 0) whose transaction's posted date is in `[start, end]` inclusive and whose **amount** is positive, added under the **transaction's currency** | `:164-180`, `:378-379` |

The pattern's `'positive #f` entry (`:249`) is read by nothing (RU:697-703 reads only `str`, `cased`, `regexp`,
`closing`).

Then, in collector arithmetic (`:338-397`):

- book balance at a date = assets + liabilities + equity + retained earnings at that date;
- unrealized gains at a date = the book balance converted by the price source's exchange function at that date,
  minus the same converted by the `average-cost` exchange function at that date (`:264-270`), each through
  `gnc:sum-collector-commodity` (CU:900-911, which rounds each commodity's converted amount to the report currency's
  SCU, `GNC-RND-ROUND`, before adding);
- net unrealized gains = unrealized at end − unrealized at start;
- net investment = start equity − (end equity − equity closing);
- investments = net investment + withdrawals;
- capital increase = net income + investments + net unrealized gains − withdrawals;
- start capital = start unrealized gains − start equity − pre-start retained earnings;
- end capital = start capital + capital increase.

**Rows** (`:405-467`), each through `add-report-line`, which converts the collector to one report-currency amount with
the given exchange function; when the line has a negative label and that amount is negative, it shows the negative
label and the negated amount:

| Row | Label (negative label) | Exchange fn | Amount column | Row markup |
|-----|------------------------|-------------|---------------|------------|
| Opening capital | "Capital, <start date>" | start | 2 | primary-subheading |
| Net income | "Net income for Period" ("Net loss for Period") | end | 1 | |
| Investments | "Investments for Period" | end | 1 | |
| Withdrawals | "Withdrawals for Period" | end | 1 | |
| Unrealized | "Unrealized Gains for Period" ("Unrealized Losses for Period"); **left out** when net unrealized gains are all zero (`gnc-commodity-collector-allzero?`) | end | 1 | |
| Change | "Increase in capital" ("Decrease in capital"); a rule above when "Show accounting-style rules" | end | 2 | |
| Closing capital | "Capital, <end date>" | end | 2 | primary-subheading |

Dates print with `qof-print-date`; the end date printed is the 23:59:59 time, so the same day. The table is three
columns wide (`gnc:html-table-add-labeled-amount-line!`, HAT:870-923: label at column 0, amount at column 1 or 2).
GnuCash first appends a row of two empty min-width cells (`:422-423`), layout only.

**Exchange.** `start` and `end` exchange functions are `gnc:case-exchange-fn` of the price source at the start and end
times (`:254-259`); price functions likewise (`:261-262`). With "Show Exchange Rates", a ruler is appended to the
statement table and a second table follows: one row of two cells, the rates table at start
(`gnc:html-make-rates-table`, HU:257-273, over the selected accounts) under the start date, the one at end under the
end date (`:473-491`).

**Title** (`:278-283`): "${company-name} ${report-title} For Period Covering ${start} to ${end}" with the company name
or "", so an unnamed book's title starts with a space.

**Quirks to keep.** Withdrawals count every debit to an equity account, closing entries included: closing a net loss
debits equity and counts as a withdrawal (the GnuCash test below shows it: $255.00 = 111 + 111 + 33). Withdrawals add
an equity account's amount under the transaction's currency, so a foreign-currency equity account mixes units. Neither
is fixed.

### 4.3 Library and result fit

Everything the statement needs is in v0.2's library: `decompose_accountlist`, `accounts_get_comm_total_assets`,
`account_get_comm_balance_at_date`, `accountlist_get_comm_balance_at_date_with_closing`,
`account_get_trans_type_balance_interval_with_closing`, `accountlist_get_comm_balance_interval_with_closing`, the
collector operations, `commodity_collector_allzero`, `sum_collector_commodity`, `case_exchange_fn` (with
`average-cost`), `case_price_fn`, `make_rates_table`, `make_no_account_warning`, `add_labeled_amount_line`,
`time64_end_day_time`, `decdate`, `book.company_name()`, `a.splits()`. `gnc:time64-previous-day` is one line on
`decdate` (DU:495-496) and goes into `lib/dates`. `account-get-total-flow` is local to the report.

The result fits: one table (label column; one amount column with `levels: 2`), rows from `add_labeled_amount_line`
(`heading`-like rows for the two capital lines, which GnuCash marks `primary-subheading`), a `rule` row before the
rates, and the rates as a `group` of two tables titled with the two dates.

### 4.4 GnuCash's test (`standard/test/test-equity-statement.scm`, 112 lines, GPL-2.0-or-later notice)

Book: `create-test-data` (`5.17:bindings/guile/test/test-engine-extras.scm:389-480`), GBP's user symbol `#`
(`:64-66`), then three transfers (`:68-75`; `env-transfer` moves `amount` from its first account to its second,
`test-engine-extras.scm:199-223`):

| Date | From | To | Amount | Description | Closing flag |
|------|------|----|--------|-------------|--------------|
| 2003-06-30 | Expenses | Equity | 111 | Closing Entries | yes (`xaccTransSetIsClosingTxn`) |
| 2003-06-01 | Expenses | Equity | 33 | Adjusting Entries | no |
| 2003-07-01 | Income | Equity | −2500 | Adjusting Entries | no |

Options: Start Date absolute 1970-01-01, End Date absolute 2005-01-01 (`:77-80`); everything else default; date format
ISO (`:63`); C locale (`:35`).

| Line | Check | Expected |
|------|-------|----------|
| `:82-84` | texts in table 1 ("current table has 22 cells", but expects 14) | 14 |
| `:86-88` | row 2 | "Capital, 1970-01-01", "$29.00" |
| `:90-92` | row 3 | "Net income for Period", "$620.00" |
| `:94-96` | row 4 | "Investments for Period", "$2,722.00" |
| `:98-100` | row 5 | "Withdrawals for Period", "$255.00" |
| `:102-104` | row 6 | "Unrealized Losses for Period", "$1.00" |
| `:106-108` | row 7 | "Increase in capital", "$3,086.00" |
| `:110-112` | row 8 | "Capital, 2005-01-01", "$3,115.00" |

Worked from §4.2, as a check that the reading above is right: start capital = 0 − 0 − (−29) = 29 (the 1969 income of
29; the void 10 counts 0); net income = closings (−111 −111 USD) − (income/expense change: 76 USD, −612 GBP) =
−298 USD + 612 GBP, and 612 GBP at the 2000-02-15 price of 1.5 (nearest to 2005) is 918, so 620; equity at end
= 111 + 111 + 33 − 2500 = −2245, equity closing = 222, so investments = 2245 + 222 + 255 = 2722; end book balance =
5 USD − 4 GBP, which is −1 USD at 1.5 and 0 at the average cost of (14 − 9)/(10 − 6) = 5/4, so unrealized −1;
increase = 620 + 2722 − 1 − 255 = 3086; end capital 3115. The count of 14 is the seven rows' label and amount; the
first row's empty cells hold no text.

**Fixture.** The Trial Balance's test builds the same book (§2.5), so both use `reports/adjusting-entries.gnucash`,
built by one script through the 5.17 Python bindings (`scripts/report-fixtures/account_summary.py` already ports
`create-test-data` and the `#` symbol). `test-data.gnucash` cannot take the three transfers: other reports' checks hold
to it.

**Hand-worked branches** (none in GnuCash's test): no accounts (the warning); a net loss, unrealized gains and a
decrease in capital (each negative label); "Show accounting-style rules"; "Show Exchange Rates" (the ruler and both
rates tables, GBP on `test-data`); each price source, including `average-cost` against itself (unrealized always 0); a
closing pattern matched by description only, case-sensitive, and as a regular expression; a Start Date at a split's
day (the start bound is the previous day's 23:59:59); a book with a company name (the title).

## 5. Multicolumn Balance Sheet and P&L (`standard/balsheet-pnl.scm`)

### 5.1 Inventory

| | Balance sheet | Profit & loss |
|---|---|---|
| Name | "Balance Sheet (Multicolumn)" (`:1242`) | "Income Statement (Multicolumn)" (`:1243`) |
| GUID | `065d5d5a77ba11e8b31e83ada73c5eea` (`:1248`) | `0e94fd0277ba11e8825d43e27232c9d4` (`:1256`) |
| Menu | `_Assets & Liabilities` (`:1249`; `report/report-core.scm:157`) | `_Income & Expense` (`:1257`; `:158`) |
| Generator, renderer | `(multicol-report-options-generator 'balsheet)`, `(multicol-report-renderer rpt 'balsheet)` (`:1250-1251`) | the same with `'pnl` (`:1258-1259`) |

1,261 lines, both templates; "By Christopher Lam, 2018", "Improved from balance-sheet.scm", GPL-2.0-or-later (`:1-26`).
`milestone5.md` calls the second template "P&L"; its name is "Income Statement (Multicolumn)". The file does not use
`html-acct-table.scm`: it has its own walker, `add-multicolumn-acct-table` (`:279-607`).

### 5.2 Options

One generator; the P&L adds one option (`:269-273`). Default section General (`:275`).

| Section | Name | Key | Type | Default | Line |
|---|---|---|---|---|---|
| General | Period duration | c2 | multichoice with callback: `disabled`, YearDelta, HalfYearDelta, QuarterDelta, MonthDelta, TwoWeekDelta, WeekDelta | `disabled` | `:135-169` |
| General | Disable amount indenting | c3 | boolean | #f | `:171-173` |
| General | Enable dual columns | c4 | boolean | #t | `:179-181` |
| General | Enable chart | c5 | boolean, help "Enable link to chart" | #f | `:175-177` |
| General | Period order is most recent first | c5 | boolean | #t | `:183-185` |
| General | If more than 1 period column, include overall period? (P&L only) | c6 | boolean | #f | `:269-273` |
| General | Start Date, End Date | ca, cb | date (`add-date-interval`) | accounting period | `:132-133` |
| Accounts | Accounts | a | accountList, any type | BANK CASH CREDIT ASSET LIABILITY STOCK MUTUAL CURRENCY PAYABLE RECEIVABLE EQUITY INCOME EXPENSE TRADING | `:188-199` |
| Accounts | Levels of Subaccounts | b | multichoice all, 1–6 | `all` | `:203-205` |
| Commodities | Common Currency | b | complex boolean (callback) | #f | `:208-219` |
| Commodities | Report's currency | c | currency | report currency setting | `:221-223` |
| Commodities | Price Source | d | multichoice | pricedb-nearest | `:225-227` |
| Commodities | Show original currency amount | e | boolean | #t | `:229-231` |
| Commodities | Show Exchange Rates | f | boolean | #t | `:233-235` |
| Display | Include accounts with zero total balances | a | boolean | #t | `:238-240` |
| Display | Omit zero balance figures | b | boolean | #f | `:242-244` |
| Display | Parent account amounts include children | c | boolean | #t | `:246-248` |
| Display | Display accounts as hyperlinks | e | boolean | #t | `:251-253` |
| Display | Display amounts as hyperlinks | e5 | boolean | #t | `:255-257` |
| Display | Account full name instead of indenting | f | boolean | #f | `:259-261` |
| Display | Label sections | g | boolean | #t | `:263-264` |
| Display | Include totals | h | boolean | #t | `:266-267` |

"Enable chart" and "Period order…" share key `c5`; red-ale keeps declaration order. The callbacks only toggle
selectability (`:147-169`, `:211-219`); red-ale never calls them. The renderer reads "Disable amount indenting" and
"Enable dual columns" only with period `disabled` (`:633-638`), "Price Source" only with a common currency
(`:677-678`); the balance sheet ignores Start Date without a period (`:685`); its chart link needs a period (`:966`),
the P&L's does not (`:1127-1129`). "Levels of Subaccounts" is "not well debugged" in GnuCash's own words (`:201-202`).

### 5.3 Computation

**Dates and columns** (`:621-690`). End = end of day of End Date; start = start of day of Start Date for the P&L, end
of day for the balance sheet. With a period, `gnc:make-date-list start end delta` (DU:356-379: month steps by
`incdate-months` from the start, keeping month ends; stops at the first date ≥ end and appends end; swaps reversed
dates with a log warning); without, the P&L has `(start end)` and the balance sheet `(end)`. Balance sheet columns
are the dates (`:1000`); P&L columns the intervals between consecutive dates (`:1165`), plus an overall column when
"include overall period" is on and there are two or more periods (`:1166-1169`). Headers: the balance sheet's
`qof-print-date` of the date (`:978-987`); the P&L's `from`, a line break, `" to "`, `to` (`:1140-1153`).

**Price date of a column** (`col-idx->price-date`, `:741-749`): now under `pricedb-latest`; the end for the overall
column; the date for the balance sheet; the end for the last P&L column; otherwise `decdate(next date, DayDelta)`,
which is **00:00 of the interval's last day**. P&L headers and Transaction Report links take their `to` from it, so
under `pricedb-latest` with a common currency every P&L header reads "<start> to <today>" (`:744`, `:1093`).

**Rows** (`:279-607`). The accounts in the **option's order** (an account-list value is not sorted,
`5.17:libgnucash/engine/gnc-option-impl.hpp:859-870`), decomposed by kind (`:817-829`); `maxindent` = 1 + the deepest
selected account's depth (`:840-841`). Each section adds:

1. a title row (section name and header or empty cells) when the section asks for it and "Label sections" is on
   (`:537-548`);
2. account rows (`:550-584`); an account's "descendants" are its descendants in this section's list (`:448-450`):
   - **recursive** ("Parent account amounts include children" on): a parent with descendants gets a subtotal row at
     its level (plain name, account link, summed amounts), shown when zero rows are shown or any column is non-zero
     (`:483-507`, `:567`); when its own balance is non-zero in any column, a row for the parent alone follows one level
     deeper (`:568-569`);
   - **multilevel** (off): every account gets a row; a parent with descendants is always shown, its amount one step
     further left; climbing back up emits "Total For <name>" rows until depth 0, a parent outside the list, or the
     next account's depth (`:570-583`);
   - a leaf row shows when zero rows are shown or any column is non-zero (`:509-534`; tested on unconverted sums,
     `:469-481`); rows deeper than the level limit are hidden, their amounts kept in subtotals (`:343-344`);
3. "Total For <section>", when "Include totals" is on or the section forces it (`:586-606`);
4. a spacer row (`:442-446`, `:607`).

Layout (`:346-369`): indent cells, the label, the data cells (reversed for "most recent first", `:364`). The staircase
exists only with one column and indenting enabled (`:333`): in red-ale terms `levels = maxindent`, `level = maxindent −
amount-indent`, with `amount-indent` = `maxindent` for titles and totals, `maxindent − lvl` for subtotals,
`maxindent − lvl − account-indent` for account rows. Full names drop the indent (`:349`, `:358-360`, `:427`).

**Per-column balances.** Per account, `gnc:account-accumulate-at-dates` (RU:505-512) with a split closure building a
`col-datum` (`:694-714`): the last split at or before the date, its no-closing balance as a monetary, its balance with
closing, and a snapshot of a running collector of every split's `value` under its transaction's currency; before the
first split, zeros and an empty collector (`:703-705`). Balance sheet cell: the with-closing balance at the date
(`:868-870`). P&L cell: no-closing balance at the interval's end minus at its start (`:1095-1107`), so **closing
transactions are left out**; overall = last minus first.

Sums (`:371-384`, `:452-467`): a local collector applies the section's sign; subtotals and totals convert; account
rows convert only with "Show original currency amount" off (`:529`); the virtual rows (Retained Earnings, Unrealized
Gains) are `#(label fn)` vectors whose result is used as is, a list appended unconverted (`:458-466`). Cell text
(`:386-411`): zero monetaries dropped under "Omit zero balance figures"; each remaining one converted when "Show
original currency amount" is on and a conversion applies, shown as the original in `<i><small>` and a space, then the
converted amount (inside the amount link); lines separated by `<br>`.

**Conversion.** Common currency = "Report's currency" when "Common Currency" is on (`:659-661`). `has-price?(c)` is
true when the price database's **latest** conversion of one unit is non-zero (`:662-676`), whatever the price source,
so under `average-cost` or `weighted-average` a commodity with no database price is never converted. `convert-curr-fn`
(`:755-763`) converts when there is a common currency, the commodity differs and `has-price?`, with
`gnc:case-exchange-time-fn(source, common, accounts-get-commodities(accounts), #f, #f, #f)` (`:733-736`) at the
column's price date:

| Source | Amounts (`case-exchange-time-fn`) | Rate shown (`case-price-fn`) |
|---|---|---|
| `average-cost` | cost alist to the **end** of the price date's day (CU:849-856), exact product (CU:663-677) | cost alist to the price date itself (CU:772-774) |
| `weighted-average` | running average to the end of that day, nearest entry (CU:857-865, :741-758) | book-wide Σ\|value\| / Σ\|amount\| (CU:599-607) |
| `pricedb-before`, `-nearest`, `-latest` | the price database, rounded to the SCU (CU:686-733) | the price database's price |

Every source tries the euro rates first. So under `weighted-average` the rates row need not show the rate the amounts
used, and in a P&L column other than the last, `average-cost` amounts use costs to 23:59:59 while the rates row uses
00:00 of the same day. GnuCash's quirks are kept.

**Exchange Rates row** (`:770-797`): for the commodities of the section's accounts other than the common currency
(`gnc:accounts-get-commodities-sorted`, RU:242-251; balance sheet asset, liability and equity accounts `:1074`, P&L
income and expense `:1218`), `"<1 unit> <rate>"` with `gnc:default-price-renderer` (`report/html-style-info.scm:221-222`):
`#1.00 $1.70`, `1. FUNDS $480 + 85/104`; with no price, `<1 unit> <symbol> missing`. It is the title row of an
"Exchange Rates" section, shown with a common currency and "Show Exchange Rates" (`:1072`, `:1215`), so it vanishes
with "Label sections" off.

**Balance sheet sections** (`:1022-1083`): "Date" header (with a period); "Asset" (left); "Liability" (right,
negated); "Equity" (right, negated): the equity accounts, then the TRADING accounts when the Book uses trading
accounts, else with a common currency a virtual "Unrealized Gains" row, then a virtual "Retained Earnings" row when
income or expense accounts are selected (`:1040-1052`); "Liability and Equity" (right, negated, total only, not forced,
`:1055-1070`); "Exchange Rates" (right). Retained earnings (`:951-963`): income and expense balances with closing at
the date, exchanged with `case-exchange-fn` at the date (now under `pricedb-latest`) into one monetary only when every
income and expense commodity `has-price?`, else the unconverted list (`:918-927`). Unrealized gains (`:933-946`, common
currency only): value balances minus balances of the asset and liability accounts, exchanged likewise. No sign
preference is read.

**P&L sections** (`:1189-1226`): "Period" header; "Income" (left, negated); "Expense" (right); "Net Income" (left,
negated, no rows, forced total) only when both income and expense accounts are selected (`:1207-1213`); "Exchange
Rates" (left). Dual columns need no period and the header needs one, so the right-hand header code
(`:1027-1031`, `:1194-1198`) never runs.

**Links.** Account names link to the register unless placeholder (`:432-436`). Balance sheet amounts of account rows
link to the **last split at or before the date** (`gnc:split-anchor-text`, HU:90-91; `:874-886`), none before the first
split. P&L account and subtotal amounts link to a **Transaction Report** (`2fe3b9833af044abb929a88d5a59620f`) through
`gnc:make-report-anchor` (`:1109-1124`; HU:168-185), copying the options the two share (General's three, so the linked
report is titled "Income Statement (Multicolumn)") and setting General Start Date and End Date (the price date),
Currency "Show original currency amount", Currency "Common Currency" (set to a commodity, which a boolean reads as
true, `5.17:bindings/guile/gnc-optiondb.i:356-358`), Currency "Report's currency" (the common currency or
`book-main-currency`), Display "Amount" `double`, and Accounts. A "Barchart" text link precedes the table: the balance
sheet's (with "Enable chart" and a period) to the Net Worth Barchart, the P&L's (with "Enable chart") to the
Income/Expense Chart, with dates, currency, step (MonthDelta without a period), price source and accounts
(`:965-976`, `:1079-1083`, `:1126-1138`, `:1222-1226`). `book-main-currency` (`:804-812`) is the currency most selected
accounts use, the report currency on a tie with none; ties keep hash order.

**Title** `"<Report name> "` and the date interval (with a period, or any P&L) or the end date (`:851-858`). The only
warning: no accounts (`:860-865`).

### 5.4 Library and result fit

Present: the OU helpers, `multichoice_callback`, `complex_boolean`, `book.accounts_of_types`,
`decompose_accountlist`, `accounts_get_commodities(_sorted)`, `account_accumulate_at_dates` with a closure, Split
`balance`, `noclosingBalance`, `value`, the collectors, `monetary_plus`, `-m`, the three exchange cases,
`book.price_convert(m, c, now, "latest")` for `has-price?`, `make_date_list`, `decdate`, day bounds, the date printers,
the account handles, the no-account warning. Local to the script: the walker and `book-main-currency` (with a fixed
tie order). Shared helper wanted: report links copying the options the target declares (also the Cash flow chart's,
§3.4).

Result additions, all additive:

| Need | Proposal |
|---|---|
| Balance sheet amount links to a split | link kind `{ "split": "<guid>" }` (also the Transaction Breakdown's and the Transaction Report's) |
| The "Barchart" link | optional `link` on a text block |
| P&L interval headers | optional `until` on a `date` cell; the host fills `localUntil` and a `display` of `<from> to <until>` |
| Converted amounts with originals | a `commodities` cell (exists): one line per monetary, `converted` when converted, `null` when every line was omitted |
| Exchange rates | one row per commodity under an "Exchange Rates" `heading` row: the one-unit monetary, then a `price` cell per column, or text `<symbol> missing`; no schema change (§12 deviation) |
| Roles | title `heading`; account and recursive subtotal `account`; "Total For <account>" `subtotal`; section total `total`; "Total For Liability and Equity" and Net Income `grandTotal`; spacer `spacer`; dual columns a `group` of two tables |

The Transaction Report links use that report's option names, which `report.run` validates only when followed; a test
checks every emitted link against the Transaction Report's declared options once it lands. The chart links can be
checked now.

### 5.5 GnuCash's test (`standard/test/test-balsheet-pnl.scm`)

The multicolumn runs use the v0.2 book unchanged: `fixtures/reports/balsheet-pnl.gnucash` (`scripts/report-fixtures/balsheet_pnl.py`,
from `:51-166`), and `balsheet-pnl.expected.json` **already holds every multicolumn run and check** (ids
`multicol-balsheet-default` … `multicol pnl pricedb-latest`, `"inV02Inventory": false`). Nothing new is needed for
GnuCash's assertions. ISO dates (`:175`), C locale (`:24`).

Base options: balance sheet (`:479-491`) "Period order…" off, Start 1970-01-01, End 1972-01-01, dual columns off,
amount indenting disabled, full names on, levels `all`, rates on; P&L (`:572-584`) the same with 1980-01-01 to
1980-03-31. Changes are cumulative. Lines 617-650 run the **balance sheet** GUID with the P&L's options object
(`gnc:make-report` takes the renderer from the GUID, `report/report-core.scm:389-409`); the P&L-only option stays at its
default, so red-ale runs them as balance sheet runs.

| Line | Template | Changes | Reads | Expected |
|---|---|---|---|---|
| `:496` | BS | base | col 1 | "Asset" "Root" "Root.Asset" "Root.Asset.Bank1" "Root.Asset.Bank1.Bonds" "Root.Asset.Bank1.Current" "Root.Asset.Bank1.Empty" "Root.Asset.Bank1.Savings" "Root.Asset.Broker" "Root.Asset.Broker" "Root.Asset.Broker.Funds" "Root.Asset.ForeignBank" "Root.Asset.ForeignBank.ForeignSavings" "Root.Asset.House" "Total For Asset" "Liability" "Root.Liability" "Root.Liability.Bank2" "Root.Liability.Bank2.CreditCard" "Root.Liability.Bank2.Loan" "Total For Liability" "Equity" "Root.Equity" "Retained Earnings" "Total For Equity" "Total For Liability and Equity" |
| `:506` | BS | base | col 2 | "#200.00" "$106,709.00" "30. FUNDS" "#200.00" "$106,709.00" "30. FUNDS" "$4,709.00" "$2,000.00" "$2,609.00" "$0.00" "$100.00" "$2,000.00" "30. FUNDS" "$2,000.00" "30. FUNDS" "#200.00" "#200.00" "$100,000.00" "30. FUNDS" "#200.00" "$106,709.00" "$9,500.00" "$9,500.00" "$500.00" "$9,000.00" "$9,500.00" "$103,600.00" "$0.00" "#0.00" "$103,600.00" "#0.00" "$113,100.00" "#0.00" |
| `:522` | BS | + YearDelta, Common Currency, USD (`:517-519`) | col 2 | "1970-01-01" "$113,100.00" "$113,100.00" "$8,970.00" "$2,000.00" "$6,870.00" "$0.00" "$100.00" "$4,000.00" "$2,000.00" "$2,000.00" "10. FUNDS " "$130.00" "$130.00" "#100.00 " "$100,000.00" "$113,100.00" "$9,500.00" "$9,500.00" "$500.00" "$9,000.00" "$9,500.00" "$103,600.00" "$0.00" "$0.00" "$103,600.00" "$113,100.00" "#1.00 $1.30" "1. FUNDS $200.00" |
| `:530` | BS | same | col 3 | "1971-01-01" "$116,006.33" "$116,006.33" "$4,709.00" "$2,000.00" "$2,609.00" "$0.00" "$100.00" "$11,000.30" "$2,000.00" "$9,000.30" "30. FUNDS " "$297.03" "$297.03" "#200.00 " "$100,000.00" "$116,006.33" "$9,500.00" "$9,500.00" "$500.00" "$9,000.00" "$9,500.00" "$103,600.00" "$2,906.33" "$0.00" "$106,506.33" "$116,006.33" "#1.00 $1 + 49/101" "1. FUNDS $300.01" |
| `:538` | BS | same | col 4 | "1972-01-01" "$117,437.00" "$117,437.00" "$4,709.00" "$2,000.00" "$2,609.00" "$0.00" "$100.00" "$12,396.63" "$2,000.00" "$10,396.63" "30. FUNDS " "$331.37" "$331.37" "#200.00 " "$100,000.00" "$117,437.00" "$9,500.00" "$9,500.00" "$500.00" "$9,000.00" "$9,500.00" "$103,600.00" "$4,337.00" "$0.00" "$107,937.00" "$117,437.00" "#1.00 $1 + 67/102" "1. FUNDS $346 + 56/101" |
| `:550` | BS | + most recent first | row 1 | "Date" "1972-01-01" "1971-01-01" "1970-01-01" |
| `:562` | BS | most recent first off, End 1980-03-01, period `disabled` | col 2 | "$122,743.52" "$122,743.52" "$5,129.00" "$2,000.00" "$3,029.00" "$0.00" "$100.00" "$16,424.52" "$2,000.00" "$14,424.52" "30. FUNDS " "$1,190.00" "$1,190.00" "#700.00 " "$100,000.00" "$122,743.52" "$9,500.00" "$9,500.00" "$500.00" "$9,000.00" "$9,500.00" "$103,600.00" "$8,373.52" "$1,270.00" "$113,243.52" "$122,743.52" "#1.00 $1.70" "1. FUNDS $480 + 85/104" |
| `:589` | P&L | base | col 1 | "Income" "Root.Income" "Root.Income" "Root.Income.Income-GBP" "Total For Income" |
| `:593` | P&L | base | col 2 | "$250.00" "#600.00" "$250.00" "#600.00" "$250.00" "#600.00" |
| `:604` | P&L | + MonthDelta, Common Currency, USD (`:599-601`) | col 2 | "1980-01-01" " to 1980-01-31" "$1,100.00" "$250.00" "$850.00" "#500.00 " "$1,100.00" "#1.00 $1.70" |
| `:608` | P&L | same | col 3 | "1980-02-01" " to 1980-02-29" "$170.00" "$0.00" "$170.00" "#100.00 " "$170.00" "#1.00 $1.70" |
| `:612` | P&L | same | col 4 | "1980-03-01" " to 1980-03-31" "$0.00" "$0.00" "$0.00" "#0.00 " "$0.00" "#1.00 $1.70" |
| `:620` | BS (P&L options) | + most recent first | row 1 | "Date" "1980-03-31" "1980-03-01" "1980-02-01" "1980-01-01" |
| `:627` | BS | + weighted-average | row −2, last column | "#1.00 $1.499" "1. FUNDS $235 + 3/7" |
| `:634` | BS | + average-cost | same | "#1.00 $1.455" "1. FUNDS $203 + 1/3" |
| `:641` | BS | + pricedb-nearest | same | "#1.00 $1.70" "1. FUNDS $480 + 85/104" |
| `:648` | BS | + pricedb-latest | same | "#1.00 $1.70" "1. FUNDS $480 + 85/104" |

Worked checks agree: unrealized at 1971 = $103,600 − ($97,209 + #200 + 30 FUNDS) = −$2,906.33, shown negated; at
1980-01-01 `weighted-average` 1499/1000 and 16480/70, `average-cost` 291/200 and 6100/30 (the 1980-02-01 trade falls
after). The strings follow Guile's breadth-first `//` (`node-closure`), so an anchored converted amount is read before
its `<small>` original, and "10. FUNDS " keeps the space inside `<small>` (`:408`); red-ale's checks compare each
`commodities` line's `display` and `convertedDisplay` instead of that order. `test-stress-options.scm` renders both
templates; nothing under `report/test/` names them.

**Hand-worked branches** need a book the test lacks: the test book has no expense accounts, closing transactions,
placeholders, unpriced commodities or trading accounts. A second script, `balsheet_pnl_extras.py`, adds them, trading
accounts in a separate book. Branches: dual columns (the default); the amount staircase with one column; indented
names; multilevel mode; zero-row and omit-zero options in both modes; levels 1–3; "Label sections" off (headers and
rates vanish); "Include totals" off (the L&E total vanishes, Net Income stays); `pricedb-before`; `pricedb-latest` on
the P&L (headers and link dates read today); weighted-average amounts against the rates row; an unpriced commodity
("missing", unconverted); retained earnings with an unpriced income commodity; trading accounts (no unrealized row); a
closing transaction (kept by the balance sheet, left out by the P&L); Net Income; the overall column both orders;
week and two-week steps; a month-end start; start after end; split links; Transaction Report link options with and
without a common currency; chart links; placeholders unlinked; no accounts; both title forms.

## 6. Transaction Breakdown (`standard/txn-columns.scm`)

### 6.1 Inventory and options

One template, "Transaction Breakdown Report", GUID `03603f0443494a6fa790b417f35d90d4`, menu `_Experimental`
(`:254-260`; `report/report-core.scm:162`). 260 lines, GPL-2.0-or-later (`:3-18`). The roadmap's "Transaction
Breakdown" is this report.

| Section | Name | Key | Type | Default | Line |
|---------|------|-----|------|---------|------|
| General | Start Date, End Date | aa, ab | date (`add-date-interval`) | accounting period's start and end | `:44-45` |
| General | Account | b | accountSel, any type, help "Account" | none | `:47-48` |
| General | Description Filter | c | string, help "Description Filter" (neither marked for translation) | "" | `:50-51` |
| General | Limit for number of columns | d | numberRange 1–100 step 1 | 10 | `:53-54` |

The Account option has no default and no allowed types, so its value is null until set
(`5.17:libgnucash/engine/gnc-option-impl.cpp:484-505`): the default report is the "No data" warning.

### 6.2 Computation

1. From = local 00:00:00 of Start Date; to = 23:59:59 of End Date (`:77-85`).
2. With no account, no transactions. Otherwise a split query: non-void splits of the one account (not its
   sub-accounts, `xaccQueryAddSingleAccountMatch`) posted in `[from, to]`, one split per transaction
   (`xaccQueryGetSplitsUniqueTrans`, `5.17:libgnucash/engine/Query.cpp:67-90`), in the query's default order,
   `xaccSplitOrder` (`5.17:libgnucash/engine/Split.cpp:2393`). The transactions kept are those whose description
   contains the filter text, case-sensitively (`string-contains`; empty matches all) (`:98-114`).
3. Columns: every account of every split of those transactions, each once, except TRADING accounts; sorted by
   `xaccAccountOrder`, then stably by the fundamental type in the order ASSET, LIABILITY, EQUITY, EXPENSE, INCOME
   (`xaccAccountTypeGetFundamental`, `5.17:libgnucash/engine/Account.cpp:4483-4513`) (`:116-139`). The selected
   account is among them.
4. No transactions: the empty-data warning (`:144-147`). More accounts than the limit: a generic warning titled "Too
   many accounts", "The number of accounts linked by the transactions found exceeds the limit. Select a different
   subset of transactions, or increase the limit in the options." (`:149-156`). In both cases the document has no
   title.
5. Headers (`:160-174`): *d* header rows, *d* the deepest account's depth (top level 1). Column headers are "Date",
   "Num", "Desc" (untranslated) in the first row, then each account's full name split into its names from the top,
   one per row, blank below a shallower account's last name.
6. One row per transaction (`:192-244`): `qof-print-date` of the posted date, `xaccTransGetNum` (the num field, not
   the split action, whatever the book's num source), the description, then per account column the sum of that
   transaction's splits to the account, value and amount, each negated when the account is a reverse-balanced one
   (`gnc-reverse-balance`); empty when the transaction has no split there. When the account's commodity is the
   transaction's currency the cell is the value in the transaction's currency, linked to the first such split; else
   `[<value in transaction currency>] ` followed by the amount in the account's commodity, the amount linked.
7. A total row (`:179-191`): "Total For " and `gnc-date-interval-format from to` spanning the three text columns, then
   per account the sum of its (signed) amounts in its commodity, `total-number-cell`.
8. Title "Transaction Breakdown Report" (`:246`), the template name and not the "Report name" option.

### 6.3 Library, result fit

Needs: `add_date_interval`, `time64_start_day_time`, `time64_end_day_time`, `make_empty_data_warning`,
`make_generic_warning`, `a.reverse_balance(ctx)`, `ctx.print_date`, `ctx.print_date_interval`, `book.split_query`
with one account and no description (the script filters with `contains` and keeps the first split of each
transaction), the Account `depth`, `parent` and `type`, and an account order. Two gaps:

- **Fundamental type.** `xaccAccountTypeGetFundamental` is not in the query API. It is a fixed table; translate it in
  `lib/accounts` (`account_type_fundamental`), citing `Account.cpp:4483-4513`.
- **Account order.** `xaccAccountOrder` as a comparison: the query API gives display order only through
  `children_sorted`/`descendants_sorted`. The script can sort columns by each account's index in
  `book.root().descendants_sorted()`, which is that order for accounts of the tree; no new read is needed.

Result model:

- **Multi-row column headers** (`gnc:html-table-set-multirow-col-headers!`): add an optional `labelLines` to a
  column (strings or `null`, top to bottom), the same length for every column of a table when present; `label`
  stays, the full name, for clients that read one line. The renderer writes that many header rows. Additive.
- **A link to a split** (`gnc:split-anchor-text`, HU:90-91): add a link kind `{ "split": "<guid>" }`. The Transaction
  Report's translation needs the same, and should own the addition.
- **"[value] amount"**: a `commodities` cell with one line, `monetary` the amount in the account's commodity and
  `converted` the value in the transaction's currency, which is what the pair means; the renderer shows it as its
  nested line. The text form differs from GnuCash's bracket (§12).
- **Dynamic account columns** need nothing new: the script builds `columns` from the accounts it found.

### 6.4 Tests

GnuCash has no test of this report; `test-stress-options.scm` renders it with its defaults (the warning) on an empty
and a populated book (`:258-264`). Hand-worked cases on `test-data.gnucash` (its "Bank" has a multi-split
transaction, foreign transfers and reverse-balanced Income): the default warning; Bank over 1970–2001 with every
column (Bank, Wallet, GBP Bank, Liabilities, Expenses, Income, Income-GBP: the type order and the account order within
a type); the multi-split row; the foreign rows with their `[value] amount` cells; reversed Income and Liabilities
signs; a filter matching some descriptions, and one differing only in case; the limit at 1 (the warning) and at
exactly the number of columns; a two-level header (an account under "Asset"); a void transaction left out; a
transaction with two splits to one account (summed, linked to the first).

## 7. Income/expense vs day of week (`example/daily-reports.scm`)

### 7.1 Inventory and options

Two templates from one list (`:291-310`), menu `E_xamples` (`report/report-core.scm:161`); 310 lines,
GPL-2.0-or-later (`:9-24`):

| Template (name) | Menu name | Menu tip | GUID | Account types |
|-----------------|-----------|----------|------|---------------|
| Income vs. Day of Week | Income vs. Day of Week | "Shows a piechart with the total income for each day of the week" | `5e2d129f28d14df881c3e47e3053f604` | INCOME |
| Expenses vs. Day of Week | Expenses vs. Day of Week | "Shows a piechart with the total expenses for each day of the week" | `dde49fed4ca940959ae7d01b72742530` | EXPENSE |

`milestone5.md`'s row name, "Income/expense vs day of week", is `features.md`'s; the templates keep GnuCash's names.

| Section | Name | Key | Type | Default | Line |
|---------|------|-----|------|---------|------|
| General | Start Date, End Date | aa, ab | date (`add-date-interval`) | accounting period | `:71-73` |
| General | Report's currency | b | currency | report currency setting | `:75-76` |
| General | Price Source | c | multichoice (`add-price-source`) | **weighted-average** | `:78-80` |
| Accounts | Include Sub-Accounts | a | boolean | #t | `:82-84` |
| Accounts | Accounts | a | accountList limited to the template's type | every account of that type, display order | `:86-93` |
| Display | Show Totals | b | boolean, help "Show the total balance in legend?" | #t | `:95-97` |
| Display | Plot Width, Plot Height | da, db | plotSize | 100 % each | `:99-102` |

Default section General (`:104`). "Include Sub-Accounts" and "Accounts" share key `a`: declaration order. The
"Levels of Subaccounts" name is defined (`:58`) and never registered.

### 7.2 Computation (`:113-289`)

1. From = start of day of Start Date, to = end of day of End Date.
2. No accounts selected: the **empty-data** warning (not the no-account one), with the "Report name" option as its
   title (`:283-286`).
3. Commodities: those of the selected accounts and all their descendants other than the report currency,
   whatever "Include Sub-Accounts" says (`:175-177`). Exchange: `gnc:case-exchange-time-fn` of the price source, report
   currency, those commodities and `to` (`:179-182`; CU:829-887), applied at each split's posted date.
4. With "Include Sub-Accounts", the accounts become the selection and all descendants (`gnc-accounts-and-all-descendants`).
   A split query: non-void splits of those accounts posted in `[from, to]` (`:187-213`); its sort order (transaction
   posted date) does not change the sums.
5. Each split's **amount** in its account's commodity is converted at its posted date and added to the total of its
   weekday: `(weekday − 1) mod 7` of `gnc:time64-get-week-day` (local time, 1 = Sunday), so index 0 is Sunday
   (`:220-231`). Totals are exact; no sign is reversed, so income totals are **negative**.
6. Days whose total is exactly zero are dropped; the rest keep Sunday-to-Saturday order (`:235-237`).
7. Nothing left: the empty-data warning (`:278-281`). Otherwise a pie chart (`:251-276`):
   - one series named after the template (`reportname`, not the "Report name" option), the day totals as values,
     colours `gnc:assign-colors` of the number of days;
   - labels the English day names, or with "Show Totals" `"<day> - <gnc:monetary->string total>"`, the exact total
     with the currency's symbol, so an income day reads `Monday - -$109.00`;
   - title two lines: the "Report name" option, then `gnc-date-interval-format from to` and, with "Show Totals",
     `": "` and the sum of all seven totals as `gnc:monetary->string`;
   - width and height from the options; axes hidden.
   The document itself has no title.

### 7.3 Library, result fit

All in v0.2's library: `add_date_interval`, `add_currency`, `add_price_source`, `add_plot_size`,
`accounts_get_commodities`, `accounts_and_all_descendants`, `case_exchange_time_fn` / `apply_exchange_time_fn`,
`book.split_query`, `ctx.local(t).weekday`, `assign_colors`, `make_empty_data_warning`, `ctx.print_date_interval`,
`m.to_string()`. Nothing to add.

The result fits the existing `pie` chart: one series, a colour per value, slice labels. Negative values are sized by
their absolute share, as Chart.js does and red-ale's renderer already does (`reports.md` §8). The result's `title` is
the "Report name" option, as the account pie charts' is (`../reports.md` §14 has that row; §12 here extends it).

### 7.4 Tests

GnuCash's only tests render it: `standard/test/test-charts.scm:71-77` (empty book, defaults) and `:187-190` (book A of
`:59-69`, `:177-185`, with Start Date `start-cal-year`, End Date `end-cal-year`, Accounts Bank and Liability). The last
option is refused by GnuCash's validation, since the accounts are not INCOME or EXPENSE
(`5.17:libgnucash/engine/gnc-option-impl.cpp:385-407`), so the run uses the default accounts; red-ale refuses the
value outright (`report-option`), so the port sets only the dates. Both run as render checks on
`reports/charts-daily.gnucash` (book A with 101 more daily transactions; a render check does not mind). Also in
`test-stress-options.scm`.

Hand-worked cases on `charts-daily.gnucash` (`income` gets −idx on each day from 1970-01-01, `test-charts.scm:197-200`,
so every weekday has a known sum) and on `test-data.gnucash` (Income-GBP for a converted total): each template's
totals by weekday over a fixed range; "Show Totals" off (plain labels, no total in the title); a day with a zero total
left out; Include Sub-Accounts off with a child account; GBP converted under `weighted-average` and `pricedb-nearest`
on each split's date; no accounts and no splits (the two warnings).

## 8. Multicolumn reports (`standard/view-column.scm`)

### 8.1 Inventory and options

One template, "Multicolumn View", menu name "Custom Multicolumn Report", GUID `d8ba4a2e89e8479ca9f6eccdeb164588`,
menu `_Multicolumn` (`:180-189`; `report/report-core.scm:163`). 189 lines, GPL-2.0-or-later. Besides the renderer it
declares an options-cleanup callback (`:170-177`) and an options-changed callback (`:161-168`).

| Section | Name | Key | Type | Default | Line |
|---------|------|-----|------|---------|------|
| `__general` | report-list | `no_key`, help `nodoc_string` | report placement (`GncOptionValue<GncOptionReportPlacementVec>`, UI `REPORT_PLACEMENT`, `5.17:libgnucash/engine/gnc-optiondb.cpp:1101-1112`) | empty | `:42` |
| General | Number of columns | a | numberRange 0–20 step 1 | 1 | `:44-47` |

A placement is `(id, wide, high)` (`5.17:libgnucash/engine/gnc-option.hpp:71-72`): a **report instance id**, its
column span and its row span. Scheme sees each as `(id wide high #f)`, the `#f` a placeholder kept so older versions
load the file (`5.17:bindings/guile/gnc-optiondb.i:304-321`). The options dialog edits it on its own page
(`5.17:gnucash/gnome/dialog-report-column-view.cpp`): "Add" makes a **new instance** of the chosen template or saved
configuration (`gnc:make-report`, `report/report-core.scm:389-409`) and places it 1 × 1 after the selection
(`:421-460`); remove, move up and down, and a size dialog setting both spans, each 0–100
(`:512-563`; `5.17:gnucash/gtkbuilder/dialog-report.glade:5-10`, `:434-439`). The available list is every template,
saved configurations and the Multicolumn View itself included (`gnc:all-report-template-guids`,
`report/report-core.scm:475-476`; dialog `:111-123`).

### 8.2 What a child is in GnuCash

- **An instance, not a reference.** `gnc:make-report` on a saved configuration runs the saved configuration's options
  generator and stores its **copy** of the options in the new instance, of the base template's type, remembering the
  saved configuration's GUID only as `custom-template` (`report/report-core.scm:389-409`). Later changes to the saved
  configuration do not reach the child.
- **Saving a Multicolumn configuration** (`gnc:report-template-serialize-internal`, `report/report-core.scm:620-643`)
  writes, inside `options-gen`: the view's own option forms, including
  `(gnc:option-set-value o '((<id> <wide> <high> #f) …))` for report-list with the session's instance ids
  (`5.17:bindings/guile/gnc-optiondb.i:1314-1320`; `5.17:bindings/guile/options.scm:130-172`); then per child, in
  placement order (`gnc:report-serialize-embedded`, `:567-598`), a
  `(let ((options (gnc:report-template-new-options/report-guid "<base template>" "<name>"))) <option forms>
  (set! new-embedded-report-ids (append new-embedded-report-ids (list (gnc:restore-report-by-guid-with-custom-template
  #f "<base template>" "<name>" "<custom template GUID or \"\">" options)))))` form; then an "Update Section:
  __general" form that replaces each saved id with the new instance's id, in order, keeping the spans (`:600-617`).
  So a saved child is exactly **a base template GUID with the options that differ from its defaults**, its spans,
  and, informationally, the saved configuration it was made from.
- **One level only.** Only the view's direct children are serialized (`:567-598`). A child that is itself a
  Multicolumn View keeps its report-list of stale session ids, so nesting does not survive a save and reload: the
  inner view then finds no report, or another one, by those ids. Nesting in one session works, and no instance can
  contain itself, since every "Add" makes a new instance.
- **Rendering a missing child.** `gnc-report-find` of an unknown id is `#f`, and `gnc:report-set-stylesheet!` on it
  (`:84-85`) is outside the error handler, so the whole view fails. A child whose renderer fails renders in its cell
  as "Report error", "An error occurred while running the report." and the error text in `<pre>` (`:87-97`).

### 8.3 Rendering (`:51-159`)

1. For each placement in order: the child is made to use the **view's style sheet** (`:84-85`); it is rendered
   without HTML headers (`gnc:report-render-html subreport #f`, `report/report-core.scm:764-786`), which still adds the
   child's options summary when its "Add options summary" says so, and its style sheet puts the child's headline or
   title as an `h3` at the top (`report/stylesheets/plain.scm:199-205`).
2. The child's cell has `rowspan` = high and `colspan` = wide, `valign=top`, and holds a two-row table: the child's
   output, then the links "Edit Options" (the options dialog of that instance) and "Single Report" (that instance in
   its own tab) (`:106-134`).
3. The grid (`:99-146`): a table of rows. `allocs[r]` counts the width reserved in row *r*. Placing a child adds its
   colspan to `allocs` of the current row and of each row its rowspan reaches. The child joins the current row and
   the current width grows by its colspan. When the current width reaches "Number of columns" (`>=`), the row is
   closed, the next row begins, and its starting width is `allocs` of that row (space taken by rowspans from above).
   A last unfinished row is closed at the end. The check runs only after a child is placed, so a row whose reserved
   width already fills it still takes one child; GnuCash's HTML then has a row wider than the grid, and red-ale keeps
   the same rows. "Number of columns" 0 behaves as 1.
4. The table is `width=100%` (`:153-155`). The view's document has no title.

The options summary of a view lists `Embedded Report: <child's Report name>` for each child, then the changed options
(`report/html-utilities.scm:324-344`).

### 8.4 Settled: children are an option, and `report.run` takes no new param

Children fit the existing options model as one new option type. `report.run`, `report.export`, `report.options` and
the configuration methods stay as they are; `report.run`'s `options` param carries the children.

**The option type `reportPlacement`** (GnuCash's UI type `REPORT_PLACEMENT`), declared as `o.report_placement(section,
name)`. The template declares it as GnuCash does, section `__general`, name `report-list`, so saved configurations
and the options summary use GnuCash's names. Wire value: an array of children in placement order:

```json
{ "__general": { "report-list": [
    { "report": "6f1e0c9d2b7a4e3f8a5b1c2d3e4f5a6b", "colspan": 1, "rowspan": 2 },
    { "report": "c4173ac99b2b448289bf4d11c731af13",
      "options": { "General": { "Balance Sheet Date": { "relative": "end-prev-year" } } },
      "colspan": 1, "rowspan": 1 } ] } }
```

| Field | Meaning |
|-------|---------|
| `report` | A GUID: a saved configuration (`kind: saved`) or a template (`builtin`, `user`). Not a name: names change and clash, and a reference must stay put |
| `options` | Optional changes over that report's values, as `report.run` takes them: over the template's defaults, or over the saved configuration's values |
| `colspan`, `rowspan` | 0–100, default 1, GnuCash's `wide` and `high` |

- **When set** (`report.run`, `report.export`, `report.saveConfiguration`, `report.updateConfiguration`): the shape
  is checked; a GUID that names no report is `report-option` (`reason` naming the child's position); a child's
  `options` are checked against that report's options, as `report.run` checks them, and a failure is `report-option`
  with the child's position in `reason`. Nothing runs.
- **A saved configuration child is a reference.** It follows the configuration's later changes, where GnuCash copied
  its options at "Add" (§12). A template child with options is GnuCash's copy, made explicit.
- **When it runs**, a child whose saved configuration was deleted since, or whose template no longer loads (a user
  script), renders a warning in its cell (milestone decision 3), titled "Report error" (GnuCash's heading,
  `:95`) with the reason, as a deleted budget does.
- **`report.options`** describes the option with `type: "reportPlacement"`; its `paramsSchema` gives the array's
  schema. A view's default is `[]`.
- **API version.** One new option type in `OptionInfo` and the result additions below: additive, `redAleApi` 0.5.0.

Considered and rejected: a `children` param on `report.run`. Children would then live outside the options a saved
configuration stores, and `saveConfiguration` would need the same param; GnuCash keeps them as an option, and so
does the import.

### 8.5 Running children: the host, not the script

Scripts cannot run other reports (the sandbox, `reports.md` §2). The view's script translates `view-column.scm`: it
declares the two options and computes the grid of §8.3 from the placements, returning a `grid` block whose cells
name children by their index in `report-list`. The **host** then runs each child and places its result in the cell:

- `Registry::run` gains a way to find saved configurations (a lookup the rpc layer builds from the Configuration),
  and runs each child with the view's Book snapshot, `now`, zone, settings and Locale, as a separate engine run with
  its own operation limit. The **run time limit is shared**: one deadline for the whole tree.
- **Style sheet:** the export renders the whole page through the view's style sheet; a child's "Stylesheet" option is
  ignored, as GnuCash overrides it (`:84-85`).
- **Nesting:** a child may be a Multicolumn View (template or saved configuration), as GnuCash's list allows. Proposed
  limits: a depth of 4 views and 100 child runs in one top-level run; past either, and for a cycle (a saved
  configuration already being run further up), the cell holds a warning and nothing more runs there. GnuCash cannot
  form a cycle; red-ale's references can (A names B, B names A).
- **A child that fails** with `report-script`, `report-option` or `report-result` renders a warning in its cell, as
  GnuCash's "Report error" does; `report-limit` stops the whole run, since the limits bound the top-level request.

### 8.6 Result model

A new block kind, additive:

```json
{ "kind": "grid", "columns": 2,
  "rows": [ [ { "rowspan": 2, "colspan": 1, "child": 0,
                "link": { "report": { "template": "…", "configuration": "…", "options": { … } } },
                "result": { "report": { … }, "title": "…", "sections": [ … ], "options": { … }, "log": [] } },
              { "rowspan": 1, "colspan": 1, "child": 1, "warning": { "title": "Report error", "text": "…" } } ] ] }
```

- `columns` is "Number of columns"; `rows` the rows of §8.3, each a list of cells in order.
- `child` (the script sets it) is the index in `report-list`. `result` (*host*) is the child's report result: the
  shape `report.run` returns, without `version` and `now`, which are the view's. `warning` (*host*) replaces it when the
  child could not run.
- `link` (*host*) is GnuCash's "Single Report": a report link that runs the child alone. Today's `ReportLink` has
  `template` and `options`; add an optional `configuration` (a saved configuration's GUID; `options` are then changes
  over it). "Edit Options" has no counterpart: `report.options` does that job.
- The renderer writes a `<table>` of the rows with `rowspan` and `colspan` (`colspan` 0 written as 1, as browsers read
  it), each cell `vertical-align: top`, holding the child's title as a heading and its sections; IDs inside a child
  take the cell's path (`chart-0-0-1-…`) so they stay unique. The host's options summary lists `Embedded Report` rows
  as GnuCash does.

### 8.7 Saved configurations and the import

A saved Multicolumn configuration stores `__general/report-list` like any changed option. Today the importer skips
GnuCash's as `untranslated` (`crates/red-ale-rpc/src/methods/reports/import.rs:222-225`: no template
`d8ba4a2e…`); once the template exists, `options_gen` would refuse them as `malformed`, since the embedded-report
and "Update Section" forms are not option forms (`crates/red-ale-report/src/import/forms.rs:240-249`, `:276-279`),
and `check_bindings` accepts `new-embedded-report-ids` only as `'()` there, which is what GnuCash writes.

Proposed: `import/forms.rs` matches the two extra forms exactly as `report-core.scm:586-617` writes them, never
evaluating them. Each embedded form becomes a child `{ report: <base template GUID>, options: <its option forms>,
colspan, rowspan }`, its spans from the report-list literal at the same position. The custom template GUID is dropped
(the child's options are already complete), and listed in `ignored`. An entry whose child template red-ale has not
translated is skipped as `untranslated`, naming the child, so importing again later works. An entry whose report-list
and embedded forms differ in number is `malformed`.

### 8.8 Tests

GnuCash has no test of the view's output; `test-stress-options.scm:8` loads it and renders it with no children
(`:117-154`). Hand-worked cases: no children (an empty grid, the title); three children in two columns, one with
rowspan 2 (the row membership of §8.3, including the row that takes a child beyond its width); a saved configuration
child and a template-with-options child; "Number of columns" 0 and a colspan wider than the grid; a deleted saved
configuration, an unknown template and a failing child (warnings in their cells); a nested view; a cycle and the depth
limit; the options summary; the child's style sheet ignored; `report.saveConfiguration` and import of a hand-written
`saved-reports-2.8` entry in the 5.17 form (`fixtures/gnucash-userdata/`), each child's options equal to the same set
by hand; deterministic export bytes.

---

## 9. What red-ale's report library lacks

Most of what these reports call is in v0.2's library (§12 of [`../reports.md`](../reports.md)): collectors, the
exchange cases, balances, the split query, the account table, the OU helpers, date lists, the rates table, the
warnings. The gaps:

| Need | Used by | GnuCash | Where |
|------|---------|---------|-------|
| `time64_previous_day`, `time64_next_day` | Trial Balance, Equity Statement | DU:495-499 | `lib/dates` |
| `account_type_fundamental` | Transaction Breakdown | `5.17:libgnucash/engine/Account.cpp:4483-4513` | `lib/accounts` |
| `cash_flow_calc_money_in_out` | Cash Flow, Cash flow chart | `5.17:gnucash/report/reports/cash-flow-calc.scm:33-106` | new `lib/cash-flow-calc` |
| A report link that copies the options its target declares (`gnc:make-report-anchor`) | Cash flow chart, both Multicolumn statements (category-barchart has a private one) | HU:168-185; `5.17:bindings/guile/options.scm:112-123` | `lib/html-utilities` |
| The multicolumn account walker, `book-main-currency` | Multicolumn statements | `standard/balsheet-pnl.scm:279-607`, `:804-812` | the report's script |
| Per-row cell arrays (`gnc:html-acct-table-get-cell`, `-set-cell!`) | Trial Balance | HAT:830-841 | the report's script |
| The `reportPlacement` option type and its builder `o.report_placement` | Multicolumn View | `5.17:libgnucash/engine/gnc-optiondb.cpp:1101-1112` | `options` (Rust) |
| Running child reports: a saved-configuration lookup in the run context, a shared deadline, depth, count and cycle limits | Multicolumn View | `standard/view-column.scm:66-147` | `engine` (Rust) |
| Importing embedded reports from `saved-reports-2.8` | Multicolumn View | `report/report-core.scm:567-617` | `import/forms.rs`, rpc import |

No new query function and no new core read is needed: every Book read is one the query API has.

## 10. What the report result needs

All additive within `redAleApi` 0.x; together with the new option type they make `redAleApi` 0.5.0.

| Addition | Used by | GnuCash |
|----------|---------|---------|
| Link kind `{ "split": "<guid>" }` | Multicolumn Balance Sheet, Transaction Breakdown, and the Transaction Report (whose note should own it) | `gnc:split-anchor-text`, HU:90-91 |
| Optional `link` on a text block | Multicolumn statements' "Barchart" | `standard/balsheet-pnl.scm:1079-1083`, `:1222-1226` |
| Optional `until` on a `date` cell (host fills `localUntil`, `display` `<from> to <until>`) | Multicolumn P&L headers | `standard/balsheet-pnl.scm:1140-1153` |
| Optional `labelLines` on a column (multi-row headers) | Transaction Breakdown | `gnc:html-table-set-multirow-col-headers!`, `standard/txn-columns.scm:170-174` |
| A `list` block, `items: [{ text, account? }]` | Cash Flow's "Selected Accounts" (and category-barchart's list) | `report/html-text.scm:210-215` |
| A `grid` block of child report results; optional `configuration` on `ReportLink` | Multicolumn View | `standard/view-column.scm:51-159` |

Everything else (the Trial Balance's columns, the Equity Statement's staircase, the charts, `commodities` cells for
converted amounts and the Transaction Breakdown's `[value] amount`) fits today.

## 11. Fixtures

| Fixture | Script | Rebuilt from | Used by |
|---------|--------|--------------|---------|
| `reports/adjusting-entries.gnucash`, `-fx` | `adjusting_entries.py` (new) | `create-test-data`, `#`, and `standard/test/test-trial-balance.scm:69-76` (= `test-equity-statement.scm:68-75`); `-fx` adds `test-trial-balance.scm:170-173` | Trial Balance, Equity Statement |
| `reports/cash-flow.gnucash` | `cash_flow.py` (new) | `standard/test/test-cash-flow.scm:14-144`, now pinned | Cash Flow |
| `reports/cashflow-barchart.gnucash` | `cash_flow.py` | `standard/test/test-cashflow-barchart.scm:51-195`, now pinned | Cash flow chart |
| `reports/balsheet-pnl.gnucash` (exists) | `balsheet_pnl.py` | `standard/test/test-balsheet-pnl.scm:51-166` | Multicolumn statements (checks already in its `expected.json`) |
| `reports/balsheet-pnl-extras.gnucash`, `-trading` | `balsheet_pnl_extras.py` (new) | none: expense accounts, a closing transaction, a placeholder, an unpriced commodity; trading accounts | Multicolumn statements, hand-worked |
| `reports/test-data.gnucash`, `charts-daily.gnucash` (exist) | | | Transaction Breakdown, day-of-week reports, Cash Flow (hand-worked); render checks |

All are built through the 5.17 Python bindings under `TZ=UTC`, with the conventions of [`../reports.md`](../reports.md)
§13; none needs Guile or `ctypes`. `test-cash-flow.scm`, `test-balsheet-pnl.scm` have no licence notice and are cited
under GnuCash's licence; the others keep theirs. The Multicolumn View needs no book of its own: its checks run
children on existing fixtures.

## 12. Floating point, locale and deviations

**Floating point.** None of these reports computes in doubles. The chart values of the Cash flow chart and the
day-of-week pies are JSON doubles in GnuCash: add both to the existing "Chart values" row. No new row.

**Locale.** Name and commodity sorting use locale collation (the existing bytewise row); description patterns use
POSIX regular expressions and case folding (the existing rows); the default patterns "Adjusting Entries" and "Closing
Entries", the Cash Flow's "~a and subaccounts" and the day names are gettext strings (the existing English row). The
Transaction Breakdown's "Date", "Num", "Desc" and "Description Filter" are untranslated in GnuCash too.

**Proposed rows for [`../reports.md`](../reports.md) §14:**

| What | GnuCash 5.17 | red-ale | Why |
|------|--------------|---------|-----|
| Trial Balance work sheet: Net Income/Loss figure | In the Adjustments column, by `add-labeled-amount-line`'s padding (HAT:899-901), not the Income Statement column `standard/trial-balance.scm:825` names | In the Income Statement column | The result names columns; GnuCash's position is an HTML artifact |
| Trial Balance amounts with two or more account columns | Shifted left of their Debit/Credit headers by the same padding | In their columns | The same |
| Trial Balance with only TRADING accounts selected | Fails at `standard/trial-balance.scm:523` | `report-script` error saying no non-trading account is selected | Fails as GnuCash does, with a reason |
| Multicolumn statements' exchange rates | All rates of a column in one title-row cell (`standard/balsheet-pnl.scm:770-797`) | One row per commodity | A cell holds one content; the figures are the same |
| Multicolumn links' main currency on a tie | Hash order (`standard/balsheet-pnl.scm:804-812`) | The first in the accounts option's order | Determinism |
| Cash Flow's "Selected Accounts" | A `<ul>` of register links (`standard/cash-flow.scm:253-262`) | A `list` block | The result carries data and keeps the links |
| The cash-flow calculation's null-account warning | Printed to standard output (`cash-flow-calc.scm:65-67`) | The report log | Scripts have no standard output |
| Transaction Breakdown's `[value] amount` | One cell of text and a link (`standard/txn-columns.scm:228-235`) | A `commodities` cell: the amount, converted = the value | The pair is a monetary and its value |
| Untitled documents: Transaction Breakdown's warnings, the day-of-week pies, the Multicolumn View | Untitled (`standard/txn-columns.scm:246`; `example/daily-reports.scm:144`; `standard/view-column.scm:52`) | Titled with the "Report name" option | Extend the existing untitled-documents row |
| Unordered sets: Cash Flow's and the day-of-week reports' `gnc-accounts-and-all-descendants` | Hash order | Fixed order | Extend the existing row |
| Multicolumn: a saved configuration child | Its options copied when added (`report/report-core.scm:389-409`) | A reference that follows the configuration's later changes | Milestone decision 3 |
| Multicolumn: a missing child | The whole view fails (`standard/view-column.scm:75-85`) | A warning in its cell | Milestone decision 3 |
| Multicolumn: nesting | One level survives saving (`report/report-core.scm:567-598`) | Saved views nest to the depth limit; cycles render a warning | References make nesting well defined |
| Multicolumn: "Edit Options" and "Single Report" links | Anchors to the child instance | A report link to run the child alone; no options link | `report.options` does that job |

Kept as GnuCash does, each flagged in its script with the line: the inverted trading option; the closing-loss
withdrawal; withdrawals under the transaction currency; the Multicolumn P&L's `pricedb-latest` headers and link dates;
the two "weighted averages" and two average-cost dates; report names copied into linked reports; negative income in the
day-of-week pies; `test-trial-balance.scm`'s "= $401" names.

## 13. Proposed tickets

Each ticket is at most about 500 lines of Rust and Rhai, tests and docs not counted. Fixture scripts are Python and
are their own tickets. Order follows `milestone5.md`'s "Order of work".

| # | Ticket | Size | Depends on | Shares |
|---|--------|------|------------|--------|
| 1 | Result additions: `split` link, text-block `link`, date-cell `until`, column `labelLines`, `list` block; schema, validation, renderer | ~250 Rust | — | the `split` link with the Transaction Report note: whichever lands first owns it |
| 2 | Fixtures: `adjusting_entries.py`, `cash_flow.py`, `balsheet_pnl_extras.py`, their books and `expected.json` | ~350 Python | — | |
| 3 | `lib/dates` previous/next day; Equity Statement and its checks | ~250 Rhai | 2 | `lib/dates` with 4 |
| 4 | Trial Balance: options, `current` and `pre-adj`, unrealized row, rates, its checks | ~320 Rhai | 2, 3 | |
| 5 | Trial Balance work sheet, and hand-worked cases | ~130 Rhai | 4 | |
| 6 | `lib/cash-flow-calc` and the Cash Flow | ~330 Rhai | 1 (`list`), 2 | the calculation with 8 |
| 7 | Multicolumn statements: options and the walker | ~330 Rhai | 1 | |
| 8 | Report-link helper in `lib/html-utilities`; Cash flow chart | ~250 Rhai | 2, 6 | the helper with 9 (and category-barchart, moved later) |
| 9 | Multicolumn statements: renderer core and the P&L, its checks | ~300 Rhai | 7, 8 | |
| 10 | Multicolumn Balance Sheet, its checks, then the hand-worked cases on the extras books | ~200 Rhai | 9 | |
| 11 | Transaction Breakdown, `account_type_fundamental` | ~220 Rhai | 1 (`split`, `labelLines`) | |
| 12 | Income and Expenses vs. Day of Week | ~150 Rhai | — | |
| 13 | Multicolumn View A: the `reportPlacement` option type (declaration, wire value, validation of child references and their options, `report.options` schema, options summary rows) | ~300 Rust | — | |
| 14 | Multicolumn View B: the `grid` block and its renderer, the engine's child runs (lookup, shared deadline, limits, cycle), `ReportLink.configuration`, `standard/view-column.rhai` | ~450 Rust + ~80 Rhai | 13 | |
| 15 | Multicolumn View C: importing embedded reports | ~200 Rust | 14 | |

Statements first (3–10), then the Transaction Breakdown (11), the charts (8's chart, 12), the Multicolumn View last
(13–15), once there are many reports to combine. Ticket 1 goes first because 6, 7 and 11 build on it. `redAleApi`
moves to 0.5.0 with ticket 1, and `api_additive.rs` checks it against 0.4.0.

## Open questions

1. **The Cash Flow's inverted trading option** (§3.1). *Proposed:* match 5.17 and say so in the option's help, since
   fixing it changes figures; revisit if GnuCash fixes it.
2. **The Trial Balance's misplaced Net Income** (§2.3). *Proposed:* red-ale puts it in the Income Statement column the
   Scheme names, and the ported check reads it there, recorded in §14. The alternative copies an HTML artifact into
   the result.
3. **Only TRADING accounts selected in the Trial Balance.** *Proposed:* `report-script` with a clear message, as
   GnuCash fails; the no-account warning would be friendlier but hides that the selection had accounts.
4. **Multicolumn children that are saved configurations follow later changes** (§8.4), where GnuCash copied them.
   *Proposed:* keep milestone decision 3's reference and record the deviation; imported children are template plus
   options, GnuCash's copy.
5. **Importing a Multicolumn entry with an untranslated child.** *Proposed:* skip it as `untranslated`, naming the
   child, so a later import adds it whole. The alternative imports it without that child.
6. **Nesting limits.** *Proposed:* views nest to depth 4, at most 100 child runs per top-level run, one shared
   deadline; a cycle or a limit renders a warning in the cell.
7. **Checking child references when set.** *Proposed:* refuse an unknown GUID and invalid child options with
   `report-option` when given; a child deleted later renders a warning when run.
8. **Name of the Multicolumn P&L.** GnuCash calls it "Income Statement (Multicolumn)". *Proposed:* use that name in
   `milestone5.md`, `features.md` and the glossary, keeping "Multicolumn Balance Sheet and P&L" only as the feature
   row.
9. **A `list` block or a table** for the Cash Flow's "Selected Accounts". *Proposed:* the `list` block, which
   category-barchart's negative-accounts text can also use and which keeps the account links.
10. **Spans of 0** in a placement. *Proposed:* accept 0–100 as GnuCash stores them and lay them out as GnuCash does;
    the renderer writes a column span of 0 as 1.
11. **The Equity Statement's unused "Show Foreign Currencies".** *Proposed:* declare it, so saved configurations and
    the import keep it, and read it nowhere, as GnuCash.
