//! Business operations: choosing what a payment applies to, and turning an existing transaction into a payment.
//!
//! The open-items list and its order follow GnuCash's payment dialog (`5.17:gnucash/gnome/dialog-payment.c`); a
//! transaction becomes a payment as the dialog opened on it does (`gnc_ui_payment_new_with_txn`, `:1798-1854`), then
//! `gncOwnerApplyPaymentSecs` with the transaction preset (`5.17:libgnucash/engine/gncOwner.c:1404-1440`). See
//! `design/api/business-posting.md`, "Payment matching and pre-payments".

use crate::business::OwnerRef;
use crate::ops::business::calc::{self, InvoiceType};
use crate::ops::business::payments::{
    self, LotWork, Payment, PaymentResult, auto_apply_with_lots, create_payment_lot, lot_end_owner_invoice_first,
    lot_end_owner_slots_first, lot_invoice, open_lots,
};
use crate::ops::business::posting::DATE_DUE;
use crate::{AccountType, Book, DomainError, DomainResult, Guid, Numeric, SlotValue, Time64, Transaction};

/// An owner's open document or pre-payment in an A/R or A/P account, as the payment dialog lists it.
#[derive(Debug, Clone, PartialEq)]
pub struct OpenItem {
    /// The lot.
    pub lot: Guid,
    /// The document, and its kind; none for a pre-payment.
    pub document: Option<(Guid, InvoiceType)>,
    /// The document's posted date, or the date of a pre-payment's latest split.
    pub date: Time64,
    /// What the dialog sorts chosen items by: the document's due date, or a pre-payment's earliest split date.
    pub sort_date: Time64,
    /// The lot's balance, signed as on the account.
    pub balance: Numeric,
}

/// The document number GnuCash's list shows, for its display order; none for a pre-payment.
fn item_id<'a>(book: &'a Book, item: &OpenItem) -> Option<&'a str> {
    item.document
        .and_then(|(g, _)| book.business.invoices.get(&g))
        .map(|i| i.id.as_str())
}

/// One lot as the dialog lists it, or none when GnuCash would skip it (a pre-payment with no splits).
fn open_item(book: &Book, work: &mut LotWork, lot: Guid, balance: Numeric) -> DomainResult<Option<OpenItem>> {
    let item = match lot_invoice(book, lot) {
        Some(inv) => {
            let txn = inv.post_txn.and_then(|g| book.transaction(g));
            let due = txn.map_or(Time64(i64::MAX), |t| {
                t.slots.get(DATE_DUE).and_then(SlotValue::as_time).unwrap_or(t.date_posted)
            });
            let date = inv.posted.unwrap_or(Time64(i64::MAX));
            OpenItem {
                lot,
                document: Some((inv.guid, calc::invoice_type(book, inv)?)),
                date,
                sort_date: due,
                balance,
            }
        }
        None if work.count(book, lot) == 0 => return Ok(None),
        None => {
            let date = work.latest_split_date(book, lot);
            OpenItem {
                lot,
                document: None,
                date,
                sort_date: work.earliest_split_date(book, lot),
                balance,
            }
        }
    };
    Ok(Some(item))
}

/// The dialog's display order (date, then number, no number first), then the order `g_list_insert_sorted` with
/// `gncOwnerLotsSortFunc` gives the chosen items: by sort date, each inserted before the first item it is not later
/// than (`5.17:gnucash/gnome/dialog-payment.c:933-945`, `:1186-1206`; `5.17:libgnucash/engine/gncOwner.c:728-745`).
pub fn dialog_order(book: &Book, mut items: Vec<OpenItem>) -> Vec<OpenItem> {
    items.sort_by(|a, b| a.date.cmp(&b.date).then_with(|| item_id(book, a).cmp(&item_id(book, b))));
    items.into_iter().fold(Vec::new(), |mut out: Vec<OpenItem>, item| {
        let at = out.iter().position(|o| o.sort_date >= item.sort_date).unwrap_or(out.len());
        out.insert(at, item);
        out
    })
}

/// The open lots of `owner` in `account`, in the dialog's order (`gnc_payment_window_fill_docs_list`,
/// `5.17:gnucash/gnome/dialog-payment.c:440-603`).
pub fn open_items(book: &Book, owner: OwnerRef, account: Guid) -> DomainResult<Vec<OpenItem>> {
    let mut work = LotWork::default();
    let items = list_items(book, &mut work, owner, account, &[])?;
    Ok(dialog_order(book, items))
}

/// The dialog's list. `compensate` holds the lots of a transaction being turned into a payment, each with its split's
/// value there: such a lot is listed with that value taken off, closed ones included, and left out when nothing
/// remains (`5.17:gnucash/gnome/dialog-payment.c:474-497`, `:555-575`).
fn list_items(
    book: &Book,
    work: &mut LotWork,
    owner: OwnerRef,
    account: Guid,
    compensate: &[(Guid, Numeric)],
) -> DomainResult<Vec<OpenItem>> {
    let mut lots = open_lots(book, work, account, |_, l| Ok(lot_end_owner_invoice_first(book, l) == Some(owner)))?;
    for (lot, _) in compensate.iter().rev() {
        let closed = book.lot(*lot).is_some_and(|l| l.account == account) && work.balance(book, *lot)?.is_zero();
        if closed && lot_end_owner_slots_first(book, *lot) == Some(owner) {
            lots.insert(0, *lot);
        }
    }
    lots.into_iter()
        .map(|lot| {
            let mut balance = work.balance(book, lot)?;
            if let Some((_, value)) = compensate.iter().find(|(l, _)| *l == lot) {
                balance = balance.sub_fixed(*value)?;
            }
            if balance.is_zero() {
                Ok(None)
            } else {
                open_item(book, work, lot, balance)
            }
        })
        .filter_map(Result::transpose)
        .collect()
}

// ---------------------------------------------------------------------------------------------------------------
// Payment from a transaction

/// A transaction to turn into a payment. Unset fields take the payment dialog's defaults.
#[derive(Debug, Clone, PartialEq)]
pub struct FromTransaction {
    /// The transaction.
    pub transaction: Guid,
    /// Who pays or is paid.
    pub owner: OwnerRef,
    /// The A/R or A/P account; by default that of the transaction's own A/R or A/P splits.
    pub account: Option<Guid>,
    /// The split to keep as the transfer split; by default the only split in a payment account.
    pub transfer_split: Option<Guid>,
    /// The engine amount (positive is money into the transfer account); by default the transfer split's amount.
    pub amount: Option<Numeric>,
    /// Transfer-commodity units per A/R or A/P commodity unit.
    pub exchange_rate: Numeric,
    /// The posted time; by default the day-neutral time of the transaction's posted date.
    pub date: Option<Time64>,
    /// The new split's memo; by default the transaction's description.
    pub memo: Option<String>,
    /// The number; by default the transaction's (the transfer split's action when the book uses it as the number).
    pub num: Option<String>,
    /// Lots to apply it to; by default the dialog's preselection.
    pub apply_to: Option<Vec<Guid>>,
    /// With nothing to apply to, apply to all the owner's open lots.
    pub auto_pay: bool,
}

/// `gncBusinessIsPaymentAcctType`: asset, liability and equity types but A/R and A/P
/// (`5.17:libgnucash/engine/gncBusiness.c:92-99`, `5.17:libgnucash/engine/Account.cpp:4469-4480`).
pub fn is_payment_account_type(t: AccountType) -> bool {
    !t.is_ap_ar()
        && (t.compatible_with_parent(AccountType::Asset) || t.compatible_with_parent(AccountType::Liability) || t == AccountType::Equity)
}

fn account_type(book: &Book, account: Option<Guid>) -> Option<AccountType> {
    account.and_then(|a| book.account(a)).map(|a| a.account_type)
}

/// `gncOwnerGetOwnerFromTxn`: for a transaction with a type, the owner of the document or pre-payment lot of its first
/// A/R or A/P split in such a lot (`5.17:libgnucash/engine/gncOwner.c:674-697`). A document gives its own owner,
/// which may be a job.
pub fn transaction_owner(book: &Book, t: &Transaction) -> Option<OwnerRef> {
    t.txn_type()?;
    t.splits
        .iter()
        .filter(|s| account_type(book, s.account).is_some_and(AccountType::is_ap_ar))
        .find_map(|s| {
            let lot = s.lot?;
            match lot_invoice(book, lot) {
                Some(inv) => Some(inv.owner),
                None => payments::lot_slot_owner(book, lot),
            }
        })
}

/// What qualifies, as `gnc_business_assign_payment`, the business menu and `gnc_ui_payment_new_with_txn` check
/// (`5.17:gnucash/gnome/gnc-plugin-business.c:677-679`, `:776-785`; `5.17:gnucash/gnome/dialog-payment.c:1614-1796`).
/// Returns the transfer split and the A/R or A/P account the splits name, if any.
fn check_transaction(book: &Book, t: &Transaction, transfer: Option<Guid>) -> DomainResult<(Guid, Option<Guid>)> {
    if t.splits.len() < 2 {
        return Err(DomainError::invalid("a payment needs a transaction with at least two splits"));
    }
    if t.txn_type() == Some('I') {
        return Err(DomainError::invalid(
            "a posted document's transaction cannot become a payment; unpost the document",
        ));
    }
    if t.is_void() || t.read_only_reason().is_some() {
        return Err(DomainError::invalid("a void or read-only transaction cannot become a payment"));
    }
    let mut apar: Vec<Guid> = t
        .splits
        .iter()
        .filter_map(|s| {
            s.account
                .filter(|a| account_type(book, Some(*a)).is_some_and(AccountType::is_ap_ar))
        })
        .collect();
    apar.sort_unstable();
    apar.dedup();
    if apar.len() > 1 {
        return Err(DomainError::invalid(
            "the transaction has splits in more than one A/R or A/P account",
        ));
    }
    let candidates: Vec<Guid> = t
        .splits
        .iter()
        .filter(|s| account_type(book, s.account).is_some_and(is_payment_account_type))
        .map(|s| s.guid)
        .collect();
    let split = match (transfer, candidates.as_slice()) {
        (Some(g), _) if candidates.contains(&g) => g,
        (Some(g), _) => {
            return Err(DomainError::invalid(format!(
                "split {g} is not a split of this transaction in a payment account"
            )));
        }
        (None, [only]) => *only,
        (None, []) => {
            return Err(DomainError::invalid(
                "the transaction has no split in a bank, cash, asset, liability or equity account",
            ));
        }
        (None, _) => {
            return Err(DomainError::invalid(
                "the transaction has several splits that could be the payment: give transferSplit",
            ));
        }
    };
    Ok((split, apar.first().copied()))
}

/// The transaction's A/R or A/P splits in business lots, each lot with the split's value (`select_txn_lots`).
fn transaction_lots(book: &Book, t: &Transaction) -> Vec<(Guid, Numeric)> {
    t.splits
        .iter()
        .filter(|s| account_type(book, s.account).is_some_and(AccountType::is_ap_ar))
        .filter_map(|s| s.lot.map(|l| (l, s.value)))
        .collect()
}

/// The payment the dialog would make from `t`, with the request's overrides.
fn payment_of(book: &Book, t: &Transaction, req: &FromTransaction, account: Guid, transfer: Guid) -> DomainResult<Payment> {
    let split = t.split(transfer).ok_or_else(|| DomainError::not_found("split", &transfer))?;
    let xfer = split
        .account
        .ok_or_else(|| DomainError::invalid("the transfer split has no account"))?;
    let commodity = |a: Guid| book.account(a).and_then(|a| a.commodity.clone());
    let amount = match req.amount {
        Some(a) => a,
        None if commodity(xfer) == commodity(account) => split.amount,
        None => {
            return Err(DomainError::invalid(
                "the transfer and A/R or A/P commodities differ: give the amount",
            ));
        }
    };
    if amount.is_zero() {
        return Err(DomainError::invalid("a payment from a transaction needs an amount"));
    }
    let date = match req.date {
        Some(d) => d,
        None => crate::ops::transactions::posted_date(t)
            .map(crate::Date::neutral_time)
            .ok_or_else(|| DomainError::invalid("the posted date is out of range"))?,
    };
    // gnc_get_num_action(txn, payment_split) (5.17:gnucash/gnome/dialog-payment.c:1836).
    let num = req
        .num
        .clone()
        .unwrap_or_else(|| crate::book_options::number(book, t, split).unwrap_or_default().to_owned());
    Ok(Payment {
        owner: req.owner,
        posted_account: account,
        transfer_account: Some(xfer),
        amount,
        exchange_rate: req.exchange_rate,
        date,
        memo: req.memo.clone().unwrap_or_else(|| t.description.clone().unwrap_or_default()),
        num,
    })
}

/// Turn a transaction into a payment, or re-match an existing payment: the payment dialog opened on it, then OK.
pub fn payment_from_transaction(book: &mut Book, env: &mut dyn crate::Env, req: &FromTransaction) -> DomainResult<PaymentResult> {
    let t = book
        .transaction(req.transaction)
        .cloned()
        .ok_or_else(|| DomainError::not_found("transaction", &req.transaction))?;
    let (transfer, own_account) = check_transaction(book, &t, req.transfer_split)?;
    let account = req
        .account
        .or(own_account)
        .ok_or_else(|| DomainError::invalid("the transaction has no A/R or A/P split: give the account"))?;
    let pay = payment_of(book, &t, req, account, transfer)?;
    payments::check_owner_and_account(book, &pay)?;
    let mut work = LotWork::default();
    // The dialog lists the owner's items, sorting pre-payment lots' splits by date as it reads them.
    let own_lots = if Some(account) == own_account {
        transaction_lots(book, &t)
    } else {
        Vec::new()
    };
    let listed = list_items(book, &mut work, req.owner, account, &own_lots)?;
    let chosen: Vec<OpenItem> = match &req.apply_to {
        Some(lots) => lots
            .iter()
            .map(|l| item_for(book, &mut work, &listed, *l))
            .collect::<DomainResult<_>>()?,
        None => listed.into_iter().filter(|i| own_lots.iter().any(|(l, _)| *l == i.lot)).collect(),
    };
    let mut selected: Vec<Option<Guid>> = if chosen.is_empty() && req.auto_pay {
        open_lots(book, &mut work, account, |_, l| {
            Ok(lot_end_owner_invoice_first(book, l) == Some(req.owner))
        })?
        .into_iter()
        .map(Some)
        .collect()
    } else {
        dialog_order(book, chosen).into_iter().map(|i| Some(i.lot)).collect()
    };
    let (txn, lot) = create_payment_lot(book, env, &mut work, &pay, Some(req.transaction))?;
    selected.insert(0, Some(lot));
    auto_apply_with_lots(book, env, &mut work, req.owner, &mut selected)?;
    let payment_lot = Some(lot).filter(|l| work.count(book, *l) > 0);
    work.drop_empty_lots(book);
    Ok(PaymentResult {
        transaction: Some(txn),
        payment_lot,
    })
}

/// A chosen lot as an item, from the list when it is there.
fn item_for(book: &Book, work: &mut LotWork, listed: &[OpenItem], lot: Guid) -> DomainResult<OpenItem> {
    if let Some(item) = listed.iter().find(|i| i.lot == lot) {
        return Ok(item.clone());
    }
    let balance = work.balance(book, lot)?;
    open_item(book, work, lot, balance)?.ok_or_else(|| DomainError::invalid(format!("lot {lot} has no splits")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(n: u8, date: i64, sort_date: i64) -> OpenItem {
        OpenItem {
            lot: Guid::from_bytes([n; 16]),
            document: None,
            date: Time64(date),
            sort_date: Time64(sort_date),
            balance: Numeric::ONE,
        }
    }

    #[test]
    fn chosen_items_sort_by_sort_date_with_ties_reversed() {
        let mut env = crate::FixedEnv::new(Time64(0), 0);
        let book = Book::new_empty(&mut env);
        let ordered = dialog_order(&book, vec![item(1, 30, 5), item(2, 10, 5), item(3, 20, 1), item(4, 40, 9)]);
        let lots: Vec<u8> = ordered.iter().map(|i| i.lot.as_bytes()[0]).collect();
        // Display order 3, 2, 1, 4; equal sort dates (2 and 1) each go before the other.
        assert_eq!(lots, [3, 1, 2, 4]);
    }

    #[test]
    fn payment_accounts_are_asset_liability_and_equity_types() {
        use AccountType as T;
        let payment = [
            T::Bank,
            T::Cash,
            T::Asset,
            T::Credit,
            T::Liability,
            T::Stock,
            T::Mutual,
            T::Currency,
            T::Equity,
        ];
        assert!(payment.iter().all(|t| is_payment_account_type(*t)));
        assert!(
            [T::Receivable, T::Payable, T::Income, T::Expense, T::Trading, T::Root]
                .iter()
                .all(|t| !is_payment_account_type(*t))
        );
    }
}
