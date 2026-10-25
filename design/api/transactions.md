# API: transactions

Methods for transactions and their splits. The rules are in `red-ale-core::ops::transactions` (`ops/transactions.rs`
and its `void`, `find`, `transfer` and `order` modules), and they build on `ops::commit`. Research notes:
[`../notes/engine-accounts-transactions.md`](../notes/engine-accounts-transactions.md) §1.5, §5, §6, §7 and §10.
Citations are to GnuCash 5.17 source.

Conventions used throughout:

- A split's **value** is in the transaction currency; its **amount** (GnuCash's quantity) is in the account's
  commodity. Both are kept exactly, and neither is derived from the other on save.
- **Every mutation is atomic and leaves every transaction balanced.** GnuCash balances an unbalanced transaction at
  commit by adding an `Imbalance-<CUR>` split (`Transaction.cpp:1548-1560`). red-ale refuses with `unbalanced`
  instead.
- The **commit rules** apply to every created or edited transaction, as GnuCash's setters and commit apply them:
  - values are rounded half up to the currency's fraction, and amounts to the account's SCU. A non-zero number that
    would round to zero is refused (`Split.cpp:1242-1312`);
  - splits are stably sorted with debits (value ≥ 0) first (`Transaction.cpp:540-553`);
  - the posted date is stored at 10:59 UTC with a `date-posted` gdate slot (`Transaction.cpp:1906-1928`);
  - empty notes delete the `notes` slot (`Transaction.cpp:2042-2046`).
- Empty num, memo and action are held as absent, because GnuCash does not write them. A new transaction's
  description is `""`, as `xaccInitTransaction` sets it (`Transaction.cpp:266-267`).
- **Read-only transactions** (with a `trans-read-only` slot: voided ones, and ones posted from a business document)
  cannot be updated or deleted (`conflict`). GnuCash refuses to destroy them (`Transaction.cpp:1363-1374`), and its
  register refuses to edit them.
- **Read-only by date.** With an auto read-only threshold, a transaction posted before the threshold date cannot be
  updated, deleted, voided or unvoided (`conflict`), and `post` and `update` refuse a posted date before it
  (`invalid`), both with `data.reason` `auto-read-only`; `reverse` and `transfer` are allowed, as in GnuCash. See
  [`book-options.md`](book-options.md).
- **Placeholder accounts** cannot take new splits (`invalid`), as the register refuses them
  (`gnucash/register/ledger-core/split-register.c:2102-2110`). The root account cannot take splits either.
- **Times.** "Today" and the find dialog's day boundaries use UTC. GnuCash uses the local time zone; red-ale has none
  yet (open question).

## Result: `TransactionInfo`

Every method that returns a transaction returns it in this form.

| Field | Meaning |
|-------|---------|
| `guid`, `currency` | GUID, and the transaction currency as `CURRENCY::XXX` |
| `num`, `description`, `notes` | Text fields; `notes` is the `notes` slot |
| `datePosted` | The posted calendar date: the `date-posted` slot, else the UTC date of the posted time (`xaccTransGetDatePostedGDate`, `Transaction.cpp:2259-2289`) |
| `datePostedTime`, `dateEntered` | The stored instants |
| `void`, `voidReason`, `voidTime` | Void status by the 5.17 rule (below), the reason and when it was voided |
| `readOnly` | The read-only reason, if any |
| `reversedBy` | The transaction that reverses this one (`reversed-by` slot) |
| `closing`, `balanced` | A book-closing entry (`book_closing` slot); the values sum to zero |
| `docLink` | The document link, as stored (see "Document links") |
| `splits` | In stored order: `guid`, `account`, `accountName` (full name), `commodity`, `value`, `amount`, `memo`, `action`, `reconcileState` (`n` `c` `y` `f` `v`), `reconcileDate`, `lot`, `number` (the split's Number: its action when the book's num field source is on, else the transaction's num; see [`book-options.md`](book-options.md)) |

## Methods

| Method | Effect | Does |
|--------|--------|------|
| `transaction.post` | write | Create a transaction |
| `transaction.get` | read | One transaction |
| `transaction.list` | read | Transactions in an account and date range |
| `transaction.update` | write | Change fields, currency and splits |
| `transaction.delete` | destructive | Delete a transaction |
| `transaction.void` | write | Void, as `xaccTransVoid` |
| `transaction.unvoid` | write | Unvoid, as `xaccTransUnvoid` |
| `transaction.reverse` | write | Create the reversing transaction, as `xaccTransReverse` |
| `transaction.transfer` | write | Transfer between two accounts, at a rate, recording the price |
| `transaction.find` | read | Find splits with the Find Transaction dialog's criteria |
| `docLink.transactions` | read | Transactions with a document link (see "Document links") |

### `transaction.post`

Params:

- `splits` (required): each has `account` (GUID or full name) and `value`. It may also have:
  - `amount`: required when the account's commodity is not the transaction currency, and otherwise equal to the value;
  - `memo`, `action`, `reconcileState` (not `v`), `reconcileDate`;
  - `lot`: a lot of the same account.
- `currency`: defaults to the first split's account commodity when that is a currency.
- `datePosted` (default today), `dateEntered` (default now: GnuCash sets it at the first commit,
  `Transaction.cpp:1570-1574`).
- `num`, `description`, `notes`.
- `docLink`: a document link (see "Document links"); empty means none.
- `recordPrice` (default false): run `xaccTransRecordPrice` with source `user:split-register` afterwards, as the
  register does after committing a multi-currency transaction (see "Prices from transactions").

Result: `TransactionInfo`.

Errors:

- `unbalanced` (`data.imbalance`, `data.currency`);
- `invalid` for a placeholder or root account, a missing amount, a value that rounds to zero, a `v` state, a lot of
  another account, or a non-currency `currency`;
- `not-found` for an unknown account or lot.

### `transaction.get`, `transaction.list`

`get` takes `guid`. `list` takes an optional `account` and optional `from` and `to` dates (inclusive, on the posted
calendar date). It returns `{ transactions }` in `xaccTransOrder` (`Transaction.cpp:1793-1835`): posted time,
closing entries last, then num, date entered, description and GUID. Num compares numerically when both nums start
with a non-zero number (`order_by_int64_or_string`, `:1773-1791`).

### `transaction.update`

Params: `guid`, then any of `currency`, `datePosted`, `dateEntered`, `num` (empty clears it), `description`, `notes`
and `docLink` (empty deletes them), `removeSplits` (GUIDs), `changeSplits`, `addSplits` (as in `post`), and
`recordPrice`.

A `changeSplits` entry has `guid` and any of `account`, `value`, `amount`, `memo`, `action`, `reconcileState`
(not `v`) and `reconcileDate`. Absent fields are kept.

- When the value or account changes and the account holds the transaction currency, the amount follows the value.
  The register keeps the two equal for such splits.
- When the split moves to an account in another commodity, a new `amount` is required.
- A lot of the old account is dropped when the split moves account.

The steps run in order: currency change, removals, changes, additions, field changes, then the commit rules. The
result must balance and keep at least one split; use `delete` to remove a transaction. The change is all or nothing.

**Currency change** re-values the existing splits as `xaccTransSetCurrency` does (`Transaction.cpp:1259-1335`):

- the rate is amount/value of the first split whose account holds the new currency;
- a split in the new currency gets value = amount;
- a split in the old currency gets value = amount × rate, rounded half up to the new currency's fraction;
- any other split gets value = amount × (value/amount × rate);
- with no split in the new currency, values keep their numbers and are converted to the new fraction.

**Deviation:** for a split in a third commodity, GnuCash *divides* the split's old price by the rate
(`Transaction.cpp:1293-1299`). The oracle confirms it. A USD transaction with EUR +110/100 and GBP −110/−90 changed
to EUR becomes EUR +100 and GBP −121, and GnuCash's commit then adds an `Imbalance-EUR` split of +21. GnuCash's
result is therefore not a balanced transaction it accepts as it stands. red-ale multiplies, so the GBP split becomes
−100 and the transaction stays balanced. When GnuCash's result does balance (no third commodity, as in the oracle
test on `synthetic/rich.gnucash`), the two agree exactly. A re-valued transaction whose rounding leaves a cent of
imbalance is refused as `unbalanced`. The caller can then give explicit values in the same update.

Errors: as `post`, plus `conflict` for a read-only transaction and `not-found` for an unknown split.

### `transaction.delete`

Params: `guid`. Refused (`conflict`) for a read-only transaction. Its splits leave their lots.

### `transaction.void`

Params: `guid`, `reason` (default `""`).

As `xaccTransVoid` (`Transaction.cpp:2502-2531`; `Split.cpp:2206-2218`):

- existing notes are copied to `void-former-notes`;
- `notes` is set to `Voided transaction`;
- `void-reason` is set to the reason. An empty reason writes no slot, and the transaction is still void;
- `void-time` is set to now as `YYYY-MM-DD HH:MM:SS` in UTC with no zone (`gnc-date.cpp:1212-1234`,
  `gnc-datetime.cpp:631-636`);
- each split's amount and value are kept in `void-former-amount` and `void-former-value`, then set to zero at the
  account's SCU and the currency's fraction (`0/100`). The split is marked `v`, and its reconcile date is kept;
- `trans-read-only` is set to `Transaction Voided`.

Translated strings are written untranslated (GnuCash in the C locale).

The checks are those GnuCash's register applies before voiding (`gnucash/gnome/gnc-plugin-page-register.cpp:2453-2468`),
reported as `conflict`:

- the transaction is already void (any split is `v`), where the register silently does nothing;
- any split is cleared, reconciled or frozen;
- the transaction is read-only.

**Void status (5.17 rule):** a transaction is void when its `void-time` slot holds a timestamp that
`GncDateTime(const char*)` parses (`xaccTransGetVoidStatus`/`GetVoidTime`, `Transaction.cpp:2533-2551`;
`gnc-datetime.cpp:432-479`). The `void-reason` slot is not what decides it. Before 5.17 it was the reason that
decided, so a void with an empty reason did not count.

### `transaction.unvoid`

Params: `guid`.

As `xaccTransUnvoid` (`Transaction.cpp:2553-2573`; `Split.cpp:2220-2231`):

- `notes` is restored from `void-former-notes`, or deleted when there were none;
- the void slots and `trans-read-only` are deleted;
- each split's amount and value are restored (zero when the slot is missing, rounded to the SCU and fraction), and
  its state is reset to `n`.

A transaction that is not void is refused with `conflict`, where GnuCash silently does nothing.

### `transaction.reverse`

Params: `guid`, `date` (default today).

As `xaccTransReverse` (`Transaction.cpp:2575-2610`) followed by what the register's Reverse Transaction command does
(`gnc-plugin-page-register.cpp:2560-2585`):

- the new transaction is a clone with a new GUID and every transaction slot;
- each split keeps its account, lot, memo, action and reconcile date, and every split slot except `online_id`
  (`Split.cpp:597-636`). Its amount and value are negated and its state is set to `n`;
- `trans-read-only` is cleared on the new transaction;
- the new transaction is posted on `date` and its date entered is now;
- the original gets `reversed-by`, the new transaction's GUID.

Refused with `conflict`:

- when `reversed-by` already names an existing transaction ("A reversing entry has already been created");
- **(deviation)** when the transaction is void. GnuCash allows this, but the all-zero clone copies the void slots
  without the read-only mark, so it would itself read as void and could be edited. Unvoid first.

Result: the new transaction.

### `transaction.transfer`

Params:

- `from`: must hold a currency;
- `to`;
- `amount`: in the from currency, not zero; negative is allowed;
- `date` (default today);
- `description`, `num`, `notes`;
- `memo`: put on both splits;
- when the commodities differ, at most one of:
  - `rate`: units of the to commodity per unit of the from currency; must be positive;
  - `toAmount`: the amount received;
- `recordPrice` (default true);
- `defaultCurrency`: the user's default currency, a GnuCash preference rather than book data.

Result: `{ transaction, toAmount, rate, price }`. `price` is `{ action: created | updated | kept, guid }` when the
commodities differ and a price was considered.

This is GnuCash's Transfer dialog on OK (`gnucash/gnome-utils/dialog-transfer.cpp`).

- Refusals (`invalid`): the same account on both sides, a placeholder account, a from account that holds no currency
  ("You can't transfer from a non-currency account"), and a zero amount (`:1392-1440`, `:1685-1693`).
- **Amount received:**
  - with `rate`, it is amount × rate, rounded half up to the to account's SCU, and the price source is `user:price`
    (`gnc_xfer_update_to_amount`, `:975-1011`);
  - with `toAmount`, the rate is toAmount / amount, exact and reduced, and the source is `user:xfer-dialog` (`:1040-1056`);
  - with neither, the rate is the one the dialog fills in (source `user:price`). Between two euro currencies it is
    their fixed euro rates. Otherwise it is the price database's price on the date, else the nearest one, inverted if
    it is stored the other way round (`:306-404`). With no price at all, the transfer is refused.
- **Transaction:**
  - its currency is the from currency, posted on the date;
  - the from split has value = amount = −amount;
  - the to split has value +amount and amount equal to the amount received (`create_transaction`, `:1473-1530`,
    with `xaccSplitSetBaseValue`'s rounding, `Split.cpp:1354-1407`);
  - the commit puts the to split (the debit) first.
- **Price** (`create_price`, `:1625-1640`). It is skipped when both commodities are euro currencies.
  - If a price between the two commodities exists on the same day, in either direction (`update_price`, `:1550-1588`):
    - it is kept if its source ranks better (`PriceSource` order, `gnc-pricedb.h:165-182`), or if both values agree
      after `round_price` (`:201-217`);
    - otherwise its time, type `transaction` and unrounded value (inverted to its direction) are updated, and its
      **source is kept**.
  - Otherwise a new price is added (`new_price`, `:1591-1620`):
    - its value is |rate|, unrounded, at neutral time on the date, with source as above and type `transaction`;
    - it is stored as the non-currency priced in the currency when only one side is a currency. When the from
      currency is `defaultCurrency`, it is stored as the other currency priced in it. Otherwise it is stored as the
      from commodity priced in the to commodity.

**Deviations:**

- The dialog takes the from currency from a non-currency account's parent. Its transaction then puts a zero amount
  on the from split, so red-ale refuses such a from account.
- red-ale refuses a zero or negative rate, and a zero amount received, which the dialog would save.

### Prices from transactions (`recordPrice` on `post` and `update`)

This is `xaccTransRecordPrice` (`Transaction.cpp:2713-2807`), which GnuCash's register calls after committing
(`split-register.c:1856`). Commit itself does not call it, so red-ale runs it only when asked.

For each split with a non-zero amount in a **stock, mutual fund or currency account** (`xaccAccountIsPriced`,
`Account.cpp:4539-4548`), the price is value/amount, of the account's commodity in the transaction currency, at the
posted time.

- If a same-day price exists in either direction:
  - it is kept if equal, or if its source ranks better. The exception is `user:xfer-dialog`, which
    `user:split-register` overrides;
  - otherwise it is updated in place. The value is inverted if the price is stored the other way round, and rounded
    to the currency's fraction × 10000 (or the commodity's, when inverted). The time, the source and the type
    `transaction` are updated too.
- Otherwise a new price is added, with the value rounded the same way.

A bank account in a foreign currency records nothing.

### `transaction.find`

Params: `query`, and `limit` (keep the last *n* matches, as GnuCash keeps them, `qofquery.cpp:799-822`).

Result: `{ matches: [{ transaction, split, account }] }` in `xaccSplitOrder` (`Split.cpp:1535-1597`): transaction
order, then memo, action, reconcile flag, amount, value, reconcile date and GUID.

A query is an object with exactly one key. It is a criterion, or `all`, `any` or `not` of other queries:

| Key | Criterion | GnuCash parameter |
|-----|-----------|-------------------|
| `all: [q…]` | every query holds; `[]` matches every split | "all criteria are met" |
| `any: [q…]` | some query holds | "any criteria are met" |
| `not: q` | the query does not hold | "delete results from current search" |
| `description`, `notes`, `memo`, `num`, `action` | text | `trans.desc`, `trans.notes`, `memo`, `trans.num`, `action` |
| `descriptionNotesOrMemo` | text; any of the three matches, also for negated operators | the dialog's compound row (`dialog-find-transactions.c:139-151`) |
| `datePosted`, `dateEntered`, `dateReconciled` | date | `trans.date-posted`, `trans.date-entered`, `date-reconciled` |
| `value`, `amount`, `sharePrice` | number | `value`, `amount` ("Shares"), `share-price` |
| `reconcile: { states, not }` | the state is (not) one of the states | `reconcile-flag` |
| `closing`, `balanced`, `void` | boolean | `trans.trans-is-closing?`, `trans.trans-balanced?`, void status |
| `accounts: { accounts, matching }` | `any` (default) or `none` of the split's account; `all`: the transaction has a split in every one | `account.guid`, `trans.split-list.account-guid` |

The operators and their meanings come from `gnome-search/search-*.c` and `qofquerycore.cpp`:

- **Text:** `{ op, text, matchCase }`.
  - `op` is `contains` (default), `notContains`, `equals`, `notEquals`, `matchesRegex` or `notMatchesRegex`.
  - Matching ignores case unless `matchCase` is set, and a missing field reads as `""` (`:126-191`).
  - Empty text is refused, and a regular expression matches anywhere in the field.
- **Date:** `{ op, date }`.
  - `op` is `before` (default), `onOrBefore`, `on`, `notOn`, `after` or `onOrAfter`.
  - `on` and `notOn` compare calendar days.
  - `onOrBefore` and `after` compare with 23:59:59 on the date.
  - `before` and `onOrAfter` compare with 10:59, the day's neutral time (`search-date.c:160-170`, `:255-264`;
    `qofquerycore.cpp:352-400`).
- **Number:** `{ op, amount, sign }`.
  - `op` is `lt`, `le`, `eq` (default), `ne`, `gt` or `ge`.
  - `sign` is `any` (default), `debit` (value ≥ 0) or `credit` (value ≤ 0). Zero passes both, as the code does
    rather than its header comment.
  - The split's **magnitude** is compared with the amount as entered.
  - `eq` and `ne` treat magnitudes within 1/10000 as equal (`qofquerycore.cpp:484-541`).
  - Share price is value/amount, rounded half up, and zero when the amount is zero (`Split.cpp:1994-2026`).

Account and reconcile criteria as in `qofquerycore.cpp:616-724` and `:1157-1180`.

The oracle test runs the same queries through GnuCash's query engine and gets the same splits in the same order.

**Deviations:**

- Case-insensitive matching lowercases both sides. GnuCash casefolds and normalises to NFC (`qofutil.cpp:54-97`),
  which differs only for characters such as `ß`.
- Regular expressions use Rust's `regex` syntax, close to POSIX extended (`REG_EXTENDED`); there are no
  backreferences in either.
- Text ties in the sort compare by code point. GnuCash uses `g_utf8_collate`, which depends on the locale.
- `dateEntered`, `void` and `notContains`/`notEquals` are query parameters GnuCash registers but the dialog does not
  offer.
- Scheduled-transaction template splits are not in the book model, so they are never found. GnuCash's results
  register excludes them too (`gnc-ledger-display.c:176-200`).

## Document links

A transaction can link to a document: a file, or a web page such as a receipt in online storage. GnuCash 5.17 calls
it a *document link*.

- **Stored** in the transaction's `assoc_uri` string slot, the name from before GnuCash 4 kept for compatibility
  (`5.17:libgnucash/engine/Transaction.cpp:179`). `xaccTransSetDocLink` writes it; an empty link deletes the slot
  (`:2027-2031`, `set_kvp_string_path` at `:1845-1853`).
- **Read-only transactions refuse a link** (`conflict`), whether read-only by a `trans-read-only` slot or by date
  under the book's auto read-only threshold ([`book-options.md`](book-options.md)): the register's Manage Document
  Link and Remove Document Link both stop at `is_trans_readonly_and_warn`
  (`5.17:gnucash/gnome/gnc-split-reg.c:1118-1158`, called at `:1255-1256` and `:1325-1326`).
- **Three forms**, told apart as GnuCash does: a link with `://` has a scheme (the text before it), and one without is
  a relative path (`gnc_uri_get_components`, `5.17:libgnucash/engine/gnc-uri-utils.c:164-172`):
  - a URL, such as `https://example.com/receipt/42`. GnuCash's location box accepts only text with a scheme
    (`location_ok_cb`, `5.17:gnucash/gnome/dialog-doclink.c:112-129`);
  - an absolute file, `file:///home/me/receipts/42.pdf`;
  - a file relative to the *path head*, such as `receipts/42.pdf`. GnuCash's file chooser stores a file under the path
    head this way (`5.17:gnucash/gnome/dialog-doclink.c:421-437`).
- red-ale stores the text it is given, as GnuCash's location box does. It does not check that a file exists, and it
  does not make a path relative: give a relative path to store one.

### Path head

The path head is the folder relative links start from. It is a GnuCash user preference (`general/assoc-head`,
`5.17:gnucash/gnome-utils/dialog-doclink-utils.h:30`), not book data, so red-ale keeps it in its Configuration as
`docLinks.pathHead`: a `file://` URI ending in `/` (`doclink_get_path_head_and_set`,
`5.17:gnucash/gnome-utils/dialog-doclink-utils.c:165-203`).

- **Resolving** a link (`gnc_doclink_get_use_uri`, `:47-75`, `:102-121`): a relative link becomes `file://` followed
  by the path head's path and the link (`gnc_file_path_absolute`,
  `5.17:libgnucash/core-utils/gnc-filepath-utils.cpp:1038-1065`); any other link is used as stored.
- **Deviation:** with no path head configured, GnuCash falls back to the user's Documents folder, else its data
  folder. red-ale reads neither and leaves a relative link unresolved (`uri` is `null`).
- **Deviation:** GnuCash offers to rewrite links when the path head changes (`gnc_doclink_pref_path_head_changed`,
  `:390-444`). red-ale only changes the setting; links stay as stored.

### Methods

| Method                  | Effect | Params                     | Result |
|-------------------------|--------|----------------------------|--------|
| `docLink.transactions`  | read   | `{}`                       | `{pathHead, links}`: `transaction`, `datePosted`, `description`, `docLink`, `relative`, `uri` |
| `docLink.documents`     | read   | `{}`                       | `{pathHead, links}`: `invoice`, `id`, `type`, `dateOpened`, `docLink`, `relative`, `uri` |
| `docLink.pathHead`      | read   | `{}`                       | `{pathHead, configFile}` |
| `docLink.setPathHead`   | write  | `{pathHead}` (`null` removes it) | as `docLink.pathHead` |

- `docLink.transactions` and `docLink.documents` are GnuCash's Transaction and Business Document Links dialogs
  (`5.17:gnucash/gnome/dialog-doclink.c:788-946`): every transaction or business document whose link is not empty.
  Transactions come in `xaccTransOrder`, documents in GUID order.
- A transaction link stored as `file:` without `//`, which GnuCash before 3.5 wrote for relative paths, is read as
  the relative path after `file:` or `file:/` (`gnc_doclink_convert_trans_link_uri`,
  `5.17:gnucash/gnome-utils/dialog-doclink-utils.c:135-161`). GnuCash also rewrites the stored link when it lists it;
  red-ale's listing reads only, so `docLink` stays as stored.
- `docLink.setPathHead` takes a `file://` URI or an absolute path, which becomes a `file://` URI; a trailing `/` is
  added. Like `period.set` it changes the Configuration only, never the Book.

## Tests

- **Unit and property tests:** `crates/red-ale-core/src/ops/transactions*.rs`. A property test checks that random
  balanced splits post exactly, that void then unvoid restores the transaction, and that a reversal cancels it.
- **API tests:** `crates/red-ale-rpc/tests/transactions.rs`; document links in `crates/red-ale-rpc/tests/doc_links.rs`.
- **Oracle tests:** `crates/red-ale-testkit/tests/oracle_transactions.rs`.
  - GnuCash reads posted, updated, deleted, voided, unvoided, reversed, re-currencied and transferred transactions
    the same as red-ale.
  - On a second copy, GnuCash itself runs `Void` (also with an empty reason), `Unvoid`, `Reverse` (plus the
    register's date handling), `SetCurrency` and `RecordPrice`, and the results equal red-ale's, denominators
    included.
  - The transfer's arithmetic matches `gnc_numeric`'s, and find matches GnuCash's query engine.
  - `crates/red-ale-testkit/tests/oracle_doc_links.rs`: GnuCash reads the transaction and document links red-ale
    wrote, and the ones it removed, through `xaccTransGetDocLink` and `gncInvoiceGetDocLink`.
