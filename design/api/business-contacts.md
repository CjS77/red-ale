# API: business contacts, tax tables and billing terms

Customers, vendors, employees and jobs (GnuCash's *owners*), the book counters that number them, and the tax
tables and billing terms they and their documents refer to. Documents (invoices, bills, expense vouchers, their
entries and orders) are in [`business-documents.md`](business-documents.md). Posting, totals, due dates and payments
are in [`business-posting.md`](business-posting.md).

Code: `red-ale-core/src/ops/business/{contacts,terms}.rs`, `red-ale-rpc/src/methods/business/{contacts,terms}.rs`.
Tests: `crates/red-ale-rpc/tests/business_contacts.rs` (API), `crates/red-ale-testkit/tests/oracle_business_contacts.rs`
(GnuCash 5.17 reads the result back, numbers and taxes the same way).

## Conventions

- **Partial updates.** In `*.update`, an absent field is left as it is; `null` clears a field that can be empty
  (`terms`, `taxTable`, `creditCardAccount`, …); a value sets it. `addr` and `shipAddr` merge field by field.
- **Deactivate, never delete.** Contacts have no delete method. `active: false` deactivates. GnuCash has no way to
  delete a saved contact either: its dialogs destroy a contact only when a new one is cancelled
  (`5.17:gnucash/gnome/dialog-customer.c:376-388`). Documents keep pointing at their owner, so deleting one would
  break them.
- **IDs come from the book counters.** On create, an absent or empty `id` takes the next number from the counter in
  the book's `counters` frame (`gncCustomer`, `gncVendor`, `gncEmployee`, `gncJob`), formatted with
  `counter_formats/<name>` or GnuCash's default `%.6` PRIi64 (`000001`) when that format is missing or invalid
  (`qof_book_increment_and_format_counter`, `qof_book_get_counter_format`,
  `5.17:libgnucash/engine/qofbook.cpp:592-750`; counter names `5.17:libgnucash/engine/gncCustomer.c:960-963`,
  `gncVendor.c:1020-1023`, `gncEmployee.c:948-951`, `gncJob.c:600-603`). As in GnuCash, the counter goes up whenever
  a number is taken, and IDs need not be unique. A saved contact keeps a non-empty ID, because GnuCash does not save
  one without (`5.17:libgnucash/backend/xml/gnc-customer-xml-v2.cpp:421-432`).
- **Currency.** Customers, vendors and employees need a currency: a commodity in the `CURRENCY` namespace, ISO or
  defined in the book. A job has none of its own; it uses its owner's (`gncOwnerGetCurrency`,
  `5.17:libgnucash/engine/gncOwner.c:478-500`).
- **Tax table and billing term references** are a GUID (which also finds a frozen copy) or the name of a visible one
  (`gncTaxTableLookupByName`, `5.17:libgnucash/engine/gncTaxTable.c:667-678`; `gncBillTermLookupByName`,
  `5.17:libgnucash/engine/gncBillTerm.c:494-505`). A new reference must be to a visible one; a reference that does not
  change is not checked, so a contact that already points at a frozen copy can still be edited.
- **Owners on the wire** are `OwnerInfo {kind: customer|vendor|employee|job, guid}`. Params that take an owner take
  its GUID; the kind is looked up.
- **Extra data feature.** When an address or job carries slots, the book gets the feature `Extra data in addresses,
  jobs or invoice entries`, as GnuCash sets it (`5.17:libgnucash/engine/gncAddress.c:493-497`, `gncJob.c:428-431`;
  `gnc_features_set_used`, `5.17:libgnucash/engine/gnc-features.cpp:107-121`).

Errors are the generic ones of [`../api.md`](../api.md): `not-found` for a missing contact, account, table or term;
`invalid` for a failed check; `conflict` for a name already taken, a frozen copy, a table or term still in use, or a
job owner change once it has documents.

## Addresses

`AddressParams` and `AddressInfo`: `name`, `addr1` to `addr4`, `phone`, `fax`, `email`, all strings.

## Customers

| Method            | Effect | Params              | Result         |
|-------------------|--------|---------------------|----------------|
| `customer.create` | write  | `CustomerFields`    | `CustomerInfo` |
| `customer.get`    | read   | `{customer}`        | `CustomerInfo` |
| `customer.list`   | read   | `ListParams`        | `{customers}`, in GUID order |
| `customer.update` | write  | `{customer, …CustomerFields}` | `CustomerInfo` |

`CustomerFields`: `name` (company name; required on create), `id`, `currency` (required on create), `addr`,
`shipAddr`, `notes`, `terms` (billing term), `taxTable`, `taxTableOverride` (new entries use `taxTable` instead of the
book default), `taxIncluded` (`yes`, `no`, `useGlobal`), `discount` (default entry discount, a percentage),
`credit` (credit limit), `active`.

Checks, as GnuCash's customer dialog: a company name, a discount from 0 to 100, a credit limit that is not negative
(`5.17:gnucash/gnome/dialog-customer.c:313-340`). New customers start from `gncCustomerCreate`'s defaults: active,
`taxIncluded: useGlobal`, zero discount and credit (`5.17:libgnucash/engine/gncCustomer.c:303-331`).

`ListParams` (all optional): `activeOnly`, `name` (case-insensitive part of the name), `id` (exact).

## Vendors

`vendor.create`, `vendor.get {vendor}`, `vendor.list` (`ListParams`; result `{vendors}`), `vendor.update`. Fields:
`name` (required), `id`, `currency` (required), `addr` (payment address), `notes`, `terms`, `taxTable`,
`taxTableOverride`, `taxIncluded`, `active`. A vendor needs a company name
(`5.17:gnucash/gnome/dialog-vendor.c:204-208`); defaults from `gncVendorCreate`
(`5.17:libgnucash/engine/gncVendor.c:453-476`).

## Employees

`employee.create`, `employee.get {employee}`, `employee.list` (`ListParams`; result `{employees}`),
`employee.update`. Fields: `name` (required), `id`, `username`, `currency` (required), `addr`, `language`, `acl`
(stored only), `workday` (hours per day), `rate` (hourly rate), `creditCardAccount` (`AccountRef`), `active`.

- An employee's name is its address name, as in GnuCash (`gncOwnerGetName`, `5.17:libgnucash/engine/gncOwner.c:439-457`;
  `5.17:libgnucash/engine/gncEmployee.c:512-520`). `name` overrides `addr.name` when both are given.
- `creditCardAccount` must be a `CREDIT` account: the dialog offers only those
  (`5.17:gnucash/gnome/dialog-employee.c:458`). Expense voucher entries paid by card are charged to it when posted.
- Defaults from `gncEmployeeCreate` (`5.17:libgnucash/engine/gncEmployee.c:428-453`).

## Jobs

`job.create`, `job.get {job}`, `job.list` (`JobListParams`: `ListParams` plus `owner`; result `{jobs}`),
`job.update`. Fields: `name` (required), `id`, `owner` (a customer or vendor; required on create), `reference`
(GnuCash's job description), `rate`, `active`. `JobInfo` adds the owner as `OwnerInfo`.

- A job needs a name and an owner that is a customer or vendor (`5.17:gnucash/gnome/dialog-job.c:134-150`,
  `5.17:libgnucash/engine/gncJob.c:336-352`).
- `rate` is kept in the job's `job-rate` slot, as GnuCash does; zero removes the slot (`gncJobSetRate`,
  `5.17:libgnucash/engine/gncJob.c:314-335`, `:459-466`). New entries on the job's documents take it as their price.
- `job.list {owner}` lists that customer's or vendor's jobs.

## Tax tables

| Method            | Effect      | Params                               | Result          |
|-------------------|-------------|--------------------------------------|-----------------|
| `taxTable.create` | write       | `{name, entries}`                    | `TaxTableInfo`  |
| `taxTable.get`    | read        | `{taxTable}`                         | `TaxTableInfo`  |
| `taxTable.list`   | read        | `{includeFrozen?}`                   | `{taxTables}`, by name then GUID |
| `taxTable.update` | write       | `{taxTable, name?, entries?}`        | `TaxTableInfo`  |
| `taxTable.delete` | destructive | `{taxTable}`                         | `Done`          |

A tax table line is `{account, amount, type: percent|value}`; the wire calls the list `entries`, but a tax table line
is not an Entry of a document. `TaxTableInfo`: `guid`, `name`, `refcount`, `invisible`, `parent`, `child`,
`entries` (each with `accountName`).

- **Checks** (the tax table dialog's, `5.17:gnucash/gnome-utils/dialog-tax-table.c:153-198`): a name no other visible
  table has, each line with an account, and a percentage from -100 to 100. A table needs at least one line: GnuCash
  deletes empty tables when it loads a book (`5.17:libgnucash/backend/xml/gnc-tax-table-xml-v2.cpp:517-524`).
- **Line order** is GnuCash's: by account full name, then amount (`gncTaxTableAddEntry`,
  `5.17:libgnucash/engine/gncTaxTable.c:594-608`). That order is written to the file.
- **Frozen copies.** Posting a document freezes the tables its entries use into invisible children (`parent` set), so
  later edits do not change posted documents. Frozen copies cannot be changed or deleted here and are listed only with
  `includeFrozen`. Replacing the lines drops the cached child, as the dialog's `gncTaxTableChanged` does; renaming
  alone keeps it (`5.17:gnucash/gnome-utils/dialog-tax-table.c:240`, `:690-709`).
- **Refcounts** are kept as GnuCash keeps them: customers, vendors and each side of each entry (invoice side and bill
  side separately) count; increments and decrements on frozen copies are no-ops, and a count never goes below zero
  (`5.17:libgnucash/engine/gncTaxTable.c:516-535`). On load GnuCash recounts the references
  (`5.17:libgnucash/backend/xml/gnc-tax-table-xml-v2.cpp:530-633`); red-ale's counts agree, which the oracle tests
  check.
- **Delete** only a visible table nothing uses (the dialog refuses one with a refcount,
  `5.17:gnucash/gnome-utils/dialog-tax-table.c:719-744`). Its frozen copies stay, without a parent, as `gncTaxTableFree`
  leaves them (`5.17:libgnucash/engine/gncTaxTable.c:452-457`).

## Billing terms

| Method            | Effect      | Params                                         | Result          |
|-------------------|-------------|------------------------------------------------|-----------------|
| `billTerm.create` | write       | `{name, description?, type, dueDays?, discountDays?, discount?, cutoff?}` | `BillTermInfo` |
| `billTerm.get`    | read        | `{billTerm}`                                   | `BillTermInfo`  |
| `billTerm.list`   | read        | `{includeFrozen?}`                             | `{billTerms}`, by name, description, GUID |
| `billTerm.update` | write       | `{billTerm, …}` (any create field)             | `BillTermInfo`  |
| `billTerm.delete` | destructive | `{billTerm}`                                   | `Done`          |

`type` is `days` (due `dueDays` after posting, discount for `discountDays`) or `proximo` (due on day `dueDays` of
the next month, discount until day `discountDays`; documents posted after day `cutoff` move a month later, and a
cutoff of 0 or less counts back from the end of the month). `discount` is a percentage. `BillTermInfo` adds
`refcount`, `invisible`, `parent`, `child`; `type` is absent for a term whose type was never set.

- **Checks.** The dialog's: a name no other visible term has, and discount days no more than due days
  (`5.17:gnucash/gnome/dialog-billterms.c:268-334`). red-ale also refuses (see Deviations) negative day counts, a
  proximo due day outside 1-31, a proximo discount day outside 0-31, a cutoff outside -31 to 31 and a discount
  outside 0-100.
- **Update** merges: changing only `dueDays` keeps the type and the other fields. Any change drops the cached frozen
  copy, as the dialog's `gncBillTermChanged` does (`5.17:gnucash/gnome/dialog-billterms.c:359-360`).
- **Refcounts** count customers, vendors and documents (`5.17:libgnucash/engine/gncBillTerm.c:420-439`), with the
  same no-op and floor rules as tax tables, and agree with GnuCash's recount on load
  (`5.17:libgnucash/backend/xml/gnc-bill-term-xml-v2.cpp:589-686`).
- **Delete** only a visible term nothing uses (`5.17:gnucash/gnome/dialog-billterms.c:663-690`); frozen copies stay,
  without a parent (`5.17:libgnucash/engine/gncBillTerm.c:288-297`).
- **Due dates are not computed here.** They belong to posting ([`business-posting.md`](business-posting.md)).

## Deviations

| Deviation | Reason |
|-----------|--------|
| No delete method for customers, vendors, employees or jobs. | GnuCash has none for saved contacts; deleting one would orphan its documents. |
| `job.update` refuses a new owner once the job has invoices, bills or orders. GnuCash allows it. | It would silently move those documents, posted ones included, to another customer or vendor. |
| Billing terms: negative days, a proximo due day outside 1-31, a discount day outside 0-31, a cutoff outside -31 to 31 and a discount outside 0-100 are refused. The dialog accepts some of them. | GnuCash computes an undefined due date from them (`design/notes/engine-business.md` §3.2). |
| Tax tables need at least one line. The dialog can leave a table empty. | GnuCash deletes empty tables when it next loads the book, which would orphan references to them. |
| Employee `creditCardAccount` must be a `CREDIT` account. The engine accepts any account. | The GUI offers only credit card accounts; posting charges card payments to it. |
| A counter format `printf` would refuse (two dots in the width and precision) falls back to the default format; the `'` and `I` flags are accepted and ignored. | GnuCash would pass such a format to `printf` with undefined results; `'` and `I` only localise digits, and red-ale writes ASCII digits. |
