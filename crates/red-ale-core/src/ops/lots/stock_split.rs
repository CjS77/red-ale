//! The stock split assistant: a share-only split in a security account, with an optional price and cash in lieu
//! (`gnc_stock_split_assistant_finish`, `5.17:gnucash/gnome/assistant-stock-split.c:339-473`).

use std::sync::Arc;

use super::edit_txn;
use crate::lot::split_slots;
use crate::ops::commit::{NewSplit, NewTransaction, add_transaction, currency_fraction, round_amount};
use crate::{AccountType, Book, CommodityRef, Date, DomainError, DomainResult, Env, Guid, Numeric, Price, SlotValue, Time64};

/// The action of a stock split's split (`C_("Action Column", "Split")`, `5.17:gnucash/gnome/assistant-stock-split.c:388`).
pub const STOCK_SPLIT_ACTION: &str = "Split";
/// The assistant's default description (`5.17:gnucash/gtkbuilder/assistant-stock-split.glade:158`).
pub const DEFAULT_DESCRIPTION: &str = "Stock Split";
/// The assistant's default cash-in-lieu memo (`5.17:gnucash/gtkbuilder/assistant-stock-split.glade:309`).
pub const DEFAULT_MEMO: &str = "Cash in lieu";
/// The source of the price the assistant records (`PRICE_SOURCE_STOCK_SPLIT`, `5.17:libgnucash/engine/gnc-pricedb.cpp:105-120`).
pub const PRICE_SOURCE_STOCK_SPLIT: &str = "user:stock-split";
/// The type of that price (`PRICE_TYPE_UNK`).
const PRICE_TYPE_UNKNOWN: &str = "unknown";

/// A stock split to record.
#[derive(Debug, Clone, PartialEq)]
pub struct StockSplit {
    /// The STOCK, MUTUAL or CURRENCY account.
    pub account: Guid,
    /// The date.
    pub date: Date,
    /// Shares added (negative for a reverse split).
    pub shares: Numeric,
    /// Description; `None` means "Stock Split".
    pub description: Option<String>,
    /// The transaction currency; `None` means the currency of the nearest ancestor account held in a currency.
    pub currency: Option<CommodityRef>,
    /// The price per share after the split, recorded when positive.
    pub price: Option<Numeric>,
    /// The price's currency; `None` means the currency of the security's latest price, else the transaction currency.
    pub price_currency: Option<CommodityRef>,
    /// Cash paid in lieu of fractional shares, recorded when positive.
    pub cash: Option<Numeric>,
    /// The INCOME account the cash comes from.
    pub income_account: Option<Guid>,
    /// The BANK, CASH or ASSET account the cash goes to.
    pub asset_account: Option<Guid>,
    /// Memo of the cash splits; `None` means "Cash in lieu".
    pub memo: Option<String>,
}

/// What became of the price.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PriceOutcome {
    /// No positive price was given.
    None,
    /// The price was added.
    Added(Guid),
    /// An equal price on the same day exists, so GnuCash drops the new one silently.
    Duplicate,
    /// A same-day price from a source of higher precedence blocks it; GnuCash shows "Error adding price." and keeps
    /// the transaction.
    Rejected {
        /// The blocking price.
        existing: Guid,
    },
}

/// The outcome of a stock split.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StockSplitOutcome {
    /// The transaction recorded.
    pub transaction: Guid,
    /// What became of the price.
    pub price: PriceOutcome,
}

/// Record a stock split as the assistant does: one split in the security account with the shares as its amount, value
/// `0/1`, action `Split` and slot `split-type = stock-split`; a `user:stock-split` price when a positive price is
/// given; and, for positive cash in lieu, a debit to the asset account and a credit to the income account with
/// amount equal to value.
pub fn stock_split(book: &mut Book, env: &mut dyn Env, req: &StockSplit) -> DomainResult<StockSplitOutcome> {
    let commodity = check_account(book, req.account)?;
    if req.shares.is_zero() {
        return Err(DomainError::invalid("the number of shares must not be zero"));
    }
    let currency = match req.currency.clone() {
        Some(c) => c,
        None => default_currency(book, req.account)?,
    };
    currency_fraction(book, &currency)?;
    let cash = check_cash(book, req)?;
    if req.price.is_some_and(Numeric::is_negative) {
        return Err(DomainError::invalid("the price must not be negative"));
    }
    let shares = round_amount(book, req.account, req.shares)?;
    let mut new = NewTransaction::new(currency.clone(), req.date);
    new.description = Some(req.description.clone().unwrap_or_else(|| DEFAULT_DESCRIPTION.to_owned())).filter(|d| !d.is_empty());
    new.splits.push(NewSplit::new(req.account, Numeric::ZERO).with_amount(shares).with_action(STOCK_SPLIT_ACTION));
    if let Some((asset, income, cash)) = cash {
        let memo = req.memo.clone().unwrap_or_else(|| DEFAULT_MEMO.to_owned());
        new.splits.push(NewSplit::new(asset, cash).with_amount(cash).with_memo(memo.clone()));
        new.splits.push(NewSplit::new(income, cash.neg()).with_amount(cash.neg()).with_memo(memo));
    }
    let txn = add_transaction(book, env, new)?;
    // xaccSplitMakeStockSplit sets the value to gnc_numeric_zero() without converting it, so it is written 0/1
    // (5.17:libgnucash/engine/Split.cpp:2063-2075).
    edit_txn(book, txn, |t| {
        t.splits.iter_mut().filter(|s| s.account == Some(req.account)).for_each(|s| {
            s.value = Numeric::ZERO;
            s.slots.set(split_slots::SPLIT_TYPE, SlotValue::String(split_slots::STOCK_SPLIT.to_owned()));
        });
    })?;
    let price = match req.price.filter(|p| p.is_positive()) {
        Some(value) => {
            let price_currency = req.price_currency.clone().unwrap_or_else(|| latest_price_currency(book, &commodity).unwrap_or(currency));
            add_price(book, env, &commodity, price_currency, req.date.neutral_time(), value)
        }
        None => PriceOutcome::None,
    };
    Ok(StockSplitOutcome { transaction: txn, price })
}

/// The assistant offers accounts that are priced, not placeholders and hold shares
/// (`fill_account_list`, `5.17:gnucash/gnome/assistant-stock-split.c:111-186`). Returns the security.
fn check_account(book: &Book, account: Guid) -> DomainResult<CommodityRef> {
    let acc = book.account(account).ok_or_else(|| DomainError::not_found("account", &account))?;
    if !matches!(acc.account_type, AccountType::Stock | AccountType::Mutual | AccountType::Currency) {
        return Err(DomainError::invalid(format!("{} is not a STOCK, MUTUAL or CURRENCY account", acc.name)));
    }
    if acc.is_placeholder() {
        return Err(DomainError::invalid(format!("{} is a placeholder", acc.name)));
    }
    let balance = Numeric::sum_fixed(book.splits_of(account).map(|(_, s)| s.amount))?;
    if balance.is_zero() {
        return Err(DomainError::invalid(format!("{} holds no shares", acc.name)));
    }
    acc.commodity.clone().ok_or_else(|| DomainError::invalid(format!("{} has no commodity", acc.name)))
}

/// The cash in lieu with its asset and income accounts, when positive (`gnc_stock_split_assistant_cash_complete`,
/// `5.17:gnucash/gnome/assistant-stock-split.c:309-335`, account filters `:485-504`).
fn check_cash(book: &Book, req: &StockSplit) -> DomainResult<Option<(Guid, Guid, Numeric)>> {
    let Some(cash) = req.cash.filter(|c| !c.is_zero()) else { return Ok(None) };
    if cash.is_negative() {
        return Err(DomainError::invalid("cash in lieu must not be negative"));
    }
    let typed = |g: Option<Guid>, what: &str, ok: fn(AccountType) -> bool| -> DomainResult<Guid> {
        let g = g.ok_or_else(|| DomainError::invalid(format!("cash in lieu needs {what}")))?;
        let acc = book.account(g).ok_or_else(|| DomainError::not_found("account", &g))?;
        if ok(acc.account_type) { Ok(g) } else { Err(DomainError::invalid(format!("{} cannot be {what}", acc.name))) }
    };
    let income = typed(req.income_account, "an INCOME account", |t| t == AccountType::Income)?;
    let asset = typed(req.asset_account, "a BANK, CASH or ASSET account", |t| matches!(t, AccountType::Bank | AccountType::Cash | AccountType::Asset))?;
    Ok(Some((asset, income, cash)))
}

/// The currency of the nearest ancestor held in a currency. GnuCash uses the user's default currency preference,
/// which a book does not hold.
fn default_currency(book: &Book, account: Guid) -> DomainResult<CommodityRef> {
    book.ancestors(account)
        .into_iter()
        .find_map(|a| a.commodity.clone().filter(CommodityRef::is_currency))
        .ok_or_else(|| DomainError::invalid("no ancestor account is held in a currency; give the currency"))
}

/// The other side of the security's latest price, in either direction (`refresh_details_page`,
/// `5.17:gnucash/gnome/assistant-stock-split.c:190-249`).
fn latest_price_currency(book: &Book, commodity: &CommodityRef) -> Option<CommodityRef> {
    let mut latest: Option<&Price> = None;
    book.prices.values().filter(|p| &p.commodity == commodity || &p.currency == commodity).for_each(|p| {
        if latest.is_none_or(|l| p.time > l.time) {
            latest = Some(p);
        }
    });
    latest.map(|p| if &p.currency == commodity { p.commodity.clone() } else { p.currency.clone() })
}

// LOCAL COPY (lots package): GnuCash's same-day price rules from `add_price`
// (5.17:libgnucash/engine/gnc-pricedb.cpp:1013-1110), kept minimal for the stock split's price. The prices package
// owns these rules; replace this with `ops::prices` when merging.
const PRICE_SOURCES: [&str; 11] = [
    "user:price-editor",
    "Finance::Quote",
    "user:price",
    "user:xfer-dialog",
    "user:split-register",
    "user:split-import",
    "user:stock-split",
    "user:stock-transaction",
    "user:invoice-post",
    "temporary",
    "invalid",
];

/// Precedence of a source: its place in GnuCash's `PriceSource` enum; unknown strings read as `invalid`.
fn source_rank(source: Option<&str>) -> usize {
    source.and_then(|s| PRICE_SOURCES.iter().position(|k| *k == s)).unwrap_or(PRICE_SOURCES.len() - 1)
}

/// Add a price, applying the same-day rules: the nearest same-day price of the pair in either direction is replaced
/// unless its source has higher precedence, and an equal same-day price makes the new one a silent duplicate. GnuCash
/// takes "same day" in the local time zone; red-ale uses UTC.
fn add_price(book: &mut Book, env: &mut dyn Env, commodity: &CommodityRef, currency: CommodityRef, time: Time64, value: Numeric) -> PriceOutcome {
    let same_day = |t: Time64| t.utc_date() == time.utc_date();
    let pair = |p: &Price| {
        (&p.commodity == commodity && p.currency == currency) || (p.commodity == currency && &p.currency == commodity)
    };
    let old = book
        .prices
        .values()
        .filter(|p| pair(p) && same_day(p.time))
        .min_by_key(|p| ((p.time.0 - time.0).abs(), p.time.0))
        .map(|p| (p.guid, source_rank(p.source.as_deref())));
    if let Some((existing, rank)) = old {
        if source_rank(Some(PRICE_SOURCE_STOCK_SPLIT)) > rank {
            return PriceOutcome::Rejected { existing };
        }
        book.prices.shift_remove(&existing);
    }
    let duplicate = book.prices.values().any(|p| &p.commodity == commodity && p.currency == currency && same_day(p.time) && p.value == value);
    if duplicate {
        return PriceOutcome::Duplicate;
    }
    let price = Price {
        guid: env.new_guid(),
        commodity: commodity.clone(),
        currency,
        time,
        source: Some(PRICE_SOURCE_STOCK_SPLIT.to_owned()),
        price_type: Some(PRICE_TYPE_UNKNOWN.to_owned()),
        value,
    };
    let guid = price.guid;
    book.prices.insert(guid, Arc::new(price));
    PriceOutcome::Added(guid)
}
