# API: business posting, payments and due reminders

Totals, due dates, posting and unposting of business documents (invoices, bills, expense vouchers), payments, and
the bills-due / invoices-due reminders. The domain code is `red-ale-core::ops::business::{calc, posting, payments}`;
the methods are in `red-ale-rpc::methods::business::{posting, payments}`. Research: `design/notes/engine-business.md`
§3–§9 and `design/notes/xml-format-business.md` §9.

A *document* is GnuCash's `GncInvoice`: a customer invoice, vendor bill or employee expense voucher, or a credit note
of one. Its kind comes from its **end owner**: a job's owner, otherwise its owner.
Documents are named by GUID (`invoice`); owners by `{ "type": "customer" | "vendor" | "employee" | "job", "id": GUID }`.

## Methods

| Method            | Effect      | Does                                                                                     |
|-------------------|-------------|------------------------------------------------------------------------------------------|
| `invoice.totals`  | read        | Subtotal, tax, total, tax per account and each entry's value, discount and tax.         |
| `invoice.dueDate` | read        | When a document posted on a date falls due under its billing term (or a given term).    |
| `invoice.post`    | write       | Post a document to an A/R or A/P account.                                                |
| `invoice.unpost`  | destructive | Unpost a document: delete its posted transaction and lot links.                          |
| `payment.process` | write       | Record an owner's payment and apply it to chosen documents, or to all open ones.        |
| `invoice.pay`     | write       | Record a payment of one posted document.                                                 |
| `payment.openItems` | read      | An owner's open documents and pre-payments in an A/R or A/P account, as the payment dialog lists them. |
| `payment.fromTransaction` | destructive | Turn an existing transaction into a payment, or re-match an existing payment.   |
| `invoice.due`     | read        | Posted, unpaid documents due within some days.                                           |

### `invoice.totals`

Params: `invoice`. Result: `subtotal`, `tax`, `total` (Amounts), `taxes` (`[{account, accountName, amount}]`) and
`entries` (`[{entry, value, discount, tax}]`, in document order). Amounts are signed as printed on the document.

Matches `gncEntryComputeValue` and `gncInvoiceGetTotal`/`GetTotalSubtotal`/`GetTotalTax`/`GetTotalTaxList`
(`5.17:libgnucash/engine/gncEntry.c:1131-1445`, `5.17:libgnucash/engine/gncInvoice.c:897-1030`):

- each entry's value is computed exactly (quantity × price, tax-included back-computation, discount PRETAX,
  SAMETIME or POSTTAX, PERCENT or VALUE), with the same `gnc_numeric` operations and flags GnuCash uses, then
  rounded HALF_UP to the document currency's fraction (100000 for an entry on no document);
- an entry's tax is the running sum of its per-account taxes, rounded at each step;
- the subtotal is the sum of the rounded entry values; the tax per account is the sum of the entries' **unrounded**
  taxes for that account, rounded once; the total is subtotal plus the per-account taxes;
- tax table lines are used in GnuCash's order (account full name, then amount), and the tax list keeps GnuCash's
  order (new accounts prepended), which sets the order of tax splits at posting.

The bill side (bills and vouchers) has no discount. The oracle dump (`oracle/dump.py`) and `model::dump` carry
`subtotal`, `tax` and `total`, so every fixture comparison checks them against GnuCash.

### `invoice.dueDate`

Params: `postDate` and exactly one of `invoice` (use its billing term) or `term`. Result: `dueDate` and `due` (the
instant GnuCash stores: 10:59 UTC of the due date). Matches `gncBillTermComputeDueDate`
(`5.17:libgnucash/engine/gncBillTerm.c:751-825`): *days* terms add the due days; *proximo* terms fall due on the due
day (clamped to the month's end) of the next month, or of the month after when the posting day is past the cutoff
(a cutoff of 0 or less counts back from the end of the posting month). No term: the posting date.

### `invoice.post`

Params:

| Param              | Type        | Default                                   |
|--------------------|-------------|-------------------------------------------|
| `invoice`          | GUID        | required                                  |
| `account`          | AccountRef  | required: A/Receivable for customer documents, A/Payable otherwise, in the document currency, not a placeholder |
| `postDate`         | Date        | required                                  |
| `dueDate`          | Date        | from the billing term, else the posting date |
| `memo`             | string      | empty                                     |
| `accumulateSplits` | bool        | true (GnuCash's preference default)       |
| `autoPay`          | bool        | false (GnuCash's preference default)      |
| `prices`           | `[{commodity, price}]` | none; needed for every entry or tax account in another commodity |

Result: `transaction`, `lot` (GUIDs) and `due` (Timestamp).

Matches `gncInvoicePostToAccount` (`5.17:libgnucash/engine/gncInvoice.c:1442-1731`):

- **Num field source:** with the book's num field source on, the document ID and the type word swap places: num =
  the type word, every split's action = the document ID ([`book-options.md`](book-options.md)).
- **Transaction:** description = the end owner's name; num = the document ID; currency = the document's; posted at
  10:59 UTC of `postDate` with the `date-posted` slot; date entered = now. Slots `gncInvoice/invoice-guid`,
  `trans-date-due`, `trans-read-only` ("Generated from an invoice. Try unposting the invoice.") and
  `trans-txn-type` = `I`.
- **Splits**, every one with action `Invoice`, `Bill`, `Expense` or `Credit Note`: with `accumulateSplits`, one per
  income or expense account (memo `memo`); without, one per entry (memo = the entry's description). Then one per
  tax account (memo `memo`), then, for a voucher whose employee has a card account, one card split per CARD entry
  (memo = description, interleaved in entry order) and an "Extra to Charge Card" split for the voucher's charge
  amount; last the A/R or A/P split for the balance (memo `memo`), in the document's lot. The order before GnuCash's
  commit sort (non-negative values first) is reproduced exactly, so the file matches GnuCash's.
- **Foreign-currency lines:** a split to an account in another commodity has amount = value / price, rounded to the
  account's SCU (`5.17:libgnucash/engine/gncInvoice.c:1408-1436`). `price` is GnuCash's temporary invoice price:
  document-currency units per unit of the account's commodity. GnuCash's dialog asks for the inverse rate and stores
  `1 / rate` (`5.17:gnucash/gnome/dialog-invoice.c:1176-1258`).
- **Lot:** slots `gncInvoice/invoice-guid` and `title` = "`<type> <id>`".
- **Freezing:** the document's billing term and each entry's tax table are replaced by a frozen copy (new GUID,
  invisible, refcount 0, parent = the original, original's `child` = the copy), made on first use and reused while
  the original keeps it (`gncTaxTableReturnChild`, `gncBillTermReturnChild`). Each replacement lowers the original's
  refcount as GnuCash's setters do. A billable bill line copies its net (pre-tax) unit price to its invoice side and
  clears its invoice-side tax-included flag.
- **Due date:** `dueDate` is stored as midnight UTC of that day, as GnuCash's date field gives; without it, the
  billing term's due date (10:59 UTC), as GnuCash's post dialog computes it; with neither, midnight of the posting
  date.
- **`autoPay`:** then `gncInvoiceAutoApplyPayments` (below).

Refused (`invalid`, nothing changed): a document with no entries, no currency or an entry without an account; a
posting account of the wrong type, a placeholder or in another currency; an employee card account in another
currency; a missing price for a foreign line. A posted document gives `conflict`.

### `invoice.unpost`

Params: `invoice`, `resetTaxTables` (default false, GnuCash's dialog default). Matches `gncInvoiceUnpost`
(`5.17:libgnucash/engine/gncInvoice.c:1735-1858`): the posted transaction is deleted; the lot loses its invoice slot
(an empty `gncInvoice` frame stays, as GnuCash leaves it) and gains `gncOwner` slots for the document's owner (a job,
not its end owner). Each lot-link transaction left in the lot is deleted and the documents it linked are applied to
each other again. If payments remain, the lot stays as a pre-payment; an empty lot is deleted. The document loses its
posted date, transaction, lot and account; its frozen billing term stays. With `resetTaxTables`, each entry points
back at its tax table's parent.

### `payment.process`

Params: `owner`, `account` (A/R or A/P), `transferAccount` (needed unless the amount is zero), `amount`,
`exchangeRate`, `date`, `memo`, `num`, `applyTo` (document or lot GUIDs, in order) and `autoPay` (default false, the
preference default). Result: `transaction` (none for a zero amount) and `paymentLot` (the lot holding any
unapplied part: a pre-payment).

Matches `gncOwnerApplyPaymentSecs` (`5.17:libgnucash/engine/gncOwner.c:1404-1440`):

- **Sign:** `amount` is positive when a customer pays or a vendor or employee is paid, as in GnuCash's payment dialog
  (`5.17:gnucash/gnome/dialog-payment.c:670-690`); red-ale passes GnuCash's engine the negation for vendors and
  employees. A negative amount is a refund.
- **Transaction** (`gncOwnerCreatePaymentLotSecs`, `5.17:libgnucash/engine/gncOwner.c:749-880`): currency = the
  transfer account's; transfer split amount = amount (× `exchangeRate` when the commodities differ), value likewise;
  A/R or A/P split = the negation, in a new lot with the owner's `gncOwner` slots. Both splits have action `Payment`
  and memo `memo` (with the num field source on, the transfer split's action is `num` instead). num = `num`; slot `trans-txn-type` = `P`; posted at 10:59 UTC of `date` with **no** `date-posted`
  slot, as GnuCash.
- **Applying** (`gncOwnerAutoApplyPaymentsWithLots`, `5.17:libgnucash/engine/gncOwner.c:1256-1394`): the payment lot
  first, then `applyTo`, or, with no `applyTo` and `autoPay`, every open lot of the owner in the account in the order
  the lots joined the account. Each open lot is set against later lots of the opposite sign: a payment pays a
  document by moving its best offsetting split, reduced and split in two when larger than needed
  (`gncOwnerFindOffsettingSplit`, `gncOwnerReduceSplitTo`); two documents are linked by a lot-link transaction
  (`trans-txn-type` `L`, action `Lot Link`, memo "Offset between business items: …", reusing an existing link and
  merging splits of one transaction in one lot, `5.17:libgnucash/engine/gncOwner.c:1121-1218`); two payment lots
  offset each other. GnuCash's quirks are kept: a lot's balance is read once before its inner loop; the split choice
  is literal (`flags >= best && |amount| > |best|`).

A zero amount records nothing and only links the chosen lots.

### `invoice.pay`

Params: `invoice`, `transferAccount`, `amount`, `exchangeRate`, `date`, `memo`, `num`. `gncInvoiceApplyPayment`
(`5.17:libgnucash/engine/gncInvoice.c:1950-1979`): a payment for the document's end owner, applied to that document
only.

### Payment matching and pre-payments: scope

v0.1 already covers, as GnuCash's engine does: payments applied to chosen documents and pre-payment lots
(`payment.process {applyTo}`) or to all open ones (`autoPay`); over-payments and payments with nothing to apply to
kept as pre-payment lots; pre-payments and opposite documents applied at posting (`invoice.post {autoPay}`); lot
links between documents of opposite sign; and linking chosen lots with no new transaction (`payment.process` with
amount 0), which applies existing pre-payments and credit notes to documents.

Compared with 5.17's payment dialog (`5.17:gnucash/gnome/dialog-payment.c`) and engine
(`5.17:libgnucash/engine/gncOwner.c`), v0.2 adds:

1. **What can be matched** (`payment.openItems`): the dialog's list of an owner's open documents and pre-payments
   (`gnc_payment_window_fill_docs_list`, `5.17:gnucash/gnome/dialog-payment.c:440-603`), in the order the dialog
   hands the chosen ones to the engine. `payment.process` applies `applyTo` in the order given, as
   `gncOwnerApplyPaymentSecs` does; giving the chosen items in `payment.openItems` order reproduces the dialog.
2. **Payment from a transaction, and editing a payment's matching** (`payment.fromTransaction`): the register's
   "Assign as payment" and "Edit payment" (`5.17:gnucash/gnome/gnc-plugin-business.c:158-159`), which open the
   dialog on an existing transaction (`gnc_ui_payment_new_with_txn`, `5.17:gnucash/gnome/dialog-payment.c:1798-1854`)
   and rebuild it as a payment (`5.17:libgnucash/engine/gncOwner.c:776-800`). Applying it to other documents, or to
   none (it becomes a pre-payment), edits or removes an existing payment's matching.

Out of scope:

- **The business scrub** (`gncScrubBusinessAccount`, `5.17:libgnucash/engine/ScrubBusiness.c:467-762`): it repairs
  books damaged by older GnuCash versions (double-posted documents, superfluous lot links, dangling payments). red-ale
  never writes what it repairs: the oracle tests run 5.17's scrub on the books red-ale wrote and find nothing to
  change. It belongs with Check & Repair ([`scrub.md`](scrub.md)) and needs its own ticket.
- Printing a check, and remembering the last transfer account (the owner's `payment/last_acct` slot,
  `5.17:gnucash/gnome/dialog-payment.c:718-733`): GUI conveniences, like `last-posted-to-acct`.

### `payment.openItems`

Params: `owner` (a customer, vendor or employee) and `account` (A/R or A/P). Result: `items`, each
`{lot, invoice, id, type, date, sortDate, balance}`:

- the open lots in `account` whose end owner is `owner`, through their document or their `gncOwner` slots
  (`xaccAccountFindOpenLots` with `gncOwnerLotMatchOwnerFunc`, `5.17:libgnucash/engine/gncOwner.c:705-722`);
- `invoice` and `id` name the document, and are null for a pre-payment; `type` is the document's type word
  (`Invoice`, `Bill`, `Expense`, `Credit Note`) or `Pre-Payment`;
- `date` is the document's posted date, or the date of a pre-payment's latest split; `balance` is the lot's balance,
  signed as on the account (the dialog's debit is positive, its credit negative);
- in the order the dialog passes chosen items to the engine: sorted by `sortDate`, which is the document's due date
  or the date of a pre-payment's earliest split (`gncOwnerLotsSortFunc`, `5.17:libgnucash/engine/gncOwner.c:728-745`,
  through `g_list_insert_sorted`, `5.17:gnucash/gnome/dialog-payment.c:933-945`). Ties come in the reverse of the
  dialog's display order (date, then number, `5.17:gnucash/gnome/dialog-payment.c:1186-1206`), as inserting before
  an equal item leaves them.

### `payment.fromTransaction`

Params:

| Param           | Default                                                                                            |
|-----------------|----------------------------------------------------------------------------------------------------|
| `transaction`   | required                                                                                           |
| `owner`         | the owner of the document or pre-payment lot of its first A/R or A/P split in a business lot (`gncOwnerGetOwnerFromTxn`, `5.17:libgnucash/engine/gncOwner.c:674-697`); required when there is none |
| `account`       | the A/R or A/P account of its splits (`select_txn_lots`, `5.17:gnucash/gnome/dialog-payment.c:1708-1796`); required when it has none |
| `transferSplit` | its only split in a payment account; required when it has several                                  |
| `amount`        | the transfer split's amount (`5.17:gnucash/gnome/dialog-payment.c:1843-1850`), signed as in `payment.process`; required when the transfer and A/R or A/P commodities differ |
| `exchangeRate`  | needed when the transfer and A/R or A/P commodities differ                                         |
| `date`          | the transaction's posted date                                                                      |
| `memo`          | the transaction's description (`5.17:gnucash/gnome/dialog-payment.c:1837`)                    |
| `num`           | the transaction's number as `gnc_get_num_action` gives it: the transfer split's action when the num field source is on (`5.17:gnucash/gnome/dialog-payment.c:1836`) |
| `applyTo`       | the dialog's preselection: lots of the transaction's own A/R or A/P splits that belong to `owner` and keep a balance once this transaction's split leaves them (`5.17:gnucash/gnome/dialog-payment.c:474-497`, `:555-575`, `:403-436`) |
| `autoPay`       | false                                                                                              |

Result: `transaction` (the same GUID) and `paymentLot`, as `payment.process`.

- **What qualifies.** A transaction with at least two splits (`5.17:gnucash/gnome/gnc-plugin-business.c:677-679`),
  splits in at most one A/R or A/P account (`5.17:gnucash/gnome/dialog-payment.c:1758-1790`), and a *transfer split*:
  a split in a payment account, which is any asset, liability or equity type but A/R and A/P
  (`gncBusinessIsPaymentAcctType`, `5.17:libgnucash/engine/gncBusiness.c:92-99`). A posted document's transaction
  (type `I`) is refused, as GnuCash offers neither menu item for it
  (`5.17:gnucash/gnome/gnc-plugin-business.c:776-785`).
- **The chosen documents** are applied in the dialog's order, as `payment.openItems` gives it.
- **The transaction is rebuilt** by `gncOwnerCreatePaymentLotSecs` with it preset
  (`5.17:libgnucash/engine/gncOwner.c:776-800`, `:806-883`): every split except the first split in the transfer
  account is deleted, leaving its lot (an emptied lot is dropped); that split keeps its memo, action and reconcile
  state, unless its amount or value changes, which sets them and marks it not reconciled. The transaction takes the
  end owner's name as description, the transfer account's commodity as currency unless it is already in the A/R or
  A/P or transfer commodity, `num`, type `P`, date entered now and the day-neutral time of `date`; a new A/R or A/P
  split (memo `memo`, action `Payment`) goes into a new payment lot. Then the payment lot and the chosen lots are
  applied as in `payment.process`. The GUID, notes and other slots stay.
- **Num field source.** The transaction's num is always `num` (`5.17:libgnucash/engine/gncOwner.c:873`). A kept
  transfer split keeps its action; a new one gets `num` as action when the num field source is on, as in
  `payment.process` (`:836`).

Refused (`invalid`): a void or read-only transaction, a lot-link transaction (it has no transfer split; link lots
with `payment.process` and amount 0) and a zero amount.

### `invoice.due`

Params: `kind` (`invoices` or `bills`), `daysInAdvance` (default 7), `now` (default now). Result:
`[{invoice, id, type, company, due, dueDate, amount}]`, sorted by due date. GnuCash shows these reminders at book
open from user preferences (`bill/` and `invoice/notify-when-due`, `days-in-advance`); red-ale has no GUI, so the
preferences are params. The query is `gnc_invoice_show_docs_due` (`5.17:gnucash/gnome/dialog-invoice.c:3780-3880`):
posted documents whose lot is open, of the kind asked (`bills`: vendor and employee documents; `invoices`: customer
documents), due (`trans-date-due`) at or before `now + daysInAdvance × 86400`, overdue included. `amount` is the
lot's balance, signed as on the A/R or A/P account; `company` is the end owner's name.

## Credit notes

A credit note is a document whose `credit-note` slot is 1 (`gncInvoiceGetIsCreditNote`,
`5.17:libgnucash/engine/gncInvoice.c:1146-1155`). It is made by `invoice.create {creditNote: true}`
([`business-documents.md`](business-documents.md)), which also sets the Book feature `Credit Notes` as
`gncInvoiceSetIsCreditNote` does (`5.17:libgnucash/engine/gncInvoice.c:576-594`,
`5.17:libgnucash/engine/gnc-features.h:54`). Its entries keep their quantity negated (`gncEntrySetDocQuantity`,
`5.17:libgnucash/engine/gncEntry.c:562-570`), so the entry arithmetic gives negative values that the accessors turn
back (`gncEntryGetDoc*`, `5.17:libgnucash/engine/gncEntry.c:1535-1568`). Every method on this page takes credit notes:

- **Totals** are signed as printed on the credit note: positive for positive quantities.
- **Posting** negates the balance where the matching invoice, bill or voucher would not, and the reverse
  (`is_cust_doc != is_cn`, `5.17:libgnucash/engine/gncInvoice.c:1516-1531`): a customer credit note credits A/R and
  debits income; a vendor or employee credit note debits A/P. Entry lines use the balance value of their side,
  unaffected by the flag (`gncEntryGetBalValue`), as do an employee credit note's card lines; its extra charge is
  negated (`5.17:libgnucash/engine/gncInvoice.c:1593-1610`, `:1664-1670`). The type word in split actions and the lot
  title is `Credit Note` (`5.17:libgnucash/engine/gncInvoice.c:1089-1108`). Numbers come from the counter of the
  matching kind (`gncInvoice`, `gncBill`, `gncExpVoucher`).
- **Unposting** is the same as for any document.
- **Payments.** A credit note's lot has the opposite sign to its owner's invoices or bills, so it is linked to them
  (a lot-link transaction) by `invoice.post {autoPay}` or by `payment.process` with amount 0, or settled by a refund:
  a negative `amount`.
- **Due reminders** list customer credit notes with invoices and vendor and employee credit notes with bills.

## Deviations from GnuCash

- **Atomic and checked first.** GnuCash posts as it goes and can stop halfway (a missing price, a missing account),
  or quietly leave an entry without an account out of the splits while counting it in the total (then balancing with
  an Imbalance split). red-ale checks everything first and refuses.
- **Posting and card accounts must be in the document currency.** GnuCash's dialog offers only such posting
  accounts; for other card accounts `xaccSplitSetBaseValue` leaves the amount at zero. red-ale refuses both.
- **The document's currency is not reset to the owner's** at posting, as GnuCash's dialog does: red-ale posts in the
  document's currency and refuses a document without one.
- **No `last-posted-to-acct` slot** is written on the owner: it is a GUI convenience.
- **Overflow is an error.** GnuCash approximates results that overflow 64 bits; red-ale refuses them.
- **Lot split order** starts from book order at each operation: the order GnuCash has after loading the file. Within
  one GnuCash session a lot's list can differ (splits join it in the order they were moved), which only matters for
  ties in the choice of split to move.
- **Strings are in English** (C locale): `Invoice`, `Bill`, `Expense`, `Credit Note`, `Payment`, `Lot Link`, the
  link memo, `Extra to Charge Card` and the read-only reason. A localised GnuCash writes translations.
- Peer-split (`lot-split`) slots are not maintained when lot-link splits are merged; business lots never have them.
- **Payment from a transaction** refuses void and read-only transactions, whose splits GnuCash would delete all the
  same, and lot-link transactions, which GnuCash lets through with no transfer split and a zero amount. When the
  transfer and A/R or A/P commodities differ it needs `amount`: GnuCash's dialog proposes one converted at a price
  of its own choosing (`xaccSplitConvertAmount`), which the user confirms.
- **A `date-posted` slot** already on a transaction turned into a payment is set to the new date. GnuCash leaves it
  as it was (`xaccTransSetDatePostedSecs`, `5.17:libgnucash/engine/Transaction.cpp:1897-1903`), so the slot could
  name another day than the posted date.

## Tests

- Unit: `crates/red-ale-core/src/ops/business/calc.rs` (GnuCash's worked examples for discount and tax modes, the
  due-date table), `posting.rs` (freezing, unposting, over-payments and auto-pay); property tests for tax-included
  values and for posted transactions balancing at the document total.
- API: `crates/red-ale-testkit/tests/business_posting.rs`, on `synthetic/business-editable.gnucash`, including open
  items, payments from transactions with the dialog's defaults and the transactions refused;
  `crates/red-ale-rpc/tests/business_documents.rs` for credit notes (negated quantities, posting the other way, the
  `Credit Notes` feature, `synthetic/business.gnucash` opening for editing and saving).
- Oracle, credit notes and payment matching: `crates/red-ale-testkit/tests/oracle_business_payments.rs` with
  `oracle/business_payments.py`: credit note totals, posting, linking and unposting; payments to chosen documents,
  credit notes and pre-payments, and auto-applied; open items in the dialog's order; transactions turned into
  payments (GnuCash's dialog logic driving `gncOwnerApplyPaymentSecs` with the transaction preset), an existing
  payment re-matched, and both with the num field source on. Every test that compares books also runs GnuCash's
  `gncScrubBusinessAccountTree` on red-ale's book and checks it changes nothing.
- Oracle: `crates/red-ale-testkit/tests/oracle_business_posting.rs` with `oracle/business_posting.py`: totals against
  `GetTotal`/`GetTotalSubtotal`/`GetTotalTax` and each entry's `GetDoc*Value`; posting (accumulated and not, card
  vouchers, billable lines, job invoices, explicit due dates, a foreign line); partial, over- and auto-applied
  payments; lot links between documents of opposite sign, extended and merged; unposting with payments and with
  reset; a vendor payment from a foreign-currency bank; documents due against GnuCash's due dates and paid states.
  Each compares GnuCash's and red-ale's results on copies of one GnuCash-made book, and checks GnuCash reads
  red-ale's file as red-ale does.
