# API: lots, capital gains and stock splits

Lots group the splits of one account that buy and then sell the same holding. A lot is *closed* when it has splits
and their amounts sum to zero. Capital gains are recorded per selling split in a separate *gains transaction*. The
stock split assistant records a change in the number of shares held.

Code: `red-ale-core/src/ops/lots.rs` (with `lots/ledger.rs`, `gains.rs`, `scrub.rs`, `stock_split.rs`) and
`red-ale-rpc/src/methods/lots.rs`. Research notes: `notes/engine-commodities-prices-lots.md` §5-8.

## Terms

- **Adjusted amount.** A split's amount restated in the units after any later stock split in its account
  (`xaccAccountRecomputeBalance`, `5.17:libgnucash/engine/Account.cpp:2263-2330`). Lot balances, closure, FIFO and
  gains use adjusted amounts. New in 5.17; not stored in the file.
- **Gains transaction.** A two-split transaction, description `Realized Gain/Loss`: a *gains split* in the lot (amount
  0, value = the gain, slot `gains-source` naming the selling split) and a split in the gains account (amount and value
  = minus the gain). The selling split names the gains split in its slot `gains-split`.
- **Gains account.** The account a lot account's slot `lot-mgmt/gains-acct/<CURRENCY::XXX>` names; when none is set,
  an INCOME account `Orphaned Gains-<XXX>` under the root, found by name or made, which the slot then names
  (`xaccAccountGainsAccount`, `5.17:libgnucash/engine/Account.cpp:4760-4817`).
- **Cut split.** A split cut in two because it would overfill a lot. Both pieces carry a `lot-split` frame
  (`peer_guid`, `date`) naming the other; a second cut overwrites the frame
  (`5.17:libgnucash/engine/cap-gains.cpp:321-426`, `qofinstance.cpp:1130-1141`).
- **Stock-split split.** A split with slot `split-type = stock-split`, value `0/1` and action `Split`. In 5.17 such
  splits never join lots and never produce gains (`5.17:libgnucash/engine/Scrub2.cpp:72-73`, `:122-128`,
  `cap-gains.cpp:580-584`).
- **Lot policy.** FIFO, the only policy 5.17 implements (`5.17:libgnucash/engine/policy.cpp:118-161`).

## Methods

| Method               | Effect      | Does                                                                                   |
|----------------------|-------------|----------------------------------------------------------------------------------------|
| `lot.list`           | read        | An account's lots as the lot viewer lists them; `state`: `all` (default), `open`, `closed`. |
| `lot.get`            | read        | One lot with its splits.                                                               |
| `lot.freeSplits`     | read        | An account's splits in no lot; with `lot`, each with the gain it would realise against it. |
| `lot.create`         | write       | An empty lot titled `Lot <n>`, then the `title` and `notes` given.                    |
| `lot.update`         | write       | Set `title` or `notes`; an empty string removes one.                                   |
| `lot.delete`         | destructive | Delete a lot. A lot with splits needs `releaseSplits: true`; its splits stay, in no lot. |
| `lot.assignSplit`    | write       | Put `split` in `lot`, cut if it would overfill it; without `lot`, FIFO chooses.        |
| `lot.removeSplit`    | write       | Take a split out of its lot; a lot left empty is deleted.                              |
| `lot.scrub`          | write       | Repair one lot and recompute its gains (`xaccScrubLot`).                               |
| `lot.scrubAccount`   | write       | Assign an account's splits to lots and repair each lot (`xaccAccountScrubLots`); `subaccounts: true` does the whole subtree first (`xaccAccountTreeScrubLots`). |
| `lot.computeGains`   | write       | Recompute a lot's gains (`xaccLotComputeCapGains`), optionally into `gainsAccount`.    |
| `stock.split`        | write       | Record a stock split, with optional price and cash in lieu.                            |

### Results

`LotInfo` (from `lot.list`, `lot.create`, `lot.update`; `lot.get` adds `splits`): `guid`, `account`, `title`,
`notes`, `invoice` (the invoice a business lot belongs to: the viewer's type `I`), `closed`, `openDate` (local date
of the earliest split's posted time, as the viewer prints it, `5.17:gnucash/gnome/dialog-lot-viewer.c:845`), `closeDate` (of the latest split, when closed), `balance` (sum of adjusted amounts),
`gainsCurrency`, `realizedGains`, `splitCount`. Lots are listed opened earliest first, empty lots last.

`LotSplitInfo`: `split`, `transaction`, `datePosted`, `num`, `description`, `memo`, `action`, `amount`,
`adjustedAmount`, `value`, `currency`, `gain`, `kind` (`normal`, `gains` or `stockSplit`), `number` (the Num column the
viewer shows: the action when the num field source is on, else `num`; [`book-options.md`](book-options.md)). The viewer hides
stock-split splits, and splits with a zero adjusted amount (gains splits among them) outside business lots
(`5.17:gnucash/gnome/dialog-lot-viewer.c:512-515`); the API returns them all so callers can choose.

`lot.scrub`, `lot.scrubAccount` and `lot.computeGains` return what changed: `lotsCreated`, `lotsDeleted`,
`transactionsCreated` (gains transactions), `transactionsChanged` (cut or merged splits, updated gains),
`transactionsDeleted`, `accountsCreated` (`Orphaned Gains-<CUR>`).

`lot.assignSplit` returns `pieces` (each split piece and its lot), `rest` (with an explicit lot: the piece left in no
lot) and `newLots` (lots FIFO opened).

`stock.split` returns `transaction`, `priceStatus` (`none`, `added`, `duplicate`, `rejected`), `price` and
`blockingPrice`.

## GnuCash behaviour matched

### Lots (`5.17:libgnucash/engine/gnc-lot.cpp`)

- A new lot (`lot.create`, and every lot FIFO opens) is titled `Lot <n>` from the account's `lot-mgmt/next-id` slot,
  which then counts up (`gnc_lot_make_default`, `:765-783`). The title is English, as decided in
  `open-questions.md`.
- A split joins only a lot of its own account (`gnc_lot_add_split`, `:579-628`).
- Balance is the sum of adjusted amounts; closed means it has splits and the balance is zero (`:367-376`,
  `:487-525`).
- A lot's splits are ordered by posted date, ties in the order they joined (`gnc_lot_get_earliest_split`, `:658-685`,
  with `xaccSplitOrderDateOnly`, `5.17:libgnucash/engine/Split.cpp:1600-1618`); the earliest is FIFO's opening split.
- An account's lot list is the Book's order reversed, because GnuCash prepends each lot when it loads or makes one
  (`xaccAccountInsertLot`, `5.17:libgnucash/engine/Account.cpp:2150`). FIFO breaks ties between lots opened the same
  day by this order.
- Removing a lot's last split detaches the lot from its account, so GnuCash no longer writes it (`:631-652`); red-ale
  deletes it.
- Deleting a lot takes its splits out of it; an invoice's lot cannot be deleted
  (`5.17:gnucash/gnome/dialog-lot-viewer.c:795-806`). The posting split of an invoice's own lot cannot be removed
  (`:472-487`).

### Assignment (`5.17:libgnucash/engine/cap-gains.cpp`, `policy.cpp`, `Scrub2.cpp`)

- `xaccSplitAssignToLot` (`:218-427`): a split joins an empty lot, a lot it enlarges, or a lot it fits; otherwise it is
  cut so that the first piece closes the lot. The piece's value is the split's value prorated by amount, rounded half
  up at the value's denominator; the rest keeps the remainder. The new piece copies memo, action, reconcile state and
  date. A closed lot refuses (`conflict`).
- FIFO lot (`xaccAccountFindEarliestOpenLot`, `:124-200`): of the open lots whose opening split's amount has the
  opposite sign, that are not overfull and share the transaction currency, the one opened first. None found: a new
  lot.
- FIFO fill (`DirectionPolicyGetSplit`, `policy.cpp:67-114`): the first unlotted split in account order
  (`xaccSplitOrder`), posted no earlier than the lot's latest split, in its currency, of the sign that shrinks it.
- Accounts with trades (`xaccAccountHasTrades`, `:79-99`): STOCK, MUTUAL and CURRENCY accounts, and any account with a
  split in a transaction whose currency differs from the account's commodity. Only they are scrubbed.
- `xaccAccountAssignLots` (`Scrub2.cpp:58-83`) skips stock-split splits and zeroed splits of void transactions.

### Gains (`xaccSplitComputeCapGains`, `5.17:libgnucash/engine/cap-gains.cpp:521-891`)

- No gains when the transaction currency is the account's commodity, for a lot's opening split, for stock-split
  splits, for zero amounts, when the opening split's currency differs, or when the lot is too thin or too fat for the
  split.
- `basis = (adjusted amount / lot amount before) × lot value before`, rounded half up at the opening value's
  denominator; `gain = basis − value`. A zero gain records nothing (an existing gains transaction is not zeroed).
  "Before" follows `gnc_lot_get_balance_before` (`5.17:libgnucash/engine/gnc-lot.cpp:529-576`), placing each gains
  split at its source's transaction in `xaccTransOrder`.
- A new gains transaction: currency of the sale, posted at the sale's posted time with no `date-posted` slot, entered
  now, no number; gains split first with amount `0` at the account's SCU; memos `Realized Gain/Loss`. An existing one
  with other than two splits is left alone (`:827-832`); otherwise it is brought up to date.
- `lot.scrub` (`xaccScrubLot`, `5.17:libgnucash/engine/Scrub3.cpp:84-154`): merge cut pieces in the lot, kick all but the
  opening split out of an overfull lot and refill it, merge again, and compute gains when the account's commodity is
  not the currency of the lot's first split.
- Lot repair skips stock-split splits, as in 5.17.
- During one scrub GnuCash reads a split whose amount the scrub set at its raw amount, not its adjusted one, until the
  scrub ends and balances are recomputed (`xaccSplitSetAmount`, `5.17:libgnucash/engine/Split.cpp:1260`; recompute is
  deferred while the account is being edited, `Account.cpp:2268`). When a sale before a stock split is cut, this can
  overfill a lot. red-ale reproduces it, so its lots and gains match GnuCash's (oracle:
  `red_ale_scrubs_lots_as_gnucash_does`).

### Stock split assistant (`gnc_stock_split_assistant_finish`, `5.17:gnucash/gnome/assistant-stock-split.c:339-473`)

- The account must be STOCK, MUTUAL or CURRENCY, not a placeholder, and hold shares (`:111-186`). Shares must be
  non-zero and are rounded to the account's SCU; negative means a reverse split.
- The transaction: posted on the date (neutral time and `date-posted` slot), description `Stock Split` unless given;
  one split in the account with the shares as amount, value `0/1`, action `Split` and slot `split-type =
  stock-split` (`xaccSplitMakeStockSplit`, `5.17:libgnucash/engine/Split.cpp:2063-2075`).
- A positive price adds a price at the date's neutral time, source `user:stock-split`, type `unknown`, in the
  price currency (default: the other side of the security's latest price, else the transaction currency, as
  `refresh_details_page` does, `:190-249`). GnuCash's same-day rules apply (`add_price`,
  `5.17:libgnucash/engine/gnc-pricedb.cpp:1013-1110`): a same-day price from a source of higher precedence blocks it,
  and the transaction is still recorded, as GnuCash only shows "Error adding price.".
- Positive cash in lieu adds a debit to the BANK, CASH or ASSET account and a credit to the INCOME account, amount
  equal to value, memo `Cash in lieu` unless given (`:309-335`, `:400-450`, `:485-504`).

## Deviations

1. **Gains transactions are written only when they change.** The first time GnuCash meets a gains transaction in a
   session it rewrites it, setting date entered to now, because the in-memory links it checks are not yet set
   (`5.17:libgnucash/engine/cap-gains.cpp:837-846`; `xaccSplitDetermineGainStatus`, `Split.cpp:1110-1142`). red-ale
   writes one only when its values, amounts, currency, posted date or lot differ, so a scrub that changes nothing
   leaves the book unchanged.
2. **Merging cut pieces deletes the merged piece's gains transaction.** GnuCash deletes it only when it has already
   looked at that piece's gains in the same session (`merge_splits`, `5.17:libgnucash/engine/Scrub2.cpp:308-315`); after
   a fresh open it leaves the gains transaction behind and the gain is counted twice. red-ale always deletes it.
3. **Gains splits follow their slots.** When GnuCash meets a gains split before the sale it records (a sale taken out of
   its lot and put back), it does not know the sale's gains split and makes a second gains transaction
   (`5.17:libgnucash/engine/cap-gains.cpp:586-612`, `:767-769`). red-ale reads the `gains-split` slot and updates the one
   it has.
4. **Stock split currency.** GnuCash uses the user's default currency preference, which a book does not hold. red-ale
   takes `currency`, defaulting to the currency of the nearest ancestor account held in a currency, and refuses when
   there is none.
5. **Same day is a local day** for the stock split's price, as in GnuCash (v0.1 used UTC days).
6. **Explicit lot edits only.** The lot viewer writes the title and notes of a lot whenever the selection moves, storing
   empty strings (`5.17:gnucash/gnome/dialog-lot-viewer.c:236-258`); `lot.update` changes only what it is given, and
   an empty string removes the slot.
7. **Deleting a lot with splits needs `releaseSplits`.** The viewer deletes any lot that is not an invoice's; the API
   asks for the release to be explicit.
8. **Read-only gains in the viewer.** GnuCash's viewer recomputes a split's gains when they are stale, which can write
   to the book (`xaccSplitGetCapGains`, `cap-gains.cpp:1037-1065`); `lot.get` reports the recorded gains and never
   writes. Call `lot.scrub` or `lot.computeGains` to recompute.
9. **Order approximations.** A lot's splits joined it in an order GnuCash keeps only in memory; red-ale uses Book order
   (transaction, then split), which is the order GnuCash builds when it loads a file. Text ties in account order
   (`xaccSplitOrder`, `xaccTransOrder`) compare bytes, where GnuCash uses locale collation.
10. **`lot.assignSplit` with a lot** cuts a split that would overfill it, as `xaccSplitAssignToLot` does, where the lot
    viewer's Add button adds it whole (`gnc_lot_add_split`, `dialog-lot-viewer.c:724-740`). The rest is returned in
    `rest`, in no lot.

## Tests

- Unit and property tests: `red-ale-core/src/ops/lots/tests.rs` (FIFO cuts and gains, value rounding and peer links,
  stock splits, idempotent scrub, gains updates, merges, CRUD, viewer, assistant; property: scrubbed closed lots
  double-balance and a second scrub changes nothing).
- API: `red-ale-testkit/tests/lots.rs`.
- Oracle: `red-ale-testkit/tests/oracle_lots.rs` compares red-ale's scrub with GnuCash's `xaccAccountTreeScrubLots` on
  the same book (lots, cut splits, gains transactions and their slots), a rescrub after an edit, the stock split with
  the assistant's code replayed in GnuCash, and books written by every lot method.
