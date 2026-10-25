"""Generate a rich synthetic GnuCash book with the 5.17 oracle (XML backend)."""
import sys, os
from datetime import datetime, timezone
from gnucash import _sw_core_utils
assert _sw_core_utils.gnc_version() == '5.17', _sw_core_utils.gnc_version()
from gnucash import Session, SessionOpenMode, Account, Transaction, Split, GncNumeric, GncCommodity, GncPrice, GncLot
import gnucash.gnucash_core_c as c

path = sys.argv[1]
SPECIAL = "amp& lt< gt> quot\" apos' eé euro€ jp日本 tab\there nl\nhere cr\rhere ctl\x01x"

s = Session("xml://" + path, SessionOpenMode.SESSION_NEW_OVERWRITE)
book = s.book
root = book.get_root_account()
ct = book.get_table()
USD = ct.lookup("CURRENCY", "USD")
EUR = ct.lookup("CURRENCY", "EUR")
ACME = GncCommodity(book, "Acme Corp", "NASDAQ", "ACME", "US0000001", 10000)
ct.insert(ACME)
c.gnc_commodity_set_quote_flag(ACME.instance, True)
c.gnc_commodity_set_quote_source(ACME.instance, c.gnc_quote_source_lookup_by_internal("yahoo_json"))
c.gnc_commodity_set_quote_tz(ACME.instance, "America/New_York")
# a currency with the quote flag set gets written as a gnc:commodity
c.gnc_commodity_set_quote_flag(EUR.instance, True)
c.gnc_commodity_set_quote_source(EUR.instance, c.gnc_quote_source_lookup_by_internal("currency"))
FUND = GncCommodity(book, "Zeta Fund", "FUND", "ZETA", "", 1000)
ct.insert(FUND)


def acct(parent, name, typ, comm=USD, **kw):
    a = Account(book)
    a.BeginEdit()
    a.SetName(name)
    a.SetType(typ)
    a.SetCommodity(comm)
    for k, v in kw.items():
        getattr(a, k)(v)
    parent.append_child(a)
    a.CommitEdit()
    return a


T = c  # ACCT_TYPE_* constants live in gnucash_core_c
assets = acct(root, "Assets", T.ACCT_TYPE_ASSET, SetPlaceholder=True, SetDescription="All assets")
checking = acct(assets, "Checking " + SPECIAL, T.ACCT_TYPE_BANK, SetCode="1001",
                SetDescription="desc " + SPECIAL, SetNotes="notes " + SPECIAL, SetColor="#ff0000",
                SetTaxRelated=True, SetLastNum="42")
checking.SetReconcileLastDate(int(datetime(2024, 1, 31, tzinfo=timezone.utc).timestamp()))
cash = acct(assets, "Cash", T.ACCT_TYPE_CASH)
eurbank = acct(assets, "Euro Bank", T.ACCT_TYPE_BANK, comm=EUR)
broker = acct(assets, "Brokerage", T.ACCT_TYPE_ASSET, SetPlaceholder=True)
acme = acct(broker, "ACME", T.ACCT_TYPE_STOCK, comm=ACME)
zeta = acct(broker, "Zeta", T.ACCT_TYPE_MUTUAL, comm=FUND, SetHidden=True)
ar = acct(assets, "Receivable", T.ACCT_TYPE_RECEIVABLE)
liab = acct(root, "Liabilities", T.ACCT_TYPE_LIABILITY)
cc = acct(liab, "Visa", T.ACCT_TYPE_CREDIT)
ap = acct(liab, "Payable", T.ACCT_TYPE_PAYABLE)
inc = acct(root, "Income", T.ACCT_TYPE_INCOME)
salary = acct(inc, "Salary", T.ACCT_TYPE_INCOME)
gains = acct(inc, "Capital Gains", T.ACCT_TYPE_INCOME)
exp = acct(root, "Expenses", T.ACCT_TYPE_EXPENSE)
food = acct(exp, "Food", T.ACCT_TYPE_EXPENSE)
fees = acct(exp, "Fees", T.ACCT_TYPE_EXPENSE)
eq = acct(root, "Equity", T.ACCT_TYPE_EQUITY)
ob = acct(eq, "Opening Balances", T.ACCT_TYPE_EQUITY)
nonstd = acct(assets, "NonStd", T.ACCT_TYPE_BANK)
nonstd.BeginEdit(); nonstd.SetCommoditySCU(1000); nonstd.SetNonStdSCU(True); nonstd.CommitEdit()


def txn(day, desc, cur, splits, num="", notes=None, entered=None):
    t = Transaction(book)
    t.BeginEdit()
    t.SetCurrency(cur)
    t.SetDate(day, 6, 2024)
    if entered:
        t.SetDateEnteredSecs(entered)
    t.SetDescription(desc)
    if num:
        t.SetNum(num)
    for (a, val, qty, memo, action) in splits:
        sp = Split(book)
        sp.SetParent(t)
        sp.SetAccount(a)
        sp.SetValue(val)
        sp.SetAmount(qty)
        if memo:
            sp.SetMemo(memo)
        if action:
            sp.SetAction(action)
    if notes:
        t.SetNotes(notes)
    t.CommitEdit()
    return t


N = GncNumeric
E = int(datetime(2024, 6, 30, 12, 0, 0, tzinfo=timezone.utc).timestamp())
t1 = txn(1, "Opening", USD, [(ob, N(-500000, 100), N(-500000, 100), "", ""),
                             (checking, N(500000, 100), N(500000, 100), "", "")], entered=E)
# negative split first to observe split sorting within a transaction
t2 = txn(2, "Groceries " + SPECIAL, USD, [(checking, N(-1234, 100), N(-1234, 100), "memo " + SPECIAL, "act " + SPECIAL),
                                          (food, N(1234, 100), N(1234, 100), "", "")],
         num="101", notes="trn notes " + SPECIAL, entered=E)
# multi-currency: USD transaction, EUR account, value != quantity
t3 = txn(3, "To euro account", USD, [(checking, N(-11000, 100), N(-11000, 100), "", ""),
                                     (eurbank, N(11000, 100), N(10000, 100), "", "")], entered=E)
# stock buy (2 lots)
t4 = txn(4, "Buy ACME", USD, [(acme, N(100000, 100), N(100000, 10000), "", "Buy"),
                              (fees, N(995, 100), N(995, 100), "", ""),
                              (checking, N(-100995, 100), N(-100995, 100), "", "")], entered=E)
t5 = txn(5, "Sell ACME", USD, [(acme, N(-60000, 100), N(-50000, 10000), "", "Sell"),
                               (checking, N(60000, 100), N(60000, 100), "", "")], entered=E)
t6 = txn(6, "Mistake", USD, [(checking, N(-100, 100), N(-100, 100), "", ""),
                             (food, N(100, 100), N(100, 100), "", "")], entered=E)
t6.Void("entered twice")
t7 = txn(7, "Fund buy", USD, [(zeta, N(2500, 100), N(1234, 1000), "", ""),
                              (cash, N(-2500, 100), N(-2500, 100), "", "")], entered=E)
# a lot with a title and notes containing the buy and sell of ACME
lot = GncLot(book)
lot.begin_edit() if hasattr(lot, 'begin_edit') else None
acme.InsertLot(lot)
for sp in t4.GetSplitList():
    if sp.GetAccount().GetName() == "ACME":
        lot.add_split(sp)
for sp in t5.GetSplitList():
    if sp.GetAccount().GetName() == "ACME":
        lot.add_split(sp)
lot.set_title("Lot " + SPECIAL)
lot.set_notes("lot notes")
# reconcile a split
for sp in t1.GetSplitList():
    if sp.GetAccount().GetName().startswith("Checking"):
        sp.SetReconcile('y')
        sp.SetDateReconciledSecs(int(datetime(2024, 6, 30, tzinfo=timezone.utc).timestamp()))

# prices
pdb = book.get_price_db()
def price(comm, cur, y, m, d, num, den, src="user:price", typ="last"):
    p = GncPrice(book)
    p.begin_edit()
    p.set_commodity(comm)
    p.set_currency(cur)
    p.set_time64(datetime(y, m, d, 10, 59, tzinfo=timezone.utc))
    p.set_value(N(num, den))
    p.set_typestr(typ)
    c.gnc_price_set_source_string(p.instance, src)
    p.commit_edit()
    pdb.add_price(p)
price(ACME, USD, 2024, 6, 4, 1000, 1)
price(ACME, USD, 2024, 6, 5, 12000, 10, "Finance::Quote", "last")
price(EUR, USD, 2024, 6, 3, 110, 100, "user:xfer-dialog", "transaction")
price(FUND, USD, 2024, 6, 7, 2026, 1000, "user:price-editor", "nav")

s.save()
s.end()
print("ok")
