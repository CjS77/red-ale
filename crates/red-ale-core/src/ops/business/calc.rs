//! Business arithmetic: billing-term due dates, entry values and document totals, computed as GnuCash 5.17 does.
//!
//! Every operation mirrors the `gnc_numeric` call GnuCash makes, with the same denominator and rounding flags, so
//! results match it exactly (`design/notes/engine-business.md` §3.2, §4.2, §5.1). Calendar dates are UTC dates: red-ale
//! treats UTC as the local time zone, as the oracle does (`TZ=UTC`).

use crate::business::{AmountType, BillTerm, BillTermKind, DiscountHow, Entry, Invoice, OwnerKind, OwnerRef, PaymentType, TaxTable, TaxTableEntry};
use crate::ops::business::{contacts, documents};
use crate::ops::commit::commodity_fraction;
use crate::time::days_in_month;
use crate::{Book, DENOM_AUTO, Date, DenomMode, DomainError, DomainResult, Guid, How, Numeric, Round, Time64, TimeZone};

/// `GNC_HOW_DENOM_LCD` with no rounding flag, which GnuCash treats as truncation (`5.17:libgnucash/engine/gnc-numeric.cpp:716`).
const LCD: How = How::new(DenomMode::Lcd, Round::Truncate);
/// `GNC_HOW_DENOM_REDUCE | GNC_HOW_RND_ROUND`.
const REDUCE_ROUND: How = How::new(DenomMode::Reduce, Round::Bankers);
/// `GNC_HOW_DENOM_REDUCE | GNC_HOW_RND_ROUND_HALF_UP`.
const REDUCE_HALF_UP: How = How::new(DenomMode::Reduce, Round::HalfUp);
/// `GNC_HOW_DENOM_EXACT | GNC_HOW_RND_ROUND_HALF_UP`.
const EXACT_HALF_UP: How = How::new(DenomMode::Exact, Round::HalfUp);
/// The denominator an entry's values are rounded to when it is on no document (`5.17:libgnucash/engine/gncEntry.c:1341-1358`).
const NO_DOCUMENT_DENOM: i64 = 100_000;

/// A list of (account, value) pairs in GnuCash's `GncAccountValue` list order.
pub type AccountValues = Vec<(Guid, Numeric)>;

/// `gncAccountValueAdd`: add into the account's existing value, or put a new account at the **front**
/// (`5.17:libgnucash/engine/gncTaxTable.c:942-967`).
pub fn account_value_add(list: &mut AccountValues, account: Guid, value: Numeric) -> DomainResult<()> {
    match list.iter_mut().find(|(a, _)| *a == account) {
        Some((_, v)) => *v = v.add(value, DENOM_AUTO, REDUCE_HALF_UP)?,
        None => list.insert(0, (account, value)),
    }
    Ok(())
}

/// `gncAccountValueAddList` (`5.17:libgnucash/engine/gncTaxTable.c:970-981`).
pub fn account_values_add_list(list: &mut AccountValues, other: &AccountValues) -> DomainResult<()> {
    other.iter().try_for_each(|(a, v)| account_value_add(list, *a, *v))
}

// ---------------------------------------------------------------------------------------------------------------
// Due dates

/// `gncBillTermComputeDueDate`: when a document posted at `post` falls due under `term`
/// (`5.17:libgnucash/engine/gncBillTerm.c:751-825`), from the local date of the posting time. No term, or a term
/// whose type was never set, gives the day-neutral time of the posting date. A proximo term with a due day of zero or
/// less is refused: GnuCash passes it to `gnc_dmy2time64_neutral` unchecked and the result is undefined.
pub fn due_date(term: Option<&BillTerm>, post: Time64, zone: &TimeZone) -> DomainResult<Time64> {
    let date = zone.local_date(post).ok_or_else(|| DomainError::invalid(format!("posting time {} is out of range", post.0)))?;
    let Some(term) = term else { return Ok(post) };
    match term.kind {
        None => Ok(zone.neutral_time(date)),
        Some(BillTermKind::Days { due_days, .. }) => Ok(zone.neutral_time(date.add_days(due_days))),
        Some(BillTermKind::Proximo { due_day, cutoff_day, .. }) => proximo_due(date, due_day, cutoff_day, &term.name, zone),
    }
}

fn proximo_due(date: Date, due_day: i64, cutoff_day: i64, name: &str, zone: &TimeZone) -> DomainResult<Time64> {
    if due_day <= 0 {
        return Err(DomainError::invalid(format!("billing term {name:?} has a proximo due day of {due_day}")));
    }
    let (year, month) = proximo_month(date, cutoff_day);
    let last = i64::from(days_in_month(year, month));
    let day = u32::try_from(due_day.min(last)).map_err(|_| DomainError::invalid("due day out of range"))?;
    Date::from_ymd(year, month, day).map(|d| zone.neutral_time(d)).ok_or_else(|| DomainError::invalid("due date out of range"))
}

/// `compute_monthyear`: the month a proximo term falls due in (`5.17:libgnucash/engine/gncBillTerm.c:751-781`).
fn proximo_month(date: Date, cutoff_day: i64) -> (i32, u32) {
    let cutoff = if cutoff_day <= 0 { cutoff_day + i64::from(days_in_month(date.year(), date.month())) } else { cutoff_day };
    let month = date.month() + if i64::from(date.day()) <= cutoff { 1 } else { 2 };
    if month > 12 { (date.year() + 1, month - 12) } else { (date.year(), month) }
}

// ---------------------------------------------------------------------------------------------------------------
// Documents

/// What kind of document an invoice is, from its end owner and credit-note flag (`gncInvoiceGetType`,
/// `5.17:libgnucash/engine/gncInvoice.c:1065-1108`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvoiceType {
    /// The end owner: a job's owner, otherwise the document's owner.
    pub end_owner: OwnerRef,
    /// A credit note.
    pub credit_note: bool,
}

impl InvoiceType {
    /// The document belongs to a customer, so entries use their invoice (`i_`) side.
    pub fn is_customer(self) -> bool {
        self.end_owner.kind == OwnerKind::Customer
    }

    /// `gncInvoiceGetTypeString`, untranslated: the word GnuCash writes into lot titles and split actions.
    pub fn type_string(self) -> &'static str {
        match (self.end_owner.kind, self.credit_note) {
            (_, true) => "Credit Note",
            (OwnerKind::Customer | OwnerKind::Job, false) => "Invoice",
            (OwnerKind::Vendor, false) => "Bill",
            (OwnerKind::Employee, false) => "Expense",
        }
    }

    /// Document values are turned into balance values by negating them when this is true: a customer invoice or a
    /// vendor or employee credit note (`5.17:libgnucash/engine/gncInvoice.c:1528-1542`).
    pub fn negate_for_balance(self) -> bool {
        self.is_customer() != self.credit_note
    }
}

/// The end owner (`contacts::end_owner`), refusing a missing job and a job owned by another job, which GnuCash
/// cannot post or pay (`5.17:libgnucash/engine/gncOwner.c:572`).
pub fn require_end_owner(book: &Book, owner: OwnerRef) -> DomainResult<OwnerRef> {
    let end = contacts::end_owner(book, owner).ok_or_else(|| DomainError::not_found("job", &owner.guid))?;
    if end.kind == OwnerKind::Job {
        return Err(DomainError::invalid(format!("job {} is owned by another job", owner.guid)));
    }
    Ok(end)
}

/// The kind of document `invoice` is.
pub fn invoice_type(book: &Book, invoice: &Invoice) -> DomainResult<InvoiceType> {
    let end_owner = require_end_owner(book, invoice.owner)?;
    if !contacts::owner_exists(book, end_owner) {
        return Err(DomainError::not_found(end_owner.kind.type_name(), &end_owner.guid));
    }
    Ok(InvoiceType { end_owner, credit_note: invoice.is_credit_note() })
}

/// A tax table's lines in GnuCash's order: by account full name, then amount (`gncTaxTableEntryCompare`,
/// `5.17:libgnucash/engine/gncTaxTable.c:594-606`, `:821-841`).
pub fn sorted_tax_lines<'a>(book: &Book, table: &'a TaxTable) -> Vec<&'a TaxTableEntry> {
    let mut lines: Vec<(Option<String>, &TaxTableEntry)> = table.entries.iter().map(|e| (e.account.and_then(|a| book.full_name(a)), e)).collect();
    lines.sort_by(|(na, a), (nb, b)| na.cmp(nb).then_with(|| a.amount.cmp_value(b.amount)));
    lines.into_iter().map(|(_, e)| e).collect()
}

// ---------------------------------------------------------------------------------------------------------------
// Entry values

/// The pricing inputs of one side of an entry.
#[derive(Debug, Clone, Copy)]
pub struct Pricing<'a> {
    /// Quantity.
    pub quantity: Numeric,
    /// Unit price.
    pub price: Numeric,
    /// The tax table's lines, sorted, when the side is taxable and has a table.
    pub tax_lines: Option<&'a [&'a TaxTableEntry]>,
    /// Prices include tax.
    pub tax_included: bool,
    /// Discount (percentage or amount).
    pub discount: Numeric,
    /// Discount type.
    pub discount_type: AmountType,
    /// When the discount applies.
    pub discount_how: DiscountHow,
}

/// What `gncEntryComputeValueInt` returns: exact, unrounded.
#[derive(Debug, Clone, PartialEq)]
pub struct EntryValue {
    /// What the seller earns: `value + Σ taxes` is what the buyer pays.
    pub value: Numeric,
    /// The discount.
    pub discount: Numeric,
    /// Tax per account, in GnuCash's list order.
    pub taxes: AccountValues,
    /// The pre-tax unit price.
    pub net_price: Numeric,
}

fn percent(n: Numeric) -> DomainResult<Numeric> {
    Ok(n.div(Numeric::from_int(100), DENOM_AUTO, How::EXACT)?)
}

/// Σ VALUE amounts and Σ PERCENT amounts / 100 of a tax table.
fn tax_rates(lines: &[&TaxTableEntry]) -> DomainResult<(Numeric, Numeric)> {
    let (tvalue, tpercent) = lines.iter().try_fold((Numeric::ZERO, Numeric::ZERO), |(tv, tp), l| -> DomainResult<_> {
        Ok(match l.kind {
            AmountType::Value => (tv.add(l.amount, DENOM_AUTO, LCD)?, tp),
            AmountType::Percent => (tv, tp.add(l.amount, DENOM_AUTO, LCD)?),
        })
    })?;
    Ok((tvalue, percent(tpercent)?))
}

/// `gncEntryComputeValueInt` (`5.17:libgnucash/engine/gncEntry.c:1131-1318`). Exact rational arithmetic with the
/// same operations GnuCash uses; overflow is an error rather than GnuCash's silent approximation.
pub fn compute_value(p: &Pricing<'_>) -> DomainResult<EntryValue> {
    let lines: &[&TaxTableEntry] = p.tax_lines.unwrap_or(&[]);
    let aggregate = p.quantity.mul(p.price, DENOM_AUTO, REDUCE_ROUND)?;
    let (tvalue, tpercent) = tax_rates(lines)?;
    let (mut pretax, net_price) = if p.tax_lines.is_some() && p.tax_included {
        let base = aggregate.sub(tvalue, DENOM_AUTO, LCD)?;
        let pretax = base.div(tpercent.add(Numeric::ONE, DENOM_AUTO, LCD)?, DENOM_AUTO, REDUCE_ROUND)?;
        let net = if p.quantity.is_zero() { p.price } else { pretax.div(p.quantity, DENOM_AUTO, REDUCE_ROUND)? };
        (pretax, net)
    } else {
        (aggregate, p.price)
    };
    let (value, discount) = match p.discount_how {
        DiscountHow::PreTax | DiscountHow::SameTime => {
            let discount = match p.discount_type {
                AmountType::Percent => pretax.mul(percent(p.discount)?, DENOM_AUTO, REDUCE_ROUND)?,
                AmountType::Value => p.discount,
            };
            let value = pretax.sub(discount, DENOM_AUTO, LCD)?;
            if p.discount_how == DiscountHow::PreTax {
                pretax = value;
            }
            (value, discount)
        }
        DiscountHow::PostTax => {
            let discount = match p.discount_type {
                AmountType::Percent => {
                    let tax = pretax.mul(tpercent, DENOM_AUTO, REDUCE_ROUND)?;
                    let after_tax = pretax.add(tax, DENOM_AUTO, LCD)?.add(tvalue, DENOM_AUTO, LCD)?;
                    after_tax.mul(percent(p.discount)?, DENOM_AUTO, REDUCE_ROUND)?
                }
                AmountType::Value => p.discount,
            };
            (pretax.sub(discount, DENOM_AUTO, LCD)?, discount)
        }
    };
    Ok(EntryValue { value, discount, taxes: tax_values(lines, pretax)?, net_price })
}

/// The tax list: one value per tax account, new accounts prepended.
fn tax_values(lines: &[&TaxTableEntry], pretax: Numeric) -> DomainResult<AccountValues> {
    let mut taxes = AccountValues::new();
    lines.iter().try_for_each(|l| {
        let account = l.account.ok_or_else(|| DomainError::invalid("a tax table line has no account"))?;
        let tax = match l.kind {
            AmountType::Value => l.amount,
            AmountType::Percent => pretax.mul(percent(l.amount)?, DENOM_AUTO, REDUCE_ROUND)?,
        };
        account_value_add(&mut taxes, account, tax)
    })?;
    Ok(taxes)
}

/// One side of an entry, as `gncEntryRecomputeValues` keeps it (`5.17:libgnucash/engine/gncEntry.c:1362-1445`).
/// Values have the internal sign: negative for credit notes, whose quantities are stored negated.
#[derive(Debug, Clone, PartialEq)]
pub struct EntryAmounts {
    /// Value, exact.
    pub value: Numeric,
    /// Value rounded to the document currency's fraction.
    pub value_rounded: Numeric,
    /// Discount, exact (always zero on the bill side).
    pub discount: Numeric,
    /// Discount, rounded.
    pub discount_rounded: Numeric,
    /// Tax per account, exact, in GnuCash's list order.
    pub taxes: AccountValues,
    /// Total tax, exact.
    pub tax: Numeric,
    /// Total tax, rounded running sum by sum (not term by term).
    pub tax_rounded: Numeric,
    /// Pre-tax unit price, exact.
    pub net_price: Numeric,
}

/// `get_entry_commodity_denom`: the fraction of the entry's invoice (else bill) currency, else 100000
/// (`5.17:libgnucash/engine/gncEntry.c:1338-1358`).
pub fn entry_denom(book: &Book, entry: &Entry) -> i64 {
    let fraction = |doc: Option<Guid>| {
        doc.and_then(|g| book.business.invoices.get(&g)).and_then(|i| i.currency.as_ref()).and_then(|c| commodity_fraction(book, c))
    };
    fraction(entry.invoice).or_else(|| fraction(entry.bill)).unwrap_or(NO_DOCUMENT_DENOM)
}

/// The pricing of one side of an entry: the invoice side for customer documents, the bill side otherwise. The bill
/// side never has a discount (`5.17:libgnucash/engine/gncEntry.c:1418-1428`).
fn entry_pricing<'a>(entry: &Entry, customer: bool, lines: Option<&'a [&'a TaxTableEntry]>) -> Pricing<'a> {
    if customer {
        Pricing {
            quantity: entry.quantity,
            price: entry.i_price,
            tax_lines: lines.filter(|_| entry.i_taxable),
            tax_included: entry.i_tax_included,
            discount: entry.i_discount,
            discount_type: entry.i_disc_type,
            discount_how: entry.i_disc_how,
        }
    } else {
        Pricing {
            quantity: entry.quantity,
            price: entry.b_price,
            tax_lines: lines.filter(|_| entry.b_taxable),
            tax_included: entry.b_tax_included,
            discount: Numeric::ZERO,
            discount_type: AmountType::Value,
            discount_how: DiscountHow::PreTax,
        }
    }
}

/// The tax table one side of an entry uses.
pub fn entry_tax_table<'a>(book: &'a Book, entry: &Entry, customer: bool) -> DomainResult<Option<&'a TaxTable>> {
    let guid = if customer { entry.i_tax_table } else { entry.b_tax_table };
    guid.map(|g| book.business.tax_tables.get(&g).map(AsRef::as_ref).ok_or_else(|| DomainError::not_found("tax table", &g))).transpose()
}

/// Compute one side of an entry, rounded to `denom`.
pub fn entry_amounts(book: &Book, entry: &Entry, customer: bool, denom: i64) -> DomainResult<EntryAmounts> {
    let table = entry_tax_table(book, entry, customer)?;
    let lines = table.map(|t| sorted_tax_lines(book, t));
    let computed = compute_value(&entry_pricing(entry, customer, lines.as_deref()))?;
    let tax = computed.taxes.iter().try_fold(Numeric::ZERO, |acc, (_, v)| acc.add(*v, DENOM_AUTO, REDUCE_HALF_UP))?;
    let tax_rounded = computed.taxes.iter().try_fold(Numeric::ZERO, |acc, (_, v)| acc.add(*v, denom, EXACT_HALF_UP))?;
    let discount = if customer { computed.discount } else { Numeric::ZERO };
    Ok(EntryAmounts {
        value: computed.value,
        value_rounded: computed.value.to_denom(denom, Round::HalfUp)?,
        discount,
        discount_rounded: discount.to_denom(denom, Round::HalfUp)?,
        taxes: computed.taxes,
        tax,
        tax_rounded,
        net_price: computed.net_price,
    })
}

/// `gncEntryGetPrice(entry, cust_doc, net = TRUE)`: the pre-tax unit price rounded to the entry's denominator
/// (`5.17:libgnucash/engine/gncEntry.c:1497-1532`).
pub fn entry_net_price(book: &Book, entry: &Entry, customer: bool) -> DomainResult<Numeric> {
    let table = entry_tax_table(book, entry, customer)?;
    let lines = table.map(|t| sorted_tax_lines(book, t));
    let computed = compute_value(&entry_pricing(entry, customer, lines.as_deref()))?;
    Ok(computed.net_price.to_denom(entry_denom(book, entry), Round::HalfUp)?)
}

/// `gncEntryGetDocTaxValues`: a copy of the tax list built by prepending (so reversed), negated for credit notes
/// (`5.17:libgnucash/engine/gncEntry.c:1548-1563`).
fn doc_tax_values(taxes: &AccountValues, credit_note: bool) -> DomainResult<AccountValues> {
    let mut out = AccountValues::new();
    taxes.iter().try_for_each(|(a, v)| account_value_add(&mut out, *a, if credit_note { v.neg() } else { *v }))?;
    Ok(out)
}

// ---------------------------------------------------------------------------------------------------------------
// Document totals

/// A document's totals, signed as printed on the document (`5.17:libgnucash/engine/gncInvoice.c:917-1030`).
#[derive(Debug, Clone, PartialEq)]
pub struct DocumentTotals {
    /// Σ entries' rounded values (`gncInvoiceGetTotalSubtotal`).
    pub subtotal: Numeric,
    /// Σ of `taxes` (`gncInvoiceGetTotalTax`).
    pub tax: Numeric,
    /// Subtotal + tax (`gncInvoiceGetTotal`).
    pub total: Numeric,
    /// Tax per account: the unrounded taxes of every entry summed per account, then rounded once
    /// (`gncInvoiceGetTotalTaxList`), in GnuCash's list order.
    pub taxes: AccountValues,
}

/// The currency fraction a document's totals are rounded to; `0` (no rounding) when it has no currency, as
/// `gnc_commodity_get_fraction(NULL)` gives.
fn document_denom(book: &Book, invoice: &Invoice) -> i64 {
    invoice.currency.as_ref().and_then(|c| commodity_fraction(book, c)).unwrap_or(0)
}

/// `gncInvoiceGetNetAndTaxesInternal` and `gncInvoiceGetTotalInternal`, optionally restricted to entries paid one
/// way (`gncInvoiceGetTotalOf`).
pub fn document_totals_of(book: &Book, invoice: &Invoice, payment: Option<PaymentType>) -> DomainResult<DocumentTotals> {
    let kind = invoice_type(book, invoice)?;
    let denom = document_denom(book, invoice);
    let mut subtotal = Numeric::ZERO;
    let mut taxes = AccountValues::new();
    documents::entries_of(book, invoice.guid).into_iter().filter(|e| payment.is_none_or(|p| e.b_payment == p)).try_for_each(|e| -> DomainResult<()> {
        let amounts = entry_amounts(book, e, kind.is_customer(), entry_denom(book, e))?;
        let value = if kind.credit_note { amounts.value_rounded.neg() } else { amounts.value_rounded };
        subtotal = subtotal.add(value, DENOM_AUTO, LCD)?;
        account_values_add_list(&mut taxes, &doc_tax_values(&amounts.taxes, kind.credit_note)?)
    })?;
    taxes.iter_mut().try_for_each(|(_, v)| -> DomainResult<()> {
        *v = v.convert(denom, EXACT_HALF_UP)?;
        Ok(())
    })?;
    let tax = taxes.iter().try_fold(Numeric::ZERO, |acc, (_, v)| acc.add(*v, DENOM_AUTO, EXACT_HALF_UP))?;
    let total = subtotal.add(tax, DENOM_AUTO, EXACT_HALF_UP)?;
    Ok(DocumentTotals { subtotal, tax, total, taxes })
}

/// One entry's rounded values as printed on its document (`gncEntryGetDoc*Value`, `5.17:libgnucash/engine/gncEntry.c:1535-1566`).
#[derive(Debug, Clone, PartialEq)]
pub struct DocumentLine {
    /// The entry.
    pub entry: Guid,
    /// Value, rounded.
    pub value: Numeric,
    /// Discount, rounded.
    pub discount: Numeric,
    /// Tax, rounded.
    pub tax: Numeric,
}

/// Every entry of a document with its rounded, document-signed values, in document order.
pub fn document_lines(book: &Book, invoice: &Invoice) -> DomainResult<Vec<DocumentLine>> {
    let kind = invoice_type(book, invoice)?;
    let sign = |n: Numeric| if kind.credit_note { n.neg() } else { n };
    documents::entries_of(book, invoice.guid)
        .into_iter()
        .map(|e| {
            let a = entry_amounts(book, e, kind.is_customer(), entry_denom(book, e))?;
            Ok(DocumentLine { entry: e.guid, value: sign(a.value_rounded), discount: sign(a.discount_rounded), tax: sign(a.tax_rounded) })
        })
        .collect()
}

/// A document's totals over all its entries.
pub fn document_totals(book: &Book, invoice: &Invoice) -> DomainResult<DocumentTotals> {
    document_totals_of(book, invoice, None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Frame;

    fn n(num: i64, denom: i64) -> Numeric {
        Numeric::new(num, denom)
    }

    fn line(account: u8, kind: AmountType, amount: Numeric) -> TaxTableEntry {
        TaxTableEntry { account: Some(Guid::from_bytes([account; 16])), amount, kind }
    }

    fn pricing<'a>(
        quantity: Numeric,
        price: Numeric,
        tax_lines: Option<&'a [&'a TaxTableEntry]>,
        tax_included: bool,
        discount: Numeric,
        discount_type: AmountType,
        discount_how: DiscountHow,
    ) -> Pricing<'a> {
        Pricing { quantity, price, tax_lines, tax_included, discount, discount_type, discount_how }
    }

    /// `design/notes/engine-business.md` §12.3: qty 4, price 5, Fee VALUE 2 + VAT 10 %.
    #[test]
    fn discount_and_tax_modes_match_the_oracle() {
        let fee = line(1, AmountType::Value, n(2, 1));
        let vat = line(2, AmountType::Percent, n(10, 1));
        let lines = [&fee, &vat];
        let case = |incl, d, t, h| {
            let v = compute_value(&pricing(n(4, 1), n(5, 1), Some(&lines), incl, d, t, h)).unwrap();
            let tax = v.taxes.iter().fold(Numeric::ZERO, |a, (_, x)| a.add_exact(*x).unwrap());
            (v.value, tax, v.discount, v.net_price)
        };
        let pct = AmountType::Percent;
        let val = AmountType::Value;
        let expect = |got: (Numeric, Numeric, Numeric, Numeric), want: [Numeric; 4]| {
            assert_eq!([got.0, got.1, got.2, got.3], want, "{got:?}");
        };
        expect(case(true, Numeric::ZERO, pct, DiscountHow::PreTax), [n(180, 11), n(40, 11), Numeric::ZERO, n(45, 11)]);
        expect(case(false, n(3, 1), val, DiscountHow::PreTax), [n(17, 1), n(37, 10), n(3, 1), n(5, 1)]);
        expect(case(false, n(10, 1), pct, DiscountHow::SameTime), [n(18, 1), n(4, 1), n(2, 1), n(5, 1)]);
        expect(case(false, n(10, 1), pct, DiscountHow::PreTax), [n(18, 1), n(19, 5), n(2, 1), n(5, 1)]);
        expect(case(false, n(10, 1), pct, DiscountHow::PostTax), [n(88, 5), n(4, 1), n(12, 5), n(5, 1)]);
        expect(case(false, n(3, 1), val, DiscountHow::PostTax), [n(17, 1), n(4, 1), n(3, 1), n(5, 1)]);
        expect(case(true, n(10, 1), pct, DiscountHow::PreTax), [n(162, 11), n(191, 55), n(18, 11), n(45, 11)]);
    }

    #[test]
    fn taxes_are_listed_in_reverse_table_order_and_rounded_by_running_sum() {
        // §4.3 A: qty 3, price 33.33, 10 % PRETAX, GST 5 % + PST 7.5 %.
        let gst = line(1, AmountType::Percent, n(5, 1));
        let pst = line(2, AmountType::Percent, n(75, 10));
        let lines = [&gst, &pst];
        let v = compute_value(&pricing(n(3, 1), n(3333, 100), Some(&lines), false, n(10, 1), AmountType::Percent, DiscountHow::PreTax)).unwrap();
        assert_eq!(v.taxes.iter().map(|(a, _)| *a).collect::<Vec<_>>(), vec![Guid::from_bytes([2; 16]), Guid::from_bytes([1; 16])]);
        let rounded = v.taxes.iter().try_fold(Numeric::ZERO, |acc, (_, x)| acc.add(*x, 100, EXACT_HALF_UP)).unwrap();
        assert!(rounded.same_repr(n(1125, 100)));
        assert!(v.value.to_denom(100, Round::HalfUp).unwrap().same_repr(n(8999, 100)));
    }

    fn term(kind: BillTermKind) -> BillTerm {
        BillTerm {
            guid: Guid::from_bytes([9; 16]),
            name: "T".into(),
            description: String::new(),
            refcount: 0,
            invisible: false,
            slots: Frame::new(),
            child: None,
            parent: None,
            kind: Some(kind),
        }
    }

    fn day(y: i32, m: u32, d: u32) -> Time64 {
        Date::from_ymd(y, m, d).unwrap().neutral_time()
    }

    /// `design/notes/engine-business.md` §3.2, oracle table.
    #[test]
    fn due_dates_match_the_oracle() {
        let days = |d| term(BillTermKind::Days { due_days: d, disc_days: 0, discount: Numeric::ZERO });
        let prox = |due, cutoff| term(BillTermKind::Proximo { due_day: due, disc_day: 0, discount: Numeric::ZERO, cutoff_day: cutoff });
        let check = |t: &BillTerm, post: Time64, want: Time64| assert_eq!(due_date(Some(t), post, &TimeZone::utc()).unwrap(), want);
        check(&days(30), day(2026, 1, 15), day(2026, 2, 14));
        check(&days(0), day(2010, 6, 14), day(2010, 6, 14));
        let p = prox(20, 19);
        check(&p, day(2010, 6, 14), day(2010, 7, 20));
        check(&p, day(2010, 6, 19), day(2010, 7, 20));
        check(&p, day(2010, 6, 22), day(2010, 8, 20));
        check(&p, day(2010, 11, 20), day(2011, 1, 20));
        check(&p, day(2010, 12, 25), day(2011, 2, 20));
        let p = prox(31, -3);
        check(&p, day(2011, 1, 10), day(2011, 2, 28));
        check(&p, day(2011, 1, 29), day(2011, 3, 31));
        check(&p, day(2011, 2, 25), day(2011, 3, 31));
        check(&p, day(2011, 2, 26), day(2011, 4, 30));
        check(&prox(15, 0), day(2012, 2, 28), day(2012, 3, 15));
        check(&prox(15, 0), day(2012, 2, 29), day(2012, 3, 15));
        assert!(due_date(Some(&prox(0, 1)), day(2012, 2, 29), &TimeZone::utc()).is_err());
        assert_eq!(due_date(None, Time64(5), &TimeZone::utc()).unwrap(), Time64(5));
    }

    proptest::proptest! {
        /// Without tax and discount the value is exactly quantity × price, and value + tax is what the buyer pays
        /// when the price includes a percentage tax.
        #[test]
        fn tax_included_values_add_back_to_the_aggregate(q in 1_i64..10_000, p in 1_i64..1_000_000, rate in 0_i64..100) {
            let vat = line(1, AmountType::Percent, n(rate, 1));
            let lines = [&vat];
            let v = compute_value(&pricing(n(q, 100), n(p, 100), Some(&lines), true, Numeric::ZERO, AmountType::Percent, DiscountHow::PreTax)).unwrap();
            let paid = v.taxes.iter().fold(v.value, |a, (_, x)| a.add_exact(*x).unwrap());
            proptest::prop_assert_eq!(paid, n(q, 100).mul(n(p, 100), DENOM_AUTO, How::EXACT).unwrap());
        }
    }
}
