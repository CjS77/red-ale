# GnuCash 5.17 XML format: business objects

Scope: the exact XML that GnuCash 5.17 reads and writes for business objects (billing terms, customers, employees,
entries, invoices/bills/vouchers, jobs, orders, tax tables, vendors), the shared `address` and `owner`
sub-structures, the KVP slots that business features put on business objects, transactions, lots and the book, and the
load-time scrubs that rewrite business data. The core entities (book, commodity, account, transaction, split, price,
lot, slot encoding, `ts:date`/numeric/GUID text formats and libxml2 serialisation) are in the core XML note in this
directory; this note refers to it as "the core note".

All citations are `5.17:<path>:<line>` against tag 5.17 (commit 0e302675ff). Paths below abbreviate
`libgnucash/backend/xml/` as `xml/` and `libgnucash/engine/` as `engine/` **only in tables**; prose citations use full
paths. Verbatim excerpts come from oracle runs (GnuCash 5.17 library and Python bindings), described in §12.

## 0. Terms and clashes with our vocabulary

| GnuCash term | Meaning here | Clash to flag in `glossary.md` |
|---|---|---|
| Invoice (`gnc:GncInvoice`) | Any business document: customer invoice, vendor bill, employee expense voucher, or a credit note of any of them. Kind is **derived** from owner type and the `credit-note` slot, never stored. | "Invoice" in XML ≠ customer invoice only. |
| Entry (`gnc:GncEntry`) | One line of an invoice, bill, voucher or order. | Not a journal entry; unrelated to Split. |
| Tax table entry (`gnc:GncTaxTableEntry`) | One (account, type, amount) line of a tax table. | `tte:amount` is a **rate** (percent) or a fixed value, not a split quantity. |
| Owner | Tagged reference to a Customer, Job, Vendor or Employee. | — |
| Billing term (`GncBillTerm`) | Payment terms (due date, early-payment discount). | — |
| Child (bill term / tax table) | Frozen, invisible copy made when an invoice is posted. | Not an account-tree child. |

## 1. Where business objects sit in the file

Inside `<gnc:book>`, after all core content (commodities, prices, accounts, transactions, template transactions,
scheduled transactions, budgets), GnuCash writes every business type in **backend registry order**
(5.17:libgnucash/backend/xml/io-gncxml-v2.cpp:1025-1026). The registry is filled by `business_core_xml_init`
(5.17:libgnucash/backend/xml/gnc-backend-xml.cpp:168-183) in this order: address, billterm, customer, employee, entry,
invoice, job, order, owner, taxtable, vendor. Address and owner register no writer and no count
(5.17:libgnucash/backend/xml/gnc-address-xml-v2.cpp:214-224, 5.17:libgnucash/backend/xml/gnc-owner-xml-v2.cpp:217-227),
so the top-level element order is:

```
gnc:GncBillTerm*  gnc:GncCustomer*  gnc:GncEmployee*  gnc:GncEntry*  gnc:GncInvoice*
gnc:GncJob*  gnc:GncOrder*  gnc:GncTaxTable*  gnc:GncVendor*
</gnc:book>
```

Each top-level element is dumped with `xmlElemDump` followed by `"\n"` (e.g.
5.17:libgnucash/backend/xml/gnc-customer-xml-v2.cpp:462-466); formatting is as described in the core note.

### 1.1 Namespace declarations

The root `<gnc-v2>` element declares, after the core namespaces (`gnc` … `lot`), each registered type's namespaces in
registry order (5.17:libgnucash/backend/xml/io-gncxml-v2.cpp:1291-1293), each as
`\n     xmlns:NAME="http://www.gnucash.org/XML/NAME"` (5.17:libgnucash/backend/xml/io-gncxml-v2.cpp:1251-1256). Observed
(oracle, verbatim):

```
     xmlns:lot="http://www.gnucash.org/XML/lot"
     xmlns:addr="http://www.gnucash.org/XML/addr"
     xmlns:billterm="http://www.gnucash.org/XML/billterm"
     xmlns:bt-days="http://www.gnucash.org/XML/bt-days"
     xmlns:bt-prox="http://www.gnucash.org/XML/bt-prox"
     xmlns:cust="http://www.gnucash.org/XML/cust"
     xmlns:employee="http://www.gnucash.org/XML/employee"
     xmlns:entry="http://www.gnucash.org/XML/entry"
     xmlns:invoice="http://www.gnucash.org/XML/invoice"
     xmlns:job="http://www.gnucash.org/XML/job"
     xmlns:order="http://www.gnucash.org/XML/order"
     xmlns:owner="http://www.gnucash.org/XML/owner"
     xmlns:taxtable="http://www.gnucash.org/XML/taxtable"
     xmlns:tte="http://www.gnucash.org/XML/tte"
     xmlns:vendor="http://www.gnucash.org/XML/vendor">
```

These are written whether or not the book has business objects. Sources: addr
(gnc-address-xml-v2.cpp:208), billterm/bt-days/bt-prox (gnc-bill-term-xml-v2.cpp:731-734), cust
(gnc-customer-xml-v2.cpp:481), employee (gnc-employee-xml-v2.cpp:412), entry (gnc-entry-xml-v2.cpp:797), invoice
(gnc-invoice-xml-v2.cpp:523), job (gnc-job-xml-v2.cpp:324), order (gnc-order-xml-v2.cpp:366), owner
(gnc-owner-xml-v2.cpp:211), taxtable/tte (gnc-tax-table-xml-v2.cpp:678-679), vendor (gnc-vendor-xml-v2.cpp:434), all
under `5.17:libgnucash/backend/xml/`.

### 1.2 Count data

After the core counts (`commodity`, `account`, `transaction`, `schedxaction`, `budget`, `price`;
5.17:libgnucash/backend/xml/io-gncxml-v2.cpp:991-1004) GnuCash writes one count per registered type that has a
`get_count`, in registry order (5.17:libgnucash/backend/xml/io-gncxml-v2.cpp:1007-1008, 955-963), as
`<gnc:count-data cd:type="TYPE">N</gnc:count-data>\n` (5.17:libgnucash/backend/xml/io-gncxml-v2.cpp:913-914).
**A count of 0 is not written** (5.17:libgnucash/backend/xml/io-gncxml-v2.cpp:884). Observed:

```
<gnc:count-data cd:type="gnc:GncBillTerm">3</gnc:count-data>
<gnc:count-data cd:type="gnc:GncCustomer">1</gnc:count-data>
<gnc:count-data cd:type="gnc:GncEmployee">1</gnc:count-data>
<gnc:count-data cd:type="gnc:GncEntry">7</gnc:count-data>
<gnc:count-data cd:type="gnc:GncInvoice">4</gnc:count-data>
<gnc:count-data cd:type="gnc:GncJob">1</gnc:count-data>
<gnc:count-data cd:type="gnc:GncOrder">1</gnc:count-data>
<gnc:count-data cd:type="gnc:GncTaxTable">3</gnc:count-data>
<gnc:count-data cd:type="gnc:GncVendor">1</gnc:count-data>
```

| `cd:type` | What is counted | Same predicate as the writer? |
|---|---|---|
| `gnc:GncBillTerm` | every bill term in the collection (gnc-bill-term-xml-v2.cpp:468-481) | yes (all written) |
| `gnc:GncCustomer` | customers with non-empty ID (gnc-customer-xml-v2.cpp:421-448) | yes |
| `gnc:GncEmployee` | employees with non-empty ID (gnc-employee-xml-v2.cpp:353-379) | yes |
| `gnc:GncEntry` | **every** entry (gnc-entry-xml-v2.cpp:749-762) | **no**: unattached entries are counted but not written (§5.4) |
| `gnc:GncInvoice` | invoices with non-empty ID (gnc-invoice-xml-v2.cpp:463-490) | yes |
| `gnc:GncJob` | jobs with non-empty ID (gnc-job-xml-v2.cpp:265-292) | yes |
| `gnc:GncOrder` | orders with non-empty ID (gnc-order-xml-v2.cpp:308-334) | yes |
| `gnc:GncTaxTable` | every tax table (gnc-tax-table-xml-v2.cpp:435-448) | yes (all written) |
| `gnc:GncVendor` | vendors with non-empty ID (gnc-vendor-xml-v2.cpp:375-401) | yes |

Counts are computed **at save time from the in-memory book**, after the load-time scrubs (§7). Confirmed: a hand-edited
book whose `GncEntry` count was 5 but held 6 attached entries plus one unattached entry was written back with count 7
and 6 `GncEntry` elements (§12, run "main").

The reader uses counts only for progress; an unknown `cd:type` is logged and ignored
(5.17:libgnucash/backend/xml/io-gncxml-v2.cpp:430-450). red-ale's reader must not rely on them.

### 1.3 Iteration order within a type: sorted by GUID

Every business writer uses `qof_object_foreach_sorted` (e.g. 5.17:libgnucash/backend/xml/gnc-customer-xml-v2.cpp:472),
which collects the collection's hash-table values and sorts them with `qof_instance_guid_compare`
(5.17:libgnucash/engine/qofobject.cpp:223-237), which is `memcmp` of the 16 GUID bytes
(5.17:libgnucash/engine/qofinstance.cpp:492-502, 5.17:libgnucash/engine/guid.cpp:243-249). Byte order equals the
order of the 32-char lowercase hex strings, so: **within each type, elements are in ascending GUID-string order.** This
is deterministic and independent of hash order. Observed (bill terms): `175f1be5…`, `872101fc…`, `de8b80dc…`.

Consequence for red-ale: sort each business type by GUID string on write. A GnuCash 5.17-written file is already in
that order, so an unedited round trip is byte-identical; new objects land where GnuCash would put them. (Files from
GnuCash versions that wrote in hash order would be reordered, exactly as GnuCash 5.17 itself reorders them; see Open
questions.)

## 2. Shared value encodings

| Kind | Writer | Text form | Reader |
|---|---|---|---|
| GUID reference | `guid_to_dom_tree` (sixtp-dom-generators.cpp:88-106) | `<TAG type="guid">32 lowercase hex</TAG>` | `dom_tree_to_guid` requires attribute `type="guid"` (or `"new"`) (sixtp-dom-parsers.cpp:47-70) |
| string | `text_to_dom_tree` (sixtp-dom-generators.cpp:45-58) | escaped text; empty string → self-closing `<TAG/>` | whole text content, no trimming |
| integer / boolean | `int_to_dom_tree` (sixtp-dom-generators.cpp:61-71) | `%" G_GINT64_FORMAT "` decimal; booleans are `0`/`1` | `dom_tree_to_integer`: trims ASCII whitespace, `from_chars` (sixtp-utils.cpp:144-154) |
| numeric | `gnc_numeric_to_dom_tree` (sixtp-dom-generators.cpp:169-186) | `num/denom` as stored (not reduced), e.g. `100000/100`, `0/1` | `dom_tree_to_gnc_numeric`; malformed → 0 (sixtp-dom-parsers.cpp:430-438) |
| date-time | `time64_to_dom_tree` (sixtp-dom-generators.cpp:134-146) | `<TAG>\n  <ts:date>YYYY-MM-DD HH:MM:SS +0000</ts:date>\n</TAG>`; **returns NULL (element omitted) for `INT64_MAX`** | invalid year → 0 (e.g. gnc-invoice-xml-v2.cpp:171-179) |
| commodity ref | `commodity_ref_to_dom_tree` (sixtp-dom-generators.cpp:109-131) | `<TAG>` + `<cmdty:space>` + `<cmdty:id>`; **omitted if commodity is NULL** | `dom_tree_to_commodity_ref` |
| enum | per type, §8 | upper-case ASCII word | unknown word → warning, field keeps its default (§8) |
| slots | `qof_instance_slots_to_dom_tree` (sixtp-dom-generators.cpp:328-338) | `<PREFIX:slots>` frame, **omitted when the KVP frame is empty** | see core note |

All file paths in this table are under `5.17:libgnucash/backend/xml/`. Every top-level business element and the
`addr`/`owner` sub-elements carry `version="2.0.0"`; no business parser reads the attribute.

### 2.1 Address (`addr:`), used by customer, vendor, employee

`gnc_address_to_dom_tree` (5.17:libgnucash/backend/xml/gnc-address-xml-v2.cpp:67-90). The wrapper element (e.g.
`cust:addr`) is **always** written with `version="2.0.0"`; children only when non-empty, in this order:

| # | element | when |
|---|---|---|
| 1 | `addr:name` | non-empty |
| 2-5 | `addr:addr1` … `addr:addr4` | each non-empty |
| 6 | `addr:phone` | non-empty |
| 7 | `addr:fax` | non-empty |
| 8 | `addr:email` | non-empty |
| 9 | `addr:slots` | address KVP non-empty |

An empty address is `<cust:addr version="2.0.0"/>`. All children optional on read
(5.17:libgnucash/backend/xml/gnc-address-xml-v2.cpp:171-183). An Employee's display name is its address name (the
Python `Employee(…, name)` constructor wrote `<addr:name>Jane Doe</addr:name>`, §12). An address with slots sets the
book feature "Extra data in addresses, jobs or invoice entries" on commit (5.17:libgnucash/engine/gncAddress.c:493-497).

### 2.2 Owner (`owner:`), used by job, order, invoice owner/billto, entry billto

`gnc_owner_to_dom_tree` (5.17:libgnucash/backend/xml/gnc-owner-xml-v2.cpp:56-89):

```
<invoice:owner version="2.0.0">
  <owner:type>gncCustomer</owner:type>
  <owner:id type="guid">105d5ff6f4fb4591b5a5068e2f4fb7ee</owner:id>
</invoice:owner>
```

`owner:type` is the QOF type name: `gncCustomer`, `gncJob`, `gncVendor`, `gncEmployee`
(5.17:libgnucash/engine/gncCustomer.h:69, gncJob.h:44, gncVendor.h:46, gncEmployee.h:44). For owner type NONE or
UNDEFINED the function returns NULL with a warning and **the element is omitted** — which can make the containing
object unreadable (§5.5). Both children required on read
(5.17:libgnucash/backend/xml/gnc-owner-xml-v2.cpp:180-185). `owner:id` is resolved using the type set by `owner:type`, so
**type must precede id**; a referenced object not yet loaded is created as a stub with that GUID
(5.17:libgnucash/backend/xml/gnc-owner-xml-v2.cpp:118-178). An unknown type string leaves the owner uninitialised
(undefined behaviour in GnuCash): red-ale must reject it.

## 3. Reader semantics common to all business objects

`dom_tree_generic_parse` (5.17:libgnucash/backend/xml/sixtp-dom-parsers.cpp:710-740) walks child elements and
dispatches by exact tag name:

1. **Unknown child element** → `gnc_xml_set_data` fails (sixtp-dom-parsers.cpp:686-708) → the parse is unsuccessful.
2. **Missing required child** → unsuccessful (sixtp-dom-parsers.cpp:669-684).
3. **A handler's return value is ignored** (sixtp-dom-parsers.cpp:694-696): a bad GUID, a dangling account reference,
   an unknown enum word, an unresolvable commodity only log an error and leave the field at its default.
4. Child order is not checked, except where one handler depends on another (owner type before id; §5.6 bill term
   `refcount` before `parent`).

For a **top-level** business element an unsuccessful parse destroys the object and the end handler returns FALSE
(e.g. 5.17:libgnucash/backend/xml/gnc-customer-xml-v2.cpp:366-375, 404-412), which clears `parsing_ok`
(5.17:libgnucash/backend/xml/sixtp.cpp:537-541): **the whole file fails to load** ("Syntax error in Xml File",
5.17:libgnucash/backend/xml/gnc-backend-xml.cpp:245-250). Confirmed: one unknown `<vendor:frobnicate>` or a missing
`<vendor:active>` makes GnuCash refuse the book (§12, runs "unknown", "missing").

For a **nested** structure (address, owner, `billterm:days`/`billterm:proximo`, a `gnc:GncTaxTableEntry`) the nested
parse fails but rule 3 hides it: GnuCash logs an error and **silently drops** the unknown element. Confirmed: an
injected `<addr:zip>` inside a vendor address was dropped on resave (§12, run "nested"). Tax table entries are worse:
the `taxtable:entries` handler stops at the first bad child, so later entries are lost
(5.17:libgnucash/backend/xml/gnc-tax-table-xml-v2.cpp:317-345).

Unknown enum words fall back silently: `<vendor:taxincluded>MAYBE</…>` was rewritten as `USEGLOBAL`, `<entry:i-disc-how>
LATER</…>` as `PRETAX` (§12, run "badenum").

**Forward references.** References are resolved by GUID at parse time; a target not yet parsed is created as an empty
stub and filled when its own element arrives (the object's own `guid` handler adopts an existing instance, e.g.
5.17:libgnucash/backend/xml/gnc-customer-xml-v2.cpp:171-192). Because of the write order, forward references are
normal: entries → invoices/orders; customers/vendors → tax tables; invoices → jobs/vendors; jobs → vendors. Account,
transaction and lot references always point backwards (core content is earlier). red-ale must resolve references after
the whole book is parsed.

**Legacy aliases accepted on read, never written:** `cust:commodity`, `vendor:commodity`, `employee:commodity`,
`invoice:commodity` (same as `…:currency`); `entry:acct`, `entry:price`, `entry:discount`, `entry:disc-type`,
`entry:disc-how`, `entry:taxable`, `entry:taxincluded`, `entry:taxtable` (pre-2.0 single-sided entries;
5.17:libgnucash/backend/xml/gnc-entry-xml-v2.cpp:589-613, 661-669, 686-692). v0.1 targets 5.17-written books; red-ale
may reject these or map them, but never writes them.

## 4. Business objects that are not written

| Object | Skipped when | Source |
|---|---|---|
| Customer, Employee, Vendor, Job, Order, Invoice | ID is NULL or `""` | `*_should_be_saved`, e.g. 5.17:libgnucash/backend/xml/gnc-customer-xml-v2.cpp:421-432 |
| Entry | attached to no order, no invoice and no bill | 5.17:libgnucash/backend/xml/gnc-entry-xml-v2.cpp:774-777 |
| Bill term, Tax table | never skipped (refcount 0 and invisible ones are written) | gnc-bill-term-xml-v2.cpp:483-498, gnc-tax-table-xml-v2.cpp:450-465 |

Inactive objects are written. Unposted invoices are written. Confirmed: a customer with `<cust:id></cust:id>` was read
and silently dropped on resave; an orphan entry was dropped but still counted (§12, run "main").

## 5. Per-object element tables

Columns: **#** = write order; **W** = when written (A = always); **R** = required on read (✓) or optional. All
elements in the "B" namespace column belong to that object's namespace.

### 5.1 `gnc:GncBillTerm` (`billterm:`, `bt-days:`, `bt-prox:`)

Writer 5.17:libgnucash/backend/xml/gnc-bill-term-xml-v2.cpp:76-130; reader table :383-396.

| # | element | W | value | R |
|---|---|---|---|---|
| 1 | `billterm:guid` | A | GUID | ✓ |
| 2 | `billterm:name` | A | string | ✓ |
| 3 | `billterm:desc` | A | string (empty → `<billterm:desc/>`) | ✓ |
| 4 | `billterm:refcount` | A | integer (§7) | ✓ |
| 5 | `billterm:invisible` | A | `0`/`1` | ✓ |
| 6 | `billterm:slots` | KVP non-empty | slots | |
| 7 | `billterm:child` | child set and child ≠ self | GUID of bill term | |
| 8 | `billterm:parent` | parent set | GUID of bill term | |
| 9 | `billterm:days` **or** `billterm:proximo` | type DAYS (1) / PROXIMO (2); **neither** if type unset (0) | sub-element, below | |

Note: slots come **before** child/parent here, unlike tax tables.

`billterm:days` children, each written only if non-zero (`maybe_add_int`/`maybe_add_numeric`,
5.17:libgnucash/backend/xml/xml-helpers.h:26-38): `bt-days:due-days` (int), `bt-days:disc-days` (int),
`bt-days:discount` (numeric, a percentage). All zero → `<billterm:days/>`.

`billterm:proximo` children, each only if non-zero: `bt-prox:due-day`, `bt-prox:disc-day`, `bt-prox:discount`,
`bt-prox:cutoff-day`. All optional on read (:181-187, :232-239).

Reader details: `invisible` non-zero → `MakeInvisible` (:326-336); `parent` → `gncBillTermSetParent`, which **resets
refcount to 0 and makes the term invisible** (5.17:libgnucash/engine/gncBillTerm.c:393-407). The days/proximo handler
refuses a second type element (:357-358, :369-370; ignored per §3 rule 3).

Observed (parent, its posted-invoice child, and a proximo term; verbatim):

```
<gnc:GncBillTerm version="2.0.0">
  <billterm:guid type="guid">175f1be5e2634254873666450f768157</billterm:guid>
  <billterm:name>Net30</billterm:name>
  <billterm:desc>Net 30, 2% in 10</billterm:desc>
  <billterm:refcount>0</billterm:refcount>
  <billterm:invisible>1</billterm:invisible>
  <billterm:parent type="guid">de8b80dcba334018a8630409838a4975</billterm:parent>
  <billterm:days>
    <bt-days:due-days>30</bt-days:due-days>
    <bt-days:disc-days>10</bt-days:disc-days>
    <bt-days:discount>2/1</bt-days:discount>
  </billterm:days>
</gnc:GncBillTerm>
<gnc:GncBillTerm version="2.0.0">
  <billterm:guid type="guid">872101fc4ffa4d8596e471842238e65c</billterm:guid>
  <billterm:name>Prox15</billterm:name>
  <billterm:desc>15th of next month</billterm:desc>
  <billterm:refcount>1</billterm:refcount>
  <billterm:invisible>0</billterm:invisible>
  <billterm:proximo>
    <bt-prox:due-day>15</bt-prox:due-day>
    <bt-prox:disc-day>5</bt-prox:disc-day>
    <bt-prox:discount>3/2</bt-prox:discount>
    <bt-prox:cutoff-day>25</bt-prox:cutoff-day>
  </billterm:proximo>
</gnc:GncBillTerm>
<gnc:GncBillTerm version="2.0.0">
  <billterm:guid type="guid">de8b80dcba334018a8630409838a4975</billterm:guid>
  <billterm:name>Net30</billterm:name>
  <billterm:desc>Net 30, 2% in 10</billterm:desc>
  <billterm:refcount>1</billterm:refcount>
  <billterm:invisible>0</billterm:invisible>
  <billterm:child type="guid">175f1be5e2634254873666450f768157</billterm:child>
  <billterm:days>
    <bt-days:due-days>30</bt-days:due-days>
    <bt-days:disc-days>10</bt-days:disc-days>
    <bt-days:discount>2/1</bt-days:discount>
  </billterm:days>
</gnc:GncBillTerm>
```

Edge forms (verbatim): type unset → no type element; days with all zeros → `<billterm:days/>`:

```
<gnc:GncBillTerm version="2.0.0">
  <billterm:guid type="guid">ecc817d524e54e3e9ce31d97089b93aa</billterm:guid>
  <billterm:name>Zero</billterm:name>
  <billterm:desc/>
  <billterm:refcount>0</billterm:refcount>
  <billterm:invisible>0</billterm:invisible>
  <billterm:days/>
</gnc:GncBillTerm>
```

### 5.2 `gnc:GncCustomer` (`cust:`)

Writer 5.17:libgnucash/backend/xml/gnc-customer-xml-v2.cpp:75-138; reader table :332-351.

| # | element | W | value | R |
|---|---|---|---|---|
| 1 | `cust:guid` | A | GUID | ✓ |
| 2 | `cust:name` | A | string | ✓ |
| 3 | `cust:id` | A | string (object skipped if empty) | ✓ |
| 4 | `cust:addr` | A | address §2.1 | ✓ |
| 5 | `cust:shipaddr` | A | address §2.1 | ✓ |
| 6 | `cust:notes` | non-empty | string | |
| 7 | `cust:terms` | terms set | GUID of bill term | |
| 8 | `cust:taxincluded` | A | `YES`/`NO`/`USEGLOBAL` | ✓ |
| 9 | `cust:active` | A | `0`/`1` | ✓ |
| 10 | `cust:discount` | A | numeric (percent) | ✓ |
| 11 | `cust:credit` | A | numeric (credit limit) | ✓ |
| 12 | `cust:currency` | currency non-NULL | commodity ref | (alias `cust:commodity`) |
| 13 | `cust:use-tt` | A | `0`/`1` (tax-table override) | |
| 14 | `cust:taxtable` | tax table set | GUID of tax table | |
| 15 | `cust:slots` | KVP non-empty | slots | |

Defaults on create: taxincluded USEGLOBAL, active TRUE, discount and credit `0/1`, empty strings
(5.17:libgnucash/engine/gncCustomer.c:303-331). Observed (verbatim):

```
<gnc:GncCustomer version="2.0.0">
  <cust:guid type="guid">105d5ff6f4fb4591b5a5068e2f4fb7ee</cust:guid>
  <cust:name>Acme &amp; Sons &lt;Ltd&gt;</cust:name>
  <cust:id>000001</cust:id>
  <cust:addr version="2.0.0">
    <addr:name>Acme Billing</addr:name>
    <addr:addr1>1 Main St</addr:addr1>
    <addr:addr2>Suite 2</addr:addr2>
    <addr:phone>555-1234</addr:phone>
    <addr:email>ap@acme.example</addr:email>
  </cust:addr>
  <cust:shipaddr version="2.0.0">
    <addr:name>Acme Warehouse</addr:name>
    <addr:addr1>9 Dock Rd</addr:addr1>
  </cust:shipaddr>
  <cust:notes>Good payer</cust:notes>
  <cust:terms type="guid">de8b80dcba334018a8630409838a4975</cust:terms>
  <cust:taxincluded>NO</cust:taxincluded>
  <cust:active>1</cust:active>
  <cust:discount>5/1</cust:discount>
  <cust:credit>100000/100</cust:credit>
  <cust:currency>
    <cmdty:space>CURRENCY</cmdty:space>
    <cmdty:id>EUR</cmdty:id>
  </cust:currency>
  <cust:use-tt>1</cust:use-tt>
  <cust:taxtable type="guid">086c1560a7ff4f64b5154ddcd7215a13</cust:taxtable>
</gnc:GncCustomer>
```

Minimal form (verbatim): `<cust:addr version="2.0.0"/>`, `<cust:shipaddr version="2.0.0"/>`,
`<cust:taxincluded>USEGLOBAL</cust:taxincluded>`, `<cust:discount>0/1</cust:discount>`, `<cust:credit>0/1</cust:credit>`,
`<cust:use-tt>0</cust:use-tt>`.

### 5.3 `gnc:GncEmployee` (`employee:`)

Writer 5.17:libgnucash/backend/xml/gnc-employee-xml-v2.cpp:73-122; reader table :267-282.

| # | element | W | value | R |
|---|---|---|---|---|
| 1 | `employee:guid` | A | GUID | ✓ |
| 2 | `employee:username` | A | string (empty → `<employee:username/>`) | ✓ |
| 3 | `employee:id` | A | string (object skipped if empty) | ✓ |
| 4 | `employee:addr` | A | address §2.1 (name = employee name) | ✓ |
| 5 | `employee:language` | non-empty | string | |
| 6 | `employee:acl` | non-empty | string | |
| 7 | `employee:active` | A | `0`/`1` | ✓ |
| 8 | `employee:workday` | A | numeric (hours/day) | ✓ |
| 9 | `employee:rate` | A | numeric (hourly rate) | ✓ |
| 10 | `employee:currency` | currency non-NULL | commodity ref | (alias `employee:commodity`) |
| 11 | `employee:ccard` | credit-card account set | GUID of account | |
| 12 | `employee:slots` | KVP non-empty | slots | |

Defaults: workday and rate `0/1`, active TRUE (5.17:libgnucash/engine/gncEmployee.c:428-453). Observed (verbatim):

```
<gnc:GncEmployee version="2.0.0">
  <employee:guid type="guid">35ebce93436148359007e720c71333a6</employee:guid>
  <employee:username>jdoe</employee:username>
  <employee:id>000001</employee:id>
  <employee:addr version="2.0.0">
    <addr:name>Jane Doe</addr:name>
  </employee:addr>
  <employee:language>en_GB</employee:language>
  <employee:acl>all</employee:acl>
  <employee:active>1</employee:active>
  <employee:workday>8/1</employee:workday>
  <employee:rate>4250/100</employee:rate>
  <employee:currency>
    <cmdty:space>CURRENCY</cmdty:space>
    <cmdty:id>EUR</cmdty:id>
  </employee:currency>
  <employee:ccard type="guid">fb828f153cdf4d2e95e1f04236882533</employee:ccard>
</gnc:GncEmployee>
```

### 5.4 `gnc:GncEntry` (`entry:`)

Writer 5.17:libgnucash/backend/xml/gnc-entry-xml-v2.cpp:107-216; reader table :623-671. An entry has an
invoice-side ("i-") group used when it is on a customer invoice/credit note, and a bill-side ("b-") group used when it is
on a vendor bill or employee voucher. **An entry can be on an order, an invoice and a bill at the same time** (a
billable bill line added to a customer invoice keeps its `entry:bill`;
5.17:gnucash/register/ledger-core/gncEntryLedgerModel.c:1158-1170), so both groups and both document references can
appear.

| # | element | W | value | R |
|---|---|---|---|---|
| 1 | `entry:guid` | A | GUID | ✓ |
| 2 | `entry:date` | A | date-time | ✓ |
| 3 | `entry:entered` | A | date-time | ✓ |
| 4 | `entry:description` | non-empty | string | |
| 5 | `entry:action` | non-empty | string | |
| 6 | `entry:notes` | non-empty | string | |
| 7 | `entry:qty` | non-zero | numeric | |
| 8 | `entry:i-acct` | invoice account set | GUID of account | |
| 9 | `entry:i-price` | non-zero | numeric | |
| 10 | `entry:i-discount` | non-zero | numeric | |
| 11 | `entry:invoice` | on an invoice | GUID of invoice | |
| 12 | `entry:i-disc-type` | on an invoice | `PERCENT`/`VALUE` | |
| 13 | `entry:i-disc-how` | on an invoice | `PRETAX`/`SAMETIME`/`POSTTAX` | |
| 14 | `entry:i-taxable` | on an invoice | `0`/`1` | |
| 15 | `entry:i-taxincluded` | on an invoice | `0`/`1` | |
| 16 | `entry:i-taxtable` | invoice tax table set | GUID of tax table | |
| 17 | `entry:b-acct` | bill account set | GUID of account | |
| 18 | `entry:b-price` | non-zero | numeric | |
| 19 | `entry:bill` | on a bill/voucher | GUID of invoice | |
| 20 | `entry:billable` | on a bill/voucher | `0`/`1` | |
| 21 | `entry:billto` | on a bill/voucher **and** bill-to owner points at an object | owner §2.2 | |
| 22 | `entry:b-taxable` | on a bill/voucher | `0`/`1` | |
| 23 | `entry:b-taxincluded` | on a bill/voucher | `0`/`1` | |
| 24 | `entry:b-pay` | on a bill/voucher (always, since the string is never empty) | `CASH`/`CARD` | |
| 25 | `entry:b-taxtable` | bill tax table set | GUID of tax table | |
| 26 | `entry:order` | on an order | GUID of order | |
| 27 | `entry:slots` | KVP non-empty | slots | |

Note `entry:invoice` sits between `i-discount` and `i-disc-type`, and `entry:bill` between `b-price` and `billable`.

Defaults (5.17:libgnucash/engine/gncEntry.c:413-444): quantity, prices, discount 0; i-taxable and b-taxable TRUE;
i-disc-type PERCENT; i-disc-how PRETAX; b-payment CASH; bill-to type CUSTOMER with no object. Posting a vendor bill
overwrites `i-price` of each **billable** line with its net price and clears `i-taxincluded`
(5.17:libgnucash/engine/gncInvoice.c:1552-1559), so a billable bill line carries an `entry:i-price` though it is on no
invoice (observed `<entry:i-price>1742/100</entry:i-price>` for b-price `1999/100` with tax included).

The `entry:invoice`, `entry:bill` and `entry:order` handlers add the entry to the document
(5.17:libgnucash/backend/xml/gnc-entry-xml-v2.cpp:520-587); documents do not list their entries in XML. The document's
entry list order is not in the file (it is sorted in memory by `gncEntryCompare`).

Dates: the GUI stores `entry:date` as the "canonical day time" (10:59:00 UTC) of the chosen day
(5.17:libgnucash/engine/gncEntry.c:504-515); the Python binding used in §12 stored local midnight instead.

Observed, customer invoice line with tax table (verbatim):

```
<gnc:GncEntry version="2.0.0">
  <entry:guid type="guid">91fa04db1e3a4b2ab40ffec4867a3b46</entry:guid>
  <entry:date>
    <ts:date>2024-03-02 00:00:00 +0000</ts:date>
  </entry:date>
  <entry:entered>
    <ts:date>2024-03-02 10:00:00 +0000</ts:date>
  </entry:entered>
  <entry:description>Consulting</entry:description>
  <entry:action>Hours</entry:action>
  <entry:notes>On site</entry:notes>
  <entry:qty>10/1</entry:qty>
  <entry:i-acct type="guid">9e10c866d5b94848a548250b633d8430</entry:i-acct>
  <entry:i-price>12050/100</entry:i-price>
  <entry:invoice type="guid">7aea09e615114d81906fe8bd04f28eea</entry:invoice>
  <entry:i-disc-type>PERCENT</entry:i-disc-type>
  <entry:i-disc-how>PRETAX</entry:i-disc-how>
  <entry:i-taxable>1</entry:i-taxable>
  <entry:i-taxincluded>0</entry:i-taxincluded>
  <entry:i-taxtable type="guid">294aa8370aee4f28af8b1cb103c53d6e</entry:i-taxtable>
</gnc:GncEntry>
```

Invoice line with a value discount after tax (excerpt): `<entry:i-discount>500/100</entry:i-discount>`,
`<entry:i-disc-type>VALUE</entry:i-disc-type>`, `<entry:i-disc-how>POSTTAX</entry:i-disc-how>`,
`<entry:i-taxable>0</entry:i-taxable>`.

Vendor bill line, billable to a customer, tax included (verbatim, second run of `make_book.py`):

```
  <entry:description>Printer paper</entry:description>
  <entry:qty>3/1</entry:qty>
  <entry:i-price>1742/100</entry:i-price>
  <entry:b-acct type="guid">724cb54702e0471bad717afcc102da3c</entry:b-acct>
  <entry:b-price>1999/100</entry:b-price>
  <entry:bill type="guid">fc2cd829ac2442269c964a2ee27244cc</entry:bill>
  <entry:billable>1</entry:billable>
  <entry:billto version="2.0.0">
    <owner:type>gncCustomer</owner:type>
    <owner:id type="guid">b8e36e1fda8a4ffbb3ab1703daaa2460</owner:id>
  </entry:billto>
  <entry:b-taxable>1</entry:b-taxable>
  <entry:b-taxincluded>1</entry:b-taxincluded>
  <entry:b-pay>CASH</entry:b-pay>
  <entry:b-taxtable type="guid">d1f98324f54f48c98e68c5d35e8005eb</entry:b-taxtable>
</gnc:GncEntry>
```

Employee voucher line paid by company card (verbatim excerpt):

```
  <entry:description>Taxi</entry:description>
  <entry:qty>1/1</entry:qty>
  <entry:b-acct type="guid">724cb54702e0471bad717afcc102da3c</entry:b-acct>
  <entry:b-price>3275/100</entry:b-price>
  <entry:bill type="guid">6193d3b14af34c25b389fc13ac14d277</entry:bill>
  <entry:billable>0</entry:billable>
  <entry:b-taxable>1</entry:b-taxable>
  <entry:b-taxincluded>0</entry:b-taxincluded>
  <entry:b-pay>CARD</entry:b-pay>
</gnc:GncEntry>
```

Order line (hand-written, read and rewritten unchanged by the oracle):

```
<gnc:GncEntry version="2.0.0">
  <entry:guid type="guid">00000000000000000000000000000e01</entry:guid>
  <entry:date>
    <ts:date>2024-03-01 00:00:00 +0000</ts:date>
  </entry:date>
  <entry:entered>
    <ts:date>2024-03-01 00:00:00 +0000</ts:date>
  </entry:entered>
  <entry:description>Ordered widget</entry:description>
  <entry:qty>2/1</entry:qty>
  <entry:order type="guid">00000000000000000000000000000a01</entry:order>
</gnc:GncEntry>
```

### 5.5 `gnc:GncInvoice` (`invoice:`) — invoices, bills, vouchers, credit notes

Writer 5.17:libgnucash/backend/xml/gnc-invoice-xml-v2.cpp:87-160; reader table :374-394.

| # | element | W | value | R |
|---|---|---|---|---|
| 1 | `invoice:guid` | A | GUID | ✓ |
| 2 | `invoice:id` | A | string (object skipped if empty) | ✓ |
| 3 | `invoice:owner` | owner type valid | owner §2.2 (customer, job, vendor or employee) | ✓ |
| 4 | `invoice:opened` | date ≠ `INT64_MAX` | date-time | ✓ |
| 5 | `invoice:posted` | date ≠ `INT64_MAX` (posted) | date-time | |
| 6 | `invoice:terms` | terms set | GUID of bill term (the **child** once posted) | |
| 7 | `invoice:billing_id` | non-empty | string | |
| 8 | `invoice:notes` | non-empty | string | |
| 9 | `invoice:active` | A | `0`/`1` | ✓ |
| 10 | `invoice:posttxn` | posted | GUID of transaction | |
| 11 | `invoice:postlot` | posted | GUID of lot | |
| 12 | `invoice:postacc` | posted | GUID of account (A/R or A/P) | |
| 13 | `invoice:currency` | currency non-NULL | commodity ref | (alias `invoice:commodity`) |
| 14 | `invoice:billto` | bill-to owner points at an object | owner §2.2 | |
| 15 | `invoice:charge-amt` | non-zero | numeric ("to charge" amount, vouchers) | |
| 16 | `invoice:slots` | KVP non-empty | slots | |

Kind of document = end owner type (job → its owner) × `credit-note` slot
(5.17:libgnucash/engine/gncInvoice.c:1065-1087). New invoices have `date_opened = date_posted = INT64_MAX`
(5.17:libgnucash/engine/gncInvoice.c:135-139), `active` TRUE, bill-to type CUSTOMER with no object
(:313-334). **GnuCash can write an invoice it cannot read back**: with no owner and no opened date both required
elements are omitted and the next load fails (§12, run "small"). red-ale must make owner and date-opened invariants of
the Invoice aggregate.

Observed, posted customer invoice with terms (verbatim):

```
<gnc:GncInvoice version="2.0.0">
  <invoice:guid type="guid">7aea09e615114d81906fe8bd04f28eea</invoice:guid>
  <invoice:id>000001</invoice:id>
  <invoice:owner version="2.0.0">
    <owner:type>gncCustomer</owner:type>
    <owner:id type="guid">105d5ff6f4fb4591b5a5068e2f4fb7ee</owner:id>
  </invoice:owner>
  <invoice:opened>
    <ts:date>2024-03-01 00:00:00 +0000</ts:date>
  </invoice:opened>
  <invoice:posted>
    <ts:date>2024-03-05 10:59:00 +0000</ts:date>
  </invoice:posted>
  <invoice:terms type="guid">175f1be5e2634254873666450f768157</invoice:terms>
  <invoice:billing_id>CUST-REF-9</invoice:billing_id>
  <invoice:notes>First invoice</invoice:notes>
  <invoice:active>1</invoice:active>
  <invoice:posttxn type="guid">4e5f94c687f94557807a54b436fec4e5</invoice:posttxn>
  <invoice:postlot type="guid">84b68a37eaeb4decb65b9e1171fd2929</invoice:postlot>
  <invoice:postacc type="guid">a1b23d36bccb4b36aff609c3b0b97890</invoice:postacc>
  <invoice:currency>
    <cmdty:space>CURRENCY</cmdty:space>
    <cmdty:id>EUR</cmdty:id>
  </invoice:currency>
</gnc:GncInvoice>
```

Unposted credit note owned by a job (verbatim):

```
<gnc:GncInvoice version="2.0.0">
  <invoice:guid type="guid">ef589bd888ad466ebfb36ce5d55aa023</invoice:guid>
  <invoice:id>000002</invoice:id>
  <invoice:owner version="2.0.0">
    <owner:type>gncJob</owner:type>
    <owner:id type="guid">16640158c2e84cb9a2cc0ce1d98184f6</owner:id>
  </invoice:owner>
  <invoice:opened>
    <ts:date>2024-03-01 00:00:00 +0000</ts:date>
  </invoice:opened>
  <invoice:active>1</invoice:active>
  <invoice:currency>
    <cmdty:space>CURRENCY</cmdty:space>
    <cmdty:id>EUR</cmdty:id>
  </invoice:currency>
  <invoice:slots>
    <slot>
      <slot:key>credit-note</slot:key>
      <slot:value type="integer">1</slot:value>
    </slot>
  </invoice:slots>
</gnc:GncInvoice>
```

Bill with a bill-to customer, and a voucher with a charge amount (verbatim excerpts):

```
  <invoice:billto version="2.0.0">
    <owner:type>gncCustomer</owner:type>
    <owner:id type="guid">53349c1a893f4405a78fae5b55c26849</owner:id>
  </invoice:billto>
```
```
  <invoice:charge-amt>500/100</invoice:charge-amt>
```

### 5.6 `gnc:GncJob` (`job:`)

Writer 5.17:libgnucash/backend/xml/gnc-job-xml-v2.cpp:63-93; reader table :186-196.

| # | element | W | value | R |
|---|---|---|---|---|
| 1 | `job:guid` | A | GUID | ✓ |
| 2 | `job:id` | A | string (object skipped if empty) | ✓ |
| 3 | `job:name` | A | string | ✓ |
| 4 | `job:reference` | non-empty | string | |
| 5 | `job:owner` | owner type valid | owner §2.2 (customer or vendor) | ✓ |
| 6 | `job:active` | A | `0`/`1` | ✓ |
| 7 | `job:slots` | KVP non-empty | slots (`job-rate`, §9.1) | |

Setting a non-zero rate creates the `job-rate` slot (5.17:libgnucash/engine/gncJob.c:315-335), and a job with any slot
sets the book feature "Extra data in addresses, jobs or invoice entries" on commit
(5.17:libgnucash/engine/gncJob.c:428-431). Observed (verbatim):

```
<gnc:GncJob version="2.0.0">
  <job:guid type="guid">16640158c2e84cb9a2cc0ce1d98184f6</job:guid>
  <job:id>JOB-1</job:id>
  <job:name>Website rebuild</job:name>
  <job:reference>PO-77</job:reference>
  <job:owner version="2.0.0">
    <owner:type>gncCustomer</owner:type>
    <owner:id type="guid">105d5ff6f4fb4591b5a5068e2f4fb7ee</owner:id>
  </job:owner>
  <job:active>1</job:active>
  <job:slots>
    <slot>
      <slot:key>job-rate</slot:key>
      <slot:value type="numeric">9000/100</slot:value>
    </slot>
  </job:slots>
</gnc:GncJob>
```

### 5.7 `gnc:GncOrder` (`order:`)

Writer 5.17:libgnucash/backend/xml/gnc-order-xml-v2.cpp:71-107; reader table :226-236. Orders have no GUI in 5.17
and are not exposed in the Python bindings; the excerpt below was hand-written and survived an oracle load/save
unchanged (§12, run "main").

| # | element | W | value | R |
|---|---|---|---|---|
| 1 | `order:guid` | A | GUID | ✓ |
| 2 | `order:id` | A | string (object skipped if empty) | ✓ |
| 3 | `order:owner` | owner type valid | owner §2.2 | ✓ |
| 4 | `order:opened` | date ≠ `INT64_MAX` | date-time | ✓ |
| 5 | `order:closed` | date ≠ `INT64_MAX` (default `INT64_MAX`, 5.17:libgnucash/engine/gncOrder.c:101) | date-time | |
| 6 | `order:notes` | non-empty | string | |
| 7 | `order:reference` | non-empty | string | |
| 8 | `order:active` | A | `0`/`1` | ✓ |
| 9 | `order:slots` | KVP non-empty | slots | |

```
<gnc:GncOrder version="2.0.0">
  <order:guid type="guid">00000000000000000000000000000a01</order:guid>
  <order:id>ORD-1</order:id>
  <order:owner version="2.0.0">
    <owner:type>gncCustomer</owner:type>
    <owner:id type="guid">b8e36e1fda8a4ffbb3ab1703daaa2460</owner:id>
  </order:owner>
  <order:opened>
    <ts:date>2024-03-01 00:00:00 +0000</ts:date>
  </order:opened>
  <order:notes>rush</order:notes>
  <order:reference>REF-9</order:reference>
  <order:active>1</order:active>
</gnc:GncOrder>
```

### 5.8 `gnc:GncTaxTable` (`taxtable:`, `tte:`)

Writer 5.17:libgnucash/backend/xml/gnc-tax-table-xml-v2.cpp:76-135; reader tables :182-188 (entry) and :355-366.

| # | element | W | value | R |
|---|---|---|---|---|
| 1 | `taxtable:guid` | A | GUID | ✓ |
| 2 | `taxtable:name` | A | string | ✓ |
| 3 | `taxtable:refcount` | A | integer (§7) | ✓ |
| 4 | `taxtable:invisible` | A | `0`/`1` | ✓ |
| 5 | `taxtable:child` | child set and child ≠ self | GUID of tax table | |
| 6 | `taxtable:parent` | parent set | GUID of tax table | |
| 7 | `taxtable:entries` | A (`<taxtable:entries/>` when empty) | list of `gnc:GncTaxTableEntry` | ✓ |
| 8 | `taxtable:slots` | KVP non-empty | slots | |

`gnc:GncTaxTableEntry` has **no `version` attribute** and children in this order: `tte:acct` (GUID of account, only if
set; optional on read), `tte:amount` (numeric; ✓), `tte:type` (`PERCENT`/`VALUE`; ✓). For PERCENT, amount is the
percentage (10% is `10/1`).

Entry order in the file is the in-memory list order, which `gncTaxTableAddEntry` keeps sorted by
`gncTaxTableEntryCompare`: account full name (`g_strcmp0`), then amount (5.17:libgnucash/engine/gncTaxTable.c:594-608,
821-840). The reader re-adds entries through the same function, so **GnuCash re-sorts on load**. Renaming an account
does not re-sort the in-memory list, so GnuCash's own round trip after a rename can reorder entries.

Reader details: `parent`/`child` pointing at self are ignored (:230-235); `parent` → `gncTaxTableSetParent`, which
resets refcount to 0 and always makes the table invisible (5.17:libgnucash/engine/gncTaxTable.c:492-505). An empty
`<taxtable:entries/>` makes the handler return FALSE (logged critical, ignored per §3) and the table is then **deleted
by the load scrub** (§7).

Observed, parent and posted child (verbatim):

```
<gnc:GncTaxTable version="2.0.0">
  <taxtable:guid type="guid">086c1560a7ff4f64b5154ddcd7215a13</taxtable:guid>
  <taxtable:name>GST+Levy</taxtable:name>
  <taxtable:refcount>1</taxtable:refcount>
  <taxtable:invisible>0</taxtable:invisible>
  <taxtable:child type="guid">294aa8370aee4f28af8b1cb103c53d6e</taxtable:child>
  <taxtable:entries>
    <gnc:GncTaxTableEntry>
      <tte:acct type="guid">6aa8ca9fa2f84f27980373e5cdeb6738</tte:acct>
      <tte:amount>250/100</tte:amount>
      <tte:type>VALUE</tte:type>
    </gnc:GncTaxTableEntry>
    <gnc:GncTaxTableEntry>
      <tte:acct type="guid">3d05441d001e4e68a0659b091bb58be5</tte:acct>
      <tte:amount>10/1</tte:amount>
      <tte:type>PERCENT</tte:type>
    </gnc:GncTaxTableEntry>
  </taxtable:entries>
</gnc:GncTaxTable>
<gnc:GncTaxTable version="2.0.0">
  <taxtable:guid type="guid">294aa8370aee4f28af8b1cb103c53d6e</taxtable:guid>
  <taxtable:name>GST+Levy</taxtable:name>
  <taxtable:refcount>0</taxtable:refcount>
  <taxtable:invisible>1</taxtable:invisible>
  <taxtable:parent type="guid">086c1560a7ff4f64b5154ddcd7215a13</taxtable:parent>
  <taxtable:entries>
    …same two entries…
  </taxtable:entries>
</gnc:GncTaxTable>
```

(The first entry is "Liabilities:Levy", the second "Liabilities:Tax": sorted by full account name.)

### 5.9 `gnc:GncVendor` (`vendor:`)

Writer 5.17:libgnucash/backend/xml/gnc-vendor-xml-v2.cpp:71-123; reader table :289-303. Same shape as customer without
`shipaddr`, `discount`, `credit`:

| # | element | W | value | R |
|---|---|---|---|---|
| 1 | `vendor:guid` | A | GUID | ✓ |
| 2 | `vendor:name` | A | string | ✓ |
| 3 | `vendor:id` | A | string (object skipped if empty) | ✓ |
| 4 | `vendor:addr` | A | address §2.1 | ✓ |
| 5 | `vendor:notes` | non-empty | string | |
| 6 | `vendor:terms` | terms set | GUID of bill term | |
| 7 | `vendor:taxincluded` | A | `YES`/`NO`/`USEGLOBAL` | ✓ |
| 8 | `vendor:active` | A | `0`/`1` | ✓ |
| 9 | `vendor:currency` | currency non-NULL | commodity ref | (alias `vendor:commodity`) |
| 10 | `vendor:use-tt` | A | `0`/`1` | |
| 11 | `vendor:taxtable` | tax table set | GUID of tax table | |
| 12 | `vendor:slots` | KVP non-empty | slots | |

Observed (verbatim):

```
<gnc:GncVendor version="2.0.0">
  <vendor:guid type="guid">66ad7409a8a6458bb27af1c376667296</vendor:guid>
  <vendor:name>Paper Supplies</vendor:name>
  <vendor:id>000001</vendor:id>
  <vendor:addr version="2.0.0">
    <addr:name>Paper Supplies Accounts</addr:name>
  </vendor:addr>
  <vendor:terms type="guid">872101fc4ffa4d8596e471842238e65c</vendor:terms>
  <vendor:taxincluded>YES</vendor:taxincluded>
  <vendor:active>1</vendor:active>
  <vendor:currency>
    <cmdty:space>CURRENCY</cmdty:space>
    <cmdty:id>EUR</cmdty:id>
  </vendor:currency>
  <vendor:use-tt>0</vendor:use-tt>
</gnc:GncVendor>
```

A vendor with no currency is written without `vendor:currency` (and logs a critical), and an empty name is
`<vendor:name/>` (§12, run "small").

## 6. Enum strings (complete)

| Field(s) | Strings | Source |
|---|---|---|
| `cust:taxincluded`, `vendor:taxincluded` | `YES` (1), `NO` (2), `USEGLOBAL` (3) | 5.17:libgnucash/engine/gncTaxTable.c:93-104, 121-129 |
| `tte:type`, `entry:i-disc-type` | `VALUE` (1), `PERCENT` (2) | 5.17:libgnucash/engine/gncTaxTable.c:79-90, 110-118 |
| `entry:i-disc-how` | `PRETAX` (1), `SAMETIME` (2), `POSTTAX` (3) | 5.17:libgnucash/engine/gncEntry.c:113-155 |
| `entry:b-pay` | `CASH` (1), `CARD` (2) | 5.17:libgnucash/engine/gncEntry.c:157-190 |
| `owner:type` | `gncCustomer`, `gncJob`, `gncVendor`, `gncEmployee` | 5.17:libgnucash/backend/xml/gnc-owner-xml-v2.cpp:62-79 |
| bill term type | element name: `billterm:days` (DAYS = 1), `billterm:proximo` (PROXIMO = 2), none (0) | 5.17:libgnucash/engine/gncBillTerm.h:122-126 |
| booleans (`active`, `use-tt`, `invisible`, `i-taxable`, …) | `0`/`1` as integers | `int_to_dom_tree` |

Matching is exact and case-sensitive (`g_strcmp0`). Numeric values in parentheses are the C enum values; they appear in
the file only as the lot slot `gncOwner/owner-type` (§9.2), where `GncOwnerType` is NONE 0, UNDEFINED 1, CUSTOMER 2,
JOB 3, VENDOR 4, EMPLOYEE 5 (5.17:libgnucash/engine/gncOwner.h:45-53).

## 7. Bill term and tax table families, refcounts, load scrubs

**Parent/child.** When an invoice is posted, GnuCash freezes its terms and each entry's tax table: `ReturnChild`
returns the existing child, or for a visible parent makes a copy (same name, description/type/days/discount/cutoff, or
same entries) and links it with `SetChild(parent, child)` + `SetParent(child, parent)`; a child or invisible table
returns itself (5.17:libgnucash/engine/gncBillTerm.c:595-609, gncTaxTable.c:749-762; called from
5.17:libgnucash/engine/gncInvoice.c:1469-1472, 1544-1551). A parent has at most one child, reused by every later
posting until the parent is edited (`gncTaxTableChanged` clears the child, 5.17:libgnucash/engine/gncTaxTable.c:622-628).
Children are `invisible = 1`, `refcount = 0` and hidden from the selection lists, but they are ordinary objects in the
collection and are always written.

**Refcount** = number of direct references from (bill terms) customers, vendors, invoices; (tax tables) customers,
vendors, entries (`i-taxtable` and `b-taxtable` count separately). `IncRef`/`DecRef` are no-ops on children and
invisible tables (5.17:libgnucash/engine/gncTaxTable.c:516-535, gncBillTerm.c:420-439); setters call them
(e.g. 5.17:libgnucash/engine/gncCustomer.c:407-420, gncEntry.c:641-643, gncInvoice.c:520-523). On read the stored
value is set directly; the reference handlers compensate with a DecRef when the target already exists
(5.17:libgnucash/backend/xml/gnc-bill-term-xml-v2.cpp:755-778, gnc-customer-xml-v2.cpp:295-316).

**Load-time scrubs** run after the whole file is parsed, in registry order: bill terms then tax tables
(5.17:libgnucash/backend/xml/io-gncxml-v2.cpp:819-823). They change data, so a GnuCash load/save of an inconsistent
file is not byte-identical:

| Scrub | Effect | Source |
|---|---|---|
| bill term, invoices | invoice terms that are grandchildren (parent has a parent) are re-pointed at the most senior child | gnc-bill-term-xml-v2.cpp:591-620 |
| bill term, all | grandchildren are destroyed (parent's child cleared); a term with **type 0 and no parent is destroyed**; a type-0 child gets its parent's type/days/discount/cutoff copied | :550-586, 702-718 |
| bill term, refcount | for terms referenced at least once: if count ≠ stored refcount and not invisible, stored refcount := count. Unreferenced terms keep their stored refcount. | :672-686, 721 |
| tax table, entries | entry tax tables that are grandchildren are re-pointed at the most senior child | gnc-tax-table-xml-v2.cpp:530-582 |
| tax table, all | grandchildren **and tables with no entries** are destroyed | :517-524, 652-666 |
| tax table, refcount | as for bill terms (customers, vendors, entries) | :619-633, 669 |

Confirmed (§12, run "main"): a parent tax table with refcount 3 but one customer reference was rewritten with 1
(`Fixing refcount on taxtable … (3 -> 1)`); an unreferenced table with refcount 5 kept 5; a table with
`<taxtable:entries/>` disappeared and the count dropped accordingly. Note the type-0 bill term destroyed by the scrub is
exactly what GnuCash writes for a term whose type was never set (§5.1 "Untyped" form).

## 8. Reading enum and boolean fields: defaults on failure

| Field | On unknown/invalid text | Source |
|---|---|---|
| `*:taxincluded` | keeps default USEGLOBAL | gnc-customer-xml-v2.cpp:243-254 |
| `entry:i-disc-type`, `entry:i-disc-how`, `entry:b-pay`, `tte:type` | keeps default (PERCENT, PRETAX, CASH, 0) | gnc-entry-xml-v2.cpp:392-420, 503-516; gnc-tax-table-xml-v2.cpp:160-171 |
| `*:active`, `*:use-tt`, `entry:*-taxable`, … | not parsed → field unchanged | gnc-customer-xml-v2.cpp:149-161 |
| `billterm:refcount`/`invisible`, `taxtable:refcount`/`invisible` | uses an uninitialised integer (GnuCash bug) | gnc-bill-term-xml-v2.cpp:315-336 |

red-ale's reader should reject these rather than guess; see Open questions.

## 9. Slots used by business features

Slot encoding (types `integer`, `string`, `guid`, `numeric`, `frame`, `timespec`, `gdate`) is in the core note.

### 9.1 On business objects

| Object | Path | Type | Meaning | Source |
|---|---|---|---|---|
| Invoice | `credit-note` | integer `1`/`0` | document is a credit note; setting it sets book feature "Credit Notes" | 5.17:libgnucash/engine/gncInvoice.c:86, 576-593 |
| Invoice | `assoc_uri` | string | document link (old key name kept) | 5.17:libgnucash/engine/gncInvoice.c:87, 544-561 |
| Job | `job-rate` | numeric | default rate; slot removed when rate set to 0 | 5.17:libgnucash/engine/gncJob.c:59, 315-335 |
| Customer, Vendor, Employee | `export-pdf-directory` | string | last PDF export directory | 5.17:libgnucash/engine/gncBusiness.h:85; gncCustomer.c:140-148 |
| Customer, Vendor, Employee | `last-posted-to-acct` | guid | last posting account | 5.17:libgnucash/engine/gncBusiness.h:86 |
| Customer, Vendor, Employee | `payment/last_acct` | guid | last payment transfer account | 5.17:libgnucash/engine/gncBusiness.h:87-88 |

Any slot on an address, job, entry or tax table sets book feature "Extra data in addresses, jobs or invoice entries"
on commit (5.17:libgnucash/engine/gncAddress.c:493-497, gncJob.c:428-431, gncEntry.c:1636-1640,
gncTaxTable.c:651-656).

### 9.2 On transactions and lots (written by posting and payment)

| Where | Path | Type | Value | Source |
|---|---|---|---|---|
| posted txn | `gncInvoice/invoice-guid` | guid | invoice GUID | 5.17:libgnucash/engine/gncInvoice.c:1255-1268; gnc-engine.h:257-258 |
| posted txn | `trans-txn-type` | string | `I` (invoice), `P` (payment), `L` (lot link) | 5.17:libgnucash/engine/Transaction.h:125-128, Transaction.cpp:183 |
| posted txn | `trans-date-due` | timespec | due date | 5.17:libgnucash/engine/Transaction.cpp:182, 1958 |
| posted txn | `trans-read-only` | string | "Generated from an invoice. Try unposting the invoice." (translated) | 5.17:libgnucash/engine/gncInvoice.c:1713 |
| invoice lot | `gncInvoice/invoice-guid` | guid | invoice GUID | 5.17:libgnucash/engine/gncInvoice.c:1219-1233 |
| invoice lot | `title` | string | "<type> <id>", type = translated `Invoice`/`Bill`/`Expense`/`Credit Note` | 5.17:libgnucash/engine/gncInvoice.c:1489-1493, 1089-1108 |
| pre-payment lot | `gncOwner/owner-type` | integer | `GncOwnerType` (customer = 2) | 5.17:libgnucash/engine/gncOwner.c:621-633 |
| pre-payment lot | `gncOwner/owner-guid` | guid | owner GUID | same |

Posting also sets the transaction's `num` to the invoice ID and each split's action to the type string (both via
`gnc_set_num_action`, 5.17:libgnucash/engine/gncInvoice.c:1504, 1612, 1675, 1696). Observed posted transaction slots
(verbatim):

```
  <trn:slots>
    <slot>
      <slot:key>date-posted</slot:key>
      <slot:value type="gdate">
        <gdate>2024-03-05</gdate>
      </slot:value>
    </slot>
    <slot>
      <slot:key>gncInvoice</slot:key>
      <slot:value type="frame">
        <slot>
          <slot:key>invoice-guid</slot:key>
          <slot:value type="guid">35ad2d2ebe3847ea9a415e7cc53cc09f</slot:value>
        </slot>
      </slot:value>
    </slot>
    <slot>
      <slot:key>trans-date-due</slot:key>
      <slot:value type="timespec">
        <ts:date>2024-04-03 23:00:00 +0000</ts:date>
      </slot:value>
    </slot>
    <slot>
      <slot:key>trans-read-only</slot:key>
      <slot:value type="string">Generated from an invoice. Try unposting the invoice.</slot:value>
    </slot>
    <slot>
      <slot:key>trans-txn-type</slot:key>
      <slot:value type="string">I</slot:value>
    </slot>
  </trn:slots>
```

Invoice lot and a pre-payment lot (verbatim):

```
    <gnc:lot version="2.0.0">
      <lot:id type="guid">990068d856184ff2966e0d91c987124a</lot:id>
      <lot:slots>
        <slot>
          <slot:key>gncInvoice</slot:key>
          <slot:value type="frame">
            <slot>
              <slot:key>invoice-guid</slot:key>
              <slot:value type="guid">35ad2d2ebe3847ea9a415e7cc53cc09f</slot:value>
            </slot>
          </slot:value>
        </slot>
        <slot>
          <slot:key>title</slot:key>
          <slot:value type="string">Invoice 000001</slot:value>
        </slot>
      </lot:slots>
    </gnc:lot>
```
```
      <lot:id type="guid">25529ce1ec6546dea5b3c79f904c05d6</lot:id>
      <lot:slots>
        <slot>
          <slot:key>gncOwner</slot:key>
          <slot:value type="frame">
            <slot>
              <slot:key>owner-guid</slot:key>
              <slot:value type="guid">b8e36e1fda8a4ffbb3ab1703daaa2460</slot:value>
            </slot>
            <slot>
              <slot:key>owner-type</slot:key>
              <slot:value type="integer">2</slot:value>
            </slot>
          </slot:value>
        </slot>
      </lot:slots>
```

A partial payment applied to an invoice is a separate transaction (`trans-txn-type` `P`) whose A/R split joins the
invoice's lot (`<split:lot>` = the invoice lot); no extra business slot is written (observed).

## 10. Book-level KVP for business

All under `<book:slots>`; a KVP frame writes its keys sorted (std::map), so `counter_formats` < `counters` <
`features` < `options` (observed: GnuCash moved a hand-inserted `counter_formats` frame before `counters`).

### 10.1 `counters` and `counter_formats`

| Path | Type | Meaning |
|---|---|---|
| `counters/<name>` | integer | **last** number issued (0 or absent = none yet) |
| `counter_formats/<name>` | string | printf format with exactly one 64-bit integer conversion |

`<name>`: `gncCustomer`, `gncVendor`, `gncEmployee`, `gncJob`, `gncOrder` (each type's QOF name;
5.17:libgnucash/engine/gncCustomer.c:962, gncVendor.c:1022, gncEmployee.c:950, gncJob.c:602, gncOrder.c:591);
invoices use the **end owner's** type: `gncInvoice` (customer, or customer job), `gncBill` (vendor, vendor job),
`gncExpVoucher` (employee), else `gncInvoice` (5.17:libgnucash/engine/gncInvoice.c:2256-2275). Credit notes share
their owner type's counter.

Next ID algorithm (5.17:libgnucash/engine/qofbook.cpp:590-695):

```
n = counters/<name> as int64            // absent → 0; a legacy double value is truncated to int (bug 798930)
n = n + 1; write counters/<name> = n (integer)
fmt = counter_formats/<name>, normalised; invalid or absent → "%.6" PRIi64   // i.e. zero-padded to 6 digits
id = sprintf(fmt, n)                    // "000001"
```

Normalisation (5.17:libgnucash/engine/qofbook.cpp:752-850) accepts a prefix, one conversion with flags `#0- +'I`,
width/precision digits, and length/conversion spelled `G_GINT64_FORMAT`, `lli`, `I64i`, `PRIi64` or `li`, and rewrites
it to the platform's `PRIi64`. Observed (verbatim, after 1 customer, 1 vendor, 1 bill, 1 voucher, 2 customer
invoices):

```
  <slot>
    <slot:key>counters</slot:key>
    <slot:value type="frame">
      <slot>
        <slot:key>gncBill</slot:key>
        <slot:value type="integer">1</slot:value>
      </slot>
      <slot>
        <slot:key>gncCustomer</slot:key>
        <slot:value type="integer">1</slot:value>
      </slot>
      <slot>
        <slot:key>gncExpVoucher</slot:key>
        <slot:value type="integer">1</slot:value>
      </slot>
      <slot>
        <slot:key>gncInvoice</slot:key>
        <slot:value type="integer">2</slot:value>
      </slot>
      <slot>
        <slot:key>gncVendor</slot:key>
        <slot:value type="integer">1</slot:value>
      </slot>
    </slot:value>
  </slot>
```

The Book Options dialog edits the same paths: option section "Counters", option keys `<name>a`/`<name>b`; the path is
`counters/<name>` or `counter_formats/<name>` (not under `options/`) (5.17:libgnucash/engine/gnc-optiondb.cpp:374-387,
1228-1297; 5.17:libgnucash/engine/qofbook.cpp:1362-1376). Counter values are saved as integer
(5.17:libgnucash/engine/gnc-optiondb.cpp:495-497, 947-955).

### 10.2 `options/Business/…` and `options/Tax/…`

Option paths are `options/<section>/<option name>`, with the **untranslated** English names
(5.17:libgnucash/engine/qofbookslots.h:55, gnc-optiondb.cpp:1299-1351). An option is written only after it has been
changed in the dialog (`is_dirty`, 5.17:libgnucash/engine/gnc-optiondb.cpp:474); absent = default.

| Path | Type | Notes |
|---|---|---|
| `options/Business/Company Name` | string | |
| `options/Business/Company Address` | string | multi-line text |
| `options/Business/Company Contact Person` | string | |
| `options/Business/Company Phone Number` | string | |
| `options/Business/Company Fax Number` | string | |
| `options/Business/Company Email Address` | string | |
| `options/Business/Company Website URL` | string | |
| `options/Business/Company ID` | string | |
| `options/Business/Default Invoice Report` | string | `"<report-guid>/<report name>"` (5.17:libgnucash/engine/qofbook.cpp:1044-1088) |
| `options/Business/Default Invoice Report Timeout` | double | number range 0-20 |
| `options/Business/Default Customer Tax Table` | guid | tax table GUID (5.17:libgnucash/engine/gnc-optiondb.cpp:433-438) |
| `options/Business/Default Vendor Tax Table` | guid | |
| `options/Business/Fancy Date Format` | frame | keys `fmt` (string), `month` (string), `years` (integer), `custom` (string); not written while format is UNSET (5.17:libgnucash/engine/gnc-optiondb.cpp:442-462) |
| `options/Tax/Tax Number` | string | |

Observed (verbatim; the second and third slots were hand-inserted and kept, re-sorted by key, on resave):

```
  <slot>
    <slot:key>options</slot:key>
    <slot:value type="frame">
      <slot>
        <slot:key>Business</slot:key>
        <slot:value type="frame">
          <slot>
            <slot:key>Company Name</slot:key>
            <slot:value type="string">Synthetic Ltd</slot:value>
          </slot>
          <slot>
            <slot:key>Default Customer Tax Table</slot:key>
            <slot:value type="guid">0378d4eceda949ecbfc4f29555413868</slot:value>
          </slot>
          <slot>
            <slot:key>Default Invoice Report</slot:key>
            <slot:value type="string">0769e242be474010b4acf264a5512e6e/Printable Invoice</slot:value>
          </slot>
        </slot:value>
      </slot>
    </slot:value>
  </slot>
```

GnuCash bug to know about: `gncTaxTableGetDefault`, used by the entry ledger to pre-fill new lines, reads
`options/Business/Default Customer TaxTable` / `Default Vendor TaxTable` (no space), which the options dialog never
writes (5.17:libgnucash/engine/gncTaxTable.c:680-699 vs. gnc-optiondb.cpp:1338-1345). red-ale writes the dialog's
paths and should read both, preferring the dialog's.

### 10.3 Book features set by business code

`features/Credit Notes` = "Customer and vendor credit notes (requires at least GnuCash 2.5.0)" and
`features/Extra data in addresses, jobs or invoice entries` = "Extra data for addresses, jobs or invoice entries
(requires at least GnuCash 2.6.4)" (5.17:libgnucash/engine/gnc-features.cpp:38-40), set as described in §5.5 and §9.1.
Both appear in the observed book.

## 11. Rules for red-ale

1. **Writer**: emit types in the order of §1, each sorted by GUID string; count lines per §1.2 (including GnuCash's
   counting of unattached entries, if red-ale can ever hold one — better: make an unattached entry impossible, so the
   question does not arise); element order and conditions exactly as §5; empty strings as `<TAG/>`.
2. **Never write an element GnuCash does not know** inside a business object: it makes the whole book unreadable (§3).
   Extensions go in `…:slots`.
3. **Invariants to enforce in the domain** so the writer never emits an unreadable or scrub-altered file: non-empty ID
   for anything that must survive a save; invoice/order/job have an owner; invoice/order have an opened date; tax
   tables have at least one entry; bill terms have a type; owner type is one of the four.
4. **Reader**: be at least as strict as GnuCash at top level (unknown or missing required element → refuse the book);
   for nested sub-structures, GnuCash drops unknown elements — red-ale should refuse or open read-only rather than drop
   (CLAUDE.md "never lose data"); see Open questions.
5. **Round trip**: keep `refcount`, `invisible`, parent/child links, tax-table entry order, and `num/denom` exactly as
   read. Do not run GnuCash's scrubs on load; apply GnuCash's IncRef/DecRef rules only when red-ale itself changes a
   reference.
6. **Counters**: issue IDs exactly as §10.1 and write the counter as `integer`.
7. Two-phase load: parse everything, then resolve GUID references (forward references are normal, §3).

## 12. Experiments

Oracle: GnuCash 5.17 Python bindings (`gnc_version() == '5.17'` asserted in every script). Directory:
`/tmp/claude-1001/-home-code-projects-red-ale/bbc44319-413c-4d3a-9c4c-5d9e0c7bf9fe/scratchpad/bizxml/` (synthetic data
only). Books are saved gzipped by default; `.xml` files are `zcat` output. `make_book.py` was run twice (the second run
made the voucher a bill-side document and gave the bill line a tax table); excerpts from the first run (bill terms,
customer, employee, invoice-side entries, invoices, job, tax tables, vendor) differ from the `biz.xml` on disk only in
GUIDs. The count-data excerpt in §1.2 is from `main.out.xml`.

| Run | Script / input | What | Result |
|---|---|---|---|
| biz | `make_book.py` → `biz.gnucash`, `biz.xml` | accounts; bill terms Net30 (days 30/10/2%) and Prox15 (proximo 15/5/1.5%/cutoff 25); tax table GST+Levy (10% + 2.50 fixed); customer with addresses, terms, tax table, discount, credit; vendor (proximo, tax included); employee with card account; job with rate; customer invoice (2 lines) posted and partially paid (500.00 of 1373.00); vendor bill (billable line, tax included) posted; employee voucher (card line) posted; job-owned unposted credit note; default invoice report option | all excerpts in §5, §9, §10.1 |
| rt | `resave.py` on a copy of `biz.gnucash` | load + no-net-change edit + save | byte-identical to `biz.xml` |
| main | `inject.py biz.xml main.gnucash main` → `main.in.xml`, `main.out.xml` | adds counter format, business options, an order and an order entry, an orphan entry, an empty tax table, an unused tax table with refcount 5, a wrong parent refcount, a customer with empty ID | order kept; orphan entry and empty-ID customer dropped; entry count 7; empty tax table deleted; refcount 3→1; unused keeps 5; book KVP re-sorted |
| unknown | `<vendor:frobnicate>` in a vendor | | load fails ("Syntax error in Xml File") |
| missing | `<vendor:active>` removed | | load fails |
| badenum | `MAYBE`, `LATER` | | rewritten as `USEGLOBAL`, `PRETAX` |
| nested | `<addr:zip>` inside a vendor address | | element silently dropped |
| small | `small.py` → `small.xml` | untyped and all-zero bill terms; vendor with no currency; vendor-owned job; voucher with charge amount; bill with bill-to; invoice with no owner and no opened date | §5.1/§5.5/§5.9 edge forms; GnuCash then **fails to load its own file** |
| prepay | `prepay.py` on a copy of `biz.gnucash` | unallocated customer payment | pre-payment lot slots §9.2 |

Side finding: creating or editing only business objects does not mark the book dirty, so `Session.save()` silently
writes nothing; the scripts touch an account to force the save.

## Open questions

1. **Reader strictness for nested unknown elements and bad enum/boolean text.** GnuCash drops or defaults them
   (§3, §8). Options: refuse the book, or open read-only with a diagnostic. Leaning: read-only, consistent with the
   "unknown feature → read-only" rule. Needs a decision in `open-questions.md`.
2. **Files written by older GnuCash in hash order.** GnuCash 5.17 re-sorts by GUID on save. Should red-ale preserve
   as-read order (byte-identical with the input) or sort (byte-identical with what GnuCash 5.17 would write)? Both
   satisfy "GnuCash considers identical". Not settled: since which version GnuCash sorts business objects.
3. **Load-time scrubs.** red-ale keeps inconsistent refcounts/grandchildren as read (rule 5). Should it instead warn,
   or offer a GnuCash-equivalent "scrub" use case? GnuCash will apply the scrub the next time it opens the book anyway.
4. **Unattached entries and `gnc:GncEntry` count.** If red-ale ever holds an entry on no document (e.g. mid-edit), must
   it reproduce GnuCash's over-count? Proposed: impossible by construction.
5. **Translated strings** in lot titles, split actions and `trans-read-only` depend on GnuCash's UI locale. Which
   locale should red-ale use when it posts? (English source strings are what the oracle wrote.)
6. **Entry and invoice date conventions** (canonical 10:59 UTC vs. local midnight): the GUI uses canonical day time for
   entry dates and posted dates; the date-opened dialog path passes a `time64` from the date widget whose time of day
   was not checked here.
7. **Legacy aliases** (`*:commodity`, `entry:acct`, …): accept on read for v0.1 or reject? They only occur in books
   last saved by GnuCash ≤ 1.8.
8. `billterm:refcount`/`invisible` parse failure uses an uninitialised value in GnuCash (§8): only matters for corrupt
   files; red-ale rejects.
