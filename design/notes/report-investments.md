# Report note: the investment reports (GnuCash 5.17)

Scope: the four §8 investment reports v0.5 translates (Investment Portfolio, Advanced Portfolio, Investment Lots, IFRS
cost basis), and the three reports GnuCash 5.17 ships that [`../features.md`](../features.md) has no row for (Lot
Viewer, dashboard, Balance Sheet using eguile). It settles [`../milestone5.md`](../milestone5.md), "Decide before
code", items 6 (investment arithmetic: core or script) and 7 (the missing rows).

Citations are `5.17:<path>:<line>`; a bare `:<line>` right after one refers to the same file. As in
[`../reports.md`](../reports.md), `standard/` stands for `5.17:gnucash/report/reports/standard/`, `report/` for
`5.17:gnucash/report/`, RU for `report/report-utilities.scm`, CU for `report/commodity-utilities.scm`, and TEE for
`5.17:bindings/guile/test/test-engine-extras.scm`. Figures quoted as GnuCash's are the expected strings of its own
tests; nothing was run through Guile. Figures marked *worked* were derived from the Scheme by hand.

---

## Facts that surprise

1. **Advanced Portfolio never reads lots.** Its FIFO and LIFO run over a list of `(units . price)` pairs built from the
   account's splits in split order; its average basis has no engine counterpart (`standard/advanced-portfolio.scm:202-319`).
2. **Advanced Portfolio rounds on purpose, and rationally.** Units and unit prices go to 10⁻⁸ with banker's rounding
   (`:57-59`), money to the currency's SCU, at about forty fixed points. These are `gnc_numeric` conversions, not
   floats; red-ale reproduces them exactly. Only the two rate columns use doubles.
3. **Rates print half away from zero.** GnuCash's test expects `-0.63%` for −5/800 and `-0.13%` for −1/800
   (`standard/test/test-portfolios.scm:89-90`): exact ties, which banker's rounding would fail.
4. **IFRS cost basis has no floating point.** All its sums are exact Scheme arithmetic; the only rounding is the
   exchange rate, read as 10,000,000 units converted at the currency's SCU (`standard/ifrs-cost-basis.scm:317-320`).
   The milestone's "Exact figures" bullet is wrong to name it.
5. **IFRS's account options default to the first account of their types.** GnuCash's test never sets Stock, Dividend
   or Cap Gains Account (`standard/test/test-ifrs-cost-basis.scm:251-259`): Stock resolves to SPY, both income
   options to the parent "Income" (`5.17:libgnucash/engine/gnc-option-impl.cpp:491-521`). Capital gains are matched
   before dividends (`standard/ifrs-cost-basis.scm:261-273`), so two defaults naming one account count everything
   there as capital gains.
6. **Investment Lots does its own basis arithmetic.** It reads the engine's lots only for membership and titles, uses
   raw (not adjusted) amounts, and treats a forward stock split as a purchase (`standard/investment-lots.scm:528-536`).
7. **"Most recent" price means "at or before now"** in the any-currency lookups (`gnc-pricedb-lookup-latest-any-currency`,
   `5.17:libgnucash/engine/gnc-pricedb.cpp:1955-1960`), while `gnc:case-exchange-fn`'s latest conversion may take a
   future price (`../reports.md` §5).
8. **Windows are odd.** IFRS shows rows with `start < posted < end`, both bounds at the start of their day, so the End
   Date's own transactions are out (`standard/ifrs-cost-basis.scm:299-304`, `:468-469`). Investment Lots starts its
   window at the *end* of the Start date's day, so that day's trades are out (`standard/investment-lots.scm:667-672`).

---

## 1. Inventory

| File | Lines | Template | GUID | Menu | Introduced | Licence |
|------|------:|----------|------|------|:----------:|---------|
| `standard/portfolio.scm` | 292 | Investment Portfolio | `4a6b82e8678c4f3d9e85d9f09634ca89` (`:286-292`) | Assets & Liabilities | 1.6 | GPL-2.0-or-later (`:5-8`) |
| `standard/advanced-portfolio.scm` | 1,190 | Advanced Portfolio | `21d7cfc59fc74f22887596ebde7e462d` (`:1184-1190`) | Assets & Liabilities | 1.8 | GPL-2.0-or-later (`:10-12`) |
| `standard/investment-lots.scm` | 2,326 | Investment Lots | `ab2acc24afd14630a551f98f1a35fa81` (`:2321-2326`) | Assets & Liabilities | 5.0 | GPL-2.0-or-later, Brent McBride 2022 (`:1-23`) |
| `standard/ifrs-cost-basis.scm` | 533 | IFRS weighted-average cost basis report | `15d5b744176c4625a703720338725291` (`:527-533`) | Experimental | 4.6 | GPL-2.0-or-later (`:3-6`) |
| `standard/lot-viewer.scm` | 206 | Lot Viewer | `b64b8cbaa633472c93ab7d9a2424d157` (`:199-206`) | Experimental, only with `--extra` (`:205`) | 3.9 | GPL-2.0-or-later (`:3-6`) |
| `standard/dashboard.scm` | 65 | none: `gnc:make-dashboard` builds a Multicolumn report | menu item `ad80271c890b11dfa79f2dcedfd72085` (`5.17:gnucash/gnome/report-menus.scm:113-121`) | Multicolumn | 4.0 | GPL-2.0-or-later |
| `standard/balsheet-eg.scm` | 559 | Balance Sheet (eguile) | `2e3751edeb7544e8a20fd19e9d08bb65` (`:550-558`) | Assets & Liabilities ("Balance Sheet using eguile-gnc") | 2.4 | GPL-2.0-or-later (`:10-12`) |

Menus: `report/report-core.scm:157`, `:162`. "Introduced" is the first stable tag containing the file's first commit:
lot-viewer `2adaf692b8` (3.9), dashboard `da4d72f89b` (3.901 → 4.0), balsheet-eg `f4f35770cb` (2.3.12 → 2.4),
investment-lots `ea026ff4ee` (4.900 → 5.0), ifrs-cost-basis `3168347ab7` (4.6). The IFRS template name is not marked
for translation (`:52`).

---

## 2. Investment Portfolio (`standard/portfolio.scm`)

### 2.1 Options (`:39-76`)

| Section | Name | Key | Type | Default | Notes |
|---------|------|-----|------|---------|-------|
| General | Date | a | date (end) | end of the accounting period | `report/options-utilities.scm:48-51` |
| General | Report's currency | c | currency | report currency | `:124-130` |
| General | Price Source | d | multichoice | `pricedb-latest` | `average-cost`, `weighted-average`, `pricedb-before`, `pricedb-nearest`, `pricedb-latest`, labels as `:133-144` |
| General | Share decimal places | e | number range | 2, 0-9 step 1 | a double; `inexact->exact` (`:99`) |
| Accounts | Accounts | b | account list | STOCK and MUTUAL descendants, display order; allowed ASSET, BANK, STOCK, MUTUAL | `:61-68` |
| Accounts | Include accounts with no shares | e | boolean | #f | `:70-73` |

Default section General.

### 2.2 Computation (`:84-284`)

1. `to-date` = end of day of Date. Title: Report name + `" "` + `qof-print-date to-date` (`:160-180`).
2. No accounts: the no-account warning only (`:276-281`).
3. `commodity-list` and `pricedb` (`:184-187`) are dead code.
4. `exchange-fn` = `gnc:case-exchange-fn price-source currency to-date` (`:188`; `lib/exchange::case_exchange_fn`).
5. `price-fn` (`:189-240`) returns `(price-object . value)`:
   - average-cost, weighted-average: `(#f . r)`, r = 10,000 units exchanged to the report currency ÷ 10,000 to
     **5 significant figures**, banker's (`:191-200`);
   - pricedb-latest: first of the latest-any-currency list, value inverted when the price's commodity is not the stock
     (`:201-218`); none → `(#f . 0)`;
   - pricedb-before, pricedb-nearest: first of the nearest-before / nearest-in-time list at `time64CanonicalDayTime`
     (local noon, `5.17:libgnucash/engine/gnc-date.cpp:404-410`), **not inverted** (`:219-240`).
6. Per account, in option order: units = own balance as of `to-date` (RU:516-529); value = `exchange-fn` of the
   units. Shown when "Include accounts with no shares" or units ≠ 0 (`:128`); then added to the total.
7. Price cell: a monetary of the **original** price object's currency and value, even when step 5 inverted it
   (`:116-119`); else the report currency and r. Linked to the price (`report/html-utilities.scm:217-224`).

### 2.3 Rows

Columns Account, Symbol, Listing, Units, Price, Value (`:242-249`). One row per shown account (alternating from
`normal-row`): account link, mnemonic, namespace, units (share print info), Price, Value. Then a rule over six
columns, then a "Total" row per collector commodity (the report currency), the figure spanning five columns
(`:255-273`). With no row shown there is no Total row; the rule stays.

### 2.4 Rounding

| Where | GnuCash | red-ale |
|-------|---------|---------|
| Average-cost and weighted-average price | exchange (rounded to SCU) of 10,000 units ÷ 10,000 to 5 significant figures, banker's (`:193-200`); `sigfigs_denom` gives an exact power of ten below 1 one more figure (`5.17:libgnucash/engine/gnc-numeric.cpp:468-486`) | `r.round_sigfigs(5, "round")` reproducing `sigfigs_denom` |
| Units | share print info: exactly n places, **half away from zero**, grouped, point always printed (`5.17:libgnucash/app-utils/gnc-ui-util.cpp:1013-1023`, `:1105-1145`) | the share display of §8 |
| Price, Value | monetary renderer: SCU, banker's (`report/html-style-info.scm:224-231`) | monetary cells |

---

## 3. Advanced Portfolio (`standard/advanced-portfolio.scm`)

### 3.1 Options (`:61-140`)

| Section | Name | Key | Type | Default | Choices |
|---------|------|-----|------|---------|---------|
| General | Date | a | date (end) | end of the accounting period | |
| General | Report's currency | c | currency | report currency | |
| General | Price Source | d | multichoice | `pricedb-nearest` | `pricedb-latest` "Most recent", `pricedb-nearest` "Nearest to report date" |
| General | Basis calculation method | e | multichoice | `average-basis` | `average-basis` "Average cost of all shares", `fifo-basis` "First-in first-out", `filo-basis` "Last-in first-out" |
| General | Set preference for price list data | f | boolean | #t | |
| General | How to report brokerage fees | g | multichoice | `include-in-basis` | `include-in-basis`, `include-in-gain`, `ignore-brokerage` ("Omit from report") |
| Display | Show ticker symbols / Show listings / Show number of shares | a / b / c | boolean | #t | |
| Display | Share decimal places | d | number range | 2, 0-9 | |
| Display | Show prices | e | boolean | #t | |
| Accounts | Accounts | b | account list | `gnc:account-is-stock?` descendants (STOCK, MUTUAL; RU:154-156); allowed ASSET, BANK, STOCK, MUTUAL | |
| Accounts | Include accounts with no shares | e | boolean | #f | |

A `pricedb-before` branch (`:1017-1021`) is unreachable: dead code.

### 3.2 The price (`:393-490`)

1. `find-price` over the any-currency list (latest: at now; nearest: at local noon of `to-date`) (`:324-336`): start
   from the first; a price in the report currency replaces it, one whose commodity is the report currency replaces it
   inverted; the last match wins.
2. Dropped if 100 units of its currency convert to 0 (`:440-446`).
3. No price, or the preference off: the newest cross-commodity split of the account up to today (latest) or
   `to-date` (nearest) with non-zero amount and value whose share price converts to non-zero gives the price
   (monetary, report currency) and `pricing-txn`; `use-txn` set (`:450-480`).
4. Still none: 1 in the report currency, `use-txn` set, no `pricing-txn` (`:483-490`).
5. `my-exchange-fn` (`:411-430`): a stock monetary into the report currency is units × price → report SCU (banker's),
   then `exchange-fn`; anything else goes to `exchange-fn`.

### 3.3 The transaction walk (`:498-776`)

Every split of the account in split order, transaction posted ≤ `to-date`, each transaction once. `cf` = transaction
currency SCU, `rf` = report currency SCU, U = 10⁻⁸; "→" is a banker's rounding.

**Pass 1** (`:531-596`), first match wins:

| Split | Effect |
|-------|--------|
| EXPENSE account | `brokerage += value` → cf, unless its other split (`xaccSplitGetOtherSplit`, `5.17:libgnucash/engine/Split.cpp:2161-2185`) is in this account |
| INCOME account | `income −= value` → cf |
| this account | `shares += |amount|` → U. Amount 0: a spin-off (§3.5) with negative value adds `−value` to `spinoff`; else a non-zero value is a gains split and `income −= value`. Amount ≠ 0: value > 0 → `bought`, `shares-bought`; else `sold −= value`, `shares-sold += amount` (so a zero-value stock split counts as sold shares) |
| type exactly ASSET | reinvestment holding: the first sets `drp-account` (its value as residual) if in the transaction currency, else `'none`; later ones add if parent, child or sibling (`:339-344`), else `'none` |

**Then** (`:619-690`): income and brokerage go to the account's collectors; unless fees are ignored and with brokerage
and shares > 0, `bought += brokerage × shares-bought/shares`, `sold += brokerage × shares-sold/shares` (negative,
so fees reduce `sold`), → cf; reinvestment (income > 0, bought > 0) fixes or extends the holding account and amount,
or resets them (`:645-661`), then `bought −= income`, `+= residual`, `−= holding` (→ cf), a negative holding absorbs
`bought`, and `bought ≥ 0` (`:666-683`); Money In += bought, Money Out += sold + spinoff.

**Pass 2** (`:693-770`): a split in this account with amount ≠ 0: `v` = `my-exchange-fn(value)`; `orig` = basis
(→ rf); its fees = brokerage × |amount|/shares → cf, converted; with `include-in-basis`, `v += fees` → rf; the basis
list takes `(amount, v)` (§3.4); if value ≤ 0 and the new basis is 0, or value < 0, realized gain += `|v| − (orig −
new)` → rf (`:736-752`). A spin-off (`:350-356`: amount 0, two-split transaction, other split not INCOME/EXPENSE): the
list takes `(0, my-exchange-fn(value))`.

### 3.4 The basis list (`:169-319`)

`sum-basis` = Σ units × price, each → rf, summed → rf (exact when called with `GNC-DENOM-AUTO`, `:304`).

| Case | Average | FIFO / LIFO |
|------|---------|-------------|
| value ≠ 0, units > 0 | one pair: units → U; price = (value + held × price)/(units + held) → U, exact parts; zero divisor throws `div/0` "buying ~0,4f share units" | append `(units . value/units → U)` |
| value ≠ 0, units < 0 | units → U, price kept | from the first (FIFO) or last (LIFO) pair: shrink, drop, or drop and recurse |
| value = 0, units ≠ 0 (split, merger) | ratio = (units + held)/held exact (0 if nothing held); units × ratio → U, price × 1/ratio → U | same |
| units = 0, value ≠ 0 (spin-off) | ratio = (value + basis)/basis exact (`div/0` "spinoff of ~,2f currency units" at 0); prices × ratio → U | same |

### 3.5 Income and fees outside the account (`:778-850`)

If the parent is ASSET or BANK in a currency: every parent split ≤ `to-date` whose other split's account has the same
leaf name and a currency adds INCOME → value to dividends, EXPENSE → −value to brokerage, under that account's
commodity (`:821-829`). With `include-in-gain`, brokerage is then subtracted from realized gains (`:849-850`).

### 3.6 Row, totals, warnings (`:852-1173`)

Collectors sum into the report currency with `my-exchange-fn`. Columns: Account; Symbol, Listing, Shares, Price (if
shown; Price from the price list via `gnc:default-price-renderer` with a price link, a transaction price as a monetary
linked to `pricing-txn`, the bogus 1 as a monetary); a marker (`*` transaction price, `**` bogus, `" "`); Basis;
Value; Money In; Money Out; Realized Gain; Unrealized Gain (`my-exchange-fn(value) − basis` → rf); Total Gain
(→ rf); Rate of Gain (`"~,2f%"` of 100 × total gain / money in in doubles, `""` when money in is 0); Income; Brokerage
Fees (unless ignored); Total Return (→ rf); Rate of Return.

Totals (`:880-887`, `:1089-1160`): a rule spanning 17 columns, then "Total", `" "` for each shown optional column and
the marker, then sums through `exchange-fn` (not `my-exchange-fn`) and the two rates. Notes after the table: "* this
commodity data was built using transaction pricing instead of the price list." `<br>` "If you are in a multi-currency
situation, the exchanges may not be correct."; "** this commodity has no price and a price of 1 has been used.",
after a `<br>` if both (`:1163-1172`). A `div/0` adds the `OVERFLOW-ERROR` text (`:51-55`) before the table; rows and
totals so far are kept (`:1076-1086`).

### 3.7 GnuCash's test, worked

`create-stock-test-data` (§9.1), defaults:

| Transaction | Basis list after | Realized | In / Out | Income, fees |
|-------------|------------------|---------:|----------|--------------|
| 1980-02-01 buy 1 for 100 | (1 . 100) | | in 100 | |
| 1980-03-01 buy 1 for 200 | (2 . 150) | | in 200 | |
| 1980-05-01 sell 1 for 400, fee 10, gains split 300 | (1 . 150) | 390 − 150 = 240 | out 390 | income 0; fees 10 |
| 1980-10-01, 11-01 splits +9, +90 | (10 . 15), (100 . 1.5) | | | |
| 1980-12-01 split −67, cash in lieu 4 | (33 . 4.54545455) | | | income 4 |
| 1981-01-01 buy 10 for 500 | (43 . 15.11627907) | | in 500 | |
| 1981-03-01 sell −1/2 (stored −1: SCU 1, half up) for 3 | (42 . 15.11627907) | 3 − (650.00 − 634.88) = −12.12 | out 3 | |
| 1981-04-01 spin-off 150 | (42 . 11.54485050) | | out 150 | |

Basis $484.88, value $252.00, unrealized −$232.88, realized $227.88, total −$5.00 (`-0.63%`), income $4.00, fees
$10.00, total return −$1.00 (`-0.13%`) (`standard/test/test-portfolios.scm:88-90`).

### 3.8 Rounding

All rational, reproducible with `round_to(denom, "round")`: → U (`:182`, `:191-192`, `:216-235`, `:246-274`,
`:553-578`); → cf (`:546-592`, `:630-640`, `:655-680`, `:719-720`); → rf (`:172-173`, `:424-428`, `:724-725`,
`:745-748`, `:859-868`, `:1093-1099`); exact ratios (`:219-225`, `:284-290`, `:307-308`, `:630`, `:638`,
`:714-715`). Past 64 bits `gnc_numeric` rounds to fit (`5.17:libgnucash/engine/gnc-numeric.cpp:71-85`) or errors to #f
(`5.17:bindings/guile/gnc-engine-guile.cpp:1729-1734`). Doubles: the rate cells (`:914-922`, `:929-936`,
`:1126-1133`, `:1145-1152`) and the two `div/0` messages (`:227`, `:306`).

---

## 4. Investment Lots (`standard/investment-lots.scm`)

The test is `standard/test/test-investment-lots.scm` (836 lines, no licence header). Dead code, left out: `is-long-term?`
(`:488-497`, 365.25-day years) and `calculate-gain` (`:466-469`), which only the test calls (`:20-25`), and the
`gnc:debug` double formatting.

### 4.1 Options

| Section | Name | Key | Type | Default | Notes |
|---|---|---|---|---|---|
| Accounts | Accounts | a | account list, ASSET BANK STOCK MUTUAL | STOCK and MUTUAL descendants (`:145-154`) | |
| Accounts | Include accounts with no shares | b | boolean | #t | off: zero balance at End date left out (`:2135-2145`) |
| Accounts | Include accounts with no lots | c | boolean | #f | (`:2150-2152`) |
| Chart | Show Chart | a | complex boolean | #t | enables the next four (`:171-183`) |
| Chart | Chart type | b | multichoice | `bar-stacked` | `bar`, `bar-stacked` |
| Chart | Chart location | c | multichoice | `top` | `top`, `bottom` |
| Chart | Plot width / Plot height | da / db | plot size | 100 % / 50 % (`:203-210`) | |
| Columns | Show lot GUID column / date / bought / sold / end columns | a / b / c / d / e | boolean | #f / #t / #t / #t / #t | |
| Columns | Show realized gain columns / Show unrealized gain columns | f / g | complex boolean | #t | ROI and CAGR selectable while either is on (`:249-270`) |
| Columns | Group gains by age (short term and long term) | h | multichoice with callback | `gains-only` | `no`, `gains-only`, `gains-and-sales` (`:272-283`, `:684-689`) |
| Columns | Long term gains age (years) | i | number range | 1, 0-10E9 step 1 | (`:289-298`) |
| Columns | Show ROI columns / Show CAGR columns | j / k | boolean | #f / #t | |
| Display | Show long account names | a | boolean | #t | |
| Display | Show mnemonic in amounts | b | boolean | #t | amounts only (`:808-812`) |
| Display | Include closed lots | c | boolean | #t | off: end amount 0 left out (`:2203-2206`) |
| Display | Show blanks instead of zeros in table cells | d | boolean | #t | not in total rows (`:1145-1153`) |
| Display | Show lot split rows | e | boolean | #f | (`:1373-1424`) |
| General | Start date / End date | aa / ab | start and end date | `start-accounting-period` / **`today`** (`:353-359`) | |
| General | Report's currency | b | currency | report currency | |
| General | Price source | c | multichoice | `pricedb-before` | `pricedb-before`, `pricedb-nearest`, `pricedb-latest` |
| Validation | Include only accounts with warnings | a | boolean | #f | |
| Validation | Warn if a lot has more than one bought split / balance drops below zero / blank title / gains mismatch | b / c / d / e | boolean | #t / #t / #f / #t | full names `:300-340` |
| Validation | Warn if a split is not assigned to a lot | f | multichoice | `count` | `no`, `count`, `list` |
| Validation | Warn if the account balance does not match the computed lots' end balance | g | boolean | #t | |

Names keep GnuCash's capitals ("Start date").

### 4.2 Computation

1. **Window**: both bounds at **end** of day (`:667-672`); in window = posted ≥ from and ≤ to.
2. **Holding period** `gnc:date-year-delta` (`5.17:bindings/guile/date-utilities.scm:201-217`): (y₂ − y₁) + (d₂ −
   d₁)/366.0 with a year-day adjustment that treats a non-leap 28 February like a leap 29 February. Long term is
   `years > threshold`, strictly, from the lot's **latest** purchase (`:779-789`, `:1913-1915`).
3. **Accounts, lots, splits** (`:539-597`, `:878-889`): the account's splits ≤ to in split order without voids; lots
   in first-appearance order; unassigned splits newest first; lot splits from `gnc-lot-get-split-list` ≤ to (voids
   kept), sorted by posted time, purchases first on a tie, purchases among themselves by `xaccTransOrder`. Raw
   `xaccSplitGetAmount`; no engine lot balance, closure or gains.
4. **Classification** (`:513-536`): stock-split event (slot `split-type` = `stock-split`, or action "split" ignoring
   case) with amount < 0 and value 0 reduces shares; amount > 0 is a purchase (forward splits included); amount < 0 a
   sale; amount 0 a realized-gain split (summed in window for the mismatch check).
5. **Prices** (`:693-745`): any-currency lookups at local noon of End date (before, nearest) or now (latest);
   `find-price` takes the first in the report currency, else the first whose commodity is the report currency,
   inverted, else the **first as it is**. Values convert with `gnc:case-exchange-fn` at End date's end of day (CU),
   rounded to the report SCU (`:857-864`, `:925-928`).
6. **Purchase** (`:1886-1925`): earliest and latest bought dates; end basis += value, end amount += amount; bought
   totals if in window; split row; bought count += 1.
7. **Sale** (`:1824-1884`): fraction = amount / end amount (exact); basis = fraction × end basis → report SCU,
   banker's; gain = value − basis → report SCU; end basis and amount reduced **also before the window**; in window:
   years held, CAGR (`calculate-cagr`, `:447-461`: (value/basis)^(1/years) − 1 in doubles, #f for bad inputs),
   weighted realized CAGR += cagr × amount (doubles), ST/LT sold figures, split row, counts. A sale against an empty
   lot makes the division an error → #f, which blanks every sum it reaches. A lot with an in-window sale and **no
   purchase** fails the report (`gnc:date-year-delta` on `'()`, `:1860-1862`, `:1473-1477`).
8. **Lot totals** (`:1663-1712`): end value = end amount × price → report SCU (0 with no price); unrealized = end
   value − end basis, all ST or LT by `long-term?(latest bought, to)`. Realized CAGR on a lot or account row =
   weighted sum / sold amount (0 if none); unrealized CAGR on a lot row from end basis and value, on an account row
   weighted by end amounts (`:1478-1530`); #f on the grand total. ROI = gain/basis in doubles (`:475-481`).
9. **Warnings** (`:1561-1661`): per lot (several bought splits; negative balance with date; blank title; computed gain
   ≠ "Realized Gain/Loss" sum with the difference), per account (unassigned splits counted or listed newest first;
   end amount ≠ `xaccAccountGetBalanceAsOfDate`). Each is one spanning cell, `total-label-cell neg`.

### 4.3 Rows

Per shown account (`:2161-2219`): the account name (link); `"  End price: <price> [<in report currency>] on <date>"`
or "No price found" (`:944-962`); header; per shown lot its row (title, GUID, Opened = earliest bought, Closed = last
in-window sale if end amount 0), split rows when asked (running end amount and basis), warnings; a rule, "Account Lots
Total", account warnings. Then a grand header and "Grand Total" (amounts, average prices and CAGRs blank). Columns
(`:983-1044`): Lot Title, [GUID], [Opened, Closed], [Bought Amount, Bought Value (Basis), Bought Average Price], [Sold
Splits, Sold Amount/Basis/Value or ST/LT ×3, Sold Average Price], [End Amount, End Basis, End Value], [realized:
ST/LT or total, [ROI], [CAGR]], [unrealized: the same]; 20 with defaults. Amounts print with the commodity's print
info; values and averages are report-currency monetaries; ROI and CAGR are `"~,1f%"` of 100 × x. Title `"~a, ~a"` of
Report name and `gnc-date-interval-format` (`:2257-2262`). Chart (`:2266-2298`): `bar`, ["Account Lot Gains",
interval], labels the shown gain columns, one series per shown account with `gnc:assign-colors`, stacked when
`bar-stacked`.

### 4.4 Rounding and doubles

| Where | GnuCash | red-ale |
|---|---|---|
| Sale basis, sale gain, end value | → report SCU, banker's (`:1830-1844`, `:1684-1690`) | the same with `round_to` |
| ROI | doubles, `~,1f` (`:480-481`, `:510`) | exact × 100, `fixed(r, (), 1)` |
| Years held | `/ 366.0` | exact; the strict long-term test agrees for every whole-number threshold |
| CAGR | `expt` of doubles (`:457-461`) | **cannot be exact** (a fractional power): a host `growth_rate(ratio, years)` in deterministic fixed point (≥ 96 fraction bits), returned rounded half-even to 10⁻¹²; exact for whole and terminating powers. `~,1f` output differs only within ~10⁻¹² of a .x5 % boundary |
| Weighted CAGRs | double accumulators (`:1342-1349`, `:1874-1875`, `:1528-1530`) | exact sums of the 10⁻¹² values |

### 4.5 Core or script

Script. The core's gains (`crates/red-ale-core/src/ops/lots/gains.rs:47-120`) differ by design: adjusted amounts against
raw; stock splits outside lots against a forward split as a purchase; `xaccTransOrder` against posted date with
purchases first; half-up to the opening value's denominator against banker's to the report SCU; transaction currency
against report-currency conversion; no window against pre-window sales that move basis. The oracle run on book D gave
an engine lot balance of −25 where the report's end amount is 25. Using core gains would also make the mismatch
warning compare the engine with itself.

### 4.6 Test

Five books, all under Root (ASSET, USD) > Assets > Broker > Cash; AAPL `NASDAQ::AAPL`, SCU 1:

| Book | Built by | Contents |
|---|---|---|
| A `reports/investment-lots.gnucash` | `:60-123` | Broker:AAPL, Broker:SPY (`NYSE::SPY`, SCU 1), Income. 2020-01-01 buy AAPL 100/100 (action "Buy 100 shares") against Cash −5000; 2020-02-01 AAPL 50/50 against −3000; 2020-06-01 AAPL −60/−60 against 4800; 2020-01-01 SPY 200/200 against −40000. Values ≠ cash, so each commit adds an Imbalance-USD split (`5.17:libgnucash/engine/Transaction.cpp:1549-1558`). Prices 1 (TEE:279-286). No lots |
| B `reports/lots-reverse-split.gnucash` | `:125-185` | one untitled lot: 2020-01-01 +100 / 5000; 2020-02-01 reverse split −50 / 0 (`xaccSplitMakeStockSplit`, action "Split") |
| C `reports/lots-baseline.gnucash` | `:187-230` | the buy alone, in a lot |
| D `reports/lots-reverse-split-sale.gnucash` | `:232-308` | B plus 2020-03-01 sale −25 / −3000 in the lot |
| E `reports/lots-postsplit-baseline.gnucash` | `:310-370` | +50 / 5000 and −25 / −3000 in one lot |

All five are rebuildable with `engine_extras.py` plus `GncLot(book)`, `lot.add_split` (after `SetParent`) and
`xaccSplitMakeStockSplit`, with a lot GUID kind letter added. Book D was built through the 5.17 oracle: `<act:lots>`
with the lot, `split:lot` on all three splits, and `lot.get_balance()` −25 (adjusted). Dates are absolute; with
§13's settings (period today–today, now 2024-03-15) every split is before the window.

| Lines | Run | Assertion |
|---|---|---|
| `:393-442` | book A, defaults and column/chart combinations | renders |
| `:454-473` | no book | the option names exist |
| `:487`, `:498-500` | A, realized + ROI, each grouping | renders |
| `:514-556` | `calculate-cagr` | 100→200 1 y ∈ [0.99, 1.01]; 100→100 ∈ [−0.01, 0.01]; basis 0, #f, −100 → #f; end −50 → #f; 1000→1100 1 y ∈ [0.09, 0.11]; 100→110 2 y ∈ [0.04, 0.06] |
| `:565-583` | `calculate-gain` (dead) | 50, −20, 0 |
| `:592-615` | `calculate-roi` | 0.25, −0.10, #f, 1.0, −0.25 within ±0.01 |
| `:624-647` | `is-long-term?` (dead, ≥ 365.25 days) | 2020-01-01→2021-01-01 LT (the report's own rule says ST), →2020-12-31 not, →2022 LT, 2015→2020 LT, same day not |
| `:667-727` | classifier | a −10/0 stock split (slot or action "Split") is a reduction; +10/0, plain −10/0, ±10/±100, 0/100 are not |
| `:742-759` | book B | header contains "Sold Splits"; that column has no `"1"`; "Realized Gain" has no `(` |
| `:793-796` | C vs B | those columns equal |
| `:830-835` | E vs D | header found; columns equal |

*From the code, not run:* the default `gains-only` has no "Realized Gain" header, so those checks compare empty lists;
the account name is a `th` too (`5.17:gnucash/report/stylesheets/plain.scm:105-112`), so the "Sold Splits" check reads
the next column. red-ale ports them by column label and adds *worked* figures: C: End Amount `100. AAPL`, End Basis
`$5,000.00`, LT Unrealized `-$5,000.00`, Unrealized CAGR `-100.0%`, "No price found"; B: as C with `50. AAPL`; D and E:
`25. AAPL`, `$2,500.00`, `-$2,500.00`, `-100.0%`.

---

## 5. IFRS cost basis (`standard/ifrs-cost-basis.scm`)

### 5.1 Options (`:79-120`)

| Section | Name | Key | Type | Default |
|---------|------|-----|------|---------|
| General | Start Date | ` a` | date (start) | start of accounting period (sort tag `" "`, `5.17:bindings/guile/options.scm:330-334`) |
| General | End Date | ` b` | date (end) | end of accounting period |
| General | Report's currency | a | currency | report currency |
| General | Stock Account | b | account, STOCK MUTUAL | first of the type in display order |
| General | Proceeds Account | c | account, ASSET BANK | first of the type |
| General | Dividend Account | c | account, INCOME | first of the type |
| General | Fees Account | c5 | account, EXPENSE | first of the type |
| General | Cap Gains Account | d | account, INCOME | first of the type |
| General | Action field filter for fees | d5 | string | `"Fee"` |
| General | Format monetary cells | e | boolean | #t |
| General | Alternative row-style for shorts | f | boolean | #t |
| General | Capitalise purchase commissions | g | boolean | #t |

red-ale's options already resolve these defaults (`crates/red-ale-report/src/options/db.rs:293-321`).

### 5.2 Computation

1. Both dates at the **start** of their day (`:299-304`).
2. No Stock Account: generic options warning (`report/html-utilities.scm:359-365`), title "IFRS weighted average cost
   basis Report", disclaimer (`:325-331`).
3. `currency` = `gnc_account_get_currency_or_parent` of the stock account (`5.17:libgnucash/engine/Account.cpp:3403-3412`).
4. Splits: the stock account's, voids included, in `xaccSplitOrder`, first per transaction
   (`5.17:libgnucash/engine/Query.cpp:67-90`; `Split.cpp:2393`).
5. `txn->info` (`:236-290`), first match: Proceeds → += amount; Cap Gains → += amount; Fees → fees-exp += amount;
   Dividend → += amount; stock account with `gnc-get-action-num` = filter → fees-cap += **value**; stock account →
   stock amount and value. Start as none. `gnc-get-action-num` is the action, or the num under split-action-for-num
   (`5.17:libgnucash/engine/engine-helpers.c:85-102`).
6. None arithmetic (`:122-149`): sums skip none; products, quotients with none, and ÷ 0 are none.
7. Per transaction (`:381-518`):

| Figure | Definition |
|--------|------------|
| new-units | units + trans-units |
| type | `ERROR: long→short` / `ERROR: short→long` on a sign cross; else the first matching row of `short-types` / `open-types` / `long-types` by the sign of units (`:174-216`), else `Unknown` |
| shorting? | new < 0, or new = 0 and trans-units > 0 (`(< 0 #f)` is a Guile error) |
| purchase-cost, purchase-val | if purchase: fees-cap + fees-exp; stock value + fees-cap − cost |
| cash-dividends | − dividend |
| proceeds-cost, proceeds-value | if sale: fees; proceeds + fees |
| fx | `gnc-pricedb-convert-balance-nearest-price-t64` of 10,000,000 at local noon of the posted date, ÷ 10,000,000 (`:317-320`, `:427-428`) |
| conv-* | fx × each of the five |
| per-unit | |basis / units| before |
| basis-of-sale | per-unit × trans-units if sale |
| basis | basis + conv-purchase-val + conv-purchase-cost (if capitalised) + basis-of-sale |
| net-proceeds, gain-post, gain-pre, variance | conv-proceeds − conv-cost; net + basis-of-sale + conv-purchase-cost (if not capitalised); conv-proceeds + basis-of-sale; gain-pre + capgains |
| running | gross += gain-pre; net += gain-post; total += gain-post + conv-dividends |

8. A row only when `start < posted < end` (`:468-469`); style `alternate-row` when shorting (with the option), else
   alternating over written rows.

### 5.3 Rows

Title "Average-Cost (Basis) Report: From ${startdate} to ${enddate}. Report-currency ${currency}" (`:362-367`). One
table of 28 columns, labels as written (`:369-379`). Cells (`:483-510`):
- date and description;
- trans-units, a commodity monetary linked to the split;
- new-units, the narrative, the currency mnemonic;
- fx, through `gnc:default-price-renderer`;
- five figures in `currency`;
- the conv-figures, basis, per-unit, **−** basis-of-sale, net, gains, capgains, variance and the running totals, in the report currency.

None is an empty cell. With "Format monetary cells" off, figures are bare numbers. No totals row. The disclaimer
follows (`:33-50`).

### 5.4 Rounding

None in the figures. The fx rounds to the report SCU (`5.17:libgnucash/engine/gnc-pricedb.cpp:2529-2586`), as
`book.price_convert(monetary(currency, 10000000), report_currency, t, "nearest")` does.

---

## 6. Floating point and rounding: proposed rows

For `../reports.md` §12, "Floating point in GnuCash, exact in red-ale":

| Where | GnuCash | red-ale |
|-------|---------|---------|
| Advanced Portfolio's Rate of Gain and Rate of Return | doubles, `~,2f%` (`standard/advanced-portfolio.scm:914-922`, `:929-936`, `:1126-1133`, `:1145-1152`) | exact rate rounded half away from zero to 2 places, `%`; empty when Money In is 0. Differs at ties the double misses: $0.29 on $8.00 is 3.625 %, GnuCash `3.62%`, red-ale `3.63%` |
| Advanced Portfolio's `div/0` messages | `~0,4f`, `~,2f` (`:227`, `:306`) | `fixed` of the exact figure |
| Investment Lots' ROI | double gain/basis, `~,1f` (`standard/investment-lots.scm:475-481`, `:1184`) | exact, `fixed(r, (), 1)` |
| Investment Lots' CAGR and its weighted averages | `expt` of doubles, double sums (`:447-461`, `:1342-1349`, `:1874`, `:1528`) | fixed-point power rounded to 10⁻¹², exact sums |
| Investment Lots' holding period | `gnc:date-year-delta` with `366.0` (`5.17:bindings/guile/date-utilities.scm:201-217`) | exact |
| "Share decimal places", both portfolios | a double option, `inexact->exact` | an integer Rational |

Not floats, but kept exactly: Advanced Portfolio's 10⁻⁸ and SCU conversions, the Portfolio's 5-significant-figure
price, the IFRS fx, Investment Lots' SCU roundings, the share display's half-up rounding.

For §14, "Deviations from GnuCash":

| What | GnuCash 5.17 | red-ale | Why |
|------|--------------|---------|-----|
| Rates at a tie | Rounded from a double | From the exact rate | No floats |
| Investment Lots' CAGR | A double | Rounded to 10⁻¹² | A fractional power cannot be exact |
| Investment arithmetic past 64 bits | Rounded to fit or #f (`5.17:libgnucash/engine/gnc-numeric.cpp:71-85`; `5.17:bindings/guile/gnc-engine-guile.cpp:1729-1734`) | Exact | Same within 64 bits |
| Investment Lots, in-window sale in a lot with no purchase | The report fails (`standard/investment-lots.scm:1860-1862`) | CAGR blank | One bad lot does not cost the report |
| Investment Lots, no accounts selected | An unconfigured chart | No chart | Nothing to draw |
| Investment Lots' link to an inverted price | A temporary price not in the book (`:946-955`) | The stored price | A link must name a real price |
| IFRS's `(< 0 #f)` | A Guile error (`standard/ifrs-cost-basis.scm:417-418`) | `report-script` error naming the transaction | The same failure, with a message |

---

## 7. Item 6 settled: core or script

**All four reports compute in their scripts; none calls the core's gains code.** The core supplies reads.

| Report | Answer | Why |
|--------|--------|-----|
| Investment Portfolio | Script | Balances and prices only |
| Advanced Portfolio | Script | No lots; FIFO and LIFO over split order, an average basis the core lacks (its one policy is per-lot FIFO, `crates/red-ale-core/src/ops/lots/scrub.rs:131`; `5.17:libgnucash/engine/policy.cpp:154-168`); its own roundings (§3.8), fees, reinvestment, spin-offs and splits. `compute_split_gains` (`ops/lots/gains.rs:87`) writes gains transactions, which no report may do |
| Investment Lots | Script | §4.5 |
| IFRS cost basis | Script | A weighted average over split roles; exact Scheme arithmetic; no average-cost basis in core |

The rule is the milestone's fallback: exact Rationals, with every place GnuCash rounds reproduced and every double recorded.

---

## 8. Library and query gaps

The query API has no lot reads, other split, stock-split flag or any-currency prices
(`crates/red-ale-report/src/query/entities.rs:28-60`, `query/book.rs:23-51`). Core has each read.

| Need | Used by | GnuCash | Core |
|------|---------|---------|------|
| `book.prices_any_currency(commodity, when, mode)`: one per other commodity, newest first; `latest` at `ctx.now` | Portfolio, Advanced Portfolio, Investment Lots | `gnc-pricedb.cpp:1955-2001` | `ops::prices::convert::{latest_before_any_currency, nearest_any_currency}` |
| `s.other_split()` | Advanced Portfolio (`:351`, `:544`, `:809`) | `Split.cpp:2161-2185` | `ops::transactions::other_split` |
| `a.currency_or_parent()` | IFRS (`:335`) | `Account.cpp:3403-3412` | `ops::scrub::currency_or_parent` |
| Split `actionNum` | IFRS (`:243`) | `engine-helpers.c:85-102` | `book_options::uses_split_action_for_num` |
| `split.lot`; Lot handle: `guid`, `title` (or `()`), `notes`, `account`, `splits()` in list order | Investment Lots | `xaccSplitGetLot`, `gnc-lot-get-*` (`standard/investment-lots.scm:549`, `:589`, `:815-823`) | `Book::lot`, `Lot::title`, `ops::lots::Ledger` |
| `split.isStockSplit` | Investment Lots (`:1729`) | `Split.cpp:2039-2058` | `lot::is_stock_split` |
| `m.to_string(use_symbol)` | Investment Lots (`:808-812`) | `gnc-commodity-print-info` | `number::format` |
| `r.round_sigfigs(n, mode)` | Portfolio (`:193-200`) | `gnc-numeric.cpp:468-486` | `number::rational` |
| `growth_rate(ratio, years)` | Investment Lots' CAGR | `expt` | new, `number` |
| `lib/dates`: `time64_canonical_day_time(ctx, t)` and `date_year_delta(ctx, t1, t2)` | all four; Investment Lots | `gnc-date.cpp:404-410`; `date-utilities.scm:201-217` | script |
| Number cells: share display (exactly `places`, half away from zero, grouped, point printed) and half-away-from-zero rounding | both portfolios; rates, ROI, CAGR | `gnc-ui-util.cpp:1013-1023`; Guile `~,nf` | additive `ReportResult` fields |
| Links `{ split }`, `{ transaction }`, `{ price }`; row role `warning` | Advanced Portfolio, IFRS, Portfolio, Investment Lots | `report/html-utilities.scm:195-224`; `standard/investment-lots.scm:1060-1080` | additive `ReportResult` kinds |

Already there:
- reads: `book.commodity_splits`, `book.price_convert`;
- library: `case_exchange_fn`, `sum_collector_commodity` (both take a closure), the collectors, `account_is_stock`, `account_get_comm_balance_at_date`;
- query fields: Split `action` and `sharePrice`, Transaction `splits()`;
- result: price cells, monetary cells for non-currency commodities;
- options: the account-option defaults;
- the warnings.

Price inversion stays in scripts.

---

## 9. GnuCash's tests and fixtures

All set the C locale and run with no preferences (`../reports.md` §13). `standard/test/test-stress-options.scm:237-256`
also renders every template under option combinations (skipping "Balance Sheet (eguile)", `:246-251`); red-ale's
"every template renders on every fixture" property covers it.

### 9.1 `test-portfolios.scm` (108 lines; no licence header)

Book `create-stock-test-data` (TEE:952-1059):
- Accounts: Root (ASSET) > Asset > Bank; Income; Expenses; Equity; Broker > AAPL, MSFT, TSLA (STOCK). Others inherit ASSET and USD (TEE:320-357).
- Commodities `NASDAQ::{AAPL,MSFT,TSLA}`, SCU 1.

| Date | Description | Splits (value/amount) |
|------|-------------|-----------------------|
| 1980-01-01 | seed money | Equity −10000, Bank +10000 |
| 1980-02-01 | buy 1 AAPL @ $100 | Bank −100, AAPL 100/1 |
| 1980-03-01 | buy 1 AAPL @ $200 | Bank −200, AAPL 200/1 |
| 1980-05-01 | sell 1 AAPL @ $400 … | Bank 390, AAPL −400/−1, Income −300, Expenses 10, AAPL 300/0 |
| 1980-10-01, 11-01 | 1:10 stock splits | AAPL 0/9, 0/90 (`xaccSplitMakeStockSplit`, action "Split") |
| 1980-12-01 | 3:1 stock split with cash-in-lieu $4 | AAPL 0/−67; Bank 4, Income −4 |
| 1981-01-01 | buy 10 AAPL @ $5 | Bank −500, AAPL 500/10 |
| 1981-03-01 | sell 1/2 AAPL @ $6 … | Bank 3, AAPL −3/−1/2 (stored −1, `Split.cpp:1254-1255`), Income −5/2, AAPL 5/2 / 0 |
| 1981-04-01 | spin-off $150 | Bank 150, AAPL −150/0 |

Prices:
- Transaction prices at local midnight, source `user:xfer-dialog`, type "test" (TEE:283-288): $100, $200, $400, $50, $6. The 1981-01-01 price is $50, 500 for 10 shares, not the $5 its description says.
- Stock-split prices, source `user:stock-split`, type "unknown": $40, $4, $12.

The stock splits use `xaccTransSetDatePostedSecsNormalized` and USD.

| Line | Run | Expected (row 1) |
|------|-----|------------------|
| 56-58 | Portfolio, `pricedb-latest` | `AAPL AAPL NASDAQ 42.00 $6.00 $252.00` |
| 63-65 | `pricedb-nearest`, Date 1980-03-01 | `AAPL AAPL NASDAQ 2.00 $200.00 $400.00` |
| 70-72 | `average-cost`, Date 1980-09-01 | `AAPL AAPL NASDAQ 1.00 $200.00 $200.00` |
| 76-78 | `weighted-average`, Date 1980-09-01 | `AAPL AAPL NASDAQ 1.00 $233.33 $233.33` |
| 87-91 | Advanced, defaults | `AAPL AAPL NASDAQ 42.00 $6.00 $484.88 $252.00 $800.00 $543.00 $227.88 -$232.88 -$5.00 -0.63% $4.00 $10.00 -$1.00 -0.13%` |
| 95-98 | Advanced, `fifo-basis` | `… 42.00 $6.00 $543.94 $252.00 $800.00 $543.00 $286.94 -$291.94 -$5.00 -0.63% $4.00 $10.00 -$1.00 -0.13%` |
| 103-106 | Advanced, `filo-basis` | `… 42.00 $6.00 $400.00 $252.00 $800.00 $543.00 $143.00 -$148.00 -$5.00 -0.63% $4.00 $10.00 -$1.00 -0.13%` |
| 26-27 | both, empty book | renders |

The marker `" "` is not in the texts. Date defaults to today; any now after 1981-04-01 works, so the pinned
2024-03-15 does. **Fixture** `reports/stock-test-data.gnucash` by `scripts/report-fixtures/stock_test_data.py`.
Buildable: `engine_extras.py` has the multisplit and structure helpers. It still needs `gnc_commodity_new` with SCU 1
and a `stock_split` helper after TEE:893-950. `xaccSplitMakeStockSplit`, `xaccTransSetDatePostedSecsNormalized`,
`gnc_account_get_currency_or_parent`, `PRICE_SOURCE_STOCK_SPLIT` and `xaccSplitSetAction` are all in
`gnucash_core_c` at 5.17.

### 9.2 `test-ifrs-cost-basis.scm` (359 lines; no licence header)

Book (`:54-243`):
- Accounts: Root (ASSET, CAD) > Asset > Broker (CAD) > CAD Cash, USD Cash (USD), SPY (STOCK, `NYSE::SPY`, **SCU 10000**); Asset > Current Assets > Checking Account; Income > USD CapGain, USD Interest; Expenses > USD Commissions; Equity > Opening Balances, Opening Balances USD.
- SPY transactions are in CAD with per-split actions, absolute dates 2019-07-01 to 2020-06-21.
- The CAD→USD transfer adds an unused price.

Options (`:251-259`): currency CAD, Proceeds USD Cash, Fees USD Commissions, Start 2019-01-01, End 2021-01-01;
ISO dates (`:246`).

| Line | Row | Expected |
|------|-----|----------|
| 264-268 | 1 | `2019-07-01, Buy SPY, 100. SPY, 100. SPY, Open Long, CAD, C$1.00, C$20,000.00, C$9.95, C$20,000.00, C$9.95, C$20,009.95, C$0.00, C$0.00, C$0.00` |
| 270-274 | 2 | `2019-12-11, Buy SPY, 50. SPY, 150. SPY, Buy, CAD, C$1.00, C$16,000.00, C$9.95, C$16,000.00, C$9.95, C$36,019.90, C$200.10, C$0.00, C$0.00, C$0.00` |
| 276-281 | 3 | `2020-03-18, Sell SPY, -75. SPY, 75. SPY, Sell, CAD, C$1.00, C$12,000.00, C$9.95, C$12,000.00, C$9.95, C$18,009.95, C$240.13, C$18,009.95, C$11,990.05, -C$6,019.90, -C$6,009.95, -C$6,009.95, -C$6,009.95, -C$6,019.90, -C$6,019.90` |
| 283-287 | 4 | `2020-04-01, Buy SPY, 250. SPY, 325. SPY, Buy, CAD, C$1.00, C$42,000.00, C$9.95, C$42,000.00, C$9.95, C$60,019.90, C$240.13, -C$6,009.95, -C$6,019.90, -C$6,019.90` |
| 289-293 | 5 | `2020-04-16, Return of Capital, 0. SPY, 325. SPY, Return of capital, CAD, C$1.00, -C$2,500.00, -C$2,500.00, C$57,519.90, C$184.68, -C$6,009.95, -C$6,019.90, -C$6,019.90` |
| 295-299 | 6 | `2020-05-02, Buy spy, 125. SPY, 450. SPY, Buy, CAD, C$1.00, C$47,500.00, C$0.00, C$47,500.00, C$0.00, C$105,019.90, C$176.98, -C$6,009.95, -C$6,019.90, -C$6,019.90` |
| 301-305 | 7 | `2020-05-11, stock split, 450. SPY, 900. SPY, Stock split, CAD, C$1.00, C$105,019.90, C$233.38, -C$6,009.95, -C$6,019.90, -C$6,019.90` |
| 307-312 | 8 | `2020-05-21, Sell SPY, -135. SPY, 765. SPY, Sell, CAD, C$1.00, C$21,500.00, C$9.95, C$21,500.00, C$9.95, C$89,266.92, C$116.69, C$15,752.98, C$21,490.05, C$5,737.06, C$5,747.02, C$5,747.02, -C$262.94, -C$282.84, -C$282.84` |
| 314-318 | 9 | `2020-06-03, Buy spy, 150. SPY, 915. SPY, Buy, CAD, C$1.00, C$21,000.00, C$0.00, C$21,000.00, C$0.00, C$110,266.92, C$116.69, -C$262.94, -C$282.84, -C$282.84` |
| 320-325 | 10 | `2020-06-10, Sell SPY, -915. SPY, 0. SPY, Sell, CAD, C$1.00, C$128,100.00, C$9.95, C$128,100.00, C$9.95, C$0.00, C$120.51, C$110,266.92, C$128,090.05, C$17,823.14, C$17,833.08, C$17,833.08, C$17,570.15, C$17,540.30, C$17,540.30` |
| 327-331 | 11 | `2020-06-10, Sell SPY Short, -85. SPY, -85. SPY, Open Short, CAD, C$1.00, -C$11,900.00, C$9.95, -C$11,900.00, C$9.95, -C$11,890.05, C$17,570.15, C$17,540.30, C$17,540.30` |
| 333-337 | 12 | `2020-06-15, Sell SPY Short, -65. SPY, -150. SPY, Short Sell, CAD, C$1.00, -C$11,050.00, C$9.95, -C$11,050.00, C$9.95, -C$22,930.10, C$139.88, C$17,570.15, C$17,540.30, C$17,540.30` |
| 339-344 | 13 | `2020-06-18, Buy SPY Close Short, 50. SPY, -100. SPY, Cover Buy, CAD, C$1.00, -C$5,000.00, C$9.95, -C$5,000.00, C$9.95, -C$15,286.73, C$152.87, -C$7,643.37, -C$5,009.95, C$2,633.42, C$2,643.37, C$2,643.37, C$20,213.52, C$20,173.72, C$20,173.72` |
| 346-351 | 14 | `2020-06-20, Buy SPY Close Short, 100. SPY, 0. SPY, Cover Buy, CAD, C$1.00, -C$8,000.00, C$4.98, -C$8,000.00, C$4.98, C$0.00, C$152.87, -C$15,286.73, -C$8,004.98, C$7,281.75, C$7,286.73, C$7,286.73, C$27,500.25, C$27,455.47, C$27,455.47` |
| 353-357 | 15 | `2020-06-21, Buy SPY, 100. SPY, 100. SPY, Open Long, CAD, C$1.00, C$8,000.00, C$4.98, C$8,000.00, C$4.98, C$8,004.98, C$27,500.25, C$27,455.47, C$27,455.47` |
| 38-39 | null test | renders |

Empty cells are absent, so checks name their columns (§5.3 order). Rows 10 and 11 share a date; creation order holds
through the fixture's entered dates. **Fixture** `reports/ifrs-cost-basis.gnucash` by
`scripts/report-fixtures/ifrs_cost_basis.py`. Buildable: it needs `add_split` to take the action (TEE:275-276) and a
commodity with SCU 10000.

### 9.3 `test-investment-lots.scm`

§4.6; fixtures A-E by `scripts/report-fixtures/investment_lots.py`.

### 9.4 Hand-worked cases

| Report | Cases |
|--------|-------|
| Investment Portfolio | `pricedb-before`; a newest price in another currency; a price quoted the other way (inverted value, original object in the cell, `:116-119`, `:206-215`); no price; alternating rows with a skipped account; "Include accounts with no shares"; 0 places (`42.`) and a half-up tie (0.125 → `0.13`); the sigfigs quirk at a power of ten; no accounts; no shown rows |
| Advanced Portfolio | preference off (`*`, first note); no price (`**`, `$1.00`, second note, `<br>` when both); an unconvertible price; a price in another currency and inversion; `pricedb-latest`; `include-in-gain`, `ignore-brokerage`; reinvestment with a sibling holding account, an unrelated one, a negative holding; a worthless stock; a sale on an empty list; short then buy to zero (`div/0`, rows kept); a spin-off on zero basis; a stock donation; parent-account income by leaf name; the Display toggles; Money In 0; a rate tie (`3.63%`); a foreign transaction currency; two accounts; no accounts |
| Investment Lots | book D in a window from 2019-12-31 to 2020-12-31 (*worked*: bought `100. AAPL` `$5,000.00` `$50.00`; Sold Splits `1`, `25. AAPL`, `$2,500.00`, `$3,000.00`, `$120.00`; ST Realized `$500.00`, Realized CAGR `204.1%`; End `25. AAPL` `$2,500.00`, ST Unrealized `-$2,500.00`, `-100.0%`); the Start date's own trades outside; `gains-and-sales` and `no`; ROI; GUID; blanks off; the long-term threshold with the leap rule, threshold 0 and a fraction; several purchases (ages, Opened, warning); a forward split as a purchase; negative balance; a sale against an empty lot; gains split matching and not; unassigned splits in both modes; balance mismatch; "only accounts with warnings"; the include options; closed lots; every price source incl. a future latest, an inverted, a third-currency and the uninverted quirk; no price; mnemonic and long names off; chart variants; no accounts; title |
| IFRS cost basis | format off; short style off; commissions not capitalised; dividends, return of capital (reclassification), notional distributions, compensatory types, reverse split with cash in lieu, `Unknown`, both ERROR rows; fx ≠ 1; End Date's day excluded, Start Date's included; another filter and split-action-for-num; Dividend = Cap Gains; no STOCK account; the `(< 0 #f)` failure |

Found while translating IFRS cost basis: two of its transaction types can never be reported. "Reverse split w/ cash in
lieu for fractionals" is matched first by "Sell" among the long types and by "Cover Buy" among the short ones, so such
a transaction reads "Sell" (`standard/ifrs-cost-basis.scm`'s type tables; the hand-worked test shows it). A purchase
whose fee goes to the Fees account matches no type and reads "Unknown", since Open Long and Buy require the expensed
fee to be empty or zero; GnuCash's fixture always capitalises purchase fees, so its test never meets this.

---

## 10. Item 7 settled: rows missing from `features.md`

| Report | What it is | Proposal |
|--------|-----------|----------|
| Lot Viewer (`standard/lot-viewer.scm`) | Experimental, menu only with `--extra` (`:205`). One account and a date range; a column per lot with its title, short GUID and invoice; a row per transaction with its splits by lot, and a lot balance row (`:155-193`). Its transaction types (None, Inv, Pmt, Link, `:40-44`) and invoice row are for business lots | Row under "Business reports", **v0.6**: needs §8's lot reads plus `gncInvoiceGetInvoiceFromLot` and `xaccTransGetTxnType` |
| Dashboard (`standard/dashboard.scm`) | Not a template: a menu item (`5.17:gnucash/gnome/report-menus.scm:113-121`) making a two-column Multicolumn report "Dashboard" of the Expense and Income pies, the Income/Expense Chart (two columns wide) and the Account Summary (`:40-65`) | Row under "Report system", **v0.6**: a predefined saved Multicolumn configuration once v0.5 settles child references |
| Balance Sheet (eguile) (`standard/balsheet-eg.scm`) | An eguile example: it evaluates a Scheme-in-HTML template and a CSS file named in options (`:169-171`, `:283-284`, `:544-545`); figures are the Balance Sheet's | **❌** under "Samples": a template is Guile code |

```
|        | Dashboard                   | A two-column page of the income and expense pie charts, the income/expense chart and the account summary | 4.0 |        |
|        | Lot Viewer report           | One account's lots side by side, with their transactions, documents and balances | 3.9 |        |
|   ❌   | Balance Sheet (eguile)      | Balance sheet laid out by an editable eguile template                            | 2.4 |        |
```

---

## 11. Proposed tickets

Sizes exclude tests, fixture scripts and docs.

| # | Ticket | Size | Depends on |
|---|--------|------|------------|
| 1 | Query reads: `prices_any_currency`, `other_split`, `currency_or_parent`, `actionNum`; `round_sigfigs`; `lib/dates::time64_canonical_day_time` | ~200 Rust, ~20 Rhai | |
| 2 | Lot reads: `split.lot`, the Lot handle, `isStockSplit`, `to_string(use_symbol)` | ~200 Rust | |
| 3 | Result additions: share display, half-away-from-zero number cells, `split`/`transaction`/`price` links, `warning` rows; renderer, schema, `redAleApi` 0.5.0 | ~250 Rust | |
| 4 | `growth_rate`: deterministic fixed-point power to 10⁻¹², property-tested on exact powers | ~200 Rust | |
| 5 | Investment Portfolio + `stock-test-data` fixture + §9.1 checks | ~250 Rhai | 1, 3 |
| 6 | Advanced Portfolio figures (§3.2-3.5) | ~450 Rhai | 1 |
| 7 | Advanced Portfolio report: options, columns, totals, notes, errors; §9.1 and §3.7 checks | ~300 Rhai | 3, 5, 6 |
| 8 | IFRS cost basis + fixture + §9.2 checks | ~350 Rhai | 1, 3 |
| 9 | Investment Lots I: options, `date_year_delta`, the walk, classification, merging, collectors, main rows, title | ~500 Rhai | 1, 2, 4 |
| 10 | Investment Lots II: split rows, warnings, price row, chart; fixtures A-E and §4.6 checks | ~350 Rhai | 3, 9 |
| 11 | Hand-worked cases (§9.4) | tests only | 5, 7, 8, 10 |
| — | Docs: §6 rows into `reports.md`, §10 rows into `features.md`, the milestone's IFRS remark corrected | docs only | lands with this note |

Order: 1-4 in parallel; then 5 and 8; then 6, 7; 9, 10; then 11.

---

## Open questions

1. **Rates at ties: exact or the double?** red-ale could emulate the doubles in integers (as `number/flonum.rs` prints
   them) and print `3.62%`. *Proposed:* exact, per the milestone, recorded in §6.
2. **Keep Advanced Portfolio's 10⁻⁸ and SCU roundings?** *Proposed:* yes; they are rational and GnuCash's test
   depends on them.
3. **Share and rate display: result fields or text cells?** *Proposed:* additive number-cell fields (share display,
   `rounding: "halfUp"`), so clients keep the exact figure; the first `redAleApi` 0.5.0 addition.
4. **Links to splits, transactions and prices.** *Proposed:* three additive link kinds, shared with the Transaction
   Report; an inverted price links the stored one.
5. **CAGR precision.** *Proposed:* 10⁻¹² absolute, ≥ 96 fraction bits, recorded in §6.
6. **Investment Lots' undefined figures.** *Proposed:* mirror GnuCash's #f with `()` so blanks match; a lot with an
   in-window sale and no purchase gets a blank CAGR instead of failing (§14 row).
7. **Same-time sales within a lot.** GnuCash keeps the lot's list order. *Proposed:* Book order, as the core's ledger
   approximates it (`ops/lots/ledger.rs:126-133`); record a deviation if ever seen to differ.
8. **The dead helpers' tests** (`is-long-term?`, `calculate-gain`). *Proposed:* don't translate the helpers; port the
   holding-period cases against the report's rule, recording that 2020-01-01 → 2021-01-01 is short term there.
9. **Portfolio's uninverted nearest and before prices.** *Proposed:* keep, with a hand-worked case.
10. **Dashboard: v0.5 or v0.6?** *Proposed:* v0.6, so Multicolumn is not rushed.
11. **IFRS's `(< 0 #f)`.** *Proposed:* a `report-script` error naming the transaction.
12. **The milestone's "Exact figures" bullet.** *Proposed:* replace "the IFRS weighted average" with "Investment Lots'
    growth rates", since IFRS has no doubles.
