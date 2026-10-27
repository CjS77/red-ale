//! Tax tables and billing terms, and their reference counts.
//!
//! GnuCash keeps both as families: posting a document freezes a copy (a *child*, invisible, refcount 0) of each
//! table and term it uses, and the visible *parent* counts the customers, vendors, entries and invoices that refer to
//! it directly (`5.17:libgnucash/engine/gncTaxTable.c:42-60`, `gncBillTerm.c:51-60`). These operations edit only
//! visible tables and terms; frozen copies are left exactly as stored. Making children is posting's job.

use std::sync::Arc;

use crate::business::{AmountType, BillTerm, BillTermKind, TaxTable, TaxTableEntry};
use crate::{Book, DomainError, DomainResult, Env, Frame, Guid, Numeric};

// ---------------------------------------------------------------------------------------------------------------
// Reference counts.

/// `gncTaxTableIncRef`/`DecRef` for a setter that moves a reference from `old` to `new`: no-ops on children and
/// invisible tables, and a decrement never goes below zero (`5.17:libgnucash/engine/gncTaxTable.c:516-535`).
pub fn retarget_tax_table(book: &mut Book, old: Option<Guid>, new: Option<Guid>) {
    if old == new {
        return;
    }
    let tables = &mut book.business.tax_tables;
    let counted = |t: &TaxTable| t.parent.is_none() && !t.invisible;
    if let Some(t) = old.and_then(|g| tables.get_mut(&g)).filter(|t| counted(t) && t.refcount > 0) {
        Arc::make_mut(t).refcount -= 1;
    }
    if let Some(t) = new.and_then(|g| tables.get_mut(&g)).filter(|t| counted(t)) {
        Arc::make_mut(t).refcount += 1;
    }
}

/// `gncBillTermIncRef`/`DecRef` for a setter that moves a reference from `old` to `new`
/// (`5.17:libgnucash/engine/gncBillTerm.c:420-439`).
pub fn retarget_bill_term(book: &mut Book, old: Option<Guid>, new: Option<Guid>) {
    if old == new {
        return;
    }
    let terms = &mut book.business.bill_terms;
    let counted = |t: &BillTerm| t.parent.is_none() && !t.invisible;
    if let Some(t) = old.and_then(|g| terms.get_mut(&g)).filter(|t| counted(t) && t.refcount > 0) {
        Arc::make_mut(t).refcount -= 1;
    }
    if let Some(t) = new.and_then(|g| terms.get_mut(&g)).filter(|t| counted(t)) {
        Arc::make_mut(t).refcount += 1;
    }
}

/// How many customers, vendors and entries (each side separately) refer to a tax table: what GnuCash's load scrub
/// counts (`5.17:libgnucash/backend/xml/gnc-tax-table-xml-v2.cpp:530-633`).
pub fn tax_table_references(book: &Book, table: Guid) -> usize {
    let b = &book.business;
    let owners = b.customers.values().filter(|c| c.tax_table == Some(table)).count()
        + b.vendors.values().filter(|v| v.tax_table == Some(table)).count();
    let entries = b
        .entries
        .values()
        .map(|e| usize::from(e.i_tax_table == Some(table)) + usize::from(e.b_tax_table == Some(table)))
        .sum::<usize>();
    owners + entries
}

/// How many invoices, customers and vendors refer to a billing term: what GnuCash's load scrub counts
/// (`5.17:libgnucash/backend/xml/gnc-bill-term-xml-v2.cpp:589-686`).
pub fn bill_term_references(book: &Book, term: Guid) -> usize {
    let b = &book.business;
    b.invoices.values().filter(|i| i.terms == Some(term)).count()
        + b.customers.values().filter(|c| c.terms == Some(term)).count()
        + b.vendors.values().filter(|v| v.terms == Some(term)).count()
}

/// A new reference to a tax table must name a visible table; GnuCash's lists offer only those
/// (`5.17:libgnucash/engine/gncTaxTable.c:152-170`). A reference that does not change is not checked, so a frozen
/// copy stays where posting put it.
pub fn check_tax_table_ref(book: &Book, new: Option<Guid>, old: Option<Guid>) -> DomainResult<()> {
    match new {
        Some(g) if new != old => {
            let t = book
                .business
                .tax_tables
                .get(&g)
                .ok_or_else(|| DomainError::not_found("tax table", &g))?;
            if t.parent.is_some() || t.invisible {
                return Err(DomainError::invalid(format!(
                    "tax table {g} is a frozen copy kept for a posted document; use its parent"
                )));
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

/// A new reference to a billing term must name a visible term (`5.17:libgnucash/engine/gncBillTerm.c:494-512`).
pub fn check_bill_term_ref(book: &Book, new: Option<Guid>, old: Option<Guid>) -> DomainResult<()> {
    match new {
        Some(g) if new != old => {
            let t = book
                .business
                .bill_terms
                .get(&g)
                .ok_or_else(|| DomainError::not_found("billing term", &g))?;
            if t.parent.is_some() || t.invisible {
                return Err(DomainError::invalid(format!(
                    "billing term {g} is a frozen copy kept for a posted document; use its parent"
                )));
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Tax tables.

/// The visible tax table with this name (`gncTaxTableLookupByName` searches visible tables only,
/// `5.17:libgnucash/engine/gncTaxTable.c:667-678`).
pub fn tax_table_by_name<'a>(book: &'a Book, name: &str) -> Option<&'a TaxTable> {
    book.business
        .tax_tables
        .values()
        .map(AsRef::as_ref)
        .find(|t| t.parent.is_none() && !t.invisible && t.name == name)
}

fn visible_tax_table(book: &Book, guid: Guid) -> DomainResult<&TaxTable> {
    let t = book
        .business
        .tax_tables
        .get(&guid)
        .ok_or_else(|| DomainError::not_found("tax table", &guid))?;
    if t.parent.is_some() || t.invisible {
        return Err(DomainError::conflict(format!(
            "tax table {guid} is a frozen copy kept for posted documents and cannot be changed"
        )));
    }
    Ok(t)
}

/// The tax table dialog's checks: a name unique among visible tables, and each line with an account and, for a
/// percentage, an amount from -100 to 100 (`5.17:gnucash/gnome-utils/dialog-tax-table.c:153-198`, `:703`). A table
/// needs at least one line: GnuCash deletes empty tables when it loads a book
/// (`5.17:libgnucash/backend/xml/gnc-tax-table-xml-v2.cpp:517-524`).
fn check_tax_table(book: &Book, guid: Guid, name: &str, entries: &[TaxTableEntry]) -> DomainResult<()> {
    if name.trim().is_empty() {
        return Err(DomainError::invalid("the tax table needs a name"));
    }
    if tax_table_by_name(book, name).is_some_and(|t| t.guid != guid) {
        return Err(DomainError::conflict(format!("a tax table named {name:?} already exists")));
    }
    if entries.is_empty() {
        return Err(DomainError::invalid("a tax table needs at least one line"));
    }
    entries.iter().try_for_each(|e| check_tax_line(book, e))
}

fn check_tax_line(book: &Book, e: &TaxTableEntry) -> DomainResult<()> {
    let acc = e
        .account
        .ok_or_else(|| DomainError::invalid("each tax table line needs an account"))?;
    if book.account(acc).is_none() || acc == book.root {
        return Err(DomainError::not_found("account", &acc));
    }
    if e.kind == AmountType::Percent && e.amount.abs().cmp_value(Numeric::from_int(100)).is_gt() {
        return Err(DomainError::invalid("a percentage must be from -100 to 100"));
    }
    Ok(())
}

/// Order lines as `gncTaxTableAddEntry` keeps them: by account full name (`g_strcmp0`), then amount
/// (`5.17:libgnucash/engine/gncTaxTable.c:594-608`, `:821-840`). This order is written to the file.
pub fn sort_tax_lines(book: &Book, entries: &mut [TaxTableEntry]) {
    let name = |e: &TaxTableEntry| e.account.and_then(|a| book.full_name(a));
    entries.sort_by(|a, b| name(a).cmp(&name(b)).then_with(|| a.amount.cmp_value(b.amount)));
}

/// Create a visible tax table. Returns its GUID.
pub fn add_tax_table(book: &mut Book, env: &mut dyn Env, name: &str, mut entries: Vec<TaxTableEntry>) -> DomainResult<Guid> {
    let guid = env.new_guid();
    check_tax_table(book, guid, name, &entries)?;
    sort_tax_lines(book, &mut entries);
    let table = TaxTable {
        guid,
        name: name.to_owned(),
        refcount: 0,
        invisible: false,
        child: None,
        parent: None,
        entries,
        slots: Frame::new(),
    };
    book.business.tax_tables.insert(guid, Arc::new(table));
    Ok(guid)
}

/// Rename a visible tax table and/or replace its lines. New lines drop the cached frozen copy, as the dialog does
/// with `gncTaxTableChanged`, so the next posting freezes the new lines; renaming alone keeps it
/// (`5.17:gnucash/gnome-utils/dialog-tax-table.c:240`, `:690-709`).
pub fn update_tax_table(book: &mut Book, guid: Guid, name: Option<&str>, entries: Option<Vec<TaxTableEntry>>) -> DomainResult<()> {
    let mut t = visible_tax_table(book, guid)?.clone();
    if let Some(n) = name {
        n.clone_into(&mut t.name);
    }
    if let Some(mut lines) = entries {
        sort_tax_lines(book, &mut lines);
        if lines != t.entries {
            t.entries = lines;
            t.child = None;
        }
    }
    check_tax_table(book, guid, &t.name, &t.entries)?;
    crate::ops::business::contacts::note_extra_data(book, !t.slots.is_empty());
    book.business.tax_tables.insert(guid, Arc::new(t));
    Ok(())
}

/// Delete a visible tax table nothing refers to (the dialog refuses one whose refcount is above zero,
/// `5.17:gnucash/gnome-utils/dialog-tax-table.c:719-744`). Its frozen copies stay, without a parent, as
/// `gncTaxTableFree` leaves them (`5.17:libgnucash/engine/gncTaxTable.c:452-457`).
pub fn delete_tax_table(book: &mut Book, guid: Guid) -> DomainResult<()> {
    let t = visible_tax_table(book, guid)?;
    let refs = tax_table_references(book, guid);
    if t.refcount > 0 || refs > 0 {
        return Err(DomainError::conflict(format!(
            "tax table {:?} is in use and cannot be deleted",
            t.name
        )));
    }
    book.business.tax_tables.remove(&guid);
    book.business
        .tax_tables
        .values_mut()
        .filter(|c| c.parent == Some(guid))
        .for_each(|c| Arc::make_mut(c).parent = None);
    Ok(())
}

// ---------------------------------------------------------------------------------------------------------------
// Billing terms.

/// The visible billing term with this name (`gncBillTermLookupByName`, `5.17:libgnucash/engine/gncBillTerm.c:494-505`).
pub fn bill_term_by_name<'a>(book: &'a Book, name: &str) -> Option<&'a BillTerm> {
    book.business
        .bill_terms
        .values()
        .map(AsRef::as_ref)
        .find(|t| t.parent.is_none() && !t.invisible && t.name == name)
}

fn visible_bill_term(book: &Book, guid: Guid) -> DomainResult<&BillTerm> {
    let t = book
        .business
        .bill_terms
        .get(&guid)
        .ok_or_else(|| DomainError::not_found("billing term", &guid))?;
    if t.parent.is_some() || t.invisible {
        return Err(DomainError::conflict(format!(
            "billing term {guid} is a frozen copy kept for posted documents and cannot be changed"
        )));
    }
    Ok(t)
}

/// The billing term dialog's checks: a name unique among visible terms and discount days no more than due days
/// (`5.17:gnucash/gnome/dialog-billterms.c:268-334`). Beyond the dialog, red-ale refuses negative day counts, a
/// proximo due day outside 1-31 (GnuCash would compute an undefined date from it,
/// `design/notes/engine-business.md` §3.2) and a discount outside 0-100 %.
fn check_bill_term(book: &Book, guid: Guid, name: &str, kind: &BillTermKind) -> DomainResult<()> {
    if name.trim().is_empty() {
        return Err(DomainError::invalid("the billing term needs a name"));
    }
    if bill_term_by_name(book, name).is_some_and(|t| t.guid != guid) {
        return Err(DomainError::conflict(format!("a billing term named {name:?} already exists")));
    }
    let (due, disc, discount) = match *kind {
        BillTermKind::Days {
            due_days,
            disc_days,
            discount,
        } => (due_days, disc_days, discount),
        BillTermKind::Proximo {
            due_day,
            disc_day,
            discount,
            cutoff_day,
        } => {
            if !(1..=31).contains(&due_day) || !(0..=31).contains(&disc_day) {
                return Err(DomainError::invalid(
                    "proximo days must be days of the month (due 1-31, discount 0-31)",
                ));
            }
            if !(-31..=31).contains(&cutoff_day) {
                return Err(DomainError::invalid("the cutoff day must be from -31 to 31"));
            }
            (due_day, disc_day, discount)
        }
    };
    if due < 0 || disc < 0 {
        return Err(DomainError::invalid("days cannot be negative"));
    }
    if disc > due {
        return Err(DomainError::invalid("discount days cannot be more than due days"));
    }
    if discount.is_negative() || discount.cmp_value(Numeric::from_int(100)).is_gt() {
        return Err(DomainError::invalid("the discount must be a percentage from 0 to 100"));
    }
    Ok(())
}

/// Create a visible billing term. Returns its GUID.
pub fn add_bill_term(book: &mut Book, env: &mut dyn Env, name: &str, description: &str, kind: BillTermKind) -> DomainResult<Guid> {
    let guid = env.new_guid();
    check_bill_term(book, guid, name, &kind)?;
    let term = BillTerm {
        guid,
        name: name.to_owned(),
        description: description.to_owned(),
        refcount: 0,
        invisible: false,
        slots: Frame::new(),
        child: None,
        parent: None,
        kind: Some(kind),
    };
    book.business.bill_terms.insert(guid, Arc::new(term));
    Ok(guid)
}

/// Change a visible billing term. Any change drops the cached frozen copy, as the dialog does with
/// `gncBillTermChanged` (`5.17:gnucash/gnome/dialog-billterms.c:359-360`).
pub fn update_bill_term(
    book: &mut Book,
    guid: Guid,
    name: Option<&str>,
    description: Option<&str>,
    kind: Option<BillTermKind>,
) -> DomainResult<()> {
    let old = visible_bill_term(book, guid)?;
    let mut t = old.clone();
    if let Some(n) = name {
        n.clone_into(&mut t.name);
    }
    if let Some(d) = description {
        d.clone_into(&mut t.description);
    }
    if let Some(k) = kind {
        t.kind = Some(k);
    }
    let k = t.kind.ok_or_else(|| DomainError::invalid("the billing term needs a type"))?;
    check_bill_term(book, guid, &t.name, &k)?;
    if t != *old {
        t.child = None;
    }
    book.business.bill_terms.insert(guid, Arc::new(t));
    Ok(())
}

/// Delete a visible billing term nothing refers to (`5.17:gnucash/gnome/dialog-billterms.c:663-690`). Its frozen
/// copies stay, without a parent (`5.17:libgnucash/engine/gncBillTerm.c:288-297`).
pub fn delete_bill_term(book: &mut Book, guid: Guid) -> DomainResult<()> {
    let t = visible_bill_term(book, guid)?;
    if t.refcount > 0 || bill_term_references(book, guid) > 0 {
        return Err(DomainError::conflict(format!(
            "billing term {:?} is in use and cannot be deleted",
            t.name
        )));
    }
    book.business.bill_terms.remove(&guid);
    book.business
        .bill_terms
        .values_mut()
        .filter(|c| c.parent == Some(guid))
        .for_each(|c| Arc::make_mut(c).parent = None);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Account, AccountType, CommodityRef, FixedEnv, Time64};

    fn setup() -> (Book, FixedEnv, Guid, Guid) {
        let mut env = FixedEnv::new(Time64(0), 100);
        let mut book = Book::new_empty(&mut env);
        let eur = Some(CommodityRef::currency("EUR"));
        let tax = Account::new(env.new_guid(), "Tax", AccountType::Liability, eur.clone(), Some(book.root));
        let levy = Account::new(env.new_guid(), "Levy", AccountType::Liability, eur, Some(book.root));
        let (t, l) = (tax.guid, levy.guid);
        book.accounts.insert(t, Arc::new(tax));
        book.accounts.insert(l, Arc::new(levy));
        (book, env, t, l)
    }

    fn line(account: Guid, amount: i64, kind: AmountType) -> TaxTableEntry {
        TaxTableEntry {
            account: Some(account),
            amount: Numeric::from_int(amount),
            kind,
        }
    }

    #[test]
    fn tax_lines_are_sorted_by_account_name_and_checked() {
        let (mut book, mut env, tax, levy) = setup();
        let lines = vec![line(tax, 10, AmountType::Percent), line(levy, 2, AmountType::Value)];
        let g = add_tax_table(&mut book, &mut env, "GST", lines).unwrap();
        let t = &book.business.tax_tables[&g];
        assert_eq!(t.entries.iter().map(|e| e.account).collect::<Vec<_>>(), [Some(levy), Some(tax)]);
        assert!(
            add_tax_table(&mut book, &mut env, "GST", vec![line(tax, 1, AmountType::Percent)]).is_err(),
            "duplicate name"
        );
        assert!(add_tax_table(&mut book, &mut env, "Big", vec![line(tax, 101, AmountType::Percent)]).is_err());
        assert!(add_tax_table(&mut book, &mut env, "Empty", vec![]).is_err());
        add_tax_table(&mut book, &mut env, "Neg", vec![line(tax, -100, AmountType::Percent)]).unwrap();
    }

    #[test]
    fn refcounts_skip_frozen_copies_and_block_deletion() {
        let (mut book, mut env, tax, _) = setup();
        let g = add_tax_table(&mut book, &mut env, "GST", vec![line(tax, 10, AmountType::Percent)]).unwrap();
        retarget_tax_table(&mut book, None, Some(g));
        assert_eq!(book.business.tax_tables[&g].refcount, 1);
        assert!(delete_tax_table(&mut book, g).is_err());
        retarget_tax_table(&mut book, Some(g), None);
        retarget_tax_table(&mut book, Some(g), None);
        assert_eq!(book.business.tax_tables[&g].refcount, 0, "never below zero");

        let mut child = (*book.business.tax_tables[&g]).clone();
        child.guid = env.new_guid();
        child.parent = Some(g);
        child.invisible = true;
        let c = child.guid;
        book.business.tax_tables.insert(c, Arc::new(child));
        Arc::make_mut(book.business.tax_tables.get_mut(&g).unwrap()).child = Some(c);
        retarget_tax_table(&mut book, None, Some(c));
        assert_eq!(book.business.tax_tables[&c].refcount, 0);
        assert!(
            update_tax_table(&mut book, c, Some("X"), None).is_err(),
            "frozen copies are not edited"
        );
        assert!(check_tax_table_ref(&book, Some(c), None).is_err());
        assert!(check_tax_table_ref(&book, Some(c), Some(c)).is_ok());

        update_tax_table(&mut book, g, Some("GST2"), None).unwrap();
        assert_eq!(book.business.tax_tables[&g].child, Some(c), "renaming keeps the frozen copy");
        update_tax_table(&mut book, g, None, Some(vec![line(tax, 5, AmountType::Percent)])).unwrap();
        assert_eq!(book.business.tax_tables[&g].child, None, "new lines drop it");
        delete_tax_table(&mut book, g).unwrap();
        assert_eq!(book.business.tax_tables[&c].parent, None);
        assert!(book.business.tax_tables[&c].invisible);
    }

    #[test]
    fn bill_terms_are_checked() {
        let (mut book, mut env, _, _) = setup();
        let days = BillTermKind::Days {
            due_days: 30,
            disc_days: 10,
            discount: Numeric::from_int(2),
        };
        let g = add_bill_term(&mut book, &mut env, "Net30", "", days).unwrap();
        assert!(add_bill_term(&mut book, &mut env, "Net30", "", days).is_err());
        let bad = BillTermKind::Days {
            due_days: 5,
            disc_days: 10,
            discount: Numeric::ZERO,
        };
        assert!(add_bill_term(&mut book, &mut env, "Bad", "", bad).is_err());
        let prox = BillTermKind::Proximo {
            due_day: 0,
            disc_day: 0,
            discount: Numeric::ZERO,
            cutoff_day: 0,
        };
        assert!(add_bill_term(&mut book, &mut env, "Prox", "", prox).is_err());
        retarget_bill_term(&mut book, None, Some(g));
        assert!(delete_bill_term(&mut book, g).is_err());
        retarget_bill_term(&mut book, Some(g), None);
        update_bill_term(&mut book, g, None, Some("Net 30 days"), None).unwrap();
        assert_eq!(book.business.bill_terms[&g].description, "Net 30 days");
        delete_bill_term(&mut book, g).unwrap();
    }
}
