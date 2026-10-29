# API: book options

Two Book options from GnuCash's File > Properties > Accounts tab: the **num field source** and the **auto read-only
threshold**. v0.1 opened books using either read-only; v0.2 reads, sets and follows both. Domain code:
`red-ale-core::book_options`; methods: `red-ale-rpc::methods::book_options`. Research:
[`../notes/storage-and-safety.md`](../notes/storage-and-safety.md) §10-11. Citations are to GnuCash 5.17 source.

## Storage

Both live in the Book's `options/Accounts` frame (`libgnucash/engine/qofbookslots.h:55-67`), where GnuCash's book
options dialog writes them (`GncOptionDB::save_to_kvp`, `libgnucash/engine/gnc-optiondb.cpp:465-519`; registration
`:1206-1215`):

| Option (slot under `options/Accounts/`) | Wire name | Slot type | Read as |
|---|---|---|---|
| `Use Split Action Field for Number` | `splitActionForNum` | string `"t"` or `"f"` | on only when exactly `"t"` (`qof_book_use_split_action_for_num_field`, `libgnucash/engine/qofbook.cpp:933-959`) |
| `Day Threshold for Read-Only Transactions (red line)` | `autoReadOnlyDays` | double, 0–3650, step 1 | the value truncated to an integer, `(gint)` (`qof_book_get_num_days_autoreadonly`, `qofbook.cpp:986-1004`); on when not 0 (`qof_book_uses_autoreadonly`, `:974-978`) |
| `Use Trading Accounts` | `useTradingAccounts` | string `"t"` or `"f"` | on only when exactly `"t"` (`qof_book_use_trading_accounts`, `qofbook.cpp:919-928`); what it does is in [`trading-accounts.md`](trading-accounts.md) |

From v0.4, one more, under `options/Budgeting/` (`5.17:libgnucash/engine/gnc-optiondb.cpp:1223-1226`):

| Option | Wire name | Slot type | Read as |
|---|---|---|---|
| `Default Budget` | `defaultBudget` | GUID | the GUID as stored, or null when unset, even when it names no Budget; what it means is in [`budgets.md`](budgets.md), "The default budget" |

From v0.6, the Business tab's strings, the Tax tab's Tax Number and the Default Invoice Report, which the Printable
invoice prints and GnuCash's Print Invoice reads ([`../notes/report-invoice.md`](../notes/report-invoice.md) §1.3,
§1.6). Registration: `gnc-optiondb.cpp:1299-1333`, `:1352-1356`.

| Option (slot under `options/`) | Wire name | Slot type | Read as |
|---|---|---|---|
| `Business/Company Name` | `companyName` | string | the string, `""` when unset or not a string |
| `Business/Company Address` (a text option: lines separated by newlines) | `companyAddress` | string | as above |
| `Business/Company Contact Person` | `companyContactPerson` | string | as above |
| `Business/Company Phone Number` | `companyPhoneNumber` | string | as above |
| `Business/Company Fax Number` | `companyFaxNumber` | string | as above |
| `Business/Company Email Address` | `companyEmailAddress` | string | as above |
| `Business/Company Website URL` | `companyWebsiteUrl` | string | as above |
| `Business/Company ID` | `companyId` | string | as above |
| `Tax/Tax Number` | `taxNumber` | string | as above |
| `Business/Default Invoice Report` | `defaultInvoiceReport` | string `<guid>/<name>` | `{ guid, name }` as GnuCash parses it: the GUID is the 32 characters before the first `/` when that slash is the 33rd character, the name what follows, `""` when nothing does (`qof_book_get_default_invoice_report_guid`, `-name`, `qofbook.cpp:1089-1147`); null when the slot is absent or not of that form. The GUID is returned as stored and may name no report red-ale has |

The Default Invoice Report Timeout (`Business/Default Invoice Report Timeout`), the default tax tables and the Fancy
Date Format are not read or set: the timeout only times GnuCash's chooser, and the invoice reads the date format
through the query API's `book.option` ([`../reports.md`](../reports.md) §5).

The dialog writes only options the user changed. A boolean is written `"t"` or `"f"`, never deleted
(`kvp_value_from_bool_option`, `gnc-optiondb.cpp:404-410`); a number range as a double (`:493-502`), which the XML
backend writes with `%24.18g`, trimmed (`libgnucash/backend/xml/sixtp-dom-generators.cpp:189-203`), so whole days
read `30`.

## Methods

| Method            | Effect | Does |
|-------------------|--------|------|
| `book.options`    | read   | The options and today's read-only threshold. |
| `book.setOptions` | write  | Change any of them, as GnuCash's book options dialog does. |

**Result** (both methods): `{ splitActionForNum, autoReadOnlyDays, readOnlyThreshold, useTradingAccounts,
defaultBudget }`, and from v0.6 `companyName`, `companyAddress`, `companyContactPerson`, `companyPhoneNumber`,
`companyFaxNumber`, `companyEmailAddress`, `companyWebsiteUrl`, `companyId`, `taxNumber` (strings) and
`defaultInvoiceReport` (`{ guid, name }` or null).
`readOnlyThreshold` is the threshold date (below), or absent when the threshold is off.

### `book.setOptions`

Params, each optional; an absent option is left alone:

- `splitActionForNum` (bool);
- `autoReadOnlyDays` (integer, 0 to 3650, the dialog's range; anything else is `invalid`);
- `useTradingAccounts` (bool; see [`trading-accounts.md`](trading-accounts.md));
- `defaultBudget` (a Budget's GUID, written as a GUID Slot; `not-found` for an unknown Budget; see
  [`budgets.md`](budgets.md));
- the nine business strings (v0.6), each any string, `""` clearing it;
- `defaultInvoiceReport` (v0.6): `{ guid, name }`, both required, or null to clear it. The GUID must be 32 hex
  digits and is not checked against the reports red-ale has, since GnuCash may print the Book with reports red-ale
  lacks; clients pick an invoice report from `report.list`.

As the dialog applies them (`gnc_book_options_dialog_apply_helper`, `gnucash/gnome-utils/gnc-main-window.cpp:4632-4681`):

- An option given its current value writes nothing, as the dialog writes only changed options.
- `splitActionForNum` writes `"t"` or `"f"`. Turning it **on** also sets the Book feature `Number Field Source`
  (`gnc_book_option_num_field_source_change_cb`, `gnc-main-window.cpp:4709-4721`; `gnc-features.h:55`), with
  GnuCash's description (`gnc-features.cpp:39`). Turning it off keeps the feature: GnuCash never removes it.
- `autoReadOnlyDays` writes a double slot, `0` included.
- A business string writes a string slot, an empty string included, as the dialog saves a changed string option
  (`GncOptionDB::save_to_kvp`, `gnc-optiondb.cpp:465-516`). GnuCash reads an empty one as unset.
- `defaultInvoiceReport` writes `<guid>/<name>`, as `qof_book_set_default_invoice_report` (`qofbook.cpp:1043-1087`)
  and the dialog's report chooser (`gnucash/gnome/business-options-gnome.cpp:188-214`) write it; null writes the
  empty string the dialog saves for the option at its default, which GnuCash reads as no report. GnuCash's migration
  of the old `invoice-printreport` preference into the Book (`gnucash/gnome/business-gnome-utils.c:115-128`) is a GUI
  step red-ale does not copy.

No other Book feature is set. Both are Book data, so the change is saved with the book and marks it dirty; neither
is journaled (the journal records transactions only).

## Num field source

GnuCash's "Use Split Action Field for Number". When on, a split's **Number** (the register's "Num" column) is the
split's Action, and the Transaction's Num moves to the register's second line as "T-Num". GnuCash stores both
fields either way; the option changes which one means "number" (`gnc_get_num_action` and `gnc_set_num_action`,
`libgnucash/engine/engine-helpers.c:56-76`, `:116-149`).

red-ale's API keeps the stored fields under their own names in both modes: `num` is always the Transaction's Num
(GnuCash's T-Num when the option is on) and `action` the Split's Action. What follows the option:

| Where | Option off | Option on | GnuCash |
|---|---|---|---|
| **`number`**, new on each split in `TransactionInfo` and on each `LotSplitInfo` | the transaction's num | the split's action | `gnc_get_num_action(trans, split)`, as the register's Num cell (`gnucash/register/ledger-core/split-register-model.c:1037-1050`) and the lot viewer (`gnucash/gnome/dialog-lot-viewer.c:522-524`) show it |
| **Split order**: `transaction.find` results, the lot ledger behind FIFO and gains, the running balances behind `account.balance`, the account order of `reconcile.findAutoClear` | `xaccTransOrder` compares transaction nums | compares the two splits' actions (`""` when absent) | `xaccSplitOrder`, `libgnucash/engine/Split.cpp:1550-1556`; `xaccTransOrder_num_action`, `libgnucash/engine/Transaction.cpp:1794-1835` |
| **`transaction.transfer`** `num` | the transaction's num | the from split's action; the transaction gets no num | `gnc_set_num_action(trans, from_split, num, NULL)`, `gnucash/gnome-utils/dialog-transfer.cpp:1514-1516` |
| **`invoice.post`** | num = the document ID; every split's action = `Invoice`, `Bill`, `Expense` or `Credit Note` | num = that word; every split's action = the document ID | `gnc_set_num_action`, `libgnucash/engine/gncInvoice.c:1387`, `:1504`, `:1612`, `:1675`, `:1696` |
| **`payment.process`, `invoice.pay`** `num` | transaction num = `num`; transfer split action `Payment` | transaction num = `num`; transfer split action = `num` (none when `num` is empty) | `gnc_set_num_action(NULL, xfer_split, num, _("Payment"))`, `libgnucash/engine/gncOwner.c:836`; the dialog always passes the entry's text (`gnucash/gnome/dialog-payment.c`), so an empty num is `""`, not `NULL` |

Unchanged by the option, as in GnuCash:

- `transaction.post` and `transaction.update` store `num` and `action` as given. The register writes its Num cell
  into the action when the option is on (`gnucash/register/ledger-core/split-register-model-save.c:111-126`); an API client names the field.
- `transaction.find`'s `num` and `action` criteria keep matching the Transaction's Num and the Split's Action. The
  Find dialog only relabels them "Transaction Number" and "Number/Action" (`gnucash/gnome/dialog-find-transactions.c:131-138`,
  `:161-174`).
- `transaction.list` is in `xaccTransOrder`, which always compares transaction nums (`Transaction.cpp:1757-1761`).
- The A/R or A/P split of a payment always has action `Payment` (`xaccSplitSetAction`, `gncOwner.c:859`).
- Lot links, gains splits and stock splits set the action directly. `gnc_set_num_action(NULL, split, NULL, action)`
  sets the action whatever the option (`engine-helpers.c:127-131`; `gncOwner.c:1187`, `:1197`;
  `libgnucash/engine/cap-gains.cpp:405`; `gnucash/gnome/assistant-stock-split.c:391`).
- `transaction.reverse` copies num and actions unchanged (`xaccTransReverse`, `Transaction.cpp:2575-2610`).
- `invoice.duplicate` numbers the copy from its document counter, not a transaction num, and makes no transaction:
  the copy is unposted (`gnc_ui_invoice_duplicate`, `gnucash/gnome/dialog-invoice.c:3181-3238`). Posting it later
  follows the `invoice.post` row.

Reports (v0.2) read the number through the same rule.

## Auto read-only threshold

GnuCash's "Day Threshold for Read-Only Transactions (red line)". With *N* > 0 days, the **threshold date** is today
minus *N* days (`qof_book_get_autoreadonly_gdate`, `qofbook.cpp:1006-1019`), and a transaction whose posted date is
strictly before it is **read-only by date** (`xaccTransIsReadonlyByPostedDate`, `Transaction.cpp:2373-2409`).
"Today" is the same today red-ale uses for default posted dates (`gnc_g_date_new_today`), and the posted date is the
`datePosted` of `TransactionInfo` (`xaccTransGetDatePostedGDate`). Scheduled-transaction templates are exempt in
GnuCash (`xaccTransIsSXTemplate`, `Transaction.cpp:2350-2371`); in red-ale they are not transactions of the Book
([`scheduled-transactions.md`](scheduled-transactions.md)).

The engine does not enforce the threshold. GnuCash's register and dialogs do, and red-ale refuses exactly what they
refuse:

| Method | Refused when | Error | GnuCash |
|---|---|---|---|
| `transaction.post` | `datePosted` is before the threshold | `invalid` | the register's date cell resets such a date to the threshold, with "Cannot store a transaction at this date" (`gnucash/register/register-gnome/datecell-gnome.c:96-129`, `:163-174`); duplicating to such a date is refused (`gnucash/register/ledger-core/split-register.c:658-681`) |
| `transaction.update` | the transaction is read-only by date | `conflict` | the register makes every cell of its cursor read-only (`gnc_split_register_cursor_is_readonly`, `split-register-model.c:1972-2002`) |
| `transaction.update` | the new `datePosted` is before the threshold | `invalid` | the date cell, as for `post` |
| `transaction.delete` | the transaction is read-only by date | `conflict` | Delete is disabled, and refused with "The date of this transaction is older than the "Read-Only Threshold" set for this book" (`gnucash/gnome/gnc-plugin-page-register.cpp:858-873`; `gnucash/gnome/gnc-split-reg.c:1117-1141`, `:1397`) |
| `transaction.void` | the transaction is read-only by date | `conflict` | Void is disabled (`gnc-plugin-page-register.cpp:886-892`) |
| `transaction.unvoid` | the transaction is read-only by date | `conflict` | Unvoid is disabled (`:894-899`) |

Both errors carry `data`: `{ "reason": "auto-read-only", "date": <the posted or requested date>, "threshold": <the
threshold date> }`. A `conflict` from a `trans-read-only` slot has no such reason.

Allowed whatever the threshold, as in GnuCash, which checks nothing there:

- `transaction.reverse`, on any date: Reverse stays enabled (`gnc-plugin-page-register.cpp:858-899` leaves it alone),
  and its date dialog takes any date (`gnc-plugin-page-register.cpp:2520-2528`).
- `transaction.transfer`: the Transfer dialog's date field has no threshold (`gnucash/gnome-utils/dialog-transfer.cpp`).
- Reconciling, scrubs, lots, capital gains and stock splits: the reconcile window and the scrub, lot and stock split
  code change splits of old transactions without asking.
- Business posting, unposting and payments: their dialogs' date fields have no threshold.
- Importers: `import.apply` adds, clears and updates transactions on any date, as GnuCash's generic importer and CSV
  importer check no threshold (`gnucash/import-export/import-main-matcher.cpp`, `gnucash/import-export/csv-imp/`).
  `import.preview` flags each transaction that is or would be read-only by date, so the caller can see it first
  ([`import.md`](import.md)).
- `book.replayJournal`: log replay does not check it (`gnucash/import-export/log-replay/gnc-log-replay.cpp:357-540`).
- Since Last Run: GnuCash creates instances whatever their date (`5.17:libgnucash/app-utils/gnc-sx-instance-model.c:1453-1457`).
  `scheduledTransaction.sinceLastRun` flags each instance dated before the threshold, and `scheduledTransaction.apply`
  creates one only when the caller names it ([`scheduled-transactions.md`](scheduled-transactions.md)).

Document links on transactions (v0.2) follow the delete rule: GnuCash refuses to add or remove a transaction's link
when the transaction is read-only by date (`gnc-split-reg.c:1255`, `:1325`).

**Deviation:** a negative day count (which the dialog cannot enter) turns the threshold off. GnuCash reports the
option as used but has no threshold date, and its `g_assert` aborts (`Transaction.cpp:2389-2390`).

## Read-only triggers lifted

A book with either option set, or declaring `Number Field Source`, opens for editing. `Number Field Source` is a
supported Book feature.

## Tests

- **Unit tests:** `crates/red-ale-core/src/book_options.rs` (reading, setting, the threshold, `number`, the swap).
- **API tests:** `crates/red-ale-rpc/tests/book_options.rs`: get and set, the feature, every refusal and every
  allowed operation around the threshold, `number` and the order of `transaction.find` with the option on.
- **Session tests:** `crates/red-ale-service/tests/session.rs`: books with either option or the feature open for
  editing.
- **Oracle tests:** `crates/red-ale-testkit/tests/oracle_book_options.rs` and `oracle_business_posting.rs`:
  - GnuCash reads both options and the feature red-ale set, reports no unknown feature, and reads every edited
    transaction as red-ale does;
  - GnuCash's own `gnc_get_num_action` gives every split the `number` red-ale reports;
  - GnuCash's `xaccTransIsReadonlyByPostedDate` is true for exactly the transactions red-ale refuses to change, on
    both sides of the threshold;
  - with the option on, GnuCash and red-ale post, pay and transfer to the same nums and actions.
- **Business options (v0.6):** `crates/red-ale-core/src/book_options.rs` (`business_options_are_written_only_when_they_change`,
  `the_default_invoice_report_is_parsed_as_gnucash_parses_it`, `the_default_invoice_report_is_written_as_gnucash_writes_it`);
  `crates/red-ale-rpc/tests/book_options.rs` (`business_options_are_set_read_and_saved`,
  `a_default_invoice_report_needs_a_guid_and_a_name`); `crates/red-ale-testkit/tests/oracle_business_options.rs`:
  GnuCash reads every option red-ale writes, on XML and SQLite, with no unknown feature
  (`gnucash_reads_the_business_options_red_ale_writes_on_xml`, `…_on_sqlite`); red-ale writes the slots GnuCash's
  `qof_book_set_string_option` and `qof_book_set_default_invoice_report` write (`red_ale_writes_the_slots_gnucash_writes`);
  cleared options read as unset in both (`cleared_options_read_as_unset_in_gnucash`).
