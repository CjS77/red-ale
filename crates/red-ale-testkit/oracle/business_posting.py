#!/usr/bin/env python3
"""GnuCash 5.17 as the oracle for business posting, payments and totals.

Commands (run with PYTHONPATH=<5.17 bindings>, TZ=UTC):

  make <book>          create a book with customers, a vendor, an employee, a job, tax tables, billing terms and
                       unposted documents INV1, INV2, INV3 (job), NEG1 and NEG2 (negative invoices), BILL1, BILL2 (foreign
                       line) and EXP1 (voucher)
  do <book> <ops.json> perform operations in GnuCash's engine on the book and save it
  view <book>          print a GUID-free view of what the operations touch: transactions, lots, documents, tax
                       tables and billing terms, so books changed by GnuCash and by red-ale can be compared
  totals <book>        print each document's totals and entry values as GnuCash computes them

Operations (a JSON list), using document IDs, account full names and owner names:
  {"op": "post", "invoice": "INV1", "account": "Assets:Receivable", "postDate": "2024-03-05",
   "dueDate": null, "memo": "", "accumulate": true, "autopay": false, "prices": [["CURRENCY::EUR", "11/10"]]}
  {"op": "unpost", "invoice": "INV1", "reset": false}
  {"op": "pay", "owner": "Acme Ltd", "account": "Assets:Receivable", "transfer": "Assets:Bank", "amount": "100",
   "exch": "1", "date": "2024-03-20", "memo": "", "num": "", "applyTo": ["INV1"], "autoPay": false}
  {"op": "payInvoice", "invoice": "INV1", "transfer": "Assets:Bank", "amount": "100", "exch": "1",
   "date": "2024-03-20", "memo": "", "num": ""}
Payment amounts are signed as red-ale's API signs them: positive when a customer pays or a vendor is paid.
"""
import datetime
import gzip
import json
import math
import os
import sys
import xml.etree.ElementTree as ET
from fractions import Fraction

from gnucash import Account, GncNumeric, GncPrice, Query, Session, SessionOpenMode, _sw_core_utils
from gnucash.gnucash_business import (Bill, BillTerm, Customer, Employee, Entry, Invoice, Job, TaxTable, TaxTableEntry, Vendor,
                                      GNC_AMT_TYPE_VALUE, GNC_AMT_TYPE_PERCENT, GNC_DISC_POSTTAX, GNC_DISC_PRETAX,
                                      GNC_DISC_SAMETIME, GNC_PAYMENT_CARD)
from gnucash.gnucash_core_c import (ACCT_TYPE_ASSET, ACCT_TYPE_BANK, ACCT_TYPE_CREDIT, ACCT_TYPE_EXPENSE, ACCT_TYPE_INCOME,
                                    ACCT_TYPE_LIABILITY, ACCT_TYPE_PAYABLE, ACCT_TYPE_RECEIVABLE)


def num(text):
    f = Fraction(text)
    return GncNumeric(f.numerator, f.denominator)


def frac(n):
    if n is None:
        return None
    a = n.num() if callable(n.num) else n.num
    b = n.denom() if callable(n.denom) else n.denom
    g = math.gcd(a, b) or 1
    return f"{a // g}/{b // g}"


def day(text):
    return datetime.date.fromisoformat(text)


def neutral(text):
    d = day(text)
    return datetime.datetime(d.year, d.month, d.day, 10, 59)


def ts(dt):
    if dt is None:
        return None
    if isinstance(dt, (int, float)):
        return int(dt)
    return int(dt.timestamp())


def check_version():
    v = _sw_core_utils.gnc_version()
    if v != "5.17":
        print(json.dumps({"error": f"the oracle must be GnuCash 5.17, found {v}"}))
        sys.exit(3)


# ----------------------------------------------------------------------------------------------------------------
# make

def make(path):
    s = Session("xml://" + path, SessionOpenMode.SESSION_NEW_OVERWRITE)
    try:
        book = s.book
        root = book.get_root_account()
        table = book.get_table()
        USD = table.lookup("CURRENCY", "USD")
        EUR = table.lookup("CURRENCY", "EUR")

        def acct(parent, name, typ, comm=USD):
            a = Account(book)
            parent.append_child(a)
            a.SetName(name)
            a.SetType(typ)
            a.SetCommodity(comm)
            return a

        assets = acct(root, "Assets", ACCT_TYPE_ASSET)
        ar = acct(assets, "Receivable", ACCT_TYPE_RECEIVABLE)
        bank = acct(assets, "Bank", ACCT_TYPE_BANK)
        acct(assets, "Euro Bank", ACCT_TYPE_BANK, EUR)
        liab = acct(root, "Liabilities", ACCT_TYPE_LIABILITY)
        ap = acct(liab, "Payable", ACCT_TYPE_PAYABLE)
        gst = acct(liab, "GST", ACCT_TYPE_LIABILITY)
        pst = acct(liab, "PST", ACCT_TYPE_LIABILITY)
        fee = acct(liab, "Fee", ACCT_TYPE_LIABILITY)
        vat = acct(liab, "VAT", ACCT_TYPE_LIABILITY)
        card = acct(liab, "Card", ACCT_TYPE_CREDIT)
        income = acct(root, "Income", ACCT_TYPE_INCOME)
        sales = acct(income, "Sales", ACCT_TYPE_INCOME)
        services = acct(income, "Services", ACCT_TYPE_INCOME)
        expenses = acct(root, "Expenses", ACCT_TYPE_EXPENSE)
        supplies = acct(expenses, "Supplies", ACCT_TYPE_EXPENSE)
        travel = acct(expenses, "Travel", ACCT_TYPE_EXPENSE)
        euro_costs = acct(expenses, "Euro Costs", ACCT_TYPE_EXPENSE, EUR)

        net30 = BillTerm(book)
        net30.BeginEdit()
        net30.SetName("Net30")
        net30.SetDescription("Net 30")
        net30.SetType(1)
        net30.SetDueDays(30)
        net30.CommitEdit()
        prox = BillTerm(book)
        prox.BeginEdit()
        prox.SetName("Prox20")
        prox.SetDescription("20th of next month")
        prox.SetType(2)
        prox.SetDueDays(20)
        prox.SetCutoff(19)
        prox.CommitEdit()

        gstpst = TaxTable(book, "GST+PST", TaxTableEntry(gst, True, num("5")))
        gstpst.AddEntry(TaxTableEntry(pst, True, num("15/2")))
        feevat = TaxTable(book, "Fee+VAT", TaxTableEntry(fee, False, num("2")))
        feevat.AddEntry(TaxTableEntry(vat, True, num("10")))

        acme = Customer(book, book.CustomerNextID(), USD, "Acme Ltd")
        acme.BeginEdit()
        acme.SetTerms(net30)
        acme.CommitEdit()
        beta = Customer(book, book.CustomerNextID(), USD, "Beta Co")
        paper = Vendor(book, book.VendorNextID(), USD, "Paper Co")
        paper.BeginEdit()
        paper.SetTerms(prox)
        paper.CommitEdit()
        jane = Employee(book, "E1", USD, "Jane Doe")
        jane.BeginEdit()
        jane.GetAddr().SetName("Jane Doe")
        jane.SetCCard(card)
        jane.CommitEdit()
        job = Job(book, "J1", acme, "Website")

        opened = datetime.date(2024, 3, 1)
        stamp = [datetime.datetime(2024, 3, 1, 9, 0, 0)]

        def entry(doc, bill, date, desc, qty, price, account, table=None, incl=False, disc=None, dtype=GNC_AMT_TYPE_PERCENT,
                  dhow=GNC_DISC_PRETAX, taxable=True, pay=None, billable_to=None):
            e = Entry(book)
            e.SetDate(day(date))
            stamp[0] += datetime.timedelta(minutes=1)
            e.SetDateEntered(stamp[0])
            e.SetDescription(desc)
            e.SetQuantity(num(qty))
            if bill:
                e.SetBillAccount(account)
                e.SetBillPrice(num(price))
                e.SetBillTaxable(taxable)
                e.SetBillTaxIncluded(incl)
                if table is not None:
                    e.SetBillTaxTable(table)
                if pay is not None:
                    e.SetBillPayment(pay)
                if billable_to is not None:
                    e.SetBillable(True)
                    e.SetBillTo(billable_to)
                doc.AddEntry(e)
            else:
                e.SetInvAccount(account)
                e.SetInvPrice(num(price))
                e.SetInvTaxable(taxable)
                e.SetInvTaxIncluded(incl)
                if table is not None:
                    e.SetInvTaxTable(table)
                if disc is not None:
                    e.SetInvDiscount(num(disc))
                    e.SetInvDiscountType(dtype)
                    e.SetInvDiscountHow(dhow)
                doc.AddEntry(e)
            return e

        inv1 = Invoice(book, "INV1", USD, acme, opened)
        inv1.BeginEdit()
        inv1.SetTerms(net30)
        inv1.CommitEdit()
        entry(inv1, False, "2024-03-02", "Widgets", "3", "33.33", sales, gstpst, disc="10")
        entry(inv1, False, "2024-03-02", "Consulting", "2", "107.50", services, gstpst, incl=True)
        entry(inv1, False, "2024-03-03", "Support", "5", "20", services, feevat, disc="10", dhow=GNC_DISC_POSTTAX)
        entry(inv1, False, "2024-03-03", "Gadget", "1", "10.01", sales, taxable=False)
        entry(inv1, False, "2024-03-04", "Fixed fee", "4", "5", services, feevat, disc="3", dtype=GNC_AMT_TYPE_VALUE)
        entry(inv1, False, "2024-03-04", "Same time", "1/3", "1", sales, feevat, disc="10", dhow=GNC_DISC_SAMETIME)

        inv2 = Invoice(book, "INV2", USD, acme, opened)
        entry(inv2, False, "2024-03-06", "Hosting", "1", "50", services)

        inv3 = Invoice(book, "INV3", USD, job, opened)
        entry(inv3, False, "2024-03-06", "Design", "8", "90", services, gstpst)
        entry(inv3, False, "2024-03-07", "Design", "2", "90", services, gstpst)

        neg = Invoice(book, "NEG1", USD, acme, opened)
        entry(neg, False, "2024-03-08", "Goodwill", "-1", "30", sales)
        neg2 = Invoice(book, "NEG2", USD, acme, opened)
        entry(neg2, False, "2024-03-08", "Rebate", "-1", "10", sales)

        bill1 = Bill(book, "BILL1", USD, paper, opened)
        bill1.BeginEdit()
        bill1.SetTerms(prox)
        bill1.CommitEdit()
        entry(bill1, True, "2024-03-02", "Paper", "3", "19.99", supplies, feevat, incl=True, billable_to=acme)
        entry(bill1, True, "2024-03-03", "Toner", "1", "40", supplies, gstpst)
        entry(bill1, True, "2024-03-03", "Courier", "1", "12.50", travel)

        bill2 = Bill(book, "BILL2", USD, paper, opened)
        entry(bill2, True, "2024-03-04", "Euro service", "1", "110", euro_costs)
        entry(bill2, True, "2024-03-04", "Local service", "1", "15", supplies)

        exp1 = Bill(book, "EXP1", USD, jane, opened)
        exp1.BeginEdit()
        exp1.SetToChargeAmount(num("5"))
        exp1.CommitEdit()
        entry(exp1, True, "2024-03-04", "Taxi", "1", "25", travel, pay=GNC_PAYMENT_CARD)
        entry(exp1, True, "2024-03-04", "Lunch", "1", "15", travel)
        entry(exp1, True, "2024-03-05", "Hotel", "1", "120", travel, pay=GNC_PAYMENT_CARD)
        s.save()
    finally:
        s.end()


# ----------------------------------------------------------------------------------------------------------------
# do

def find_invoice(book, ident):
    q = Query()
    q.search_for("gncInvoice")
    q.set_book(book)
    for i in q.run():
        inv = Invoice(instance=i)
        if inv.GetID() == ident:
            return inv
    raise KeyError(ident)


def find_owner(book, name):
    for kind, cls in (("gncCustomer", Customer), ("gncVendor", Vendor), ("gncEmployee", Employee), ("gncJob", Job)):
        q = Query()
        q.search_for(kind)
        q.set_book(book)
        for i in q.run():
            o = cls(instance=i)
            if o.GetName() == name:
                return o, kind
    raise KeyError(name)


def find_account(book, name):
    a = book.get_root_account()
    for part in name.split(":"):
        a = next((c for c in a.get_children() if c.GetName() == part), None)
        if a is None:
            raise KeyError(name)
    return a


def find_lot(book, ident):
    return find_invoice(book, ident).GetPostedLot()


def post(book, op):
    inv = find_invoice(book, op["invoice"])
    acc = find_account(book, op["account"])
    post_date = day(op["postDate"])
    if op.get("dueDate"):
        due = day(op["dueDate"])
    elif inv.GetTerms() is not None:
        terms = inv.GetTerms()
        due = terms.ComputeDueDate(datetime.datetime.combine(post_date, datetime.time(0, 0)))
    else:
        due = post_date
    for comm, value in op.get("prices", []):
        ns, mnemonic = comm.split("::")
        p = GncPrice(book)
        p.begin_edit()
        p.set_commodity(book.get_table().lookup(ns, mnemonic))
        p.set_currency(inv.GetCurrency())
        p.set_value(num(value))
        p.commit_edit()
        inv.AddPrice(p)
    inv.PostToAccount(acc, post_date, due, op.get("memo", ""), op.get("accumulate", True), op.get("autopay", False))


def pay(book, op):
    owner, kind = find_owner(book, op["owner"])
    amount = num(op["amount"])
    end_kind = kind if kind != "gncJob" else ("gncCustomer" if owner.GetOwner().GetType() == 2 else "gncVendor")
    if end_kind != "gncCustomer":
        amount = amount.neg()
    lots = [find_lot(book, i) for i in op.get("applyTo", [])]
    transfer = find_account(book, op["transfer"]) if op.get("transfer") else None
    apply = Customer.ApplyPayment
    apply(owner, None, lots, find_account(book, op["account"]), transfer, amount, num(op.get("exch", "1")),
          neutral(op["date"]), op.get("memo", ""), op.get("num", ""), op.get("autoPay", False))


def pay_invoice(book, op):
    inv = find_invoice(book, op["invoice"])
    amount = num(op["amount"])
    if inv.GetOwnerType() != 2:
        amount = amount.neg()
    inv.ApplyPayment(None, find_account(book, op["transfer"]), amount, num(op.get("exch", "1")), neutral(op["date"]),
                     op.get("memo", ""), op.get("num", ""))


def do(path, ops_path):
    with open(ops_path) as f:
        ops = json.load(f)
    s = Session("xml://" + path, SessionOpenMode.SESSION_NORMAL_OPEN)
    try:
        book = s.book
        for op in ops:
            if op["op"] == "post":
                post(book, op)
            elif op["op"] == "unpost":
                find_invoice(book, op["invoice"]).Unpost(op.get("reset", False))
            elif op["op"] == "pay":
                pay(book, op)
            elif op["op"] == "payInvoice":
                pay_invoice(book, op)
            else:
                raise ValueError(op["op"])
        s.save()
    finally:
        s.end()


# ----------------------------------------------------------------------------------------------------------------
# view

def xml_root(path):
    with open(path, "rb") as f:
        data = f.read()
    if data[:2] == b"\x1f\x8b":
        data = gzip.decompress(data)
    return ET.fromstring(data)


def local(tag):
    return tag.split("}")[-1] if "}" in tag else tag.split(":")[-1]


def child(el, name):
    for c in el:
        if local(c.tag) == name:
            return c
    return None


def text(el, name):
    c = child(el, name)
    return c.text if c is not None else None


def labels(root):
    """GUID -> label for everything an operation may reference."""
    out = {}
    book = next(c for c in root if local(c.tag) == "book")
    accounts = {}
    for a in book:
        if local(a.tag) == "account":
            parent = child(a, "parent")
            accounts[text(a, "id")] = (text(a, "name"), parent.text if parent is not None else None, text(a, "type"))

    def full(g):
        name, parent, typ = accounts[g]
        if typ == "ROOT":
            return ""
        p = full(parent) if parent else ""
        return f"{p}:{name}" if p else name

    for g in accounts:
        out[g] = "account:" + full(g)
    for el in book:
        tag = local(el.tag)
        if tag == "GncInvoice":
            out[text(el, "guid")] = "invoice:" + text(el, "id")
        elif tag in ("GncCustomer", "GncVendor", "GncJob"):
            out[text(el, "guid")] = "owner:" + text(el, "name")
        elif tag == "GncEmployee":
            out[text(el, "guid")] = "owner:" + (text(child(el, "addr"), "name") or "")
        elif tag in ("GncTaxTable", "GncBillTerm"):
            frozen = " (frozen copy)" if child(el, "parent") is not None else ""
            out[text(el, "guid")] = tag + ":" + text(el, "name") + frozen
        elif tag == "transaction":
            out[text(el, "id")] = "txn:" + (text(el, "num") or "") + " " + text(child(el, "date-posted"), "date")
    return book, out


def slots_of(el, guid_labels):
    """A frame as a dict, GUID values replaced by labels."""
    out = {}
    if el is None:
        return out
    for slot in el:
        key = text(slot, "key")
        value = child(slot, "value")
        typ = value.get("type")
        if typ == "frame":
            out[key] = slots_of(value, guid_labels)
        elif typ == "guid":
            out[key] = guid_labels.get(value.text, "guid?")
        elif typ in ("timespec",):
            out[key] = text(value, "date")
        elif typ == "gdate":
            out[key] = text(value, "gdate")
        else:
            out[key] = value.text
    return out


def view(path):
    root = xml_root(path)
    book, lab = labels(root)
    lot_labels = {}
    lots = []
    for a in book:
        if local(a.tag) != "account":
            continue
        lots_el = child(a, "lots")
        for lot in (lots_el if lots_el is not None else []):
            g = text(lot, "id")
            slots = slots_of(child(lot, "slots"), lab)
            owner = slots.get("gncOwner", {}).get("owner-guid")
            lot_labels[g] = slots.get("title") or ("prepayment of " + str(owner))
            lots.append({"account": lab[text(a, "id")], "label": lot_labels[g], "slots": slots, "splits": []})
    lab.update({g: "lot:" + label for g, label in lot_labels.items()})
    txns = []
    by_lot = {}
    for t in book:
        if local(t.tag) != "transaction":
            continue
        splits = []
        for s in child(t, "splits"):
            lot = text(s, "lot")
            sp = {
                "account": lab.get(text(s, "account")), "memo": text(s, "memo"), "action": text(s, "action"),
                "reconciled": text(s, "reconciled-state"), "value": text(s, "value"), "quantity": text(s, "quantity"),
                "lot": lot_labels.get(lot) if lot else None, "slots": slots_of(child(s, "slots"), lab),
            }
            splits.append(sp)
            if lot:
                by_lot.setdefault(lot, []).append([text(t, "num"), text(s, "value")])
        txns.append({
            "currency": text(child(t, "currency"), "id"), "num": text(t, "num"), "description": text(t, "description"),
            "datePosted": text(child(t, "date-posted"), "date"), "slots": slots_of(child(t, "slots"), lab), "splits": splits,
        })
    lot_by_label = {}
    for g, label in lot_labels.items():
        lot_by_label.setdefault(label, []).extend(sorted(by_lot.get(g, []), key=lambda x: (x[0] or "", x[1])))
    for lot in lots:
        lot["splits"] = lot_by_label.get(lot["label"], [])
    business = []
    for el in book:
        tag = local(el.tag)
        if tag in ("GncInvoice", "GncEntry", "GncTaxTable", "GncBillTerm"):
            business.append(business_el(el, lab, tag))
    key = lambda t: (t["datePosted"], t["num"] or "", t["description"] or "", json.dumps(t["splits"], sort_keys=True))
    return {
        "transactions": sorted(txns, key=key),
        "lots": sorted(lots, key=lambda l: (l["account"], l["label"], json.dumps(l["splits"]))),
        "business": sorted(business, key=lambda b: json.dumps(b, sort_keys=True)),
    }


def business_el(el, lab, tag):
    """A business object's XML with GUIDs replaced by labels; tax tables and terms are named by name and family."""
    def family(e):
        name = text(e, "name")
        return name + (" (frozen copy)" if child(e, "parent") is not None else "")
    out = {"kind": tag}
    for c in el:
        name = local(c.tag)
        if name in ("guid",):
            continue
        if c.get("type") == "guid" or name in ("parent", "child", "i-taxtable", "b-taxtable", "terms", "postlot", "posttxn",
                                                "postacc", "acct", "i-acct", "b-acct", "invoice", "bill"):
            out[name] = lab.get(c.text, "ref") if c.text else None
        elif name == "entries":
            out[name] = [{local(x.tag): (lab.get(x.text, x.text) if x.get("type") == "guid" else x.text) for x in e} for e in c]
        elif name == "slots":
            out[name] = slots_of(c, lab)
        elif len(c):
            out[name] = {local(x.tag): x.text for x in c}
        else:
            out[name] = c.text
    if tag in ("GncTaxTable", "GncBillTerm"):
        out["family"] = family(el)
    return out


# ----------------------------------------------------------------------------------------------------------------
# totals

def totals(path):
    s = Session("xml://" + path, SessionOpenMode.SESSION_READ_ONLY)
    out = {}
    try:
        book = s.book
        q = Query()
        q.search_for("gncInvoice")
        q.set_book(book)
        for i in q.run():
            inv = Invoice(instance=i)
            cust = inv.GetOwnerType() == 2
            cn = bool(inv.GetIsCreditNote())
            entries = sorted(inv.GetEntries(), key=lambda e: e.GetDescription())
            out[inv.GetID()] = {
                "subtotal": frac(inv.GetTotalSubtotal()), "tax": frac(inv.GetTotalTax()), "total": frac(inv.GetTotal()),
                "entries": [{"description": e.GetDescription(), "value": frac(e.GetDocValue(True, cust, cn)),
                             "discount": frac(e.GetDocDiscountValue(True, cust, cn)), "tax": frac(e.GetDocTaxValue(True, cust, cn))}
                            for e in entries],
            }
    finally:
        s.end()
    return out


if __name__ == "__main__":
    os.environ.setdefault("TZ", "UTC")
    check_version()
    cmd = sys.argv[1]
    if cmd == "make":
        make(sys.argv[2])
    elif cmd == "do":
        do(sys.argv[2], sys.argv[3])
    elif cmd == "view":
        json.dump(view(sys.argv[2]), sys.stdout, indent=1, sort_keys=True, ensure_ascii=False)
    elif cmd == "totals":
        json.dump(totals(sys.argv[2]), sys.stdout, indent=1, sort_keys=True, ensure_ascii=False)
    else:
        raise SystemExit(f"unknown command {cmd}")
