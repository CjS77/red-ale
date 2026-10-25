# Engine note: business features (GnuCash 5.17)

Scope: the domain rules behind milestone-1 §6 Business (customers, vendors, employees, jobs, invoices, bills, expense
vouchers, orders, tax tables, billing terms, post, unpost, process payment, due reminders). This note covers
**behaviour**. Element names and order in the XML file are covered by the XML-format note. Slot paths are given here
because they come from engine behaviour.

Citations are `5.17:<path>:<line>`. "GUI" means GnuCash's GTK front end. GnuCash code there is policy, not engine,
but it sets the defaults users get. Oracle runs used the GnuCash 5.17 Python bindings with `TZ=UTC`. The scripts and
books are in the scratchpad (`business/exp1.py`, `exp2.py`, `exp3.py`) and are summarised in §12.

Notation: `Q` = rational (`num/denom`, i64). "Exact" = an exact rational result. GnuCash's `GNC_DENOM_AUTO` with
REDUCE or LCD stays exact unless 64-bit overflow forces an approximation. **HALF_UP** = round half away from zero
(`5.17:libgnucash/engine/gnc-rational-rounding.hpp:173`). `round(x, d)` = convert to denominator `d` with HALF_UP.

---

## 1. Owners

### 1.1 Owner (the polymorphic reference)

`GncOwner = { type, ptr }`, where type ∈ `NONE=0, UNDEFINED=1, CUSTOMER=2, JOB=3, VENDOR=4, EMPLOYEE=5`
(`5.17:libgnucash/engine/gncOwner.h:47`). The integer is what is stored in lot slot `gncOwner/owner-type` (§6.3).

- **End owner** (`gncOwnerGetEndOwner`): a customer, vendor or employee is its own end owner. For a job it is the job's
  owner (a customer or vendor). NONE and UNDEFINED give null (`5.17:libgnucash/engine/gncOwner.c:572`). Every
  "is this a customer document?" test uses the end owner: `gncInvoiceGetOwnerType`
  (`5.17:libgnucash/engine/gncInvoice.c:888`).
- **Equality** (`gncOwnerEqual`): same type **and** same object pointer, i.e. same entity (same GUID). Null on
  either side gives false (`5.17:libgnucash/engine/gncOwner.c:404`). A job and its customer are *not* equal. Code that
  matches lots to owners compares **end owners** (§8.4).
- `gncOwnerGetName` of a job is the job's name. The posting and payment code always uses the **end owner's** name for
  transaction descriptions (`gncInvoice.c:1499`, `gncOwner.c:771`).
- An address exists for customers, vendors and employees. A job has none (`gncOwner.c:459`). A job's currency is its
  owner's currency (`gncOwner.c:478`).
- Account types for posting and payments: customer → RECEIVABLE; vendor and employee → PAYABLE
  (`gncOwner.c:1443`). Job: the GUI uses its end owner.

### 1.2 Address

Fields: `name, addr1, addr2, addr3, addr4, phone, fax, email`, all strings and empty by default
(`5.17:libgnucash/engine/gncAddress.c:38`). An address belongs to its parent owner. Committing an address that
carries KVP sets book feature `Extra data in addresses, jobs or invoice entries` (`gncAddress.c:493`,
`5.17:libgnucash/engine/gnc-features.h:56`).

### 1.3 Customer

Fields (`5.17:libgnucash/engine/gncCustomer.c:51`), with defaults from `gncCustomerCreate` (`gncCustomer.c:303`):

| Field | Type | Default | Behaviour |
|---|---|---|---|
| id | string | `""` | Free text. Uniqueness is not enforced by the engine or the GUI. |
| name | string | `""` | "Company name". The GUI refuses blank (`5.17:gnucash/gnome/dialog-customer.c:313`). |
| notes | string | `""` | |
| addr, shipaddr | Address | empty | Billing and shipping addresses. |
| terms | BillTerm ref | null | Default terms for new invoices (GUI, `dialog-invoice.c:2981`). Refcounted (§3.4). |
| currency | commodity | null | The GUI sets the invoice currency from it (§5.3). |
| taxtable | TaxTable ref | null | Refcounted. |
| taxtable_override | bool | false | XML `use-tt`. When true, new entries default to `taxtable` (§4.4). |
| taxincluded | enum `YES=1, NO=2, USEGLOBAL=3` | USEGLOBAL | Default tax-included flag for new entries (§4.4). Strings `YES`/`NO`/`USEGLOBAL` (`gncTaxTable.c:93`). |
| active | bool | true | |
| discount | Q | 0 | Default percent discount for new invoice entries (§4.4). The GUI checks 0–100 (`dialog-customer.c:323`). |
| credit | Q | 0 | Credit limit. Stored only; nothing in 5.17 reads it except the dialog. |
| jobs | list | — | Derived from jobs' owners. Not stored. |

KVP written by the GUI (`gncCustomer.c:140`, `5.17:libgnucash/engine/gncBusiness.h:85`):
`last-posted-to-acct` (GUID of the last account posted to, written at post: `dialog-invoice.c:1268`),
`payment/last_acct` (last transfer account in the payment dialog) and `export-pdf-directory`. These are optional
conveniences. red-ale must keep them; it need not write them.

### 1.4 Vendor

Same as customer **minus** `discount`, `credit` and `shipaddr` (`5.17:libgnucash/engine/gncVendor.c:51`).
Defaults are the same (`gncVendor.c:453`). The same three KVPs apply.

### 1.5 Employee

Fields (`5.17:libgnucash/engine/gncEmployee.c:47`, defaults at `gncEmployee.c:428`): `id ""`, `username ""`,
`addr`, `currency`, `active true`, `language ""`, `acl ""`, `workday Q 0`, `rate Q 0` and `ccard_acc` (Account
ref, null).

- **The employee's name is `addr.name`**: `gncEmployeeSetName` and `gncEmployeeGetName` go through the address
  (`gncEmployee.c:512`, `gncEmployee.c:633`). The GUI requires it ("Payment-Address Name",
  `dialog-employee.c:203`).
- `ccard_acc`: when it is set, expense-voucher entries with payment type CARD are credited to this account at post
  (§6.5).
- `language`, `acl`, `workday` and `rate` are stored only. Nothing computes with them.

### 1.6 Job

Fields (`5.17:libgnucash/engine/gncJob.c:41`, defaults `gncJob.c:212`): `id ""`, `name ""`, `desc ""` (UI and
API name "reference"), `owner` (customer or vendor only; anything else is rejected with an error,
`gncJob.c:336`) and `active true`.

- **Rate** is KVP `job-rate` (numeric). Setting zero **removes** the slot (`gncJob.c:314`). Any job with KVP sets the
  `Extra data …` feature on commit (`gncJob.c:427`). The GUI uses the rate as the default price of new entries on
  an invoice whose owner is the job (`5.17:gnucash/register/ledger-core/gncEntryLedgerLoad.c:385`).
- Changing a job's owner moves it between the owners' job lists (`gncJob.c:354`).
- Sort order is by `id` (`gncJob.c:497`).

### 1.7 ID assignment (counters)

`gnc<Type>NextID(book)` = `qof_book_increment_and_format_counter(book, <counter>)`:

| Entity | Counter name | Source |
|---|---|---|
| Customer | `gncCustomer` | `gncCustomer.c:960` |
| Vendor | `gncVendor` | `gncVendor.c:1020` |
| Employee | `gncEmployee` | `gncEmployee.c:948` |
| Job | `gncJob` | `gncJob.c:600` |
| Order | `gncOrder` | `gncOrder.c:589` |
| Invoice (end owner customer) | `gncInvoice` | `gncInvoice.c:2256` |
| Bill (end owner vendor) | `gncBill` | ″ |
| Expense voucher (end owner employee) | `gncExpVoucher` | ″ |
| Other owner | `gncInvoice` (`_GNC_MOD_NAME`) | ″ |

Credit notes share their document type's counter: customer credit notes use `gncInvoice`, and so on.

Algorithm (`5.17:libgnucash/engine/qofbook.cpp:590`, `:636`, `:697`):

```
n = book.slots["counters"][name]      // int64. Missing → 0. If the int read is 0, it is re-read as a double (bug 798930).
n += 1
book.slots["counters"][name] = KvpValue(int64 n)   // book marked dirty
fmt = book.slots["counter_formats"][name]          // string, optional
fmt = normalize(fmt) or "%.6" PRIi64               // default → "%.6li" on Linux: at least 6 digits, zero-padded
return printf(fmt, n)
```

- **Default format:** `"%.6" PRIi64` (`qofbook.cpp:747`), so 1 → `000001` and 1234567 → `1234567`. Oracle: the first
  customer, invoice, bill and voucher were `000001`.
- **Normalisation** (`qofbook.cpp:752`): the format must have exactly one conversion. Allowed: a prefix with `%%`
  escapes, `%`, flags from `#0- +'I`, width and precision digits and `.`, then a length+conversion that is one of
  `G_GINT64_FORMAT`, `lli`, `I64i`, `PRIi64`, `li`, then a suffix with `%%` escapes only. The specifier is rewritten to
  `PRIi64`. Anything else is invalid: GnuCash logs a warning and **falls back to the default format**. Oracle:
  `INV-%04li` → `INV-0003`, `C-%li%%` → `C-2%`, `%d` → `000001` plus a warning.
- The counter goes up **whenever** NextID is called, even if the caller then discards the ID (the GUI calls it only
  when the ID field is empty on OK: `dialog-customer.c:339`, `dialog-invoice.c:478`, `dialog-job.c:161`).
- The book option dialog edits counter values and formats under the same `counters` and `counter_formats` frames
  (`qofbook.cpp:1362`).
- `gnc_search_{customer,vendor,invoice,bill}_on_id` look up by exact ID string. They return the first match. IDs are
  not unique keys (`5.17:libgnucash/engine/gncIDSearch.c:96`).

---

## 2. Tax tables

### 2.1 Model

`TaxTable { name, entries: [TaxTableEntry], refcount: i64, invisible: bool, parent?, child?, modtime }`
(`5.17:libgnucash/engine/gncTaxTable.c:36`).
`TaxTableEntry { account: Account, type: VALUE=1 | PERCENT=2, amount: Q }` (`gncTaxTable.c:56`,
`gncTaxTable.h:80`). Type strings are `VALUE` and `PERCENT` (`gncTaxTable.c:79`).

- PERCENT amounts are percentages (`5` = 5 %). VALUE amounts are a fixed currency amount **per entry line, not per
  unit** (§4.2).
- Entries are kept sorted by **account full name** (`g_strcmp0`), then amount (`gncTaxTableEntryCompare`,
  `gncTaxTable.c:821`; insertion `gncTaxTable.c:594`). This order drives tax-split order (§6.4).
- `modtime` (not stored) changes whenever an entry is added, removed or edited. Entries cache their computed values
  and recompute when the table's modtime changes (`gncEntry.c:1370`).
- The book's visible-table list holds only tables that are neither children nor invisible, sorted by name
  (`gncTaxTable.c:152`, `:167`).
- **Name uniqueness:** the engine does not enforce it. The GUI refuses a duplicate **visible** name on create and
  on rename (`5.17:gnucash/gnome-utils/dialog-tax-table.c:163`, `:703`). Invisible children share their parent's
  name. Lookup by name searches visible tables only (`gncTaxTable.c:667`).
- GUI validation: PERCENT must be in [-100, 100] (negatives allowed, "European tax rules"); an account is required
  (`dialog-tax-table.c:184`, `:197`). It refuses to delete a table with refcount > 0 (`dialog-tax-table.c:726`).
- Book defaults: book option `options/Business/Default Customer TaxTable` and `…/Default Vendor TaxTable` (GUID)
  (`gncTaxTable.c:680`).
- Committing a table that has KVP sets the `Extra data …` feature (`gncTaxTable.c:651`).

### 2.2 Refcount, parent and child (immutability of posted documents)

Tax tables and billing terms share this mechanism ("Code that handles this is *identical*", `gncTaxTable.c:42`,
`gncBillTerm.c:51`).

- **refcount** counts references from customers, vendors, entries and invoices (terms) to a *visible* table.
  `IncRef` and `DecRef` are no-ops on a child or an invisible table (`gncTaxTable.c:516`, `:526`). Setters on
  referrers do DecRef(old) then IncRef(new) (for example `gncEntry.c:630`, `gncCustomer.c:482`). Freeing a referrer
  outside book shutdown does DecRef (`gncEntry.c:467`).
- **ReturnChild(t, make_new)** (`gncTaxTable.c:749`):
  ```
  if t.child: return t.child                 // reuse the existing frozen copy
  if t.parent or t.invisible: return t       // t is already a frozen copy
  if make_new:
      c = Copy(t)                            // new GUID, same name, copies of the entries (same accounts, types, amounts)
      t.child = c
      c.SetParent(t)                         // c.refcount = 0; c.invisible = true; removed from the visible list
      return c
  return null
  ```
- **When a child is made:** only at **post** (§6.2): each entry's tax table becomes `ReturnChild(table, TRUE)`, and
  the same for the invoice's terms. Because the entry setter does `DecRef(parent)` and `IncRef(child)` is a no-op,
  posting drops the parent's refcount once per entry. Oracle: refcount 3 → 0 after posting three taxable entries
  (§12.1). The child's refcount stays 0.
- **When the cached child is dropped:** `gncTaxTableChanged` sets `t.child = null`. The GUI calls it after any edit to
  the table's entries (`dialog-tax-table.c:240`). The next post then makes a new child. Unchanged tables keep reusing
  one child, so many posted invoices share one frozen copy.
- `gncTaxTableSetParent` **always** makes the table invisible, even when parent is null (`gncTaxTable.c:492`).
  The bill-term version does so only when parent ≠ null (`gncBillTerm.c:403`).
- Destroying a parent sets each child's parent to null (`gncTaxTable.c:451`). The children stay invisible.
- Unpost with `reset_tax_tables` moves entries back to the parent (§7).

---

## 3. Billing terms

### 3.1 Model

`BillTerm { name, desc, type: DAYS=1 | PROXIMO=2, due_days: i32, disc_days: i32, discount: Q, cutoff: i32,
refcount, invisible, parent?, child? }` (`5.17:libgnucash/engine/gncBillTerm.c:40`, `gncBillTerm.h:116`).
Defaults: name and desc `""`, discount 0, all ints 0, **type 0** (unset; no valid enum value) (`gncBillTerm.c:248`).

- `disc_days` and `discount` (early-payment discount) are **stored only**. Nothing in 5.17 computes a discount date or
  amount from them (no reader outside the dialog and backends).
- `cutoff` is used only by PROXIMO.
- The visible list is sorted by name, then desc (`gncBillTerm.c:629`). Lookup by name searches visible terms only.
- GUI: a name is required and must be unique among visible terms (`5.17:gnucash/gnome/dialog-billterms.c:322`); due
  days ≥ discount days (`dialog-billterms.c:287`); it refuses to delete a term with refcount > 0
  (`dialog-billterms.c:670`). After an edit it calls `gncBillTermChanged` (clears `child`, `dialog-billterms.c:360`).
- Parent and child copies work as in §2.2 (`gncBillTermReturnChild`, `gncBillTerm.c:595`). The copy includes name,
  desc, type, due_days, disc_days, discount and cutoff (`gncBillTerm.c:571`). At post the **invoice's** terms are
  replaced by the child (`gncInvoice.c:1470`). Unpost does **not** put them back (oracle §12.2).

### 3.2 Due date computation

`gncBillTermComputeDueDate(term, post_date)`. A null term gives `post_date` unchanged (`gncBillTerm.c:823`). All
dates are local-time calendar dates. The result is the **day-neutral** time of the due date (10:59:00 UTC in most
time zones; see the dates note).

```
fn due_date(term, post: time64) -> time64:
    d = local_date(post)
    match term.type:
      DAYS:    return neutral(d + term.due_days days)                 // gncBillTerm.c:808
      PROXIMO: (m, y) = proximo_month(term, d)                        // gncBillTerm.c:812
               day = min(term.due_days, last_day_of_month(m, y))      // "if (days < day) day = days"
               return neutral(day, m, y)

fn proximo_month(term, d):                                            // compute_monthyear, gncBillTerm.c:751
    cutoff = term.cutoff
    if cutoff <= 0: cutoff += last_day_of_month(d.month, d.year)      // counted back from the posting month's end
    m = d.month + (1 if d.day <= cutoff else 2)
    y = d.year; if m > 12: m -= 12; y += 1
    return (m, y)
```

`last_day_of_month` uses the Gregorian leap rule (`5.17:libgnucash/engine/gnc-date.cpp:413`). A PROXIMO `due_days`
of 0 or less is passed straight to `gnc_dmy2time64_neutral(day ≤ 0, …)`. Behaviour is undefined: avoid it, or refuse
it in red-ale.

Oracle results (§12.2):

| Term | Post date | Due |
|---|---|---|
| DAYS 30 | 2026-01-15 | 2026-02-14 |
| DAYS 0 | 2010-06-14 | 2010-06-14 |
| PROXIMO due 20, cutoff 19 | 2010-06-14 | 2010-07-20 |
| ″ | 2010-06-19 (day = cutoff) | 2010-07-20 |
| ″ | 2010-06-22 | 2010-08-20 |
| ″ | 2010-11-20 | 2011-01-20 |
| ″ | 2010-12-25 | 2011-02-20 |
| PROXIMO due 31, cutoff −3 | 2011-01-10 (cutoff 28) | 2011-02-28 (clamped) |
| ″ | 2011-01-29 | 2011-03-31 |
| ″ | 2011-02-25 (cutoff 25) | 2011-03-31 |
| ″ | 2011-02-26 | 2011-04-30 (clamped) |
| PROXIMO due 15, cutoff 0 (= month end) | 2012-02-28 / 2012-02-29 | 2012-03-15 |

The source comment's "Example 2 … 20-02-2010" is a typo for 20-08-2010 (`gncBillTerm.c:745`).

**Who calls it:** the engine never does. `gncInvoicePostToAccount` takes `due_date` as an argument. The GUI's post
dialog computes it from the invoice's (pre-post, parent) terms and the chosen post date. When the invoice has terms,
this **overrides** any due date the user typed (`5.17:gnucash/gnome/dialog-date-close.c:94`, `:184`). Without terms,
the user's due date is used. Its default is the post date (`dialog-invoice.c:1072`).

---

## 4. Entries (invoice/bill lines)

### 4.1 Fields

From `5.17:libgnucash/engine/gncEntry.c:42`. Defaults are from `gncEntryCreate` (`gncEntry.c:413`); GObject
zero-init for the rest.

| Field | Meaning | Default |
|---|---|---|
| date | line date (time64). The GUI stores local noon (`time64CanonicalDayTime`, `gncEntry.c:514`). | 0 |
| date_entered | entry time. The GUI sets it to now (`gncEntryLedger.c:955`). | 0 |
| desc, action, notes | strings. `action` is a free unit label ("Hours", "Material", …). | `""` |
| quantity | Q. **Stored negated for credit notes** (`gncEntrySetDocQuantity`, `gncEntry.c:562`). | 0 |
| i_account | income account (customer docs) | null |
| i_price | unit price (customer docs) | 0 |
| i_discount | Q | 0 |
| i_disc_type | VALUE / PERCENT | **PERCENT** |
| i_disc_how | PRETAX=1 / SAMETIME=2 / POSTTAX=3 (strings `PRETAX`, `SAMETIME`, `POSTTAX`) | **PRETAX** |
| i_taxable | bool | **true** |
| i_taxincluded | bool | false |
| i_tax_table | TaxTable ref (refcounted) | null |
| b_account, b_price, b_taxable (**true**), b_taxincluded, b_tax_table | the same for bills and vouchers. **Bills have no discount.** | |
| billable | bool: bill line to re-invoice to `billto` | false |
| billto | Owner | type CUSTOMER, null |
| b_payment | CASH=1 / CARD=2 (strings `CASH`, `CARD`), vouchers only | **CASH** |
| order, invoice, bill | back references (an entry may sit on an order **and** an invoice) | null |

One entry has both an invoice side (`i_*`) and a bill side (`b_*`). Which side counts depends on the document:
`is_cust_doc = (end owner type == CUSTOMER)`.

Invoices keep `entries` sorted by `gncEntryCompare`: date, then date_entered, desc, action, GUID
(`gncEntry.c:1647`). Sorting happens at insert and on `SetDate` (not the first SetDate, `gncEntry.c:494`). Changing
the description does **not** re-sort. For a book loaded from XML the order is therefore (date, date_entered, desc,
action, GUID) as of load. Posting iterates in this order (§6.4).

Entries with KVP set the `Extra data …` feature on commit (`gncEntry.c:1635`).

### 4.2 Value computation (`gncEntryComputeValueInt`, `gncEntry.c:1131`)

Inputs: qty, price, tax_table (null when `!taxable`), tax_included, discount, disc_type, disc_how. The bill side
always passes discount 0, VALUE, PRETAX (`gncEntry.c:1424`). All arithmetic is exact rational. The calls carry
`GNC_HOW_RND_ROUND` (banker's) only as an overflow fallback.

```
aggregate = qty * price
tvalue    = Σ amount  over table entries of type VALUE
tpercent  = (Σ amount over entries of type PERCENT) / 100
if tax_table && tax_included:
    pretax    = (aggregate - tvalue) / (1 + tpercent)
    net_price = qty != 0 ? pretax / qty : price
else:
    pretax    = aggregate
    net_price = price

match disc_how:
  PRETAX | SAMETIME:
      if disc_type == PERCENT: discount = pretax * (discount / 100)   // VALUE: discount used as-is (whole line)
      value = pretax - discount
      if PRETAX: pretax = value            // tax base becomes the discounted amount
  POSTTAX:
      if disc_type == PERCENT: discount = (pretax + pretax*tpercent + tvalue) * (discount / 100)
      value = pretax - discount            // tax base stays the undiscounted pretax
  // unknown how: value is left unset (PWARN)

taxes = []                                 // list of (account, amount), one per distinct account
for te in table.entries (table order):
    t = te.type == VALUE ? te.amount : pretax * (te.amount / 100)
    taxes.add(te.account, t)               // gncAccountValueAdd: sums into an existing account, else PREPENDS
return value, discount, taxes, net_price
```

`value` is what the seller earns; `value + Σ taxes` is what the customer pays. With tax-included and a discount the
customer therefore pays less than `aggregate` (oracle row "incl, PRETAX 10%" in §12.3).

**Rounding** (`gncEntryRecomputeValues`, `gncEntry.c:1362`). `denom` is the **document currency's fraction**; if
the entry is on no invoice or bill, 100000 (`gncEntry.c:1341`). The `SCU` argument of `gncEntryComputeValue` is
unused (`G_GNUC_UNUSED`, `gncEntry.c:1333`).

```
value_rounded    = round(value, denom)
disc_rounded     = round(discount, denom)
tax_total        = Σ taxes (exact)
tax_rounded      = fold(0, |acc, t| round(acc + t, denom))   // rounds each running sum, not each term
```

For credit notes the stored quantity is negative, so the internal results are negative too. The accessors flip
signs (`gncEntry.c:1535`–`1605`):

| Accessor family | Returned sign |
|---|---|
| `Doc*(entry, round, is_cust_doc, is_cn)` | internal × (is_cn ? −1 : 1). Always "as printed on the document". |
| `Bal*(entry, round, is_cust_doc)` | internal × (is_cust_doc ? −1 : 1). Sign of the effect on the income or expense account. |

The bill side's discount value is always 0 (`gncEntry.c:1488`).

`gncEntryGetPrice(entry, cust_doc, net=true)` = `round(net_price, denom)`. It is used only when posting billable bill
lines (§6.2) (`gncEntry.c:1502`).

### 4.3 Worked examples (all oracle-verified, §12.1 and §12.3; USD, denom 100)

**A. PRETAX percent discount, tax excluded.** qty 3, price 33.33, discount 10 % PRETAX, table GST 5 % + PST 7.5 %.
aggregate = 99.99; discount = 9.999; value = 89.991 → **89.99**; tax base = 89.991; GST = 4.49955, PST = 6.749325;
tax = 89991/8000 = 11.248875. tax_rounded = round(round(4.49955) + 6.749325) = round(4.50 + 6.749325) = **11.25**.
discount_rounded = 10.00.

**B. Tax included, mixed VALUE and PERCENT tax.** qty 4, price 5, table Fee VALUE 2.00 + VAT 10 %, tax included.
aggregate = 20; pretax = (20 − 2) / 1.1 = 180/11 = 16.3636…; value = 180/11 → **16.36**; taxes: Fee 2, VAT 18/11;
tax = 40/11 → **3.64**; net price = 45/11 → 4.09. Check: 180/11 + 40/11 = 20.

**C. Other discount modes** (qty 4, price 5, Fee 2.00 + VAT 10 %, tax excluded):

| Mode | value | discount | tax |
|---|---|---|---|
| PRETAX VALUE 3 | 17 | 3 | 2 + 1.7 = 3.70 |
| SAMETIME 10 % | 18 | 2 | 2 + 2.0 = 4.00 |
| PRETAX 10 % | 18 | 2 | 2 + 1.8 = 3.80 |
| POSTTAX 10 % | 17.60 | (20+2+2)×10 % = 2.40 | 2 + 2.0 = 4.00 |
| POSTTAX VALUE 3 | 17 | 3 | 4.00 |
| tax incl, PRETAX 10 % | 162/11 → 14.73 | 18/11 | 2 + 162/110 = 191/55 → 3.47 |

### 4.4 GUI defaults for a new line (not engine, but users' books reflect them)

From `5.17:gnucash/register/ledger-core/gncEntryLedgerLoad.c:375`. These apply to invoices and bills, not vouchers:

- date = last date entered in the ledger; date_entered = now.
- price = the job's rate if the document owner is a job, else 0.
- tax-included: owner `taxincluded` YES → true, NO → false, USEGLOBAL → user pref `tax-included` (default **true**,
  `gnucash/gschemas/org.gnucash.GnuCash.dialogs.business.gschema.xml.in:105`, `:147`).
- tax table = book default for the owner type. If the owner has `taxtable_override`, the owner's table instead.
  `taxable = (table != null)`.
- customer documents: discount = customer `discount` (type PERCENT and how PRETAX from the entry defaults).

---

## 5. Invoices, bills, vouchers and credit notes

### 5.1 Model (`5.17:libgnucash/engine/gncInvoice.c:49`, defaults `:135`, `:313`)

| Field | Notes | Default |
|---|---|---|
| id | document number (§1.7). Not unique-checked. | `""` |
| owner | Owner: customer, job, vendor or employee | none |
| billto | Owner. "Bill to" customer or job for re-billing vendor costs. Stored only. | type CUSTOMER, null |
| job | unused QOF field. The job lives in `owner` when the owner is a job. | null |
| notes | | `""` |
| billing_id | the customer's reference (PO number) or the vendor's invoice number | `""` |
| active | | true |
| terms | BillTerm ref (refcounted). Becomes the frozen child at post. | null |
| date_opened | time64 | **INT64_MAX** (unset) |
| date_posted | time64. INT64_MAX when not posted. | INT64_MAX |
| currency | commodity | null |
| to_charge_amount | Q, vouchers only: extra amount charged to the employee's card | 0 |
| entries | sorted list (§4.1) | |
| posted_acc, posted_txn, posted_lot | set by post, cleared by unpost. Each is write-once (`g_return_if_fail(old == NULL)`, `gncInvoice.c:628`). | null |
| prices | in-memory exchange rates used at post (§6.6). **Not persisted.** | |
| KVP `credit-note` | int64 0/1 (`GNC_INVOICE_IS_CN`, `gncInvoice.c:86`) | absent |
| KVP `assoc_uri` | document link (`gncInvoice.c:87`). Empty string removes it. | absent |

- **Document type** (`gncInvoiceGetType`, `gncInvoice.c:1065`): end owner type × credit-note flag →
  `CUST_INVOICE=1, VEND_INVOICE=2, EMPL_INVOICE=3, CUST_CREDIT_NOTE=4, VEND_CREDIT_NOTE=5, EMPL_CREDIT_NOTE=6`,
  `UNDEFINED=0` (`gncInvoice.h:61`).
- **Type string** (`gncInvoiceGetTypeString`, `gncInvoice.c:1089`) is **translated** (`_()`): `Invoice`, `Bill`,
  `Expense`, `Credit Note` (all three credit-note types). Note that it is "Expense", not "Expense voucher". This
  string goes into lot titles and split actions, so books written by a localised GnuCash contain translated words
  (§11).
- `gncInvoiceSetIsCreditNote(inv, b)` always writes the slot (`0` or `1`). With `b = true` it sets book feature
  **`Credit Notes`** (`gncInvoice.c:576`). The GUI calls it for every new or duplicated document, so GUI-made invoices
  carry `credit-note = 0` (`dialog-invoice.c:433`). Python-made ones in §12.1 do not. Milestone 1 opens books with
  this feature read-only (`design/milestone1.md` "Compatibility in v0.1").
- "Amount positive" (`gncInvoiceAmountPositive`, `gncInvoice.c:1287`): true for customer invoices and vendor or
  employee credit notes. It describes the sign of the posted A/R or A/P balance.
- **Is posted** = `posted_txn` is a transaction (`gncInvoice.c:1981`). **Is paid** = the posted lot exists and is
  closed, i.e. the sum of its split *amounts* is 0 (`gncInvoice.c:1987`, `gnc-lot.cpp:487`).
- **Due date** = the posted transaction's `trans-date-due` slot. INT64_MAX when not posted (`gncInvoice.c:849`).
- Totals (`gncInvoice.c:917`–`1030`), all **document-signed**:
  - `Subtotal` = Σ_entries round(value). Each entry is rounded first; the sum is exact.
  - `TaxList` = per tax account, Σ_entries of the *unrounded* tax, **then rounded once per account**
    (`gncInvoice.c:965`).
  - `Tax` = Σ TaxList. `Total` = Subtotal + Tax.
  - `TotalOf(payment type)` = the same, restricted to entries with that `b_payment`.
  - Note that `TaxList` is **not** Σ of the per-entry rounded taxes. Example §12.1: entry-rounded taxes would sum to
    47.63 or 47.65 depending on grouping. The posted GST is 19.06 and PST 28.58 (= round(19.055106),
    round(28.582658)).
- **Next document number**: §1.7. The GUI assigns it on OK or post when the ID field is empty
  (`dialog-invoice.c:478`).

### 5.2 Copy (duplicate)

`gncInvoiceCopy` (`gncInvoice.c:336`) copies the id, notes, billing_id, active, the credit-note slot, terms (IncRef),
owner, billto, job, to_charge, date_opened, currency, doc link and all entries (new GUIDs via `gncEntryCopy`). It
does not copy prices, posted state or date_posted. The GUI's duplicate then sets a **new ID from the counter**, sets
active and the dates (`dialog-invoice.c:3193`–`3233`).

### 5.3 GUI invariants worth enforcing in the API

- An owner is required (`dialog-invoice.c:463`). Posting requires ≥ 1 entry (`dialog-invoice.c:1133`).
- The invoice currency is set from the **owner's currency** on save and again just before post
  (`dialog-invoice.c:414`, `:1168`). A new invoice starts with the book's default currency (`dialog-invoice.c:2967`).
- A new invoice takes the owner's terms (`dialog-invoice.c:2981`).
- Post account: the type comes from §1.1 and the commodity must be the owner's currency
  (`gncOwnerGetCommoditiesList`, used at `dialog-invoice.c:1046`). The GUI pre-selects the owner's
  `last-posted-to-acct`.

---

## 6. Post to accounts (`gncInvoicePostToAccount`, `gncInvoice.c:1442`)

Signature: `(invoice, acc, post_date, due_date, memo, accumulatesplits, autopay) → Transaction`. It returns null if
the invoice is already posted.

### 6.1 Caller-side defaults (GUI)

- post_date defaults to now for customer documents. For bills and vouchers it is the latest entry date
  (`dialog-invoice.c:1056`). due_date: §3.2. memo: empty.
- accumulatesplits = user pref `invoice/accumulate-splits`, default **true**. The user can change it per post
  (`dialog-invoice.c:1083`, gschema `:140`).
- autopay = user pref `invoice/auto-pay` (customer documents) or `bill/auto-pay` (others), default **false**
  (`dialog-invoice.c:1281`, gschema `:110`, `:152`).
- Foreign-currency lines: §6.6.

### 6.2 Algorithm

```
assert !posted
is_cust_doc = end_owner.type == CUSTOMER;  is_cn = credit-note slot
if invoice.terms: invoice.SetTerms(ReturnChild(terms, true))      // freeze terms (§2.2)
ccard = (end owner is an employee) ? employee.ccard_acc : null
lot = new Lot; AttachToLot(invoice, lot)                          // lot slot gncInvoice/invoice-guid; invoice.posted_lot
type = TypeString(invoice)                                        // "Invoice" | "Bill" | "Expense" | "Credit Note"
lot.title = "{type} {invoice.id}"                                 // lot slot "title"
txn = new Transaction
txn.description = end_owner.name or ""
gnc_set_num_action(txn, null, num=invoice.id, action=type)        // §6.3
txn.currency = invoice.currency
txn.date_entered = now
txn.SetDatePostedSecsNormalized(post_date)                        // neutral time plus "date-posted" gdate slot
invoice.date_posted = txn.date_posted
txn.SetDateDue(due_date)                                          // slot "trans-date-due" (timespec)

total = invoice.Total()         // document-signed, rounded (§5.1)
taxes = invoice.TaxList()       // document-signed, rounded per account
if is_cust_doc XOR is_cn: total = -total; negate each tax           // convert to balance sign

splitinfo = []
for entry in invoice.entries (sorted order):
    if is_cust_doc: entry.SetInvTaxTable(ReturnChild(entry.i_tax_table, true))   // freeze (§2.2)
    else:
        entry.SetBillTaxTable(ReturnChild(entry.b_tax_table, true))
        if entry.billable:                           // copy the cost to the invoice side for re-billing
            entry.SetInvPrice(entry.GetPrice(cust_doc=false, net=true))   // net (pre-tax) bill price, rounded
            entry.SetInvTaxIncluded(false)
    value = entry.BalValue(round=true, is_cust_doc)
    acct  = is_cust_doc ? entry.i_account : entry.b_account
    if acct:                                         // lines with no account still count in total → see §6.7
        if accumulatesplits: splitinfo.add(acct, value)            // sums per account; NEW accounts PREPENDED
        else: AddSplit(acct, value, memo = entry.desc)             // appended now, in entry order
        if ccard && entry.b_payment == CARD:
            total = round(total - value, scu(acc))
            append split: account ccard, memo entry.desc, action/num as below, BaseValue = -value
splitinfo.addList(taxes)                             // tax accounts merged in; new ones PREPENDED
for (a, v) in splitinfo: AddSplit(a, v, memo = memo)               // the caller's memo
if ccard && to_charge_amount != 0:
    c = is_cn ? -to_charge : to_charge
    append split: account ccard, memo _("Extra to Charge Card"), BaseValue = -c
    total = round(total - c, scu(acc))
append posted split: account acc, memo = memo, BaseValue = -total; lot.add(this split)
txn: slot gncInvoice/invoice-guid = invoice.guid; slot trans-txn-type = "I"; invoice.posted_txn = txn
invoice.posted_acc = acc
txn slot trans-read-only = _("Generated from an invoice. Try unposting the invoice.")
commit txn                                            // sorts splits: value >= 0 first, then < 0 (stable)
if autopay: gncInvoiceAutoApplyPayments(invoice)      // §8.5
```

`AddSplit` (`gncInvoicePostAddSplit`, `gncInvoice.c:1371`): new split with the given memo; action/num via
`gnc_set_num_action(null, split, invoice.id, type)`; inserted into the account and appended to the transaction. If the
account commodity equals the invoice currency: `SetBaseValue(value)`, so amount = value. Otherwise see §6.6.

Sign summary for the A/R or A/P split: customer invoice +total (debit A/R); customer credit note −; vendor bill and
employee voucher −total (credit A/P); their credit notes +. Income lines are credits (negative) on customer invoices.
Expense lines are debits (positive) on bills.

### 6.3 Strings, slots and order

- **num / action** (`5.17:libgnucash/engine/engine-helpers.c:116`). With the default book option (split-action
  off): `txn.num = invoice.id` and every split's action = type string. With book option "Use Split Action Field for
  Number" (feature `Number Field Source`, read-only in milestone 1): `txn.num = type` and split action = invoice id.
  The option is read from the **current session's** book, not the invoice's book (`engine-helpers.c:119`).
- **Memos:** the posted split and accumulated splits (including tax splits) get the caller's `memo` (empty string by
  default; the XML then has no `<split:memo>`, see §12.2 TXN3). Non-accumulated entry splits and card splits get the
  entry description. The extra-charge split gets "Extra to Charge Card".
- **Transaction slots** (oracle §12.1 TXN3, in file order): `date-posted` (gdate), `gncInvoice/invoice-guid`
  (guid), `trans-date-due` (timespec), `trans-read-only` (string), `trans-txn-type` = `"I"`
  (`Transaction.cpp:1963`, `gnc-engine.h:257`).
- **Lot slots** (§12.1 LOT1): `gncInvoice/invoice-guid` and `title`. A document lot has **no** `gncOwner` frame.
  Its owner is found via the invoice (`gncOwner.c:711`).
- **Split order in the file** = order after commit. `xaccTransCommitEdit` stable-sorts by `value < 0`, so
  non-negative values come first (`5.17:libgnucash/engine/Transaction.cpp:540`, `:1510`). Before the sort, the
  append order is:
  1. non-accumulated entry splits and card splits, interleaved in entry order;
  2. `splitinfo` order. Building it by prepending gives: tax accounts first, in the reverse of the order they
     appear in `TaxList`. `TaxList`'s own order comes from a double prepend: for a single tax table it is the
     *reverse* of the table's account-name order, so the final tax splits are in account-name order. Then income or
     expense accounts, **reverse** of first appearance in entry order;
  3. the extra-charge split;
  4. the posted (A/R or A/P) split.

  Oracle §12.1: `A/R +427.50 | GST −19.06 | PST −28.58 | Sales −100.00 | Services −279.86`. GnuCash does not care
  about split order when it reads a file, but matching this order keeps red-ale output byte-identical to GnuCash for
  the same operation.

### 6.4 Accumulation

With `accumulatesplits`, one split per income or expense account carries the **sum of per-entry rounded values**,
which is exact. Tax is always one split per tax account (taxes are always accumulated). Without it, one split per
entry with an account. Tax splits are still per account.

### 6.5 Expense vouchers (employee) and the credit card

- Entries use the bill side (`b_*`). Each entry's `b_payment` is CASH or CARD.
- If the employee has `ccard_acc`: each CARD entry adds a split crediting `ccard_acc` by its value (memo = entry
  description) and lowers the A/P amount. `to_charge_amount` adds a further card credit ("Extra to Charge Card") and
  also lowers A/P (§12.2 TXN9: Supplies +40, Visa −25 "Taxi", Visa −5 extra, A/P −10). The comment says vouchers have
  no taxes, so only values are moved to the card (`gncInvoice.c:1599`).
- With no ccard account, CARD entries are treated like CASH.

### 6.6 Foreign-currency lines

If a line's account (or a tax account) has a commodity ≠ the invoice currency, the invoice must hold a price for that
commodity, or posting **fails and returns null halfway** (`gncInvoice.c:1418`). Then `split.value = value` and
`split.amount = value / price.value` (HALF_UP, then rounded to the account SCU by SetAmount). In the GUI the user
enters, per foreign commodity, a rate *invoice currency → account currency*. GnuCash stores the inverse as a
temporary `GNCPrice` (source `temporary`, type `last`) on the invoice only. It is **not** added to the price DB
(`dialog-invoice.c:1176`–`1258`). red-ale's API must take these rates as inputs.

### 6.7 Failure modes to refuse up front in red-ale

GnuCash fails late, leaving partial state, when: an account has no price (above); or the posting account is null.
An entry with no account is silently left out of the splits but still counted in `total`, which makes the
transaction unbalanced (the engine then adds an Imbalance split at commit). The GUI prevents both. red-ale should
validate everything before mutating, to keep posting atomic (CLAUDE.md "Every mutation is atomic").

---

## 7. Unpost (`gncInvoiceUnpost(invoice, reset_tax_tables)`, `gncInvoice.c:1735`)

```
if !posted: return false
txn = invoice.posted_txn; lot = invoice.posted_lot
clear txn read-only; destroy txn                       // its splits leave their accounts and the lot
DetachFromLot(lot)                                     // sets lot property "invoice" to NULL → empty frame "gncInvoice" stays (§12.2)
OwnerAttachToLot(invoice.owner, lot)                   // lot slots gncOwner/owner-type (int), gncOwner/owner-guid
for split in copy(lot.splits):                         // what is left: payments, links
    t = split.transaction
    if t.txn_type != 'L': continue                     // payment ('P') splits stay in this lot
    other_lots = [s.lot for s in t.splits if s.lot != lot] (original order)
    clear read-only; destroy t                         // the lot-link transaction is removed entirely
    gncOwnerAutoApplyPaymentsWithLots(invoice.owner, other_lots)   // re-balance the other documents (§8.4)
    for l in other_lots: if l.split_count == 0: destroy l  (else notify its invoice)
if lot.split_count == 0: destroy lot
invoice.posted_acc = posted_txn = posted_lot = null; date_posted = INT64_MAX
if reset_tax_tables:
    for entry: set (is_cust_doc ? i_ : b_)tax_table = GetParent(current)   // back to the visible parent; IncRefs it
```

Kept: the invoice, its ID, entries, notes and dates opened. Kept: the **frozen terms child** (terms are not reset).
Kept: the child tax tables (invisible, refcount 0, still the parent's `child`, so the next post reuses them). Kept:
payment transactions.

If payments had been applied, the old lot **survives as a pre-payment lot**. It keeps the payment splits, gains
`gncOwner/*` slots, keeps its `title` ("Invoice 000001") and keeps an **empty** `gncInvoice` frame. Oracle §12.2 LOT2:

```xml
<slot><slot:key>gncInvoice</slot:key><slot:value type="frame"/></slot>
<slot><slot:key>gncOwner</slot:key><slot:value type="frame">
  <slot><slot:key>owner-guid</slot:key><slot:value type="guid">…customer…</slot:value></slot>
  <slot><slot:key>owner-type</slot:key><slot:value type="integer">2</slot:value></slot></slot:value></slot>
<slot><slot:key>title</slot:key><slot:value type="string">Invoice 000001</slot:value></slot>
```

The `reset_tax_tables` argument: the GUI asks the user with two radio buttons. The default is **"No, keep them as they
are"**, i.e. false (`dialog-invoice.c:296`, `5.17:gnucash/gtkbuilder/dialog-invoice.glade:1426`). Duplicating a posted
invoice unposts the copy with `TRUE` (`dialog-invoice.c:3198`).
Note that `GetParent(null)` gives null, so untaxed lines stay untaxed.

---

## 8. Process payment

### 8.1 Entry points

- `gncOwnerApplyPaymentSecs(owner, &txn?, lots, posted_acc, xfer_acc, amount, exch, date, memo, num, auto_pay)`
  (`gncOwner.c:1404`). This is the GUI payment dialog path (`dialog-payment.c:1031`).
  - Selected lots = `lots` if given. Else, if auto_pay, **all open lots in `posted_acc` whose end owner equals
    `owner`**, either via their invoice or via the `gncOwner` slots (`gncOwnerLotMatchOwnerFunc`, `gncOwner.c:705`).
    The order is the account's lot-list insertion order (`Account.cpp:3970`): for a loaded book that is file order,
    which is GUID-sorted (§11).
  - If `amount ≠ 0`: create the payment lot (§8.2) and prepend it to the list. Then
    `gncOwnerAutoApplyPaymentsWithLots(owner, list)` (§8.4).
  - `amount = 0` is allowed: it only links the selected documents with each other.
- `gncInvoiceApplyPayment(invoice, txn, xfer_acc, amount, exch, date, memo, num)` (`gncInvoice.c:1950`) is the same,
  with lots = [payment lot, invoice lot] and owner = the invoice's end owner.

**Sign of `amount`:** it is the amount of the **transfer** (bank) split in the posted-account commodity: positive =
money in. The A/R or A/P split is −amount. The GUI computes `amount = credit − debit`. For customers the "Payment"
field is the credit edit (positive) and "Refund" the debit; for vendors and employees they are swapped
(`dialog-payment.c:267`, `:680`). So a customer payment is positive and a vendor payment negative. The GUI rounds the
amount to the post account's commodity fraction (HALF_UP).

### 8.2 Payment transaction (`gncOwnerCreatePaymentLotSecs`, `gncOwner.c:749`)

```
txn = preset_txn ?: new            // preset (editing an existing payment): every split except the existing xfer split is destroyed
txn.description = end_owner.name or ""
if txn.currency not in {post_comm, xfer_comm}: txn.currency = xfer_comm   // a NEW txn always gets the xfer account's commodity
xfer_amount = post_comm == xfer_comm ? amount : amount * exch             // exact (HALF_UP fallback) …
txn_value   = post_comm == txn.currency ? amount : amount * exch          // … then rounded by SetAmount/SetValue to SCU/fraction
if no xfer split:
    s = new split(xfer_acc); memo = memo; gnc_set_num_action(null, s, num, _("Payment"))
    s.amount = xfer_amount; s.value = txn_value
elif amount or value changed: update both; reconcile state = 'n'
p = new split(posted_acc); memo = memo; p.action = _("Payment")            // always the literal, never num
p.amount = -amount; p.value = -txn_value
payment_lot = new Lot; OwnerAttachToLot(owner, payment_lot); payment_lot.add(p)   // slots gncOwner/owner-type, owner-guid; no title
txn.num = num; slot trans-txn-type = "P"
txn.date_entered = now; txn.SetDatePostedSecs(date)   // NOT normalised: no "date-posted" gdate slot
commit                                                // the sign sort applies
```

`exch` = units of the **transfer** commodity per 1 unit of the posted-account commodity. The GUI asks for it only
when the two commodities differ, and defaults to 1/1 (`dialog-payment.c:977`, `:1000`). The GUI's `date` is the
day-neutral time of the chosen date (`gdate_to_time64`, `gnc-date.cpp:1323`). Oracle §12.2 TXN7: vendor payment
−65.00 USD from an EUR bank at exch 0.9. The transaction currency is EUR; the A/P split has amount 65.00 (USD) and
value 58.50 (EUR); the bank split has −58.50/−58.50.

### 8.3 Lot linking primitives

- **OffsetLots(from = payment lot, to = lot)** (`gncOwner.c:1221`): `target = balance(to)`. Pick a split in `from`
  with `FindOffsettingSplit(from, target)`. If |split.amount| > |target|, `ReduceSplitTo(split, −target)`. Move the
  split into `to`. `from` must not be a document lot.
- **FindOffsettingSplit** (`gncOwner.c:896`): over the splits in `from` whose amount has the opposite sign to the
  target (`positive_p` differs; note a 0 amount counts as "not positive"):
  `flags = (|amt| == |target| ? 8 : |amt| > |target| ? 4 : 2) + (txn type != 'L' ? 1 : 0)`.
  Take the split if `flags >= best_flags && |amt| > |best_amt|` (best starts as 0/1 and flags 0). The first strictly
  larger amount wins ties. This is literal: a later split with better flags but a smaller amount is **not** taken.
- **ReduceSplitTo(split, target_amt)** (`gncOwner.c:958`): same sign required.
  `exch = value/amount` (exact LCD); `new_val = round(target_amt × exch, txn currency fraction)`. Set the split's
  amount and value. A **remainder split** (`xaccSplitCopyOnto`: same account, memo and action) gets
  `amount − target` and `value − new_val`. It is appended to the same transaction and **added to the split's
  current lot** (the payment lot). The reduced original then moves on.
- **CreateLotLink(from_doc_lot, to_doc_lot, owner)** (`gncOwner.c:1121`), for two document lots of opposite sign:
  offset = the smaller |balance|. Reuse an existing 'L' transaction already in either lot (it must link to another
  lot; see the code's "lot link proliferation" note, `gncOwner.c:1075`), or create one:
  description = end owner name or `"(Unknown)"`; currency = the account's commodity; date posted = the later of the
  two lots' latest-split transaction dates, set with `SetDatePostedSecs`, so there is no gdate slot. The date is
  raised if the reused transaction is older. Slot `trans-txn-type` = `"L"`. **No num.** Two splits, both with action
  `_("Lot Link")` (set via `gnc_set_num_action` with num = null, so with split-action on, action is not set). BaseValue
  is −from_bal and −to_bal; each split is added to its lot. Then `xaccScrubMergeLotSubSplits` on both lots, and
  `gncOwnerSetLotLinkMemo`: every split of the 'L' transaction that is in a document lot gets memo
  `_("Offset between business items: ") + join(sorted(["{type} {id}" …]), " - ")` (`gncOwner.c:1009`). Oracle
  §12.2 TXN5: `Offset between business items: Credit Note CN1 - Invoice INV3`.

### 8.4 `gncOwnerAutoApplyPaymentsWithLots(owner, lots)` (`gncOwner.c:1256`)

```
for i, left in enumerate(lots):
    if left is null: continue
    if left.split_count == 0: destroy left; lots[i] = null; continue
    if left.is_closed: continue
    left_bal = balance(left); left_doc = left has invoice
    for right in lots[i+1..]:
        skip null / destroy-if-empty / closed
        skip if right.account != left.account
        right_bal = balance(right)
        skip if positive_p(left_bal) == positive_p(right_bal)       // must be opposite signs
        if left_doc and right_doc:   CreateLotLink(left, right, owner)
        elif !left_doc and !right_doc:
            if |left_bal| >= |right_bal|: OffsetLots(left, right) else OffsetLots(right, left)
        else: OffsetLots(pay_lot, doc_lot)
        // NOTE: left_bal is NOT recomputed inside the inner loop
```

Note that `left_bal` is cached for the whole inner loop, so after a partial offset, sign checks against later lots
use the stale balance. red-ale must reproduce this to stay byte-identical.

### 8.5 Auto-apply at post (`gncInvoiceAutoApplyPayments`, `gncInvoice.c:1907`)

lots = [invoice lot] + open lots in the posted account with a balance of the opposite sign and the same end owner
(the document's or the `gncOwner` slots, `gncInvoice.c:1880`). Then §8.4. This consumes pre-payments and links
opposite documents (credit notes).

### 8.6 Worked flow (oracle §12.1 and §12.2)

1. Invoice 000001, total 427.50, posted. A/R lot LOT1 balance +427.50.
2. `gncInvoiceApplyPayment` of 100.00: the payment lot's split (−100) is smaller than the target, so the **whole split
   moves** into LOT1. The payment lot becomes empty, `gnc_lot_remove_split` drops it from the account
   (`gnc-lot.cpp:644`), and it is not written. TXN1: `Bank +100 | A/R −100 [LOT1]`, num `CHK-1`, both actions
   `Payment`, memo `partial memo`.
3. Owner payment of 500.00 with auto_pay: the payment lot (−500) is offset against LOT1 (327.50 left). The split is
   reduced to −327.50 (into LOT1) and the remainder −172.50 stays in the payment lot: an **over-payment, i.e. a
   pre-payment lot** with `gncOwner` slots. The invoice is paid.
4. Invoice 000002 (50.00) posted with autopay: the pre-payment split is reduced to −50 (into the invoice 2 lot) and a
   new remainder of −122.50 stays in the payment lot. TXN2 ends with four splits:
   `Bank +500 | A/R −327.50 [LOT1] | A/R −50 [inv 2 lot] | A/R −122.50 [payment lot]`.

---

## 9. Due reminders

GUI only, run at book open by the hook `HOOK_BOOK_OPENED` (`5.17:gnucash/gnome/top-level.c:452`):
`gnc_invoice_remind_bills_due_cb` and `gnc_invoice_remind_invoices_due_cb` (`dialog-invoice.c:3963`, `:3972`).

- Enabled by **user** prefs (not book data): `bill/notify-when-due` and `invoice/notify-when-due`, both default **true**.
- N = `bill/days-in-advance` or `invoice/days-in-advance`: a double, default **7.0**, **truncated to int** on read
  (`gint days = gnc_prefs_get_float(…)`, `dialog-invoice.c:3945`, `:3958`). Negative → 0 (`dialog-invoice.c:3881`).
- Query (`gnc_invoice_show_docs_due`, `dialog-invoice.c:3780`): every invoice with
  - `is_posted == true`, AND
  - `posted_lot.is_closed == false` (not fully paid), AND
  - bills (`DUE_FOR_VENDOR`): type ∉ {CUST_INVOICE, CUST_CREDIT_NOTE}, so **vendor bills, employee vouchers and all
    their credit notes**; invoices (`DUE_FOR_CUSTOMER`): type ∉ {VEND_INVOICE, VEND_CREDIT_NOTE, EMPL_INVOICE,
    EMPL_CREDIT_NOTE}, so customer invoices and customer credit notes, AND
  - `due <= now + N × 86400` seconds (full time64 compare, `QOF_DATE_MATCH_NORMAL`), where `due` = the posted
    transaction's `trans-date-due`.
  - Overdue documents are included: there is no lower bound.
- Columns: Due, Company (`owner → parent → name`), Type, Amount (`posted_lot` balance, i.e. the open A/R or A/P
  balance, signed). Sorted by due date ascending. Nothing is shown when the result is empty.

red-ale has no GUI. Expose a read-only query `invoice.due(kind: bills|invoices, days_in_advance: int = 7,
now?)` with exactly these predicates. The prefs are user settings, not book data, so take them as parameters.

---

## 10. Orders

Model (`5.17:libgnucash/engine/gncOrder.c:42`, `:281`): `id ""`, `notes ""`, `reference ""`, `active true`,
`owner`, `entries` (sorted by `gncEntryCompare`), `opened`, and `closed` (**INT64_MAX = open**, `gncOrder.c:99`).

- `IsClosed` = closed ≠ INT64_MAX (`gncOrder.c:477`). Closing = `SetDateClosed(t)`. The GUI asks first when some
  entries are not yet on an invoice, and requires ≥ 1 entry (`5.17:gnucash/gnome/dialog-order.c:258`). Nothing
  reopens an order except setting `closed` back.
- Entries: `gncOrderAddEntry` sets `entry.order`. An entry can be on an order and an invoice at once. Removing an
  entry from an invoice destroys it only if it is on no invoice, bill or order (`gncInvoice.c:785`).
- ID counter `gncOrder` (§1.7).
- **The 5.17 GUI does not expose orders**: the "Customer's Orders" search button is commented out
  (`dialog-customer.c:874`) and the business plugin has no order actions. Orders reach users only from older books or
  other tools. red-ale should store and round-trip them, with at most minimal create/close and entry-attach
  operations.

---

## 11. Rules for red-ale (summary)

1. Arithmetic: exact rationals throughout §4.2. Round only where §4.2, §5.1 and §6 say: HALF_UP to the currency
   fraction, per entry for values, per tax account after summing for taxes. Never use floats.
2. Freeze tax tables and terms via the child mechanism at post. Keep parent and child links, refcounts and
   `invisible` exactly. Reuse `parent.child` when it is set.
3. Translatable strings written into books: `Invoice`, `Bill`, `Expense`, `Credit Note`, `Payment`, `Lot Link`,
   `Offset between business items: `, `Extra to Charge Card`, `Generated from an invoice. Try unposting the invoice.`.
   red-ale writes the C-locale (English) strings. When reading, never rely on them (identify documents by slots and
   transaction type).
4. Counters are book KVP that red-ale must update with each NextID. Keep unknown `counter_formats`, and fall back to
   the default exactly as §1.7 says.
5. Feature flags set as side effects: `Credit Notes` (any credit note); `Extra data in addresses, jobs or invoice
   entries` (KVP on an address, job, entry or tax table, including any job rate). Writing either into a book that
   lacks it changes which GnuCash versions can open the book. Milestone 1 should refuse those edits or set the flag
   exactly as GnuCash would.
6. Determinism hazards inherited from GnuCash, which red-ale must reproduce consistently rather than "fix":
   - lot list order is insertion order: GUID order after a load, then creation order (auto-pay candidates);
   - entry order is (date, date_entered, desc, action, GUID);
   - lots are written sorted by GUID (`5.17:libgnucash/backend/xml/gnc-account-xml-v2.cpp:152`);
   - empty lots are dropped from the account and so from the file;
   - unpost leaves an empty `gncInvoice` frame in the lot.

---

## 12. Oracle runs (GnuCash 5.17, `TZ=UTC`)

### 12.1 exp1: customer, invoice with four lines, post, partial and over-payment, auto-applied second invoice

Setup: accounts Assets:Bank (BANK), Assets:Accounts Receivable, Income:Sales, Income:Services, Liabilities:GST,
Liabilities:PST (all USD). Tax table "GST+PST" = GST 5 % + PST 7.5 %. Term "Net 30" (DAYS 30, disc-days 10,
discount 2). Customer via `CustomerNextID` → `'000001'`; invoice via `InvoiceNextID` → `'000001'`.

Entry results (`Doc*` accessors, unrounded and rounded):

```
Posttax disc         value 355/4    r 8875/100  tax 25/2        r 1250/100  disc 45/4      r 1125/100
Untaxed gadget       value 1001/100 r 1001/100  tax 0/1         r 0/1       disc 0/1       r 0/100
Consulting incl tax  value 1720/9   r 19111/100 tax 215/9       r 2389/100  disc 0/1       r 0/100
Widgets              value 89991/1000 r 8999/100 tax 89991/8000 r 1125/100  disc 9999/1000 r 1000/100
total 42750/100 subtotal 37986/100 tax 4764/100
due 2026-02-14 10:59:00
term after post: invoice terms has parent (child made); parent refcount 2 → 1
entries' tax table = child (refcount 0); parent tt refcount 3 → 0
```

Posted transaction (GUIDs replaced by labels):

```xml
<trn:num>000001</trn:num>
<trn:date-posted><ts:date>2026-01-15 10:59:00 +0000</ts:date></trn:date-posted>
<trn:description>Acme Ltd</trn:description>
<trn:slots>
  <slot><slot:key>date-posted</slot:key><slot:value type="gdate"><gdate>2026-01-15</gdate></slot:value></slot>
  <slot><slot:key>gncInvoice</slot:key><slot:value type="frame">
    <slot><slot:key>invoice-guid</slot:key><slot:value type="guid">INV</slot:value></slot></slot:value></slot>
  <slot><slot:key>trans-date-due</slot:key><slot:value type="timespec"><ts:date>2026-02-14 10:59:00 +0000</ts:date></slot:value></slot>
  <slot><slot:key>trans-read-only</slot:key><slot:value type="string">Generated from an invoice. Try unposting the invoice.</slot:value></slot>
  <slot><slot:key>trans-txn-type</slot:key><slot:value type="string">I</slot:value></slot>
</trn:slots>
splits (memo "post memo", action "Invoice" on all):
  A/R       +42750/100  lot LOT1
  GST        -1906/100
  PST        -2858/100
  Sales     -10000/100   (89.99 + 10.01)
  Services  -27986/100   (191.11 + 88.75)
```

Invoice lot:

```xml
<lot:slots>
  <slot><slot:key>gncInvoice</slot:key><slot:value type="frame">
    <slot><slot:key>invoice-guid</slot:key><slot:value type="guid">INV</slot:value></slot></slot:value></slot>
  <slot><slot:key>title</slot:key><slot:value type="string">Invoice 000001</slot:value></slot>
</lot:slots>
```

Payments:

```
TXN1 num CHK-1 desc "Acme Ltd" 2026-02-01 10:59 slots: trans-txn-type=P only
  Bank +10000/100 memo "partial memo" action Payment
  A/R  -10000/100 memo "partial memo" action Payment lot LOT1       ← whole split moved, payment lot vanished
TXN2 num CHK-2 2026-02-10 (auto_pay, no lots given)
  Bank +50000/100
  A/R  -32750/100 lot LOT1 (invoice 1, now closed)
  A/R   -5000/100 lot LOT3 (invoice 2, applied when invoice 2 was posted with autopay)
  A/R  -12250/100 lot LOT2 (pre-payment lot: slots gncOwner/owner-guid=customer, owner-type=2; no title)
TXN4 invoice 000002 (memo ""): A/R +5000/100 [LOT3] | Services -5000/100; no <split:memo> element written
```

Billing terms after post: parent `refcount 1, invisible 0, child → BT2`; child `refcount 0, invisible 1,
parent → BT1`, same name, desc and days. Tax tables: child `refcount 0, invisible 1, parent → …`; parent `refcount 0,
invisible 0, child → …`. Book slots: `counters/gncCustomer = 1`, `counters/gncInvoice = 2` (integers). No
`features` were set (no credit note, no extra KVP).

### 12.2 exp2: counter formats, PROXIMO, unpost, credit note, bill, foreign payment, voucher

- `counter_formats`: `gncCustomer = "C-%li%%"` → `C-2%`; `gncInvoice = "INV-%04li"` → `INV-0003`;
  `gncVendor = "%d"` → warning, `000001`.
- PROXIMO table: §3.2.
- Unpost invoice 000001 with reset: entries' tax tables are the parent again, terms are still the child, and the old
  lot becomes LOT2 as shown in §7. After reset the parent tax table's refcount is 3 again.
- Credit note CN1 (20.00; stored entry quantity −1/1, `GetTotal` +20.00) posted with autopay against open invoice
  INV3 (30.00):
  ```
  TXN6 num CN1 type I: Services +2000/100 memo "cn" action "Credit Note" | A/R -2000/100 [CN lot, title "Credit Note CN1"]
  TXN5 (no num) desc "Acme Ltd" 2026-04-05 type L:
       A/R +2000/100 [CN lot]   memo "Offset between business items: Credit Note CN1 - Invoice INV3" action "Lot Link"
       A/R -2000/100 [INV3 lot] same memo/action
  ```
  Result: CN paid; INV3 open with 10.00. Book `features/Credit Notes` = "Customer and vendor credit notes (requires
  at least GnuCash 2.5.0)". Invoice slot `credit-note` = integer 1.
- Bill 000001 (counter `gncBill`), two lines, accumulate **off**:
  `Supplies +2500/100 memo "Paper" | Supplies +4000/100 memo "Toner" | A/P -6500/100 memo "bill memo"`, all action
  `Bill`, lot title `Bill 000001`.
- Vendor payment −65 USD from EUR bank, exch 9/10: transaction currency EUR,
  `A/P value 5850/100 qty 6500/100 [bill lot] | EUR Bank -5850/100 -5850/100`, num `W1`, type P. Bill paid.
- Expense voucher 000001 (counter `gncExpVoucher`), employee with ccard Visa, Taxi 25 CARD, Lunch 15 CASH,
  to_charge 5, accumulate on: `Supplies +4000/100 "ev memo" | Visa -2500/100 "Taxi" | Visa -500/100 "Extra to Charge
  Card" | A/P -1000/100 "ev memo"`, all action `Expense`, lot title `Expense 000001`. Invoice element
  `charge-amt 500/100`.

### 12.3 exp3: discount and tax modes (qty 4, price 5, Fee VALUE 2 + VAT 10 %)

```
incl, VALUE+PCT tax          value 180/11 r 1636/100 tax 40/11 r 364/100 disc 0/1  netprice 409/100
excl, PRETAX value-disc 3    value 17/1   r 1700/100 tax 37/10 r 370/100 disc 3/1  netprice 500/100
excl, SAMETIME 10%           value 18/1   r 1800/100 tax 4/1   r 400/100 disc 2/1  netprice 500/100
excl, PRETAX 10%             value 18/1   r 1800/100 tax 19/5  r 380/100 disc 2/1  netprice 500/100
excl, POSTTAX 10%            value 88/5   r 1760/100 tax 4/1   r 400/100 disc 12/5 netprice 500/100
excl, POSTTAX value 3        value 17/1   r 1700/100 tax 4/1   r 400/100 disc 3/1  netprice 500/100
incl, PRETAX 10%             value 162/11 r 1473/100 tax 191/55 r 347/100 disc 18/11 netprice 409/100
qty 1/3 price 1 excl         value 1/3    r 33/100  tax 61/30 r 203/100 disc 0/1  netprice 100/100
```

---

## Open questions

1. **Int64 overflow in §4.2.** `GNC_DENOM_AUTO` with REDUCE or ROUND falls back to banker's rounding when an exact
   result does not fit in 64 bits. Its exact behaviour (which digits survive) is in `GncNumeric` and is not
   reproduced here. Do we refuse such inputs (proposed: an error when any intermediate exceeds i64), or port the
   fallback?
2. **PROXIMO with `due_days <= 0`**: GnuCash passes day 0 or a negative day to `gnc_dmy2time64_neutral`, and the
   result is not checked. Proposed: refuse such terms in the API, but keep and round-trip them.
3. **Card splits on credit-note vouchers**: when `is_cn`, the per-entry CARD split uses `BalValue` (not negated for
   cn) while `to_charge` is negated. Untested. An employee credit note with CARD lines needs an oracle check before
   red-ale supports it.
4. **Split order** depends on the in-memory entry order. For entries created and posted in one session without a
   reload, GnuCash's order can differ from (date, date_entered, desc, action, GUID) because description changes do not
   re-sort (seen in exp1). For red-ale, which always works on loaded or validated state, proposed: use the sorted
   order.
5. **Foreign-currency invoice lines** (§6.6) were not exercised by the oracle (no `gncInvoiceAddPrice` from Python).
   Expected amounts follow from the source. Needs a test before red-ale enables it.
6. **Preset payment transactions** (editing an existing payment, `preset_txn`, `gncOwner.c:777`) are not covered by
   an experiment. They matter if red-ale offers "edit payment".
