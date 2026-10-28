# Engine note: budgets and the v0.4 budget and forecast reports (GnuCash 5.17)

Scope: the domain rules behind milestone-4 §5 Budgets (budgets, estimate from history, budget notes, natural signs)
and the §8 reports v0.4 adds (Budget Report, Budget Balance Sheet, Budget Income Statement and its Profit & Loss
twin, Budget Flow, Budget Chart, Future Scheduled Transactions Summary, Balance Forecast). This note covers
**behaviour**. The budget's XML elements are in [`xml-format-core.md`](xml-format-core.md) §8.3-8.4; the SQL tables
are covered elsewhere, and §6 here only records what is budget-specific. Scheduled transactions themselves (Since Last
Run, formulas, instance states) are another note's subject; §4.10 covers only the cash-flow read the two
forecast reports make.

Citations are `5.17:<path>:<line>`. A bare `:<line>` right after a citation refers to the same file. In report
citations `standard/` stands for `5.17:gnucash/report/reports/standard/`, RU for `5.17:gnucash/report/report-utilities.scm`
and HAT for `5.17:gnucash/report/html-acct-table.scm`, as in [`../reports.md`](../reports.md). "GUI" means GnuCash's GTK
front end: its code is policy, not engine, but it decides what users' books contain.

**Oracle method.** GnuCash 5.17's Python bindings do not wrap `gnc-budget.h`: `bindings/python/gnucash_core.i:48-84`
includes no budget header, and only the Guile `bindings/engine.i:406` does. The experiments in §7 therefore call the
5.17 engine's exported C functions directly with Python `ctypes` (`libgnc-engine.so` for budgets,
`libgnc-expressions.so` for the scheduled-transaction cash flow), passing the raw pointers behind the bindings' `Book`
and `Account` objects (`int(obj.instance)`). All runs used `TZ=UTC` and checked `gnc_version() == '5.17'`. The scripts
are not committed; §7 records what they did and printed.

---

## Facts that surprise

1. **Unset is not zero.** An amount is either set (a numeric Slot exists, even `0/1`) or unset (no Slot). Reads of an
   unset amount return 0, but the reports' roll-up treats only *unset* parents as sums of their children (§1.2, §4.2).
2. **Shrinking the number of periods hides data but keeps it.** Amounts and notes beyond the new count vanish from the
   engine's view, yet their Slots stay and are written to the file; reopening the book (or growing the count again
   after a reload) brings them back (§1.3, exp3).
3. **Amounts for deleted accounts are never removed.** Deleting an account leaves its budget Slots in every budget,
   and GnuCash writes them on every save (§1.7, exp3). red-ale must keep them.
4. **A split at 23:59:59 on a period's last day is in no period's actuals.** Period end is local 23:59:59 and the
   balance read is *strictly before* it (§1.5, exp4).
5. **Estimate is not "copy the actuals".** It reads balances converted at each period boundary's price, so a foreign
   child account's revaluation counts as spending; the Budget Report's actuals convert only the change (§3.4, exp4).
6. **"Significant digits" never rounds left of the decimal point.** With the default of 1, 1234.56 becomes 1235,
   5.67 becomes 6 and 0.567 becomes 0.6; the stored denominators are powers of ten and not reduced (`10/10`) (§3.4).
7. **The natural-signs scrub is a guess.** It decides per budget, from the signs of the summed income and expense
   amounts, which of three pre-3.8 sign conventions the book used; only the GUI runs it, on open (§2).
8. **Deleting the last budget in the GUI unsets `Use natural signs in budget amounts`**, and so does opening a book
   with the feature and no budgets (§2.3).
9. **In SQL books every amount is stored twice**: in `budget_amounts` and again in `slots`, because the backend saves
   the budget's whole KVP frame. Notes are only in `slots` (§6, exp5).
10. **Copying a budget drops its notes**, and the copy's name is the untranslated `Copy of <name>` (§1.7, §3.7).
11. **The default budget falls back to "some" budget.** With no `Default Budget` option set (or a stale one),
    `gnc_budget_get_default` returns the first budget in the collection's hash order (§1.6).
12. **The two forecast reports count a scheduled transaction's occurrences, not its instances**: the remaining
    occurrences after `last`, ignoring deferred instances, auto-create and days in advance, with formulas evaluated
    with no variables (§4.10).

---

## 1. The budget entity (`5.17:libgnucash/engine/gnc-budget.cpp`)

### 1.1 Fields and defaults

| Field | Storage | Default from `gnc_budget_init` (`:103-119`) |
|-------|---------|--------------------------------------------|
| GUID | `QofInstance` | new |
| name | cached string (`:78`) | `_("Unnamed Budget")`, translated (`:110`) |
| description | cached string (`:81`) | `""` (`:111`) |
| number of periods | `guint` (`:89`) | 12 (`:114`) |
| recurrence | `Recurrence` by value (`:84`) | every 1 month from the first of the current month, local today, weekend adjust none (`:115-117`) |
| amounts and notes | the instance's KVP frame, with a lazily filled per-account cache `acct_map` (`:61-72`, `:86`) | empty |

- `gnc_budget_set_num_periods` refuses 0 (`g_return_if_fail(num_periods > 0)`, `:472`); the engine has no upper
  bound. `GNC_BUDGET_MAX_NUM_PERIODS_DIGITS 3` (`5.17:libgnucash/engine/gnc-budget.h:96`) says 999; the GUI's spin
  button allows 1-60 (`5.17:gnucash/gtkbuilder/gnc-plugin-page-budget.glade:401-407`), and every report's period
  option is 1-60 (§4).
- Names and descriptions must be non-NULL (`:390`, `:416`); empty strings are allowed.
- The GUI's New Budget sets the description to `"<Created>: <today in the locale date format>"`
  (`5.17:gnucash/gnome/gnc-plugin-budget.c:191-194`) and keeps the engine's name.

### 1.2 Amounts and notes

Paths (`:494-508`): the amount of account *A* in period *p* (0-based) is the Slot `<A's GUID>/<p>`; its note is
`notes/<A's GUID>/<p>` (`GNC_BUDGET_NOTES_PATH "notes"`, `gnc-budget.h:98`). The period is written in decimal
(`std::to_string`), so keys sort as strings (`0`, `10`, `2`) in the file (exp3).

| Operation | Behaviour |
|-----------|-----------|
| `gnc_budget_set_account_period_value(b, A, p, v)` (`:540-576`) | `p >= num_periods`: warns `Period %i does not exist` and does nothing. A numeric in error (`gnc_numeric_check`, e.g. denominator 0) **unsets** the amount. Otherwise stores `v` as given: no conversion to the account's SCU, no reduction (exp3 stored `1234/100`, `0/1`). |
| `gnc_budget_unset_account_period_value` (`:516-536`) | deletes the Slot; `p >= num_periods` is a `g_return_if_fail` |
| `gnc_budget_is_account_period_value_set` (`:578-585`) | true iff the cache holds a value, i.e. the Slot exists **and is of type numeric** (`:691-692`). A Slot of another type at that path reads as unset |
| `gnc_budget_get_account_period_value` (`:587-597`) | the value, or `0/1` when unset or `p` out of range |
| `gnc_budget_set_account_period_note(b, A, p, s)` (`:599-636`) | `s == NULL` deletes the Slot; any string, **including `""`**, is stored. The GUI passes NULL for empty text (`5.17:gnucash/gnome/gnc-plugin-page-budget.cpp:1295-1296`), but an empty string written by another program is kept and written as `<slot:value type="string"></slot:value>` (exp3) |
| `gnc_budget_get_account_period_note` (`:638-645`) | NULL when the note is empty or absent, so `""` and "no note" read alike; a non-string Slot reads as no note (`:693-694`) |

The cache (`get_perioddata`, `:671-701`) is filled per account from the KVP on first access and indexed by `Account*`;
it is never invalidated except by `set_num_periods` (§1.3). Engine readers therefore see the KVP as it was at first
access for that account plus every change made through the setters.

### 1.3 Changing the number of periods

`gnc_budget_set_num_periods` resizes each cached account vector (`:479-480`) and **does not touch the KVP**.
Shrinking drops cached periods; growing adds empty ones. GnuCash's own test asserts the in-memory loss ("value and
note are lost", `5.17:libgnucash/engine/test/utest-Budget.c:75-106`), but the Slots survive and are written. exp3:
20 periods, amount at period 15 and note at 11, shrink to 10, grow to 20: both read as unset; saved; reopened: both
are back (`100/1`, `n11`). Saving with 10 periods writes them too, so a 5.17 book can hold Slots for periods at or
beyond `num-periods`.

### 1.4 Period dates

`gnc_budget_get_period_start_date(b, p)` = `recurrenceGetPeriodTime(r, p, FALSE)` and end date =
`recurrenceGetPeriodTime(r, p, TRUE)` (`:647-659`), with (`5.17:libgnucash/engine/Recurrence.cpp:397-418`):

- `recurrenceNthInstance(r, n)` starts from `r.start` and applies `recurrenceNextInstance` *n* times (`:384-395`).
  Instance 0 is `r.start` itself, **not** weekend-adjusted.
- start of period *p* = local 00:00:00 of instance *p* (`gnc_dmy2time64`);
- end of period *p* = local 23:59:59 of (instance *p*+1 minus one day) (`gnc_dmy2time64_end`).

`recurrenceNextInstance` (`:198-381`) is the scheduled-transactions algorithm (step forward one period, then align to
the start's phase). The budget-relevant results, all from exp4:

| Recurrence | Periods 0-2 |
|------------|-------------|
| monthly from 2024-01-31 | 01-31..02-28, 02-29..03-30, 03-31..04-29 (a start day past the month's end clamps to the last day, `:364-365`) |
| end of month from 2024-01-31 | identical to the row above |
| end of month from 2024-02-10 | `recurrenceSet` moves the start to 02-29 (`:106-110`): 02-29..03-30, 03-31..04-29, 04-30..05-30 |
| every 3 months from 2024-11-30 | 2024-11-30..2025-02-27, 02-28..05-29, 05-30..08-29 |
| yearly from 2024-02-29 | 2024-02-29..2025-02-27, 2025-02-28..2026-02-27, …, 2027-02-28..2028-02-28, 2028-02-29.. |
| every 2 weeks from 2024-01-03 | 01-03..01-16, 01-17..01-30, 01-31..02-13 |
| monthly from 2024-03-02 (a Saturday), weekend `back` | 03-02..04-01, 04-02..05-01, 05-02..05-30 (06-02 is a Sunday, so period 3 starts 05-31) |
| the same, weekend `forward` | **03-02..03-03**, 03-04..04-01, 04-02..05-01: instance 1 is the adjusted start (`:220-229`) |

The GUI's recurrence widget always sets weekend adjust `none` (`5.17:gnucash/gnome-utils/gnc-recurrence.c:183`,
`:326`), so only another program writes a budget with weekend adjustment; GnuCash reads it and computes as above.
The budget view's column titles walk `recurrenceNextInstance` from `r->start`
(`5.17:gnucash/gnome/gnc-budget-view.c:1383-1396`), the same dates.

### 1.5 Actuals

`gnc_budget_get_account_period_actual_value(b, A, p)` (`:661-669`) =
`recurrenceGetAccountPeriodValue` (`Recurrence.cpp:420-430`) =
`xaccAccountGetNoclosingBalanceChangeInCurrencyForPeriod(A, start_p, end_p, TRUE)`
(`5.17:libgnucash/engine/Account.cpp:3903-3923`):

1. `b1`, `b2` = the **no-closing** running balance (`xaccSplitGetNoclosingBalance`) of *A*'s last split posted
   **strictly before** `start_p` and `end_p` respectively (`GetBalanceAsOfDate`, `:3505-3518`), 0 if none.
2. own change = `b2 - b1` (`GNC_DENOM_AUTO`, `GNC_HOW_DENOM_FIXED`, `:3912`), in *A*'s commodity, unconverted.
3. for every descendant *D* (`gnc_account_foreach_descendant`, so grandchildren too, each once): its own no-closing
   change over the same bounds, converted from *D*'s commodity to *A*'s with
   `gnc_pricedb_convert_balance_nearest_before_price_t64` at `end_p` (`:3897`, `:3587-3607`; same commodity or zero
   change: unchanged), then added with `gnc_numeric_add(…, fraction of A's commodity, ROUND_HALF_UP)` (`:3898-3900`).

Consequences:

- Closing transactions (the `book_closing` Slot) are excluded, through the no-closing balance.
- Signs are the engine's: debits positive. An expense's actual is positive, an income's negative.
- Both bounds are strict: a split posted exactly at a period's end, 23:59:59 local of its last day, falls in neither
  that period nor the next. exp4: `1/1` posted 2024-01-31 23:59:59 UTC (TZ=UTC) appears in no period's actual, while
  the all-periods change of §3.4 includes it.
- Each child's change is rounded to the parent's SCU after conversion, one child at a time (exp4: EUR 3.33 at 1.1 is
  USD 3.66 in period 0).
- The denominator of a zero result is that of the balances read: `0/1` before any split, `0/100` after (exp1).
- There is no bound check on `p`: the reports call it with period indices at or past `num_periods` (§4.3, §4.6) and
  get real figures for the recurrence's later periods.

### 1.6 The default budget

The Book option `options/Budgeting/Default Budget` is a GUID Slot (`5.17:libgnucash/engine/qofbook.cpp:176-179`,
`:215-218`; registered at `5.17:libgnucash/engine/gnc-optiondb.cpp:1223-1226`; GUID KVP in
`gnc-optiondb.cpp:529-535`). `gnc_budget_get_default` (`gnc-budget.cpp:720-752`) looks that GUID up and, if it is
unset or names no budget, returns the first budget `qof_collection_foreach` yields: hash-table order, not file order.
Nothing clears the option when its budget is deleted. Every budget report's "Budget" option defaults to
`gnc-budget-get-default` of the current book (§4).

### 1.7 Clone, delete, deleted accounts

- `gnc_budget_clone` (`:354-383`) copies name, description, recurrence and number of periods, then, for every
  account in the tree and every period below the count, each amount that is set. **Notes and any other Slots are
  not copied**; amounts the cache lost to §1.3 are not copied; amounts of accounts no longer in the tree are not
  copied. exp3: the copy had `2/0/1` and `3/1234/100`, no notes.
- `gnc_budget_destroy` (`:320-328`) marks the instance destroying and commits; it does not touch the Default Budget
  option or the Book's features (the GUI does, §2.3).
- Deleting an account: nothing in the engine removes budget Slots (`gnc-budget.cpp:515`, `:539` ask the question in
  comments; `Account.cpp` has no budget code). exp3: an account destroyed after budget amounts and a note were set
  for it; the saved file still holds `<guid>/0 = 42/1` and `notes/<guid>/0 = gone note`, and in an SQLite book the
  `slots` rows stay while `budget_amounts` omits it (§6).

---

## 2. Natural signs

### 2.1 What the feature means

`GNC_FEATURE_BUDGET_UNREVERSED` = `"Use natural signs in budget amounts"`, description `"Store budget amounts
unreversed (i.e. natural) signs (requires at least Gnucash 3.8)"` (`5.17:libgnucash/engine/gnc-features.h:62`,
`5.17:libgnucash/engine/gnc-features.cpp:45`). With it, amounts carry the engine's sign, as actuals do (§1.5):
expense budgets positive, income budgets negative, liability increases negative. Before 3.8 the GUI stored what the
user typed under the "Reverse Balanced Accounts" preference of the day, so the stored sign depended on that
preference (the scrub's warning text, `5.17:gnucash/gnome-utils/gnc-file.c:756-760`).

### 2.2 The scrub (`5.17:libgnucash/engine/ScrubBudget.c:193-216`)

`gnc_maybe_scrub_all_budget_signs(book)`:

1. no budgets and the feature set: **unset** the feature (`:200-205`).
2. no budgets, or the feature already set: return FALSE, nothing else (`:207-208`).
3. otherwise, for **each budget** (`:212-213`), `maybe_scrub_budget` (`:172-191`), then set the feature
   (`:214`) and return TRUE.

Per budget, `heuristics_on_budget` (`:105-125`): for each account (all descendants of the root), sum its set amounts
over periods `0..num_periods-1` with `gnc_numeric_add_fixed` (`:69-75`), take the sum's sign (−1, 0, +1;
`gnc_numeric_compare` returns 0 for an error such as overflow, `5.17:libgnucash/engine/gnc-numeric.cpp:837-842`), and
add it to a tally for the account's fundamental type (`xaccAccountTypeGetFundamental`,
`5.17:libgnucash/engine/Account.cpp:4482-4513`: BANK, STOCK, MUTUAL, CURRENCY, CASH, ASSET, RECEIVABLE → asset;
CREDIT, LIABILITY, PAYABLE, CREDITLINE → liability; INCOME; EXPENSE; EQUITY; TRADING → none). Then:

| Tallies | Policy | Flipped (every set amount of every period, `:158-167`) |
|---------|--------|----------------|
| expense tally < 0 | `HEURISTICS_INC_EXP` | INCOME and EXPENSE accounts |
| else income tally < 0 | `HEURISTICS_NONE` | nothing ("doesn't need reversing", but the feature is still set) |
| else | `HEURISTICS_CREDIT_ACC` | LIABILITY, EQUITY and INCOME fundamentals (so CREDIT, PAYABLE, CREDITLINE too) |

Asset and equity tallies are computed and logged, never used. Unset amounts stay unset. A budget with no amounts, or
all zero, gets `CREDIT_ACC` (all tallies 0) and its zeros are negated to zeros. exp2 checked all four rows.

### 2.3 When the feature is set and unset

| Where | What | Citation |
|-------|------|----------|
| GUI, after every successful file open | runs the scrub; if it returns TRUE, shows "This book has budgets. The internal representation of budget amounts no longer depends on the Reverse Balanced Accounts preference. Please review the budgets and amend signs if necessary." | `5.17:gnucash/gnome-utils/gnc-file.c:753-793`, called at `:1183`, also for read-only opens (no `is_readonly` check) |
| GUI, New Budget | sets the feature if missing, before creating the budget | `5.17:gnucash/gnome/gnc-plugin-budget.c:181-186` |
| GUI, Delete Budget | after destroying it, if no budgets remain, **unsets** the feature | `5.17:gnucash/gnome/gnc-plugin-page-budget.cpp:927-952` (`:944-948`) |
| GUI, Copy Budget | nothing (the source budget already implies the feature after an open) | `gnc-plugin-budget.c:233-272` |
| engine `gnc_budget_new`, `_destroy`, `_clone`; XML and SQL load; Python bindings; `gnucash-cli` | nothing | the only callers of the scrub and of the feature are the four GUI sites above (grep of the 5.17 tree) |

So a book with budgets written by GnuCash ≥ 3.8 through the GUI carries the feature once opened; a book whose budgets
were all deleted in the GUI loses it at once; a book made through the bindings may have budgets and no feature.

The second feature, `Show extra account columns in the Budget View` (`gnc-features.h:63`, `gnc-features.cpp:46`), is
set by the budget options dialog when either "Show Account Code" or "Show Description" is ticked and never unset
(`gnc-plugin-page-budget.cpp:902-907`). The ticks themselves live in the GUI's `.gcm` state file
(`gnc-budget-view.c:435-463`), not in the Book.

### 2.4 Signs in views and reports

The engine never reverses budget amounts. Display code does, with `gnc_reverse_balance(account)` (the "Reverse
Balanced Accounts" preference; red-ale's `a.reverse_balance(ctx)`):

- Budget view cells and totals negate for display (`gnc-budget-view.c:1114-1115`, `:1140-1141`, `:1234-1235`) and
  **negate typed input before storing** (`:1283-1286`). Input is parsed with `xaccParseAmount` (`:1270`), so the
  stored denominator is the typed one (`100/1`, `1234/100`), not the SCU.
- The bottom totals rows negate the sum of the top-level accounts once, whatever their types (`:1351`).
- Budget Report: budget, actual and difference negated for reverse-balance accounts (`standard/budget.scm:340-341`).
- Budget Income Statement: account budget figures negated for reverse-balance accounts
  (`standard/budget-income-statement.scm:274-276`); net income is the raw sum negated (`:430-435`).
- Budget Balance Sheet: liabilities through a sign handler keyed on the first liability account
  (`standard/budget-balance-sheet.scm:250-254`).
- Budget Flow and Budget Chart: natural signs, unreversed (`standard/budget-flow.scm:130-136`,
  `standard/budget-barchart.scm:207-213`).

---

## 3. GUI operations (`5.17:gnucash/gnome/gnc-plugin-page-budget.cpp`, `gnc-plugin-budget.c`, `gnc-budget-view.c`)

### 3.1 Open, new, copy, delete

| Command | Behaviour | Citation |
|---------|-----------|----------|
| New Budget | feature (§2.3), `gnc_budget_new`, description `Created: <date>` | `gnc-plugin-budget.c:167-198` |
| Open Budget | one budget: the default (§1.6); several: a chooser preselecting the default; none: New Budget | `:200-230`, `:307-359` |
| Copy Budget | as Open to pick; `gnc_budget_clone` (§1.7), name `"Copy of %s"` (not translated) | `:232-272` |
| Delete Budget | "Delete %s?"; `gnc_budget_destroy`; feature unset if none remain | `:274-294`; `gnc-plugin-page-budget.cpp:927-952` |
| Budget Options | name, description, number of periods (1-60), recurrence (no weekend adjust), Show Account Code / Description (§2.3). Name, description, column flags, periods, then recurrence are set in that order | `gnc-plugin-page-budget.cpp:813-924` |
| Run Report | creates a Budget Report for this budget (`gnc:budget-report-create`) | `:1312-1351` |

All writing actions are disabled when the book is read-only (`:316-318`).

### 3.2 Editing one cell

Empty text unsets; otherwise the parsed amount, negated under `gnc_reverse_balance`, is set (`gnc-budget-view.c:1258-1288`).
A parent account whose amount is unset shows its children's roll-up in grey (`:1102-1131`), computed as in §4.2 but
converted with the nearest price (not nearest before) at the period start and added with `GNC_HOW_DENOM_LCD`
(`:1023-1076`).

### 3.3 All Periods (`:1094-1229`)

For the selected accounts and every period: **Replace** with the value (empty value: **Unset**); **Add** the value
(negated under `gnc_reverse_balance`) to the current amount (an unset amount reads 0, so Add sets it); **Multiply** the
current amount by the value, *not* negated (an unset amount becomes a set `0`). Add and Multiply round with
`GNC_HOW_DENOM_SIGFIGS(n) | ROUND_HALF_UP` (§3.4); Replace stores the parsed value unrounded. The value is parsed with
`xaccParseAmount`; a value that does not parse does nothing (`:1215-1222`). "Significant Digits" spin button: 1-12,
default 1 (`gnc-plugin-page-budget.glade:5-11`), remembered per page for the session.

### 3.4 Estimate (`:955-1092`)

Inputs (dialog `budget_estimate_dialog`, `gnc-plugin-page-budget.glade:238-372`):

| Input | Default | Notes |
|-------|---------|-------|
| selected accounts | the view's selection; none: "You must select at least one account to estimate." | `:1029-1041` |
| Start Date | today, the first time the page opens (`recurrenceSet(&priv->r, 1, PERIOD_MONTH, NULL, …)` sets today, `:329`, `Recurrence.cpp:93-100`); afterwards the last date used on that page | `:1052-1058` |
| Significant Digits | 1 (`:327`), range 1-12 | `:1060-1062` |
| Use Average | off (`:328`) | `:1064-1065` |

On OK the estimate recurrence is `recurrenceSet(mult, period type, start date, weekend adjust)` of the budget's
recurrence with the chosen start (`:1072-1077`), so `recurrenceSet`'s phase fixes apply (an end-of-month start moves
to the month's end). Then, per selected account, inside one budget edit (`:1083-1085`):

- **Per period** (`:989-1005`), for `i` in `0..num_periods-1`:
  `num = xaccAccountGetNoclosingBalanceChangeForPeriod(A, start_i, end_i, TRUE)` over the *estimate* recurrence's
  period `i`; if it is not an error, `num = gnc_numeric_convert(num, GNC_DENOM_AUTO, SIGFIGS(n) | ROUND_HALF_UP)` and
  it is stored in budget period `i`. History period `i` fills budget period `i`, whatever their dates.
- **Use Average** (`:972-988`): `num` = the same change from the start of estimate period 0 to the end of estimate
  period `num_periods-1`, divided by `num_periods` with `SIGFIGS(n) | ROUND_HALF_UP`, stored in every period. No
  error check.

`xaccAccountGetNoclosingBalanceChangeForPeriod` (`Account.cpp:3869-3878`) is **not** the actuals' function: it takes
`xaccAccountGetNoclosingBalanceAsOfDateInCurrency(A, t, NULL, TRUE)` at both bounds (`:3848-3856`), each the
account's no-closing balance strictly before `t` plus every descendant's converted at the nearest-before price **at
`t`** and added with HALF_UP to the account's fraction (`:3683-3695`, `:3746-3781`), then subtracts. A foreign child's
unchanged balance therefore contributes its revaluation. exp4: actual for February was `200/100`, the estimate's raw
figure `267/100` (EUR 3.33 held, priced 1.1 then 1.3).

Signs are natural (positive for spending). Every amount is overwritten, including set ones; zero results are stored as
set zeros.

**`SIGFIGS(n)` rounding** (`GncNumeric::sigfigs_denom`, `5.17:libgnucash/engine/gnc-numeric.cpp:468-486`, used by
`convert_sigfigs`, `5.17:libgnucash/engine/gnc-numeric.hpp:215-226`): for `x = num/den`, if `|num| > den`, let `d` be
the number of integer digits of `⌊|num|/den⌋` minus one; the new denominator is `10^(n-d-1)` when `d < n`, else 1.
Otherwise (`|x| ≤ 1`) let `d` be the digits of `⌊den/|num|⌋` minus one; the denominator is `10^(n+d)`. Zero gives
denominator 1. The result keeps that denominator unreduced. exp4, HALF_UP:

| x | n=1 | n=2 | n=3 | n=6 |
|---|-----|-----|-----|-----|
| 1234.56 | `1235/1` | `1235/1` | `1235/1` | `123456/100` |
| 5.6789 | `6/1` | `57/10` | `568/100` | `567890/100000` |
| 0.567 | `6/10` | `57/100` | `567/1000` | `567000/1000000` |
| 0.0567 | `6/100` | `57/1000` | `567/10000` | `567000/10000000` |
| 9.99 | `10/1` | `100/10` | `999/100` | `999000/100000` |
| 1 | `10/10` | `100/100` | `1000/1000` | `1000000/1000000` |
| 98765 | `98765/1` | `98765/1` | `98765/1` | `987650/10` |
| -1234.56 | `-1235/1` | | | |

### 3.5 Notes (`:1231-1304`)

The cursor's cell (account row, period column); no cell: "You must select one budget cell to edit." Empty text
deletes the note (NULL, §1.2). Notes have no length limit in the engine.

---

## 4. The reports

### 4.1 Inventory

| File | Lines | Template | GUID | Menu |
|------|-------|----------|------|------|
| `standard/budget.scm` | 769 | Budget Report | `810ed4b25ef0486ea43bbd3dddb32b11` (`:763-769`, version 1.1) | Budget |
| `standard/budget-balance-sheet.scm` | 791 | Budget Balance Sheet | `ecc35ea9dbfa4e20ba389fc85d59cb69` (`:785-791`) | Budget |
| `standard/budget-income-statement.scm` | 611 | Budget Income Statement | `583c313fcc484efc974c4c844404f454` (`:591-598`) | Budget |
| | | Budget Profit & Loss | `e5fa5ce805e840ecbeca4dba3fa4ead9` (`:602-609`) | Budget |
| `standard/budget-flow.scm` | 322 | Budget Flow | `e6e34fa3b6e748debde3cb3bc76d3e53` (`:315-321`) | Budget |
| `standard/budget-barchart.scm` | 328 | Budget Chart | `415cd38d39054d9e9c4040455290c2b1` (`:322-328`) | Budget |
| `standard/account-summary.scm` | 437 | Future Scheduled Transactions Summary | `47f45d7d6d57b68518481c1fc8d4e4ba` (`:430-435`) | (top level) |
| `standard/balance-forecast.scm` | 306 | Balance Forecast | `321d940d487d4ccbb4bd0467ffbadbf2` (`:300-306`) | Assets & Liabilities |

The Budget menu is `gnc:menuname-budget` = `"B_udget"` (`5.17:gnucash/report/report-core.scm:159`). All files are
GPL-2.0-or-later (headers checked). The Budget Profit & Loss's renderer passes the Income Statement's name
(`budget-income-statement.scm:587-588`), so only its default title differs. "Budget Chart" is the file
`budget-barchart.scm`.

### 4.2 Shared budget helpers

From RU (`5.17:gnucash/report/report-utilities.scm`):

| Function | Lines | Meaning |
|----------|-------|---------|
| `gnc:budget-get-start-date`, `-end-date` | `:727-733` | start of period 0, end of the last period |
| `gnc:budget-account-get-net` | `:744-760` | collector: the account's own amounts (unset = 0) for periods `[start, end)`, `#f` = whole budget; **not rolled up** |
| `gnc:budget-accountlist-get-net` | `:762-776` | the same summed over a list (merge) |
| `gnc:budget-account-get-initial-balance` | `:778-786` | `gnc:account-get-comm-balance-at-date` at the budget's start, children excluded |
| `gnc:budget-accountlist-get-initial-balance` | `:788-794` | the same over a list |
| `gnc:get-account-period-rolledup-budget-value` | `:819-838` | set: the amount; else with children: `budget-account-sum` (`:806-817`), each child's rolled-up value converted to the parent's commodity with `gnc-pricedb-convert-balance-nearest-price-t64` at the **period start**, summed exactly; else 0 |
| `gnc:budget-account-get-rolledup-net` | `:840-856` | rolled-up values summed for periods `[start, end]`, **inclusive**, `#f` = whole budget |

`gnc:html-make-generic-budget-warning` (`5.17:gnucash/report/html-utilities.scm:276-279`, "No budgets exist. You must
create at least one budget.") is already in red-ale's `lib/html-utilities`.

The HTML account table takes two env keys the budget and forecast reports need (HAT):

- `report-budget` (`:238-241`, `:607`): with zero-balance mode `omit-leaf-acct`, an account whose recursive balance is
  all zero is still shown when its `gnc:budget-account-get-rolledup-net` over the whole budget is non-zero
  (`:749-756`).
- `get-balance-fn` (`:591`, `:619-658`): when given, each account's balance is `(get-balance-fn acct start end)`, a
  commodity collector, instead of the split walk. Used by the Budget Balance Sheet (`budget-balance-sheet.scm:257-262`,
  `:624`, `:659`, `:690`), the Budget Income Statement (`budget-income-statement.scm:457-473`) and the Future
  Scheduled Transactions Summary (`account-summary.scm:327-336`). v0.2's `lib/acct-table` already took
  `get_balance_fn` (its `calculate_balances`); it refused `report_budget` until v0.4.

### 4.3 Budget Report (`standard/budget.scm`)

Options (`:107-238`):

| Section | Name | Key | Default |
|---------|------|-----|---------|
| General | Budget | a | `gnc-budget-get-default` |
| General | Use accumulated amounts | b | #f |
| General | Report for range of budget periods | f | #f (enables the five below) |
| General | Range start | g1.1 | `current`; choices first, previous, current, next, last, manual |
| General | Exact start period | g1.2 | 1, range 1-60 step 1 |
| General | Range end | g2.1 | `next` |
| General | Exact end period | g2.2 | 1, range 1-60 |
| General | Include collapsed periods before selected. | g3 | #t |
| General | Include collapsed periods after selected. | g4 | #t |
| Accounts | Account Display Depth | aa | 2 (`add-account-selection`, `5.17:gnucash/report/options-utilities.scm:96-120`) |
| Accounts | Always show sub-accounts | ab | #f |
| Accounts | Account | ac | accounts of exactly the types ASSET, LIABILITY, INCOME, EXPENSE (not BANK, CREDIT, …) |
| Accounts | Flatten list to depth limit | c | #f |
| Display | Show Budget | s1 | #t (enables Show Budget Notes) |
| Display | Show Budget Notes | s15 | #t |
| Display | Show Actual | s2 | #t (enables the link option) |
| Display | Link to actual transactions | s25 | #f |
| Display | Show Difference | s3 | #f |
| Display | Show Column with Totals | s4 | #f |
| Display | Include accounts with zero total balances and budget values | s5 | #t |

Computation:

- Accounts: the selection, plus all descendants when "Always show sub-accounts" (`gnc-accounts-and-all-descendants`),
  sorted by full name (`:656-658`, `:690`); the account table gets `report-budget` (`:681-692`).
- Columns (`calc-periods`, `:560-578`): without ranges every period, then `total`. With ranges, period numbers from
  `calc-user-period` (`:537-545`); the periods before the range collapse into one column (a list), those after into
  another, singletons unwrap.
- "Relative to current" (`find-period-relative-to-current`, `:519-534`) compares `current-time` with period starts and
  ends: before the budget it returns **1** (the second period) and after it `num_periods` (past the last), neither
  adjusted, so `previous`/`next` around the budget's ends give odd columns. The period index then reaches the C calls
  of §1.2 and §1.5 out of range: amounts read 0 (with a CRITICAL log), actuals read the recurrence's later period.
- Per column: budget = sum of `gnc:get-account-period-rolledup-budget-value` over its periods (`:303-308`); actual =
  sum of `gnc-budget-get-account-period-actual-value` (`:321-325`); with "accumulated", a single period *p* means
  periods `0..p` (`:420`) and the total column `0..max` (`:343-345`); difference = budget − actual, after the sign
  flip (§2.4).
- Cells: a budget of exactly 0 prints `.`; a difference prints `.` when budget and actual are both 0 (`:368-369`,
  `:392-393`). A note (only for single-period columns, `:422-424`) becomes a numbered footnote superscript; the notes
  list follows the table (`:244-259`, `:758`).
- Headers: a period's start date (`qof-print-date`); a collapsed column the interval from its first period's start to
  its **last period's start** (`:467-470`); "Bgt"/"Act"/"Diff".
- Title `"<report name>: <budget name> <using accumulated amounts>"` (`:727-735`).
- "Link to actual transactions" anchors each actual to a Transaction Report (`trep-uuid`
  `2fe3b9833af044abb929a88d5a59620f`, `:39`, `:375-386`), which red-ale does not have.

### 4.4 Budget Balance Sheet (`standard/budget-balance-sheet.scm`)

Options (`:112-212`): Report Title (General a, the name); Single column Balance Sheet (c, #t); Budget (d, default);
Accounts (a, BANK CASH CREDIT ASSET LIABILITY STOCK MUTUAL CURRENCY PAYABLE RECEIVABLE EQUITY INCOME EXPENSE); Levels
of Subaccounts (b, 3); Flatten list (c, #f); Commodities: Report's currency, Price Source (`pricedb-nearest`), Show
Foreign Currencies (#t), Show Exchange Rates (#f); Display: Include accounts with zero total balances (#t), Omit zero
balance figures (#f), parent balance and subtotal modes (`add-subtotal-view`), Display accounts as hyperlinks (#t),
Show accounting-style rules (#f), Label/Include total for assets, liabilities, equity (all #t), Include new/existing
totals (#t). Default section Accounts.

Figures (`:372-578`): the date is the budget's start (`:272`), for prices too (`:337-340`). Each account's balance is
its balance at the budget start (`gnc:account-get-comm-balance-at-date`, children excluded) plus the sum of its own
amounts over the whole budget (`gnc:budget-account-get-net`, not rolled up), through `get-balance-fn`
(`:226-262`). Existing/allocated/unallocated assets, existing/new liabilities, existing/new retained earnings
(income+expense negated, `:494-511`), unrealized gain (`gnc:account-get-comm-value-at-date` and
`gnc:accounts-get-comm-total-assets`, `:540-553`), existing/new equity and totals are collector arithmetic on these.

### 4.5 Budget Income Statement and Budget Profit & Loss (`standard/budget-income-statement.scm`)

Options (`:127-256`): Report Title (General a); Budget (c); Report for range of budget periods (d, #f); Range start
(e) and Range end (f), **number ranges** 1-60 default 1 (no relative choices, unlike §4.3); Accounts (a, INCOME and
EXPENSE); Levels of Subaccounts (b, 3); Flatten (c, #f); the same Commodities options; Display: zero-balance options,
subtotal view, hyperlinks (#t), rules (#f), Label/Include revenue and expense totals (#t), Display as a two column
report (j, #f), Display in standard, income first, order (k, #t). Default section Accounts.

Figures: periods `[start−1, end)` 0-based when ranged, else all (`:300-303`); each account's own amounts summed
(`gnc:budget-account-get-net`, not rolled up), negated for reverse-balance accounts (`:274-276`); net income from the
raw sums negated (`:430-435`); prices at the first reported period's start (`:304-309`, `:366-369`). Range errors:
end < start, end > number of periods (`:397-410`). Titles: "for Budget ~a", "for Budget ~a Period ~d", "for Budget ~a
Periods ~d - ~d" (`:498-532`).

### 4.6 Budget Flow (`standard/budget-flow.scm`)

Options (`:48-88`): Budget (General a); Period (b, number range 1-60, default 1, not limited to the budget's count);
Price Source (c, **`pricedb-latest`**); Report's currency (d); Account (Accounts ac, every account in display order).

Figures: accounts decomposed by type; **only accounts without children** get rows (`:124-127`, the FIXME explains
actuals already include children). Each row: amount `gnc-budget-get-account-period-value(p−1)` (unset = 0, not
rolled up) and actual `gnc-budget-get-account-period-actual-value(p−1)` (`:131-136`); per-type totals and a grand total
through `gnc:sum-collector-commodity` in the report currency. The exchange function's date is
`gnc-budget-get-period-end-date budget period` with the **1-based** period (`:286-288`), i.e. the end of the *next*
period; with `pricedb-latest` the date does not matter. Title "~a: ~a - ~a" with the period start (`:295-299`).

### 4.7 Budget Chart (`standard/budget-barchart.scm`)

Options (`:72-146`): Budget (General a); Range start (g1.1, `current`), Exact start period (g1.2, 1-60), Range end
(g2.1, `next`), Exact end period (g2.2) (no "use ranges" switch: always ranged); Accounts (c, BANK ASSET LIABILITY);
Levels of Subaccounts (d, 6); Running Sum (Display a, #t); Chart Type (b, `bars` or `lines`); Plot Width/Height (c,
100 %). Exact periods round (`:272-273`) where §4.3 truncates.

One chart per account that is a leaf within the depth limit or sits exactly at the limit (`:304-317`). For every
period in `[start, end]`, inclusive, a point at the period start's date: budget = rolled-up value, actual =
`gnc-budget-get-account-period-actual-value`, both cumulative from period 0 when Running Sum (sums run over all
periods, also those outside the range, `:165-222`). Series "Actual" `#FF4136` then "Budget" `#0074D9`; title the
account name and, with running sums, "Bgt: ~a Act: ~a". The relative-period quirk of §4.3 applies (`:246-261`).

### 4.8 Future Scheduled Transactions Summary (`standard/account-summary.scm`, `sx? = #t`)

Verified: a second template in the Account Summary's file (`:430-435`), sharing its options generator and renderer
with `sx?` true (`:138`, `:241`). Differences from the Account Summary:

- "Start Date"/"End Date" (`add-date-interval`, General key c) replace "Date" (`:146-151`); defaults the accounting
  period's start and end (`gnc:options-make-date-interval!`, `5.17:bindings/guile/options.scm:330-334`).
- From date = start of day of Start Date, to date = end of day of End Date (`:250-257`); prices at the to date.
- Title "${company-name} ${report-title} For Period Covering ${start} to ${end}" (`:290-295`).
- Balances: `gnc-sx-all-instantiate-cashflow-all from to` (`:310-311`) gives GUID → amount; each account's balance is
  that amount in its own commodity, or an empty collector (`:327-336`), through `get-balance-fn`. The table's parent
  subtotals then add children as for any balance. No real transaction is read.

GnuCash's test (`standard/test/test-account-summary.scm:67-80`) runs it on `create-test-data`, which has no scheduled
transactions, and checks the account column and three `$0.00` cells.

### 4.9 Balance Forecast (`standard/balance-forecast.scm`)

Options (`:73-126`): Accounts (Accounts a, BANK and CASH); Start Date/End Date (General a); Interval (b, `DayDelta`);
Report's currency (c); Price Source (d, `pricedb-nearest`); Display: Plot Width/Height (a, 100 %), Data markers? (b,
#f), Show reserve line (c, #f) and Reserve amount (d, 0, −10E9..10E9 step 0.01), Show target line (e, #f) and Target
amount above reserve (f, 0, same range), Show future minimum (g, #f). The two amounts are doubles in GnuCash
(number ranges, `../reports.md` §6).

Figures (`:139-298`):

1. intervals = `gnc:make-date-interval-list from to delta` (`5.17:bindings/guile/date-utilities.scm:323-349`): `[d,
   next−1s]` pieces, the last ending at `to`; months step from the start date (`incdate-months`).
2. per account, `gnc:account-get-balances-at-dates` at each interval's end (`:171-175`); if every figure is zero, the
   empty-data warning (`:184-188`).
3. a scheduled-transaction accumulator starts with `cashflow(earliest, from)` where `earliest` is the earliest posted
   date of any selected account's first split (`gnc-account-get-earliest-date`, `Account.cpp:5114-5120`; accounts
   without splits skipped) (`:194-208`).
4. per interval: add `cashflow(interval start, interval end)` to the accumulator; the point is the real balance at
   the interval end plus the accumulator, converted to the report currency at the interval end
   (`gnc:case-exchange-time-fn`, `:166-169`, `:211-232`).
5. series: optional "Minimum" `#0AA` (each point's minimum of itself and all later points, `:235-245`), "Balance"
   `#0A0`, optional "Target" `#FF0` (reserve + target) and "Reserve" `#F00`, all line width 1.5, point radius 3 with
   markers; labels the interval ends as ISO dates; title the report title and `gnc-date-interval-format from to`.

Both cash-flow ranges in steps 3 and 4 include the start day: `cashflow` turns its time64 bounds into local dates
(`gnc_time64_to_GDate`, `5.17:bindings/guile/expressions.i:27-32`) and counts both ends (§4.10), so an occurrence on
the start date is counted twice. From the source; not run through the oracle.

### 4.10 Scheduled-transaction cash flow (`5.17:libgnucash/app-utils/gnc-sx-instance-model.c`)

`gnc_sx_all_instantiate_cashflow_all(start, end)` (`:1968-1976`, declared `gnc-sx-instance-model.h:225-232`, exported
from `libgnc-expressions.so`) runs over the current book's scheduled transactions:

1. per scheduled transaction, `count = gnc_sx_get_num_occur_daterange(sx, start, end)`
   (`5.17:libgnucash/engine/SchedXaction.cpp:739-812`): 0 if it has a number of occurrences and none remain, or an end
   date before `start`; otherwise walk occurrences from its `last` occurrence (or from its start if never run),
   counting those in `[start, end]` (local dates, both inclusive) up to its end date or remaining count. The `last`
   occurrence itself is never counted (`:756-760`, `:805-808`). Deferred instances, days in advance and auto-create
   play no part.
2. disabled scheduled transactions contribute nothing (`:1902-1907`).
3. for each template transaction and each template split, the split's real account from `sched-xaction/account`
   (`:1060-1085`; an unknown account **stops that template transaction's remaining splits**, `:1822-1826`), and
   `debit − credit` (`:1854`) times `count` with `gnc_numeric_mul(…, denom of the difference, ROUND_HALF_UP)`
   (`:1856-1858`), added to that account's entry with `GNC_DENOM_AUTO | REDUCE | RND_NEVER` (`:1767-1773`).
4. debit and credit (`_get_sx_formula_value`, `:1087-1148`): with no variable bindings, a valid **non-zero**
   `*-numeric` Slot is used as is; otherwise the `*-formula` string is parsed with the expression parser and no
   variables. An unknown variable is created with value 0 (`5.17:libgnucash/app-utils/calculation/expression_parser.c:676-705`),
   but with no bindings the parser is initialised with, and writes back to, GnuCash's persisted calculator variables
   (`5.17:libgnucash/app-utils/gnc-exp-parser.c:508-509`, `:591-594`), so in the GUI a variable may carry a
   calculator value. A parse error leaves 0.
5. a split whose account's commodity differs from the template transaction's first split's account's commodity
   contributes 0 (`:1869-1878`).

Figures are amounts in each account's commodity, debits positive. exp6 confirmed occurrence counting and the weekend
adjustment on `rich.gnucash`'s monthly rent (last 2024-09-01, end 2025-07-01, weekend `back`): Sep 1-Dec 31 → 3; Aug
1-Sep 30 → 0; Nov 29-Nov 29 → 1 (December's occurrence moves back to Friday Nov 29); Dec 1-31 → 0; 2025-01-01 to
2026-12-31 → 7. Formula evaluation could not be run: the expression parser's initialisation loads Guile
(`gnc-exp-parser.c:85`), and the Python process segfaults there.

### 4.11 What red-ale's report library lacked

Added in v0.4 (`../reports.md` §5 "Budget", §6, §12): the Budget handle and the three Book reads, the budget,
complex-boolean and multichoice-callback options and the budget option's saved form, `lib/budget`, `report_budget`,
`make_date_interval_list`; `book.price_convert(…, "nearest")` was already the nearest conversion; then
`book.sx_cashflow` (`../reports.md` §5 "Book"). `a.earliest_date()` was not needed: the Balance Forecast reads the
first split of `a.splits()`, which is in split order as GnuCash's split list is.

| Need | Used by | GnuCash |
|------|---------|---------|
| Budget handle: name, description, periods, period start/end, amount, set?, note, actual | all budget reports | `gnc-budget-*` (§1) |
| `book.default_budget()` | every "Budget" option default | `gnc_budget_get_default` (§1.6) |
| A `budget` option type; saved configurations write `(gnc-budget-lookup "<guid>" …)` (`5.17:bindings/guile/options.scm:148-160`), which `import/forms.rs` refuses today | all budget reports | `gnc-register-budget-option` |
| complex boolean and multichoice-with-callback options (their callbacks only toggle selectability) | Budget Report, Budget IS, Budget Chart, Balance Forecast | `gnc-register-complex-boolean-option`, `gnc-register-multichoice-callback-option` |
| `lib/budget`: the RU helpers of §4.2 | all budget reports | RU:727-856 |
| `lib/acct-table`: `report_budget` and `get_balance_fn` | Budget Report, Budget BS, Budget IS, FSTS | HAT:591-658, :749-756 |
| `lib/dates`: `make_date_interval_list` | Balance Forecast | DU:323-349 |
| `a.earliest_date()` (first split's posted date or none) | Balance Forecast | `gnc_account_get_earliest_date` |
| `book.sx_cashflow(from, to)`: GUID → Rational, computed from the model without creating anything | FSTS, Balance Forecast | §4.10 |
| `pricedb-nearest` conversion (not nearest-before) at a time, for the roll-up | Budget Report, Budget Chart | `gnc-pricedb-convert-balance-nearest-price-t64` |

Everything else (collectors, exchange functions, `account-get-comm-balance-at-date`, charts, warnings,
`gnc:account-get-balances-at-dates`, `gnc:case-exchange-time-fn`) is already in v0.2's library.

---

## 5. GnuCash's tests

| Test | What it checks | Rebuildable as a red-ale fixture |
|------|----------------|---------------------------------|
| `standard/test/test-budget.scm` (254 lines, GPL-2.0-or-later header) | Budget Report: cell counts and the Bank, Income and Expenses rows for all-on, `current`..`next`, `last`..`last`, manual 2-4 without collapsing, accumulated (`:66-161`); footnote texts `"income-0 -$60"`, `"expense-1 $25"` (`:109-112`). Budget Income Statement: `$195.00` income, "Net income for Budget test budget" `$105.00` (`:163-184`). Budget Balance Sheet: existing/allocated/unallocated assets `$3,118.00`/`$120.00`/`-$15.00`, total `$3,223.00`, liabilities `-$3.00`, retained earnings `$3,227.00`/`$105.00`, unrealized losses `$1.00`, equity `$3,115.00`/`$105.00`/`$3,220.00`, total L&E `$3,223.00` (`:186-253`) | Yes. The book is `create-test-data` plus `gnc:create-budget-and-transactions` (`5.17:bindings/guile/test/test-engine-extras.scm:851-884`): budget "test budget", 6 periods, the engine's default monthly recurrence (first of the current month), nine amounts, six transactions at each period's midpoint `⌊(start+end)/2⌋`, one marked closing, two notes. With `../reports.md` §13's pinned now (2024-03-15) the recurrence is monthly from 2024-03-01; the script sets it explicitly. The budget calls go through `ctypes` (§7), since the bindings lack them. `current`/`next` follow from the pinned now |
| `standard/test/test-account-summary.scm:67-80` | FSTS on a book without scheduled transactions: account names, three `$0.00` | Yes, on `reports/test-data.gnucash` |
| `standard/test/test-stress-options.scm:262` | builds the same budget and runs every report under option combinations; checks only that each renders | as a red-ale property: every template on the budget fixture renders |
| `libgnucash/engine/test/utest-Budget.c` (204 lines) | defaults (name "Unnamed Budget", description "", 12 periods, monthly, weekend none); set name, description, periods; §1.3's in-memory loss; 12 monthly periods from 2012-01-01 with the right end days (leap February); setting period 12 of 12 warns (`:33-191`) | Yes, as core unit tests; the §1.3 case should assert GnuCash's on-disk behaviour too (exp3) |
| `libgnucash/engine/test/test-recurrence.c` | recurrences in general (the scheduled-transactions note's subject) | for period dates via §1.4 |

No GnuCash test covers Budget Flow, Budget Chart, Balance Forecast, the estimate, the scrub or the cash flow with real
scheduled transactions. Those need hand-worked cases citing the lines above, checked where possible with §7's
`ctypes` oracle (the scrub, period dates, actuals, `SIGFIGS`, cash flow with numeric templates).

---

## 6. Budget-specific points in the SQL backend (`5.17:libgnucash/backend/sql/gnc-budget-sql.cpp`)

- Tables `budgets` (version 1: `guid`, `name` ≤ 2048, `description` ≤ 2048 nullable, `num_periods`) and
  `budget_amounts` (version 1: `id` autoincrement, `budget_guid`, `account_guid`, `period_num`, `amount` as
  `amount_num`/`amount_denom`) (`:49-107`); the recurrence in `recurrences` (`:313-317`, `:403-404`).
- **Notes are in `slots`** only (`notes/<acct>/<period>`, string), saved with the budget's whole KVP frame
  (`gnc_sql_slots_save`, `:408`).
- **Amounts are written twice**: `save_budget_amounts` deletes the budget's rows and inserts one per set amount, for
  accounts in the tree and periods below the count only (`:251-289`); the KVP frame in `slots` holds every amount
  Slot, including §1.3's hidden periods and §1.7's deleted accounts. exp5: `budget_amounts` had one row; `slots` had
  the same amount, the hidden period 15, the deleted account's amount and the note.
- Load order: budget row, then `budget_amounts` through `gnc_budget_set_account_period_value` (`:310-319`, `:196-218`),
  then all budgets' slots merged into each instance's frame (`:335-339`;
  `5.17:libgnucash/backend/sql/gnc-slots-sql.cpp:776-797`), which overwrite the same paths. The cache filled from
  `budget_amounts` is not refreshed, so if the two copies ever disagree, the engine's reads follow `budget_amounts`
  for accounts already cached and the KVP (written back on save) follows `slots`. Only an outside edit makes them
  disagree.
- Every commit rewrites all of the budget's amount rows (`:264`), so the `id` values change on each save.
- A deleted budget's amounts, recurrence and slots are deleted (`:411-421`).

---

## 7. Oracle runs (GnuCash 5.17, `TZ=UTC`, `ctypes`)

### 7.1 exp1: `rich.gnucash`'s budget

Read on a copy: "Budget 2024", description "", 12 monthly periods from 2024-01-01; period 0 runs 2024-01-01
00:00:00 to 2024-01-31 23:59:59, February ends on the 29th. Food: set `100/1` with note "January food" (0), unset (1),
`300/1` (2), `1100/1` (10). Actuals `0/1` before June, `1234/100` in June, `0/100` after. The book has no features
Slot; `gnc_maybe_scrub_all_budget_signs` returned 1, set the feature, left the positive expense amounts (policy
`CREDIT_ACC`), and returned 0 when run again. The saved file gained the features frame with the feature's description
string.

### 7.2 exp2: the scrub's three policies

One USD book per case with Bank, Card (CREDIT), Equity, Salary (INCOME), Food (EXPENSE), no feature:

| Amounts before | After |
|----------------|-------|
| Food −100, Salary 500, Card 50, Bank 10 | Food 100, Salary −500; Card, Bank unchanged (INC_EXP) |
| Food 100, Salary −500, Card 50 | unchanged (NONE), feature set |
| Food 100, Salary 500, Card 50, Equity 7 (period 1), Bank −3 (period 2) | Salary −500, Card −50, Equity −7; Food, Bank unchanged (CREDIT_ACC) |
| Food 0, Salary 0 | unchanged, feature set |

A book with the feature and no budgets: scrub returned 0 and unset the feature.

### 7.3 exp3: periods, notes, deleted accounts, clone, destroy

New budget: "Unnamed Budget", "", 12 periods, monthly from 2026-10-01 (the run's month). Set 20 periods; Food: `100/1`
at 15, note "n11" at 11, note `""` at 1, `0/1` at 2, `1234/100` at 3, `5/0` at 4; account Gone: `42/1` and a note at 0.
Shrink to 10, grow to 20: 15 and 11 read as unset. Note 1 read as NULL; period 2 set to `0/1`; period 4 unset. Period
25 warned. Clone: same name, 20 periods, `1234/100` at 3, no notes. Gone destroyed, saved: the file holds Food's 15, 2,
3 (keys sorted as strings) and notes 1 (empty string element) and 11; Gone's amount and note; the clone's two amounts
and Gone's amount, no notes. Reopened: two budgets, default = "B1" (first in the collection), Food period 15 set
`100/1` and note 11 back. Destroying B1 left one budget, and the default became "Copy".

### 7.4 exp4: period dates, actuals, estimate

The recurrence table of §1.4. Actuals for monthly periods from 2024-01-01 with Exp (USD) and child EurChild (EUR):
Exp 10.00 on 01-15, 1.00 at 01-31 23:59:59, 2.00 at 02-01 00:00:00, a closing −5.00 on 01-31; EurChild 3.33 on 01-20;
EUR→USD 1.10 on 01-10 and 1.30 on 02-15. Period 0: Exp `1366/100`, EurChild `333/100`; period 1: Exp `200/100`;
period 2: `0/100`. Estimate's raw figures: period 0 `1366/100`, period 1 `267/100`; all six periods `1733/100`,
averaged `3/1` with 1 digit and `289/100` with 3. The `SIGFIGS` table of §3.4.

### 7.5 exp5: an SQLite book

Built in `sqlite3://` with the exp3 pattern: `budgets` one row with `num_periods` 10; `budget_amounts` one row (Food,
period 3, 1234/100); `slots` frames for both account GUIDs (including Gone's `42/1` and Food's period 15) and
`notes/<Food>/3 = note3`; `recurrences` `(1, <budget>, 1, 'month', '20261001', 'none')`; versions `budgets` 1,
`budget_amounts` 1, `slots` 4, `recurrences` 2.

### 7.6 exp6: the cash flow

`gnc_sx_all_instantiate_cashflow_all` on a read-only copy of `rich.gnucash`, after `gnc_set_current_session`: the
ranges and counts of §4.10, Food `+456 × n`, Checking `−456 × n`, reduced (`1368/1`). With the Food leg's formula
changed to `rent` and its numeric to `0/1`, the call segfaulted in the parser's Guile initialisation.

---

## 8. Implications for red-ale (proposals)

1. **Model.** A `Budget` entity: GUID, name, description, number of periods, a `Recurrence` (shared with scheduled
   transactions), and its Slots kept whole. Amounts and notes are *read from* and *written to* those Slots, so
   anything else in the frame (hidden periods, deleted accounts, unknown keys, a non-numeric Slot at an amount path)
   round-trips untouched. `BudgetAmount` is `Unset | Set(Numeric)`, never `Option<Zero>`.
2. **Hidden data stays hidden, not lost.** Shrinking the number of periods changes only `num-periods`; red-ale reads
   amounts only below the count, as GnuCash's reads do after a load. *Proposed:* `budget.update` that lowers the count
   reports how many hidden amounts and notes remain, and offers no way to drop them in v0.4 (GnuCash has none).
3. **Store amounts as given.** No reduction, no rounding to the SCU, keep the denominator; refuse a zero denominator
   (GnuCash silently unsets) with `invalid-params`.
4. **Empty notes.** `budget.setNotes` with `""` deletes the note, as the GUI does; an empty-string Slot already in a
   file is kept and reads as no note.
5. **Natural signs.** Keep milestone4's proposal: never scrub or unset on open. Port the scrub exactly (§2.2,
   including the zero-tally and overflow cases) as `budget.scrubSigns`, which also sets the feature, as one Mutation,
   destructive. New: when a client deletes the last budget, red-ale **does not** unset the feature; that differs from
   the GUI, but leaving a feature set is harmless to GnuCash (≥ 3.8 knows it) and unsetting it is a write GnuCash's
   GUI would redo on its own next open. Record as a deviation.
6. **Creating a budget** sets the feature if missing, as New Budget does; name default `Unnamed Budget` (C locale),
   description `""` (red-ale does not invent `Created: <date>`; a client can send it), 12 periods, monthly from the
   first of the Env clock's current local month, weekend adjust none.
7. **Copy** (`budget.copy`, *proposed*): GnuCash's clone, notes dropped and name `Copy of <name>`, or a copy that keeps
   notes. *Proposed:* match GnuCash and say so in the API doc; it is the Parity row's behaviour.
8. **Delete.** Removes the budget; leaves `Default Budget` as GnuCash does (a stale GUID is harmless, §1.6). Deleting
   an account used by a budget stays refused (milestone4 "Compatibility"), which differs from GnuCash but loses
   nothing.
9. **Default budget.** `budget.list` marks the default: the option's budget, else the **first budget in book order**
   (deviation: GnuCash takes hash order).
10. **Actuals and period dates** are core reads ported line by line (§1.4, §1.5), strict bounds included, with the
    children converted at the nearest-before price at period end and rounded HALF_UP per child.
11. **Estimate.** `budget.estimate` takes accounts, start date, significant digits (1-12, default 1) and average
    (default false); computes per §3.4 with `xaccAccountGetNoclosingBalanceChangeForPeriod`'s semantics (not the
    actuals'), and `SIGFIGS` ported exactly (unreduced power-of-ten denominators). One Mutation, destructive.
12. **All Periods** (*proposed* `budget.setAllPeriods`, or left to clients via `budget.setAmounts`): if offered, it
    takes amounts in natural signs, so the reverse-balance negation of §3.3 belongs to clients.
13. **SQL Backend.** Write `budget_amounts` exactly as `save_budget_amounts` does (delete and reinsert, accounts in the
    tree, periods below the count) and the full frame in `slots`; read both and refuse to open for writing (read-only,
    with the reason) a book where they disagree.
14. **Reports.** Add §4.11's reads and library pieces; translate the five budget templates (six with Budget P&L) and the
    two forecast templates. Keep GnuCash's quirks as they are (relative period 1 before the budget, collapsed headers
    ending at the last period's start, Budget Flow's price date, the forecast's double-counted start day) and list them
    in `reports.md` §14. "Link to actual transactions" is refused until the Transaction Report exists.
15. **Cash flow** is a core read over the scheduled-transaction model, never creating anything: occurrences per §4.10
    from `last`, `debit − credit` from the numeric Slots, and formulas evaluated with every variable 0. Deviation:
    GnuCash's GUI may use persisted calculator variables (§4.10 step 4).
16. **Oracle for budgets.** A small `ctypes` layer in `red-ale-testkit` over `libgnc-engine.so` (the functions of §1)
    lets oracle tests read back amounts, notes and period dates, run the scrub, and build the `test-budget.scm`
    fixture. It needs the 5.17 library path, already pinned. The Guile engine module is also installed with the
    oracle (`share/guile/site/3.0/gnucash`), but running it is a decision for the maintainers, given "No Guile, ever".

---

## Open questions

1. **Formulas in the cash flow.** What GnuCash 5.17's GUI really gives for a template formula with a variable depends
   on its persisted calculator variables; the oracle cannot evaluate formulas without Guile. Is "variables are 0" an
   acceptable recorded deviation, or should the oracle gain a Guile-initialised harness?
2. **The forecast's start day.** §4.9's double count of an occurrence on the start date is from the source only.
   red-ale keeps it, and a hand-built case worked from the source shows it
   (`crates/red-ale-rpc/tests/report_forecast.rs`, `an_occurrence_on_the_start_date_counts_twice`); only a Guile
   harness could confirm it against GnuCash itself.
3. **SQL copies that disagree.** Proposal 13 refuses writing; reading could instead prefer `slots` (what GnuCash writes
   back). Needs a decision with the SQL Backend's design.
4. **Weekend-adjusted budgets.** Only another program writes them; red-ale reads them and computes as §1.4. Should
   `budget.create`/`update` accept a weekend adjustment, which GnuCash's GUI never sets?
5. **Number of periods.** The engine accepts any positive count, the GUI 1-60, the reports' period options 1-60.
   *Proposed:* accept 1-999 (`GNC_BUDGET_MAX_NUM_PERIODS_DIGITS`) through the API, and note that GnuCash's dialog
   clamps larger counts when the user opens Budget Options.
6. **Feature on last delete** (proposal 5) differs from the GUI; confirm the maintainers prefer "never unset".
