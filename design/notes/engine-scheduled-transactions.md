# Engine note: scheduled transactions (GnuCash 5.17)

Scope: the domain rules behind milestone-4 §4 (scheduled transactions, recurrence rules, formulas and variables, Since
Last Run, schedule from transaction, the mortgage and loan assistant) and the financial functions their formulas call.
This note covers **behaviour**. The XML of scheduled transactions, template transactions and recurrences is in
[`xml-format-core.md`](xml-format-core.md) §8; it is not repeated here.

Citations are `5.17:<path>:<line>`. A bare `:<line>` right after a citation refers to the same file. "GUI" means
GnuCash's GTK front end: its code is policy, not engine, but Since Last Run, Schedule from transaction and the loan
assistant only exist there, so they set what users' books contain. "SX" is GnuCash's code abbreviation for a
scheduled transaction; this note writes *scheduled transaction* except when quoting code. "Oracle" runs used the
GnuCash 5.17 libraries (§9 says how, since the Python bindings expose none of this); the scripts are not committed,
and §9 records what they did and what they printed.

Notation: `Q` = rational (`num/denom`, i64). **HALF_UP** = round half away from zero. `round(x, d)` = convert to
denominator `d` with HALF_UP. GDate arithmetic is GLib's: `g_date_add_months` and `g_date_subtract_months` keep the
day of month and clamp it to the last day of a shorter month (GLib behaviour, docs-only here; confirmed by the oracle
sequences in §2.6).

---

## Facts that surprise

1. **A postponed instance makes Since Last Run propose already-created instances again.** Instances after a
   deferred instance are generated from the deferred instance's saved state, not from the schedule's last
   occurrence (`5.17:libgnucash/app-utils/gnc-sx-instance-model.c:500-535`). Postpone January, create February, and
   every later run proposes February again as *to create*; the oracle created the same February transaction three
   times (§9.4). §4.8 lists this and the other state quirks.
2. **An instance whose formula fails still creates its transaction.** The transaction is committed with 0 for the
   failing amounts, the error is reported, and the schedule is *not* advanced, so the next run creates it again
   (`gnc-sx-instance-model.c:1127-1139`, `:1410-1412`, `:1518-1535`; oracle §9.3, "AutoVar": four zero
   transactions from two runs).
3. **Since Last Run never checks the auto read-only threshold.** It refuses only a read-only Book
   (`gnc-sx-instance-model.c:1453-1457`); the oracle created transactions dated 2024 in a book whose threshold was
   30 days (§9.4).
4. **Formula functions are Guile procedures called with doubles.** `pmt(a : b : …)` calls the Guile procedure
   `gnc:pmt`, every argument converted to a C `double`, and the result is converted back by keeping 12 significant
   figures, half up (`5.17:libgnucash/app-utils/gnc-exp-parser.c:288-360`). Arguments are separated by `:`, not
   `,`. A zero interest rate makes `pmt` divide by zero and the formula fails (oracle §9.2).
5. **Variables are unbound until the user binds them, except `i`.** Every name in a template formula becomes a
   variable that Since Last Run must be given a value for; `i` is predefined as the instance count, read-only
   (`gnc-sx-instance-model.c:435-456`). Outside scheduled transactions, an unknown name is silently 0 (oracle §9.2:
   `a + b` = 0).
6. **`(5)` is −5.** An expression whose whole token string is `(number)` is negated, the accounting convention
   (`5.17:libgnucash/app-utils/calculation/expression_parser.c:609-620`); `(5) + 1` is 6.
7. **The exchange-rate variable means different things for currencies and for other commodities.** For a split in
   another currency the amount is `value × rate` rounded to 1/1000 first; for a stock it is `value ÷ rate` rounded to
   the commodity's fraction (`gnc-sx-instance-model.c:1218-1224`; oracle §9.3).
8. **`from-sched-xaction` holds the scheduled transaction's GUID,** although the property's own description says
   "the originating template transaction" (`5.17:libgnucash/engine/Transaction.cpp:455-464`,
   `gnc-sx-instance-model.c:1405-1409`; oracle §9.3).
9. **A scheduled transaction that ends is kept and stays enabled.** Nothing deletes or disables it; it just has no
   next occurrence (§4.9).
10. **SQL books lose deferred instances.** The `schedxactions` table has no column for them and no other table holds
    them (`5.17:libgnucash/backend/sql/gnc-schedxaction-sql.cpp:51-82`); only XML keeps them
    (`5.17:libgnucash/backend/xml/gnc-schedxaction-xml-v2.cpp:169-190`).
11. **A start date on a weekend with "back" adjustment skips the first occurrence.** Monthly from Saturday
    2025-05-31, adjusted back, first occurs on 2025-06-30, not Friday 2025-05-30 (§2.4; oracle §9.1).
12. **Opening a book in the GUI rewrites template numerics.** `gnc_sx_scrub_split_numerics` runs over every template
    split on open and sets `credit-numeric` / `debit-numeric` wherever the stored value differs from what the
    formula parses to (`5.17:gnucash/gnome-utils/gnc-file.c:1117-1131`,
    `gnc-sx-instance-model.c:113-174`).
13. **Schedule from transaction writes locale-formatted formulas,** with group separators and the locale's decimal
    point: `1,234.56` in `en_US` (`5.17:gnucash/gnome/dialog-sx-from-trans.cpp:238-250`). The formula text is
    therefore only readable in the same locale.
14. **Rounding can unbalance an instance;** the commit's scrub then adds an `Imbalance-<CUR>` split, creating the
    account if needed (oracle §9.3, "Round": `1.005` against `1`).
15. **The loan assistant writes locale-formatted formulas and rounds other payments to two significant figures:**
    `pmt( .06000 / 12.00 : 360.00 : 100,000.00 : 0 : 0 )` in `en_US`, and a 250.75 tax payment becomes `251`
    (§6.2). It writes no numerics, and the template currency is the default currency, not the loan's.

---

## 1. The engine model

### 1.1 Fields and defaults

`struct _SchedXaction` (`5.17:libgnucash/engine/SchedXaction.h:91-123`); defaults from `gnc_schedxaction_init`
(`5.17:libgnucash/engine/SchedXaction.cpp:71-88`) and `xaccSchedXactionInit` (`:384-407`):

| Field | Type | Default | Meaning |
|---|---|---|---|
| name | string | null | GUI: the dialogs set one; `xaccSchedXactionSetName` refuses null (`:545-558`). |
| schedule | list of Recurrence | empty | The Schedule. Empty means it never occurs (`5.17:libgnucash/engine/Recurrence.cpp:440-444`). |
| last_date | GDate | invalid | The last occurrence acted on (created, ignored or postponed, §4.5). |
| start_date | GDate | invalid | First possible occurrence. A null or invalid date is refused with `g_critical` (`:574-591`). |
| end_date | GDate | invalid | Invalid means no end. A date before the start is refused (`:625-648`). |
| num_occurances_total | int | 0 | 0 = no occurrence limit (`SchedXaction.h:104-105`). `xaccSchedXactionSetNumOccur` sets remaining to the same value (`:702-711`). |
| num_occurances_remain | int | 0 | Remaining as of `last_date`. `SetRemOccur` above total only warns and keeps the old value (`:719-737`). |
| instance_num | int | 0 | The instance count: +1 per instance acted on. The header calls 0 "the incorrect value" (`SchedXaction.h:214-217`); the GUI sets 1 for a new scheduled transaction (`5.17:gnucash/gnome/dialog-sx-editor.c:1546-1549`, `5.17:gnucash/gnome/dialog-sx-from-trans.cpp:484`). |
| enabled | bool | true | Disabled ones are left out of Since Last Run (§4.2). |
| autoCreateOption, autoCreateNotify | bool | false, false | The GUI stores notify only with auto-create (`dialog-sx-editor.c:976-985`). |
| advanceCreateDays, advanceRemindDays | int | 0, 0 | Days ahead to create and to remind (§4.2). |
| template_acct | Account | new account | Created by `xaccSchedXactionInit`, §1.2. |
| deferredList | list of SXTmpStateData | empty | Postponed instances, sorted by date (§1.4). |

Plus the QofInstance GUID and KVP Slots. An end date and an occurrence count can both be set in memory; the XML
writer writes the count and drops the end date in that case ([`xml-format-core.md`](xml-format-core.md) §8.2), and
`xaccSchedXactionGetNextInstance` tests only the end date when one is set (§1.5). The GUI sets one and clears the
other (`dialog-sx-editor.c:928-964`).

### 1.2 Template root, template accounts, template transactions

- **Template root:** every book gets an Account of type ROOT named `Template Root`
  (`5.17:libgnucash/engine/SX-book.cpp:113-125`), held apart from the Book's root.
- **Template account:** `xaccSchedXactionInit` creates one per scheduled transaction, named by the scheduled
  transaction's GUID as 32 hex digits, commodity `template:template`, type BANK, a child of the template root
  (`SchedXaction.cpp:393-406`). Since Last Run, the cash flow and the editor find it **by name** under the template
  root (`gnc_sx_get_template_transaction_account`, `gnc-sx-instance-model.c:378-388`), not through the
  `template_acct` field; the XML reader does the same when `sx:templ-acct` is missing (`xml-format-core.md` §8.2).
- **Template transactions:** ordinary Transactions whose splits all sit in the template account
  (`xaccSchedXactionSetTemplateTrans`, `SchedXaction.cpp:999-1029`): description, num, notes and currency from the
  caller, date posted = now (`:1015`), one split per template split with memo, action, the template account, and the
  `sched-xaction` frame (`pack_split_info`, `:966-996`). Their value and quantity stay 0 (the register writes
  value `0/1`: `5.17:gnucash/register/ledger-core/split-register-model-save.c:731-733`). A scheduled transaction
  can have several template transactions; Since Last Run creates one real transaction per template transaction, in
  the template account's split order (`xaccAccountForEachTransaction`,
  `5.17:libgnucash/engine/Account.cpp:5138-5144`).
- **Destroying** a scheduled transaction destroys its template transactions and template account
  (`SchedXaction.cpp:461-488`).
- **Accounts referenced by templates:** `gnc_sx_get_sxes_referencing_account` lists the scheduled transactions
  whose template splits name an account (`SX-book.cpp:364-389`).

### 1.3 The `sched-xaction` split frame

Keys (`5.17:libgnucash/engine/Split.cpp:69-75`), exposed as GObject properties (`:395-460`):

| Key | Slot type | Meaning |
|---|---|---|
| `account` | guid | The real Account the created split goes to. |
| `credit-formula` | string | Credit amount formula (§3). Empty = no credit. |
| `debit-formula` | string | Debit amount formula. |
| `credit-numeric` | numeric | The formula's value when it parses **without variables**, else 0 (`split-register-model-save.c:687-713`). A cache, used only when an instance has no variable bindings (§3.7). |
| `debit-numeric` | numeric | Same for debit. |
| `shares` | string | Written by the register as the constant phony text `(x + y)/42` (`split-register-model-save.c:740-748`, FIXME at `Split.cpp:440-444`); read by nothing in Since Last Run. Keep, never interpret. |

- A **marker split** has both formulas empty (`split_is_marker`, `gnc-sx-instance-model.c:200-219`). It is still
  created, with value 0 (oracle §9.3, "Marker"), but its commodity does not count when choosing the transaction's
  currency (§3.8).
- A transaction whose first split has a credit or debit formula slot (even empty) is a template:
  `xaccTransIsSXTemplate` (`Transaction.cpp:2350-2371`) exempts it from the read-only-by-date check (`:2385-2386`).

### 1.4 Temporal state and deferred instances

`SXTmpStateData = { last_date, num_occur_rem, num_inst }` (`SchedXaction.h:130-136`): the variable part of a
scheduled transaction, so instances can be generated without changing it.

- `gnc_sx_create_temporal_state` copies the three fields; **an invalid `last_date` becomes 1970-01-01**
  (`SchedXaction.cpp:1031-1042`).
- `gnc_sx_incr_temporal_state`: `last_date ← next instance`; `num_occur_rem -= 1` if the scheduled transaction has an
  occurrence count; `num_inst += 1` (`:1044-1054`).
- **Deferred instances** (postponed) are SXTmpStateData values: the state *before* the postponed instance. They are
  kept sorted by `last_date` (`gnc_sx_add_defer_instance`, `:1098-1104`); removal finds the first entry with an equal
  `last_date` (`:1110-1125`). The XML reader appends them in file order, assuming sorted (`gnc-schedxaction-xml-v2.cpp:540-541`);
  the writer omits `sx:last` when the date is invalid (`:180-184`) although the reader marks `sx:last` required
  (`:516`), so a deferred instance without it would fail to load. 5.17 never writes one: the state it defers always
  has a valid date (the 1970-01-01 rule above).

### 1.5 Next occurrence

`xaccSchedXactionGetNextInstance(sx, tsd)` (`SchedXaction.cpp:885-926`):

```
prev ← tsd ? tsd.last_date : invalid          // tsd = NULL ignores sx.last_date entirely
if start valid and (prev invalid or prev < start): prev ← start − 1 day
next ← recurrenceListNextInstance(schedule, prev)       // §2.5
if sx has an end date:          if next > end: next ← invalid
else if sx has a count:         if (tsd ? tsd.num_occur_rem : sx.rem) == 0: next ← invalid
```

So the first occurrence is the first recurrence date on or after the start date, and with a NULL state the result is
always that first occurrence (oracle §9.4: `GetNextInstance(NULL)` stays 2025-01-01 after three runs). Only the
count path reads the remaining occurrences of the scheduled transaction itself.

`gnc_sx_get_num_occur_daterange(sx, start, end)` (`:739-812`) counts occurrences in an inclusive range by stepping a
temporal state; used by the cash flow (§3.9). GnuCash's test `test_basic` checks it
(`5.17:libgnucash/app-utils/test/test-sx.cpp:74-81`).

---

## 2. Recurrence

### 2.1 Model and normalisation

`Recurrence = { start: GDate, ptype, mult: u16, wadj }` (`5.17:libgnucash/engine/Recurrence.h:76-82`). Period types
`once day week month end of month nth weekday last weekday year` (indices 0–7) and weekend adjustments
`none back forward` (`Recurrence.cpp:44-52`, `Recurrence.h:51-71`).

`recurrenceSet` (`Recurrence.cpp:87-139`) normalises:

- an invalid period type becomes `month`; `mult` = 0 for `once`, else `max(mult, 1)`;
- an invalid start becomes today;
- `end of month`: start moves to the last day of its month; `last weekday`: start moves forward by weeks to the last
  such weekday of its month; `nth weekday` in the fifth week (`(day−1)/7 == 4`) becomes `last weekday`;
- weekend adjustment is kept only for `month`, `end of month` and `year`, else `none`.

**The backends do not call it.** The XML reader sets the fields directly
(`5.17:libgnucash/backend/xml/gnc-recurrence-xml-v2.cpp:58-112`), so a file can hold a recurrence `recurrenceSet`
would never make (an `end of month` starting on the 15th, a fifth-week `nth weekday`, a weekly recurrence with
`back`). Next occurrences are then computed from the raw fields, with odd results (oracle §9.1: `last weekday`
stored with start 2025-01-10 gives 01-10, 01-17, 02-14 …). red-ale must compute from the stored fields too.

### 2.2 `recurrenceNextInstance(r, ref)` (`Recurrence.cpp:202-381`)

The next occurrence strictly after `ref`. Port this exactly, with GLib month arithmetic:

```
adjusted_start ← adjust_for_weekend(pt, wadj, start)
if ref < adjusted_start: return adjusted_start
next ← ref
# Step 1: forward one period
case YEAR: mult ← 12·mult, then as MONTH
case MONTH, NTH_WEEKDAY, LAST_WEEKDAY, END_OF_MONTH:
  if r.wadj == BACK and pt ∈ {YEAR, MONTH, END_OF_MONTH} and next is Sat/Sun:
      next ← preceding Friday
  if r.wadj == BACK and pt ∈ {YEAR, MONTH, END_OF_MONTH} and next is Friday:
      sat, sun ← next+1, next+2
      if pt == END_OF_MONTH:
          next += mult months if next, sat or sun is last of month, else mult−1 months
      else if day(sat) == day(start):  next += 1 day,  then mult months
      else if day(sun) == day(start):  next += 2 days, then mult months
      else if day(next) >= day(start) or next is last of month: next += mult months
      else if sat is last of month: next += 1 day,  then mult months
      else if sun is last of month: next += 2 days, then mult months
      else next += mult−1 months
  else if next is last of month
          or (pt ∈ {MONTH, YEAR} and day(next) >= day(start))
          or (pt ∈ {NTH, LAST}_WEEKDAY and nth_weekday_compare(start, next, pt) <= 0):
      next += mult months
  else next += mult−1 months
case WEEK: mult ← 7·mult, then as DAY
case DAY: next += mult days
case ONCE: return invalid
# Step 2: back up to the phase of start
MONTH-like: n ← 12·(year(next)−year(start)) + month(next)−month(start); next −= (n mod mult) months
            if pt ∈ {NTH, LAST}_WEEKDAY: next += nth_weekday_compare(start, next, pt) days
            else if pt == END_OF_MONTH or day(start) >= days_in_month(next): day(next) ← last day
            else day(next) ← day(start)
            next ← adjust_for_weekend(pt, wadj, next)
WEEK/DAY:   next −= (days_between(start, next) mod mult) days
```

Note the `wadj == BACK` tests in step 1 read `r->wadj`, the others read the local copy; they are the same value.
`mult` is the stored multiplier: an unnormalised `mult` of 0 for a non-`once` type would make `n mod mult` divide by
zero (C undefined behaviour). red-ale should refuse to compute with it and keep the file unchanged.

`nth_weekday_compare(start, next, pt)` (`:148-173`) returns the offset in days from `next` to the matching weekday in
`next`'s month:

```
week ← min(day(start) / 7, 3);  if week > 0 and day(start) mod 7 == 0 and day(start) != 28: week −= 1
matchday ← 7·week + (day(next) − weekday(next) + weekday(start) + 7) mod 7      # GLib weekday: Mon=1 … Sun=7
if pt == LAST_WEEKDAY and days_in_month(next) − matchday >= 7: matchday += 7
if pt == NTH_WEEKDAY and matchday mod 7 == 0: matchday += 7
return matchday − day(next)
```

### 2.3 End of month and month-day clamping

- `month` keeps the start's day where the month has it and uses the last day otherwise; the day comes back in longer
  months: monthly from 2025-01-31 gives 01-31, 02-28, 03-31, 04-30 … (oracle §9.1). The start's day, not the
  previous occurrence's, decides (step 2).
- `end of month` always takes the last day.
- `year` from 2024-02-29 gives 2025-02-28 … 2028-02-29.
- `nth weekday` uses the start's week number capped at the fourth; fifth-week starts are normalised to
  `last weekday` by `recurrenceSet`.

### 2.4 Weekend adjustment

`adjust_for_weekend(pt, wadj, date)` (`:176-196`) applies only to `year`, `month` and `end of month`: on Saturday,
`back` subtracts 1 day and `forward` adds 2; on Sunday, `back` subtracts 2 and `forward` adds 1. Applied to the
start (`:223-229`) and to every computed date (`:369-370`).

Because the scheduled transaction asks for the first occurrence after *start − 1 day* (§1.5), a start on a Saturday
or Sunday with `back` has an adjusted start *at or before* that reference, so the first occurrence is skipped: monthly
from Saturday 2025-05-31 with `back` gives 2025-06-30 first, and yearly from Saturday 2025-03-01 gives 2026-02-27
first (oracle §9.1, and §9.4 for the whole scheduled transaction). With `forward` the first occurrence is the adjusted
start (2025-06-02).

### 2.5 Lists of recurrences

- `recurrenceListNextInstance(list, ref)` (`:432-460`): the earliest of each recurrence's next instance; an empty
  list gives invalid. The GUI uses two monthly recurrences for "semi-monthly" (`recurrenceListIsSemiMonthly`,
  `:555-582`) and one weekly recurrence per weekday for "weekly on several days"; oracle §9.4 ("Two": the 1st and 15th
  interleave, each counted as one instance).
- `recurrenceNthInstance(r, n)` (`:384-395`) iterates from the start (zero-based); `recurrenceGetPeriodTime`
  (`:397-418`) gives period boundaries. Budgets use these, not scheduled transactions.

### 2.6 Worked sequences (oracle §9.1, `recurrenceSet` then repeated `recurrenceNextInstance` from start − 1)

| Recurrence | Occurrences |
|---|---|
| month ×1 from 2025-01-31 | 01-31 02-28 03-31 04-30 05-31 06-30 … 2026-02-28 |
| month ×2 from 2025-01-31 | 01-31 03-31 05-31 07-31 09-30 11-30 2026-01-31 |
| end of month from 2025-01-15 (normalised to 01-31) | 01-31 02-28 03-31 04-30 |
| nth weekday from 2025-01-08 (2nd Wed) | 01-08 02-12 03-12 04-09 05-14 |
| nth weekday from 2025-01-29 (5th Wed → last weekday) | 01-29 02-26 03-26 04-30 05-28 |
| nth weekday from 2025-01-28 (4th Tue) | 01-28 02-25 03-25 04-22 05-27 |
| last weekday from 2025-01-10 (normalised to 01-31, last Fri) | 01-31 02-28 03-28 04-25 05-30 |
| month from 2025-03-01, back | 04-01 05-01 05-30 07-01 … (03-01 is a Saturday: skipped) |
| month from 2025-03-01, forward | 03-03 04-01 05-01 06-02 … |
| end of month from 2025-01-31, back | … 05-30 06-30 07-31 08-29 … |
| end of month from 2025-01-31, forward | … 04-30 06-02 06-30 07-31 09-01 … |
| week ×2 from 2025-01-01 | 01-01 01-15 01-29 02-12 |
| day ×3 from 2025-01-30 | 01-30 02-02 02-05 |
| once on 2025-03-15 | 03-15, then none |

### 2.7 GnuCash's tests

- `5.17:libgnucash/engine/test/test-recurrence.c`: `test_some` (`:361-399`) checks 21 fixed cases
  `(type, mult, start, ref) → next` with weekend adjustment `none`, including years 1–1340 (e.g. yearly ×3 from
  0838-09-08, ref 1094-06-30 → 1096-09-08; monthly from 0001-01-30, ref 0001-02-27 → 0001-02-28; end of month ×5
  from 0001-02-28, ref 0001-05-21 → 0001-07-31). `test_all` (`:212-252`) runs every type × weekend adjustment × 300
  start dates × 300 random references × multipliers 0–9 through `check_valid` (`:33-202`): the result is strictly
  after the reference, and its phase (day of month, weekday and week of month, end of month, or days modulo the
  period) agrees with the start, allowing the weekend moves. A port should run the same property over the same
  ranges, with the oracle as the reference for exact dates.
- `5.17:libgnucash/app-utils/test/test-sx.cpp`: instances of a daily scheduled transaction from yesterday to
  tomorrow (`test_basic`, `:37-85`); none for an empty book (`:87-101`); a once-only one (`:103-144`); the state-change
  rule (`test_state_changes`, `:152-223`, §4.3); auto-create with values `123`, `0`, empty formulas, and empty
  formulas with a template currency, each creating exactly one transaction (`:299-358`).
- There is no `test-schedxaction.c` in 5.17. `5.17:libgnucash/backend/dbi/test/test-dbi-stuff.cpp:96-140` compares
  scheduled transactions and recurrences after an SQL round trip (not deferred instances), and
  `5.17:libgnucash/backend/xml/test/test-files/load-save/bug-746937-sx-split-order.gnucash` is a load-save fixture
  for template split order.

---

## 3. Formulas and variables

### 3.1 Grammar (`5.17:libgnucash/app-utils/calculation/expression_parser.c`)

Recursive descent (`:871-1270`): `assignment := add_sub ( ('=' | '+=' | '-=' | '*=' | '/=') assignment )*`;
`add_sub := mul_div (('+'|'-') mul_div)*`; `mul_div := primary (('*'|'/') primary)*`;
`primary := '(' assignment ')' | ('+'|'-') primary | number | name | name '(' [assignment (':' assignment)*] ')' | '"' text '"'`.

- Operators are `+-*/()=:` (`:449`); an operator followed by `=` is a compound assignment (`:785-801`).
- A name is `[A-Za-z_][A-Za-z0-9_]*`; a name immediately followed by `(` is a function call (`:818-852`).
  `pmt (…)` with a space is a variable followed by a parenthesis, a grammar error.
- Strings are `"…"` and may only be function arguments (`:1252-1264`).
- Two operands in a row are an error (`check_expression_grammar_error`, `:1083-1096`): `1 2`, `1 asdf`.
- Assignment needs a variable on the left (`:949-954`) and returns the value, so `jkl = 5 * (nj = 68.9)` works.
- `(I)` — the whole expression a single parenthesised number — is negated (`:609-620`).
- An assignment to a function argument binds it: `sub( a = 42 : … ) + a` (GnuCash test,
  `5.17:libgnucash/app-utils/test/test-exp-parser.c:213`).

### 3.2 Numbers and arithmetic (`5.17:libgnucash/app-utils/gnc-exp-parser.c`)

Numbers are read with `xaccParseAmount` in the locale's monetary decimal point and group separator
(`trans_numeric`, `:362-381`; `:522-526`). Arithmetic is exact: `gnc_numeric_add/sub/mul/div` with `GNC_DENOM_AUTO`
and `GNC_HOW_DENOM_EXACT` (`numeric_ops`, `:383-421`); the result is reduced (`:546`). Division by zero is a numeric
error. red-ale's port (`crates/red-ale-import/src/business/expr.rs`) already matches this part.

### 3.3 Variables

- **Predefined variables** come from two places, searched first (`get_named_var`, `expression_parser.c:677-705`):
  the caller's hash (for scheduled transactions, the instance's bindings) and the parser's global table, which
  `gnc_exp_parser_init` loads from the user's `expressions-2.0` key file and saves back on shutdown
  (`gnc-exp-parser.c:62-164`). The caller's entries are prepended, so they shadow the global ones (`:515-520`,
  `:209-238`). Whether the global table is loaded depends on who initialised the parser first (`:508-509`: a call
  with a variable hash initialises it *without* the file). red-ale should ignore the global table (it is per-user
  state, not the book's) and say so as a deviation.
- **An unknown name is created with value 0** (`expression_parser.c:691-702`). `gnc_exp_parser_parse` accepts the
  result unless a variable's value is the numeric error `GNC_ERROR_ARG` (`gnc-exp-parser.c:436-443`, `:476-483`), so
  for a plain expression unknown names are 0.
- `gnc_exp_parser_parse_separate_vars(expr, &v, &err, hash)` (`:492-601`) returns every named variable the expression
  created or assigned in `hash`, without touching the global table.

### 3.4 Function calls

`func_op` (`gnc-exp-parser.c:288-360`):

1. evaluate the Guile expression `gnc:<name>` (`:299-310`); if it is not a procedure, the call fails;
2. convert each argument: numeric → `scm_from_double(gnc_numeric_to_double(n))` (`:318-321`; `gnc_numeric_to_double`
   is `(double)num / (double)denom`, `5.17:libgnucash/engine/gnc-numeric.cpp:1346-1352`), string → Guile string;
3. apply through `gnc:apply-with-error-handling` (`5.17:bindings/guile/c-interface.scm:72-73`); a Scheme error or a
   non-number result fails the call (`:336-344`);
4. convert the result with `double_to_gnc_numeric(d, GNC_DENOM_AUTO, GNC_HOW_DENOM_SIGFIGS(12) | GNC_HOW_RND_ROUND_HALF_UP)`
   (`:346-357`): first `GncNumeric(double)` takes `floor(d · 10^k)` over `10^k` with `k = 19 − floor(log10|d|)` for
   |d| > 1, else 18 (`5.17:libgnucash/engine/gnc-numeric.cpp:87-119`; NaN or |d| > 10^18 fails), then that is rounded
   half up to 12 significant figures (`sigfigs_denom`, `:467-482`). A NaN or infinite result fails.

Any Guile procedure whose name starts `gnc:` can be called this way, so in the GUI a formula could reach any loaded
GnuCash Scheme function. The parser loads `gnucash/app-utils/fin` on init (`gnc-exp-parser.c:84-85`), installed from
`5.17:bindings/guile/fin.scm` (`5.17:bindings/guile/CMakeLists.txt:213-221`); those are the functions formulas are
meant to call (§7).

A failed call reports `Stack underflow` rather than `Not a defined function`, because the parser's end-of-input
check overwrites the error code (`expression_parser.c:1231-1236`, `:623-633`; oracle §9.2). Only the failure matters.

### 3.5 Variables in scheduled transactions (`5.17:libgnucash/app-utils/gnc-sx-instance-model.c`)

- `gnc_sx_get_variables(sx, hash)` (`:390-395`, helper `:313-376`) parses every non-empty credit and debit formula of
  every template split with the parser and collects the variables it names (`gnc_sx_parse_vars_from_formula`,
  `:234-263`). For each non-marker split whose account commodity differs from the transaction currency (§3.8) it adds
  an **exchange-rate variable** named `"<split mnemonic> -> <txn mnemonic>"`, e.g. `EUR -> USD` (`:300-311`,
  `:363-372`).
- When the first instance of a scheduled transaction is generated, every collected variable's value is set to the
  numeric error `GNC_ERROR_ARG`, i.e. unbound (`_wipe_parsed_sx_var`, `:194-198`, `:435-441`). Each instance gets
  its own copy, plus **`i` = the instance's `num_inst`**, not editable (`:443-456`). So the first instance of a
  scheduled transaction created by the GUI has `i = 1`; one with instance count 0 in the file has `i = 0` (oracle §9.3,
  "Count").
- Variables assigned inside a formula (`x = 5`) are variables too, and must be bound like any other; an assignment
  binds the name only for the rest of that one formula, since each formula is parsed separately with a copy of the
  bindings (`gnc_sx_instance_get_variables_for_parser`, `:221-232`).
- `gnc_sx_instance_model_check_variables` lists the unbound variables of every *to create* instance (`:1617-1650`);
  the GUI refuses OK until each is bound (`5.17:gnucash/gnome/dialog-sx-since-last-run.c:1563-1594`). Values are
  only ever numerics (`gnc-sx-instance-model.h:62-67`).
- The editor refuses auto-create for a scheduled transaction with variables other than `i`, or with more than one
  commodity (`dialog-sx-editor.c:617-634`, `:882-884`). A book can still hold one (oracle §9.3, "AutoVar"), and then
  every run creates zero transactions (Fact 2).

### 3.6 Amounts when a transaction is created

For each template split (`create_each_transaction_helper`, `gnc-sx-instance-model.c:1310-1422`):

1. **Formula values** (`_get_sx_formula_value`, `:1087-1148`): if the instance has *no* variable bindings and the
   stored numeric is valid and non-zero, use the stored numeric (`:1104-1118`). Every Since Last Run instance has at
   least `i`, so there the formula is always parsed; the shortcut serves only the cash flow (§3.9). An empty formula
   gives 0. A parse failure is reported and leaves the value 0 (`:1127-1139`).
2. **Value** = debit − credit, exact (`gnc_numeric_sub_fixed`; with `GNC_DENOM_AUTO` and `RND_NEVER` it stays exact
   across denominators, oracle §9.2), or 0 with an error if that fails (`:1169-1194`). `xaccSplitSetValue` rounds it
   HALF_UP to the transaction currency's fraction, and keeps the old 0 if a non-zero value rounds to 0
   (`5.17:libgnucash/engine/Split.cpp:1280-1300`).
3. **Amount (quantity):** if the account's commodity is the transaction currency, `xaccSplitScrub` sets amount =
   value (`5.17:libgnucash/engine/Scrub.cpp:236-347`). Otherwise (`split_apply_exchange_rate`, `:1196-1231`):
   rate = the exchange-rate variable, or 1 if absent; for a **non-currency** commodity
   `amount = round(value ÷ rate, commodity fraction)`; for a **currency** `amount = round(value × rate, 1000)`; then
   `xaccSplitSetAmount` rounds HALF_UP to the account's SCU (`Split.cpp:1243-1266`). Oracle §9.3: 100 USD into EUR at
   `EUR -> USD = 0.9` gives quantity −90.00 EUR; 100 USD of ACME at `ACME -> USD = 3` gives 33.3333 shares.
4. The commit scrubs the transaction (`Transaction.cpp:1549-1558`): if rounding left it unbalanced, an
   `Imbalance-<CUR>` split is added. The splits are then sorted debits first (`:1508-1510`, `:541-553`).

### 3.7 The transaction currency (`get_transaction_currency`, `:1245-1308`)

From the template transaction's currency `T` and its non-marker splits' account commodities: the first currency
among the splits wins if `T` is in none of the splits or `T` is not a currency; else the first split commodity if
`T` is in none of them; else `T`; with no template currency and only marker splits, the first split's commodity. An
unknown split account aborts the instance (`:1269-1298`, `:1060-1085`).

### 3.8 What is copied from the template

`xaccTransCloneNoKvp` (`Transaction.cpp:605-633`, `Split.cpp:598-630`) copies num, description, currency and, per
split, memo, action, reconcile state and date, value and amount, but **no Slots**; notes (a Slot) are copied
explicitly (`gnc-sx-instance-model.c:1337-1339`). Then (§4.6) date, date entered, account, value and amount are set.

### 3.9 Cash flow for forecasts

`gnc_sx_all_instantiate_cashflow_all(start, end)` (`:1968-1976`), used by the Account Summary's scheduled-transaction
option and the Balance Forecast (`5.17:gnucash/report/reports/standard/account-summary.scm:311`,
`5.17:gnucash/report/reports/standard/balance-forecast.scm:202`, `:217`): for every **enabled** scheduled
transaction (`:1902-1907`) with `n = gnc_sx_get_num_occur_daterange(start, end) > 0`, each template split adds
`round(n × (debit − credit), denom(debit − credit))` to its account (`:1793-1886`). Formulas are evaluated with **no
variable bindings**, so the stored numeric is used when non-zero, else the formula is parsed with every variable (and
`i`) as 0; a split in another commodity than the first split's contributes 0 with an error (`:1869-1878`). Sums are
exact (`GNC_DENOM_AUTO | REDUCE | RND_NEVER`, `:1767-1773`). This matters for the v0.4 reports.

---

## 4. Since Last Run (`5.17:libgnucash/app-utils/gnc-sx-instance-model.c`)

### 4.1 Instance states

`GncSxInstanceState` (`5.17:libgnucash/app-utils/gnc-sx-instance-model.h:52-60`): `IGNORED`, `POSTPONED`,
`TO_CREATE`, `REMINDER`, `CREATED` (and `MAX_STATE`); GUI names *Ignored*, *Postponed*, *To-Create*, *Reminder*,
*Created* (`dialog-sx-since-last-run.c:367-375`). The user can pick any of the first four (`:380-396`). Each instance
keeps its original state, its current state, its date, its temporal state (the state before it) and its variable
bindings (`gnc-sx-instance-model.h:69-77`).

### 4.2 Building the proposal (`_gnc_sx_gen_instances`, `:482-556`)

For each scheduled transaction (only enabled ones unless `include_disabled`, `:602-638`), with `range_end` = today
(local date, `gnc_sx_get_current_instances`, `:593-600`):

```
creation_end ← range_end + advanceCreateDays
remind_end   ← creation_end + advanceRemindDays          # reminders count from creation_end
state ← temporal state of the scheduled transaction (§1.4)
for each deferred instance d (date order):
    emit POSTPONED at GetNextInstance(sx, d), with temporal state d
    state ← d; incr(state)                                # <- the duplication bug, §4.8
date ← GetNextInstance(sx, state)
while date valid and date <= creation_end: emit TO_CREATE (state); incr; date ← next
while date valid and date <= remind_end:   emit REMINDER (state);  incr; date ← next
```

Auto-create scheduled transactions produce `TO_CREATE` instances like any other; auto-create is applied at the next
step. `gnc_sx_get_select_instances` (the editor's "since last run for these") adds one `REMINDER` for the next
occurrence when nothing is due (`:558-583`).

### 4.3 The state rule

Reminders are not recorded (only the last occurrence and the deferred list are), so a reminder may not precede any
other state (`gnc-sx-instance-model.h:138-158`). `gnc_sx_instance_model_change_instance_state` (`:1556-1596`): setting
a non-reminder state turns every *earlier* reminder into postponed; setting reminder turns every *later* instance
into reminder. Tested by `test_state_changes` (`test-sx.cpp:152-223`).

### 4.4 Applying (`gnc_sx_instance_model_effect_change`, `:1445-1554`)

Nothing happens if the Book is read-only (`:1453-1457`). For each scheduled transaction with at least one instance
(none: its fields are left alone so the book is not dirtied, `:1467-1471`), starting from its last occurrence,
instance count and remaining occurrences:

- with `auto_create_only`, instances of non-auto-create scheduled transactions are skipped, and the walk stops at the
  first one that is not `TO_CREATE` (`:1483-1491`);
- an instance that was `POSTPONED` and now is not is removed from the deferred list (`:1493-1500`);
- `CREATED`: nothing. `IGNORED`: advance. `POSTPONED` (newly): add a copy of its temporal state to the deferred list,
  then advance. `TO_CREATE`: create its transactions (§4.6); on success advance and mark `CREATED`; on error keep it
  and pass the errors up. `REMINDER`: nothing (`:1502-1544`);
- **advance** (`increment_sx_state`, `:1042-1058`): last occurrence ← the instance date if later; instance count ←
  the instance's `num_inst + 1` (overwritten, not maximised); remaining ← remaining − 1 if > 0;
- finally the three values are written back, each setter skipping an unchanged value (`:1550-1552`).

### 4.5 The GUI

- **On open:** a `HOOK_BOOK_OPENED` handler (`dialog-sx-editor.c:1893-1899`) runs
  `gnc_sx_sxsincelast_book_opened` when the preference `dialogs.sxs.since-last-run.show-at-file-open` is true
  (default true) (`dialog-sx-since-last-run.c:1477-1484`, `dialog-sx-since-last-run.h:38-40`,
  `5.17:gnucash/gschemas/org.gnucash.GnuCash.dialogs.sxs.gschema.xml.in:15-19`). So by default **GnuCash changes a
  book on open**: auto-create instances are created at once.
- **Every run** (`sx_since_last_run_dialog`, `:1417-1469`): auto-create instances are created first
  (`effect_change(auto_create_only = TRUE)`); the dialog opens if any instance is not auto-create-without-notify
  (`need_dialog`, `gnc_sx_instance_model_summarize`, `gnc-sx-instance-model.c:1652-1702`), else an information
  message says how many were created, shown on open only when `show-notify-window-at-file-open` (default true,
  gschema `:20-24`). In the dialog, OK checks variables, then applies every state (`:1550-1617`); creation errors are
  shown in one message (`:884-900`). "Review created transactions" (pref `review-transactions`, default false) opens
  a register of them (`:1604-1608`).
- red-ale is headless and never runs this on open ([`../milestone4.md`](../milestone4.md), "Out").

### 4.6 The transactions created (`create_each_transaction_helper`, `:1310-1422`; oracle §9.3)

| Field | Value |
|---|---|
| GUID | new |
| Date posted | the instance date, as a date: `xaccTransSetDate` → `xaccTransSetDatePostedGDate` (`Transaction.cpp:1913-1950`), so 10:59:00 UTC plus the `date-posted` gdate Slot |
| Date entered | now (`:1346`) |
| Currency | §3.7 |
| Num, description | the template transaction's (`Transaction.cpp:605-633`) |
| Notes | the template's notes Slot, if any (`:1337-1339`) |
| Slots | `date-posted` (gdate), `notes` if copied, and **`from-sched-xaction` = GUID of the scheduled transaction** (`:1405-1409`). Nothing else: the template's other Slots are not copied |
| Splits | one per template split, in template order, then sorted debits first by the commit; memo, action and reconcile state from the template split; account from `sched-xaction/account`; value and amount from §3.6; no Slots, no lot |

Template transactions dated now and with value 0 are untouched. The created GUIDs are returned to the caller
(`:1414-1419`).

### 4.7 The scheduled transaction afterwards (oracle §9.3–§9.5)

`sx:last` = the latest date acted on; `sx:instanceCount` = last acted instance's count + 1; `sx:rem-occur` reduced by
one per instance advanced; the deferred list gains newly postponed and loses re-decided instances. Nothing else
changes: not `enabled`, not the template.

### 4.8 Quirks a faithful port inherits (oracle §9.4, §9.5)

1. **Duplicates after a deferral.** With a deferred instance, *to create* instances are generated from the deferred
   state + 1, so instances created after the postponed one are proposed again on every later run, until the deferred
   instance is decided. Postpone Jan, create Feb; next month the run offers Jan (postponed), Feb and Mar.
2. **Postponing consumes an occurrence; creating it later consumes another.** Count 5: postpone Jan → 4 remaining;
   create the postponed Jan → 3, though only one transaction exists.
3. **The instance count is overwritten,** so acting only on an old postponed instance can lower it.
4. **A postponed instance set to Reminder is dropped:** it leaves the deferred list and is never proposed again.
5. **Errors do not stop creation** (Fact 2).
6. **The auto read-only threshold is not checked** (Fact 3).

### 4.9 When a scheduled transaction ends

With an end date, occurrences after it are invalid; with a count, none once remaining is 0 (§1.5). The scheduled
transaction is kept, enabled, with its template; no code in 5.17 deletes or disables it (the only callers of
`xaccSchedXactionDestroy` and `SetEnabled` are the user's own actions:
`5.17:gnucash/gnome/gnc-plugin-page-sx-list.cpp:978`, `:1029-1031`, `dialog-sx-editor.c:973`, `:1122`,
`5.17:gnucash/gnome-utils/gnc-tree-view-sx-list.c:183`). Oracle §9.4 ("Ends"): enabled `y`, `rem-occur 0`.

---

## 5. Schedule from transaction (`5.17:gnucash/gnome/dialog-sx-from-trans.cpp`)

Refused for a transaction being edited (`:348-351`, `:771-779`). It builds:

- **Name:** the transaction's description, editable (`:361-368`, `:453`).
- **Frequency:** a combo of Daily, Weekly, Bi-Weekly, Monthly, Quarterly, Yearly (`:70-77`;
  `5.17:gnucash/gtkbuilder/dialog-sx.glade:122-139`), default Daily (`:422`), giving one Recurrence: day ×1, week ×1
  or ×2, month ×1, ×3 or ×12 — Yearly is **month ×12**, not `year` — all with weekend adjustment `none`
  (`:278-327`; for months it passes the adjustment of a zeroed Recurrence, i.e. none, `:318`). The recurrence's
  start is the chosen start date.
- **Start date:** the first occurrence after the transaction's date for the chosen frequency, recomputed whenever the
  frequency changes (`:417-430`, `:567-586`). The scheduled transaction's start date is the same date (`:455-462`).
- **End:** never (default, `dialog-sx.glade:309-316`), on a date, or after *n* occurrences (`:154-203`, `:464-482`;
  `SetNumOccur` also sets remaining to *n*).
- **Instance count 1** (`:484`); auto-create, notify, days to create and days to remind from the editor's
  preferences `create-auto` (false), `notify` (true; stored only with auto-create), `create-days` (0),
  `remind-days` (0) (`:486-507`; gschema `:47-83`).
- **One template transaction** (`sxftd_add_template_trans`, `:206-275`): description, num
  (`gnc_get_num_action(trans, NULL)`, i.e. the transaction num), notes and currency of the transaction; per split, in
  order: memo, action (`gnc_get_num_action(NULL, split)`, the split action), the split's account, and **the split's
  value as text**: a positive value as the debit formula, otherwise (zero included) its negation as the credit
  formula, printed with `xaccPrintAmount(…, gnc_default_print_info(FALSE))` (`:227-256`): no symbol, the locale's
  monetary separators with grouping, at least the locale's `frac_digits` decimals and never rounded
  (`5.17:libgnucash/app-utils/gnc-ui-util.cpp:833-859`, `:1083-1200`). Example in `en_US`: a split of value
  −1234.5 gets credit formula `1,234.50`. The numerics (`credit-numeric`, `debit-numeric`) are **not** set by this
  path (`pack_split_info` sets only formulas and account, `SchedXaction.cpp:986-993`), so they stay absent until the
  next GUI open scrubs them (Fact 12).
- **Balance:** the values are summed at denominator 100 with LCD (`:235-236`); if not zero the user is asked
  (`:258-268`). The template is written either way.
- **Advanced…** opens the full editor on the same new scheduled transaction (`:589-612`).

The source transaction is not changed and gets no Slot.

---

## 6. Mortgage and loan assistant (`5.17:gnucash/gnome/assistant-loan.cpp`)

`AL` below is that file. The oracle build has no GUI (`WITH_GNUCASH=OFF`), so nothing here was run through the
assistant itself; items marked **[inferred]** come from reading the code only.

### 6.1 Inputs

- **Info page:** the loan Account (LIABILITY only, required: `AL:566-567`, `:598`, `:1060-1063`); the principal (a
  `GNCAmountEdit` in the loan account's commodity; an empty field is 0 and accepted, `AL:1066-1075`); the interest
  rate in percent (spin box, 3 decimals, 0.001–100, stored as a C `float`: `AL:194`, `:698-703`, `:1094`; the
  tooltip says zero-interest loans are not supported, `5.17:gnucash/gtkbuilder/assistant-loan.glade:339`); the rate
  type, `IRateType` (`AL:166-175`): 0 = "Interest Rate" (`GNC_IRATE_SIMPLE`, default), 1–6 = APR compounded daily,
  weekly, monthly, quarterly, semi-annually, annually; the loan type (fixed, 3/1 … 10/1 ARM) and a rate-change
  frequency, **saved but never used** when the scheduled transactions are made (`AL:965-971`, `:1096-1104`); start
  date (default today); length in months or years (1–9999); months remaining, recomputed from the start date and
  today when either changes (`AL:1011-1034`).
- **Options page:** an optional escrow Account (any type, required if ticked: `AL:729-735`, `:1296-1314`) and four
  fixed "other payments" (`REPAY_DEFAULTS`, `AL:123-132`): *Taxes* ("Tax Payment", via escrow by default),
  *Insurance* ("Insurance Payment", via escrow), *PMI* ("PMI Payment", via escrow), *Other Expense* ("Miscellaneous
  Payment", not via escrow), each off with amount 0 by default (`AL:938-950`).
- **Repayment page:** name (default "Loan", `AL:923`); a read-only display of the payment formula (`AL:1345-1351`);
  Payment From (BANK, CASH, CREDIT, ASSET, LIABILITY), Principal To (default: the loan account) and Interest To
  (those plus EXPENSE), all required (`AL:550-605`, `:1089-1092`, `:1397-1420`); the repayment frequency (default
  monthly ×1 from today, no weekend adjustment, `AL:916-921`).
- **Payment-options page,** per enabled option: memo, amount (shown `%0.2f`, read back with `strtod` into a `float`,
  both locale-dependent, `AL:1482`, `:1775`), optional source Account, Payment To (required), and frequency "Part of
  Payment Transaction" or "Other" (`AL:1645-1658`). Its "use escrow" box only changes sensitivity (`AL:1567-1597`,
  `:1761-1815`), and `loan_pay_all_opt_valid` returns the validity of the last enabled option only (`AL:1737-1757`).
- No Account is created or changed except through the selectors' "New account" dialog.

### 6.2 The scheduled transactions (`loan_create_sxes`, `AL:2838-3046`)

All are made with `xaccSchedXactionMalloc` and keep its defaults (§1.1): enabled, no auto-create, 0 days in
advance, no occurrence count. Template transactions are in `gnc_default_currency()`, not the loan account's
commodity (`AL:2868`, `:2928`, `:3015`, `:3018`); template splits get `account` and one formula only, **no
numerics** (`SchedXaction.cpp:986-993`; `5.17:libgnucash/engine/SX-ttinfo.hpp:73-84`).

**The repayment** (`AL:2850-2985`): name = the repayment name; start = the loan start date; last occurrence =
repayment start − 1 month; end = repayment start + (months remaining − 1) months; instance count = total months −
months remaining + 1, so `i` is 1 for a new loan and 13 after twelve payments (`AL:2850-2865`). Both are counted in
months whatever the payment frequency. Template "*name* - Payment" with, in order: Payment From **credit** `pmt(…)`
(memo "*name* - Payment"), Principal To **debit** `ppmt(…)` ("*name* - Principal"), Interest To **debit**
`ipmt(…)` ("*name* - Interest") (`AL:2870-2985`). With an escrow account the first template becomes "*name* - Escrow
Payment" (From credit and Escrow debit of `pmt(…)`, memo "*name*") and a second template "*name* - Payment" moves it
from escrow to principal and interest (`AL:2877`, `:2903-2935`).

**Formula text** (`AL:2440-2528`), `boost::locale::format` templates:

| Rate type | pmt | ppmt / ipmt |
|---|---|---|
| Interest Rate | `pmt( {1} / {2} : {3} : {4} : 0 : 0 )` | `ppmt( {1} / {2} : i : {3} : {4} : 0 : 0 )`, same for `ipmt` |
| APR | `pmt( {1} : {2} : {3} : 0 : 0 )` | `ppmt( {1} : i : {2} : {3} : 0 : 0 )`, same for `ipmt` |

`{1}` = annual rate as a fraction (`rate / 100`) to 5 decimals, `{2}` = payments per year to 2 decimals, `{3}` =
periods to 2 decimals, `{4}` = principal to 2 decimals (`AL:2446`, `:2492-2496`). For APR, `{1}` is the per-period
effective rate `(1 + rate/m)^(m/ppy) − 1` computed in C with `pow` and returned as a `float`
(`loan_apr_to_simple_formula`, `AL:2296-2314`, `:2455-2473`), printed to 5 decimals. Payments per year sum over the
recurrences: year 1/mult, week 52/mult, the month types 12/mult, day `365/mult` (integer division, assigned not
added), once 0 (`AL:2403-2430`). Periods = `(int)(months × ppy / 12)`, at least 1 (`AL:2433-2435`): the **full
original term and principal**; payments already made enter only through `i`. `fv` and `type` are literal `0`.

**Numbers are printed in the locale** (`to_str_with_prec`, `AL:2334-2395`): with a C or POSIX locale (no currency
symbol), `std::fixed`; otherwise `std::put_money` with the locale's monetary punctuation and grouping. Worked example
(100000 at 6 %, "Interest Rate", 30 years, monthly), as the subagent reproduced by compiling `to_str_with_prec`:

```
en_US:  pmt( .06000 / 12.00 : 360.00 : 100,000.00 : 0 : 0 )
        ppmt( .06000 / 12.00 : i : 360.00 : 100,000.00 : 0 : 0 )
        ipmt( .06000 / 12.00 : i : 360.00 : 100,000.00 : 0 : 0 )
C:      pmt( 0.06000 / 12.00 : 360.00 : 100000.00 : 0 : 0 )
de_DE:  pmt( ,06000 / 12,00 : 360,00 : 100.000,00 : 0 : 0 )        [inferred]
```

GnuCash's parser accepts a leading decimal point and skips group separators in its own locale
(`5.17:libgnucash/app-utils/gnc-ui-util.cpp:1777-1838`), so the text parses where it was written; in another locale
it may not.

**Other payments** (`ld_setup_repayment_sx`, `AL:2615-2819`). The amount text is
`xaccSPrintAmount(double_to_gnc_numeric(amount, 100, SIGFIGS(2) | HALF_UP), gnc_default_price_print_info)`
(`AL:2686-2699`): **two significant figures**, so 250.75 → `251`, 1234.56 → `1,235`, 5.67 → `5.7`. "Part of Payment
Transaction" appends ` + amt` to the From credit (or adds a From split if a source is given) and adds a To **debit**
`amt` with the option's memo; via escrow, the deposit goes into escrow in the first template and the escrow → To
transfer into the second (`AL:2701-2818`). An option with frequency "Other" becomes its own scheduled transaction
"*name* - *option*" (start = loan start, last = repayment start, end = repayment start + months remaining months,
instance count from `ld_calc_sx_instance_num`) with two template transactions, one always empty (`AL:2991-3025`,
`:3014-3019`, `:2586-2590`); with escrow its deposit still goes into the repayment, at the repayment's frequency.
Creation order: the repayment, then the options in order (`AL:3027-3040`).

### 6.3 Defects to know about (all **[inferred]** unless cited as observed)

1. The repayment start date is `g_date_new()` (invalid) and is set only when the user touches the repayment
   frequency (`AL:924`, `:1318-1363`, `:1425-1446`); left untouched, the last occurrence and end date are invalid, so
   the loan schedule never ends.
2. `ld_calc_sx_instance_num` steps from an unchanging start date (`AL:2532-2553`) and can loop forever.
3. Options with "via escrow" default on keep it when no escrow account is set (`AL:1180-1203`); with their own
   frequency or a source account, their From split lands in the repayment and their To split in their own
   scheduled transaction: both unbalanced (`AL:2800-2804`).
4. APR rates rounded to 5 decimals per period change the payment noticeably: 6 % APR compounded daily, paid
   monthly, gives `.00501` instead of 0.0050121, payment 600.32 instead of 600.48.
5. The review page's preview computes from the loan start with `i` = 1, 2 … and rounds to 1/100 whatever the
   currency (`AL:2057-2235`); it is not what Since Last Run will create for a loan already under way.
6. `pmt` and `ipmt + ppmt` differ by about 10⁻¹⁰ after the 12-figure conversion (599.550525153 against
   500 + 99.5505251528); the split values round to the same cents in the worked example, but nothing guarantees it,
   and an unbalanced instance gets an Imbalance split (Fact 14).

---

## 7. Financial functions

### 7.1 What exists (`5.17:bindings/guile/fin.scm`, GPL-2.0-or-later, Copyright 2002 Joshua Sled, 2012 Frank H. Ellenberger)

Callable from formulas without the `gnc:` prefix:

| Function | Source | Maths (all in Guile inexact reals, i.e. IEEE doubles) |
|---|---|---|
| `foobar(v)` | `:27` | returns `v` (test helper) |
| `pmt(rate : nper : pv : fv : type)` | `:49-51` | `−calc_pmt`; `calc_pmt = (−pv·(1+r)^n − fv) / (fvifa · (1.0 + r·type))`, `fvifa = ((1+r)^n − 1)/r` (`:75-90`). Positive for a loan (pv > 0). |
| `ipmt(rate : per : nper : pv : fv : type)` | `:34-39` | `−(r · −principal(pv, calc_pmt, r, min(per, nper) − 1))`, `principal(pv, p, r, k) = pv(1.0+r)^k + p((1+r)^k − 1)/r` (`:92-97`) |
| `ppmt(rate : per : nper : pv : fv : type)` | `:42-46` | `−(calc_pmt + ipmt)`; so `pmt = ipmt + ppmt` up to rounding |
| `futureValue(a : r : n : t)` | `:56-64` | `a(1 + r/n)^(n·t)` |
| `computeInterestIncrement(pv : ann_rate : compounds : period)` | `:66-68` | `(r/c) · pv · (1 + r/c)^(period−1)` |
| `periodic_rate(rate : yper : ycomp)` | `:110-112` | `(1.0 + rate/ycomp)^(ycomp/yper) − 1.0` |
| `cpd_pmt(rate : yfreq : ycomp : nper : pv : fv : type)` | `:176-184` | `0.01 · round(−100 · calc_pmt(periodic_rate(…), …))` |
| `cpd_ipmt(rate : yfreq : ycomp : per : nper : pv : fv : type)` | `:152-162` | `0.01 · round(−100 · r · −principal(pv, calc_pmt, r, per−1))`, `r = periodic_rate(…)` |
| `cpd_ppmt(…same as cpd_ipmt…)` | `:164-174` | `−(−cpd_pmt + r · principal(pv, −cpd_pmt, r, per−1))` (unrounded interest) |
| `amort_balance(py : cy : iy : pv : pmt : n : places)` | `:300-309` | iterated balance: each period `bal += roundToPlaces(bal · periodic_rate(iy, py, cy), places)`, then `bal = max(0, bal − pmt)`; `pv < 0` → 0; `n < 0` → `#f` (fails) |
| `amort_pmt`, `amort_ppmt`, `amort_ipmt` (same arguments) | `:316-337` | the n-th payment, its principal and its interest from the iterated table; `n < 1` → `#f` |

`roundToPlaces(v, places)` = `round(v · 10^places) / 10^places`, `places = 999` for none (`:362-366`). The `type`
argument (0 = end of period, 1 = beginning) only enters `calc_pmt`'s denominator.

### 7.2 Floating point, precisely

Everything is double: the arguments arrive as doubles (§3.4) and the Scheme literals `1.0`, `0.01` keep them inexact.
Two Guile details decide the low bits (Guile 3.0.9 behaviour, observed with the oracle's `libguile-3.0.so.1` and the
`guile` binary; Guile's source is not in the GnuCash tree):

- **`expt` with an integer-valued exponent** (an exact integer, or a double such as `360.0`) uses
  square-and-multiply, not C `pow`: `(expt 1.005 360.0)` is `6.022575212262888`, `pow(1.005, 360.0)` is
  `6.0225752122629865` (§9.6). A non-integer exponent (`periodic_rate`'s `ycomp/yper`) uses `pow`.
- **`round` is round-half-to-even** on doubles: `(round 2.5)` = 2.0, `(round 0.5)` = 0.0 (§9.6). So `cpd_*` and
  `amort_*` round ties to even, unlike GnuCash's HALF_UP everywhere else.

Then the result passes through the 12-significant-figure, half-up conversion (§3.4), and the split value through
HALF_UP to the currency fraction (§3.6). Example (oracle §9.2): `pmt(0.005 : 360 : 100000 : 0 : 0)` → Guile
`599.550525152759` → `599550525153/1000000000` → value 599.55. The exact rational is `599.55052515275239…`.

### 7.3 The financial calculator (`5.17:libgnucash/app-utils/calculation/fin.c`)

Terry Boldt's financial-calculator engine (`fin.c:1-60`): solves n, interest, PV, PMT and FV with compounding and
payment frequencies, discrete or continuous compounding, payments at the beginning or end. All `double`. Payment
(`_fi_calc_payment`, `fin.c:1418-1434`): `−(fv + pv(A+1)) / (A·B)` with `A = (1+e)^n − 1`, `B = (1 + e·bep)/e`, and
`e = eff_int(nint/100, CF, PF, disc)` = `nint/CF` if `CF == PF`, else `(1 + nint/CF)^(CF/PF) − 1`, continuous
`exp(nint/PF) − 1` (`:1492-1513`); results rounded by printing with `%.*f` and reading back with `strtod`
(`:1214-1225`, `:1402-1410`). Only the Financial Calculator dialog calls it
(`5.17:gnucash/gnome/dialog-fincalc.c:328`, `:454-466`); neither formulas nor the loan assistant do —
`gnc-exp-parser.c` includes `finproto.h` only for the parser's own prototypes
(`5.17:libgnucash/app-utils/calculation/finproto.h:106-110`). The Financial calculator is ❌ in `features.md`, so
nothing here needs porting.

### 7.4 Reproducing the figures without floats

Each function call is a boundary: arguments become doubles, the result a 12-significant-figure numeric. A port has
two ways to give GnuCash's figures (*Proposed*, for "Decide before code" item 6 of `milestone4.md`):

1. **Exact rationals, then GnuCash's two roundings.** Compute the function exactly (for `pmt`, `ipmt`, `ppmt` with a
   whole number of periods, `(1+r)^n` is a rational: `(201/200)^360` has about 830-digit terms, so it needs big
   integers), round half up to 12 significant figures, then to the SCU. This differs from GnuCash only when the exact
   value and Guile's double lie on different sides of a 12-significant-figure rounding point *and* that changes the
   rounded cent, which needs the value within about 10⁻¹⁰ relative of a half-cent. The port can detect that case
   (bound the double's error, here a few units in the 15th digit) and report it rather than guess. Functions with a
   non-integer power (`periodic_rate`, `cpd_*`, `amort_*` with `cy ≠ py`) have irrational results; an exact port
   needs an interval method, and `cpd_*`/`amort_*` add half-even ties on doubles.
2. **Emulate the doubles bit for bit in software.** IEEE-754 `+ − × ÷` are correctly rounded and can be done with
   integer arithmetic (a soft-float library), and Guile's `expt` is square-and-multiply, so `pmt`, `ipmt`, `ppmt`
   reproduce Guile exactly with no hardware float. `pow` is glibc's, which is not specified to the last bit, so the
   `periodic_rate` family can only be emulated up to glibc's own error. This keeps `clippy::float_arithmetic`
   satisfied (no `f64` operations) but is float semantics in disguise, against the spirit of "Money is exact".

The amounts that reach the book are rounded to the SCU in both, so either matches GnuCash for every realistic loan;
hand-worked cases confirm a table of them (§9.2 records the values GnuCash gave, observed before the no-Guile decision).

---

## 8. The SQL backend, briefly

`schedxactions` columns (`5.17:libgnucash/backend/sql/gnc-schedxaction-sql.cpp:58-82`): `guid`, `name`, `enabled`,
`start_date`, `end_date`, `last_occur`, `num_occur`, `rem_occur`, `auto_create`, `auto_notify`, `adv_creation`,
`adv_notify`, `instance_count`, `template_act_guid`; recurrences in `recurrences` keyed by `obj_guid`
(`5.17:libgnucash/backend/sql/gnc-recurrence-sql.cpp:48-110`). No deferred instances (Fact 10).

---

## 9. Oracle experiments

**How.** The 5.17 Python bindings expose nothing for scheduled transactions: no `SchedXaction`, `Recurrence`,
instance model or expression parser (a scan of `gnucash_core_c` and `_sw_app_utils` finds no such names; the only
SWIG wrapper, `gnc_sx_all_instantiate_cashflow_all`, is Guile-only, `5.17:bindings/expressions.i:71`). They do give
the raw `QofSession*` and `QofBook*` (`int(session.instance)`), and the libraries export everything, so the runs used
`ctypes`: `libgnc-engine.so` (`recurrenceSet`, `recurrenceNextInstance`, `xaccSchedXactionGetNextInstance`,
`gnc_sx_*`), and `libgnc-expressions.so`, which in 5.17 holds the instance model and the expression parser
(`gnc_sx_get_instances`, `gnc_sx_instance_model_change_instance_state`, `…_set_variable`, `…_effect_change`,
`gnc_exp_parser_parse`). Setup: `gnc_set_current_session(session)` (the instance model uses the current book);
`scm_init_guile()`, `(use-modules (gnucash app-utils c-interface))` for `gnc:apply-with-error-handling`, and
`gnc_exp_parser_real_init(FALSE)`, with `GUILE_LOAD_PATH` and `GUILE_LOAD_COMPILED_PATH` pointing into
`~/.local/opt/gnucash-5.17`. Structs (`GncSxInstances`, `GncSxInstance`, `SXTmpStateData`, `GncSxVariable`,
`Recurrence`, `GDate` as 8 opaque bytes via GLib's accessors) were declared to match the 5.17 headers. Books were
built by appending scheduled transactions, template accounts and template transactions in 5.17's own XML form to a
copy of `fixtures/synthetic/rich.gnucash`, then opened, run with a chosen `range_end`, saved and read back. Version
checked as 5.17 first. All of it worked.

**Not for red-ale's tests.** These runs started Guile (`scm_init_guile`, the `c-interface` module, the parser's
Guile functions), and were made before the decision of 2026-10-27 that red-ale runs no Guile, in its tests either
(`CLAUDE.md`, "No Guile, ever"). Their results are recorded here as observations of GnuCash. Oracle tests may use
only calls that start no Guile: `recurrenceSet`, `recurrenceNextInstance`, `xaccSchedXactionGetNextInstance`, and
reading back the books red-ale saved. Anything that evaluates a formula (the expression parser, the instance
model's amounts, `fin.scm`) is tested with hand-worked cases citing the 5.17 lines they follow.

### 9.1 Recurrences

`recurrenceSet` then repeated `recurrenceNextInstance` from start − 1 (results in §2.6), plus unnormalised
recurrences set field by field: `end of month` with start 01-15 → 01-15, 01-31, 02-28 …; `nth weekday` stored in the
fifth week (start 2025-01-29) → 04-23 instead of 04-30; `last weekday` with start 01-10 → 01-10, 01-17, 02-14,
03-14 …; `week` with `back` → unaffected. `mult` 0 normalised to 1. A reference after the start (monthly from
2025-01-15, ref 2025-06-20) → 2025-07-15.

### 9.2 The expression parser (`gnc_exp_parser_parse`)

```
1 + 2 → 3/1        (5) → -5/1        (5) + 1 → 6/1       1,234.5 * 2 → 2469/1     a = 3 → 3/1
x + 2 → 2/1        a + b → 0/1       10 / 4 → 5/2        1 / 0 → error "Numeric error"
1 2 → error        pmt(0.005, 360, 100000, 0, 0) → error (',' is not the separator)
pmt(0.005 : 360 : 100000 : 0 : 0)        → 599550525153/1000000000
pmt( 0.06 / 12 : 360 : 100000 : 0 : 0 )  → 599550525153/1000000000
ipmt(0.005 : 1 : 360 : 100000 : 0 : 0)   → 500/1
ppmt(0.005 : 1 : 360 : 100000 : 0 : 0)   → 124438156441/1250000000   (99.5505251528)
ipmt(0.005 : 360 : 360 : 100000 : 0 : 0) → 1864274021/625000000      (2.9828384336)
ppmt(0.005 : 360 : 360 : 100000 : 0 : 0) → 596567686719/1000000000
pmt(0 : 12 : 1200 : 0 : 0)               → error (0/0 is NaN)
cpd_pmt(0.06 : 12 : 2 : 360 : 100000 : 0 : 0) → 29741/50 (594.82)
periodic_rate(0.06 : 12 : 2)             → 6173277539/1250000000000 (0.0049386220312)
amort_pmt(12 : 2 : 0.05 : 100000 : 4385 : 24 : 2)  → 219051/50 (4381.02, the smaller last payment)
amort_ipmt(12 : 2 : 0.05 : 100000 : 4385 : 9 : 2)  → 13971/50 (279.42, matching fin.scm's own table, :260)
futureValue(1000 : 0.05 : 12 : 10)       → 164700949769/100000000
foobar(1/3) → 333333333333/1000000000000     foobar(0.1) → 1/10     foobar(123456789.123456789) → 123456789123/1000
foobar("abc") → error (string result)        nosuch(1) → error      foobar(2) * 1.5 → 3/1
```

`gnc_numeric_sub(a, b, GNC_DENOM_AUTO, DENOM_FIXED | RND_NEVER)` (the `sub_fixed` of §3.6): 3/2 − 1/4 = 5/4,
1/3 − 1/7 = 4/21, 123/100 − 1/3 = 269/300: exact.

### 9.3 Since Last Run, creation (`slr1`: ten scheduled transactions, `range_end` 2025-02-12)

Proposal (excerpt): *Plain* (monthly from 2025-01-01, remind 20 days) → 01-01 and 02-01 *to create*, 03-01
*reminder*; every instance has `i` = its `num_inst` (1, 2, 3); *Vars* (`amt * 2 + i`) has `amt` unbound
(`-1/0` = `GNC_ERROR_ARG`); *Euro* has `EUR -> USD` and *Stock* `ACME -> USD`. After binding `amt = 25/2`,
`EUR -> USD = 9/10`, `ACME -> USD = 3/1`, applying auto-create first and then everything:

```
Plain  2025-01-01 10:59:00 +0000, entered = now, num "CHK", slots date-posted, from-sched-xaction=<SX guid>, notes "tmpl notes"
       Food 10000/100 memo "food memo" action "act1" | Bank -10000/100 memo "bank memo"
Vars   Food 2600/100 | Bank -2600/100 (i=1); next month 2700/100 (i=2)
Count  (instanceCount 0 in file) i = 0 for the first instance; num 3 rem 3 → rem 1, count 2 after two
Round  debit 1.005, credit 1:  Food 101/100 | Bank -100/100 | Imbalance-USD -1/100   (account created)
Third  10/3 each side:          Food 333/100 | Bank -333/100
Marker Food 500/100 | Food 0/100 memo "marker" | Bank -500/100
Euro   Food 10000/100 | Euro Bank value -10000/100 quantity -9000/100
Stock  ACME value 10000/100 quantity 333333/10000 | Bank -10000/100
AutoNotify  created by the auto-create pass (7.00 each)
AutoVar     (auto-create, formula "x", x unbound) errors "Error parsing SX [AutoVar] key [sx-debit-formula]=formula [x]
            at [x]: Numeric error." — and two zero transactions per pass, four in all; SX not advanced
```

Created splits carry no Slots; reconcile state `n`.

### 9.4 Since Last Run, schedule state (`slr2`, book threshold 30 days)

```
run1 (to 2025-02-15): Defer (count 6) Jan POSTPONED, Feb TO_CREATE → last 02-01, count 3, rem 4, deferred [(1970-01-01, 6, 1)]
     Ends (count 2) both created → rem 0, GetNextInstance invalid, enabled stays y
     Old (start 2024-01-01, last 2024-06-01) → 2024-07-01 … 2025-02-01 all created despite the 30-day threshold
     Weekend (monthly from Sat 2025-05-31, back) → next 2025-06-30
     Two (1st and 15th) → 01-01, 01-15, 02-01, 02-15
run2 (to 2025-03-15): Defer proposes Jan POSTPONED, Feb TO_CREATE (again), Mar TO_CREATE → both created, rem 1
run3 (to 2025-03-20): Defer proposes Feb and Mar again → created again, rem 0
run4 (to 2025-04-15): Defer's postponed Jan set IGNORED → deferred list empty; Feb, Mar, Apr created again
```

The saved Defer: `instanceCount 5`, `last 2025-04-01`, `num-occur 6`, `rem-occur 0`, and seven transactions from a
schedule limited to six occurrences, Feb and Mar three times each.

### 9.5 Since Last Run, postponing (`slr3`: monthly, count 5, instanceCount 3, remind 45 days)

```
to 01-15: Jan POSTPONED (Feb, Mar reminders)  → last 01-01, count 4, rem 4, deferred [(1970-01-01, 5, 3)]
to 01-20: postponed Jan → TO_CREATE           → created; count 4, rem 3, deferred []
to 01-20: Mar (a reminder) → TO_CREATE         → Feb reminder became POSTPONED; Mar created; count 6, rem 1
to 02-20: postponed Feb → REMINDER             → all later forced to REMINDER; Feb removed from the deferred list
```

### 9.6 Guile arithmetic

`(expt 1.005 360.0)` = `(expt 1.005 360)` = 6.022575212262888 (C `pow`: 6.0225752122629865; square-and-multiply in
doubles: 6.022575212262888); `(expt 1.005 (/ 1.0 6.0))` = 1.0008316025082902 = `pow`; `(round 2.5)` = 2.0,
`(round 0.5)` = 0.0. Emulating fin.scm's `gnc:pmt` with square-and-multiply doubles gives 599.550525152759, which
GnuCash's conversion turns into the 599.550525153 of §9.2.

---

## 10. Implications for red-ale (*Proposed*)

1. **Model.** `ScheduledTransaction` in the core holds every field of §1.1 as read, including the raw Recurrence
   fields, the deferred list in file order, and both an end date and a count if a file has both. Template accounts
   and template transactions are their own aggregate, never in the Account tree. The `sched-xaction` frame is a
   value object with its five keys plus `shares` kept as text.
2. **Recurrences** are ported, not redesigned (§2.2), computing from stored fields; `recurrenceSet`'s normalisation
   applies only to recurrences red-ale creates. A stored `mult` of 0 for a periodic type is refused for computation,
   not rewritten. The tests port `test_some` exactly and `test_all` as a property test over the same ranges, plus the
   §2.6 table, with the oracle (§9) producing the expected dates.
3. **Formulas.** Move `expr.rs` into the core and add assignments, compound assignments, `(n)` negation, strings as
   function arguments and `:`-separated calls. Variables come only from the instance's bindings; the user's global
   `expressions-2.0` table is not consulted (deviation). Functions: the fin.scm set of §7.1, implemented per §7.4;
   any other name is refused with a reason (GnuCash would look it up in Guile).
4. **Since Last Run** follows §4.2–§4.4 for the proposal and the apply, with three deviations to decide, each
   documented: (a) do **not** reproduce the deferral duplicates (§4.8.1): generate *to create* instances from the
   schedule's last occurrence and the deferred instances separately; (b) refuse an instance whose formulas fail,
   creating nothing, instead of creating zero transactions (§4.8.5); (c) keep GnuCash's occurrence counting
   (§4.8.2–3), since it is what the next GnuCash run will read. Whether (a) changes what GnuCash later shows needs an
   oracle check: GnuCash reading a book red-ale advanced must propose the same instances.
5. **Auto read-only threshold.** GnuCash ignores it in Since Last Run (Fact 3). Proposed: red-ale flags such instances
   in the proposal (as `milestone4.md` says) and refuses them by default, with the caller able to create them
   explicitly, since creating a transaction GnuCash's register then shows as read-only is surprising.
6. **Created transactions** carry exactly the fields of §4.6 and no others; `from-sched-xaction` is the scheduled
   transaction's GUID.
7. **Schedule from transaction** builds the template as §5 does but writes formulas in a fixed, locale-free form
   (`1234.50`, `.` decimal point, no grouping) and sets the numerics too. Deviation: GnuCash writes the locale's form;
   red-ale's is the form every locale's GnuCash parses with a `.` decimal point, which needs checking for `,`
   locales (open question 2).
8. **Never change a book on open.** GnuCash's open-time scrub of template numerics (Fact 12) is not done on open; a
   red-ale save leaves numerics as read.
9. **SQL books:** deferred instances cannot be stored; postponing in an SQLite book is refused (or the book opens
   read-only for Since Last Run) rather than silently lost, as GnuCash loses it.

---

## Open questions

1. **What GnuCash proposes after red-ale's deviations.** If red-ale avoids the deferral duplicates (§10.4a), the
   state it saves is the same fields GnuCash saves, but GnuCash's next run will still generate from the deferred
   state and propose duplicates. Can red-ale save a state GnuCash reads without duplicates (for example by creating
   later instances only after the deferred one is decided)? Needs an oracle experiment.
2. **Formula text in other locales.** How `xaccParseAmount` in a `,`-decimal locale reads `1234.50` and `1,234.50`
   (§10.7), and what red-ale should do with an existing locale-formatted formula from another locale: the book does
   not record the locale it was written in. Needs oracle runs with `LC_ALL=de_DE.UTF-8` (the locale must exist).
3. **What `scheduledTransaction.loan` writes.** GnuCash's text is locale-formatted (§6.2) and carries its defects
   (§6.3). Proposed: the same formula shapes (`pmt( r / ppy : n : pv : 0 : 0 )`, `i` for the period) in the C-locale
   form, the instance count and dates computed as GnuCash intends rather than as defect 1 leaves them, numerics set,
   other-payment amounts at full precision rather than two significant figures. Each departure is a deviation to
   list, and needs an oracle check that GnuCash computes the same repayments from red-ale's text.
4. **Int64 overflow** in exact formula arithmetic (`GNC_HOW_DENOM_EXACT` fails where GnuCash would return an error and
   use 0). Proposed: refuse, as `expr.rs` does.
5. **Template transactions in reports and queries.** GnuCash keeps them out by putting them under the template root;
   red-ale's core keeps them out by type. Check that GnuCash's Find never matches template splits (it searches the
   book's split collection, which includes them) before claiming parity for "no find query ever sees them".
6. **Unnormalised recurrences** (§2.1) in real books: only hand-edited or very old files hold them. Confirm that no
   5.17 code path other than the backends builds a Recurrence without `recurrenceSet`.
