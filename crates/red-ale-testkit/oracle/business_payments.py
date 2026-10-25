#!/usr/bin/env python3
"""GnuCash 5.17 as the oracle for credit notes, payment matching and payments made from transactions.

Builds on business_posting.py (same book, op language and view). Commands (PYTHONPATH=<5.17 bindings>, TZ=UTC):

  make <book>             business_posting's book plus credit notes CN1 (Acme Ltd), VCN1 (Paper Co) and ECN1 (Jane Doe,
                          a card line and an extra charge), unposted
  do <book> <ops.json>    business_posting's ops plus:
      {"op": "txn", "description": "Deposit", "num": "D1", "date": "2024-03-20", "notes": "",
       "splits": [["Assets:Bank", "100", "c"], ["Income:Sales", "-100", "n"]]}
      {"op": "fromTxn", "find": "D1", "owner": "Acme Ltd", "applyTo": ["INV2"], "amount": "100", "date": "...",
       "memo": "...", "num": "...", "autoPay": false}
          the transaction whose num is "find", turned into a payment as GnuCash's payment dialog opened on it does
          (gnc_ui_payment_new_with_txn and OK): its defaults, its preselection and its order, then the engine call
          gncOwnerApplyPaymentSecs with the transaction preset. Absent fields take the dialog's defaults.
  view <book>, totals <book>   as business_posting.py
  openitems <book> <owner> <account>   the dialog's list of open items, in the order it applies chosen ones
  scrub <book>            GnuCash's business scrub of every A/R and A/P account (gncScrubBusinessAccountTree), then save
"""
import ctypes
import datetime
import json
import os
import sys

import business_posting as bp
from gnucash import GncLot, Split, Transaction
from gnucash.gnucash_business import Invoice
from gnucash.gnucash_core_c import (ACCT_TYPE_ASSET, ACCT_TYPE_BANK, ACCT_TYPE_CASH, ACCT_TYPE_CREDIT, ACCT_TYPE_CURRENCY,
                                    ACCT_TYPE_EQUITY, ACCT_TYPE_LIABILITY, ACCT_TYPE_MUTUAL, ACCT_TYPE_PAYABLE,
                                    ACCT_TYPE_RECEIVABLE, ACCT_TYPE_STOCK, GNC_HOW_RND_ROUND_HALF_UP)

from gnc_engine import NO_PROGRESS, GncNumeric as CNumeric, engine, ptr

PAYMENT_TYPES = {ACCT_TYPE_BANK, ACCT_TYPE_CASH, ACCT_TYPE_ASSET, ACCT_TYPE_CREDIT, ACCT_TYPE_LIABILITY, ACCT_TYPE_STOCK,
                 ACCT_TYPE_MUTUAL, ACCT_TYPE_CURRENCY, ACCT_TYPE_EQUITY}
APAR_TYPES = {ACCT_TYPE_RECEIVABLE, ACCT_TYPE_PAYABLE}
glib = ctypes.CDLL("libglib-2.0.so.0")
glib.g_list_append.restype = ctypes.c_void_p
glib.g_list_append.argtypes = [ctypes.c_void_p, ctypes.c_void_p]


class COwner(ctypes.Structure):
    _fields_ = [("type", ctypes.c_int), ("owner", ctypes.c_void_p), ("qof_temp", ctypes.c_void_p)]


def c_owner(obj, kind):
    o = COwner()
    init = {"gncCustomer": "gncOwnerInitCustomer", "gncVendor": "gncOwnerInitVendor",
            "gncEmployee": "gncOwnerInitEmployee", "gncJob": "gncOwnerInitJob"}[kind]
    getattr(engine, init).argtypes = [ctypes.POINTER(COwner), ctypes.c_void_p]
    getattr(engine, init)(ctypes.byref(o), ptr(obj))
    return o


def key(obj):
    """Identity of an engine object: the bindings give lots no GUID accessor."""
    return ptr(obj).value


def as_lot(lot):
    return lot if lot is None or isinstance(lot, GncLot) else GncLot(instance=lot)


def cnum(n):
    return CNumeric(n.num(), n.denom())


def ts(dt):
    return int(dt.timestamp()) if hasattr(dt, "timestamp") else int(dt)


# ----------------------------------------------------------------------------------------------------------------
# make

def make(path):
    bp.make(path)
    s = bp.Session("xml://" + path, bp.SessionOpenMode.SESSION_NORMAL_OPEN)
    try:
        book = s.book
        usd = book.get_table().lookup("CURRENCY", "USD")
        opened = datetime.date(2024, 3, 1)
        stamp = [datetime.datetime(2024, 3, 2, 9, 0, 0)]

        def note(ident, owner_name, lines, charge=None):
            owner, _ = bp.find_owner(book, owner_name)
            doc = Invoice(book, ident, usd, owner, opened)
            doc.BeginEdit()
            doc.SetIsCreditNote(True)
            if charge:
                doc.SetToChargeAmount(bp.num(charge))
            doc.CommitEdit()
            customer = owner_name == "Acme Ltd"
            for date, desc, qty, price, account, table, pay in lines:
                e = bp.Entry(book)
                e.SetDate(bp.day(date))
                stamp[0] += datetime.timedelta(minutes=1)
                e.SetDateEntered(stamp[0])
                e.SetDescription(desc)
                e.SetDocQuantity(bp.num(qty), True)
                acc = bp.find_account(book, account)
                if customer:
                    e.SetInvAccount(acc)
                    e.SetInvPrice(bp.num(price))
                    e.SetInvTaxable(table is not None)
                    if table:
                        e.SetInvTaxTable(book.TaxTableLookupByName(table))
                else:
                    e.SetBillAccount(acc)
                    e.SetBillPrice(bp.num(price))
                    e.SetBillTaxable(table is not None)
                    if table:
                        e.SetBillTaxTable(book.TaxTableLookupByName(table))
                    if pay:
                        e.SetBillPayment(bp.GNC_PAYMENT_CARD)
                doc.AddEntry(e)

        note("CN1", "Acme Ltd", [("2024-03-09", "Returned widgets", "2", "15", "Income:Sales", "GST+PST", False),
                                 ("2024-03-09", "Goodwill", "1", "4.99", "Income:Services", None, False)])
        note("VCN1", "Paper Co", [("2024-03-10", "Damaged paper", "1", "20", "Expenses:Supplies", "GST+PST", False)])
        note("ECN1", "Jane Doe", [("2024-03-10", "Taxi refund", "1", "10", "Expenses:Travel", None, True),
                                  ("2024-03-10", "Lunch refund", "1", "6", "Expenses:Travel", None, False)], charge="2")
        book.get_root_account().SetName(book.get_root_account().GetName())
        s.save()
    finally:
        s.end()


# ----------------------------------------------------------------------------------------------------------------
# do

def find_txn(book, num):
    for acc in book.get_root_account().get_descendants():
        for split in acc.GetSplitList():
            t = split.GetParent()
            if t.GetNum() == num:
                return t
    raise KeyError(num)


def txn(book, op):
    usd = book.get_table().lookup("CURRENCY", "USD")
    t = Transaction(book)
    t.BeginEdit()
    t.SetCurrency(usd)
    t.SetDescription(op.get("description", ""))
    t.SetNum(op.get("num", ""))
    t.SetDatePostedSecsNormalized(bp.neutral(op["date"]))
    if op.get("notes"):
        t.SetNotes(op["notes"])
    for account, value, state in op["splits"]:
        s = Split(book)
        s.SetParent(t)
        s.SetAccount(bp.find_account(book, account))
        s.SetValue(bp.num(value))
        s.SetAmount(bp.num(value))
        s.SetReconcile(state)
    t.CommitEdit()


def lot_end_owner_matches(lot, owner):
    engine.gncOwnerLotMatchOwnerFunc.restype = ctypes.c_bool
    engine.gncOwnerLotMatchOwnerFunc.argtypes = [ctypes.c_void_p, ctypes.POINTER(COwner)]
    return engine.gncOwnerLotMatchOwnerFunc(ptr(lot), ctypes.byref(owner))


def dialog_items(book, owner, post_acct, tx_lots, tx_post_acct):
    """gnc_payment_window_fill_docs_list (dialog-payment.c:440-603): [(lot, date, id, sort_date)] in display order."""
    lots = [l for l in post_acct.GetLotList() if not l.is_closed() and lot_end_owner_matches(l, owner)]
    if tx_post_acct is not None and tx_post_acct.GetName() == post_acct.GetName():
        for lot, _ in tx_lots:
            if lot.get_balance().zero_p() and lot_end_owner_matches(lot, owner):
                lots.insert(0, lot)
    items = []
    for lot in lots:
        inv = lot.GetInvoiceFromLot()
        if inv:
            date = ts(inv.GetDatePosted())
        else:
            latest = lot.get_latest_split()
            if latest is None:
                continue
            date = ts(latest.GetParent().GetDate())
        value = lot.get_balance()
        match = next((v for l, v in tx_lots if key(l) == key(lot)), None)
        if match is not None:
            value = value.sub(match, 100, GNC_HOW_RND_ROUND_HALF_UP)
        if value.zero_p():
            continue
        items.append((lot, date, inv.GetID() if inv else None))
    items.sort(key=lambda i: (i[1], i[2] is not None, i[2] or ""))
    return items


def sort_key(lot):
    """gncOwnerLotsSortFunc (gncOwner.c:728-745)."""
    inv = lot.GetInvoiceFromLot()
    if inv:
        return ts(inv.GetDateDue())
    return ts(lot.get_earliest_split().GetParent().GetDate())


def insert_sorted(lots):
    """g_list_insert_sorted: each lot goes before the first one it is not later than."""
    out = []
    for lot in lots:
        k = sort_key(lot)
        at = next((i for i, o in enumerate(out) if sort_key(o) >= k), len(out))
        out.insert(at, lot)
    return out


def from_txn(book, op):
    t = find_txn(book, op["find"])
    splits = t.GetSplitList()
    pay_splits = [s for s in splits if s.GetAccount().GetType() in PAYMENT_TYPES]
    assert len(pay_splits) == 1, "the oracle handles one transfer split"
    xfer_split = pay_splits[0]
    apar = [s for s in splits if s.GetAccount().GetType() in APAR_TYPES]
    tx_lots, post_acct = [], None
    for s in apar:
        lot = as_lot(s.GetLot())
        if lot is not None:
            tx_lots.insert(0, (lot, s.GetValue()))
            post_acct = s.GetAccount()
    if post_acct is None and apar:
        post_acct = apar[-1].GetAccount()
    tx_post_acct = post_acct
    if op.get("account"):
        post_acct = bp.find_account(book, op["account"])
    owner_obj, kind = bp.find_owner(book, op["owner"])
    owner = c_owner(owner_obj, kind)
    items = dialog_items(book, owner, post_acct, tx_lots, tx_post_acct)
    if "applyTo" in op:
        chosen = [bp.find_lot(book, i) for i in op["applyTo"]]
        by_guid = {key(i[0]): n for n, i in enumerate(items)}
        chosen.sort(key=lambda l: by_guid.get(key(l), len(items)))
    else:
        tx_guids = {key(l) for l, _ in tx_lots}
        chosen = [i[0] for i in items if key(i[0]) in tx_guids]
    chosen = insert_sorted(chosen)
    if "amount" in op:
        amount = bp.num(op["amount"])
        if kind != "gncCustomer":
            amount = amount.neg()
    else:
        amount = xfer_split.GetAmount()
    date = bp.neutral(op["date"]) if op.get("date") else bp.neutral(t.GetDate().date().isoformat())
    memo = op.get("memo", t.GetDescription())
    num = op.get("num", t.GetNum())
    glist = None
    for lot in chosen:
        glist = glib.g_list_append(glist, ptr(lot))
    f = engine.gncOwnerApplyPaymentSecs
    f.restype = None
    f.argtypes = [ctypes.POINTER(COwner), ctypes.POINTER(ctypes.c_void_p), ctypes.c_void_p, ctypes.c_void_p, ctypes.c_void_p,
                  CNumeric, CNumeric, ctypes.c_int64, ctypes.c_char_p, ctypes.c_char_p, ctypes.c_bool]
    preset = ctypes.c_void_p(ptr(t).value)
    f(ctypes.byref(owner), ctypes.byref(preset), glist, ptr(post_acct), ptr(xfer_split.GetAccount()), cnum(amount),
      CNumeric(1, 1), ts(date), memo.encode(), num.encode(), bool(op.get("autoPay", False)))


def do(path, ops_path):
    with open(ops_path) as f:
        ops = json.load(f)
    s = bp.Session("xml://" + path, bp.SessionOpenMode.SESSION_NORMAL_OPEN)
    try:
        book = s.book
        for op in ops:
            kind = op["op"]
            if kind == "post":
                bp.post(book, op)
            elif kind == "unpost":
                bp.find_invoice(book, op["invoice"]).Unpost(op.get("reset", False))
            elif kind == "pay":
                bp.pay(book, op)
            elif kind == "payInvoice":
                bp.pay_invoice(book, op)
            elif kind == "txn":
                txn(book, op)
            elif kind == "fromTxn":
                from_txn(book, op)
            else:
                raise ValueError(kind)
        book.get_root_account().SetName(book.get_root_account().GetName())
        s.save()
    finally:
        s.end()


# ----------------------------------------------------------------------------------------------------------------
# openitems, scrub

def openitems(path, owner_name, account):
    s = bp.Session("xml://" + path, bp.SessionOpenMode.SESSION_READ_ONLY)
    try:
        book = s.book
        owner_obj, kind = bp.find_owner(book, owner_name)
        items = dialog_items(book, c_owner(owner_obj, kind), bp.find_account(book, account), [], None)
        ordered = insert_sorted([i[0] for i in items])
        out = []
        for lot in ordered:
            inv = lot.GetInvoiceFromLot()
            out.append({"id": inv.GetID() if inv else None, "balance": bp.frac(lot.get_balance()), "sortDate": sort_key(lot)})
        return out
    finally:
        s.end()


def scrub(path):
    s = bp.Session("xml://" + path, bp.SessionOpenMode.SESSION_NORMAL_OPEN)
    try:
        root = s.book.get_root_account()
        engine.gncScrubBusinessAccountTree.argtypes = [ctypes.c_void_p, type(NO_PROGRESS)]
        engine.gncScrubBusinessAccountTree(ptr(root), NO_PROGRESS)
        root.SetName(root.GetName())
        s.save()
    finally:
        s.end()


if __name__ == "__main__":
    os.environ.setdefault("TZ", "UTC")
    bp.check_version()
    cmd = sys.argv[1]
    if cmd == "make":
        make(sys.argv[2])
    elif cmd == "do":
        do(sys.argv[2], sys.argv[3])
    elif cmd == "view":
        json.dump(bp.view(sys.argv[2]), sys.stdout, indent=1, sort_keys=True, ensure_ascii=False)
    elif cmd == "totals":
        json.dump(bp.totals(sys.argv[2]), sys.stdout, indent=1, sort_keys=True, ensure_ascii=False)
    elif cmd == "openitems":
        json.dump(openitems(sys.argv[2], sys.argv[3], sys.argv[4]), sys.stdout, indent=1, sort_keys=True)
    elif cmd == "scrub":
        scrub(sys.argv[2])
    else:
        raise SystemExit(f"unknown command {cmd}")
