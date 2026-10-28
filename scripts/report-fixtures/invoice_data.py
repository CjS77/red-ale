# This program is free software; you can redistribute it and/or
# modify it under the terms of the GNU General Public License as
# published by the Free Software Foundation; either version 2 of
# the License, or (at your option) any later version.
#
# This program is distributed in the hope that it will be useful,
# but WITHOUT ANY WARRANTY; without even the implied warranty of
# MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
# GNU General Public License for more details.
#
# You should have received a copy of the GNU General Public License
# along with this program; if not, contact:
#
# Free Software Foundation           Voice:  +1-617-542-5942
# 51 Franklin Street, Fifth Floor    Fax:    +1-617-542-2652
# Boston, MA  02110-1301,  USA       gnu@gnu.org
"""`create-test-invoice-data` (`5.17:bindings/guile/test/test-engine-extras.scm:487-849`, licence notice above, kept
from that file), replayed in GnuCash 5.17's engine.

Every business call goes through ctypes on `libgnc-engine.so`, in the Scheme's order: the bindings lack the
`gncOrder*` functions (`gncOrderAddEntry`, `:807`, among them), and calling the rest the same way keeps one style.

Differences from a GnuCash test run, all for reproducible output:

- GUIDs are fixed, continuing `engine_extras`'s scheme with kinds that are not hex letters: `1` lots, `2` owners
  (customer, job, vendor, employee, in creation order), `3` invoices, `4` entries, `5` tax tables, `6` bill terms,
  `7` orders; transactions and splits stay `c` and `d`. Objects the engine makes inside a call
  (`gncInvoicePostToAccount`'s transaction, splits and lot, the child tax table and bill term it copies, the payment's
  transaction, splits and lot) get theirs right after the call (`adopt_new`), the splits in their transaction's order.
- Those transactions' entered dates, which the engine takes from the wall clock
  (`5.17:libgnucash/engine/gncInvoice.c:1508`, `5.17:libgnucash/engine/gncOwner.c:877`), are replaced by the next
  pinned entered date, as `engine_extras` does for its own transactions.
- Each invoice is opened on the day it is posted, just before posting, with `gncInvoiceSetDateOpenedGDate` (noon of
  that day, `5.17:libgnucash/engine/gncInvoice.c:488-492`, `5.17:libgnucash/engine/gnc-date.cpp:404-410`). GnuCash's
  test never sets one: `gncInvoiceCreate` leaves it at `INT64_MAX` (`gncInvoice.c:138`), which the XML backend cannot
  write (`time64_to_dom_tree` refuses it, `5.17:libgnucash/backend/xml/sixtp-dom-generators.cpp:137`, so
  `invoice:opened` is missing and the book does not load). No report of TT reads it.
- `(current-time)`, which dates every entry (`:640`, `:795`), is the pinned now, 2024-03-15 12:00:00 UTC: each entry
  is dated 2024-03-15 12:00:00 UTC (`gncEntrySetDateGDate` takes noon of the day,
  `5.17:libgnucash/engine/gncEntry.c:504-515`). Entries' entered dates are never set (`gncEntryCreate` leaves them 0,
  `gncEntry.c:413-443`), as in the test.
"""
import ctypes
import re
from fractions import Fraction

import engine_extras as ex
from engine_extras import ASSET, EXPENSE, INCOME, LIABILITY, NOW, dmy2time64, mnemonic_to_commodity, ptr

import gnucash.gnucash_core_c as c

from gnc_engine import GncNumeric as CNumeric, engine, set_current_session, clear_current_session

VP, I, S, T = ctypes.c_void_p, ctypes.c_int, ctypes.c_char_p, ctypes.c_int64
RECEIVABLE = c.ACCT_TYPE_RECEIVABLE
PAYABLE = c.ACCT_TYPE_PAYABLE
GNC_TAXINCLUDED_YES = 1
GNC_AMT_TYPE_PERCENT = 2
GNC_TERM_TYPE_DAYS = 1

# Kinds of fixed GUID (see the module docstring), and the QOF collections adopt_new watches.
LOT, OWNER, INVOICE, ENTRY, TAXTABLE, BILLTERM, ORDER = "1", "2", "3", "4", "5", "6", "7"
COLLECTIONS = {"Trans": "c", "Split": "d", "Lot": LOT, "gncTaxTable": TAXTABLE, "gncBillTerm": BILLTERM}
ALL_COLLECTIONS = ["Account", "Trans", "Split", "Lot", "Price", "gncCustomer", "gncJob", "gncVendor", "gncEmployee",
                   "gncInvoice", "gncEntry", "gncTaxTable", "gncBillTerm", "gncOrder"]


def declare(name, restype, *argtypes):
    f = getattr(engine, name)
    f.restype = restype
    f.argtypes = list(argtypes)
    return f


def setters(prefix, *names, arg=S):
    return [declare(f"{prefix}Set{n}", None, VP, arg) for n in names]


customer_create = declare("gncCustomerCreate", VP, VP)
customer_set_id, customer_set_name, customer_set_notes = setters("gncCustomer", "ID", "Name", "Notes")
customer_set_currency, = setters("gncCustomer", "Currency", arg=VP)
customer_set_tax_included, = setters("gncCustomer", "TaxIncluded", arg=I)
vendor_create = declare("gncVendorCreate", VP, VP)
vendor_set_id, vendor_set_name, vendor_set_notes = setters("gncVendor", "ID", "Name", "Notes")
vendor_set_currency, = setters("gncVendor", "Currency", arg=VP)
vendor_set_tax_included, = setters("gncVendor", "TaxIncluded", arg=I)
employee_create = declare("gncEmployeeCreate", VP, VP)
employee_set_id, employee_set_name = setters("gncEmployee", "ID", "Name")
employee_set_currency, = setters("gncEmployee", "Currency", arg=VP)
job_create = declare("gncJobCreate", VP, VP)
job_set_id, job_set_name = setters("gncJob", "ID", "Name")
job_set_owner, = setters("gncJob", "Owner", arg=VP)
owner_new = declare("gncOwnerNew", VP)
owner_init = {k: declare(f"gncOwnerInit{k}", None, VP, VP) for k in ("Customer", "Job", "Vendor", "Employee")}
invoice_create = declare("gncInvoiceCreate", VP, VP)
invoice_copy = declare("gncInvoiceCopy", VP, VP)
invoice_set_id, invoice_set_notes, invoice_set_billing_id = setters("gncInvoice", "ID", "Notes", "BillingID")
invoice_set_owner, invoice_set_currency, invoice_set_terms = setters("gncInvoice", "Owner", "Currency", "Terms", arg=VP)
invoice_set_is_credit_note, = setters("gncInvoice", "IsCreditNote", arg=I)
invoice_set_date_opened_gdate, = setters("gncInvoice", "DateOpenedGDate", arg=VP)
invoice_add_entry = declare("gncInvoiceAddEntry", None, VP, VP)
invoice_post = declare("gncInvoicePostToAccount", VP, VP, VP, T, T, S, I, I)
invoice_apply_payment = declare("gncInvoiceApplyPayment", None, VP, VP, VP, CNumeric, CNumeric, T, S, S)
invoice_is_paid = declare("gncInvoiceIsPaid", I, VP)
taxtable_create = declare("gncTaxTableCreate", VP, VP)
taxtable_inc_ref = declare("gncTaxTableIncRef", None, VP)
taxtable_set_name, = setters("gncTaxTable", "Name")
taxtable_add_entry = declare("gncTaxTableAddEntry", None, VP, VP)
tt_entry_create = declare("gncTaxTableEntryCreate", VP)
tt_entry_set_account, = setters("gncTaxTableEntry", "Account", arg=VP)
tt_entry_set_type, = setters("gncTaxTableEntry", "Type", arg=I)
tt_entry_set_amount, = setters("gncTaxTableEntry", "Amount", arg=CNumeric)
entry_create = declare("gncEntryCreate", VP, VP)
entry_set_date_gdate, = setters("gncEntry", "DateGDate", arg=VP)
entry_set_description, entry_set_action, entry_set_notes = setters("gncEntry", "Description", "Action", "Notes")
entry_set_inv_account, entry_set_bill_account, entry_set_inv_tax_table = setters(
    "gncEntry", "InvAccount", "BillAccount", "InvTaxTable", arg=VP)
entry_set_inv_price, entry_set_bill_price, entry_set_inv_discount = setters(
    "gncEntry", "InvPrice", "BillPrice", "InvDiscount", arg=CNumeric)
entry_set_inv_discount_type, entry_set_inv_discount_how, entry_set_inv_taxable, entry_set_inv_tax_included = setters(
    "gncEntry", "InvDiscountType", "InvDiscountHow", "InvTaxable", "InvTaxIncluded", arg=I)
entry_set_doc_quantity = declare("gncEntrySetDocQuantity", None, VP, CNumeric, I)
amount_type_to_string = declare("gncAmountTypeToString", S, I)
discount_how_to_string = declare("gncEntryDiscountHowToString", S, I)
order_create = declare("gncOrderCreate", VP, VP)
order_set_id, order_set_reference = setters("gncOrder", "ID", "Reference")
order_set_owner, = setters("gncOrder", "Owner", arg=VP)
order_set_active, = setters("gncOrder", "Active", arg=I)
order_add_entry = declare("gncOrderAddEntry", None, VP, VP)
billterm_create = declare("gncBillTermCreate", VP, VP)
billterm_set_name, billterm_set_description = setters("gncBillTerm", "Name", "Description")
billterm_set_type, billterm_set_due_days = setters("gncBillTerm", "Type", "DueDays", arg=I)
get_collection = declare("qof_book_get_collection", VP, VP, S)
collection_foreach = declare("qof_collection_foreach", None, VP, VP, VP)
instance_get_guid = declare("qof_instance_get_guid", VP, VP)
guid_to_string_buff = declare("guid_to_string_buff", S, VP, S)
trans_split_list = declare("xaccTransGetSplitList", VP, VP)
trans_begin_edit = declare("xaccTransBeginEdit", None, VP)
trans_commit_edit = declare("xaccTransCommitEdit", None, VP)
trans_set_date_entered = declare("xaccTransSetDateEnteredSecs", None, VP, T)
set_guid_raw = declare("qof_instance_set_guid", None, VP, VP)
template_root = declare("gnc_book_get_template_root", VP, VP)
glib = ctypes.CDLL("libglib-2.0.so.0")
glib.g_date_new_dmy.restype = VP
glib.g_date_new_dmy.argtypes = [ctypes.c_uint8, I, ctypes.c_uint16]
FOREACH = ctypes.CFUNCTYPE(None, VP, VP)


class GList(ctypes.Structure):
    pass


GList._fields_ = [("data", VP), ("next", ctypes.POINTER(GList)), ("prev", ctypes.POINTER(GList))]


def num(x):
    """A Scheme exact number as `gnc_scm_to_numeric` passes it: reduced."""
    f = Fraction(x)
    return CNumeric(f.numerator, f.denominator)


def fix(p, kind):
    """Give the instance at pointer `p` the next fixed GUID of `kind`; returns `p`."""
    set_guid_raw(p, ctypes.create_string_buffer(bytes.fromhex(ex.BOOK.guid(kind)), 16))
    return p


def book():
    return ptr(ex.BOOK.book)


def members(collection):
    """The instances of a QOF collection of the current book, as pointer values."""
    out = []
    cb = FOREACH(lambda inst, _data: out.append(inst))
    collection_foreach(get_collection(book(), collection.encode()), cb, None)
    return set(out)


def snapshot():
    return {name: members(name) for name in COLLECTIONS}


def glist(head):
    out, node = [], ctypes.cast(head, ctypes.POINTER(GList))
    while node:
        out.append(node.contents.data)
        node = node.contents.next
    return out


def adopt_new(before):
    """Fix the GUIDs of the instances made since `before` (a `snapshot`): each new transaction, then its splits in
    their order in it, with the next pinned entered date; then at most one new lot, tax table and bill term."""
    new = {name: members(name) - old for name, old in before.items()}
    splits = []
    for txn in sorted(new["Trans"]):
        if len(new["Trans"]) > 1:
            raise SystemExit("more than one new transaction: their order would not be fixed")
        fix(txn, "c")
        trans_begin_edit(txn)
        trans_set_date_entered(txn, ex.BOOK.next_entered())
        trans_commit_edit(txn)
        splits = glist(trans_split_list(txn))
        for split in splits:
            fix(split, "d")
    if set(splits) != new["Split"]:
        raise SystemExit("new splits outside the new transaction")
    for name in ("Lot", "gncTaxTable", "gncBillTerm"):
        if len(new[name]) > 1:
            raise SystemExit(f"more than one new {name}: their order would not be fixed")
        for inst in new[name]:
            fix(inst, COLLECTIONS[name])


FIXED = re.compile(r"^[0-9a-f]{1,2}0{16}")


def check_fixed_guids():
    """Every instance of the book has a fixed GUID, so a rebuild gives the same bytes. The template root, which every
    book has and which is not saved while it has no children, is left out."""
    buf = ctypes.create_string_buffer(33)
    skip = {template_root(book())}
    loose = [(name, guid_to_string_buff(instance_get_guid(inst), buf).decode())
             for name in ALL_COLLECTIONS for inst in members(name) - skip]
    loose = [x for x in loose if not FIXED.match(x[1])]
    if loose:
        raise SystemExit(f"instances without a fixed GUID: {loose}")


def owner(kind, thing):
    """`gncOwnerNew` then `gncOwnerInit<kind>`."""
    o = owner_new()
    owner_init[kind](o, thing)
    return o


def make_customer(usd):
    """`:512-518`."""
    cust = fix(customer_create(book()), OWNER)
    customer_set_id(cust, b"cust-1-id")
    customer_set_name(cust, b"cust-1-name")
    customer_set_notes(cust, b"cust-1-notes")
    customer_set_currency(cust, usd)
    customer_set_tax_included(cust, GNC_TAXINCLUDED_YES)
    return cust


def make_invoice(id_, own, usd, notes=None, billing_id=None):
    """`gncInvoiceCreate`, ID, owner, notes, billing ID, currency (`:525-531` and the like)."""
    inv = fix(invoice_create(book()), INVOICE)
    invoice_set_id(inv, id_)
    invoice_set_owner(inv, own)
    if notes is not None:
        invoice_set_notes(inv, notes)
    if billing_id is not None:
        invoice_set_billing_id(inv, billing_id)
    invoice_set_currency(inv, usd)
    return inv


def make_job(own):
    """`:533-537`."""
    job = fix(job_create(book()), OWNER)
    job_set_id(job, b"job-1-id")
    job_set_name(job, b"job-1-name")
    job_set_owner(job, own)
    return job


def make_vendor(usd):
    """`:551-557`."""
    vend = fix(vendor_create(book()), OWNER)
    vendor_set_id(vend, b"vend-1-id")
    vendor_set_name(vend, b"vend-1-name")
    vendor_set_notes(vend, b"vend-1-notes")
    vendor_set_currency(vend, usd)
    vendor_set_tax_included(vend, GNC_TAXINCLUDED_YES)
    return vend


def make_employee(usd):
    """`:571-575`."""
    emp = fix(employee_create(book()), OWNER)
    employee_set_id(emp, b"emp-1-id")
    employee_set_currency(emp, usd)
    employee_set_name(emp, b"emp-1-name")
    return emp


def copy_credit_note(inv, id_, usd):
    """`gncInvoiceCopy`, then ID, credit note, currency (`:590-608`)."""
    cn = fix(invoice_copy(inv), INVOICE)
    invoice_set_id(cn, id_)
    invoice_set_is_credit_note(cn, 1)
    invoice_set_currency(cn, usd)
    return cn


def make_tax_table(name, account, rate):
    """`gncTaxTableCreate` with one percentage entry (`:616-636`, `:760-769`)."""
    tt = fix(taxtable_create(book()), TAXTABLE)
    taxtable_inc_ref(tt)
    taxtable_set_name(tt, name)
    entry = tt_entry_create()
    tt_entry_set_account(entry, account)
    tt_entry_set_type(entry, GNC_AMT_TYPE_PERCENT)
    tt_entry_set_amount(entry, num(rate))
    taxtable_add_entry(tt, entry)
    return tt


def today_gdate():
    """`(time64-to-gdate (current-time))` for the pinned now."""
    tm = ex.localtime(NOW)
    return glib.g_date_new_dmy(tm.tm_mday, tm.tm_mon, tm.tm_year)


def gdate(dd, mm, yy):
    return glib.g_date_new_dmy(dd, mm, yy)


def add_simple_entry(inv, name, account, bill, is_cn):
    """One entry of 2 widgets at $3 (`:638-712`): invoice account and price, or bill account and price for a bill."""
    entry = fix(entry_create(book()), ENTRY)
    entry_set_date_gdate(entry, today_gdate())
    entry_set_description(entry, f"{name}-desc".encode())
    entry_set_action(entry, f"{name}-action".encode())
    entry_set_notes(entry, f"{name}-notes".encode())
    (entry_set_bill_account if bill else entry_set_inv_account)(entry, account)
    entry_set_doc_quantity(entry, num(2), is_cn)
    (entry_set_bill_price if bill else entry_set_inv_price)(entry, num(3))
    invoice_add_entry(inv, entry)


def post(inv, account, posted, due, memo):
    """`gncInvoicePostToAccount` with accumulated splits and no autopay, then fix what it made. The invoice is first
    opened on the day it is posted (see the module docstring)."""
    invoice_set_date_opened_gdate(inv, gdate(*posted))
    before = snapshot()
    invoice_post(inv, account, dmy2time64(*posted), dmy2time64(*due), memo, 1, 0)
    adopt_new(before)


def add_combo_entry(inv, order, combo, income, tt):
    """One entry of inv-8 (`:784-820`)."""
    taxable, tax_included = combo[0] == 1, combo[1] == 1
    disc_type, disc_how = combo[2], combo[3]
    desc = (f"taxable={'Y' if taxable else 'N'} tax-included={'Y' if tax_included else 'N'} "
            f"discount-type={amount_type_to_string(disc_type).decode()} "
            f"discount-how={discount_how_to_string(disc_how).decode()}")
    entry = fix(entry_create(book()), ENTRY)
    entry_set_date_gdate(entry, today_gdate())
    entry_set_description(entry, desc.encode())
    entry_set_action(entry, b"action")
    entry_set_inv_account(entry, income)
    entry_set_doc_quantity(entry, num(11), 0)
    entry_set_inv_price(entry, num(Fraction(777, 4)))
    entry_set_inv_discount(entry, num(Fraction(7, 2)))
    entry_set_inv_discount_type(entry, disc_type)
    entry_set_inv_discount_how(entry, disc_how)
    entry_set_inv_taxable(entry, taxable)
    entry_set_inv_tax_included(entry, tax_included)
    entry_set_inv_tax_table(entry, tt)
    order_add_entry(order, entry)
    invoice_add_entry(inv, entry)


def scheme_number(x):
    """A Scheme exact number as `format ~a` prints it: `109/10`, `11`."""
    f = Fraction(x)
    return str(f.numerator) if f.denominator == 1 else f"{f.numerator}/{f.denominator}"


def create_test_invoice_data():
    """`create-test-invoice-data` (`:487-849`) in the current book. Needs GnuCash's current session to be this book's
    (`gnc_set_num_action` reads the book option from it, `5.17:libgnucash/engine/engine-helpers.c:119`), as in the
    test; it is set for the call. Returns the eight invoices and the accounts by name."""
    set_current_session(ex.BOOK.session)
    try:
        return build_invoices()
    finally:
        clear_current_session()


def build_invoices():
    usd_obj = mnemonic_to_commodity("USD")
    usd = ptr(usd_obj)
    env = ex.create_test_env()
    structure = ["Root", {"type": ASSET, "commodity": usd_obj},
                 ["Asset", ["Bank"]],
                 ["VAT", ["VAT-on-Purchases"], ["VAT-on-Sales", {"type": LIABILITY}]],
                 ["Income", {"type": INCOME}],
                 ["Expense", {"type": EXPENSE}],
                 ["A/Receivable", {"type": RECEIVABLE}],
                 ["A/Payable", {"type": PAYABLE}]]
    accts = ex.env_create_account_structure_alist(env, structure)
    bank, income, expense, vat_sales, receivable, payable = (
        ptr(accts[n]) for n in ("Bank", "Income", "Expense", "VAT-on-Sales", "A/Receivable", "A/Payable"))

    cust_1 = make_customer(usd)
    owner_1 = owner("Customer", cust_1)
    inv_1 = make_invoice(b"0001", owner_1, usd, b"inv-1-notes", b"inv-1-billing-id")
    job_1 = make_job(owner_1)
    owner_2 = owner("Job", job_1)
    inv_2 = make_invoice(b"0002", owner_2, usd, b"inv-2-notes")
    vend_1 = make_vendor(usd)
    owner_3 = owner("Vendor", vend_1)
    inv_3 = make_invoice(b"0003", owner_3, usd, b"inv-3-notes")
    emp_1 = make_employee(usd)
    owner_4 = owner("Employee", emp_1)
    inv_4 = make_invoice(b"0004", owner_4, usd, b"inv-4-notes")
    inv_5 = copy_credit_note(inv_1, b"0005", usd)
    inv_6 = copy_credit_note(inv_3, b"0006", usd)
    inv_7 = copy_credit_note(inv_4, b"0007", usd)
    inv_8 = make_invoice(b"0008", owner_1, usd)
    # :616-636, made and never used
    make_tax_table(b"10% vat on sales", vat_sales, 10)
    make_tax_table(b"10% vat on purchases", ptr(accts["VAT-on-Purchases"]), 10)

    # :638-712
    for inv, name, bill, is_cn in [(inv_1, "entry-1", False, 0), (inv_2, "entry-inv-2", False, 0),
                                   (inv_3, "entry-inv-3", True, 0), (inv_4, "entry-inv-4", True, 0),
                                   (inv_5, "entry-5", False, 1), (inv_6, "entry-inv-6", True, 1),
                                   (inv_7, "entry-inv-7", True, 1)]:
        add_simple_entry(inv, name, expense if bill else income, bill, is_cn)

    # :714-754
    for inv, acc, posted, due, memo in [
            (inv_1, receivable, (1, 9, 1980), (1, 9, 1980), b"cust-invoice"),
            (inv_2, receivable, (2, 9, 1980), (3, 9, 1980), b"job-invoice"),
            (inv_3, payable, (3, 9, 1980), (3, 9, 1980), b"vendor-bill"),
            (inv_4, payable, (4, 9, 1980), (4, 9, 1980), b"emp-bill"),
            (inv_5, receivable, (5, 9, 1980), (5, 9, 1980), b"cust-credit-note"),
            (inv_6, payable, (6, 9, 1980), (6, 9, 1980), b"vend-credit-note"),
            (inv_7, payable, (7, 9, 1980), (7, 9, 1980), b"emp-credit-note")]:
        post(inv, acc, posted, due, memo)

    # :756-847
    taxrate, discount, unitprice, quantity = Fraction(109, 10), Fraction(7, 2), Fraction(777, 4), 11
    combo_tt = make_tax_table(f"{scheme_number(taxrate)}% vat on sales".encode(), vat_sales, taxrate)
    order = fix(order_create(book()), ORDER)
    order_set_id(order, b"order-id")
    order_set_owner(order, owner_1)
    order_set_reference(order, b"order-ref")
    order_set_active(order, 1)
    term = fix(billterm_create(book()), BILLTERM)
    billterm_set_name(term, b"billterm-name")
    billterm_set_description(term, b"billterm-desc")
    billterm_set_type(term, GNC_TERM_TYPE_DAYS)
    billterm_set_due_days(term, 8)
    invoice_set_terms(inv_8, term)
    for combo in [(1, 2, 1, 1), (2, 1, 2, 2), (1, 1, 2, 3), (2, 2, 1, 3),
                  (2, 1, 1, 1), (1, 2, 2, 2), (1, 2, 1, 2), (1, 1, 2, 1)]:
        add_combo_entry(inv_8, order, combo, income, combo_tt)
    invoice_set_notes(inv_8, (f"tax={scheme_number(taxrate)}%, discount={scheme_number(discount)}, "
                              f"qty={scheme_number(quantity)}, price={scheme_number(unitprice)}").encode())
    post(inv_8, receivable, (8, 9, 1980), (8, 9, 1980), b"trans-posting-memo")
    before = snapshot()
    invoice_apply_payment(inv_8, None, bank, num(Fraction(1747918, 100)), num(1), dmy2time64(10, 9, 1980),
                          b"trans-payment-memo-1", b"trans-payment-num-1")
    adopt_new(before)
    return [inv_1, inv_2, inv_3, inv_4, inv_5, inv_6, inv_7, inv_8], accts
