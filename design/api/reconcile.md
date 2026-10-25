# API: reconciliation

Reconcile states, an account's reconcile information, and reconciling an account against a bank statement. Domain
code: `red-ale-core::ops::reconcile`; methods: `red-ale-rpc::methods::reconcile`. Research:
[`../notes/engine-accounts-transactions.md`](../notes/engine-accounts-transactions.md) §5.7 and §8.

All amounts here use the **raw sign**: debits positive, as GnuCash stores them. GnuCash's window shows credit accounts
with the sign flipped, but that is a display preference (`5.17:libgnucash/app-utils/gnc-ui-util.cpp:126-172`); the
postponed balance it stores is raw (`5.17:gnucash/gnome/window-reconcile.cpp:2506`).

## Reconcile states

| Method               | Effect | Params                                   | Result |
|----------------------|--------|------------------------------------------|--------|
| `reconcile.setState` | write  | `split`, `state` (`n` `c` `y` `f`), `date` (with `y` only) | `split`, `transaction`, `state`, `reconcileDate` |

- The states are GnuCash's `n` (new), `c` (cleared), `y` (reconciled), `f` (frozen) and `v` (void)
  (`5.17:libgnucash/engine/Split.h:73-77`). `xaccSplitSetReconcile` accepts all five
  (`5.17:libgnucash/engine/Split.cpp:1818-1842`).
- **Refused** (red-ale enforces what GnuCash's dialogs enforce):
  - `v`, which comes only from voiding the transaction (`invalid`);
  - any change to a split of a voided transaction, or to a `v` split (`conflict`). GnuCash's register makes voided
    transactions read-only and its reconcile window never lists them (`5.17:gnucash/gnome/reconcile-view.c:381`);
  - `date` with a state other than `y` (`invalid`);
  - a split whose transaction is unbalanced (`unbalanced`), as for every red-ale mutation. Scrub it first.
- **Deviation:** setting `y` stamps the reconcile date with `date`, or now. `xaccSplitSetReconcile` leaves the date
  alone, and only the reconcile window sets it (`5.17:gnucash/gnome/reconcile-view.c:931-937`). red-ale stamps it so a
  reconciled split always says when. Other states keep the stored date, as GnuCash does.
- Transactions that are read-only for another reason (posted invoices) can still be reconciled, as in GnuCash's window.

## Reconcile information

| Method              | Effect | Params | Result |
|---------------------|--------|--------|--------|
| `reconcile.info`    | read   | `account` | `account`, `lastDate`, `lastInterval {months, days}`, `postponeDate`, `postponeBalance`, `includeSubaccounts` |
| `reconcile.setInfo` | write  | `account`; any of `lastDate`, `lastInterval`, `postpone {date, balance}`, `includeSubaccounts` | as `reconcile.info` |

In `reconcile.setInfo` an absent field is left alone and `null` removes it.

The account slots, written with GnuCash's types (`5.17:libgnucash/engine/Account.cpp:4554-4668`, `:4870-4890`):

| Slot | Type | Notes |
|------|------|-------|
| `reconcile-info/last-date` | int64 seconds | |
| `reconcile-info/last-interval/months`, `/days` | int64 | read only when both are present; negative values refused |
| `reconcile-info/postpone/date` | int64 seconds | |
| `reconcile-info/postpone/balance` | numeric | raw sign |
| `reconcile-info/include-children` | int64 0 or 1 | read as a bool: non-zero, or the string `"true"` |

Clearing the postponed reconciliation deletes the whole `postpone` frame, as `xaccAccountClearReconcilePostpone`
does (`Account.cpp:4662-4665`).

## Reconciling against a statement

| Method               | Effect | Does |
|----------------------|--------|------|
| `reconcile.start`    | read   | Fill in GnuCash's defaults, list the splits to tick, and show the balances with the pre-ticked splits. |
| `reconcile.status`   | read   | The same for a given statement and set of ticks; `mark` and `unmark` add and remove ticks. |
| `reconcile.finish`   | write  | Ticked splits become `y`; the statement date becomes the last reconcile date. |
| `reconcile.postpone` | write  | Ticked splits become `c`; the statement date and balance are kept for next time. |

Params: `account`, `statementDate` (a date), `endingBalance`, `includeSubaccounts`, `utcOffsetMinutes`; `status`
also takes `ticked`, `mark` and `unmark`; `finish` and `postpone` take `ticked`, and `finish` takes `force`.

`start` and `status` return the statement (`statementDate`, `statementTime`, `endingBalance`,
`includeSubaccounts`, `resumedPostponed`, `lastReconcileDate`), the balances (`startingBalance`, `clearedBalance`,
`tickedBalance`, `reconciledBalance`, `difference`), `ticked`, and `splits` (each with its transaction, account, posted
date, description, memo, amount, state and whether it is ticked).

### Stateless by design

GnuCash keeps the ticks in its window's memory until Finish or Postpone; Postpone is the only way to keep them,
as `c` states. red-ale does the same without a window: **the caller carries the statement and its ticks from call to
call**, and nothing is kept in the server or the session.

- `red-ale exec` runs each command in its own process, so state kept in a session would not survive between calls.
- One server can have several clients, and nothing is left behind if a client goes away.
- Only `reconcile.finish` and `reconcile.postpone` change the book. Each is one atomic mutation, so a refusal leaves
  everything as it was.

### Defaults (`reconcile.start`)

- **Statement date** (`gnc_get_reconcile_info`, `5.17:gnucash/gnome/window-reconcile.cpp:1635-1675`):
  - today, when the account has never been reconciled;
  - otherwise the last reconcile date plus the last interval (one month if there is none). A month-end date stays at
    month ends (31 Jan → 29 Feb → 31 Mar). The result is no later than today;
  - a postponed statement's date overrides both.
  - GnuCash's preference to always reconcile to today is off by default and is not modelled.
- **Ending balance:**
  - the postponed balance, if the statement date was not changed;
  - otherwise the balance of the account (and sub-accounts, if included) from every split posted strictly before the
    end of the statement date (`xaccAccountGetBalanceAsOfDate`, `5.17:libgnucash/engine/Account.cpp:3505-3525`;
    `recn_date_changed_cb`, `window-reconcile.cpp:449-512`).
- **Include sub-accounts:**
  - defaults to the account's `include-children` slot;
  - is forced off, and refused if asked for, when a descendant holds another commodity
    (`has_account_different_commodities`, `window-reconcile.cpp:186-202`).

### Splits, ticks and the difference

- **Listed:** every `n` or `c` split of the account (and descendants if included), with **no date filter**, as
  GnuCash lists them (`5.17:gnucash/gnome/reconcile-view.c:358-399`). They are ordered by posted date.
- **Pre-ticked:** `c` splits posted on or before the statement date. This is GnuCash's default `check-cleared`
  preference (`reconcile-view.c:401-415`).
- **Refused ticks:** a tick that is not a listed split is refused (`invalid`).
- **Balances** (`recnRecalculateBalance`, `window-reconcile.cpp:327-391`):
  - `startingBalance` is every `y` and `f` split, whatever its date;
  - `reconciledBalance = startingBalance + tickedBalance`;
  - `difference = endingBalance − reconciledBalance`;
  - `clearedBalance` (every split not `n`) is for information.

### Finish (`recnFinishCB`, `window-reconcile.cpp:2419-2475`)

- **Refused** unless `difference` is zero (`invalid`). `force: true` finishes anyway, GnuCash's "The account is not
  balanced. Are you sure you want to finish?" (`:2428-2434`).
- **Each ticked split** becomes `y`, with the statement date as its reconcile date (`reconcile-view.c:931-937`).
  Unticked splits keep their state.
- **The account:**
  - the `postpone` frame is deleted;
  - `last-date` is set to the statement date;
  - `include-children` and `last-interval` are recorded as described under "Start records" below.
- **Result:** the splits reconciled, the statement time, the interval recorded and the final difference.

### Postpone (`recnPostponeCB`, `window-reconcile.cpp:2482-2510`)

- **Splits:** every listed split posted on or before the statement date, or ticked, becomes `c` if ticked and `n` if
  not. This un-clears cleared splits left unticked, as GnuCash does (`reconcile-view.c:961-998`). Later unticked
  splits are left alone.
- **The account:** `postpone/date` and `postpone/balance` are written; `last-date` is not touched. `include-children`
  and `last-interval` are recorded as described under "Start records" below.
- **Result:** the splits whose state changed.

### Start records

GnuCash writes `include-children` and `last-interval` when the start dialog's OK is pressed
(`window-reconcile.cpp:929-939`). red-ale writes them at Finish or Postpone, its only writes.

The interval rule (`gnc_save_reconcile_interval`, `:693-740`) uses `days = (statement − last date) / 86400`, with
integer division:

- if `days` is 28 and the stored interval is one month (or absent), it stays one month;
- otherwise, if `days` is over 28, it is the calendar-month difference;
- otherwise it is `days` days.

Nothing is stored without a previous last date, or if the result is negative.

### Time zone

GnuCash's statement date is the end of the chosen day in **local time** (`gnc_date_edit_get_date_end`). red-ale has no
local time zone, so the caller gives one: `utcOffsetMinutes` (default 0, within ±18 hours).

- The statement date is stored as 23:59:59 at that offset.
- "Today" and the local dates used for defaults and intervals use the same offset.
- **Deviation:** a fixed offset, not a time zone with daylight saving. Pass the offset in force on the statement date.

## Deviations from GnuCash, summarised

- `y` stamps the reconcile date (above).
- `include-children` and `last-interval` are written at Finish or Postpone rather than when the window opens, so
  cancelling (not calling either) records nothing.
- Unbalanced transactions are refused rather than silently balanced by GnuCash's commit-time scrub; use `scrub.*`.
- The interest-transfer prompt and the credit-card payment prompt after Finish are GUI conveniences, not modelled;
  use ordinary transfers.

## Tests

- **Unit:** `red-ale-core/src/ops/reconcile.rs`, covering the state rules, default dates, the interval rules, Finish
  and Postpone. A property test checks that Finish moves exactly the ticked amounts into the reconciled balance.
- **API:** `crates/red-ale-rpc/tests/reconcile.rs`.
- **Oracle:** `crates/red-ale-testkit/tests/oracle_reconcile.rs`.
  - GnuCash 5.17 computes the default ending balance and the reconciled and cleared balances.
  - After Finish, Postpone, `setState` and `setInfo`, GnuCash reads back every state, reconcile date and
    `reconcile-info` slot red-ale wrote (`oracle/reconcile.py`, which reads through the engine's own getters).
