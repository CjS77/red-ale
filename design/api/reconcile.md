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
| `reconcile.setState` | write  | `split`, `state` (`n` `c` `y` `f`), `date` (with `y` only; a Date, or a Timestamp as v0.5 took) | `split`, `transaction`, `state`, `reconcileDate` |

- The states are GnuCash's `n` (new), `c` (cleared), `y` (reconciled), `f` (frozen) and `v` (void)
  (`5.17:libgnucash/engine/Split.h:73-77`). `xaccSplitSetReconcile` accepts all five
  (`5.17:libgnucash/engine/Split.cpp:1818-1842`).
- **Refused** (red-ale enforces what GnuCash's dialogs enforce):
  - `v`, which comes only from voiding the transaction (`invalid`);
  - any change to a split of a voided transaction, or to a `v` split (`conflict`). GnuCash's register makes voided
    transactions read-only and its reconcile window never lists them (`5.17:gnucash/gnome/reconcile-view.c:381`);
  - `date` with a state other than `y` (`invalid`);
  - a split whose transaction is unbalanced (`unbalanced`), as for every red-ale mutation. Scrub it first.
- **`date`** is a Date, stamped as local 23:59:59 of that day, the time the reconcile window gives its statement date
  (`gnc_date_edit_get_date_end`, `5.17:gnucash/gnome/window-reconcile.cpp:931`) and so the one Finish stamps. A
  Timestamp is also taken, for an exact time: v0.5 took only that, and the wire stays additive.
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
  not. This un-clears cleared splits left unticked, as GnuCash does (`reconcile-view.c:988-995`): such a split is
  pre-ticked when the window opens, so in GnuCash it goes back to `n` only if the user unticks it; in red-ale, if
  `ticked` leaves it out. Later unticked splits are left alone, cleared or not.
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

GnuCash's statement date is the end of the chosen day in **local time** (`gnc_date_edit_get_date_end`). red-ale uses
the server's local time zone (see [`../api.md`](../api.md), "Dates, times and the local time zone"), with its
daylight-saving rules. A caller in another zone may give `utcOffsetMinutes` (within ±18 hours) to use that fixed
offset instead; v0.1 used 0 when it was absent.

- The statement date is stored as 23:59:59 local time on the chosen day.
- "Today" and the local dates used for defaults and intervals use the same zone.

## Auto-clear

Auto-clear finds the uncleared splits of one account that bring its cleared balance to a given figure, and marks them
cleared: GnuCash's Actions > Auto-clear (`5.17:gnucash/gnome/window-autoclear.c`), whose search is
`gnc_account_get_autoclear_splits` (`5.17:libgnucash/app-utils/gnc-autoclear.cpp:171-252`).

| Method                    | Effect | Params                       | Result |
|---------------------------|--------|------------------------------|--------|
| `reconcile.findAutoClear` | read   | `account`, `clearedBalance`  | `account`, `currentClearedBalance`, `clearedBalance`, `outcome`, `message`, `splits` |
| `reconcile.autoClear`     | write  | `account`, `clearedBalance`  | as `reconcile.findAutoClear`, with `outcome` always `found` |

`clearedBalance` is the cleared balance wanted, raw sign. `currentClearedBalance` is the account's cleared balance
now (every split not `n`), which GnuCash's window starts from (`xaccAccountGetClearedBalance`,
`window-autoclear.c:248-252`).
`splits` lists the splits found, in account order (`xaccSplitOrder`): `split`, `transaction`, `datePosted`,
`description`, `memo` and `amount`.

### The search

- **Units.** Amounts are compared as whole numbers of the account's SCU (`numeric_to_int64`,
  `gnc-autoclear.cpp:178-186`). The window rounds the figure typed to the SCU with banker's rounding
  (`GNC_HOW_RND_ROUND`, `window-autoclear.c:139-140`); so does red-ale.
- **Splits considered** (`:186-199`): every split of the account itself, not its sub-accounts, at any date: the window
  passes no date limit (`INT64_MAX`, `window-autoclear.c:142-146`).
  - A split in any state but `n` (`c`, `y`, `f` or `v`) counts as already cleared: its amount comes off the target.
  - An `n` split with a zero amount is skipped. The other `n` splits are the candidates.
- **Already there.** If the target is then zero, the outcome is `alreadyCleared` ("Account is already at Auto-Clear
  Balance.", `:201-207`).
- **Order** (`:209-216`): candidates sorted by size, largest first, and for equal sizes the positive one first. For
  each, the sums of the positive and of the negative amounts from it to the end are kept (`:218-225`).
- **Depth-first search** (`subset_sum`, `:111-169`): at each candidate, first take it, then leave it.
  - When the remaining target reaches zero, the splits taken so far are a solution, and that branch stops there:
    nothing after it is added. A second solution ends the search with `ambiguous` ("Cannot uniquely clear splits.
    Found multiple possibilities.").
  - A branch stops when the target lies outside what the remaining candidates can reach, between their negative and
    positive sums.
- **Outcomes** (`:236-247`, `gnc-autoclear.h:35-42`): `found` with exactly one solution, `ambiguous` with two,
  `unreachable` with none ("The selected amount cannot be cleared."), and `limit` when the search stops early.
- **Equal amounts.** Candidates of equal size may come in any order in GnuCash's unstable `std::sort`. The outcome
  does not depend on it: if one of two equal splits is in a solution and the other is not, swapping them gives a
  second solution, so the outcome is `ambiguous`. red-ale breaks ties by account order.

### Applying it

`reconcile.autoClear` runs the same search and, on `found`, sets each split to `c`, as the window's OK does
(`xaccSplitSetReconcile (…, CREC)`, `window-autoclear.c:159-164`). Reconcile dates are left alone. Any other outcome is
refused with `invalid`, `data.reason` set to the outcome. Like every Mutation, a split in an unbalanced transaction is
refused (`unbalanced`).

### Deviations

- **A step limit, not a time limit.** GnuCash gives up after 5 seconds (`MAX_AUTOCLEAR_SECONDS`,
  `window-autoclear.c:142`; checked every 100,000 steps, `gnc-autoclear.cpp:48-67`). red-ale stops after a fixed
  number of search steps (2²⁵), so the same book always gives the same answer. Either may stop where the other
  finishes.
- **Stopping early says so.** When GnuCash times out before finding any solution it reports "The selected amount
  cannot be cleared." (`:236-241`); red-ale reports `limit`.
- **Amounts off the SCU.** A split amount that is not a whole number of SCUs (possible in a book whose commodity's SCU
  changed) is refused with `invalid`. GnuCash's conversion fails silently there and searches with an error code in
  place of the amount.
- **Raw sign.** The window shows and takes the figure with credit accounts' sign flipped (`gnc_reverse_balance`,
  `:136-137`, `:250-251`), a display preference; red-ale uses the raw sign, as everywhere in reconciliation.

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
- **Unit:** `red-ale-core/src/ops/autoclear.rs` runs GnuCash's own auto-clear cases
  (`5.17:libgnucash/app-utils/test/test-autoclear.cpp`).
- **API:** `crates/red-ale-rpc/tests/reconcile.rs`; auto-clear in `crates/red-ale-rpc/tests/autoclear.rs`.
- **Oracle:** `crates/red-ale-testkit/tests/oracle_reconcile.rs`.
  - GnuCash 5.17 computes the default ending balance and the reconciled and cleared balances.
  - After Finish, Postpone, `setState` and `setInfo`, GnuCash reads back every state, reconcile date and
    `reconcile-info` slot red-ale wrote (`oracle/reconcile.py`, which reads through the engine's own getters).
  - `crates/red-ale-testkit/tests/oracle_autoclear.rs`: GnuCash's `gnc_account_get_autoclear_splits`, run on the same
    book (`oracle/autoclear.py`), picks the same splits or fails the same way, for found, ambiguous, unreachable and
    already-cleared targets; after `reconcile.autoClear`, GnuCash reads the cleared states back.
