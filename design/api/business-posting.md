# API: business posting, payments and due reminders

Totals, due dates, posting and unposting of business documents (invoices, bills, expense vouchers), payments, and
the bills-due / invoices-due reminders. The domain code is `red-ale-core::ops::business::{calc, posting, payments}`;
the methods are in `red-ale-rpc::methods::business::{posting, payments}`. Research: `design/notes/engine-business.md`
§3–§9 and `design/notes/xml-format-business.md` §9.

A *document* is GnuCash's `GncInvoice`: a customer invoice, vendor bill or employee expense voucher (or a credit note
of one, which v0.1 opens read-only). Its kind comes from its **end owner**: a job's owner, otherwise its owner.
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

### `invoice.due`

Params: `kind` (`invoices` or `bills`), `daysInAdvance` (default 7), `now` (default now). Result:
`[{invoice, id, type, company, due, dueDate, amount}]`, sorted by due date. GnuCash shows these reminders at book
open from user preferences (`bill/` and `invoice/notify-when-due`, `days-in-advance`); red-ale has no GUI, so the
preferences are params. The query is `gnc_invoice_show_docs_due` (`5.17:gnucash/gnome/dialog-invoice.c:3780-3880`):
posted documents whose lot is open, of the kind asked (`bills`: vendor and employee documents; `invoices`: customer
documents), due (`trans-date-due`) at or before `now + daysInAdvance × 86400`, overdue included. `amount` is the
lot's balance, signed as on the A/R or A/P account; `company` is the end owner's name.

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
- Credit notes are computed (signs follow `gncEntryGetDoc*`) but v0.1 opens books that use them read-only.

## Tests

- Unit: `crates/red-ale-core/src/ops/business/calc.rs` (GnuCash's worked examples for discount and tax modes, the
  due-date table), `posting.rs` (freezing, unposting, over-payments and auto-pay); property tests for tax-included
  values and for posted transactions balancing at the document total.
- API: `crates/red-ale-testkit/tests/business_posting.rs`, on `synthetic/business-editable.gnucash`.
- Oracle: `crates/red-ale-testkit/tests/oracle_business_posting.rs` with `oracle/business_posting.py`: totals against
  `GetTotal`/`GetTotalSubtotal`/`GetTotalTax` and each entry's `GetDoc*Value`; posting (accumulated and not, card
  vouchers, billable lines, job invoices, explicit due dates, a foreign line); partial, over- and auto-applied
  payments; lot links between documents of opposite sign, extended and merged; unposting with payments and with
  reset; a vendor payment from a foreign-currency bank; documents due against GnuCash's due dates and paid states.
  Each compares GnuCash's and red-ale's results on copies of one GnuCash-made book, and checks GnuCash reads
  red-ale's file as red-ale does.
