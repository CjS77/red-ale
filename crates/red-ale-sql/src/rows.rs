//! Row decoders: one row of a table into the core value it holds, as the XML reader would give it for the element
//! GnuCash's XML writer writes from the same engine value (`design/sql-fidelity.md` §3, "One reading for both
//! formats"). Pure, so they can be fuzzed without a database.
//!
//! A decoder returns `None` when the row cannot enter the Book (its primary key, or a value the entity cannot do
//! without, is refused); every refusal is a finding.

use std::collections::HashMap;

use red_ale_core::budget::DEFAULT_NUM_PERIODS;
use red_ale_core::commodity::CURRENCY_NAMESPACE;
use red_ale_core::{
    Account, AccountType, Commodity, CommodityRef, Guid, Lot, Numeric, Occurrences, Price, ReconcileState, Recurrence, ScheduledTransaction,
    SlotValue, Split, Time64, Transaction, WeekendAdjust,
};

use crate::cell::{Cells, Row};
use crate::double::format_double;
use crate::tables::{
    ACCOUNTS, BOOKS, BUDGET_AMOUNTS, BUDGETS, COMMODITIES, LOTS, PRICES, RECURRENCES, SCHEDXACTIONS, SLOTS, SPLITS, TRANSACTIONS,
};

/// What decoders look up: each commodity's reference by its row's GUID.
#[derive(Debug, Default, Clone)]
pub struct Lookup {
    /// Commodities by GUID.
    pub commodities: HashMap<Guid, CommodityRef>,
}

impl Lookup {
    /// The commodity a reference column names; a GUID with no row is a finding.
    fn commodity(&self, cells: &mut Cells<'_>, column: &str) -> Option<CommodityRef> {
        let guid = cells.guid(column)?;
        let found = self.commodities.get(&guid).cloned();
        if found.is_none() {
            cells.refuse(column, format!("commodity {guid} has no row"));
        }
        found
    }
}

/// The primary key, or a finding and `None`.
fn key(cells: &mut Cells<'_>, column: &str) -> Option<Guid> {
    let guid = cells.guid(column);
    if guid.is_none() {
        cells.row_finding("left out: its GUID is missing or malformed");
    }
    guid
}

/// The `books` row: the Book's GUID and its two roots.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BookRow {
    /// The Book.
    pub guid: Guid,
    /// The root of the Account tree.
    pub root: Guid,
    /// The Template root.
    pub template_root: Guid,
}

/// Decode a `books` row.
pub fn book(row: &Row, findings: &mut Vec<String>) -> Option<BookRow> {
    let cells = &mut Cells::new(&BOOKS, row, findings);
    let guid = key(cells, "guid")?;
    let root = cells.guid("root_account_guid");
    let template_root = cells.guid("root_template_guid");
    let (Some(root), Some(template_root)) = (root, template_root) else {
        cells.row_finding("left out: a root GUID is missing");
        return None;
    };
    Some(BookRow { guid, root, template_root })
}

/// Decode a `commodities` row into its GUID and the commodity. A currency's full name, exchange code and fraction
/// are what the engine held, as for any commodity; GnuCash's XML writer leaves them out for currencies only because
/// the engine takes them from its ISO table (`5.17:libgnucash/backend/xml/gnc-commodity-xml-v2.cpp:79-95`).
pub fn commodity(row: &Row, findings: &mut Vec<String>) -> Option<(Guid, Commodity)> {
    let cells = &mut Cells::new(&COMMODITIES, row, findings);
    let guid = key(cells, "guid")?;
    let id = CommodityRef::new(cells.req_text("namespace"), cells.req_text("mnemonic"));
    let currency = id.namespace == CURRENCY_NAMESPACE;
    let fraction = cells.req_int("fraction");
    if fraction <= 0 {
        cells.refuse("fraction", format!("{fraction} is not a fraction"));
        return None;
    }
    let mut c = Commodity::new(id, Some(cells.text("fullname").unwrap_or_default()), fraction);
    // The XML writer writes an exchange code only when it is not empty, and none for a currency, whose ISO entry has one.
    c.cusip = if currency { Some(cells.text("cusip").unwrap_or_default()) } else { cells.nonempty_text("cusip") };
    c.quote_flag = cells.req_bool("quote_flag");
    // Quote source and time zone are written only for a commodity with quotes on (`:97-107`).
    if c.quote_flag {
        c.quote_source = cells.text("quote_source");
        c.quote_tz = cells.text("quote_tz");
    }
    Some((guid, c))
}

/// An `accounts` row: the account, and the two columns that duplicate its Slots.
#[derive(Debug, Clone, PartialEq)]
pub struct AccountRow {
    /// The account, with no Slots yet.
    pub account: Account,
    /// `hidden`; NULL is `None`.
    pub hidden: Option<bool>,
    /// `placeholder`; NULL is `None`.
    pub placeholder: Option<bool>,
}

/// Decode an `accounts` row. The SCU and the non-standard flag are written to XML only with a commodity
/// (`5.17:libgnucash/backend/xml/gnc-account-xml-v2.cpp:96-108`), so without one they read as the XML reader leaves
/// them; code and description are written only when not empty (`:110-118`).
pub fn account(row: &Row, lookup: &Lookup, findings: &mut Vec<String>) -> Option<AccountRow> {
    let cells = &mut Cells::new(&ACCOUNTS, row, findings);
    let guid = key(cells, "guid")?;
    let type_name = cells.req_text("account_type");
    let account_type = type_name.parse().unwrap_or_else(|_| {
        cells.refuse("account_type", format!("{type_name:?} is not an account type"));
        AccountType::None
    });
    let commodity = lookup.commodity(cells, "commodity_guid");
    let scu = cells.narrow::<i32>("commodity_scu");
    let non_std = cells.req_bool("non_std_scu");
    let account = Account {
        guid,
        name: cells.req_text("name"),
        account_type,
        commodity_scu: commodity.is_some().then_some(i64::from(scu)),
        non_standard_scu: commodity.is_some() && non_std,
        commodity,
        code: cells.nonempty_text("code"),
        description: cells.nonempty_text("description"),
        slots: red_ale_core::Frame::new(),
        parent: cells.guid("parent_guid"),
    };
    Some(AccountRow {
        account,
        hidden: cells.bool("hidden"),
        placeholder: cells.bool("placeholder"),
    })
}

/// Decode a `lots` row. `is_closed` is the engine's cache (−1, 0 or 1), not data (`design/sql-fidelity.md` §3).
pub fn lot(row: &Row, findings: &mut Vec<String>) -> Option<Lot> {
    let cells = &mut Cells::new(&LOTS, row, findings);
    let guid = key(cells, "guid")?;
    let closed = cells.req_int("is_closed");
    if !(-1..=1).contains(&closed) {
        cells.refuse("is_closed", format!("{closed} is not -1, 0 or 1"));
    }
    let Some(account) = cells.guid("account_guid") else {
        cells.row_finding("left out: a lot with no account");
        return None;
    };
    Some(Lot::new(guid, account))
}

/// Decode a `transactions` row, with no splits yet. The XML writer leaves out an empty num
/// (`5.17:libgnucash/backend/xml/gnc-transaction-xml-v2.cpp:146-147`) and always writes the description, which the
/// engine keeps as `""` when unset.
pub fn transaction(row: &Row, lookup: &Lookup, findings: &mut Vec<String>) -> Option<Transaction> {
    let cells = &mut Cells::new(&TRANSACTIONS, row, findings);
    let guid = key(cells, "guid")?;
    let Some(currency) = lookup.commodity(cells, "currency_guid") else {
        cells.row_finding("left out: a transaction with no currency");
        return None;
    };
    Some(Transaction {
        guid,
        currency,
        num: cells.nonempty_text("num"),
        date_posted: cells.time("post_date"),
        date_entered: cells.time("enter_date"),
        description: Some(cells.text("description").unwrap_or_default()),
        slots: red_ale_core::Frame::new(),
        splits: Vec::new(),
    })
}

/// Decode a `splits` row into its transaction's GUID and the split. Memo and action are written to XML only when
/// not empty, and the reconcile date only when not 0 (`5.17:libgnucash/backend/xml/gnc-transaction-xml-v2.cpp:75-107`);
/// the null GUID is no account (`:114-117`).
pub fn split(row: &Row, findings: &mut Vec<String>) -> Option<(Guid, Split)> {
    let cells = &mut Cells::new(&SPLITS, row, findings);
    let guid = key(cells, "guid")?;
    let Some(tx) = cells.guid("tx_guid") else {
        cells.row_finding("left out: a split with no transaction");
        return None;
    };
    let state = cells.req_text("reconcile_state");
    let mut chars = state.chars();
    let one = chars.next().and_then(ReconcileState::from_char).filter(|_| chars.next().is_none());
    let reconcile_state = one.unwrap_or_else(|| {
        cells.refuse("reconcile_state", format!("{state:?} is not a reconcile state"));
        ReconcileState::New
    });
    let split = Split {
        guid,
        memo: cells.nonempty_text("memo"),
        action: cells.nonempty_text("action"),
        reconcile_state,
        reconcile_date: Some(cells.time("reconcile_date")).filter(|t| *t != Time64(0)),
        value: cells.req_numeric("value"),
        amount: cells.req_numeric("quantity"),
        account: cells.guid("account_guid").filter(|g| !g.is_null()),
        lot: cells.guid("lot_guid"),
        slots: red_ale_core::Frame::new(),
    };
    Some((tx, split))
}

/// Decode a `prices` row. Source and type are written to XML only when not empty.
pub fn price(row: &Row, lookup: &Lookup, findings: &mut Vec<String>) -> Option<Price> {
    let cells = &mut Cells::new(&PRICES, row, findings);
    let guid = key(cells, "guid")?;
    let commodity = lookup.commodity(cells, "commodity_guid");
    let currency = lookup.commodity(cells, "currency_guid");
    let (Some(commodity), Some(currency)) = (commodity, currency) else {
        cells.row_finding("left out: a price without its commodity or currency");
        return None;
    };
    Some(Price {
        guid,
        commodity,
        currency,
        time: cells.time("date"),
        source: cells.nonempty_text("source"),
        price_type: cells.nonempty_text("type"),
        value: cells.req_numeric("value"),
    })
}

/// A `budgets` row, before its Recurrence and Slots are known.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BudgetRow {
    /// Identity.
    pub guid: Guid,
    /// Name.
    pub name: String,
    /// Description; NULL reads as empty, the engine's default.
    pub description: String,
    /// Periods; 0, which GnuCash refuses, reads as its default.
    pub num_periods: u32,
}

/// Decode a `budgets` row.
pub fn budget(row: &Row, findings: &mut Vec<String>) -> Option<BudgetRow> {
    let cells = &mut Cells::new(&BUDGETS, row, findings);
    let guid = key(cells, "guid")?;
    let num_periods = cells.narrow::<u32>("num_periods");
    Some(BudgetRow {
        guid,
        name: cells.req_text("name"),
        description: cells.text("description").unwrap_or_default(),
        num_periods: if num_periods == 0 { DEFAULT_NUM_PERIODS } else { num_periods },
    })
}

/// A `budget_amounts` row.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AmountRow {
    /// The budget.
    pub budget: Guid,
    /// The account.
    pub account: Guid,
    /// The period, from 0.
    pub period: i64,
    /// The amount, as stored.
    pub amount: Numeric,
}

/// Decode a `budget_amounts` row.
pub fn budget_amount(row: &Row, findings: &mut Vec<String>) -> Option<AmountRow> {
    let cells = &mut Cells::new(&BUDGET_AMOUNTS, row, findings);
    let (budget, account) = (cells.guid("budget_guid"), cells.guid("account_guid"));
    let (Some(budget), Some(account)) = (budget, account) else {
        cells.row_finding("left out: a budget amount without its budget or account");
        return None;
    };
    Some(AmountRow {
        budget,
        account,
        period: cells.req_int("period_num"),
        amount: cells.req_numeric("amount"),
    })
}

/// Decode a `recurrences` row into its owner's GUID and the Recurrence.
pub fn recurrence(row: &Row, findings: &mut Vec<String>) -> Option<(Guid, Recurrence)> {
    let cells = &mut Cells::new(&RECURRENCES, row, findings);
    let owner = cells.guid("obj_guid");
    let mult = cells.narrow::<u16>("recurrence_mult");
    let period_name = cells.req_text("recurrence_period_type");
    let period = period_name.parse().map_err(|e| cells.refuse("recurrence_period_type", e)).ok();
    let start = cells.date("recurrence_period_start");
    let adjust_name = cells.req_text("recurrence_weekend_adjust");
    let adjust = adjust_name.parse().unwrap_or_else(|e| {
        cells.refuse("recurrence_weekend_adjust", e);
        WeekendAdjust::None
    });
    let (Some(owner), Some(period), Some(start)) = (owner, period, start) else {
        cells.row_finding("left out: a recurrence without its owner, period type or start");
        return None;
    };
    match Recurrence::new(mult, period, start, adjust) {
        Ok(r) => Some((owner, r)),
        Err(e) => {
            cells.refuse("recurrence_mult", e);
            None
        }
    }
}

/// Decode a `schedxactions` row, with no Schedule or Slots yet. As the XML writer does, an occurrence count wins
/// over an end date (`5.17:libgnucash/backend/xml/gnc-schedxaction-xml-v2.cpp:127-148`); a count of 0 is none.
pub fn scheduled(row: &Row, findings: &mut Vec<String>) -> Option<ScheduledTransaction> {
    let cells = &mut Cells::new(&SCHEDXACTIONS, row, findings);
    let guid = key(cells, "guid")?;
    let Some(start) = cells.date("start_date") else {
        cells.row_finding("left out: a scheduled transaction with no start date");
        return None;
    };
    let total = cells.narrow::<i32>("num_occur");
    let remaining = cells.narrow::<i32>("rem_occur");
    let occurrences = (total != 0).then_some(Occurrences { total, remaining });
    let end = cells.date("end_date");
    Some(ScheduledTransaction {
        guid,
        name: cells.text("name").unwrap_or_default(),
        enabled: cells.req_bool("enabled"),
        auto_create: cells.req_bool("auto_create"),
        auto_create_notify: cells.req_bool("auto_notify"),
        advance_create_days: cells.narrow("adv_creation"),
        advance_remind_days: cells.narrow("adv_notify"),
        instance_count: cells.narrow("instance_count"),
        start,
        last: cells.date("last_occur"),
        end: if occurrences.is_some() { None } else { end },
        occurrences,
        template_account: cells.guid("template_act_guid"),
        schedule: Vec::new(),
        deferred: Vec::new(),
        slots: red_ale_core::Frame::new(),
    })
}

/// What one `slots` row holds.
#[derive(Debug, Clone, PartialEq)]
pub enum SlotCell {
    /// A value.
    Value(SlotValue),
    /// A frame: its children are the rows owned by this GUID.
    Frame(Guid),
    /// A list: its items are the rows owned by this GUID, in `rowid` order.
    List(Guid),
}

/// One `slots` row.
#[derive(Debug, Clone, PartialEq)]
pub struct SlotRow {
    /// The row's `id`.
    pub rowid: i64,
    /// The entity, frame or list that owns it.
    pub owner: Guid,
    /// Its full path.
    pub name: String,
    /// What it holds.
    pub cell: SlotCell,
}

/// Decode a `slots` row. Only the value column the `slot_type` names is read; the filler in the others is ignored,
/// as GnuCash ignores it (`5.17:libgnucash/backend/sql/gnc-slots-sql.cpp:315`, `:343`, `:374`, `:399`, `:509`,
/// `:539`). A value GnuCash's reader would skip is a finding, and the row is left out.
pub fn slot(row: &Row, findings: &mut Vec<String>) -> Option<SlotRow> {
    let cells = &mut Cells::new(&SLOTS, row, findings);
    let owner = cells.guid("obj_guid");
    let name = cells.req_text("name");
    let slot_type = cells.req_int("slot_type");
    let cell = match slot_type {
        1 => cells.int("int64_val").map(SlotValue::Integer).map(SlotCell::Value),
        2 => cells.real("double_val").map(|d| SlotCell::Value(SlotValue::Double(format_double(d.to_bits())))),
        3 => cells.numeric("numeric_val").map(SlotValue::Numeric).map(SlotCell::Value),
        4 => cells.text("string_val").map(SlotValue::String).map(SlotCell::Value),
        5 => cells.guid("guid_val").map(SlotValue::Guid).map(SlotCell::Value),
        6 => Some(SlotCell::Value(SlotValue::Timespec(cells.time("timespec_val")))),
        8 => cells.guid("guid_val").map(SlotCell::List),
        9 => cells.guid("guid_val").map(SlotCell::Frame),
        10 => cells.date("gdate_val").map(SlotValue::Date).map(SlotCell::Value),
        other => {
            cells.refuse("slot_type", format!("{other} is not a slot type GnuCash 5.17 reads"));
            return None;
        }
    };
    let (Some(owner), Some(cell)) = (owner, cell) else {
        cells.row_finding(format!("left out: slot {name:?} has no owner or no value of its type {slot_type}"));
        return None;
    };
    Some(SlotRow {
        rowid: cells.rowid(),
        owner,
        name,
        cell,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cell::Cell;

    fn text(s: &str) -> Cell {
        Cell::Text(s.as_bytes().to_vec())
    }

    const G: &str = "0123456789abcdef0123456789abcdef";
    const H: &str = "fedcba9876543210fedcba9876543210";

    #[test]
    fn a_double_slot_reads_as_printf_text() {
        let row = Row {
            rowid: 4,
            cells: vec![
                Cell::Integer(4),
                text(G),
                text("aa-double"),
                Cell::Integer(2),
                Cell::Integer(0),
                Cell::Null,
                Cell::Real(0.1),
                text("1970-01-01 00:00:00"),
                Cell::Null,
                Cell::Integer(0),
                Cell::Integer(1),
                Cell::Null,
            ],
        };
        let slot = slot(&row, &mut Vec::new()).unwrap();
        assert_eq!(slot.cell, SlotCell::Value(SlotValue::Double("0.100000000000000006".into())));
    }

    #[test]
    fn an_account_without_commodity_has_no_scu() {
        let row = Row {
            rowid: 1,
            cells: vec![
                text(G),
                text("Root Account"),
                text("ROOT"),
                Cell::Null,
                Cell::Integer(0),
                Cell::Integer(0),
                Cell::Null,
                text(""),
                text(""),
                Cell::Integer(0),
                Cell::Integer(0),
            ],
        };
        let mut findings = Vec::new();
        let a = account(&row, &Lookup::default(), &mut findings).unwrap();
        assert_eq!((a.account.commodity_scu, a.account.non_standard_scu, a.account.code), (None, false, None));
        assert!(findings.is_empty(), "{findings:?}");
    }

    #[test]
    fn a_transaction_needs_its_currency_row() {
        let row = Row {
            rowid: 3,
            cells: vec![text(G), text(H), text(""), text("20240603105900"), Cell::Null, Cell::Null],
        };
        let mut findings = Vec::new();
        assert!(transaction(&row, &Lookup::default(), &mut findings).is_none());
        assert_eq!(
            findings,
            [
                format!("transactions row 3, column currency_guid: commodity {H} has no row"),
                "transactions row 3: left out: a transaction with no currency".to_owned(),
            ]
        );
        let mut lookup = Lookup::default();
        lookup.commodities.insert(H.parse().unwrap(), CommodityRef::currency("USD"));
        let t = transaction(&row, &lookup, &mut Vec::new()).unwrap();
        assert_eq!((t.num, t.description.as_deref(), t.date_posted), (None, Some(""), Time64(1_717_412_340)));
    }
}
