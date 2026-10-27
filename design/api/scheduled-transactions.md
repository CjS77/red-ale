# API: scheduled transactions

Scheduled transactions, their Schedules and template transactions; formulas and variables; Since Last Run, which
creates the transactions that have fallen due; making a scheduled transaction from a transaction; and the mortgage and
loan assistant. Scope and decisions: [`../milestone4.md`](../milestone4.md) ("Scheduled transactions", "Compatibility
in v0.4", "Decided" items 6 and 9–12). Research: [`../notes/engine-scheduled-transactions.md`](../notes/engine-scheduled-transactions.md)
(here *the note*), whose §10 lists the implications this page adopts; the XML is in
[`../notes/xml-format-core.md`](../notes/xml-format-core.md) §8.1–§8.5. Terms: [`../glossary.md`](../glossary.md),
"Scheduled transactions". Citations are to GnuCash 5.17 source.

Since Last Run, Schedule from transaction and the loan assistant exist only in GnuCash's GUI, so their GUI code sets
what users' books hold, and red-ale follows it. Each GUI dialog becomes one or two calls, with the dialog's choices as
params. Where GnuCash's behaviour can damage a book silently, red-ale refuses or flags instead, and the deviation is
listed at the end.

Code (planned): `red-ale-core::schedule` (the entity, Recurrences, temporal state), `red-ale-core::formula` (the
expression parser moved from `red-ale-import`, with the financial functions), `red-ale-core::ops::scheduled`
(the Mutations, Since Last Run, the loan), `red-ale-rpc::methods::scheduled`.

## Contents

1. [The model on the wire](#the-model-on-the-wire)
2. [Methods](#methods): create, get, list, update, delete, occurrences
3. [Formulas](#formulas): grammar, numbers, variables, functions, evaluation
4. [Since Last Run](#since-last-run): the proposal, apply, discard, what is created, deviations
5. [Schedule from transaction](#schedule-from-transaction)
6. [The loan assistant](#the-loan-assistant)
7. [Errors](#errors), [effects](#methods-and-mcp), [deviations](#deviations-from-gnucash), [tests](#tests)

## The model on the wire

A **Scheduled transaction** holds every field GnuCash's XML writer writes
([`../notes/xml-format-core.md`](../notes/xml-format-core.md) §8.2; the note §1.1), as read. Its template accounts
and template transactions are their own aggregate, under the Template root, never in the Account tree: no balance,
report, find query, import match, scrub or export sees them ([`../milestone4.md`](../milestone4.md), "Compatibility
in v0.4").

### `ScheduledTransactionInfo`

| Field | Meaning |
|-------|---------|
| `guid`, `name` | As stored. Names need not be unique |
| `enabled` | Disabled ones are left out of Since Last Run and the forecasts |
| `startDate` | The first possible occurrence |
| `end` | `End` (below) |
| `lastOccurrence` | The last occurrence acted on (created, ignored or postponed), or null if never run |
| `instanceCount` | The Instance count: the value of `i` for the next instance |
| `autoCreate`, `notify` | Create due instances without asking; and say so (`notify` only with `autoCreate`) |
| `advanceCreateDays`, `advanceRemindDays` | Days ahead to create, and to remind after that |
| `schedule` | `Recurrence[]` ([`../api.md`](../api.md), "Wire types"), as stored |
| `nextOccurrence` | The next occurrence after `lastOccurrence` (or the first), or null when it has ended or never occurs |
| `deferredInstances` | `[{ lastOccurrence, remainingOccurrences, instanceCount }]`, the saved state before each postponed instance, in file order |
| `templateAccount` | The template account's GUID |
| `templateTransactions` | `TemplateTransaction[]`, in the template account's split order |
| `variables` | Every variable the formulas name, `i` and the exchange-rate variables, sorted bytewise |

`End` is one of:

| Form | Meaning | Stored as |
|------|---------|-----------|
| `{ "never": true }` | No end | neither an end date nor a count |
| `{ "date": "2025-12-31" }` | No occurrence after this date | `sx:end` |
| `{ "occurrences": 12, "remaining": 12 }` | At most this many occurrences in all, this many still to come | `sx:num-occur`, `sx:rem-occur` |

A file can hold both an end date and a count; `get` then shows both (`{ date, occurrences, remaining }`), the end date
decides as GnuCash's `xaccSchedXactionGetNextInstance` lets it (`5.17:libgnucash/engine/SchedXaction.cpp:885-926`),
and an update of `end` keeps one only, as the editor does (`5.17:gnucash/gnome/dialog-sx-editor.c:928-964`).

`TemplateTransaction`: `{ guid, description, num, notes, currency, splits: TemplateSplit[] }`.

`TemplateSplit`: `{ guid, account, accountName, memo, action, debitFormula, creditFormula, debitNumeric,
creditNumeric }`. `account` is the real account the created split goes to (the `sched-xaction/account` Slot). The
formulas are text; an empty or absent one means no amount on that side. The numerics are read only (see "Numerics").
A split with neither formula is a *marker split*: it is still created, with value 0
(`5.17:libgnucash/app-utils/gnc-sx-instance-model.c:200-219`, `:1169-1194`).

### What red-ale writes for a new scheduled transaction

As `xaccSchedXactionInit` and `xaccSchedXactionSetTemplateTrans` make them (the note §1.2):

- a **Template root** (type ROOT, named `Template Root`, commodity `template:template`) if the book has none
  (`5.17:libgnucash/engine/SX-book.cpp:113-125`);
- a **template account** per scheduled transaction, named by its GUID in 32 hex digits, type BANK, commodity
  `template:template`, under the Template root (`SchedXaction.cpp:393-406`);
- **template transactions** with the given description, num, notes and currency, posted now (`:1015`); each template
  split in the template account with value and amount zero, as the register writes them
  (`5.17:gnucash/register/ledger-core/split-register-model-save.c:731-733`), its memo and action, and the
  `sched-xaction` Frame: `account`, `credit-formula`, `credit-numeric`, `debit-formula`, `debit-numeric`
  (`SchedXaction.cpp:966-996`). The `shares` key the register writes as the phony text `(x + y)/42` is kept when read
  and never written new: nothing reads it (the note §1.3).

**Numerics.** Every formula red-ale writes gets its numeric as GnuCash's template register sets it: the formula's value
when it parses and names no variable, else `0/1` (`save_cell`, `split-register-model-save.c:687-713`). That is also
what GnuCash's open-time scrub would set (`gnc_sx_scrub_split_numerics`, `gnc-sx-instance-model.c:113-174`), so
opening the book in GnuCash changes nothing. Numerics read from a file are kept as read, even stale ones (the note
Fact 12): red-ale never scrubs on open.

## Methods

| Method | Effect | Does |
|--------|--------|------|
| `scheduledTransaction.create` | write | Create a scheduled transaction with its template transactions |
| `scheduledTransaction.get` | read | One scheduled transaction |
| `scheduledTransaction.list` | read | Scheduled transactions, in book order |
| `scheduledTransaction.update` | write | Change fields, the Schedule, the end or the template transactions |
| `scheduledTransaction.delete` | destructive | Delete it with its template account and template transactions |
| `scheduledTransaction.occurrences` | read | The next occurrence dates of a scheduled transaction, or of a Schedule not yet saved |
| `scheduledTransaction.fromTransaction` | write | GnuCash's Schedule from transaction |
| `scheduledTransaction.loan` | write | GnuCash's mortgage and loan assistant |
| `scheduledTransaction.sinceLastRun` | read | Propose the due instances: Since Last Run's first call |
| `scheduledTransaction.apply` | write | Apply a proposal with the caller's choices, as one Mutation |
| `scheduledTransaction.discard` | read | Drop a held proposal |

`scheduledTransaction` params are GUIDs. Every write and destructive method has Effect target `book`, is one Mutation
and is refused on a read-only book.

### `scheduledTransaction.create` (write)

| Param | Type | Default | GnuCash |
|-------|------|---------|---------|
| `name` | string, not empty | required | "Please name the Scheduled Transaction." (`dialog-sx-editor.c:504-515`) |
| `enabled` | bool | true | `gnc_schedxaction_init` (`SchedXaction.cpp:71-88`) |
| `startDate` | Date | today | |
| `schedule` | `Recurrence[]`, at least one | required | each Recurrence's `start` defaults to `startDate`, as the editor gives both from one date (`dialog-sx-editor.c:1009-1023`) |
| `end` | `End` | never | `occurrences` with `remaining` ≤ `occurrences` and `occurrences` ≥ 1 (`dialog-sx-editor.c:547-590`) |
| `autoCreate` | bool | false | the editor's preference `create-auto` |
| `notify` | bool | true with `autoCreate`, else false | stored only with auto-create (`dialog-sx-editor.c:976-985`) |
| `advanceCreateDays`, `advanceRemindDays` | integer ≥ 0 | 0, 0 | |
| `templateTransactions` | `[{ description?, num?, notes?, currency?, splits: [{ account, memo?, action?, debitFormula?, creditFormula? }] }]` | `[]` | |

→ `ScheduledTransactionInfo`, with `warnings`.

- **Instance count 1**, as the editor sets for a new scheduled transaction (`dialog-sx-editor.c:1546-1549`), so the
  first instance has `i` = 1. No last occurrence, no deferred instances.
- **Recurrences** red-ale creates go through `recurrenceSet`'s normalisation (`5.17:libgnucash/engine/Recurrence.cpp:87-139`):
  `mult` 0 becomes 1 except for `once`; an `endOfMonth` start moves to its month's end; a `lastWeekday` start moves
  to the last such weekday; a fifth-week `nthWeekday` becomes `lastWeekday`; a weekend adjustment is kept only for
  `month`, `endOfMonth` and `year`. The result shows what was stored.
- **Template transactions.** `currency` defaults to the first split's account commodity when that is a currency
  (else it is required), as `transaction.post` defaults it. The real transaction's currency is chosen at creation
  anyway (see "The transactions created"). Each split's `account` must exist and must not be the root, a placeholder,
  or a template account: red-ale's commit rules would refuse the created split
  ([`transactions.md`](transactions.md)).
- **Formulas** must parse and name only supported functions (see "Formulas"); a formula naming no variable must
  evaluate. The editor refuses an unparsable formula ("Split with memo %s has an unparsable Credit Formula.",
  `dialog-sx-editor.c:766-790`).
- **Balance.** A template transaction whose formulas name no variable and whose accounts share one commodity must
  balance: debits minus credits zero, or `unbalanced`. The editor tries random variable values and asks "Should it
  still be entered?" (`dialog-sx-editor.c:803-904`); with no dialog, red-ale refuses what it can prove unbalanced
  and accepts the rest, which Since Last Run will balance as GnuCash does.
- **Auto-create** is refused (`invalid`) for a scheduled transaction with variables other than `i`, with more than one
  commodity, or with no template split, as the editor refuses them (`gnc_sxed_check_autocreate`,
  `dialog-sx-editor.c:617-648`). `notify: true` without `autoCreate` is refused: the editor disables it
  (`update_sensitivity`, `:1031-1043`).
- **Warnings**, not refusals, where the editor asks: `duplicate-name` ("A Scheduled Transaction with the name "%s"
  already exists.", `:536-545`) and `never-runs` (no occurrence at all, or none before the end date, `:594-611`).

### `scheduledTransaction.get`, `scheduledTransaction.list` (read)

`get { scheduledTransaction }` → `ScheduledTransactionInfo`.

`list { enabled?, account? }` → `{ scheduledTransactions }`, in book order (the order read, new ones last), each
without `templateTransactions`. `enabled` filters by it; `account` (AccountRef) keeps those whose template splits name
the account, as `gnc_sx_get_sxes_referencing_account` finds them (`SX-book.cpp:364-389`).

### `scheduledTransaction.update` (write)

`{ scheduledTransaction, name?, enabled?, startDate?, schedule?, end?, autoCreate?, notify?, advanceCreateDays?,
advanceRemindDays?, templateTransactions? }` → `ScheduledTransactionInfo`, with `warnings`.

Absent params are left alone; the checks are `create`'s, applied to the result. As the editor saves
(`gnc_sxed_save_sx`, `dialog-sx-editor.c:916-1026`):

- `end` sets one end and clears the other: an end date clears the count, a count clears the end date, `never`
  clears both. A new count sets the remaining occurrences to `remaining`, default the count.
- `templateTransactions` replaces the whole list. An entry or split carrying the `guid` of an existing template
  transaction or split keeps that GUID; the others are new. Template transactions left out are deleted.
- The last occurrence, instance count and deferred instances are not changed by `update`: only Since Last Run moves
  them, as in GnuCash.

### `scheduledTransaction.delete` (destructive)

`{ scheduledTransaction }` → `{ deleted: true }`. Deletes the scheduled transaction, its template transactions and
its template account, as GnuCash's list page does (`_destroy_sx`, `5.17:gnucash/gnome/gnc-plugin-page-sx-list.cpp:1019-1031`;
`xaccSchedXactionFree`, `SchedXaction.cpp:462-488`). Transactions it created stay, with their `from-sched-xaction`
Slot.

**A scheduled transaction that ends is never deleted or disabled by red-ale**, as GnuCash never does (the note §4.9):
it stays, enabled, with no next occurrence.

### `scheduledTransaction.occurrences` (read)

| Param | Type | Meaning |
|-------|------|---------|
| `scheduledTransaction` | GUID | Its occurrences from its current state |
| or `schedule`, `startDate`, `end` | as in `create` | A Schedule not yet saved, as never run: the editor's and the Schedule-from-transaction dialog's example calendars (`gnc_sxed_update_cal`, `dialog-sx-editor.c:1651`; `sxftd_update_example_cal`, `5.17:gnucash/gnome/dialog-sx-from-trans.cpp:108`) |
| `count` | integer, 1–1000 | How many, default 10 |
| `after` | Date? | Only occurrences after this date |

→ `{ occurrences: [{ date, instanceCount }] }`, ascending.

Each date is `xaccSchedXactionGetNextInstance` of the state before it (`SchedXaction.cpp:885-926`): the first
occurrence is the first Recurrence date on or after the start date; each next is the earliest next instance of any of
the Schedule's Recurrences (`recurrenceListNextInstance`, `Recurrence.cpp:432-460`); none after the end date or once
the remaining count reaches zero. Recurrences are computed from their stored fields, as GnuCash's reader leaves them
(the note §2.1), with `recurrenceNextInstance` ported exactly (`Recurrence.cpp:202-381`; the note §2.2). A stored
`mult` of 0 for a periodic type, on which GnuCash would divide by zero (the note §2.2), is refused when the book
is read, as `red_ale_core::Recurrence::new` refuses it for budgets; GnuCash itself never writes one.

Deferred instances are not occurrences: they appear in Since Last Run. A start on a weekend with `back` adjustment
skips the first occurrence, as in GnuCash (the note Fact 11).

## Formulas

A **Formula** is the text of a template split's debit or credit amount. GnuCash parses it with its expression parser
(`5.17:libgnucash/app-utils/calculation/expression_parser.c`, `5.17:libgnucash/app-utils/gnc-exp-parser.c`); red-ale
translates that parser, moved from `crates/red-ale-import/src/business/expr.rs` into `red-ale-core`, and adds what
formulas need ([`../milestone4.md`](../milestone4.md), "Formulas and variables").

### Grammar

As `expression_parser.c:871-1270` (the note §3.1):

```
assignment := add_sub ( ( "=" | "+=" | "-=" | "*=" | "/=" ) assignment )*
add_sub    := mul_div ( ( "+" | "-" ) mul_div )*
mul_div    := primary ( ( "*" | "/" ) primary )*
primary    := "(" assignment ")" | ( "+" | "-" ) primary | number | name
            | name "(" [ argument ( ":" argument )* ] ")"
argument   := assignment | '"' text '"'
name       := [A-Za-z_][A-Za-z0-9_]*
```

- A name directly followed by `(` is a function call; `pmt (` with a space is a variable then a parenthesis, an error.
  Arguments are separated by `:`, never `,`.
- Strings are allowed only as function arguments (`:1252-1264`).
- Two operands in a row are an error (`1 2`, `1 asdf`; `:1083-1096`).
- An assignment needs a variable on its left and gives its value (`jkl = 5 * (nj = 68.9)`); it binds the name for the
  rest of that one formula only (`gnc-sx-instance-model.c:221-232`).
- **A whole formula that is one parenthesised number is negated**, the accounting convention: `(5)` is −5,
  `(5) + 1` is 6 (`expression_parser.c:609-620`).

### Numbers and arithmetic

- **Numbers** are read as GnuCash reads them, with `xaccParseAmount` in the locale's monetary decimal point and group
  separator (`gnc-exp-parser.c:362-381`): red-ale uses its Locale's, which under the default `C` locale are `.` and
  `,`, as `gnc_localeconv` defaults them (`5.17:libgnucash/core-utils/gnc-locale-utils.c:91-92`). So `1,234.50` and
  `100,000.00`, which GnuCash's dialogs write in `en_US`, read as written. A formula written in a locale with other
  separators is read correctly only under that Locale, in red-ale as in GnuCash: the book doesn't record it.
  As in `expr.rs`, a group separator after the decimal point and a number that doesn't fit 64 bits are refused,
  and fraction digits are kept exactly where `xaccParseAmount` cuts them at twelve.
- **Arithmetic is exact**: GnuCash's `gnc_numeric` operations with `GNC_DENOM_AUTO` and `GNC_HOW_DENOM_EXACT`
  (`gnc-exp-parser.c:383-421`), the result reduced (`:546`). Division by zero and overflow fail the formula, where
  GnuCash gives a numeric error and uses 0 (the note §10, open question 4).

### Variables

A **Variable** is a name a formula uses, given a value per instance by Since Last Run.

- **Every name in a template formula is a variable**, assigned ones too (`gnc_sx_get_variables`,
  `gnc-sx-instance-model.c:390-395`, helper `:313-376`). Each starts unbound; Since Last Run needs a value for every
  one before it creates an instance, as GnuCash's dialog refuses OK until then
  (`gnc_sx_instance_model_check_variables`, `:1617-1650`; `5.17:gnucash/gnome/dialog-sx-since-last-run.c:1563-1594`).
- **`i` is predefined**: the instance's Instance count, not editable (`gnc-sx-instance-model.c:443-456`). A new
  scheduled transaction's first instance has `i` = 1; one whose file holds instance count 0 has `i` = 0.
- **Exchange-rate variables.** For each non-marker split whose account commodity differs from the transaction
  currency, a variable named `<split mnemonic> -> <transaction mnemonic>`, such as `EUR -> USD`
  (`gnc-sx-instance-model.c:300-311`, `:363-372`). It is not a name a formula can write; its value is the rate (see
  "The transactions created").
- **Values** are Amounts (`gnc-sx-instance-model.h:62-67`), given as `{ "<name>": Amount }`.
- **No global variables.** GnuCash's parser also searches the user's saved calculator variables (the
  `expressions-2.0` file, `gnc-exp-parser.c:62-164`), depending on who started the parser first (`:508-509`).
  red-ale takes variables only from the instance's bindings: they are per-user state, not the book's (the note §3.3).

### Functions

In GnuCash a function call is a Guile procedure `gnc:<name>`, called with each argument converted to a C double, its
result converted back by keeping 12 significant figures, half up (`func_op`, `gnc-exp-parser.c:288-360`). Any loaded
`gnc:` procedure can be called; formulas are meant to call the financial functions of `5.17:bindings/guile/fin.scm`
(GPL-2.0-or-later; Copyright 2002 Joshua Sled, 2012 Frank H. Ellenberger), which the parser loads (`:84-85`).

red-ale never runs Guile. It implements fin.scm's financial functions **exactly** ("Decided", item 6): with a whole
number of periods `(1+r)^n` is a rational, so the result is computed with exact rationals (big integers inside the
computation), then rounded half up to 12 significant figures as GnuCash rounds a function's result
(`double_to_gnc_numeric(…, GNC_HOW_DENOM_SIGFIGS(12) | GNC_HOW_RND_ROUND_HALF_UP)`, `:346-357`), then, as part of a
split value, to the currency fraction.

| Function | fin.scm | Computed when |
|----------|---------|---------------|
| `pmt(rate : nper : pv : fv : type)` | `:49-51`, `:75-90` | `nper` is a whole number |
| `ipmt(rate : per : nper : pv : fv : type)` | `:34-39`, `:92-97` | `per` and `nper` are whole numbers |
| `ppmt(rate : per : nper : pv : fv : type)` | `:42-46` | as `ipmt` |
| `futureValue(a : r : n : t)` | `:56-64` | `n · t` is a whole number |
| `computeInterestIncrement(pv : rate : compounds : period)` | `:66-68` | `period` is a whole number |
| `periodic_rate(rate : yper : ycomp)` | `:110-112` | `ycomp ÷ yper` is a whole number |
| `foobar(v)` | `:27` | always (fin.scm's test helper, returning its argument) |

- **Refused, with a reason** (`formula`, `data.reason: "non-integer-power"`): any call whose power is not a whole
  number. Its result is irrational, and exact methods for it are not yet designed ("Decided", item 6). Equal
  frequencies, the common case, are always computed.
- **Refused** (`formula`, `data.reason: "inexact"`): `cpd_pmt`, `cpd_ipmt`, `cpd_ppmt` (`:152-184`) and
  `amort_balance`, `amort_pmt`, `amort_ppmt`, `amort_ipmt` (`:300-337`). They round to the cent with Scheme's `round` on
  doubles (`:176-184`, `:362-366`), and exact ties are common in an amortisation table; until how Guile rounds those
  ties is settled (the note's H9), red-ale can't reproduce them.
- **Refused** (`formula`, `data.reason: "unknown-function"`): every other name. GnuCash would look it up in Guile.
- **Zero rate.** `pmt`, `ipmt` and `ppmt` divide by the rate (`:88-90`), so a zero rate fails, as in GnuCash, where
  the double result is NaN and the conversion refuses it (`5.17:libgnucash/engine/gnc-numeric.cpp:90-95`); red-ale
  reports `division-by-zero`.
- **A string argument** fails the call: no financial function takes one.

GnuCash computes in doubles, so its result can differ from the exact one in the last bits. Both are rounded to 12
significant figures and then to the SCU, so they differ only when the exact value lies within about 10⁻¹⁰ (relative)
of a half-cent rounding point. Such a case gives a cent different from GnuCash's and is recorded as a deviation; the
hand-worked cases of the note §9.3 (H2–H5) are the test table (the note §7.4).

### Evaluation

- **By Since Last Run**: every formula is parsed with the instance's bindings, which always hold `i`, so the stored
  numerics are never used (`_get_sx_formula_value`, `gnc-sx-instance-model.c:1087-1148`). An empty formula gives 0.
- **By the forecast reports** (the cash-flow read of the report library, [`../milestone4.md`](../milestone4.md),
  "Reports"): with no bindings, so a valid non-zero numeric is used, else the formula is parsed with every variable,
  `i` included, at 0 ([`../notes/engine-budgets.md`](../notes/engine-budgets.md) §4.10). Not an API method.

Formula errors carry `data`: `{ reason, formula, position, scheduledTransaction, templateTransaction, split, side,
index }`, `side` being `debit` or `credit`, `position` a character offset (or null), and `index` the instance's in a
Since Last Run proposal (or null). Reasons: `syntax`, `unknown-function`, `non-integer-power`, `string-argument`,
`division-by-zero`, `overflow`. A missing variable value is not a formula error: `apply` refuses it first, as
`invalid`. GnuCash reports a failed call as "Stack underflow" (the note §3.4); red-ale names the cause.

## Since Last Run

**Since Last Run** creates the transactions of every enabled scheduled transaction that have fallen due since it last
ran, and reminds of those coming. In GnuCash it is a GUI dialog over the instance model
(`5.17:libgnucash/app-utils/gnc-sx-instance-model.c`), which the GUI also runs when a book opens. red-ale never
changes a book on open ([`../milestone4.md`](../milestone4.md), "Out"): a client asks for it. It works as import does,
in two calls on the same rules ([`import.md`](import.md), "The pipeline"):

1. **`scheduledTransaction.sinceLastRun`** builds the **Since Last Run proposal** of due instances and changes nothing.
2. The caller reads it, and may change an instance's state or give variable values.
3. **`scheduledTransaction.apply`** applies it with those choices as **one Mutation**: every transaction created and
   every schedule advanced, or nothing.

The server holds each proposal by ID until it is applied, discarded, pushed out by 16 newer Since Last Run proposals,
or the book closes. Since Last Run proposals and Import proposals are held apart, so neither pushes out the other. A
proposal records the Book revision it was made at, and `apply` refuses it with `conflict` at any other.

### Instances and their states

An **Instance** is one occurrence of a scheduled transaction as Since Last Run acts on it: its date, its Instance count
(`i`), its variable bindings, the temporal state before it, and its state (`gnc-sx-instance-model.h:69-77`). States,
GnuCash's `GncSxInstanceState` (`:52-60`), on the wire:

| State | GnuCash (GUI) | Applying it |
|-------|---------------|-------------|
| `toCreate` | `TO_CREATE` (To-Create) | Create its transactions, then advance the schedule |
| `reminder` | `REMINDER` (Reminder) | Nothing: reminders are not recorded, so it is proposed again next time |
| `postponed` | `POSTPONED` (Postponed) | Save it as a Deferred instance, then advance the schedule |
| `ignored` | `IGNORED` (Ignored) | Advance the schedule without creating anything |
| `created` | `CREATED` (Created) | Only in results; never chosen |

A **Deferred instance** is a postponed instance as saved: the schedule's temporal state before it (last occurrence,
remaining occurrences, instance count; `SXTmpStateData`, `5.17:libgnucash/engine/SchedXaction.h:130-136`), kept in
the scheduled transaction's deferred list, sorted by date (`SchedXaction.cpp:1098-1104`).

### `scheduledTransaction.sinceLastRun` (read)

Params: `{}`. "Today" is the local day of the Session's clock ([`../api.md`](../api.md)).

For each enabled scheduled transaction, in book order, as `_gnc_sx_gen_instances` builds them
(`gnc-sx-instance-model.c:482-556`; the note §4.2), with one deviation (see "Deviations in Since Last Run", 1):

```
creation_end ← today + advanceCreateDays
remind_end   ← creation_end + advanceRemindDays
for each deferred instance d, in order:    propose `postponed` at the next occurrence after d, with d's state
state ← the schedule's own state (last occurrence, remaining, instance count; an unset last occurrence reads 1970-01-01)
date  ← next occurrence after state
while date ≤ creation_end:  propose `toCreate`; advance state
while date ≤ remind_end:    propose `reminder`; advance state
```

Auto-create scheduled transactions are proposed like any other, flagged `autoCreate`. Works on a read-only book: it
only reads.

**The Since Last Run proposal**:

| Field | Meaning |
|-------|---------|
| `id` | The proposal's ID, a string |
| `bookRevision` | The Book revision it was made at |
| `today` | The day it was made for |
| `thresholdDate` | The auto read-only threshold date, or null with no threshold ([`book-options.md`](book-options.md)) |
| `summary` | Counts: instances by state, `autoCreate`, `needVariables`, `readOnlyByDate`, `errors` |
| `schedules` | One entry per scheduled transaction with at least one instance: `{ scheduledTransaction, name, autoCreate, notify, instances }` |

Each instance:

| Field | Meaning |
|-------|---------|
| `index` | Its position across the whole proposal, in the order above: the key `apply` changes it by |
| `date` | The occurrence date |
| `state` | The proposed state: `postponed`, `toCreate` or `reminder` |
| `instanceCount` | Its `i` |
| `variables` | `[{ name, value, editable }]`: every variable, `value` null while unbound; `i` has its value and `editable: false` |
| `readOnlyByDate` | True when `date` is before `thresholdDate`: creating it needs the caller's explicit choice (see "Deviations in Since Last Run", 3) |
| `errors` | `[FormulaError]` and template errors (`missing-account`, `placeholder`) found without the caller's values: creating it as it stands would fail |
| `transactions` | When every variable has a value: the transactions creating it would make, as "The transactions created" builds them, each `{ templateTransaction, currency, description, num, splits: [{ account, accountName, value, amount, memo, action }], imbalance }`, `imbalance` being `{ value, currency, account }` of the `Imbalance-<CUR>` split creation would add, or null. Otherwise null |

### `scheduledTransaction.apply` (write)

| Param | Type | Meaning |
|-------|------|---------|
| `proposal` | string | The proposal's ID |
| `choices` | `[{ index, state?, variables? }]` | The caller's changes by index: a new state (`toCreate`, `reminder`, `postponed`, `ignored`) and variable values `{ name: Amount }`. Default none |
| `autoCreateOnly` | bool | Apply only the instances of auto-create scheduled transactions, as GnuCash does before showing its dialog (`effect_change` with `auto_create_only`, `gnc-sx-instance-model.c:1483-1491`): per schedule, its `toCreate` instances up to the first in another state. Everything else is left as if not proposed. Default false |

Instances not named keep the proposal's state and bindings.

**Checks, before anything changes** (each `invalid` with `data.reason` and `data.index`, unless it says otherwise):

- every `toCreate` instance has a value for each variable (`unbound-variable`); `i` and unknown names can't be set
  (`not-editable`, `unknown-variable`);
- **a reminder may not come before any other state** in one schedule, GnuCash's rule (`gnc-sx-instance-model.h:138-158`,
  `:1556-1596`), refused as `reminder-first`, naming the instances (see "Deviations in Since Last Run", 4);
- every `toCreate` instance dated before the threshold date is named in `choices` with `state: "toCreate"`
  (`read-only-by-date`);
- `postponed` is refused in an SQLite book (`sql-deferred-instances`; see "SQLite books");
- with `autoCreateOnly`, `choices` name only instances of auto-create scheduled transactions;
- every formula of every `toCreate` instance evaluates (`formula`, with the instance's index; see "Deviations in Since
  Last Run", 2), and every template split's account exists and is not a placeholder (`missing-account`,
  `placeholder`).

**Then, per scheduled transaction, in order of its instances** (`gnc_sx_instance_model_effect_change`,
`gnc-sx-instance-model.c:1445-1554`; the note §4.4):

- an instance proposed `postponed` and now in another state leaves the deferred list (`:1493-1500`);
- `toCreate`: its transactions are created ("The transactions created"), then the schedule advances;
- `postponed` (newly): its temporal state joins the deferred list, then the schedule advances;
- `ignored`: the schedule advances;
- `reminder`: nothing;
- **advancing** (`increment_sx_state`, `:1042-1058`): the last occurrence becomes the instance's date if later; the
  instance count becomes the instance's `i` + 1; the remaining occurrences go down by one if above zero;
- a schedule with no instance acted on is not touched (`:1467-1471`). Nothing else changes: not `enabled`, not the
  template (the note §4.7).

**Result**:

| Field | Meaning |
|-------|---------|
| `created` | `[{ index, scheduledTransaction, date, transactions: [GUID] }]` |
| `postponed`, `ignored`, `reminders` | Indexes |
| `schedules` | `[{ scheduledTransaction, lastOccurrence, instanceCount, end, deferredInstances }]` after the apply |
| `imbalanceSplits` | `[{ index, transaction, split, account, value }]`: every `Imbalance-<CUR>` split made |
| `warnings` | Every Imbalance split again, and every transaction created read-only by date |

Errors: `no-book`, `read-only`, `not-found` (`data.kind: "sinceLastRunProposal"`: unknown, applied, discarded, pushed
out, or from before the book was opened), `conflict` (`data.reason: "book-revision"`, with `data.proposalRevision` and
`data.bookRevision`), `invalid` (above, and an index out of range or the state `created`), `formula`. A proposal stays
held after a failed apply.

### `scheduledTransaction.discard` (read)

`{ proposal }` → `{ discarded: true }`. Drops a held proposal. Its effect is `read`, as `import.discard`'s: it changes
neither the Book nor the Configuration. `not-found` for an unknown ID.

### The transactions created

One real transaction per template transaction of the instance, in the template account's split order
(`xaccAccountForEachTransaction`, `5.17:libgnucash/engine/Account.cpp:5138-5144`), built as
`create_each_transaction_helper` builds it (`gnc-sx-instance-model.c:1310-1422`; the note §3.6–§3.8, §4.6):

| Field | Value |
|-------|-------|
| GUID | New |
| Date posted | The instance date, at the Day-neutral time, with the `date-posted` Slot (`xaccTransSetDate`, `5.17:libgnucash/engine/Transaction.cpp:1913-1950`) |
| Date entered | Now (`:1346`) |
| Currency | From the template transaction's currency `T` and its non-marker splits' account commodities: the first currency among the splits if `T` is none of theirs or not a currency; else the first split commodity if `T` is in none of them; else `T` (`get_transaction_currency`, `:1245-1308`) |
| Num, description | The template transaction's (`xaccTransCloneNoKvp`, `Transaction.cpp:605-633`) |
| Notes | The template's notes Slot, if any (`:1337-1339`) |
| Slots | `date-posted`, `notes` if copied, and **`from-sched-xaction`, the scheduled transaction's GUID** (`:1405-1409`). No other Slot: the template's others are not copied |
| Splits | One per template split, in template order, then sorted debits first by the commit; memo, action and reconcile state from the template split; account from `sched-xaction/account`; no Slots, no lot |

Each split's amounts (`:1087-1231`):

1. **Value** = debit − credit, exact, then rounded half up to the transaction currency's fraction; a non-zero value
   that would round to zero keeps 0, as `xaccSplitSetValue` does (`5.17:libgnucash/engine/Split.cpp:1280-1300`).
2. **Amount**: the value, when the account's commodity is the transaction currency (`xaccSplitScrub`,
   `5.17:libgnucash/engine/Scrub.cpp:236-347`). Otherwise, with the exchange-rate variable's value as the rate (1 if
   the variable is absent): for a currency, value × rate rounded to 1/1000; for any other commodity, value ÷ rate
   rounded to the commodity's fraction (`split_apply_exchange_rate`, `:1196-1231`); then half up to the account's SCU
   (`Split.cpp:1243-1266`). 100 USD into EUR at `EUR -> USD` = 0.9 is −90.00 EUR; 100 USD of ACME at
   `ACME -> USD` = 3 is 33.3333 shares.
3. **Balance.** If rounding leaves the transaction unbalanced, its commit adds an `Imbalance-<CUR>` split, creating
   the account if needed, as GnuCash's commit scrub does (`Transaction.cpp:1549-1558`): a debit of `1.005` against a
   credit of `1` gives 1.01, −1.00 and an Imbalance split of −0.01. This is the one place red-ale's commit balances
   instead of refusing, as `import.apply` does ([`import.md`](import.md), "Imbalance"); each such split is in the
   proposal's `imbalance` and the result's `imbalanceSplits`.

With trading accounts on, the commit keeps the trading splits as every commit does
([`trading-accounts.md`](trading-accounts.md)). A scheduled transaction with no template transaction creates nothing
and advances.

Created transactions dated before the threshold date are read-only by date once created, as in GnuCash. Template
transactions are exempt from the threshold (`xaccTransIsSXTemplate`, `Transaction.cpp:2350-2371`, `:2385-2386`) and
are never journaled: the journal records the transactions of the Book.

### Deviations in Since Last Run

The three of "Decided", item 11, and one about the state rule:

1. **No duplicates after a postponement.** GnuCash generates the `toCreate` instances that follow a deferred instance
   from that deferred state, not from the schedule's own, so instances already created after a postponed one are
   proposed again on every later run until the postponed one is decided (`gnc-sx-instance-model.c:500-535`; the note
   Fact 1, §4.8.1, §9.2): postpone January, create February, and March's run offers January, February *and* March.
   red-ale proposes each deferred instance from its own state, then the `toCreate` and `reminder` instances from the
   schedule's own state. *Why:* a duplicate created is money recorded twice, and nothing in the proposal would tell
   the caller. What red-ale saves is the same fields GnuCash saves, so GnuCash's own next run, while a deferred
   instance remains, still proposes the duplicates; red-ale can't prevent that (the note, open question 1).
2. **A failing formula refuses the apply.** GnuCash commits the transaction with 0 for the failing amounts, reports
   the error and does not advance the schedule, so the next run creates it again (`:1127-1139`, `:1410-1412`,
   `:1518-1535`; the note Fact 2). red-ale refuses the whole apply with `formula`, naming the instance, and creates
   nothing; the caller can fix the variables, or choose another state for that instance. *Why:* a zero-amount
   transaction is a wrong record, and a repeat on the next run doubles it.
3. **Instances before the read-only threshold need an explicit choice.** GnuCash's Since Last Run checks only that
   the Book is not read-only (`:1453-1457`) and creates instances whatever their date (the note Fact 3). red-ale
   flags each such instance (`readOnlyByDate`), and `apply` refuses unless `choices` names it with
   `state: "toCreate"`. *Why:* the transaction would be one GnuCash's register then refuses to change; the caller
   should know before it exists. Since Last Run joins the list of what the threshold allows, with that condition
   ([`book-options.md`](book-options.md)).
4. **The state rule is checked, not applied.** GnuCash's dialog keeps its rule by changing other instances: a
   non-reminder state turns every earlier reminder into postponed, and a reminder turns every later instance into a
   reminder (`gnc_sx_instance_model_change_instance_state`, `:1556-1596`). red-ale refuses choices that break the
   rule and names the instances. *Why:* a call should not change instances it didn't name, and an unasked postponement
   saves a Deferred instance.

Kept from GnuCash, because they are what GnuCash's next run reads (the note §4.8, §10.4c): postponing consumes an
occurrence and creating the postponed instance later consumes another; the instance count is overwritten, not
maximised, so acting only on an old postponed instance can lower it; a postponed instance set to `reminder` leaves
the deferred list and is never proposed again.

### SQLite books

An SQLite book cannot hold Deferred instances: the `schedxactions` table has no column for them and no other table
holds them (`5.17:libgnucash/backend/sql/gnc-schedxaction-sql.cpp:51-82`), so GnuCash loses them
([`../notes/sql-backend.md`](../notes/sql-backend.md) §0; the note Fact 10). In an SQLite book, `apply` refuses the
state `postponed` (`invalid`, `data.reason: "sql-deferred-instances"`), and the proposal never holds a postponed
instance. *Why:* a postponement that vanishes on the next load would silently turn into an ignored instance; the
caller can choose `ignored` or `reminder` knowing what each does. ([`../milestone4.md`](../milestone4.md), "Data
SQLite can't hold".)

## Schedule from transaction

`scheduledTransaction.fromTransaction` is GnuCash's Schedule from transaction dialog
(`5.17:gnucash/gnome/dialog-sx-from-trans.cpp`; the note §5) as one operation.

| Param | Type | Default | GnuCash |
|-------|------|---------|---------|
| `transaction` | GUID | required | |
| `name` | string, not empty | the transaction's description | `:361-368` |
| `frequency` | `daily`, `weekly`, `biWeekly`, `monthly`, `quarterly`, `yearly` | `daily` | `:70-77`, `:422` |
| `startDate` | Date | the first occurrence of `frequency` after the transaction's posted date | `:417-430` |
| `end` | `End` | never | `:154-203`, `:464-482` |
| `autoCreate`, `notify`, `advanceCreateDays`, `advanceRemindDays` | as `create` | false, true with `autoCreate`, 0, 0 | the editor's preferences `create-auto`, `notify`, `create-days`, `remind-days`, at their shipped defaults (`:486-507`) |

→ `ScheduledTransactionInfo`, with `warnings`.

- **The Schedule** is one Recurrence starting at `startDate`, weekend adjustment `none` (`sxftd_update_schedule`,
  `:278-327`): `daily` is day ×1, `weekly` week ×1, `biWeekly` week ×2, `monthly` month ×1, `quarterly` month ×3,
  `yearly` **month ×12**, not `year`. The scheduled transaction starts on `startDate`, instance count 1 (`:484`).
- **One template transaction** (`sxftd_add_template_trans`, `:206-275`): the transaction's description, num,
  notes and currency; per split, in order, its memo, its action, its account and **its value as a formula**: a
  positive value as the debit formula, otherwise (zero included) its negation as the credit formula.
- **The formula text** is the value in red-ale's Locale's decimal form **without grouping**, with at least the
  currency's decimal places and never rounded: `1234.50` under the default `C` locale, `1234,50` under `de_DE`; a
  value whose denominator is not a power of ten is written `num / denom`, which the parser reads exactly. GnuCash
  writes `xaccPrintAmount` with the locale's grouping (`1,234.50` in `en_US`, `:227-256`;
  `5.17:libgnucash/app-utils/gnc-ui-util.cpp:833-859`). Grouping is left out because GnuCash skips a group separator
  wherever it appears before the decimal point: `1.234,50` written in one locale reads as 1.23450 or 123450 in
  another. The numerics are set (see "Numerics"), where GnuCash's path sets none (`SchedXaction.cpp:986-993`).
- **Every split is copied**, trading splits included, as GnuCash copies them. A split in another commodity than the
  transaction currency gets an exchange-rate variable at Since Last Run, as in GnuCash.
- The source transaction is not changed and gets no Slot. GnuCash refuses a transaction being edited (`:348-351`);
  red-ale has no open edits. The template balances, since every transaction of the Book does.

The checks of `create` apply. The dialog's "Advanced…" button, which opens the full editor (`:589-612`), is
`scheduledTransaction.update` afterwards.

## The loan assistant

`scheduledTransaction.loan` is GnuCash's Mortgage and Loan Repayment assistant
(`5.17:gnucash/gnome/assistant-loan.cpp`, here `AL`; the note §6) as one operation, as the stock split assistant became
`stock.split`: the loan in, the scheduled transaction GnuCash's assistant builds (`loan_create_sxes`, `AL:2838-3046`)
out, with the same `pmt`, `ppmt` and `ipmt` formulas (`AL:2440-2528`) and instance count (`AL:2863-2865`), so GnuCash
computes the same repayments from it. Defects the note records (§6.3) are not copied.

### Params

| Param | Type | Default | GnuCash |
|-------|------|---------|---------|
| `name` | string | `Loan` | the repayment name (`AL:923`) |
| `loanAccount` | AccountRef, type LIABILITY | required | `AL:566-567`, `:598`, `:1060-1063` |
| `principal` | Amount, > 0 | required | in the loan account's commodity (`AL:1066-1075`) |
| `rate` | decimal string, percent, 0.001–100, at most 3 decimals | required | the spin box (`AL:194`, `:698-703`) |
| `rateType` | `simple`, `aprDaily`, `aprWeekly`, `aprMonthly`, `aprQuarterly`, `aprSemiAnnually`, `aprAnnually` | `simple` | `IRateType` (`AL:166-175`); `simple` is "Interest Rate" |
| `startDate` | Date | today | `AL:912-914` |
| `length` | `{ months }` or `{ years }`, 1–9999 | required | `AL:1011-1034` |
| `monthsRemaining` | integer, 1 to the length in months | as GnuCash computes it | the length in months minus the months from `startDate` to today, counted by adding a month until reaching today (`loan_info_calc_update_cb`, `AL:1011-1034`) |
| `repayment` | `{ from, principalTo?, interestTo, start?, frequency? }` | | the repayment page (`AL:550-605`, `:1089-1092`, `:1397-1420`) |
| `escrow` | AccountRef? | none | the options page (`AL:729-735`, `:1296-1314`) |
| `otherPayments` | `[{ kind, memo?, amount, from?, to, viaEscrow? }]` | none | the payment-options pages (`AL:1645-1658`) |

- `repayment.from` is a BANK, CASH, CREDIT, ASSET or LIABILITY account; `principalTo` (default `loanAccount`) and
  `interestTo` are of those types or EXPENSE (`AL:550-605`).
- `repayment.frequency` is one Recurrence's `{ mult, periodType, weekendAdjust? }`, default monthly ×1 with no
  weekend adjustment (`AL:916-921`); it starts on `repayment.start`, default `startDate`.
- `otherPayments[].kind` is `taxes`, `insurance`, `pmi` or `other`, GnuCash's four fixed options (`REPAY_DEFAULTS`,
  `AL:123-132`), with their memos `Tax Payment`, `Insurance Payment`, `PMI Payment` and `Miscellaneous Payment` by
  default; `viaEscrow` defaults to true for the first three when `escrow` is given, else false, and is refused
  without `escrow`. `to` is required; `from` defaults to `repayment.from`.

→ `{ scheduledTransaction: ScheduledTransactionInfo, payment }`, `payment` being the `pmt` formula's value (12
significant figures).

### What it builds

One scheduled transaction (`AL:2850-2985`), with `xaccSchedXactionMalloc`'s defaults otherwise (enabled, no
auto-create, 0 days in advance, no occurrence count):

| Field | Value |
|-------|-------|
| Name | `name` |
| Start date | `startDate` |
| Last occurrence | `repayment.start` minus one month (`AL:2853-2854`) |
| End | `repayment.start` plus `monthsRemaining − 1` months (`AL:2855-2858`) |
| Instance count | the length in months − `monthsRemaining` + 1 (`AL:2863-2865`): 1 for a new loan, 13 after twelve payments |
| Schedule | `repayment.frequency`, starting on `repayment.start` |

Both dates are counted in months whatever the payment frequency, as in GnuCash.

**Template transactions** (`AL:2870-2985`), in the loan account's currency:

- without escrow, one, `<name> - Payment`: `repayment.from` credit `pmt(…)` (memo `<name> - Payment`), `principalTo`
  debit `ppmt(…)` (`<name> - Principal`), `interestTo` debit `ipmt(…)` (`<name> - Interest`);
- with escrow, two: `<name> - Escrow Payment`, `repayment.from` credit and `escrow` debit of `pmt(…)` (memo `<name>`);
  then `<name> - Payment`, `escrow` credit `pmt(…)`, principal and interest as above (`AL:2903-2935`).

Each other payment adds its amount as `ld_setup_repayment_sx` does for a payment that is part of the payment
transaction (`AL:2615-2819`): ` + <amount>` on the paying split's credit formula, or a new credit split in `from` when
it differs from `repayment.from`; a debit of the amount to `to` with the option's memo; and, via escrow, the deposit
into escrow in the first template and the escrow-to-`to` transfer in the second.

**Formula text** (`AL:2440-2528`), the shapes of GnuCash's "Interest Rate" templates:

```
pmt( {r} / {ppy} : {n} : {pv} : 0 : 0 )
ppmt( {r} / {ppy} : i : {n} : {pv} : 0 : 0 )
ipmt( {r} / {ppy} : i : {n} : {pv} : 0 : 0 )
```

`{r}` is `rate ÷ 100` to 5 decimals (exact, since `rate` has at most 3), `{n}` the periods to 2 decimals, and `{pv}`
the principal with at least 2 decimals, never rounded. `{ppy}`, the payments per year, is written to 2 decimals when
that is exact, and otherwise as a parenthesised division (`( 52 / 3 )` for every three weeks), where GnuCash prints
`17.33`. Payments per year are
GnuCash's (`AL:2403-2430`): `year` 1/mult, `week` 52/mult, the month types 12/mult, `day` 365/mult truncated to an
integer, `once` refused (`invalid`). Periods are the length in months × ppy ÷ 12, truncated, at least 1
(`AL:2433-2435`): the full original term and principal, payments already made entering only through `i`. Numbers are
in the Locale's decimal form without grouping, as for Schedule from transaction: under `C`,
`pmt( 0.06000 / 12.00 : 360.00 : 100000.00 : 0 : 0 )`, what GnuCash writes under `C` (the note §6.2).

**APR rate types.** GnuCash converts an APR compounded *m* times a year to a per-period rate
`(1 + rate/m)^(m/ppy) − 1`, in C `float`, printed to 5 decimals (`loan_apr_to_simple_formula`, `AL:2296-2314`,
`:2455-2473`). When *m* equals the payments per year that rate is exactly `rate ÷ ppy`, and red-ale writes the
"Interest Rate" form above. Otherwise the power is not a whole number, and the rate type is refused (`invalid`,
`data.reason: "non-integer-power"`), for the reason financial functions with one are ("Decided", item 6).

**Numerics** follow the rule of "Numerics": the `pmt` formula's value, `0/1` for `ppmt` and `ipmt`, which name `i`.

### Left out of v0.4

- **Other payments on their own schedule** (frequency "Other"), which GnuCash makes into separate scheduled
  transactions with an empty second template (`AL:2991-3025`). Their instance count loops forever in some cases, and
  with a source account or escrow their splits are unbalanced (the note §6.3, defects 2 and 3).
- **The loan type and rate-change frequency** (fixed, 3/1 … 10/1 ARM): GnuCash saves them and never uses them
  (`AL:965-971`, `:1096-1104`).
- **The review page's repayment table** (`AL:2057-2235`): it computes from the loan start whatever the payments made,
  and rounds to 1/100 whatever the currency (defect 5). `scheduledTransaction.occurrences` and Since Last Run give the
  dates and figures.
- **APR rate types whose compounding is not the payment frequency** (above).

### Not copied from GnuCash

| Defect (the note §6.3) | red-ale |
|------------------------|---------|
| 1. The repayment start is invalid unless the user touches the frequency, so the schedule never ends | `repayment.start` defaults to `startDate` |
| 4. An APR per-period rate rounded to 5 decimals changes the payment | Exact rates only |
| Other payments rounded to two significant figures (`250.75` → `251`, `AL:2686-2699`) | Written exactly |
| Numbers with the locale's grouping, the rate as a C `float`, payments per year and the principal to 2 decimals | Locale decimal form without grouping; the rate, payments per year and principal exact |
| The template currency is the user's default currency (`AL:2868`) | The loan account's currency |
| No numerics | Numerics set |

Defect 6 (`ipmt + ppmt` and `pmt` rounding a cent apart) can still happen after rounding to the SCU; the instance then
gets an Imbalance split, as in GnuCash.

## Errors

Codes from [`../api.md`](../api.md), and one new one:

| Code | Name | When |
|------|------|------|
| 6000 | `formula` | A template formula does not parse, names a function red-ale does not compute, or can't be evaluated for an instance. `data`: `{ reason, formula, position, scheduledTransaction, templateTransaction, split, side, index }` (see "Formulas"). From 0.4.0 |

Elsewhere: `not-found` (`data.kind`: `scheduledTransaction`, `transaction`, `account`, `sinceLastRunProposal`),
`invalid` (each check above, with `data.reason`), `unbalanced` (a template that provably doesn't balance),
`conflict` (a stale proposal), `read-only`, `no-book`.

## Methods and MCP

| Method | Effect (target) | MCP |
|--------|-----------------|-----|
| `scheduledTransaction.create`, `update`, `fromTransaction`, `loan`, `apply` | write (book) | — |
| `scheduledTransaction.get`, `list`, `occurrences`, `sinceLastRun`, `discard` | read | read only |
| `scheduledTransaction.delete` | destructive (book) | destructive |

`apply` is `write`, not `destructive`: it creates transactions and moves schedules forward, and overwrites nothing.

## CLI

```
red-ale exec --book books/home.gnucash scheduledTransaction.create name=Rent \
    'schedule:=[{"mult": 1, "periodType": "month", "start": "2025-01-01"}]' \
    'templateTransactions:=[{"description": "Rent", "splits": [
        {"account": "Expenses:Rent", "debitFormula": "750.00"},
        {"account": "Assets:Checking", "creditFormula": "750.00"}]}]'
red-ale call scheduledTransaction.sinceLastRun
red-ale call scheduledTransaction.apply proposal=slr-2 'choices:=[{"index": 0, "state": "postponed"}]'
```

## Deviations from GnuCash

| What | GnuCash 5.17 | red-ale | Why |
|------|--------------|---------|-----|
| Since Last Run on open | Run by the GUI, auto-create instances created at once | Only when asked | red-ale never changes a book on open |
| Choosing and applying | A dialog, auto-create instances first | A proposal, applied as one Mutation | No GUI; Since Last Run applies whole or not at all |
| Instances after a postponed one | Proposed again on every run | Proposed once | A duplicate is money recorded twice |
| A failing formula | Transaction created with 0, schedule not advanced | Apply refused | No wrong record, no repeat |
| Instances before the read-only threshold | Created, unchecked | Flagged; created only when named | The caller sees it first |
| The reminder rule | Other instances' states changed to keep it | Choices that break it refused | A call changes only what it names |
| Postponing in an SQLite book | Accepted, then lost on the next load | Refused | No silent loss |
| A template split in a placeholder account | Created | Refused | red-ale's commit rules hold for every door |
| Template numerics on open | Scrubbed by the GUI | Kept as read; set correctly when red-ale writes a formula | red-ale never changes a book on open |
| Global calculator variables | Searched, depending on parser start-up | Never | Per-user state, not the book's |
| Function calls | Any Guile `gnc:` procedure, in doubles | fin.scm's financial functions, exact; non-integer powers and other names refused | No Guile; money never passes through a float |
| Formula errors | "Stack underflow" for a failed call | The cause named | |
| Overflow and division by zero in a formula | Numeric error, used as 0 | Refused | A wrong amount is worse than a refused one |
| Formula text from Schedule from transaction and the loan | The locale's grouped form | The Locale's decimal form, no grouping | A group separator reads differently in another locale |
| Loan assistant defects (§6.3) and two-significant-figure other payments | As in GnuCash | Not copied | See "The loan assistant" |
| Loan template currency | The user's default currency | The loan account's | red-ale has no per-user default currency in the Book |
| Editor confirmations (duplicate name, never runs, unprovable balance) | Asked | Warnings; a provably unbalanced template refused | No dialog |

## Tests

From [`../milestone4.md`](../milestone4.md), "Testing". **No test runs Guile** ("Decided", item 12): expected values
that need it are hand-worked from the 5.17 source, each citing the lines it follows (the note §9.3).

- **Recurrences**: GnuCash's `test_some` exactly and `test_all` as a property test
  (`5.17:libgnucash/engine/test/test-recurrence.c:212-252`, `:361-399`), the table of the note §2.6, and the
  unnormalised cases of §9.1, with the oracle's `recurrenceNextInstance` (through `ctypes`, no Guile) giving the
  expected dates; next occurrences never go backwards.
- **Formulas**: `5.17:libgnucash/app-utils/test/test-exp-parser.c:153-213` where it needs no Guile, and H1–H5 by hand;
  fuzzing of formulas with variables: no input panics.
- **Since Last Run**: `5.17:libgnucash/app-utils/test/test-sx.cpp` (`test_basic`, the once-only case,
  `test_state_changes`, auto-create), H6–H8 by hand; run twice on the same day it creates nothing; a stale proposal is
  `conflict`; a failing formula creates nothing; a postponement in an SQLite book is refused; 5,000 transactions in
  one Mutation within the memory budgets of [`../milestone3.md`](../milestone3.md).
- **The loan**: hand-worked repayments matching GnuCash's to the SCU (the note §9.3, H2–H3); the formula text for
  each rate type and locale.
- **Oracle**: GnuCash 5.17 opens every book red-ale wrote with no warnings and reads the scheduled transactions,
  templates, deferred instances and created transactions red-ale expects; its `xaccSchedXactionGetNextInstance` and
  `gnc_sx_get_num_occur_daterange` agree with red-ale's occurrences; its open-time numeric scrub would change nothing.
