"""Create a synthetic GnuCash 5.17 XML book exercising every business object."""
import sys
import datetime
from gnucash import Session, SessionOpenMode, Account, GncNumeric, _sw_core_utils
from gnucash.gnucash_business import (Customer, Employee, Vendor, Job, Invoice, Bill, Entry, TaxTable,
                                      TaxTableEntry, BillTerm, GNC_AMT_TYPE_VALUE, GNC_AMT_TYPE_PERCENT,
                                      GNC_DISC_POSTTAX, GNC_DISC_SAMETIME, GNC_TAXINCLUDED_NO, GNC_TAXINCLUDED_YES,
                                      GNC_PAYMENT_CARD)
from gnucash.gnucash_core_c import (ACCT_TYPE_ASSET, ACCT_TYPE_RECEIVABLE, ACCT_TYPE_INCOME, ACCT_TYPE_LIABILITY,
                                    ACCT_TYPE_PAYABLE, ACCT_TYPE_EXPENSE, ACCT_TYPE_BANK, ACCT_TYPE_CREDIT)

assert _sw_core_utils.gnc_version() == '5.17', _sw_core_utils.gnc_version()

path = sys.argv[1]
# --no-credit-note makes the job's invoice an ordinary one, so the book does not declare the Credit Notes feature
# (which red-ale v0.1 opens read-only).
credit_note = '--no-credit-note' not in sys.argv[2:]
s = Session("xml://" + path, SessionOpenMode.SESSION_NEW_OVERWRITE)
try:
    book = s.book
    root = book.get_root_account()
    EUR = book.get_table().lookup('CURRENCY', 'EUR')

    def acct(parent, name, typ):
        a = Account(book)
        parent.append_child(a)
        a.SetName(name)
        a.SetType(typ)
        a.SetCommodity(EUR)
        return a

    assets = acct(root, 'Assets', ACCT_TYPE_ASSET)
    ar = acct(assets, 'Receivable', ACCT_TYPE_RECEIVABLE)
    bank = acct(assets, 'Bank', ACCT_TYPE_BANK)
    liab = acct(root, 'Liabilities', ACCT_TYPE_LIABILITY)
    ap = acct(liab, 'Payable', ACCT_TYPE_PAYABLE)
    tax = acct(liab, 'Tax', ACCT_TYPE_LIABILITY)
    levy = acct(liab, 'Levy', ACCT_TYPE_LIABILITY)
    card = acct(liab, 'Card', ACCT_TYPE_CREDIT)
    income = acct(root, 'Income', ACCT_TYPE_INCOME)
    expenses = acct(root, 'Expenses', ACCT_TYPE_EXPENSE)

    # Billing terms
    bt_days = BillTerm(book)
    bt_days.BeginEdit()
    bt_days.SetName('Net30')
    bt_days.SetDescription('Net 30, 2% in 10')
    bt_days.SetType(1)  # GNC_TERM_TYPE_DAYS
    bt_days.SetDueDays(30)
    bt_days.SetDiscountDays(10)
    bt_days.SetDiscount(GncNumeric(2, 1))
    bt_days.CommitEdit()

    bt_prox = BillTerm(book)
    bt_prox.BeginEdit()
    bt_prox.SetName('Prox15')
    bt_prox.SetDescription('15th of next month')
    bt_prox.SetType(2)  # GNC_TERM_TYPE_PROXIMO
    bt_prox.SetDueDays(15)
    bt_prox.SetDiscountDays(5)
    bt_prox.SetDiscount(GncNumeric(3, 2))
    bt_prox.SetCutoff(25)
    bt_prox.CommitEdit()

    # Tax table: 10% + fixed 2.50
    tt = TaxTable(book, 'GST+Levy', TaxTableEntry(tax, True, GncNumeric(10, 1)))
    tt.AddEntry(TaxTableEntry(levy, False, GncNumeric(250, 100)))

    cust = Customer(book, book.CustomerNextID(), EUR, 'Acme & Sons <Ltd>')
    cust.BeginEdit()
    a = cust.GetAddr()
    a.SetName('Acme Billing')
    a.SetAddr1('1 Main St')
    a.SetAddr2('Suite 2')
    a.SetPhone('555-1234')
    a.SetEmail('ap@acme.example')
    sa = cust.GetShipAddr()
    sa.SetName('Acme Warehouse')
    sa.SetAddr1('9 Dock Rd')
    cust.SetNotes('Good payer')
    cust.SetTerms(bt_days)
    cust.SetTaxIncluded(GNC_TAXINCLUDED_NO)
    cust.SetDiscount(GncNumeric(5, 1))
    cust.SetCredit(GncNumeric(100000, 100))
    cust.SetTaxTableOverride(True)
    cust.SetTaxTable(tt)
    cust.CommitEdit()

    vend = Vendor(book, book.VendorNextID(), EUR, 'Paper Supplies')
    vend.BeginEdit()
    vend.GetAddr().SetName('Paper Supplies Accounts')
    vend.SetTerms(bt_prox)
    vend.SetTaxIncluded(GNC_TAXINCLUDED_YES)
    vend.CommitEdit()

    emp = Employee(book, '000001', EUR, 'Jane Doe')
    emp.BeginEdit()
    emp.SetUsername('jdoe')
    emp.SetLanguage('en_GB')
    emp.SetAcl('all')
    emp.SetWorkday(GncNumeric(8, 1))
    emp.SetRate(GncNumeric(4250, 100))
    emp.SetCCard(card)
    emp.GetAddr().SetName('Jane Doe')
    emp.CommitEdit()

    job = Job(book, 'JOB-1', cust, 'Website rebuild')
    job.BeginEdit()
    job.SetReference('PO-77')
    job.SetRate(GncNumeric(9000, 100))
    job.CommitEdit()

    d_open = datetime.date(2024, 3, 1)
    d_post = datetime.date(2024, 3, 5)
    d_due = datetime.date(2024, 4, 4)

    # Customer invoice, posted, partially paid
    inv = Invoice(book, book.InvoiceNextID(cust), EUR, cust, d_open)
    inv.BeginEdit()
    inv.SetTerms(bt_days)
    inv.SetBillingID('CUST-REF-9')
    inv.SetNotes('First invoice')
    inv.CommitEdit()
    e1 = Entry(book, inv, datetime.date(2024, 3, 2))
    e1.SetDescription('Consulting')
    e1.SetAction('Hours')
    e1.SetNotes('On site')
    e1.SetQuantity(GncNumeric(10, 1))
    e1.SetInvAccount(income)
    e1.SetInvPrice(GncNumeric(12050, 100))
    e1.SetInvTaxable(True)
    e1.SetInvTaxIncluded(False)
    e1.SetInvTaxTable(tt)
    e1.SetDateEntered(datetime.datetime(2024, 3, 2, 10, 0, 0))
    e2 = Entry(book, inv, datetime.date(2024, 3, 3))
    e2.SetDescription('Travel')
    e2.SetQuantity(GncNumeric(1, 1))
    e2.SetInvAccount(income)
    e2.SetInvPrice(GncNumeric(5000, 100))
    e2.SetInvDiscount(GncNumeric(500, 100))
    e2.SetInvDiscountType(GNC_AMT_TYPE_VALUE)
    e2.SetInvDiscountHow(GNC_DISC_POSTTAX)
    e2.SetInvTaxable(False)
    e2.SetDateEntered(datetime.datetime(2024, 3, 3, 10, 0, 0))
    inv.PostToAccount(ar, d_post, d_due, 'posted memo', True, False)
    inv.ApplyPayment(None, bank, GncNumeric(50000, 100), GncNumeric(1, 1), datetime.date(2024, 3, 20), 'part pay', 'CHK-1')

    # Vendor bill, posted, one billable line charged to the customer
    bill = Bill(book, book.BillNextID(vend), EUR, vend, d_open)
    be = Entry(book)
    be.SetDate(datetime.date(2024, 3, 2))
    be.SetDescription('Printer paper')
    be.SetQuantity(GncNumeric(3, 1))
    be.SetBillAccount(expenses)
    be.SetBillPrice(GncNumeric(1999, 100))
    be.SetBillTaxable(True)
    be.SetBillTaxIncluded(True)
    be.SetBillTaxTable(tt)
    be.SetBillable(True)
    be.SetBillTo(cust)
    be.SetDateEntered(datetime.datetime(2024, 3, 2, 11, 0, 0))
    bill.AddEntry(be)
    bill.PostToAccount(ap, d_post, d_due, '', True, False)

    # Employee expense voucher, posted, paid by card
    vou = Bill(book, book.InvoiceNextID(emp), EUR, emp, d_open)
    ve = Entry(book)
    ve.SetDate(datetime.date(2024, 3, 4))
    ve.SetDescription('Taxi')
    ve.SetQuantity(GncNumeric(1, 1))
    ve.SetBillAccount(expenses)
    ve.SetBillPrice(GncNumeric(3275, 100))
    ve.SetBillPayment(GNC_PAYMENT_CARD)
    ve.SetDateEntered(datetime.datetime(2024, 3, 4, 12, 0, 0))
    vou.AddEntry(ve)
    vou.PostToAccount(ap, d_post, d_due, '', True, False)

    # Unposted invoice owned by the job, flagged as a credit note unless --no-credit-note
    cn = Invoice(book, book.InvoiceNextID(cust), EUR, job, d_open)
    cn.SetIsCreditNote(credit_note)
    ce = Entry(book, cn, datetime.date(2024, 3, 6))
    ce.SetDescription('Refund')
    ce.SetQuantity(GncNumeric(1, 1))
    ce.SetInvAccount(income)
    ce.SetInvPrice(GncNumeric(1000, 100))
    ce.SetInvDiscountType(GNC_AMT_TYPE_PERCENT)
    ce.SetInvDiscountHow(GNC_DISC_SAMETIME)
    ce.SetDateEntered(datetime.datetime(2024, 3, 6, 9, 0, 0))

    book.set_default_invoice_report('0769e242be474010b4acf264a5512e6e', 'Printable Invoice')
    s.save()
finally:
    s.end()
