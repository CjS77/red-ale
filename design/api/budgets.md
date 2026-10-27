# API: budgets

Budgets, their amounts and notes, estimating amounts from history, and the natural-signs scrub, as GnuCash 5.17 keeps
and edits them. Scope and decisions: [`../milestone4.md`](../milestone4.md) ("Budgets", "Compatibility in v0.4",
"Decided" items 7, 9 and 10). Research: [`../notes/engine-budgets.md`](../notes/engine-budgets.md) §1–§3 and §8; the
XML is in [`../notes/xml-format-core.md`](../notes/xml-format-core.md) §8.3–§8.4. Terms: [`../glossary.md`](../glossary.md).
Citations are to GnuCash 5.17 source.

GnuCash's budget code is split between the engine (`gnc-budget.cpp`, `ScrubBudget.c`) and the GUI's budget page and
plugin (`gnc-plugin-page-budget.cpp`, `gnc-plugin-budget.c`, `gnc-budget-view.c`). The GUI decides what users' books
hold (the Book feature, the copy's name, estimate), so red-ale follows the GUI where it adds rules to the engine and
says so. Where the GUI changes a book on open, red-ale does not: the change waits for an explicit call.

Code: `red_ale_core::budget` (the entity), `red_ale_core::ops::budgets` (the Mutations, periods, actuals, the
estimate and the scrub), `red_ale_rpc::methods::budgets` (the methods), and the `Recurrence` wire type in
`red_ale_rpc::wire`. Where what was built differs from this design as first written, the text says so ("As built").

## The model on the wire

A **Budget** has a GUID, a name, a description, a number of periods (its *Budget periods*) and a Recurrence that gives
their dates. Its amounts and notes live in its Slots (`5.17:libgnucash/engine/gnc-budget.cpp:494-508`):

| What | Slot | Type |
|------|------|------|
| The Budget amount of account *A* in period *p* (0-based) | `<A's GUID>/<p>` | numeric |
| The Budget note of account *A* in period *p* | `notes/<A's GUID>/<p>` | string |

A Budget amount is either **set** (a numeric Slot exists, even `0/1`) or **unset** (no Slot). Reads of an unset amount
give 0, but the reports' roll-up treats only unset parents as the sum of their children
([`../notes/engine-budgets.md`](../notes/engine-budgets.md) §1.2), so the API never confuses the two: `null` is unset,
`"0"` is a set zero. Amounts are stored as given: not rounded to the account's SCU and not reduced (`1234/100` stays
`1234/100`), as `gnc_budget_set_account_period_value` stores them (`gnc-budget.cpp:540-576`).

Every other Slot of a Budget is kept as read and written back unchanged: amounts and notes for periods at or beyond
the number of periods (hidden, below), amounts and notes of accounts no longer in the tree, a Slot of another type at
an amount path (which GnuCash reads as unset, `gnc-budget.cpp:691-692`), and any key red-ale doesn't know.

`Recurrence` is the wire type of [`../api.md`](../api.md) ("Wire types"). A Budget's Recurrence is never `once`, and
red-ale creates only Recurrences with weekend adjustment `none` (see "Recurrences").

### `BudgetInfo`

| Field | Meaning |
|-------|---------|
| `guid`, `name`, `description` | As stored |
| `numPeriods` | The number of periods |
| `recurrence` | `Recurrence`, as stored |
| `isDefault` | True for the default budget (see "The default budget") |
| `periods` | `[{ index, startDate, endDate }]`, one per period below `numPeriods` (see "Period dates") |
| `amounts` | `[{ account, accountName, period, amount }]`: every set amount of an account in the tree, for periods below `numPeriods`, by account full name then period |
| `notes` | `[{ account, accountName, period, note }]`, likewise |
| `hidden` | `{ amounts, notes }`: how many amount and note Slots of accounts in the tree are at or beyond `numPeriods`. Kept, never listed |
| `orphaned` | `{ amounts, notes }`: how many are for GUIDs that name no account in the tree. Kept, never listed |

As built, both count what would be listed if the count were raised or the account came back: numeric Slots for
amounts and non-empty string Slots for notes, at period keys written as GnuCash writes them (`std::to_string`, so
`015` is not a period). Any other Slot is kept and not counted.

`budget.list` returns each Budget without `periods`, `amounts` and `notes`.

### Period dates

Period *p* starts at local 00:00:00 of the Recurrence's instance *p* and ends at local 23:59:59 of the day before
instance *p*+1 (`gnc_budget_get_period_start_date` and `_end_date`, `gnc-budget.cpp:647-659`;
`recurrenceGetPeriodTime`, `5.17:libgnucash/engine/Recurrence.cpp:397-418`). Instance 0 is the Recurrence's start
itself, not weekend-adjusted; each later one is `recurrenceNextInstance` of the one before (`:384-395`). On the wire
they are Dates: `startDate` and `endDate` are the first and last local days of the period. The day boundaries are
those of [`../api.md`](../api.md) ("Dates, times and the local time zone").

A start day past a month's end clamps to its last day: monthly from 2024-01-31 gives 01-31..02-28, 02-29..03-30,
03-31..04-29 ([`../notes/engine-budgets.md`](../notes/engine-budgets.md) §1.4 lists more).

## Methods

| Method | Effect | Does |
|--------|--------|------|
| `budget.create` | write | Create a Budget, setting the natural-signs Book feature with the first |
| `budget.get` | read | One Budget with its periods, amounts and notes |
| `budget.list` | read | Every Budget, in book order, and which is the default |
| `budget.update` | write | Change the name, description, number of periods or Recurrence |
| `budget.copy` | write | Copy a Budget as GnuCash's Copy Budget does |
| `budget.delete` | destructive | Delete a Budget, unsetting the natural-signs Book feature with the last |
| `budget.setAmounts` | write | Set or unset many Budget amounts, atomically |
| `budget.setNotes` | write | Set or delete many Budget notes, atomically |
| `budget.estimate` | destructive | Fill accounts' amounts from past transactions, as GnuCash's Estimate dialog does |
| `budget.scrubSigns` | destructive | Run GnuCash's natural-signs scrub |

Every write and destructive method has Effect target `book`, is one Mutation, and is refused on a read-only book
(`read-only`). `budget` params are GUIDs: Budget names need not be unique.

The default budget is read and set through `book.options` and `book.setOptions` (see "The default budget").

### `budget.create` (write)

| Param | Type | Default |
|-------|------|---------|
| `name` | string | `Unnamed Budget` |
| `description` | string | `""` |
| `numPeriods` | integer, 1–60 | 12 |
| `recurrence` | Recurrence | every 1 month from the first of the current local month, weekend adjustment `none` |

→ `BudgetInfo`.

The defaults are `gnc_budget_init`'s (`gnc-budget.cpp:103-119`), with the name untranslated, as red-ale writes in the
C locale. GnuCash's New Budget also sets the description to `Created: <today in the locale's date format>`
(`5.17:gnucash/gnome/gnc-plugin-budget.c:191-194`); red-ale invents no text, and a client can send it. The Budget goes
after every other in book order.

Creating the first Budget of a book that lacks it sets the Book feature `Use natural signs in budget amounts`, as New
Budget does before creating the Budget (`gnc-plugin-budget.c:181-186`). The feature's description is GnuCash's
(`5.17:libgnucash/engine/gnc-features.cpp:45`).

Errors: `invalid` (`numPeriods` out of range, a `once` Recurrence, a weekend adjustment other than `none`), `conflict`
(`data.reason: "natural-signs"`, see "Natural signs").

### `budget.get`, `budget.list` (read)

`budget.get { budget, accounts? }` → `BudgetInfo`. `accounts` (AccountRef[]) limits `amounts` and `notes` to those
accounts. `not-found` for an unknown Budget or account.

`budget.list {}` →

| Field | Meaning |
|-------|---------|
| `budgets` | `BudgetInfo` without `periods`, `amounts` and `notes`, in book order: the order read, new Budgets last |
| `default` | The default budget's GUID, or null when the book has none |
| `defaultFrom` | `option` (the Book option names it) or `first` (the option is unset or stale, so the first Budget in book order is the default), or null |
| `naturalSigns` | Whether the Book has the feature `Use natural signs in budget amounts` |
| `needsSignScrub` | True when the book has Budgets and lacks the feature: edits are refused until `budget.scrubSigns` |

Both work on a read-only book and in a book that needs the scrub.

### `budget.update` (write)

`{ budget, name?, description?, numPeriods?, recurrence? }` → `BudgetInfo`. Absent params are left alone; the checks
are `budget.create`'s.

This is GnuCash's Budget Options dialog, which sets the name, description, number of periods and Recurrence, in that
order (`5.17:gnucash/gnome/gnc-plugin-page-budget.cpp:813-924`).

- **Lowering `numPeriods` hides, never deletes.** GnuCash only resizes its in-memory cache and keeps the Slots, which
  it writes on every save; reopening the book brings them back
  (`gnc_budget_set_num_periods`, `gnc-budget.cpp:472-480`; [`../notes/engine-budgets.md`](../notes/engine-budgets.md)
  §1.3). red-ale changes only `num-periods`: amounts and notes beyond the new count stay in the Slots, are counted in
  `hidden`, and are listed again when the count is raised. There is no way to drop them in v0.4, as GnuCash has none.
- **Changing the Recurrence moves no amount.** Amounts stay keyed by period index, as in GnuCash; only the period
  dates change.
- **Up to 60 periods.** The engine has no upper bound, but the Budget Options dialog's spin button holds 1 to 60
  (`5.17:gnucash/gtkbuilder/gnc-plugin-page-budget.glade:401-407`), puts the stored count into it (which clamps it,
  `gnc-plugin-page-budget.cpp:862`) and writes it back on OK (`:909-910`). A count above 60 would be lowered, hiding
  amounts, the next time a user pressed OK there. A book that already holds more is read and kept.

### `budget.copy` (write)

`{ budget }` → `BudgetInfo` of the copy.

As GnuCash's Copy Budget (`5.17:gnucash/gnome/gnc-plugin-budget.c:232-272`), which calls `gnc_budget_clone`
(`gnc-budget.cpp:354-383`):

- the copy has a new GUID and is named `Copy of <name>`, untranslated as GnuCash writes it;
- it copies the description, Recurrence and number of periods, and every set amount of an account in the tree for
  periods below the count;
- **notes are not copied**, nor hidden or orphaned amounts, nor any other Slot.

Rename it with `budget.update`. The copy goes last in book order.

### `budget.delete` (destructive)

`{ budget }` → `{ deleted: true, featureUnset: bool }`.

- The Budget and every Slot it holds go.
- **Deleting the last Budget unsets `Use natural signs in budget amounts`**, as GnuCash's budget page does after
  destroying it (`5.17:gnucash/gnome/gnc-plugin-page-budget.cpp:927-952`, `:944-948`). `featureUnset` says so.
- The Book option `Default Budget` is left alone, even when it names the deleted Budget: nothing in GnuCash clears it,
  and a stale GUID falls back to the first Budget ([`../notes/engine-budgets.md`](../notes/engine-budgets.md) §1.6).

Allowed in a book that needs the sign scrub: deleting changes no other Budget's signs.

### `budget.setAmounts` (write)

| Param | Type | Meaning |
|-------|------|---------|
| `budget` | GUID | |
| `amounts` | `[{ account, period, amount }]` | `account` an AccountRef, `period` 0-based, `amount` an Amount or null |

→ `{ budget, set, unset }`: how many cells were set, and how many set cells were unset (an unset cell asked to be
unset is not counted).

Every cell applies, or none does. Each is GnuCash's budget cell edit (`5.17:gnucash/gnome/gnc-budget-view.c:1258-1288`)
without the display sign:

- **An Amount sets the cell**, an explicit `"0"` included (stored `0/1`, or with the denominator given: `"0.00"` is
  `0/100`). It is stored exactly as given (`gnc_budget_set_account_period_value`, `gnc-budget.cpp:540-576`).
- **`null` unsets it**: the Slot is deleted (`gnc_budget_unset_account_period_value`, `:516-536`). Unsetting an unset
  cell does nothing.
- **Amounts are natural signs** (see "Natural signs"): an expense budget is positive, an income budget negative. The
  GUI negates typed input for accounts whose balance it shows reversed (`gnc-budget-view.c:1283-1286`); that
  preference is a display choice and belongs to clients.

Refused with `invalid`: a `period` at or beyond `numPeriods` (GnuCash warns and does nothing, `gnc-budget.cpp:549-553`),
the same cell twice in one call, the root account. `not-found` for an unknown Budget or account. A zero denominator
cannot arrive: the wire's Amount refuses it (`invalid-params`), where GnuCash would unset the cell.

Any account in the tree may have amounts, placeholders and hidden accounts included, as the budget view shows every
account. A template account of a scheduled transaction is not in the tree and resolves to nothing.

### `budget.setNotes` (write)

`{ budget, notes: [{ account, period, note }] }` → `{ budget, set, deleted }`: `deleted` counts the note Slots
removed.

As `gnc_budget_set_account_period_note` (`gnc-budget.cpp:599-636`), with the GUI's empty-text rule
(`gnc-plugin-page-budget.cpp:1295-1296`): a non-empty string sets the note; `""` or null deletes it. An empty-string
Slot already in a file is kept, and reads as no note, as GnuCash reads it (`gnc-budget.cpp:638-645`). Notes have no
length limit. The checks and atomicity are `budget.setAmounts`'.

## Estimate from history

`budget.estimate` is GnuCash's Estimate dialog (`5.17:gnucash/gnome/gnc-plugin-page-budget.cpp:955-1092`).

| Param | Type | Default | GnuCash |
|-------|------|---------|---------|
| `budget` | GUID | | |
| `accounts` | AccountRef[], at least one | | the view's selection; none is refused, "You must select at least one account to estimate." (`:1029-1041`) |
| `startDate` | Date | **required** (`invalid-params` when absent, as the schema requires it) | the dialog shows today the first time, then the last date used on that page (`:1052-1058`) |
| `significantDigits` | integer, 1–12 | 1 | `:1060-1062`; the spin button's range (`5.17:gnucash/gtkbuilder/gnc-plugin-page-budget.glade:5-11`) |
| `useAverage` | bool | false | `:1064-1065` |

→ `{ budget, amounts: [{ account, accountName, period, amount }] }`: every amount written.

**History periods.** The estimate's Recurrence is the Budget's (multiplier, period type, weekend adjustment) starting
at `startDate`, through `recurrenceSet`, so its phase fixes apply: an end-of-month Recurrence moves the start to the
month's end (`:1072-1077`). History period *i* is that Recurrence's period *i*, by the rule of "Period dates".

**Per period** (`:989-1005`), for each account and *i* in `0..numPeriods-1`: the account's change over history period
*i* is stored in Budget period *i*, whatever their dates.

**With `useAverage`** (`:972-988`): the account's change from the start of history period 0 to the end of history
period `numPeriods-1`, divided by `numPeriods`, is stored in every period.

**The change** is `xaccAccountGetNoclosingBalanceChangeForPeriod` (`5.17:libgnucash/engine/Account.cpp:3869-3878`), not
the actuals' function: at each bound, the account's no-closing balance strictly before it plus every descendant's,
each converted to the account's commodity at the nearest price before **that bound** and added with HALF_UP to the
account's fraction (`:3683-3695`, `:3746-3781`); then the two are subtracted. A foreign child's unchanged balance
therefore contributes its revaluation, as in GnuCash ([`../notes/engine-budgets.md`](../notes/engine-budgets.md)
§3.4, Fact 5). Closing transactions are left out. Signs are natural.

**Rounding** is `SIGFIGS(significantDigits)` with HALF_UP, ported exactly (`GncNumeric::sigfigs_denom`,
`5.17:libgnucash/engine/gnc-numeric.cpp:468-486`): never left of the decimal point, with an unreduced power-of-ten
denominator. With the default of 1, 1234.56 becomes `1235/1`, 5.6789 `6/1`, 0.567 `6/10`, and 1 becomes `10/10`. The
average is divided with the same rounding. A zero change is stored as a set zero, `0/1`.

**It overwrites.** Every period of every chosen account is written, set amounts included, so the Effect is
`destructive`. Choosing a parent and its child estimates both, each from its own subtree. An account given twice is
estimated once.

Errors: `invalid` (no account, `significantDigits` out of range, the root account),
`not-found`, `conflict` (`natural-signs`), `arithmetic` (a change or the average overflows: the whole estimate is
refused, where GnuCash skips the period or stores an error).

## Natural signs

**Natural signs** means a Budget amount carries the engine's sign, as actuals do: debits positive, so an expense
budget is positive, an income budget negative, a liability increase negative. Since GnuCash 3.8 amounts are stored
that way, and the Book feature `Use natural signs in budget amounts` says so (`5.17:libgnucash/engine/gnc-features.h:62`).
Before 3.8 the stored sign followed the user's "Reverse Balanced Accounts" preference of the day.

What GnuCash does ([`../notes/engine-budgets.md`](../notes/engine-budgets.md) §2):

- after every file open, read-only ones too, the GUI runs `gnc_maybe_scrub_all_budget_signs`
  (`5.17:gnucash/gnome-utils/gnc-file.c:753-793`, called at `:1183`): it guesses each Budget's old sign convention and
  flips accordingly, then sets the feature; with no Budgets it unsets the feature
  (`5.17:libgnucash/engine/ScrubBudget.c:193-216`);
- New Budget sets the feature; Delete Budget unsets it when none remain (above);
- nothing else does: the engine, the backends, the bindings and `gnucash-cli` never touch it.

What red-ale does ("Decided", item 7):

- **Never on open.** A book is read as stored. A book with Budgets and without the feature (written by GnuCash before
  3.8, or by the bindings) reads normally, and `budget.list` says `needsSignScrub: true`.
- **Edits wait for the scrub.** In such a book, `budget.create`, `budget.update`, `budget.copy`,
  `budget.setAmounts`, `budget.setNotes` and `budget.estimate` are refused with `conflict`, `data.reason:
  "natural-signs"`. A new natural-sign amount mixed into a Budget of unknown convention would make the scrub's guess
  wrong. `budget.get`, `budget.list`, `budget.delete` and `budget.scrubSigns` are allowed.
- **The feature is set with the first Budget and unset with the last**, as GnuCash's GUI does.
- **A book with the feature and no Budgets** keeps the feature until `budget.scrubSigns` or the next first Budget;
  GnuCash's GUI unsets it on its next open.

### `budget.scrubSigns` (destructive)

`{}` → `{ featureSet, featureUnset, budgets: [{ budget, policy, flipped }] }`.

GnuCash's scrub, ported exactly (`gnc_maybe_scrub_all_budget_signs`, `5.17:libgnucash/engine/ScrubBudget.c:193-216`):

1. No Budgets and the feature set: unset it (`:200-205`).
2. No Budgets, or the feature already set: nothing (`:207-208`).
3. Otherwise each Budget is scrubbed, then the feature is set (`:212-214`).

Each Budget (`heuristics_on_budget`, `:105-125`): for every account, the sum of its set amounts over periods below the
count, added with `gnc_numeric_add_fixed`, gives a sign (−1, 0, +1; 0 when the sum overflows, as
`gnc_numeric_compare` gives 0 for an error, `5.17:libgnucash/engine/gnc-numeric.cpp:837-842`), tallied by the
account's fundamental type (`xaccAccountTypeGetFundamental`, `5.17:libgnucash/engine/Account.cpp:4482-4513`). Then:

| Tallies | `policy` | Flipped (every set amount of every period below the count, `:158-167`) |
|---------|----------|---------|
| expense tally < 0 | `incomeExpense` | INCOME and EXPENSE accounts |
| else income tally < 0 | `none` | nothing |
| else | `creditAccounts` | LIABILITY, EQUITY and INCOME fundamentals (so CREDIT and PAYABLE too) |

Unset amounts stay unset. A Budget with no amounts, or only zeros, gets `creditAccounts` and its zeros stay zeros.
Hidden and orphaned amounts are not flipped, since GnuCash's scrub reads only accounts in the tree and periods below
the count. `flipped` counts the amounts negated.

One Mutation. It is destructive because it rewrites amounts on a guess; the caller should review the Budgets after,
as GnuCash's message asks ("Please review the budgets and amend signs if necessary.", `gnc-file.c:756-760`).

## The default budget

GnuCash's Book option `options/Budgeting/Default Budget` is a GUID Slot naming the Budget every budget report's
"Budget" option defaults to (`5.17:libgnucash/engine/gnc-optiondb.cpp:1223-1226`; `5.17:libgnucash/engine/qofbook.cpp:176-179`).
`gnc_budget_get_default` looks it up and, when it is unset or names no Budget, takes the first Budget the collection
yields, in hash order (`gnc-budget.cpp:720-752`).

- **Read** with `book.options`, which gains `defaultBudget`: the stored GUID, or null when the option is unset. A stale
  GUID is returned as stored. `budget.list` gives the default in effect (`default`, `defaultFrom`).
- **Set** with `book.setOptions { defaultBudget: <GUID> }`, which writes the GUID Slot as GnuCash's book options dialog
  writes a budget option (`gnc-optiondb.cpp:529-535`). It must name a Budget of the book (`not-found` otherwise). It
  cannot be unset, as the dialog's budget list has no empty choice; deleting the Budget leaves it stale.
- **Fallback:** the first Budget in book order, where GnuCash takes hash order. Book order is what red-ale keeps;
  GnuCash's order is not reproducible ([`../notes/engine-budgets.md`](../notes/engine-budgets.md) §1.6, Fact 11).

See [`book-options.md`](book-options.md).

## Not in the API

- **Actuals.** A Budget's actual figures per account and period (`gnc_budget_get_account_period_actual_value`,
  [`../notes/engine-budgets.md`](../notes/engine-budgets.md) §1.5) are a core read the budget reports use through the
  report library ([`../reports.md`](../reports.md)). A client gets them from the Budget Report (`report.run`), which
  returns exact figures, and balances from `account.balance`. No `budget.actuals` method in v0.4.
- **Period dates beyond the count.** `budget.get` lists the periods below `numPeriods`. The reports read later ones
  through the library, as GnuCash's reports do.
- **All Periods** (the budget page's Replace, Add and Multiply over every period,
  `gnc-plugin-page-budget.cpp:1094-1229`): a client does it with one `budget.setAmounts` call.
- **`Show extra account columns in the Budget View`**, a Book feature the GUI's budget options set for its own columns
  (`gnc-plugin-page-budget.cpp:902-907`): red-ale never sets it, and keeps it when read.

## Accounts and Budgets

Deleting an account that has amounts or notes in any Budget is refused (`account.delete`, `conflict`, the message
naming the Budgets; `red_ale_core::ops::accounts::check_budgets`), the rule of [`../milestone4.md`](../milestone4.md) ("Compatibility in v0.4"). GnuCash
keeps a deleted account's amounts in every Budget and writes them on every save
([`../notes/engine-budgets.md`](../notes/engine-budgets.md) §1.7); red-ale keeps such Slots when a file holds them
(`orphaned`) and creates none.

## Recurrences

A Budget's Recurrence is stored and computed as read, weekend adjustment included
([`../notes/engine-budgets.md`](../notes/engine-budgets.md) §1.4). A Recurrence red-ale writes goes through
`recurrenceSet`'s normalisation (`5.17:libgnucash/engine/Recurrence.cpp:87-139`), as the GUI's recurrence widget
builds one (`5.17:gnucash/gnome-utils/gnc-recurrence.c:258-326`), and is refused with `invalid` when:

- its period type is `once`: the widget offers day, week, month (with its end-of-month and weekday forms) and year
  (`gnc-recurrence.c:271-321`);
- its weekend adjustment is not `none`: the widget always writes `none` (`gnc-recurrence.c:326`), so the next Budget
  Options OK in GnuCash would reset it.

## Errors

Codes from [`../api.md`](../api.md); no new ones.

| Code | When |
|------|------|
| `no-book`, `read-only` | As everywhere |
| `not-found` | An unknown Budget or account (`data.kind`: `budget`, `account`) |
| `invalid` | Out-of-range `numPeriods`, `period` or `significantDigits`; a `once` Recurrence or a weekend adjustment; a cell twice; the root account; no accounts to estimate. A missing `startDate`, an amount as a JSON number or with a zero denominator, and a `significantDigits` that is not a small integer are `invalid-params` |
| `conflict` | `data.reason: "natural-signs"`: the book has Budgets and lacks the feature |
| `arithmetic` | An estimate overflowed |

## Methods and MCP

| Method | Effect (target) | MCP |
|--------|-----------------|-----|
| `budget.create`, `budget.update`, `budget.copy`, `budget.setAmounts`, `budget.setNotes` | write (book) | — |
| `budget.get`, `budget.list` | read | read only |
| `budget.delete`, `budget.estimate`, `budget.scrubSigns` | destructive (book) | destructive |

## CLI

```
red-ale exec --book books/home.gnucash budget.create name=2025 'recurrence:={"mult": 1, "periodType": "month", "start": "2025-01-01"}'
red-ale exec --book books/home.gnucash budget.setAmounts budget=0123456789abcdef0123456789abcdef \
    'amounts:=[{"account": "Expenses:Groceries", "period": 0, "amount": "450.00"}, {"account": "Expenses:Rent", "period": 0, "amount": null}]'
```

## Deviations from GnuCash

| What | GnuCash 5.17 | red-ale | Why |
|------|--------------|---------|-----|
| Natural-signs scrub | Guessed and applied by the GUI after every open, read-only ones too | Only by `budget.scrubSigns`; Budget edits refused until then | red-ale never changes a book on open |
| A book with the feature and no Budgets | Feature unset on the next GUI open | Kept until `budget.scrubSigns` or a new first Budget | As above |
| Number of periods | Any positive count in the engine; 1–60 in the dialog | 1–60 | The dialog clamps a larger count and writes it back on OK, hiding amounts |
| Weekend adjustment, `once` | Read and computed; never written by the GUI | Read and computed; refused on create and update | The next Budget Options OK in GnuCash would change them |
| Raising the count again after lowering it | Hidden amounts reappear only after a reload | They reappear at once | red-ale reads the Slots, not a cache |
| New Budget's description | `Created: <date in the locale>` | `""` unless given | No invented text |
| A zero denominator | Unsets the cell | Refused (`invalid-params`) | A malformed amount is an error, not an unset |
| Estimate's start date | Today the first time | Required | With no dialog, a default of today would overwrite every amount with zero |
| Estimate overflow | The period skipped, or an error stored | Refused whole | Every Mutation applies whole or not at all |
| Default budget fallback | First in hash order | First in book order | Deterministic |
| Deleting an account with budget amounts | Allowed; the amounts stay in every Budget | Refused, naming the Budgets | [`../file-fidelity.md`](../file-fidelity.md), "References to out-of-scope data" |
| Display sign | The GUI negates input and display for reverse-balance accounts | Natural signs on the wire | The preference is a display choice; clients apply it |
| Unsetting a cell that was never set | `KvpFrame::set_path` creates the account's frame on the way (`5.17:libgnucash/engine/kvp-frame.cpp:84-98`, `:133-141`), so an empty `<account-guid>` frame is written | Nothing changes | GnuCash reads the two books alike; an empty frame carries nothing |

## Tests

From [`../milestone4.md`](../milestone4.md), "Testing":

- **GnuCash's own tests, rebuilt**: `5.17:libgnucash/engine/test/utest-Budget.c` (amounts, notes, the period count) and
  `test-budget.scm`'s fixture for the reports, rebuilt by the oracle.
- **Unit and property tests**: set against unset against zero; stored denominators; hidden and orphaned Slots
  round-trip; `SIGFIGS` against the table in [`../notes/engine-budgets.md`](../notes/engine-budgets.md) §3.4; the
  scrub's four rows and its overflow case; the feature on first create and last delete; every refusal of a book that
  needs the scrub.
- **Oracle**: budgets read back through `ctypes` calls into `libgnc-engine.so` (the bindings have no budget
  functions, [`../notes/engine-budgets.md`](../notes/engine-budgets.md) §7): amounts with their stored denominators,
  notes, period dates, the default budget; GnuCash's own scrub on a copy gives what `budget.scrubSigns` gave; the
  estimate's figures against `xaccAccountGetNoclosingBalanceChangeForPeriod` and `gnc_numeric_convert` on the same
  book; the book declares only the features GnuCash would have set.

As built:

- **Unit tests**: `crates/red-ale-core/src/ops/budgets/tests.rs`: defaults and the feature, every refusal, set against
  unset against zero and stored denominators, notes, hidden and orphaned counts, copy, delete and the stale default,
  the scrub's four rows and its overflow case, period dates, and exp4's actuals and estimate figures, hand-worked from
  [`../notes/engine-budgets.md`](../notes/engine-budgets.md) §7.4, with the `SIGFIGS` table of §3.4.
- **API tests**: `crates/red-ale-rpc/tests/budgets.rs`; MCP hints in `crates/red-ale-mcp/tests/tools.rs`; the CLI
  examples above in `crates/red-ale/tests/cli.rs`.
- **Oracle**: `crates/red-ale-testkit/tests/oracle_budgets.rs` with `oracle/budgets.py`. GnuCash's Estimate dialog
  needs its GUI, so the oracle runs the dialog helper's engine calls on the same book
  (`xaccAccountGetNoclosingBalanceChangeForPeriod`, `gnc_numeric_convert` and `gnc_numeric_div` with `SIGFIGS`,
  `5.17:gnucash/gnome/gnc-plugin-page-budget.cpp:955-1005`) and compares every figure. Actuals are compared with
  `gnc_budget_get_account_period_actual_value` for every account and 14 periods (past the count too), and the scrub
  with `gnc_maybe_scrub_all_budget_signs`, which loads no Guile. `utest-Budget.c`'s cases are covered by the unit
  tests; the reports' `test-budget.scm` fixture belongs to the budget reports.
