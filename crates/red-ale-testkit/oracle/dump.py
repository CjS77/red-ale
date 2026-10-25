#!/usr/bin/env python3
"""Load a book with the GnuCash 5.17 oracle (read-only) and print GnuCash's view of it as canonical JSON.

red-ale-testkit's `model::dump` builds the same JSON from a red-ale Book, so the two can be compared.

Conventions, shared with the Rust side:
- amounts are reduced fractions "num/denom" (GnuCash reduces what it returns; stored denominators are checked by the
  byte-level round-trip tests instead);
- times are epoch seconds (run with TZ=UTC: the bindings return naive local datetimes);
- lists are sorted by GUID or, where the bindings expose no GUID, by a natural key;
- currencies appear only when an account, transaction or price uses them.

Usage: PYTHONPATH=<5.17 bindings> TZ=UTC dump.py <book>
"""
import json
import math
import os
import sys
from fractions import Fraction

from gnucash import Account, GncLot, Query, Session, SessionOpenMode, Transaction, _sw_core_utils
from gnucash import gnucash_core_c as c
from gnucash.gnucash_business import BillTerm, Customer, Employee, Entry, Invoice, Job, TaxTable, Vendor


def frac(n):
    if n is None:
        return None
    # GncNumeric has num()/denom() methods; the raw struct some getters return has num/denom fields.
    num = n.num() if callable(n.num) else n.num
    den = n.denom() if callable(n.denom) else n.denom
    if den < 0:  # GnuCash's "reciprocal" form; never in files red-ale writes
        return f"{num}x{-den}"
    g = math.gcd(num, den) or 1
    return f"{num // g}/{den // g}"


def ts(dt):
    if dt is None:
        return None
    if isinstance(dt, (int, float)):
        return int(dt)
    return int(dt.timestamp())


def wrap(x, cls):
    """Business getters return raw SWIG pointers; wrap them in their class."""
    if x is None or cls is None or hasattr(x, "GetGUID") or hasattr(x, "GetName"):
        return x
    return cls(instance=x)


def raw(x):
    return getattr(x, "instance", x)


def guid(x, cls=None):
    x = wrap(x, cls)
    return x.GetGUID().to_string() if x is not None else None


def text(s):
    return s if s else None


def commodity_id(cm):
    return cm.get_unique_name() if cm is not None else None


def quote_source(cm):
    qs = cm.get_quote_source()
    return c.gnc_quote_source_get_internal_name(qs) if qs is not None else None


def accounts(root):
    out = []
    for a in root.get_descendants():
        parent = a.get_parent()
        out.append({
            "guid": guid(a),
            "name": a.GetName(),
            "type": c.xaccAccountTypeEnumAsString(a.GetType()),
            "commodity": commodity_id(a.GetCommodity()),
            "scu": a.GetCommoditySCU(),
            "nonStandardScu": bool(a.GetNonStdSCU()),
            "parent": None if parent is None or parent.is_root() else guid(parent),
            "code": text(a.GetCode()),
            "description": text(a.GetDescription()),
            "notes": text(a.GetNotes()),
            "placeholder": bool(a.GetPlaceholder()),
            "hidden": bool(a.GetHidden()),
            "taxRelated": bool(a.GetTaxRelated()),
            "taxCode": text(a.GetTaxUSCode()),
            "taxSource": text(a.GetTaxUSPayerNameSource()),
            "taxCopyNumber": a.GetTaxUSCopyNumber(),
            "balance": frac(a.GetBalance()),
        })
    return sorted(out, key=lambda x: x["guid"])


def splits_of(t):
    out = []
    for s in t.GetSplitList():
        lot = s.GetLot()
        out.append({
            "guid": guid(s),
            "account": guid(s.GetAccount()) if s.GetAccount() else None,
            "memo": text(s.GetMemo()),
            "action": text(s.GetAction()),
            "reconcile": s.GetReconcile(),
            "reconcileDate": ts(s.GetDateReconciled()) if s.GetReconcile() == "y" else None,
            "value": frac(s.GetValue()),
            "amount": frac(s.GetAmount()),
            "lotTitle": GncLot(instance=lot).get_title() if lot is not None else None,
        })
    return sorted(out, key=lambda x: x["guid"])


def transactions(root):
    seen = {}
    for a in root.get_descendants():
        for s in a.GetSplitList():
            t = s.GetParent()
            g = guid(t)
            if g in seen:
                continue
            seen[g] = {
                "guid": g,
                "currency": commodity_id(t.GetCurrency()),
                "num": text(t.GetNum()),
                "datePosted": ts(t.GetDate()),
                "dateEntered": ts(t.GetDateEntered()),
                "description": text(t.GetDescription()),
                "notes": text(t.GetNotes()),
                "void": bool(t.GetVoidStatus()),
                "voidReason": text(t.GetVoidReason()) if t.GetVoidStatus() else None,
                "splits": splits_of(t),
            }
    return [seen[k] for k in sorted(seen)]


def commodities(book, used):
    out = []
    for ns in book.get_table().get_namespaces_list():
        name = ns.get_name()
        if name == "template":
            continue
        for cm in ns.get_commodity_list():
            uid = cm.get_unique_name()
            entry = {"id": uid, "fraction": cm.get_fraction(), "quoteFlag": bool(cm.get_quote_flag()),
                     "quoteSource": quote_source(cm) if cm.get_quote_flag() else None,
                     "quoteTz": text(cm.get_quote_tz()), "userSymbol": text(cm.get_user_symbol())}
            if name == "CURRENCY":
                if uid in used:
                    out.append(entry)
                continue
            entry.update({"fullname": text(cm.get_fullname()), "cusip": text(cm.get_cusip())})
            out.append(entry)
    return sorted(out, key=lambda x: x["id"])


def prices(book):
    pdb = book.get_price_db()
    out = []
    for ns in book.get_table().get_namespaces_list():
        for cm in ns.get_commodity_list():
            for p in pdb.get_prices(cm, None):
                out.append({
                    "commodity": commodity_id(p.get_commodity()),
                    "currency": commodity_id(p.get_currency()),
                    "time": ts(p.get_time64()),
                    "source": p.get_source_string(),
                    "type": text(p.get_typestr()),
                    "value": frac(p.get_value()),
                })
    return sorted(out, key=lambda x: (x["commodity"], x["currency"], x["time"], x["source"] or "", x["type"] or "", x["value"]))


def lot_gains(lot):
    """The lot viewer's realised gains: the values of the zero-amount splits in the currency of the first one
    (find_first_currency, get_realized_gains, 5.17:gnucash/gnome/dialog-lot-viewer.c:146-187)."""
    lot.get_earliest_split()  # sorts the split list by date, as the viewer's fill does first
    zero = [s for s in lot.get_split_list() if s.GetAmount().zero_p()]
    if not zero:
        return None, "0/1"
    currency = commodity_id(zero[0].GetParent().GetCurrency())
    total = sum((Fraction(s.GetValue().num(), s.GetValue().denom()) for s in zero
                 if commodity_id(s.GetParent().GetCurrency()) == currency), Fraction(0))
    return currency, f"{total.numerator}/{total.denominator}"


def lots(root):
    out = []
    for a in root.get_descendants():
        for lot in a.GetLotList():
            split_guids = sorted(guid(s) for s in lot.get_split_list())
            earliest, latest = lot.get_earliest_split(), lot.get_latest_split()
            gains_currency, gains = lot_gains(lot)
            out.append({
                "account": guid(a),
                "title": text(lot.get_title()),
                "notes": text(lot.get_notes()),
                "splits": split_guids,
                "balance": frac(lot.get_balance()),
                "closed": bool(lot.is_closed()),
                "openDate": ts(earliest.GetParent().GetDate()) if earliest else None,
                "closeDate": ts(latest.GetParent().GetDate()) if latest and lot.is_closed() else None,
                "gainsCurrency": gains_currency,
                "gains": gains,
            })
    return sorted(out, key=lambda x: (x["account"], x["splits"], x["title"] or ""))


def run_query(book, kind, cls):
    q = Query()
    q.search_for(kind)
    q.set_book(book)
    return [cls(instance=i) for i in q.run()]


def address(addr):
    if addr is None:
        return None
    fields = {k: text(getattr(addr, "Get" + k)()) for k in ("Name", "Addr1", "Addr2", "Addr3", "Addr4", "Phone", "Fax", "Email")}
    out = {k[0].lower() + k[1:]: v for k, v in fields.items()}
    return out if any(out.values()) else None


def owner_ref(owner):
    if owner is None:
        return None
    g = owner.GetGUID()
    return {"type": owner.GetType(), "guid": g.to_string() if g is not None else None}


def name_of(x, cls):
    x = wrap(x, cls)
    return x.GetName() if x is not None else None


def business(book):
    customers = [{
        "guid": guid(x), "id": x.GetID(), "name": x.GetName(), "notes": text(x.GetNotes()), "active": bool(x.GetActive()),
        "currency": commodity_id(x.GetCurrency()), "terms": name_of(x.GetTerms(), BillTerm), "taxTable": name_of(x.GetTaxTable(), TaxTable),
        "taxTableOverride": bool(x.GetTaxTableOverride()), "taxIncluded": x.GetTaxIncluded(),
        "discount": frac(x.GetDiscount()), "credit": frac(x.GetCredit()), "addr": address(x.GetAddr()),
        "shipAddr": address(x.GetShipAddr()),
    } for x in run_query(book, "gncCustomer", Customer)]
    vendors = [{
        "guid": guid(x), "id": x.GetID(), "name": x.GetName(), "notes": text(x.GetNotes()), "active": bool(x.GetActive()),
        "currency": commodity_id(x.GetCurrency()), "terms": name_of(x.GetTerms(), BillTerm), "taxTable": name_of(x.GetTaxTable(), TaxTable),
        "taxTableOverride": bool(x.GetTaxTableOverride()), "taxIncluded": x.GetTaxIncluded(), "addr": address(x.GetAddr()),
    } for x in run_query(book, "gncVendor", Vendor)]
    employees = [{
        "guid": guid(x), "id": x.GetID(), "username": text(x.GetUsername()), "active": bool(x.GetActive()),
        "currency": commodity_id(x.GetCurrency()), "language": text(x.GetLanguage()), "acl": text(x.GetAcl()),
        "workday": frac(x.GetWorkday()), "rate": frac(x.GetRate()),
        "ccard": guid(x.GetCCard(), Account), "addr": address(x.GetAddr()),
    } for x in run_query(book, "gncEmployee", Employee)]
    jobs = [{
        "guid": guid(x), "id": x.GetID(), "name": x.GetName(), "reference": text(x.GetReference()), "active": bool(x.GetActive()),
        "rate": frac(x.GetRate()), "owner": owner_ref(x.GetOwner()),
    } for x in run_query(book, "gncJob", Job)]
    invoices = [{
        "id": x.GetID(), "type": x.GetTypeString(), "owner": owner_ref(x.GetOwner()), "notes": text(x.GetNotes()),
        "billingId": text(x.GetBillingID()), "active": bool(x.GetActive()), "currency": commodity_id(x.GetCurrency()),
        "terms": name_of(x.GetTerms(), BillTerm), "dateOpened": ts(x.GetDateOpened()),
        "datePosted": ts(x.GetDatePosted()) if x.IsPosted() else None,
        "dateDue": ts(x.GetDateDue()) if x.IsPosted() else None,
        "postedAccount": guid(x.GetPostedAcc(), Account),
        "postedTxn": guid(x.GetPostedTxn(), Transaction),
        "toChargeAmount": frac(x.GetToChargeAmount()), "total": frac(x.GetTotal()),
        "subtotal": frac(x.GetTotalSubtotal()), "tax": frac(x.GetTotalTax()), "paid": bool(x.IsPaid()) if x.IsPosted() else None,
        "entries": sorted(guid(e, Entry) for e in x.GetEntries()),
    } for x in run_query(book, "gncInvoice", Invoice)]
    entries = [{
        "guid": guid(x), "date": ts(x.GetDate()), "description": text(x.GetDescription()), "action": text(x.GetAction()),
        "notes": text(x.GetNotes()), "quantity": frac(x.GetQuantity()),
        "invAccount": guid(x.GetInvAccount(), Account), "invPrice": frac(x.GetInvPrice()),
        "billAccount": guid(x.GetBillAccount(), Account), "billPrice": frac(x.GetBillPrice()),
        "invTaxable": bool(x.GetInvTaxable()), "billTaxable": bool(x.GetBillTaxable()),
        "invTaxTable": name_of(x.GetInvTaxTable(), TaxTable), "billTaxTable": name_of(x.GetBillTaxTable(), TaxTable),
        "billable": bool(x.GetBillable()),
    } for x in run_query(book, "gncEntry", Entry)]
    # The bindings type GetEntries' GList as gnc_monetary, so entry details cannot be read; invoice tax totals
    # check them instead.
    tax_tables = [{"name": x.GetName(), "entryCount": len(x.GetEntries())}
                  for x in run_query(book, "gncTaxTable", TaxTable) if x.GetParent() is None]
    # Lists of bill terms come back mistyped from the bindings (as gnc_monetary), but single references work, so
    # the terms dumped are those a customer, vendor or invoice uses.
    referenced = [wrap(x.GetTerms(), BillTerm) for x in run_query(book, "gncCustomer", Customer) + run_query(book, "gncVendor", Vendor)
                  + run_query(book, "gncInvoice", Invoice)]
    terms = {}
    for x in referenced:
        if x is not None:
            terms[x.GetName()] = {
                "name": x.GetName(), "description": text(x.GetDescription()), "type": x.GetType(), "dueDays": x.GetDueDays(),
                "discountDays": x.GetDiscountDays(), "discount": frac(x.GetDiscount()), "cutoff": x.GetCutoff(),
            }
    bill_terms = list(terms.values())
    return {
        "customers": sorted(customers, key=lambda x: x["guid"]),
        "vendors": sorted(vendors, key=lambda x: x["guid"]),
        "employees": sorted(employees, key=lambda x: x["guid"]),
        "jobs": sorted(jobs, key=lambda x: x["guid"]),
        "invoices": sorted(invoices, key=lambda x: (x["type"], x["id"])),
        "entries": sorted(entries, key=lambda x: x["guid"]),
        "taxTables": sorted(tax_tables, key=lambda x: x["name"]),
        "billTerms": sorted(bill_terms, key=lambda x: x["name"]),
    }


def used_currencies(accts, txns, pris):
    used = {a["commodity"] for a in accts} | {t["currency"] for t in txns}
    used |= {p["currency"] for p in pris} | {p["commodity"] for p in pris}
    return {u for u in used if u and u.startswith("CURRENCY::")}


def main(path):
    version = _sw_core_utils.gnc_version()
    if version != "5.17":
        print(json.dumps({"error": f"the oracle must be GnuCash 5.17, found {version}"}))
        sys.exit(3)
    session = Session(("xml://" if "://" not in path else "") + path, SessionOpenMode.SESSION_READ_ONLY)
    try:
        book = session.book
        root = book.get_root_account()
        accts, txns, pris = accounts(root), transactions(root), prices(book)
        dump = {
            "gnucashVersion": version,
            "book": {"tradingAccounts": bool(book.use_trading_accounts()), "splitActionForNum": bool(book.use_split_action_for_num_field()),
                     "autoReadonlyDays": book.get_num_days_autoreadonly()},
            "commodities": commodities(book, used_currencies(accts, txns, pris)),
            "accounts": accts,
            "transactions": txns,
            "prices": pris,
            "lots": lots(root),
            "business": business(book),
        }
    finally:
        session.end()
    json.dump(dump, sys.stdout, indent=1, sort_keys=True, ensure_ascii=False)
    sys.stdout.write("\n")


if __name__ == "__main__":
    os.environ.setdefault("TZ", "UTC")
    main(sys.argv[1])
