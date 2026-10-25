# API: scrubs and Check & Repair

GnuCash's data-integrity repairs: the imbalance scrub, the orphan scrub, the currency scrub and Check & Repair.
Domain code: `red-ale-core::ops::scrub`; methods: `red-ale-rpc::methods::scrub`. Research:
[`../notes/engine-accounts-transactions.md`](../notes/engine-accounts-transactions.md) §9.

## Methods

| Method                 | Effect      | Does |
|------------------------|-------------|------|
| `scrub.imbalance`      | destructive | Balance unbalanced transactions with an `Imbalance-<CUR>` split, after fixing orphans and split amounts. |
| `scrub.orphans`        | destructive | Put splits with no account into `Orphan-<CUR>`, balancing each transaction changed. |
| `scrub.currency`       | destructive | Give a transaction whose currency is not a currency the common currency of its splits. |
| `scrub.checkAndRepair` | destructive | GnuCash's Check & Repair: orphans, then imbalances, then (with `lots`) the lot scrub. |

**Params:**

- Give a `transaction`; or an `account` (a GUID or full name), with `subaccounts: true` to include its descendants;
  or neither, for the whole book.
- `scrub.checkAndRepair` also takes `lots` (default false).
- Giving both a transaction and an account, or `subaccounts` without an account, is `invalid-params`.

**Result:** `{ "repairs": [...] }`, every change in the order it was made. Each repair has a `kind`:

| `kind` | Fields | Meaning |
|--------|--------|---------|
| `accountCreated` | `account`, `name`, `commodity` | An `Imbalance-<CUR>` or `Orphan-<CUR>` account was created under the root. |
| `orphanAssigned` | `transaction`, `split`, `account` | A split with no account was put in `Orphan-<CUR>`. |
| `invalidNumberZeroed` | `transaction`, `split`, `field` | An invalid value or amount (zero denominator) was set to zero. |
| `amountSetToValue` | `transaction`, `split`, `from`, `to` | In an account of the transaction currency, the amount was set to the value. |
| `currencySet` | `transaction`, `from`, `to` | The transaction got a currency. |
| `valueChanged` | `transaction`, `split`, `from`, `to` | A value was re-expressed in the new currency. |
| `balanceSplitAdded` | `transaction`, `split`, `account`, `value` | A balancing split was added. |
| `balanceSplitChanged` | `transaction`, `split`, `account`, `from`, `to` | An existing balancing split was adjusted. |
| `balanceSplitRemoved` | `transaction`, `split`, `account`, `value` | A balancing split that came to zero, with no memo or action, was deleted. |
| `lotsRepaired` | `account` | The lot scrub changed the account's lots or splits. |
| `tradingSplitRemoved`, `tradingSplitAdded` | see [`trading-accounts.md`](trading-accounts.md) | Trading splits replaced, in a book with trading accounts. |

**Effect:** each method is one atomic mutation. Scrubs exist to change data, and can delete a split, so all four are
marked `destructive`. Nothing is left half-repaired: if any step fails, the book is unchanged.

## Scrubs run only on request

GnuCash scrubs every XML book while it loads it (`5.17:libgnucash/backend/xml/io-gncxml-v2.cpp:282-283`, `:827-833`).
Per transaction it runs the currency scrub, which first runs the orphan scrub; then it runs the split scrub over the
whole tree.

**red-ale does not.** It keeps a book as read, and these methods run the same repairs only when asked. A book opened
by red-ale can therefore hold an orphan split or a mismatched amount that GnuCash would quietly fix when it next
opens the book. Running `scrub.checkAndRepair` leaves nothing for GnuCash's load-time scrubs to change.

## What each step does

### Imbalance (`xaccTransScrubImbalance`, `5.17:libgnucash/engine/Scrub.cpp:845-899`)

1. **Scrub the splits** (`xaccTransScrubSplits`, `:236-430`):
   1. orphans first;
   2. an invalid value or amount becomes zero;
   3. for each split whose account holds the transaction currency, if the amount and value differ once both are
      rounded to `min(account SCU, currency fraction)`, the amount becomes the value, rounded to the account's SCU.
      Smaller differences are tolerated.
2. **If the values sum to zero**, stop.
3. **Otherwise find the balancing account** `Imbalance-<CUR>`: a BANK account in the transaction currency with that
   name, looked for anywhere in the tree (`gnc_account_lookup_by_type_and_commodity`,
   `5.17:libgnucash/engine/Account.cpp:3176-3210`).
   - Matches among a parent's direct children win over deeper ones.
   - Among several matches, GnuCash's list order applies (last child first).
   - **If none exists**, it is created (`construct_account`, `Scrub.cpp:1457-1475`): type BANK, the transaction
     currency, SCU equal to the currency's fraction, not a placeholder, appended as the root's last child.
4. **Adjust or add the split.**
   - The transaction's split in that account is used, or a new one is added.
   - Its value becomes `old − imbalance`, rounded to the currency fraction (ROUND_HALF_UP), and its amount the same at
     the account's SCU.
   - If the new value is zero and the split has no memo and no action, the split is **deleted** (`:607-619`). This
     matches 5.13 and later (commit 048dfa1232, Bug 343711). A balanced transaction holding a zero Imbalance split is
     left alone.
   - Splits are re-sorted (value ≥ 0 first), as commit does.

With trading accounts on, "balanced" is `xaccTransIsBalanced`'s rule and the trading branch runs instead: see
[`trading-accounts.md`](trading-accounts.md), "Scrubs".

### Orphans (`TransScrubOrphansFast`, `Scrub.cpp:110-136`)

Every split with no account goes to `Orphan-<CUR>` (found or created as above, with the transaction currency).

A split whose account GUID names no account in the book is an orphan too. GnuCash's XML reader leaves such a split
with no account (`5.17:libgnucash/backend/xml/gnc-transaction-xml-v2.cpp:273-292`). (A split with no `split:account`
element at all is dropped by GnuCash's reader, so it cannot be repaired in GnuCash.)

### Currency (`xaccTransScrubCurrency`, `Scrub.cpp:1120-1235`)

1. Run the orphan scrub.
2. If the transaction currency is a currency, stop.
3. **Choose a currency** (`xaccTransFindCommonCurrency`, `:1055-1118`):
   - each non-trading split counts 3 for its account's currency, or 1 for the nearest ancestor's currency;
   - GnuCash sorts the tallies ascending with a stable sort and takes the head, so the **lowest** tally wins and ties
     go to the first seen;
   - red-ale does the same, and the oracle test confirms the tie case.
4. **Set the currency** (`xaccTransSetCurrency`, `5.17:libgnucash/engine/Transaction.cpp:1257-1335`):
   - the rate is `amount / value` of the first split in the new currency;
   - splits in the new currency take their amount as value;
   - splits in the old commodity are priced at the rate;
   - other splits are priced at their own price over the rate;
   - without a rate, values are re-rounded.
5. **Fix amounts:** splits in the new currency whose amount differs from their value get amount = value.
6. **Fallbacks:** if no currency is found, the first split's account commodity is used. The older fallback through the
   obsolete per-account currency slots is not modelled.

### Commit-time balancing

GnuCash runs the imbalance scrub every time a transaction is committed
(`5.17:libgnucash/engine/Transaction.cpp:1549-1566`). So assigning an orphan or changing the currency also balances
that transaction, and red-ale does the same. Every transaction a scrub changes comes out balanced, which red-ale's
session also requires.

## Scopes

| Scope | Imbalance | Orphans | Check & Repair |
|-------|-----------|---------|----------------|
| `transaction` | orphans, then imbalance: the register's Check & Repair (`5.17:gnucash/gnome/gnc-plugin-page-register.cpp:3542-3680`) | that transaction | as imbalance; `lots` is refused |
| `account` | `xaccAccountScrubImbalance`: for each transaction with a split in the account, orphans, currency, then imbalance (`Scrub.cpp:353-391`) | `xaccAccountScrubOrphans` | "Check & Repair Account" |
| `account` + `subaccounts` | `xaccAccountTreeScrubImbalance` | `xaccAccountTreeScrubOrphans` | "Check & Repair Subaccounts" |
| none | every transaction in the book | every transaction | "Check & Repair All" |

**Check & Repair** (`5.17:gnucash/gnome/gnc-plugin-page-account-tree.cpp:1940-2027`) runs, in order:

1. the orphan scrub;
2. the imbalance scrub;
3. with `lots: true` only, the lot scrub of each account, descendants before the account (`xaccAccountTreeScrubLots`,
   `5.17:libgnucash/engine/Scrub3.cpp:190-196`). GnuCash 5.17 runs this step only when the `GNC_AUTO_SCRUB_LOTS`
   environment variable is set, hence the default. The lot scrub itself belongs to `ops::lots::scrub_lots`.

## Deviations from GnuCash

- **Account names are not translated.** GnuCash names the accounts `_("Imbalance")-<CUR>` and `_("Orphan")-<CUR>`
  (`Scrub.cpp:121`, `:464`), so a GnuCash running in another language creates and looks for translated names.
  red-ale always uses `Imbalance-` and `Orphan-`. A French user's existing `Déséquilibre-EUR` account is therefore
  not found, and red-ale creates `Imbalance-EUR` beside it.
- **Order.** GnuCash processes an account's transactions in an unspecified order (an `unordered_set`,
  `Scrub.cpp:96-106`); red-ale uses book order. This only decides which transaction creates a new account first.
- **Whole-book scope.** "All" covers every transaction in the book, including one whose splits are all orphans.
  GnuCash reaches transactions only through accounts, but its load-time scrub repairs those too.
- **Not modelled:**
  - giving an account with no commodity a commodity (`xaccAccountScrubCommodity`); such splits are left alone;
  - the business scrub that Check & Repair ends with (`gncScrubBusinessAccount`,
    `5.17:libgnucash/engine/ScrubBusiness.c:736-762`), which belongs to the business package.

## Tests

- **Unit:** `red-ale-core/src/ops/scrub.rs`. A property test checks that after the book's imbalance scrub every
  transaction balances and only balancing splits changed.
- **API:** `crates/red-ale-rpc/tests/scrub.rs`, covering scopes, refusals, read-only books and effects.
- **Oracle:** `crates/red-ale-testkit/tests/oracle_scrub.rs`. Two broken copies of `synthetic/rich.gnucash` are used.
  - The breakages: an unbalanced transaction, a split whose account is missing, an amount that differs from its
    value, an Imbalance split that exactly absorbs its transaction's imbalance, and two transactions whose currency
    is a security (one with a EUR/USD tie).
  - GnuCash 5.17 scrubs one copy itself: its load-time scrubs, then the imbalance step of Check & Repair through
    `libgnc-engine.so` (`oracle/scrub.py`).
  - red-ale scrubs the other copy through the API.
  - GnuCash must see the same accounts and transactions in both, and must read red-ale's book exactly as red-ale
    does.
