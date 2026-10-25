//! red-ale's view of a book in the oracle's JSON shape (`oracle/dump.py`), so the two can be compared.
//!
//! Each field says what GnuCash would report for the data red-ale holds. Where GnuCash derives a value at load (a
//! currency's quote flag, a lot's closed state), the derivation is reproduced here and cited.

use std::collections::BTreeSet;

use red_ale_core::account::slot_paths as acct;
use red_ale_core::business::{Address, OwnerKind, OwnerRef, TaxIncluded};
use red_ale_core::ops::commit::commodity_fraction;
use red_ale_core::ops::commodities::{effective_quote, lookup, slot_paths, usage_table};
use red_ale_core::transaction::slot_paths as txn;
use red_ale_core::{Book, CommodityRef, Guid, Numeric, SlotValue, Transaction};
use serde_json::{Value, json};

/// A reduced fraction `num/denom`, as the oracle prints amounts.
pub fn frac(n: Numeric) -> Value {
    let r = n.reduce();
    Value::String(format!("{}/{}", r.num(), r.denom()))
}

fn text(s: Option<&str>) -> Value {
    s.filter(|s| !s.is_empty()).map_or(Value::Null, |s| Value::String(s.to_owned()))
}

fn guid(g: Option<Guid>) -> Value {
    g.map_or(Value::Null, |g| Value::String(g.to_string()))
}

fn sum(values: impl Iterator<Item = Numeric>) -> Numeric {
    values.fold(Numeric::ZERO, |acc, v| acc.add_exact(v).unwrap_or(acc))
}

/// Every account but the root.
fn accounts(book: &Book) -> Vec<Value> {
    let mut out: Vec<Value> = book
        .accounts
        .values()
        .filter(|a| a.guid != book.root)
        .map(|a| {
            let fraction = a.commodity.as_ref().and_then(|c| commodity_fraction(book, c)).unwrap_or(0);
            json!({
                "guid": a.guid.to_string(),
                "name": a.name,
                "type": a.account_type.as_str(),
                "commodity": a.commodity.as_ref().map(ToString::to_string),
                "scu": a.scu(fraction),
                // xaccAccountSetCommoditySCU marks the account non-standard when the SCU read differs from the commodity's
                // fraction (5.17:libgnucash/engine/Account.cpp:2719-2720).
                "nonStandardScu": a.non_standard_scu || a.commodity_scu.is_some_and(|scu| scu != fraction),
                "parent": a.parent.filter(|p| *p != book.root).map(|p| p.to_string()),
                "code": text(a.code.as_deref()),
                "description": text(a.description.as_deref()),
                "notes": text(a.notes()),
                "placeholder": a.is_placeholder(),
                "hidden": a.is_hidden(),
                "taxRelated": a.is_tax_related(),
                "taxCode": text(a.slots.get_str(acct::TAX_US_CODE)),
                "taxSource": text(a.slots.get_str(acct::TAX_US_PAYER_NAME_SOURCE)),
                // xaccAccountGetTaxUSCopyNumber returns 1 when unset (5.17:libgnucash/engine/Account.cpp).
                "taxCopyNumber": a.slots.get(acct::TAX_US_COPY_NUMBER).and_then(SlotValue::as_int).unwrap_or(1),
                "balance": frac(sum(book.splits_of(a.guid).map(|(_, s)| s.amount))),
            })
        })
        .collect();
    out.sort_by(|x, y| x["guid"].as_str().cmp(&y["guid"].as_str()));
    out
}

fn lot_title(book: &Book, lot: Option<Guid>) -> Value {
    lot.and_then(|l| book.lot(l)).map_or(Value::Null, |l| Value::String(l.title().unwrap_or_default().to_owned()))
}

fn transaction(book: &Book, t: &Transaction) -> Value {
    let mut splits: Vec<Value> = t
        .splits
        .iter()
        .map(|s| {
            json!({
                "guid": s.guid.to_string(),
                "account": guid(s.account),
                "memo": text(s.memo.as_deref()),
                "action": text(s.action.as_deref()),
                "reconcile": s.reconcile_state.as_char().to_string(),
                "reconcileDate": if s.reconcile_state.as_char() == 'y' { s.reconcile_date.map_or(json!(0), |d| json!(d.0)) } else { Value::Null },
                "value": frac(s.value),
                "amount": frac(s.amount),
                "lotTitle": lot_title(book, s.lot),
            })
        })
        .collect();
    splits.sort_by(|x, y| x["guid"].as_str().cmp(&y["guid"].as_str()));
    json!({
        "guid": t.guid.to_string(),
        "currency": t.currency.to_string(),
        "num": text(t.num.as_deref()),
        "datePosted": t.date_posted.0,
        "dateEntered": t.date_entered.0,
        "description": text(t.description.as_deref()),
        "notes": text(t.notes()),
        "void": t.is_void(),
        "voidReason": if t.is_void() { text(t.slots.get_str(txn::VOID_REASON)) } else { Value::Null },
        "splits": splits,
    })
}

/// Transactions GnuCash reaches through its accounts: those with a split in some account.
fn transactions(book: &Book) -> Vec<Value> {
    let mut out: Vec<Value> =
        book.transactions.values().filter(|t| t.splits.iter().any(|s| s.account.is_some())).map(|t| transaction(book, t)).collect();
    out.sort_by(|x, y| x["guid"].as_str().cmp(&y["guid"].as_str()));
    out
}

fn used_currencies(book: &Book) -> BTreeSet<CommodityRef> {
    let accounts = book.accounts.values().filter_map(|a| a.commodity.clone());
    let txns = book.transactions.values().map(|t| t.currency.clone());
    let prices = book.prices.values().flat_map(|p| [p.commodity.clone(), p.currency.clone()]);
    accounts.chain(txns).chain(prices).filter(CommodityRef::is_currency).collect()
}

fn commodities(book: &Book) -> Vec<Value> {
    let usage = usage_table(book);
    let currencies = used_currencies(book).into_iter().map(|id| {
        let stored = lookup(book, &id);
        let accounts = usage.get(&id).map_or(0, |u| u.accounts);
        // Loading a book turns quotes on for a currency an account uses, unless auto_quote_control is "false"
        // (gnc_commodity_increment_usage_count, 5.17:libgnucash/engine/gnc-commodity.cpp:1402-1425).
        let quote = stored.as_deref().map(|c| effective_quote(c, accounts));
        let flag = quote.as_ref().is_some_and(|q| q.flag);
        json!({
            "id": id.to_string(),
            "fraction": commodity_fraction(book, &id),
            "quoteFlag": flag,
            "quoteSource": if flag { quote.and_then(|q| q.source).map_or(Value::Null, Value::String) } else { Value::Null },
            "quoteTz": text(stored.as_deref().and_then(|c| c.quote_tz.as_deref())),
            "userSymbol": text(stored.as_deref().and_then(|c| c.slots.get_str(slot_paths::USER_SYMBOL))),
        })
    });
    let securities = book.commodities.values().filter(|c| !c.id.is_currency() && c.id.namespace != "template").map(|c| {
        json!({
            "id": c.id.to_string(),
            "fraction": c.fraction,
            "quoteFlag": c.quote_flag,
            "quoteSource": if c.quote_flag { text(c.quote_source.as_deref()) } else { Value::Null },
            "quoteTz": text(c.quote_tz.as_deref()),
            "fullname": text(c.fullname.as_deref()),
            "cusip": text(c.cusip.as_deref()),
            "userSymbol": text(c.slots.get_str(slot_paths::USER_SYMBOL)),
        })
    });
    let mut out: Vec<Value> = currencies.chain(securities).collect();
    out.sort_by(|x, y| x["id"].as_str().cmp(&y["id"].as_str()));
    out
}

fn prices(book: &Book) -> Vec<Value> {
    let mut out: Vec<Value> = book
        .prices
        .values()
        .map(|p| {
            json!({
                "commodity": p.commodity.to_string(),
                "currency": p.currency.to_string(),
                "time": p.time.0,
                "source": p.source.clone().unwrap_or_default(),
                "type": text(p.price_type.as_deref()),
                "value": frac(p.value),
            })
        })
        .collect();
    let key = |v: &Value| {
        (
            v["commodity"].as_str().unwrap_or_default().to_owned(),
            v["currency"].as_str().unwrap_or_default().to_owned(),
            v["time"].as_i64(),
            v["source"].as_str().unwrap_or_default().to_owned(),
            v["type"].as_str().unwrap_or_default().to_owned(),
            v["value"].as_str().unwrap_or_default().to_owned(),
        )
    };
    out.sort_by_key(key);
    out
}

fn lots(book: &Book) -> Vec<Value> {
    let mut out: Vec<Value> = book
        .lots
        .values()
        .map(|l| {
            let mut splits: Vec<String> = book.splits_in_lot(l.guid).map(|(_, s)| s.guid.to_string()).collect();
            splits.sort();
            let balance = sum(book.splits_in_lot(l.guid).map(|(_, s)| s.amount));
            json!({
                "account": l.account.to_string(),
                "title": text(l.title()),
                "notes": text(l.notes()),
                "splits": splits,
                "balance": frac(balance),
                // gnc_lot_is_closed: a lot whose balance is zero is closed (5.17:libgnucash/engine/gnc-lot.cpp).
                "closed": balance.is_zero(),
            })
        })
        .collect();
    out.sort_by(|x, y| {
        let k = |v: &Value| (v["account"].as_str().unwrap_or_default().to_owned(), v["splits"].to_string(), v["title"].as_str().unwrap_or_default().to_owned());
        k(x).cmp(&k(y))
    });
    out
}

fn address(a: &Address) -> Value {
    let v = json!({
        "name": text(Some(&a.name)), "addr1": text(Some(&a.addr1)), "addr2": text(Some(&a.addr2)), "addr3": text(Some(&a.addr3)),
        "addr4": text(Some(&a.addr4)), "phone": text(Some(&a.phone)), "fax": text(Some(&a.fax)), "email": text(Some(&a.email)),
    });
    if v.as_object().is_some_and(|o| o.values().all(Value::is_null)) { Value::Null } else { v }
}

fn owner(o: OwnerRef) -> Value {
    json!({ "type": o.kind.owner_type_number(), "guid": o.guid.to_string() })
}

fn tax_included(t: TaxIncluded) -> i64 {
    match t {
        TaxIncluded::Yes => 1,
        TaxIncluded::No => 2,
        TaxIncluded::UseGlobal => 3,
    }
}

fn term_name(book: &Book, g: Option<Guid>) -> Value {
    g.and_then(|g| book.business.bill_terms.get(&g)).map_or(Value::Null, |t| Value::String(t.name.clone()))
}

fn table_name(book: &Book, g: Option<Guid>) -> Value {
    g.and_then(|g| book.business.tax_tables.get(&g)).map_or(Value::Null, |t| Value::String(t.name.clone()))
}

fn currency(c: Option<&CommodityRef>) -> Value {
    c.map_or(Value::Null, |c| Value::String(c.to_string()))
}

/// The owner kind an invoice is for, following a job to its owner.
fn document_owner_kind(book: &Book, o: OwnerRef) -> Option<OwnerKind> {
    match o.kind {
        OwnerKind::Job => book.business.jobs.get(&o.guid).map(|j| j.owner.kind),
        k => Some(k),
    }
}

/// `gncInvoiceGetTypeString` (`5.17:libgnucash/engine/gncInvoice.c`), untranslated.
fn invoice_type(book: &Book, inv: &red_ale_core::business::Invoice) -> &'static str {
    match (document_owner_kind(book, inv.owner), inv.is_credit_note()) {
        (Some(OwnerKind::Customer), false) => "Invoice",
        (Some(OwnerKind::Vendor), false) => "Bill",
        (Some(OwnerKind::Employee), false) => "Expense",
        (Some(_), true) => "Credit Note",
        _ => "Undefined",
    }
}

fn invoices(book: &Book) -> Vec<Value> {
    let b = &book.business;
    let mut out: Vec<Value> = b
        .invoices
        .values()
        .map(|inv| {
            let posted = inv.is_posted();
            let post_txn = inv.post_txn.and_then(|g| book.transaction(g));
            let due = post_txn.map(|t| t.slots.get("trans-date-due").and_then(SlotValue::as_time).unwrap_or(t.date_posted).0);
            let lot_balance = inv.post_lot.map(|l| sum(book.splits_in_lot(l).map(|(_, s)| s.amount)));
            let mut entries: Vec<String> =
                b.entries.values().filter(|e| e.invoice == Some(inv.guid) || e.bill == Some(inv.guid)).map(|e| e.guid.to_string()).collect();
            entries.sort();
            json!({
                "id": inv.id,
                "type": invoice_type(book, inv),
                "owner": owner(inv.owner),
                "notes": text(Some(&inv.notes)),
                "billingId": text(Some(&inv.billing_id)),
                "active": inv.active,
                "currency": currency(inv.currency.as_ref()),
                "terms": term_name(book, inv.terms),
                "dateOpened": inv.opened.0,
                "datePosted": if posted { inv.posted.map(|t| t.0) } else { None },
                "dateDue": if posted { due } else { None },
                "postedAccount": guid(inv.post_acc),
                "postedTxn": guid(inv.post_txn),
                "toChargeAmount": frac(inv.charge_amount),
                // Filled in by the business work package (invoice totals); ignored by comparisons until then.
                "total": Value::Null,
                // gncInvoiceIsPaid: posted and its lot is closed.
                "paid": if posted { Some(lot_balance.is_some_and(Numeric::is_zero)) } else { None },
                "entries": entries,
            })
        })
        .collect();
    out.sort_by(|x, y| (x["type"].as_str(), x["id"].as_str()).cmp(&(y["type"].as_str(), y["id"].as_str())));
    out
}

fn business(book: &Book) -> Value {
    let b = &book.business;
    let customers: Vec<Value> = b
        .customers
        .values()
        .map(|x| {
            json!({
                "guid": x.guid.to_string(), "id": x.id, "name": x.name, "notes": text(Some(&x.notes)), "active": x.active,
                "currency": currency(x.currency.as_ref()), "terms": term_name(book, x.terms), "taxTable": table_name(book, x.tax_table),
                "taxTableOverride": x.use_tax_table, "taxIncluded": tax_included(x.tax_included), "discount": frac(x.discount),
                "credit": frac(x.credit), "addr": address(&x.addr), "shipAddr": address(&x.ship_addr),
            })
        })
        .collect();
    let vendors: Vec<Value> = b
        .vendors
        .values()
        .map(|x| {
            json!({
                "guid": x.guid.to_string(), "id": x.id, "name": x.name, "notes": text(Some(&x.notes)), "active": x.active,
                "currency": currency(x.currency.as_ref()), "terms": term_name(book, x.terms), "taxTable": table_name(book, x.tax_table),
                "taxTableOverride": x.use_tax_table, "taxIncluded": tax_included(x.tax_included), "addr": address(&x.addr),
            })
        })
        .collect();
    let employees: Vec<Value> = b
        .employees
        .values()
        .map(|x| {
            json!({
                "guid": x.guid.to_string(), "id": x.id, "username": text(Some(&x.username)), "active": x.active,
                "currency": currency(x.currency.as_ref()), "language": text(Some(&x.language)), "acl": text(Some(&x.acl)),
                "workday": frac(x.workday), "rate": frac(x.rate), "ccard": guid(x.ccard), "addr": address(&x.addr),
            })
        })
        .collect();
    let jobs: Vec<Value> = b
        .jobs
        .values()
        .map(|x| {
            json!({
                "guid": x.guid.to_string(), "id": x.id, "name": x.name, "reference": text(Some(&x.reference)), "active": x.active,
                // gncJobGetRate reads the job-rate slot, zero when unset.
                "rate": frac(x.slots.get("job-rate").and_then(SlotValue::as_numeric).unwrap_or(Numeric::ZERO)),
                "owner": owner(x.owner),
            })
        })
        .collect();
    let entries: Vec<Value> = b
        .entries
        .values()
        .map(|x| {
            json!({
                "guid": x.guid.to_string(), "date": x.date.0, "description": text(Some(&x.description)), "action": text(Some(&x.action)),
                "notes": text(Some(&x.notes)), "quantity": frac(x.quantity), "invAccount": guid(x.i_account), "invPrice": frac(x.i_price),
                "billAccount": guid(x.b_account), "billPrice": frac(x.b_price), "invTaxable": x.i_taxable, "billTaxable": x.b_taxable,
                "invTaxTable": table_name(book, x.i_tax_table), "billTaxTable": table_name(book, x.b_tax_table), "billable": x.billable,
            })
        })
        .collect();
    let mut tax_tables: Vec<Value> =
        b.tax_tables.values().filter(|t| t.parent.is_none()).map(|t| json!({ "name": t.name, "entryCount": t.entries.len() })).collect();
    tax_tables.sort_by(|x, y| x["name"].as_str().cmp(&y["name"].as_str()));
    let referenced: BTreeSet<Guid> =
        b.customers.values().filter_map(|c| c.terms).chain(b.vendors.values().filter_map(|v| v.terms)).chain(b.invoices.values().filter_map(|i| i.terms)).collect();
    let mut bill_terms: Vec<Value> = referenced.iter().filter_map(|g| b.bill_terms.get(g)).map(bill_term).collect();
    bill_terms.sort_by(|x, y| x["name"].as_str().cmp(&y["name"].as_str()));
    bill_terms.dedup_by(|x, y| x["name"] == y["name"]);
    json!({
        "customers": customers, "vendors": vendors, "employees": employees, "jobs": jobs, "invoices": invoices(book),
        "entries": entries, "taxTables": tax_tables, "billTerms": bill_terms,
    })
}

fn bill_term(t: &std::sync::Arc<red_ale_core::business::BillTerm>) -> Value {
    use red_ale_core::business::BillTermKind;
    let (kind, due, disc_days, discount, cutoff) = match t.kind {
        Some(BillTermKind::Days { due_days, disc_days, discount }) => (1, due_days, disc_days, discount, 0),
        Some(BillTermKind::Proximo { due_day, disc_day, discount, cutoff_day }) => (2, due_day, disc_day, discount, cutoff_day),
        None => (0, 0, 0, Numeric::ZERO, 0),
    };
    json!({
        "name": t.name, "description": text(Some(&t.description)), "type": kind, "dueDays": due, "discountDays": disc_days,
        "discount": frac(discount), "cutoff": cutoff,
    })
}

/// red-ale's view of `book`, shaped like the oracle's dump.
pub fn dump(book: &Book) -> Value {
    json!({
        "gnucashVersion": crate::oracle::PINNED_VERSION,
        "book": {
            "tradingAccounts": red_ale_core::book_options::uses_trading_accounts(book),
            "splitActionForNum": red_ale_core::book_options::uses_split_action_for_num(book),
            "autoReadonlyDays": red_ale_core::book_options::auto_readonly_days(book),
        },
        "commodities": commodities(book),
        "accounts": accounts(book),
        "transactions": transactions(book),
        "prices": prices(book),
        "lots": lots(book),
        "business": business(book),
    })
}

/// Paths the comparison skips until red-ale computes them.
pub const NOT_YET_COMPUTED: [&str; 1] = ["business.invoices.*.total"];
