# API: invoices, bills, expense vouchers, entries and orders

GnuCash keeps customer invoices, vendor bills and employee expense vouchers as one entity, `GncInvoice`, here called
a **document**. Its lines are **entries** (`GncEntry`), and orders (`GncOrder`) group entries
too. This page covers creating and editing them while unposted. Totals, due dates, posting, unposting,
payments and credit notes are in [`business-posting.md`](business-posting.md); contacts, tax tables and billing terms
are in [`business-contacts.md`](business-contacts.md), whose conventions (partial updates, `null` clears, owners as
`OwnerInfo`, tax table and billing term references) apply here too.

Code: `red-ale-core/src/ops/business/documents.rs`, `red-ale-rpc/src/methods/business/documents.rs`. Tests:
`crates/red-ale-rpc/tests/business_documents.rs` (API), `crates/red-ale-testkit/tests/oracle_business_documents.rs`
(GnuCash 5.17 reads the documents back, numbers the next ones as red-ale does, and computes the expected totals from
red-ale's entries).

## Document kind

The kind is never stored: it follows from the owner, through a job to the job's owner (`gncInvoiceGetType`,
`5.17:libgnucash/engine/gncInvoice.c:1065-1087`).

| End owner | Kind (`type` on the wire) | Counter          | Entry side |
|-----------|---------------------------|------------------|-----------|
| customer  | `invoice`                 | `gncInvoice`     | invoice (`i-` fields) |
| vendor    | `bill`                    | `gncBill`        | bill (`b-` fields)    |
| employee  | `expenseVoucher`          | `gncExpVoucher`  | bill (`b-` fields)    |

The counters are GnuCash's (`gncInvoiceNextID`, `5.17:libgnucash/engine/gncInvoice.c:2256-2275`) and work as the
contact counters do: a number taken is never given back, even when its document is deleted. `creditNote` exists on
the wire for documents GnuCash made; red-ale v0.1 does not create or edit credit notes.

## Dates

`dateOpened` and an entry's `date` are calendar dates (`YYYY-MM-DD`) and default to today. They are stored as **noon
in the server's local time zone**, as GnuCash's dialogs and register store them (`time64CanonicalDayTime`,
`5.17:libgnucash/engine/gnc-date.cpp:404-410`), so GnuCash shows the same day. Results give them as UTC timestamps.
An entry's `dateEntered` is the time it was created.

## Documents

| Method           | Effect      | Params                                   | Result           |
|------------------|-------------|------------------------------------------|------------------|
| `invoice.create` | write       | `InvoiceFields` (`owner` required)       | `InvoiceDetail`  |
| `invoice.get`    | read        | `{invoice}`                              | `InvoiceDetail`  |
| `invoice.list`   | read        | `{type?, owner?, posted?, activeOnly?, id?}` | `{invoices}`, in GUID order, without entries |
| `invoice.update` | write       | `{invoice, …InvoiceFields}`              | `InvoiceDetail`  |
| `invoice.delete` | destructive | `{invoice}`                              | `Done`           |

`InvoiceFields`: `owner`, `id`, `dateOpened`, `terms`, `billingId` (the customer's reference or the vendor's
invoice number), `notes`, `billTo` (bills and vouchers: the customer or job the costs are for), `chargeAmount`
(vouchers: an extra amount to charge to the employee's card when posted), `active`.

`InvoiceDetail`: `guid`, `id`, `type`, `owner`, `currency`, `dateOpened`, `posted`, `datePosted`, `terms`,
`billingId`, `notes`, `active`, `billTo`, `chargeAmount`, `postedAccount`, `postedTransaction`, `postedLot`,
`entryCount`, and `entries`, sorted as GnuCash sorts them (by date, date entered, description, action, then GUID;
`gncEntryCompare`, `5.17:libgnucash/engine/gncEntry.c:1647-1665`).

- **Create** starts from `gncInvoiceCreate`'s defaults (`5.17:libgnucash/engine/gncInvoice.c:313-334`), then takes,
  as GnuCash's invoice dialog does, the owner's currency (`5.17:gnucash/gnome/dialog-invoice.c:414`), the owner's
  billing term unless `terms` is given, and today's date. It writes the `credit-note` slot as 0, as every document
  the dialog saves has it (`5.17:gnucash/gnome/dialog-invoice.c:433`, `:2981`). The billing term's refcount goes up.
- **Checks.** The owner must be a customer, vendor, employee or job of one of them. `billTo` must be a customer or
  job, and only on bills and vouchers. `chargeAmount` only on vouchers. The document's currency is always its
  owner's.
- **Update.** An unposted document may change anything but its kind: a new owner must give the same kind (a
  customer's job for a customer, and so on), and the currency follows it. A posted document may only be
  (de)activated; anything else is a `conflict`. Unpost it first ([`business-posting.md`](business-posting.md)).
- **`invoice.list {owner}`** matches the document's owner or its end owner, so a customer also finds its jobs'
  documents.
- **Delete** (red-ale only, see Deviations) removes an unposted document as `gncInvoiceRemoveEntries` then
  `gncInvoiceDestroy` do (`5.17:libgnucash/engine/gncInvoice.c:409-440`): entries that are also on an order stay there;
  the others are destroyed and release their tax tables (`gncEntryFree`, `5.17:libgnucash/engine/gncEntry.c:466-474`);
  the document releases its billing term. It is refused while a lot still names the document.

## Entries

| Method                | Effect      | Params                                          | Result      |
|-----------------------|-------------|-------------------------------------------------|-------------|
| `invoice.addEntry`    | write       | `{invoice, order?, …LineFields}`                | `EntryInfo` |
| `invoice.updateEntry` | write       | `{entry, …LineFields}`                          | `EntryInfo` |
| `invoice.removeEntry` | destructive | `{entry}`                                       | `Done`      |

`LineFields` (all optional): `date`, `description`, `action` (a unit label such as "Hours"), `notes`, `quantity`,
`account`, `price`, `discount`, `discountType` (`percent`, `value`), `discountHow` (`preTax`, `sameTime`,
`postTax`), `taxable`, `taxIncluded`, `taxTable`, `billable`, `billTo`, `payment` (`cash`, `card`).

A `GncEntry` has two sides: the invoice side (`i-` account, price, discount, tax fields) for customer documents, and
the bill side (`b-` account, price, tax fields, billable, bill-to, payment) for bills and vouchers. The params set
the side of the entry's document; `EntryInfo` shows both (`invoiceAccount`, `invoicePrice`, `discount*`,
`invoiceTax*`, `billAccount`, `billPrice`, `billTax*`, `billable`, `billTo`, `payment`) plus `invoice`, `bill` and
`order`.

- **Fields by kind.** A field GnuCash's entry ledger has no column for on that kind of document is refused
  (`5.17:gnucash/register/ledger-core/gncEntryLedgerLayout.c:218-282`): invoices have no `billable`, `billTo` or
  `payment`; bills have no discounts or `payment`; vouchers have no discounts or tax settings.
- **Defaults for a new entry** are `gncEntryCreate`'s (`5.17:libgnucash/engine/gncEntry.c:413-444`), then the entry
  ledger's defaults from the owner (`5.17:gnucash/register/ledger-core/gncEntryLedgerLoad.c:375-466`): the job's rate
  as the price on a job's document; the customer's discount; tax included from the owner's setting, where
  `useGlobal` gives yes (see Deviations); the owner's tax table when it overrides, else the book's default customer or
  vendor tax table; and taxable when a table applies. Vouchers get the engine defaults only. Params then override the
  defaults.
- **Accounts.** An entry cannot use an A/R, A/P, bank, cash, equity, trading or placeholder account, nor an expense
  account on an invoice or an income account on a bill or voucher, as the entry ledger offers none of them
  (`5.17:gnucash/register/ledger-core/gncEntryLedgerLoad.c:140-181`).
- **Tax tables.** A new table reference must name a visible table; the table's refcount follows the entry, side by
  side. `taxTable: null` with `taxable: false` makes an untaxed entry.
- **Links.** An entry stays on the documents and order it was added to; `updateEntry` cannot move it. Its documents
  must be unposted and its order open for any change. `removeEntry` takes the entry off its documents and order and
  destroys it, as the ledger's delete does (`5.17:gnucash/register/ledger-core/gncEntryLedger.c:837-880`).
- **Extra data feature.** An entry carrying slots sets the book feature `Extra data in addresses, jobs or invoice
  entries`, as GnuCash does (`5.17:libgnucash/engine/gncEntry.c:1636-1640`).

## Expense vouchers

An expense voucher is a document whose owner is an employee. Its entries use the bill side and may be `payment: card`,
meaning the company card paid: posting charges those to the employee's `creditCardAccount` (see
[`business-contacts.md`](business-contacts.md)). `chargeAmount` is an extra amount charged to that card when the voucher is
posted. Entries may be `billable` to a customer or job (`billTo`). Totals and posting are in
[`business-posting.md`](business-posting.md).

## Orders

| Method              | Effect      | Params                                         | Result        |
|---------------------|-------------|------------------------------------------------|---------------|
| `order.create`      | write       | `{owner, id?, dateOpened?, notes?, reference?, active?}` | `OrderDetail` |
| `order.get`         | read        | `{order}`                                      | `OrderDetail` |
| `order.list`        | read        | `{owner?, closed?, activeOnly?}`               | `{orders}`, in GUID order |
| `order.update`      | write       | `{order, …}` (any create field)                | `OrderDetail` |
| `order.close`       | write       | `{order, date?}` (a timestamp; defaults to now) | `OrderDetail` |
| `order.addEntry`    | write       | `{order, …LineFields}`                         | `EntryInfo`   |
| `order.updateEntry` | write       | as `invoice.updateEntry`                       | `EntryInfo`   |
| `order.removeEntry` | destructive | as `invoice.removeEntry`                       | `Done`        |

`OrderDetail`: `guid`, `id`, `owner`, `dateOpened`, `closed`, `dateClosed`, `notes`, `reference`, `active`,
`entryCount`, `entries`.

- The owner is a customer, vendor or job. New orders are open and active (`gncOrderCreate`,
  `5.17:libgnucash/engine/gncOrder.c:281-299`) and numbered from the `gncOrder` counter
  (`5.17:libgnucash/engine/gncOrder.c:589-592`).
- **Order entries use the invoice side, whoever owns the order**, because GnuCash's order ledger is a customer-document
  ledger (`5.17:gnucash/register/ledger-core/gncEntryLedger.c:286-294`). They get the engine's defaults only, as the
  order ledger has no document to take owner defaults from
  (`5.17:gnucash/register/ledger-core/gncEntryLedgerLoad.c:354-366`). An entry can join an order from a customer invoice
  (`invoice.addEntry {order}`), not from a bill or voucher.
- **Close** needs at least one entry, as GnuCash's order dialog does (`5.17:gnucash/gnome/dialog-order.c:255-316`). A
  closed order is frozen: no entry may be added, changed or removed, and it cannot be closed again. Closing is the only
  way to set `dateClosed`.
- The owner can change only while the order has no entries.
- Deleting a document leaves its entries on their order.

**Orders are not checked by the oracle.** GnuCash's Python bindings do not wrap `GncOrder`; the oracle checks only
that GnuCash reads every order back (`orderCount`). Order fields are covered by the API tests and by the XML round
trip of `book.save` and reopen.

## Deviations

| Deviation | Reason |
|-----------|--------|
| `invoice.delete` deletes any unposted document. GnuCash's GUI only discards a new one. | Agents need to undo a draft. It is limited to unposted documents and keeps GnuCash's entry and refcount rules. |
| Dates are stored as noon in the server's local time. | That is GnuCash's own convention; a server in another time zone than the user's GnuCash may show a date one day off only for zones more than 12 hours apart. |
| New entry defaults follow GnuCash's entry ledger (the GUI), not only `gncEntryCreate`. A `useGlobal` tax-included setting gives yes. | The GUI is how GnuCash users make entries; the global preference is a GUI setting red-ale cannot read, so its shipped default (yes) is used. |
| The book's default tax table is read from both option spellings: `Default Customer Tax Table` (what the options dialog writes) first, then `Default Customer TaxTable` (what `gncTaxTableGetDefault` reads, `5.17:libgnucash/engine/gncTaxTable.c:680-699`); likewise for vendors. | GnuCash 5.17 writes one and reads the other (`design/notes/xml-format-business.md` §10.2). |
| Entry fields with no ledger column on a kind of document, and accounts the ledger does not offer, are refused. The engine would store them. | GnuCash would ignore or mis-post them, and users could not see or fix them in GnuCash. |
| Credit notes cannot be created or edited. | Out of v0.1's scope; documents GnuCash made as credit notes are kept unchanged. |
| A new bill or voucher entry cannot go on an order. The engine allows it. | Order entries use the invoice side in GnuCash's order ledger; a bill-side entry on an order would show empty there. |
| Order behaviour is not oracle-tested. | GnuCash's Python bindings do not wrap orders. |
