# Report note: the Printable Invoice and the Dashboard (GnuCash 5.17)

Scope: the two report rows of [`../milestone6.md`](../milestone6.md). For the Printable Invoice: its template and
options, every Book read its renderer makes, the layout it produces, its rounding, GnuCash's test and the fixture
that ports it, and how GnuCash runs it for one invoice. For the Dashboard: what GnuCash builds and how red-ale's
Multicolumn View and a built-in saved configuration express it. Then what red-ale has to build, in order.

Citations are `5.17:<path>:<line>`; a bare `:<line>` refers to the file last named. `standard/` stands for
`5.17:gnucash/report/reports/standard/`, `report/` for `5.17:gnucash/report/`, INV for `standard/invoice.scm`, TI for
`standard/test/test-invoice.scm` (262 lines, no licence notice) and TEE for
`5.17:bindings/guile/test/test-engine-extras.scm`. HU is `report/html-utilities.scm`, RU
`report/report-utilities.scm`, BC `5.17:bindings/guile/business-core.scm`.

**Method.** Everything here was read from the 5.17 source. No Scheme was run. The invoices of
`fixtures/reports/transaction.gnucash` were read back through GnuCash 5.17's Python bindings (a copy, read-only):
their totals, entry values and lot splits are quoted below as *oracle*. Figures quoted as GnuCash's are the expected
strings of TI.

---

## Facts that surprise

1. **The Easy and Fancy invoices are the Printable Invoice with other defaults.** All three templates share one
   options generator and one renderer (`reg-renderer`, INV:882-883, :892-893, :902-903); `variant-list` (:340-372)
   gives each its defaults for the six Layout rows and the CSS, and nothing else differs. Fancy differs from Printable
   in Row 3 Right (`none`) and two CSS rules; Easy in six CSS rules. Each is therefore a saved configuration of the
   Printable Invoice (§1.1, §6 Q7).
2. **GnuCash never fails to print an invoice for want of a report.** When the Book's Default Invoice Report names
   anything that is not an invoice report, Print Invoice uses the Printable Invoice
   (`5.17:gnucash/gnome/dialog-invoice.c:957-963`). [`../milestone6.md`](../milestone6.md), "Decided", item 6, says a
   default naming an untranslated template "runs as `report.run` does", which is `not-found` (§1.6).
3. **red-ale cannot read or set the Business book options.** `book.options` has five fields, none of them business
   (`crates/red-ale-rpc/src/methods/book_options.rs:37-50`); core reads only Company Name
   (`crates/red-ale-core/src/book_options.rs:21`, `:102`). The milestone's "red-ale already edits them through its
   book options methods" (`../milestone6.md`, "Compatibility in v0.6") is not so, and [`../reports.md`](../reports.md)
   §9's "red-ale reads [the Default Invoice Report]" is not implemented: no code names that slot. A user who keeps
   books only with red-ale cannot put a company name or address on an invoice (§1.6, §6 Q8).
4. **"T" marks every taxable entry, with or without a tax table.** The Taxable cell tests
   `(and (gncEntryGetInvTaxable entry) (gncEntryGetInvTaxTable entry))` (INV:650-654), and a null tax table comes to
   Scheme as `'()`, which is true. TI expects `"T"` for inv-1's entry (TI:100), whose tax table is unset (*oracle*:
   taxable, no table). Only the flag counts.
5. **The picture is always a `file:///` URL.** `make-file-url` prefixes `file:///` to anything that does not already
   start with it (`report/report-utilities.scm:1028-1031`), so `https://example.com/logo.png` becomes
   `file:///https://example.com/logo.png`. Decided item 4's "a URL" does not work in GnuCash either; keeping the
   prefix means an exported page never fetches anything from the network (§3.3).
6. **The entries table's rows are wider than its columns.** With *n* columns shown, an entry row covers *n*
   columns, a total row 1 + max(3, *n*) (INV:670-678) and a payment row max(3, *n*) (:601-606). GnuCash leaves the
   layout to the browser, which puts each payment's amount one column left of the totals'. red-ale's result requires
   every row to cover exactly the table's columns (§3.2).
7. **With the Total column off, negative totals and payments print positive** (`display-subtotal`, INV:570-578). The
   sign depends on a display option.
8. **The invoice's own CSS styles the layout by class name.** The default Layout › CSS (`base-css`, INV:36-45) floats
   the right-hand header cells right and borders the entries table, through `div-align-right`, `entries-table` and
   other classes on GnuCash's `<div>`s. red-ale's report result carries no class names and no report-level CSS (§3).
9. **[`../reports.md`](../reports.md) §8 forbids any image or URL in an exported page**, and Decided item 4 puts the
   picture's string into `<img src>`. One of the two has to give (§3.3).
10. **An invoice option in an options summary prints as a pointer.** `disp` (HU:302-323) has no case for an invoice,
    so it falls through to `(format #f "~a" d)`: `#<swig-pointer _p__gncInvoice 55d…>`, different on every run.

---

## 1. The Printable Invoice (`standard/invoice.scm`)

### 1.1 Inventory

| Template | GUID | Menu | Options generator | Renderer | Licence |
|----------|------|------|-------------------|----------|---------|
| Printable Invoice | `5123a759ceb9483abf2182d01c140e8d` (INV:873) | `_Business` (:881) | `(options-generator 'invoice)` | `reg-renderer` | GPL-2.0-or-later, Derek Atkins 2002, 2003 (:1-22) |
| Easy Invoice (❌) | `67112f318bef4fc496bdc27d106bbda4` (:874) | `_Business` | `(options-generator 'easy-invoice)` | the same | the same |
| Fancy Invoice (❌) | `3ce293441e894423a2425d7a22dd1ac6` (:875) | `_Business` | `(options-generator 'fancy-invoice)` | the same | the same |

All three are `'hook 'invoice` and `'in-menu? #t` (:877-905). The hook is what makes a template an *invoice report*:
`gnc:report-is-invoice-report?` is true for a template with that hook and for a saved configuration (GnuCash's
custom report) whose parent template has it (`report/report-core.scm:482-489`).

**Translating the Printable one alone.** `options-generator` takes the variant and reads exactly seven things from
it through `keylist-get-info` (`(cdr (assq info (cdr (assq key keylist))))`, :374-375): the CSS (:403) and the six
Layout defaults (:521, :527, :533, :539, :545, :551). For `'invoice` those are `none`, `invoice`, `client`, `company`,
`none`, `today` and `base-css` (:342-348). The translation drops `variant-list` and writes these seven defaults
directly; nothing else in the file depends on the variant.

| Variant | Row 3 Right | CSS |
|---------|-------------|-----|
| Printable | `today` | `base-css` |
| Easy | `today` | `base-css` plus six rules: `.invoice-in-progress`, `.invoice-title`, `.main-table > table`, `.invoice-details-table > table`, `.invoice-notes`, `.entries-table > table` (:356-362) |
| Fancy | `none` | `base-css` plus `.company-name` and `.client-name` at `x-large` (:370-372) |

### 1.2 Options

Sections sort by name and options by key within a section (`../reports.md` §6). Every option below is GnuCash's;
"red-ale" says what red-ale's option model (`crates/red-ale-report/src/options/model.rs`) has for it.

| Section | Name | Key | GnuCash type (call) | Default | Help | red-ale |
|---------|------|-----|---------------------|---------|------|---------|
| Display | Due Date | `c` | simple boolean (:445-447) | `#t` | Display due date? | `boolean` |
| Display | Subtotal | `d` | simple boolean (:449-451) | `#t` | Display the subtotals? | `boolean` |
| Display | Use Detailed Tax Summary | `o` | simple boolean (:481-483) | `#f` | Display all tax categories separately (one per line) instead of one single tax line? | `boolean` |
| Display | References | `s` | simple boolean (:485-487) | `#t` | Display the invoice references? | `boolean` |
| Display | Billing Terms | `t` | simple boolean (:489-491) | `#t` | Display the invoice billing terms? | `boolean` |
| Display | Billing ID | `ta` | simple boolean (:493-495) | `#t` | Display the billing id? | `boolean` |
| Display | Invoice owner ID | `tam` | simple boolean (:497-499) | `#f` | Display the customer/vendor id? | `boolean` |
| Display | Invoice Notes | `tb` | simple boolean (:501-503) | `#f` | Display the invoice notes? | `boolean` |
| Display | Payments | `tc` | simple boolean (:505-507) | `#t` | Display the payments applied to this invoice? | `boolean` |
| Display | Job Details | `td` | simple boolean (:509-511) | `#f` | Display the job name for this invoice? | `boolean` |
| Display | Extra Notes | `u` | text (`gnc-register-text-option`, :513-516) | `Thank you for your patronage!` | Extra notes to put on the invoice. | `text` |
| Display | Payable to | `ua1` | complex boolean (:453-458); its callback makes "Payable to string" selectable | `#f` | Display the Payable to: information. | `boolean` through `o.complex_boolean` |
| Display | Payable to string | `ua2` | text (:460-463) | `Please make all checks payable to` | The phrase for specifying to whom payments should be made. | `text` |
| Display | Company contact | `ub1` | complex boolean (:465-469), callback as above | `#f` | Display the Company contact information. | `boolean` through `o.complex_boolean` |
| Display | Company contact string | `ub2` | text (:471-474) | `Please direct all enquiries to` | The phrase used to introduce the company contact. | `text` |
| Display | Minimum # of entries | `zz` | number range (:476-479), a double from Scheme (`5.17:bindings/guile/gnc-optiondb.i:1717`) | 1, min 0, max 23, step 1 | The minimum number of invoice entries to display. | `numberRange` |
| Display Columns | Date | `b` | simple boolean (:409-411) | `#t` | Display the date? | `boolean` |
| Display Columns | Description | `d` | simple boolean (:413-415) | `#t` | Display the description? | `boolean` |
| Display Columns | Action | `g` | simple boolean (:417-419) | `#t` | Display the action? | `boolean` |
| Display Columns | Quantity | `ha` | simple boolean (:421-423) | `#t` | Display the quantity of items? | `boolean` |
| Display Columns | Price | `hb` | simple boolean (:425-427) | `#t` | Display the price per item? | `boolean` |
| Display Columns | Discount | `k` | simple boolean (:429-431) | `#t` | Display the entry's discount? | `boolean` |
| Display Columns | Taxable | `l` | simple boolean (:433-435) | `#t` | Display the entry's taxable status? | `boolean` |
| Display Columns | Tax Amount | `m` | simple boolean (:437-439) | `#f` | Display each entry's total total tax? (sic) | `boolean` |
| Display Columns | Total | `n` | simple boolean (:441-443) | `#t` | Display the entry's value? | `boolean` |
| General | Invoice Number | `x` | **invoice** (`gnc-register-invoice-option`, :391-393; `5.17:libgnucash/engine/gnc-optiondb.cpp:915-923`): a `GncOptionQofInstanceValue` with UI type `INVOICE` | `'()`, no invoice | `""` (none) | **new** |
| General | Custom Title | `z` | string (:395-398) | `""` | A custom string to replace Invoice, Bill or Expense Voucher. | `string` |
| Layout | Row 1 Left | `1a` | multichoice (:518-522) | `none` | 1st row, left | `multichoice` |
| Layout | Row 1 Right | `1b` | multichoice (:524-528) | `invoice` | 1st row, right | `multichoice` |
| Layout | Row 2 Left | `2a` | multichoice (:530-534) | `client` | 2nd row, left | `multichoice` |
| Layout | Row 2 Right | `2b` | multichoice (:536-540) | `company` | 2nd row, right | `multichoice` |
| Layout | Row 3 Left | `3a` | multichoice (:542-546) | `none` | 3rd row, left | `multichoice` |
| Layout | Row 3 Right | `3b` | multichoice (:548-552) | `today` | 3rd row, right | `multichoice` |
| Layout | Picture Location | `zy` | **image** (`gnc-register-pixmap-option`, :405-407; `gnc-optiondb.cpp:725-732`): a `std::string` with UI type `PIXMAP` | `""` | Location for Picture | **new** |
| Layout | CSS | `zz` | text (:400-403) | `base-css` (INV:36-45) | CSS code. This field specifies the CSS code for styling the invoice. Please see the exported report for the CSS class names. | `text` |

The six Layout choices are `layout-key-list` (:314-338) through `keylist->vectorlist` (:377-383), keys as symbols
and no help text: `client` "Client or vendor name, address and ID", `company` "Company name, address and tax-ID",
`invoice` "Invoice date, due date, billing ID, terms, job details", `today` "Today's date", `picture` "Picture",
`none` "Empty space". The help strings "1st row, left" and so on are not marked for translation.

The default section is General (:554). As for every template, `gnc:report-template-new-options` adds General's
"Report name" (default "Printable Invoice"), "Stylesheet" and "Add options summary" (`report/report-core.scm:298-335`);
the renderer reads none of them. Two options share key `zz`, Minimum # of entries (Display) and CSS (Layout), in
different sections, which does not matter.

**The two new types and their wire values** (proposed, additive to [`../reports.md`](../reports.md) §6):

| Type | GnuCash | Value on the wire | In a script | Builder |
|------|---------|-------------------|-------------|---------|
| `invoice` | `GncOptionQofInstanceValue`, UI `INVOICE`; serialized as the GUID (`5.17:libgnucash/engine/gnc-option-impl.cpp:259-284`), saved into `saved-reports-2.8` as that string (`5.17:bindings/guile/gnc-optiondb.i:1242-1247`), restored through `qof_instance_from_guid` (`gnc-option-impl.cpp:238-256`) | the document's GUID, or `null` (the default) | Invoice or `()` | `o.invoice(s, n, k, help, ())`, `gnc-register-invoice-option` |
| `image` | `GncOptionValue<std::string>`, UI `PIXMAP` | a string | string | `o.image(s, n, k, help, default)`, `gnc-register-pixmap-option` |

An invoice value is any GUID, as a budget's is (`../reports.md` §6, `budget`): a saved configuration may be made with
no Book open. At run time a GUID that names no document reads as `()`, which is what GnuCash's failed lookup leaves
(the option keeps its `'()` default) and renders the no-invoice warning (§1.5). Any document qualifies: invoice,
bill, expense voucher or credit note, as the `INVOICE` lookup searches the one `gncInvoice` collection. An imported
saved configuration's Invoice Number is the GUID string GnuCash wrote.

"Image" is red-ale's word for GnuCash's pixmap options already ([`../reports.md`](../reports.md) §8, "Left out"); the
style sheets' image options can stay as they are.

### 1.3 The renderer's Book reads

`reg-renderer` (INV:753-871) reads the invoice option and, if it is `'()`, renders a warning (§1.5). Otherwise every
read below. "Core" is what `red-ale-core` already has; "query" is the proposed read in the style of
[`../reports.md`](../reports.md) §5 (table in §4).

**The document** (`gncInvoice*`, `5.17:libgnucash/engine/gncInvoice.c`):

| Read | Where | GnuCash | Core | Query |
|------|-------|---------|------|-------|
| type | INV:769-782 | `gncInvoiceGetType`: owner type through a job, then credit note (`:1065-1087`) | `ops::business::calc::invoice_type` | `inv.type` as `InvoiceDetail`'s, `inv.isCustomerDocument` (GnuCash's `cust-doc?`) |
| ID | :789 | `gncInvoiceGetID` | `Invoice::id` | `inv.id` |
| owner | :125, :238, :239 | `gncInvoiceGetOwner` | `Invoice::owner` (`OwnerRef`) | `inv.owner` (Owner) |
| posted? | :241 | `gncInvoiceIsPosted`: has a posted transaction (`:1981-1985`) | `Invoice::posted_txn` | `inv.posted` |
| date posted | :246 | `gncInvoiceGetDatePosted` (`:843-847`) | `Invoice::date_posted` | `inv.datePosted` (Time64 or `()`) |
| due date | :251 | `gncInvoiceGetDateDue`: the posted transaction's `trans-date-due`, else its date posted (`:849-856`; `5.17:libgnucash/engine/Transaction.cpp:2298-2303`) | the slot name `ops::business::posting::DATE_DUE`; no read yet | `inv.dateDue` (Time64 or `()`), **new core read** |
| billing ID | :261 | `gncInvoiceGetBillingID` | `Invoice::billing_id` | `inv.billingId` |
| terms | :273-274 | `gncInvoiceGetTerms`, `gncBillTermGetDescription`; a posted document points at the frozen copy, which keeps the description | `Invoice::terms`, `BillTerm::description` | `inv.terms` (BillTerm or `()`: `guid`, `name`, `description`) |
| notes | :830 | `gncInvoiceGetNotes` | `Invoice::notes` | `inv.notes` |
| entries | :118, :610 | `gncInvoiceGetEntries`, sorted by `gncEntryCompare` | `ops::business::documents::entries_of` | `inv.entries()` |
| posted lot, transaction | :565-566 | `gncInvoiceGetPostedLot`, `-PostedTxn` | `Invoice::posted_lot`, `posted_txn` | `inv.postedLot` (Lot), `inv.postedTransaction` (Transaction) |
| currency | :567 | `gncInvoiceGetCurrency` | `Invoice::currency` | `inv.currency` (Commodity) |
| amount positive | :568 | `gncInvoiceAmountPositive`: customer invoice, vendor and employee credit notes (`:1287-1306`) | from `invoice_type` | derived in the script |
| totals | :702, :712, :715, :718, :721 | `gncInvoiceGetTotalSubtotal`, `-TotalTaxList`, `-TotalTax`, `-Total` (`:917-1038`) | `calc::document_totals` (subtotal, tax, total, `taxes` in GnuCash's list order) | `inv.totals()`: `#{ subtotal, tax, total, taxes: [#{ account, value }] }` |
| book | :170, :236, :768 | `gncInvoiceGetBook` | the Book | `book` |

**The owner** (BC, translated into a library module; §4):

| Read | Where | GnuCash | Core |
|------|-------|---------|------|
| name | INV:134 | `gnc:owner-get-name-dep` (BC:63-67): a job's owner's name, else `gncOwnerGetName`. An employee's name is its address name (`5.17:libgnucash/engine/gncEmployee.c:512-517`, `:633-637`) | `contacts::owner_name`, `end_owner` |
| address | :141 | `gnc:owner-get-address-dep` (BC:69-82): the address's name and lines 1-4 that are not empty, joined by newlines; a job's owner's address (BC:35-50). An employee's first line is therefore its name again | `Customer::addr` and the like |
| ID | :149 | `gnc:owner-get-owner-id` (BC:91-105): a job's owner's ID | `Customer::id` and the like |
| job ID and name | :238-239 | `gncJobGetID`, `gncJobGetName` of `gncOwnerGetJob`, which is null unless the owner is a job (`5.17:libgnucash/engine/gncOwner.c:376-381`) | `Job::id`, `Job::name` |

On a document whose owner is not a job, `jobnumber` is the string of a null `gncJobGetID`. TI renders every one of its
documents with Job Details on (TI:85-90) and its customer, vendor and employee documents render, so the value must
read as empty there: no job rows. The Scheme alone would suggest `#f`, which `string-null?` refuses; red-ale reads a
non-job owner as having no job.

**Entries** (`gncEntry*`, `5.17:libgnucash/engine/gncEntry.c`). `cust-doc?` picks the invoice side or the bill side;
`credit-note?` the document's sign.

| Read | Where | GnuCash | Core | Query (`Entry`, `EntryInfo`'s names) |
|------|-------|---------|------|-------|
| date | INV:617 | `gncEntryGetDate` | `Entry::date` | `e.date` |
| description, action | :620, :623 | `gncEntryGetDescription`, `-Action` | fields | `e.description`, `e.action` |
| quantity | :629 | `gncEntryGetDocQuantity(entry, credit-note?)`: negated for a credit note, whose quantity is stored negated (`:952-956`) | `Entry::quantity` | `e.documentQuantity` |
| unit price | :636-638 | `gncEntryGetInvPrice` or `-BillPrice` | `Entry::i_price`, `b_price` | `e.invoicePrice`, `e.billPrice` |
| discount | :644-646 | `gncEntryGetInvDiscount`, `-InvDiscountType` | `Entry::i_discount`, `i_disc_type` | `e.discount`, `e.discountType` |
| taxable | :650-654 | `gncEntryGetInvTaxable`, `-InvTaxTable` or `-BillTaxable`, `-BillTaxTable`; only the flag decides (Facts, 4) | fields | `e.invoiceTaxable`, `e.billTaxable` (and the tables, `e.invoiceTaxTable`, `e.billTaxTable`, for user scripts) |
| tax amount | :662 | `gncEntryGetDocTaxValue(entry, #t, cust-doc?, credit-note?)`: `i_tax_value_rounded`, the per-account taxes summed with rounding at each step (`:1435-1442`), negated for a credit note (`:1541-1546`) | `calc::document_lines` (`DocumentLine::tax`) | `e.documentTaxValue` |
| total | :668 | `gncEntryGetDocValue(entry, #t, cust-doc?, credit-note?)`: the value rounded half up to the currency's fraction (`:1431-1432`), negated for a credit note (`:1535-1539`) | `calc::document_lines` (`DocumentLine::value`) | `e.documentValue` |
| order | :116 | `gncEntryGetOrder`, then `gncOrderGetReference` (:153) | `Entry::order`, `Order::reference` | `e.order` (Order or `()`: `guid`, `id`, `reference`) |

**Payments** (INV:723-736): the posted lot's splits (`gnc-lot-get-split-list`), sorted by `xaccTransOrder` of their
transactions with `sort-list!` (a stable merge sort), leaving out the split of the posting transaction; each split's
`xaccSplitGetAmount` and `xaccTransGetDate`. The query API has these since v0.5: Lot `splits()`, Split `amount` and
`transaction`, Transaction `datePosted` and `t.order(other)`. Every split in the lot other than the posting counts
as a payment, a credit note's lot link included, and is labelled "Payment, thank you!".

**The Book's options**, read through `qof-book-get-option` (`gnc:book-get-option-value`,
`5.17:bindings/guile/options.scm:85-87`), slots under `options/`:

| Read | Where | Slot (`options/…`) | GnuCash's option (`5.17:libgnucash/engine/gnc-optiondb.cpp`) | Core |
|------|-------|--------------------|-----------------------------------------------|------|
| company name | INV:171, :838 | `Business/Company Name` | string (`:1301-1303`) | `book_options::company_name` |
| company address | :172 | `Business/Company Address` | text (`:1304-1306`) | none |
| company contact | :850 | `Business/Company Contact Person` | string (`:1307-1310`) | none |
| phone, fax, email, URL | :173-176 | `Business/Company Phone Number`, `…Fax Number`, `…Email Address`, `…Website URL` | strings (`:1311-1326`) | none |
| company ID | :178 | `Business/Company ID` | string (`:1327-1329`) | none |
| tax number | :177 | `Tax/Tax Number` | string (`:1354-1356`) | none |
| date format | :237 | `Business/Fancy Date Format/custom` (`gnc:options-fancy-date`, `options.scm:403-423`) | date format (`:1347-1350`; frame written at `:448-462`) | none |

Absent or empty values leave their row out (`(and x (not (string-null? x)))`, INV:180-218). The labels are
`gnc:*company-name*` and the rest (`options.scm:388-400`).

**Dates.** `make-date-row` prints the posted and due dates with `gnc-print-time64` and the Book's fancy date format
(:223-228, :237). With no `Fancy Date Format` slot that is `qof-date-format-get-string` of the global format, so the
text equals `qof-print-date`'s (`5.17:libgnucash/engine/gnc-date.cpp:370-385`, `:573-595`). With one, the `custom`
string is a strftime format; GnuCash's dialog fills it with the computed format whatever the choice
(`5.17:gnucash/gnome-utils/gnc-date-format.c:458-461`). Entry dates, payment dates and today's date use
`qof-print-date` (:596, :617, :312). Today is `(current-time)`: in red-ale the run's `now`.

Not read: the date opened, the document's active flag, bill-to, the charge amount, the document link, entries'
notes.

### 1.4 The layout

**The document** (INV:753-871) has no title (`gnc:html-document-set-title!` is never called) and one object: a
`div.main-table` holding a two-column table. Its style text is the Layout › CSS option (:795), which every style
sheet appends after its own CSS (`report/html-fonts.scm:162-192`; `report/stylesheets/css.scm:128`).

| Main-table row | Content | Lines |
|----------------|---------|-------|
| 1 | one cell over both columns: `div.invoice-title` with `~a #~a`, the Custom Title or the default title, and the ID: "Invoice", "Bill", "Expense Voucher" or "Credit Note" (all three credit note kinds): `Invoice #0001` | :772-789, :799-802 |
| 2-4 | Row *n* Left, then Row *n* Right wrapped in `div.div-align-right`; each the chosen renderer's output, `none` an empty cell | :790-793, :804-820 |
| 5 | one cell over both columns: `div.entries-table` with the entries table | :822-827 |
| 6 | with Invoice Notes: `div.invoice-notes`, the notes, newlines as `<br>` | :829-835 |
| 7 | with Payable to and a company name: `div.invoice-footer-payable-to`, `<Payable to string>: <name>` | :837-847 |
| 8 | with Company contact and a contact: `div.invoice-footer-company-contact`, `<Company contact string>: <contact>` | :849-859 |
| 9 | always: `div.invoice-notes`, the Extra Notes | :861-866 |

**The Layout renderers:**

- `client` (:111-161): `div.client-table`, a one-column table: the owner's name (`div.maybe-align-right.client-name`),
  its address (`client-address`, newlines as `<br>`), with Invoice owner ID its ID (`client-id`), then with
  References one row `REF <reference>` per order with a non-empty reference. The orders are collected by folding
  `cons` over the entries (:113-118), so they come in the reverse of the order their first entries appear; an entry
  with no order adds `'()`, whose reference is null and skipped.
- `company` (:163-220): `div.company-table`, one row per non-empty value, each `div.maybe-align-right` plus
  `company-name`, `company-address`, `company-phone`, `company-fax`, `company-email`, `company-url`,
  `company-tax-id` (Company ID) or `company-tax-nr` (Tax Number), in that order.
- `invoice` (:230-298): `div.invoice-details-table`, two columns. Posted: `Date:` and the posted date, then with Due
  Date `Due Date:` and the due date, values in `div.div-align-right`. Unposted: one cell over both columns,
  `span.invoice-in-progress` "Invoice in progress…" (U+2026). Then with Billing ID and a non-empty one: `Reference:`
  and the billing ID (newlines as `<br>`), followed by an empty row (:270). With Billing Terms and a non-empty
  description: `Terms:` and the description. With Job Details and a job: `Job number:` and `Job name:`.
- `today` (:310-312): `div.invoice-print-date`, today by `qof-print-date`.
- `picture` (:300-308): `div.picture` with `<img src=…>`, the src `make-file-url` of Picture Location (Facts, 5),
  written by `gnc:html-markup-img` with Scheme's `~s` quoting, not HTML escaping (`report/html-text.scm:227-231`).
- `none` (:336-338): nothing.

**The entries table** (`make-entry-table`, INV:558-750). Headers are the shown columns' labels (:84-103): Date,
Description, Action, Quantity, Unit Price, Discount, Taxable, Tax Amount, Total. With *n* shown columns:

| Row | Markup | Cells | Lines |
|-----|--------|-------|-------|
| one per entry | `normal-row`, `alternate-row` alternating from `normal-row` | the shown columns: date (`qof-print-date`); description; action; quantity, `number-cell`; unit price, `number-cell`, `gnc:default-price-renderer` of the currency; discount, `number-cell`: a percent as `<number> %`, a value as money, and on bills and vouchers `""`; `T` or `""`; tax amount, money; total, money | :612-668 |
| padding up to Minimum # of entries | continuing the alternation | *n* empty cells | :692-699 |
| with Subtotal: Net Price | `grand-total` | `total-label-cell` "Net Price", then `total-number-cell` over max(3, *n*) columns | :670-678, :701-703 |
| with Use Detailed Tax Summary: one per tax account | `grand-total` | the account's name (`xaccAccountGetName`), the account's tax | :705-712 |
| otherwise: Tax | `grand-total` | "Tax", the total tax | :714-716 |
| Total Price | `grand-total` | "Total Price", the total | :718-719 |
| with Payments: one per payment | `grand-total` | with Date the payment's date; with Description "Payment, thank you!"; the amount, `total-number-cell` over max(3, *n*) less those two columns | :580-606, :723-736 |
| Amount Due | `grand-total` | "Amount Due", the total plus the payments | :738-739 |

The padding loop runs while `Minimum # of entries` less the number of entries is positive, decreasing by one, so a
fractional option value pads to its ceiling (2.5 with one entry gives two rows).

With Use Detailed Tax Summary and no taxes, no tax row at all appears: TI's documents with every Display option on
show Net Price, Total Price and Amount Due only (TI:96-98).

**Signs.** Every figure is as printed on the document: `gncEntryGetDoc*` and `gncInvoiceGetTotal*` negate a credit
note's internal values, so a credit note's quantities, values and totals print positive like an invoice's. Payments
are the lot splits' amounts, negated unless the document's amount is positive (:568, :583-586):

| Document | `gncInvoiceAmountPositive` | Posting split in the lot (*oracle*) | A payment prints | Amount Due after full payment |
|----------|:--------------------------:|-------------------------------------|------------------|-------------------------------|
| customer invoice | yes | +total (debit A/R) | its amount, negative | 0 |
| vendor bill, expense voucher | no | −total (credit A/P) | negated: negative | 0 |
| customer credit note | no | −total | negated: negative | 0 |
| vendor or employee credit note | yes | +total | its amount, negative | 0 |

So a settling payment always prints negative. With the Total column off, `display-subtotal` prints the absolute value
of every total and payment instead (:570-578).

### 1.5 No invoice

With the option `'()`, the document holds `gnc:html-make-generic-warning` (INV:760-766; HU:350-357): `<h3>Invoice:</h3>`,
an empty `<h3>`, the paragraph "No valid invoice selected. Click on the Options button and select the invoice to use."
and an options link. red-ale's `lib/html-utilities` `make_generic_warning("Invoice", "", …)` gives the warning block
titled "Invoice:" (`crates/red-ale-report/scripts/lib/html-utilities.rhai:136-142`). That is also what a deleted
document gives (§1.2).

### 1.6 How GnuCash prints an invoice

- **`gnc:invoice-report-create`** (`5.17:gnucash/report/reports/reports.scm:68-80`): if the template GUID is a known
  template, make its options, set General › Invoice Number to the document, make the report; otherwise return 0 (no
  report). In red-ale: `report.run { report, options: { General: { "Invoice Number": "<guid>" } } }`, Decided item 6.
- **Which template.** Print Invoice (`5.17:gnucash/gnome/dialog-invoice.c:1009-1014`, `:3450`, `:3469`) asks
  `use_default_report_template_or_change` (`:845-941`): with the Book's Default Invoice Report Timeout at 0 and a
  valid default it takes the default; otherwise it shows a chooser of invoice reports
  (`gnc:custom-report-invoice-template-guids`, `report/report-core.scm:492-495`) for that many seconds. Then
  `gnc_invoice_window_print_invoice` (`:943-985`) **falls back to the Printable Invoice** when the GUID is not an
  invoice report (`:957-963`) and calls `gnc:invoice-report-create`.
- **The Default Invoice Report** is the string slot `options/Business/Default Invoice Report`, written
  `<guid>/<name>` (`5.17:libgnucash/engine/qofbook.cpp:1043-1087`). Its GUID is the 32 characters before the first
  `/` when the slash is the 33rd character (`:1089-1116`), its name what follows the slash, `""` when nothing does
  (`:1118-1147`). The GUID may name a template or a saved configuration of one (`report-core.scm:482-489`). With no
  slot, `gnc_get_default_invoice_print_report` migrates the old `invoice-printreport` preference into the Book (name
  `" "`) or returns the Printable Invoice (`5.17:gnucash/gnome/business-gnome-utils.c:109-143`); the migration is a
  GUI step red-ale does not copy. The option itself is `gnc_register_invoice_print_report_option` (UI `INV_REPORT`,
  `gnc-optiondb.cpp:937-944`, `:1330-1333`); the timeout is a double, 0-20 (`:1334-1337`, `qofbook.cpp:1149-1170`).
- **Proposed for red-ale's From GnuCash page:** read the Default Invoice Report; when it names nothing that
  `report.list` lists as an invoice report (a template whose General › Invoice Number is of type `invoice`, or a saved
  configuration of one), run the Printable Invoice, as GnuCash does. That needs the slot to be readable (Facts, 3).

### 1.7 Floating point and rounding

- **One double.** "Minimum # of entries" is a double in GnuCash; red-ale holds it as a Rational and the padding count
  is the same (§1.4). "Default Invoice Report Timeout" is a double the GUI reads; no report does.
- **Entry values** round half up to the currency's fraction, and an entry's tax rounds half up at each step of its
  sum, before the report sees them (§1.3). The document's tax per account sums the entries' **unrounded** taxes and
  rounds once, and the total tax is the sum of those; so the Tax row need not equal the sum of the Tax Amount column.
  Core does all of this already (`calc::document_totals`, `document_lines`); the oracle tests check it.
- **Display.** Money (`gnc:make-gnc-monetary`) goes through the monetary renderer: rounded to the commodity's SCU
  and printed with its print info (`report/html-style-info.scm:224-231`), red-ale's monetary cell. The unit price is
  `gnc:default-price-renderer` (`:221-222`), red-ale's price cell (`../reports.md` §3); GnuCash's "force price
  decimal" preference, which red-ale does not have, is off by default. The quantity and a percentage discount are
  `xaccPrintAmount` with `gnc-default-print-info #f` (`:217-218`): at least the Locale's `frac_digits` decimals (2
  under C), **no maximum and no rounding**, grouped, a non-decimal remainder as `w + n/d`
  (`5.17:libgnucash/app-utils/gnc-ui-util.cpp:834-860`, `:1083-1180`). So 1/8 prints `0.125`, 4/3 prints `1 + 1/3`
  and 1/3 prints `1/3`, a whole part of 0 being left out (`:1237-1242`). No red-ale cell prints this way: number
  cells round to `places` (§3.2).
- No other arithmetic: the report adds the total and the payments in a commodity collector, exactly.

### 1.8 GnuCash's test (`standard/test/test-invoice.scm`)

**The book.** TI builds its own account tree (TI:40-49, unused by the invoices) and calls `create-test-invoice-data`
(TEE:487-849), which makes eight documents: inv-1 a customer invoice with a billing ID (TEE:525-531), inv-2 a job's
invoice (:544-549), inv-3 a vendor bill (:564-569), inv-4 an expense voucher (:582-587), inv-5, -6 and -7 credit
notes copied from inv-1, -3 and -4 before those have entries, so they keep the notes and billing ID (:590-608), and
inv-8 a customer invoice with eight entries combining taxable, tax included, discount type and timing over a 10.9 %
tax table, all in one order with reference `order-ref`, with a billing term `billterm-desc`, posted and paid in full
(:610-847). inv-1 to -7 each have one entry of 2 at $3 (:638-712).

**red-ale's fixture.** `fixtures/reports/transaction.gnucash`, built by `scripts/report-fixtures/transaction.py` and
`invoice_data.py`, replays `create-test-invoice-data` in GnuCash 5.17 into the Transaction Report's book. It holds all
eight documents, IDs `0001` to `0008`, GUIDs `30000000000000000000000000000000` to `…07`, with 15 entries, the order,
the billing term and its frozen copy, four tax tables and inv-8's payment (*oracle*: inv-8 subtotal 16368.17, tax
1111.01, total 17479.18, paid; its lot holds the posting split +17479.18 and the payment −17479.18). Its deviations
from TI's book do not touch the invoice report: each document is opened on its posting day (the report does not read
the date opened), entries are dated at the pinned now, and the invoice accounts sit beside the Transaction Report's.
TI's own account tree is not in it and is not needed. TI calls the generator again for the Easy and Fancy variants
(TI:24-26); red-ale ports the Printable run only.

**How TI reads the HTML.** `sxml->table-row-col` (`report/test/test-report-extras.scm:96-109`) takes the first
`<table>` inside the `div` of a class, then `<td>` text nodes: row `#f` is every row, row *k* the *k*-th `<tr>`,
column −1 the last `<td>`. Header cells are `<th>`, never counted; an empty cell has no text node and drops out of
the list. "In the body" means some text node of `<body>` equals the string.

**Assertions**, with every Display Columns and Display boolean set to true (or, "sparse", false), TI:77-91:

| # | Lines | Document | Asserts | Expected |
|--:|-------|----------|---------|----------|
| 1-19 | 35-38, 79-90 | each | every option `set-option!` names exists (General Invoice Number; nine Display Columns; Due Date, Use Detailed Tax Summary, Subtotal, References, Billing Terms, Billing ID, Invoice Notes, Payments, Job Details) | no `wrong-option` |
| 20 | 96-98 | inv-1 | entries table, last cell of each row | `$6.00` ×4 (entry, Net Price, Total Price, Amount Due) |
| 21 | 99-101 | inv-1 | entries row 1 less its date | `entry-1-desc`, `entry-1-action`, `2.00`, `$3.00`, `0.00 %`, `T`, `$0.00`, `$6.00` |
| 22 | 102-104 | inv-1 | client table row 1 | `cust-1-name` |
| 23 | 105-108 | inv-1 | in the body | `inv-1-billing-id` |
| 24 | 109-112 | inv-1 | in the body | `inv-1-notes` |
| 25 | 118-120 | inv-1 sparse | entries table, first cell of each row | `Tax`, `Total Price`, `Amount Due` |
| 26 | 121-123 | inv-1 sparse | last cells | `$0.00`, `$6.00`, `$6.00` |
| 27 | 129-131 | inv-2 | last cells | `$6.00` ×4 |
| 28 | 132-134 | inv-2 | row 1 less date | `entry-inv-2-desc`, `entry-inv-2-action`, `2.00`, `$3.00`, `0.00 %`, `T`, `$0.00`, `$6.00` |
| 29 | 135-137 | inv-2 | client row 1 (through the job) | `cust-1-name` |
| 30-32 | 138-149 | inv-2 | in the body | `inv-2-notes`, `job-1-id`, `job-1-name` |
| 33 | 155-157 | inv-3 (bill) | last cells | `$6.00` ×4 |
| 34 | 158-160 | inv-3 | row 1 less date: no discount on a bill | `entry-inv-3-desc`, `entry-inv-3-action`, `2.00`, `$3.00`, `T`, `$0.00`, `$6.00` |
| 35 | 161-163 | inv-3 | client row 1 | `vend-1-name` |
| 36 | 164-167 | inv-3 | in the body | `inv-3-notes` |
| 37-40 | 173-185 | inv-4 (voucher) | as inv-3 | `$6.00` ×4; `entry-inv-4-…` with `T`; `emp-1-name`; `inv-4-notes` |
| 41-43 | 191-199 | inv-5 (customer credit note) | last cells; row 1; client | `$6.00` ×4; `entry-5-desc`, `entry-5-action`, `2.00`, `$3.00`, `0.00 %`, `T`, `$0.00`, `$6.00`; `cust-1-name` |
| 44-47 | 205-217 | inv-6 (vendor credit note) | as inv-3 | `$6.00` ×4; `entry-inv-6-…`; `vend-1-name`; `inv-3-notes` (copied) |
| 48-51 | 223-235 | inv-7 (employee credit note) | as inv-3 | `$6.00` ×4; `entry-inv-7-…`; `emp-1-name`; `inv-4-notes` (copied) |
| 52 | 241-244 | inv-8 | in the body | `billterm-desc` |
| 53 | 245-248 | inv-8 | in the body | `REF order-ref` |
| 54 | 249-251 | inv-8 | invoice details row 1, column 1 | `Date:` |
| 55 | 252-254 | inv-8 | invoice details row 2, column 1 | `Due Date:` |
| 56 | 255-259 | inv-8 | last cells | `$2,133.25`, `$2,061.96`, `$2,133.25`, `$2,061.96`, `$2,133.25`, `$2,133.25`, `$1,851.95`, `$1,859.30` (entries), `$16,368.17` (Net Price), `$1,111.01` (VAT-on-Sales), `$17,479.18` (Total Price), `-$17,479.18` (payment), `$0.00` (Amount Due) |
| 57 | 260-261 | inv-8 | `gncInvoiceIsPaid`: the posted lot is closed (`gncInvoice.c:1987-1992`), a Book read, not the report | true |

The *oracle* gives exactly #56's eight entry values in this order (the entries sort by description: same date, no
entered date) and the totals.

**Porting.** A Rust test runs the Printable Invoice on the fixture, XML and SQLite, with `now` the fixture's pinned
now, and reads the report result, not the HTML:

- "entries table", "client table", "invoice details table" are the table blocks with those classes (§3.2);
- a row's "last cell" is its last non-`null` cell's `display` or `text` (red-ale pads rows to the table's width,
  §3.2), and "row *k* less its date" the cells of the *k*-th data row after the first, dropping `null` and empty ones
  as TI's text nodes drop empty cells;
- "in the body" is any `text` or `display` string of the result equal to the expected one;
- #1-19 read `report.options` for the template; #57 checks the posted lot's balance is zero through core.

Every assertion has a counterpart; none needs Guile.

**Not reached by TI**, so hand-worked cases ([`../milestone6.md`](../milestone6.md), "Testing"), on a new fixture
`reports/invoice-extras.gnucash` built by the oracle (the Business slots set with `qof_book_set_string_option`,
`5.17:libgnucash/engine/qofbook.cpp:1194-1205`): an unposted document ("Invoice in progress…"); the company table
and both footers with every Business option set; Invoice owner ID; a customer address on several lines; a Fancy Date
Format; a tax table over two accounts with Use Detailed Tax Summary on and off; a value discount; quantities 1/8 and
1/3; Minimum # of entries above the entry count, and fractional; a bill and a credit note each with a partial
payment, with the Total column on and off; References with two orders and an entry without one; Custom Title; every
Layout choice, the picture among them, with an empty and a set Picture Location; a changed CSS; no invoice; a deleted
invoice's GUID.

---

## 2. The Dashboard (`standard/dashboard.scm`)

### 2.1 What GnuCash builds

Not a template. `gnc:make-dashboard` (`standard/dashboard.scm:40-65`, GPL-2.0-or-later, Bill Gribble 2001, Geert
Janssens 2019) runs from a menu item (`5.17:gnucash/gnome/report-menus.scm:113-121`): name `Dashboard`, GUID
`ad80271c890b11dfa79f2dcedfd72085` (`:117`), tip "A basic dashboard for your accounting data" (`:118`), under Reports
› Multicolumn (`:119`). Each click makes a new Multicolumn View instance (`d8ba4a2e89e8479ca9f6eccdeb164588`, `:34`)
and four child instances with their templates' default options (`:41-45`), then sets on the view:

- General › "Report name" = `(G_ "Dashboard")` (`:51`), translated;
- General › "Number of columns" = 2 (`:52`; the template's default is 1, `standard/view-column.scm:44-47`);
- `__general` › `report-list` (`:60-64`), each `(report wide high #f)`:

| # | Child | Template GUID (`:35-38`) | wide (colspan) | high (rowspan) |
|--:|-------|--------------------------|:----:|:----:|
| 0 | Expense Accounts (Expense Piechart) | `9bf1892805cb4336be6320fe48ce5446` | 1 | 1 |
| 1 | Income Accounts (Income Piechart) | `e1bd09b8a1dd49dd85760db9d82b045c` | 1 | 1 |
| 2 | Income/Expense Chart | `80769921e87943adade887b9835a7685` | 2 | 1 |
| 3 | Account Summary | `3298541c236b494998b236dfad6ad752` | 2 | 1 |

The children are marked to be saved with the view (`:55-58`). GnuCash has no test for it.

Laid out by `render-view` (`standard/view-column.scm:51-159`) at width 2: row 0 holds the two pies, row 1 the chart,
row 2 the summary.

### 2.2 In red-ale

The Multicolumn View (`crates/red-ale-report/scripts/standard/view-column.rhai`) expresses it with no new code: its
children are template GUIDs with no `options`, which run on their templates' defaults at each run, as GnuCash's fresh
children run on theirs. Its stored options, in the Configuration's wire form (`../reports.md` §9):

```json
{ "General": { "Report name": "Dashboard", "Number of columns": 2 },
  "__general": { "report-list": [
    { "report": "9bf1892805cb4336be6320fe48ce5446", "colspan": 1, "rowspan": 1 },
    { "report": "e1bd09b8a1dd49dd85760db9d82b045c", "colspan": 1, "rowspan": 1 },
    { "report": "80769921e87943adade887b9835a7685", "colspan": 2, "rowspan": 1 },
    { "report": "3298541c236b494998b236dfad6ad752", "colspan": 2, "rowspan": 1 } ] } }
```

The script's grid comes out as GnuCash's: `allocs` reaches 2 after child 1 and closes row 0; children 2 and 3 each
fill a row.

### 2.3 A built-in saved configuration

Saved configurations live only in the Configuration's `reports.saved` (`../reports.md` §9); `ReportInfo.kind` is
`builtin`, `user` or `saved` (`../api/reports.md`, "Types"), and nothing built in is a saved configuration yet.
Proposed, following Decided item 5:

- **Where.** A list in `red-ale-report`'s library, compiled in beside the built-in scripts: each entry the same
  record a Configuration entry deserializes into (`guid`, `name`, `template`, `options`) plus GnuCash's menu fields.
  The Configuration never stores it.
- **`report.list`** lists it as `kind: "saved"`, `template: "d8ba4a2e89e8479ca9f6eccdeb164588"`, `script:
  "standard/view-column.rhai"`, with a new optional field `readOnly: true`, and, unlike user configurations, the menu
  item's fields: `menuPath: ["_Multicolumn"]`, `menuName: "Dashboard"`, `menuTip: "A basic dashboard for your
  accounting data"`, `inMenu: true`. `kind` keeps its three values, so a client that switches on it is not surprised.
- **`report.options`, `report.run`, `report.export`** treat it as any saved configuration: `report.run { report:
  "Dashboard" }` or by GUID runs the view with these values; `options` change them for that run.
- **`report.updateConfiguration`, `report.renameConfiguration`, `report.deleteConfiguration`** refuse it with
  `conflict` ("the entity exists but may not be changed this way"), saying to copy it.
- **Copying** is `report.saveConfiguration { name, report: "Dashboard", options? }`, which already makes a new
  configuration of the same template from a saved one.
- **Clashes.** A Configuration entry with the built-in's GUID is left out and listed under `problems` with
  `report-load`, as a user script reusing a built-in GUID is (`../reports.md` §11). Names: see §6 Q5.

---

## 3. The report result and the renderer

What the invoice needs from [`../reports.md`](../reports.md) §7 and §8, each change additive.

### 3.1 What fits as it is

- Sections and blocks; `text` blocks with newlines (shown as `<br>`, §8); `warning` blocks (§1.5); `group` blocks for
  the left and right halves of a header row, an empty text block standing for `none`.
- Tables: the client and company tables as one-column tables, the invoice details as a two-column table, all with
  empty column labels, which the renderer already writes without a header
  (`crates/red-ale-report/src/render/table.rs:23-24`). Their rows set `alternate: false`, since GnuCash gives them
  no row style. The `Reference:` row's empty follower is a `spacer` row.
- The entries table: `date` cells (`qof-print-date`), text cells, `price` cells for the unit price, `monetary` cells
  for money, `number` cells for quantities and percentages (but see §3.2), `span` for the total and payment cells, row
  role `grandTotal` for every row GnuCash marks `grand-total`, data rows alternating by position as GnuCash's do.

### 3.2 What needs an addition

| Addition | Why | Proposal |
|----------|-----|----------|
| **Exact number display** | Quantities and percentage discounts print with at least `frac_digits` decimals, no maximum and no rounding (§1.7); a number cell rounds to `places` | `numberFormat: "exact"` on a number cell (§7 already has `numberFormat: "share"`): `xaccPrintAmount` with `gnc_default_print_info(FALSE)`, no `places`, no `rounding`; `suffix` allowed (`" %"`) |
| **Class names** | The CSS option, and its default, style the invoice by GnuCash's class names on its `div`s and cells (Facts, 8; §1.4) | optional `class` on any block and any cell: one or more names separated by single spaces, each `[A-Za-z][A-Za-z0-9_-]*`. The renderer wraps a classed block in `<div class="…">` with the block's own element as its only child, so `.entries-table > table` matches, and adds a cell's classes to its `<td>` |
| **Report CSS** | `gnc:html-document-set-style-text!` (INV:795): the report's own CSS, appended after the style sheet's | optional `css` on the result: appended to the page's one `<style>` after the style sheet's CSS, under the same rule as the CSS template's text (§8: no `</style`, `@import`, `url(`, `image(`, `image-set(`, `src(`, backslash) |
| **Image** | the `picture` layout choice | `{ "kind": "image", "src": "…" }`; the renderer writes `<img src="…">`, the src HTML-escaped and otherwise as given |

**Rows wider than the columns** (Facts, 6) need no addition: the script declares 1 + max(3, *n*) columns, the shown
ones then unlabelled `text` columns, and pads entry rows (and payment rows, which GnuCash leaves one column short)
with `null` cells at the end. Every row then covers the table exactly, the figures land in the columns GnuCash's
browser puts them in, and the header shows only the shown columns' labels. With no column shown, the table has four
unlabelled columns and no header.

**The title.** GnuCash's document has no title; the invoice title is a `div` in the body. Proposed: the result's
`title` is `""`, as the Multicolumn View's is, and the first block is a heading text block with class
`invoice-title` (§6 Q1).

### 3.3 The picture and §8

§8 says an exported page carries no image or URL of any kind, and that a document link shows as text so a page
"carries no path to a user's files". Decided item 4 writes the Picture Location into `<img src>`. Proposed: §8 names
the `image` block as its one exception, written only when a script puts one in the result. Keeping `make-file-url`
(Facts, 5) means the src always starts `file:///`: the page may name a local file, but never fetches anything from
the network, and red-ale never reads the file (Decided item 4). An empty Picture Location gives `file:///`, as in
GnuCash, which a browser shows as a broken image.

---

## 4. Proposed query API additions

In the style of [`../reports.md`](../reports.md) §5. Properties take `InvoiceDetail` and `EntryInfo`'s names
([`../api/business-documents.md`](../api/business-documents.md)); every read is a thin wrapper over core.

| Function or property | Returns | Mirrors | GnuCash | Core |
|----------------------|---------|---------|---------|------|
| `book.invoice(guid)` | Invoice or `()` | `invoice.get` | `gncInvoiceLookup` | `Book::business.invoices` |
| `book.option(path)` | string, integer, Rational, GUID string or `()` | `book.options` | `qof-book-get-option` (`options.scm:85-87`): the slot `options/<path…>`; a double comes back as the exact Rational of its value, a frame as `()` | `Slots` path read |
| Invoice: `guid`, `id`, `type`, `isCustomerDocument`, `owner`, `currency`, `posted`, `datePosted`, `dateDue`, `terms`, `billingId`, `notes`, `postedLot`, `postedTransaction` | | `InvoiceDetail` | §1.3 | `Invoice`; **new** `posting::date_due` |
| `inv.entries()` | Entry[], document order | `InvoiceDetail.entries` | `gncInvoiceGetEntries` | `documents::entries_of` |
| `inv.totals()` | `#{ subtotal, tax, total, taxes: [#{ account, value }] }`, signed as printed | `invoice.totals` | `gncInvoiceGetTotal*` | `calc::document_totals` |
| Entry: `guid`, `date`, `description`, `action`, `notes`, `quantity`, `documentQuantity`, `invoicePrice`, `billPrice`, `discount`, `discountType`, `discountHow`, `invoiceTaxable`, `invoiceTaxTable`, `billTaxable`, `billTaxTable`, `order` | | `EntryInfo` | `gncEntryGet*` | `Entry` |
| Entry: `documentValue`, `documentTaxValue` | Rational, rounded, signed as printed on its document | `invoice.totals` `entries` | `gncEntryGetDocValue`, `-DocTaxValue` with `round` true | `calc::document_lines` |
| Owner: `kind`, `guid`, `id`, `name`, `address` (`#{ name, addr1, addr2, addr3, addr4, phone, fax, email }`, `()` for a job), `owner` (a job's customer or vendor, else `()`) | | `OwnerInfo`, `CustomerInfo`… | `gncOwnerGet*` | contacts |
| Order: `guid`, `id`, `reference`; BillTerm: `guid`, `name`, `description` | | | `gncOrderGet*`, `gncBillTermGet*` | `Order`, `BillTerm` |
| `ctx.print_time(t, format)` | string | | `gnc-print-time64` (`gnc-date.cpp:370-385`), month and day names from the Locale | the context's strftime, which `print_month_year` uses |

`book.option` replaces nothing: `book.company_name()` stays. `lib/business.rhai` translates the three BC helpers
the invoice uses (`owner_get_name_dep`, `owner_get_address_dep`, `owner_get_owner_id`, BC:63-105) and
`company_info(book, key)` (`options.scm:409-411`), keeping BC's GPL-2.0-or-later notice.

---

## 5. Implications for red-ale

What to build, in order. Sizes are Rust lines without tests and docs, or Rhai lines where said.

1. **Core reads** (~40): `posting::date_due` (the posted transaction's due date, else its date posted); nothing else
   in core is missing for the invoice.
2. **Query API** (§4, ~300): the Invoice, Entry, Owner, Order and BillTerm handles; `book.invoice`, `book.option`;
   `ctx.print_time`. Documented in [`../reports.md`](../reports.md) §5 with the core table.
3. **Option types `invoice` and `image`** (~150): model, wire reading and writing, `report.options` and the
   params schema, the builder (`o.invoice`, `o.image`), the run-time value, the options summary (an invoice shows its
   GUID, where GnuCash prints a pointer, Facts 10), and §15's import of an invoice GUID. `redAleApi` 0.6.0.
4. **Report result additions** (§3.2, ~200): `numberFormat: "exact"`, `class` on blocks and cells, `css` on the
   result, the `image` block; their validation and HTML. §7 and §8 updated, §8's no-image rule given its exception.
   Every earlier report's golden bytes stay the same, since none sets the new fields.
5. **`standard/invoice.rhai` and `lib/business.rhai`** (Rhai, ~400): the Printable Invoice alone, its defaults written
   from `variant-list`'s `'invoice` entry. Registered in §11 (`Printable Invoice`, `5123a759ceb9483abf2182d01c140e8d`,
   `_Business`, `standard/invoice.scm:877-885`) and §12.
6. **TI ported** (§1.8): 57 checks on `reports/transaction.gnucash`, XML and SQLite. No fixture change needed.
7. **The hand-worked fixture and cases** (§1.8): `scripts/report-fixtures/invoice_extras.py`,
   `reports/invoice-extras.gnucash`.
8. **Built-in saved configurations and the Dashboard** (§2.3, ~150): the library list, `report.list`'s `readOnly`
   and menu fields, the refusals, the GUID clash; the Dashboard entry. Its test: the same bytes as a Multicolumn View
   saved by hand with the same children.
9. **Golden bytes** for both reports on every fixture, XML and SQLite.
10. **Records.** Rows in [`../differences-from-gnucash.md`](../differences-from-gnucash.md) and §14: the result's
    empty title and padded entries table; the invoice's options summary text; `ReportInfo` for the Dashboard (a saved
    configuration where GnuCash has a menu item); the image block's HTML escaping where GnuCash writes Scheme quoting;
    a non-job owner read as having no job.

If Q8 below is taken, the Business book options come between 1 and 2 (~150).

## 6. Open questions, with proposed answers

1. **The result's title.** `""` with an `invoice-title` heading block, as GnuCash's document has no title; or the
   invoice title as `title`, which names the page in `<title>` and for clients? *Proposed:* `""` and the block, as
   GnuCash; a client finds the title in the first block.
2. **An invoice GUID that names no document.** Refuse in `report.run`, or read as `()`? *Proposed:* read as `()` and
   render the no-invoice warning, as for a deleted budget and as GnuCash shows.
3. **The CSS option holding refused text** (`url(` and the rest). Fail the run, or leave it out? A style sheet's CSS
   is refused when set, but the host cannot tell which report option feeds `css`. *Proposed:* the renderer leaves the
   `css` out and appends a `debug` log entry saying why; figures are unaffected and `report.run` is unchanged.
4. **Class names in scope?** They change no figure. *Proposed:* yes: without them the invoice's CSS option, whose
   help text tells users to style by class name, does nothing, and the default layout loses its right-hand column and
   table borders.
5. **The Dashboard's name and a user's saved "Dashboard".** GnuCash users who saved their dashboard have a custom
   report named "Dashboard" (GnuCash names it from "Report name"). If the built-in's name were reserved,
   `report.importConfigurations` would skip theirs as `conflict`. *Proposed:* built-in names are not reserved, as
   template names are not; a name matching both gives `conflict` with `data.matches`, and the GUID always works.
6. **Is a template an invoice report?** GnuCash uses `'hook 'invoice`. *Proposed:* no new field: a template whose
   General › Invoice Number has type `invoice`, and saved configurations of it. The From GnuCash page states the rule.
7. **The Easy and Fancy invoices.** Decided item 2 makes them ❌, but each is the Printable Invoice with other
   defaults (Facts, 1), so with item 8 of §5 each costs one built-in saved configuration and its golden bytes, and a
   Book whose Default Invoice Report names one then prints as GnuCash does. Imports of GnuCash custom reports built on
   them would still be `untranslated` unless §15 maps their parent to the Printable Invoice. *Proposed:* raise with the
   maintainer; if taken, they keep GnuCash's GUIDs and names and `report.list` shows them as `readOnly` saved
   configurations.
8. **The Business book options.** No method reads or writes them (Facts, 3). *Proposed:* add to `book.options` and
   `book.setOptions` the Business tab's strings (company name, address, contact, phone, fax, email, URL, ID), the Tax
   tab's Tax Number, and `defaultInvoiceReport` (`{ guid, name }` or `null`, parsed and written as §1.6 describes);
   Fancy Date Format read only. It needs a `features.md` row and fixes the milestone's "Compatibility in v0.6"
   sentence. If not taken, the milestone and §9 must say red-ale only reads these slots, through `book.option` in
   reports.
9. **Decided item 6's untranslated default.** *Proposed:* the From GnuCash page tells clients to fall back to the
   Printable Invoice, as GnuCash does (§1.6), rather than stop at `not-found`.
10. **`exact` as the name** of the new number display. *Proposed:* yes; `numberFormat` already has `share`, and the
    point of this one is that nothing is rounded.
