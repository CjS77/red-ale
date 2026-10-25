# API: trading accounts

The Book option "Use Trading Accounts", the TRADING account tree, and the trading splits every commit keeps. Domain
code: `crates/red-ale-core/src/ops/trading.rs`, applied from `ops/commit.rs`; option: `book_options.rs`; methods:
`crates/red-ale-rpc/src/methods/book_options.rs`. Research: [`notes/engine-accounts-transactions.md`](../notes/engine-accounts-transactions.md)
§9.6 and [`notes/storage-and-safety.md`](../notes/storage-and-safety.md) §11.

With the option on, a transaction that moves value between commodities also records, in TRADING accounts, how much
of each commodity was bought or sold, so that every commodity's amounts sum to zero inside the transaction as well as
its values. GnuCash adds and replaces these splits itself every time a transaction is committed; red-ale does the
same, in the same accounts, with the same figures.

Books using trading accounts were read-only in v0.1. From v0.2 they open for editing.

## The option

The Book slot `options/Accounts/Use Trading Accounts` (string). On when it is exactly `"t"`
(`5.17:libgnucash/engine/qofbook.cpp:919-928`).

It is one of the Book options of [`book-options.md`](book-options.md):

- `book.options` returns it as `useTradingAccounts: bool`.
- `book.setOptions` takes `useTradingAccounts: bool`, applied as GnuCash's book options dialog applies it:
  - an unchanged value writes nothing; a change writes `"t"` or `"f"` (`5.17:libgnucash/engine/gnc-optiondb.cpp:404-410`),
    never deleting the slot;
  - no transaction changes. As in GnuCash, an existing transaction gets trading splits when it is next changed.

## Account type

TRADING is an ordinary account type for `account.create` and `account.update`: any commodity
(`5.17:gnucash/gnome-utils/dialog-account.c:224-241`), parent TRADING or the root, never changing type once it has
splits. GnuCash offers no opening balance for it (`:1340-1344`).

## Balanced, with trading accounts

With the option on, a transaction is balanced when (`xaccTransIsBalanced`, `5.17:libgnucash/engine/Transaction.cpp:1078-1121`):

1. the values of its non-trading splits sum to zero;
2. the values of its trading splits sum to zero;
3. for each commodity, the amounts of the splits whose account holds it sum to zero (`xaccTransGetImbalance`,
   `:1006-1076`).

With the option off, balanced means the values sum to zero, as in v0.1.

`TransactionInfo.balanced` follows the Book's option. Trading splits appear in `splits` like any other, with
`accountName` such as `Trading:CURRENCY:EUR`.

## The commit rule

Every Mutation that creates or changes a transaction commits it. With the option on, the commit runs GnuCash's
trading branch of `xaccTransScrubImbalance` (`5.17:libgnucash/engine/Scrub.cpp:844-895`):

1. If the transaction is balanced (above), nothing changes. **Trading splits of a balanced transaction are kept as
   they are**, whether GnuCash, red-ale or the caller made them.
2. Otherwise every split in a TRADING account is removed.
3. If the values of the remaining splits do not sum to zero, the Mutation is refused with `unbalanced`. (GnuCash adds
   an `Imbalance-<CUR>` split here; red-ale refuses unbalanced input, as for every commit, except in
   `scrub.imbalance`, which adds it as GnuCash does.)
4. For each commodity whose amounts do not sum to zero, in GnuCash's order (the newest commodity first, the
   transaction currency last), a split in that commodity's trading account takes:
   - amount = −(the commodity's amount imbalance), rounded half up to the commodity's fraction and then to the
     account's SCU;
   - value = the amount, for the transaction currency; otherwise −(the sum of the values of the commodity's splits),
     rounded half up to the currency's fraction.
5. If the values still do not sum to zero, each split with a value and a zero amount (a realized gain) is offset by
   the value of the trading split of its account's commodity.
6. Splits are sorted as every commit sorts them, debits first; new trading splits follow the existing splits of the
   same sign.

Where this runs:

- `transaction.post`, `transaction.update`, `transaction.transfer`, `transaction.reverse` (which first commits the
  original, as `xaccTransReverse` does), `transaction.void` and `transaction.unvoid`, `stock.split`, and journal
  replay commit through `ops::commit`, so their results already show the trading splits.
- **Every other transaction a Mutation adds or changes** — capital gains, business posting and payments, reconcile,
  lot assignment, delete with move — is committed by the session after the Mutation's own work and before its
  checks, as GnuCash's `xaccTransCommitEdit` runs after every setter. A method whose result was built before that
  does not show the trading splits; `transaction.get` does.
- The session then checks every added or changed transaction is balanced by the rule above, and refuses the
  Mutation if not.

Deleting a transaction commits nothing. Turning the option on or off changes no transaction.

## The trading account tree

The trading split for commodity `NS::MN` is in `Trading:NS:MN`, found or created as `get_trading_split` does
(`5.17:libgnucash/engine/Scrub.cpp:518-579`, `xaccScrubUtilityGetOrMakeAccount`, `:1507-1540`):

| Level | Under | Found by (type TRADING) | Created with |
|---|---|---|---|
| `Trading` | root | any TRADING account; the name is not checked | placeholder, the root currency |
| `NS` (e.g. `CURRENCY`, `NASDAQ`) | `Trading` | name | placeholder, the root currency |
| `MN` (e.g. `EUR`, `ACME`) | `NS` | commodity; the name is not checked | not placeholder, the commodity, its fraction as SCU |

- Lookup is GnuCash's (`gnc_account_lookup_by_type_and_commodity`, `5.17:libgnucash/engine/Account.cpp:3176-3210`):
  direct children first, last match first; only if none, each child's matches, last child first. Among several
  matches, one in the root currency (when no commodity is asked for), then one with the name asked for, then the
  first.
- New accounts are appended under their parent.
- The root currency is the root account's commodity, else that of the first top-level INCOME account that has one
  (`find_root_currency`, `Scrub.cpp:493-516`); with neither, `Trading` and its namespace accounts have no commodity.

## Scrubs

`scrub.imbalance` and `scrub.checkAndRepair` run the whole trading branch: steps 1–6 above, with step 3 adding or
adjusting an `Imbalance-<CUR>` split as without trading accounts. New repair kinds:

| `kind` | Fields | Meaning |
|--------|--------|---------|
| `tradingSplitRemoved` | `transaction`, `split`, `account`, `value`, `amount` | A trading split was removed before rebalancing. |
| `tradingSplitAdded` | `transaction`, `split`, `account`, `value`, `amount` | A trading split was added. |

Trading accounts the scrub creates are reported as `accountCreated`.

## Deviations from GnuCash

- **Unbalanced values are refused** outside the scrubs (step 3), as for every red-ale commit.
- **Names are not translated.** GnuCash names the top account `_("Trading")` (`Scrub.cpp:532`); red-ale always uses
  `Trading`. A translated top account is still found, since that lookup ignores names.
- **Order within one Mutation.** When the session commits several transactions of one Mutation, it does so in book
  order; GnuCash commits them in the order the operation touched them. This decides only which transaction creates
  a new trading account first, and so the order of siblings under a namespace account.
- GnuCash's commit first runs the split scrub (`xaccTransScrubSplits`); red-ale's commits do not, as in v0.1. The
  scrubs run it.

## Tests

- **Unit and property:** `ops/trading.rs`. Every committed multi-commodity transaction balances by the rule above;
  trading splits of balanced transactions are untouched; the tree is found and made as GnuCash does.
- **API:** `crates/red-ale-rpc/tests/trading.rs`: the option, posting, editing, voiding and reversing in a trading
  book, refusals.
- **Round-trip:** `piecash/simple_sample.gnucash` and `piecash/investment.gnucash` open for editing and save unedited
  as the same book.
- **Oracle:** `crates/red-ale-testkit/tests/oracle_trading.rs`. GnuCash 5.17 posts the same multi-commodity
  transactions in a book with trading accounts (adding the trading splits itself) as red-ale posts through the API,
  and the two books must match split for split, in stored order, with the same trading tree. GnuCash's imbalance
  scrub and red-ale's `scrub.checkAndRepair` rebuild the same trading splits on a book whose transactions predate the
  option. Piecash books saved unedited and edited by red-ale (post, update, reverse, void) read the same in GnuCash,
  every transaction is balanced by `xaccTransIsBalanced`, GnuCash's own imbalance scrub changes nothing and its
  engine logs no warning. GnuCash's side runs in `oracle/trading.py`, which sets the current session to the open
  book as the GUI does; `oracle/scrub.py` now does too.
