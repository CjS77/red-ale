# API: accounts and the accounting period

The account tree, account settings, renumbering, delete with move, balances, and the accounting period. Domain code:
`crates/red-ale-core/src/ops/accounts.rs` and `ops/period.rs`; configuration: `crates/red-ale-service/src/config.rs`;
methods: `crates/red-ale-rpc/src/methods/accounts.rs` and `methods/period.rs`. Research:
[`notes/engine-accounts-transactions.md`](../notes/engine-accounts-transactions.md) §2–§4.

GnuCash's engine accepts almost any account edit; its rules live in the account dialog
(`5.17:gnucash/gnome-utils/dialog-account.c`) and the delete dialog (`5.17:gnucash/gnome/gnc-plugin-page-account-tree.cpp`).
red-ale applies those rules to every API call.

Tests: unit tests in `ops/accounts.rs`, `ops/period.rs` and `config.rs`; API tests in `crates/red-ale-rpc/tests/accounts.rs`;
oracle tests in `crates/red-ale-testkit/tests/oracle_accounts.rs`.

## Accounts

`AccountInfo`:

| Field                         | Meaning                                                                         |
|-------------------------------|---------------------------------------------------------------------------------|
| `guid`, `name`, `fullName`    | The full name joins ancestors' names with `:`, root excluded                    |
| `type`                        | GnuCash's name (`BANK`, `RECEIVABLE`, …)                                        |
| `parent`                      | GUID; the root for top-level accounts; null only for the root                   |
| `commodity`                   | `NAMESPACE::MNEMONIC`                                                           |
| `scu`, `nonStandardScu`       | The smallest fraction in use (`xaccAccountGetCommoditySCU`) and whether it was chosen |
| `code`, `description`, `notes` | Null when empty                                                                |
| `placeholder`, `hidden`       | The account's own flags                                                         |
| `hiddenInTree`                | Hidden itself or through an ancestor (`xaccAccountIsHidden`, `5.17:libgnucash/engine/Account.cpp:4190`) |
| `taxRelated`, `taxUs`         | `taxUs` = `{ code, payerNameSource, copyNumber }` (copy number 1 when unset, as GnuCash reads it) |
| `depth`, `children`, `hasSplits` | Levels below the root, number of immediate sub-accounts, whether it has splits |

### `account.types` (read)

Every GnuCash 5.17 type, in enum order, with `label` (GnuCash's untranslated label, `Account.cpp:4319`),
`fundamental` (`xaccAccountTypeGetFundamental`, `:4483`), `choosable` (all but the deprecated `CURRENCY` and `ROOT`,
`xaccAccountTypesValid`, `:4460`), `commodity` (`currency`, `security` for STOCK and MUTUAL, `any` for TRADING,
`dialog-account.c:224-241`), `parentTypes` (`xaccParentAccountTypesCompatibleWith`, `:4395`) and `changesWithSplits`
(`xaccAccountTypesCompatibleWith`, `:4355`). A/Receivable and A/Payable can never change type once they have splits.

### `account.list` (read)

`{ under?: AccountRef, excludeHidden?: bool }` → `{ accounts: AccountInfo[], nameViolations: Guid[] }`. Accounts come
depth first, each followed by its sub-accounts, siblings in the book's order: the order GnuCash keeps children in
(`gnc_account_append_child` appends, `Account.cpp:2824`) and writes them (pre-order,
`5.17:libgnucash/backend/xml/io-utils.cpp:59-82`). `nameViolations` lists accounts whose names contain `:`, which GnuCash
reports when it opens a book (`gnc_account_list_name_violations`, `Account.cpp:262-281`).

### `account.get` (read)

`{ account: AccountRef }` → `AccountInfo`.

### `account.tree` (read)

`{ under?: AccountRef, sorted?: bool, excludeHidden?: bool }` → a node `{ guid, name, fullName, type, code, commodity,
placeholder, hidden, children: [node] }` for `under` (the root by default). `sorted` orders siblings as GnuCash displays
them (`xaccAccountOrder`, `Account.cpp:2363`): code bytewise, then type, then name, then GUID. The type order is the one
5.17 actually computes, not the one its table intends: `typeorder` has 15 slots and 14 initialisers, so BANK ranks
after TRADING (`:2348-2400`; confirmed by the oracle).

### `account.create` (write)

| Param                  | Meaning                                                                    |
|------------------------|----------------------------------------------------------------------------|
| `name`                 | Required; not empty, no `:`, unlike every sibling's                        |
| `type`                 | Required; choosable and compatible with the parent's type                  |
| `parent`               | Default the root                                                           |
| `commodity`            | Default the parent's when it suits the type; required otherwise; must exist (book or ISO table) |
| `scu`                  | `"commodity"` (default) or a power of ten from 1 to 10⁹                    |
| `code`, `description`, `notes`, `placeholder`, `hidden`, `taxRelated`, `taxUsCode`, `taxUsPayerNameSource`, `taxUsCopyNumber` | As in `account.update` |

Returns `AccountInfo`. The checks are `gnc_common_ok` (`dialog-account.c:938-1060`): a name, no sibling with the same
name (`conflict`), a type compatible with the parent (`invalid`), a commodity. The new account becomes the parent's
last child and is placed in the book just after the parent's subtree, where GnuCash would write it. The commodity's
SCU is stored as the account's `commodity-scu`, as `xaccAccountSetCommodity` does (`Account.cpp:2666`), and GnuCash's
currency quote rule runs (`ops::commodities::account_commodity_changed`).

### `account.update` (write)

`{ account, name?, type?, retypeChildren?, commodity?, scu?, code?, description?, notes?, placeholder?, hidden?,
taxRelated?, taxUsCode?, taxUsPayerNameSource?, taxUsCopyNumber? }` → `AccountInfo`. Absent fields are kept; an empty
string clears a text field. Each field changes only when it differs, so a slot written another way (such as
`tax-related` as an integer, as in `gnucash/taxreport.gnucash`) stays as read unless it is changed.

- **Name**: as on create; the root cannot be edited.
- **Type** (`dialog-account.c:1700-1730`, `:684-726`, `:824-899`): choosable, compatible with the parent; with splits
  only a type in `changesWithSplits` (`conflict`). If the old type cannot sit under the new one, every sub-account not
  already of the new type takes it, recursively; GnuCash asks first, so red-ale needs `retypeChildren: true`
  (`conflict` otherwise). red-ale also refuses when a retyped sub-account's splits forbid the change; GnuCash does not
  check.
- **Commodity**: only without splits (`conflict`; the dialog greys it out, `dialog-account.c:1620-1625`). The SCU
  returns to the commodity's fraction, then the dialog's fraction choice is applied again, so a chosen non-standard
  fraction survives the change (`gnc_ui_to_account`, `:525-545`). The commodity must suit the type.
- **SCU**: `"commodity"` clears the non-standard flag; a power of ten sets it and the fraction
  (`xaccAccountSetCommoditySCU`, `Account.cpp:2710`). Amounts already in the account are not re-rounded, as in GnuCash.
- **Flags**: written as GnuCash writes them: the string `"true"`, or the slot removed (`set_kvp_boolean_path`,
  `Account.cpp:2558`).
- **TXF**: `tax-US/code` and `tax-US/payer-name-source` (GnuCash offers `current` and `parent`) are strings;
  `tax-US/copy-number` an integer, 0 removing it (`Account.cpp:4034-4073`). The TXF report is out of scope.

### `account.move` (write)

`{ account, parent }` → `AccountInfo`. The account and its sub-accounts go under `parent`, as its last child. Refused:
the root, a parent inside the moved subtree (`invalid`), an incompatible type (`invalid`), a sibling with the same name
(`conflict`). The subtree is placed just after the new parent's subtree in the book, so a parent always precedes its
children in the file: GnuCash's reader rejects an account whose parent comes later
(`5.17:libgnucash/backend/xml/gnc-account-xml-v2.cpp:349-365`).

### `account.renumber` (write)

`{ account, prefix?, interval?, digits? }` → `[{ account, fullName, code }]`. GnuCash's "Renumber sub-accounts"
(`dialog-account.c:2214-2375`): the immediate sub-accounts in display order get `prefix-NNN` (or `NNN` with an empty
prefix) for `interval`, `2·interval`, …; the prefix defaults to the account's own code, `interval` to 10 (1–10000),
`digits` to 3 (1–100), and never fewer digits than `n·interval` needs. Grandchildren are untouched; no uniqueness check.
An account without sub-accounts is refused (`invalid`), as GnuCash disables the action.

### `account.delete` (destructive)

| Param                      | Meaning                                                                        |
|----------------------------|--------------------------------------------------------------------------------|
| `account`                  | The account; at most one sub-account                                           |
| `transactions`             | `"delete"` or `{ "moveTo": AccountRef }`; required when it has splits          |
| `subaccount`               | Same; required when it has a sub-account (`moveTo` re-parents it)              |
| `subaccountTransactions`   | Same, for a deleted sub-account and its descendants; required when they have splits |
| `allowCommodityMismatch`   | Allow moving splits to an account in another commodity                         |

Returns `{ deletedAccounts, reparented, movedSplits, deletedTransactions, deletedLots }`. The rules are the delete
dialog's (`gnc_plugin_page_account_tree_cmd_delete_account` and `do_delete_account`,
`gnc-plugin-page-account-tree.cpp:1503-1745`):

- Refused (`conflict`) while business objects use the account: a document posted to it, an entry's income or expense
  account, an employee's credit card, a tax table (`qof_instance_get_referring_object_list`; `gncInvoice.c:230`,
  `gncEntry.c:345`, `gncEmployee.c:242`, `gncTaxTable.c:310`). red-ale also checks a sub-account it would delete.
- Refused (`conflict`) with more than one sub-account.
- Every target must have the deleted account's type (the dialog's "FIXME: same account type"), and not be the account
  or a sub-account being deleted (a re-parented sub-account may receive the splits). Split targets cannot be
  placeholders or hidden (`gnc-account-sel.c:560-582`). The re-parent target may be.
- "Delete" is refused (`conflict`) when a transaction involved is read-only (`trans-read-only`), as the dialog greys it out.
- A commodity mismatch is only a warning in GnuCash ("Do it anyway"); red-ale needs `allowCommodityMismatch`, because the
  amounts keep their numbers and silently count the new commodity.
- Order: the sub-account first (re-parent, or move or delete its and its descendants' splits), then the account's own
  splits, then the accounts. Moving re-rounds each amount to the target's SCU, keeps the value, takes the split out
  of a lot of another account and re-sorts the transaction's splits (`xaccAccountMoveAllSplits`, `Account.cpp:2184`;
  `Split.cpp:995-1003`). Deleting removes whole transactions, other accounts' splits included
  (`xaccAccountDestroyAllTransactions`, `:1588`), and a lot left without splits (`gnc_lot_remove_split`,
  `gnc-lot.cpp:631-652`). The deleted accounts' lots go with them.

Deviation: with no choice given, GnuCash's engine deletes the splits one by one and lets the imbalance scrub add
`Imbalance-<CUR>` splits; red-ale requires a choice and never leaves an imbalance. A re-parented sub-account whose
name clashes with a child of its new parent is refused (GnuCash does not check). The account delete itself is atomic.

### `account.balance` (read)

`{ account, asOf?: Date, includeSubaccounts?: bool }` →
`{ account, asOf, includesSubaccounts, balances: { commodity, balance, cleared, reconciled }, unconverted: [...] }`.
Balances are amounts in the account's commodity (`balances` is null for an account without one, such as the root).

- `xaccAccountRecomputeBalance` (`Account.cpp:2263-2335`): splits in `xaccSplitOrder` order (`Split.cpp:1536`), each
  amount at the account's SCU as GnuCash holds it once loaded; `cleared` counts every state but `n`, `reconciled` counts
  `y` and `f`; a stock-split split is skipped while the balance is zero or when it would make it zero.
- `asOf`: the running balances at the last split posted before the end of that day, 23:59:59 UTC
  (`GetBalanceAsOfDate`, `:3506`; GnuCash's cut-off is local, and posted dates at 10:59 UTC make the two agree from
  UTC−10 to UTC+13).
- `includeSubaccounts` (`xaccAccountGetXxxBalanceInCurrencyRecursive`, `:3669-3740`): each sub-account's balance is
  added rounded half up to the commodity's fraction. GnuCash converts sub-accounts in other commodities at the latest
  price; **red-ale v0.1 does not**: it adds nothing for them (GnuCash's figure when no price exists) and lists their
  totals, by commodity and unconverted, in `unconverted`. A caller can convert them with the price methods.

Text compares bytewise where GnuCash's split order collates by locale; this only affects running balances on a day
with several splits and a stock split.

## The accounting period

GnuCash keeps the accounting period as user preferences, not in the book
(`5.17:gnucash/gschemas/org.gnucash.GnuCash.window.pages.account.tree.gschema.xml.in`). red-ale keeps it in its own
configuration: a JSON file given with `--config <path>` (`Settings::config`), or, without one, in memory for the life of
the server. A book is never touched, so `period.set` works with no book open and never makes one dirty. The file is
rewritten atomically and keeps keys red-ale does not know:

```json
{ "accountingPeriod": { "start": "fiscal-year", "end": "fiscal-year", "fiscalYearEnd": "03-31" } }
```

A bound is a relative choice or a `YYYY-MM-DD` date (GnuCash's `*-choice-relative`/`*-choice-absolute`). The choices,
in GnuCash's order (`GncAccountingPeriod`, `5.17:libgnucash/engine/gnc-accounting-period.h:57-73`): `today`, `month`,
`previous-month`, `quarter`, `previous-quarter`, `year`, `previous-year`, `fiscal-year`, `previous-fiscal-year`. As a
start a choice means the period's first day; as an end, its last. The default is `year` for both, GnuCash's default.

The fiscal year end (`MM-DD`) comes from the configuration, else from the open book's `fy_end` slot (the QofBook
`fy-end` property, `5.17:libgnucash/engine/qofbook.cpp:180-220`), which 5.17 reads but never writes. GnuCash's
preferences never offer the fiscal choices because no fiscal year end reaches them; red-ale offers them when one is
known and refuses them (`invalid`) otherwise, where GnuCash would return the epoch.

### `period.get` (read)

No params → `{ start, end, fiscalYearEnd, fiscalYearEndSource: "config" | "book" | null, configFile, choices }`.

### `period.set` (write)

`{ start?, end?, fiscalYearEnd?, clearFiscalYearEnd? }` → as `period.get`. Writes the configuration; a fiscal choice
without a fiscal year end is refused.

### `period.dates` (read)

`{ date?, start?, end?, fiscalYearEnd? }` → `{ today, start, end }`: the first and last days of the period containing
`date`, by default today by the session's clock (the server's without a book), as a UTC date. The optional bounds
override the configured ones for this call. The computation is `gnc_accounting_period_start_gdate` and `_end_gdate`
(`5.17:libgnucash/engine/gnc-accounting-period.c:138-290`) with the date helpers of
`5.17:libgnucash/engine/gnc-date.cpp:1524-1716`, including their quirks: a fiscal year starts on the day after this
year's fiscal year end moved back a year, so with a year ending 28 February the start can be 28 February. The oracle
test compares every choice on edge dates with GnuCash's own functions. GnuCash turns the days into local 00:00:00
and 23:59:59; red-ale reports dates only, as time zones are an open question.

## Deviations, in short

- Names with `:` are refused; GnuCash's name entry silently drops the separator as it is typed (`dialog-account.c:1420-1447`).
- Name ties in display order compare bytewise, not by the locale's collation.
- Type changes check retyped sub-accounts' splits; deletes check sub-accounts' business references and re-parent
  name clashes; deletes need explicit choices and `allowCommodityMismatch`.
- Sub-account balances in other commodities are reported unconverted.
- The accounting period lives in red-ale's configuration; dates are UTC dates.
