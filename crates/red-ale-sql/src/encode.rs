//! Row encoders: one entity of the core model into the cells GnuCash 5.17 writes for it, column by column in the
//! order [`crate::tables`] lists them (`design/sql-fidelity.md` §4, "Which rows an entity writes"). The inverse of
//! [`crate::rows`]: decoding what an encoder gives yields the entity's stored form.
//!
//! Pure, so they can be checked against every row GnuCash wrote in the fixtures without a database. Tables whose
//! first column is an `AUTOINCREMENT` `id` get no cell for it: SQLite assigns it.
//!
//! Values are written as GnuCash writes them (`5.17:libgnucash/backend/sql/gnc-sql-column-table-entry.cpp`):
//! timestamps `YYYY-MM-DD HH:MM:SS` in UTC, or NULL outside `(MINTIME, MAXTIME)` (`:444-455`); dates `YYYYMMDD`, or
//! NULL when unset (`:512-529`); numerics unreduced (`:593-634`); booleans 0 or 1; an unset string in a `NOT NULL`
//! column as the engine's `""`.

pub mod business;
pub mod slots;

use indexmap::IndexMap;
use red_ale_core::{
    Account, Budget, Commodity, CommodityRef, Date, Guid, Lot, Numeric, Price, Recurrence, ScheduledTransaction, SlotValue, Split, Time64,
    Transaction,
};

use crate::cell::Cell;

/// GnuCash's `MINTIME` and `MAXTIME`: a time outside the open interval between them is written as NULL
/// (`5.17:libgnucash/engine/gnc-datetime.hpp:55-56`).
pub const MINTIME: i64 = -17_987_443_200;
/// See [`MINTIME`].
pub const MAXTIME: i64 = 253_402_214_400;

/// Each commodity's row GUID, for the columns that refer to commodities.
#[derive(Debug, Clone, Copy)]
pub struct Refs<'a> {
    /// Commodity row GUIDs by reference.
    pub commodities: &'a IndexMap<CommodityRef, Guid>,
}

impl Refs<'_> {
    /// The cell referring to a commodity: its row's GUID, or NULL when it has none.
    pub fn commodity(&self, c: Option<&CommodityRef>) -> Cell {
        opt_guid(c.and_then(|c| self.commodities.get(c)).copied())
    }
}

/// A text cell.
pub fn text(s: &str) -> Cell {
    Cell::Text(s.as_bytes().to_vec())
}

/// A text cell, NULL for `None`.
pub fn opt_text(s: Option<&str>) -> Cell {
    s.map_or(Cell::Null, text)
}

/// A text cell for a column GnuCash always fills: `None` is the engine's `""`.
pub fn req_text(s: Option<&str>) -> Cell {
    text(s.unwrap_or_default())
}

/// An integer cell.
pub fn int(i: impl Into<i64>) -> Cell {
    Cell::Integer(i.into())
}

/// A boolean cell: 0 or 1.
pub fn boolean(b: bool) -> Cell {
    Cell::Integer(i64::from(b))
}

/// A GUID cell: 32 lower-case hex digits, as `guid_to_string` writes them.
pub fn guid(g: Guid) -> Cell {
    text(&g.to_string())
}

/// A GUID cell, NULL for `None`.
pub fn opt_guid(g: Option<Guid>) -> Cell {
    g.map_or(Cell::Null, guid)
}

/// The two cells of a numeric, unreduced.
pub fn numeric(n: Numeric) -> [Cell; 2] {
    [Cell::Integer(n.num()), Cell::Integer(n.denom())]
}

/// The two cells of a numeric, NULL for `None`.
pub fn opt_numeric(n: Option<Numeric>) -> [Cell; 2] {
    n.map_or([Cell::Null, Cell::Null], numeric)
}

/// True when GnuCash can write the time: inside `(MINTIME, MAXTIME)`.
pub fn time_in_range(t: Time64) -> bool {
    t.0 > MINTIME && t.0 < MAXTIME
}

/// A timestamp cell, `YYYY-MM-DD HH:MM:SS` in UTC, NULL outside GnuCash's range.
pub fn time(t: Time64) -> Cell {
    if !time_in_range(t) {
        return Cell::Null;
    }
    let iso = t.to_file_string();
    iso.get(..19).map_or(Cell::Null, text)
}

/// A timestamp cell, NULL for `None`.
pub fn opt_time(t: Option<Time64>) -> Cell {
    t.map_or(Cell::Null, time)
}

/// A date cell, `YYYYMMDD`, NULL when unset.
pub fn date(d: Option<Date>) -> Cell {
    d.map_or(Cell::Null, |d| text(&format!("{:04}{:02}{:02}", d.year(), d.month(), d.day())))
}

/// True when an account Slot says yes, as `xaccAccountGetHidden` and `GetPlaceholder` read it: the string `true` or a
/// non-zero integer (`5.17:libgnucash/engine/Account.cpp:2563-2577`).
fn slot_truth(a: &Account, key: &str) -> bool {
    match a.slots.get_key(key) {
        Some(SlotValue::String(s)) => s == "true",
        Some(SlotValue::Integer(i)) => *i != 0,
        _ => false,
    }
}

/// A `commodities` row. Without quotes, the engine's quote time zone is `""`, and its source is unset but for a
/// currency's, which is always `currency` (`5.17:libgnucash/backend/sql/gnc-commodity-sql.cpp:57-79`).
pub fn commodity(row: Guid, c: &Commodity) -> Vec<Cell> {
    let source = c.quote_source.as_deref().or(c.id.is_currency().then_some("currency"));
    vec![
        guid(row),
        text(&c.id.namespace),
        text(&c.id.mnemonic),
        opt_text(c.fullname.as_deref()),
        opt_text(c.cusip.as_deref()),
        int(c.fraction),
        boolean(c.quote_flag),
        opt_text(source),
        req_text(c.quote_tz.as_deref()),
    ]
}

/// An `accounts` row. `hidden` and `placeholder` repeat the account's Slots (`5.17:libgnucash/engine/Account.cpp:116-117`).
pub fn account(a: &Account, refs: Refs<'_>) -> Vec<Cell> {
    vec![
        guid(a.guid),
        text(&a.name),
        text(a.account_type.as_str()),
        refs.commodity(a.commodity.as_ref()),
        int(a.commodity_scu.unwrap_or(0)),
        boolean(a.non_standard_scu),
        opt_guid(a.parent),
        req_text(a.code.as_deref()),
        req_text(a.description.as_deref()),
        boolean(slot_truth(a, "hidden")),
        boolean(slot_truth(a, "placeholder")),
    ]
}

/// A `lots` row: `is_closed` is −1, unknown, so GnuCash recomputes it from the splits, as it does after its own split
/// changes (`5.17:libgnucash/engine/gnc-lot.cpp:620-623`).
pub fn lot(l: &Lot) -> Vec<Cell> {
    vec![guid(l.guid), guid(l.account), int(-1)]
}

/// A `transactions` row.
pub fn transaction(t: &Transaction, refs: Refs<'_>) -> Vec<Cell> {
    vec![
        guid(t.guid),
        refs.commodity(Some(&t.currency)),
        req_text(t.num.as_deref()),
        time(t.date_posted),
        time(t.date_entered),
        req_text(t.description.as_deref()),
    ]
}

/// A `splits` row of transaction `tx`. A split with no account is written with the null GUID, which reads back as
/// none; an unset reconcile date is time 0, which GnuCash writes as `1970-01-01 00:00:00`.
pub fn split(tx: Guid, s: &Split) -> Vec<Cell> {
    let [value_num, value_denom] = numeric(s.value);
    let [quantity_num, quantity_denom] = numeric(s.amount);
    vec![
        guid(s.guid),
        guid(tx),
        guid(s.account.unwrap_or(Guid::NULL)),
        req_text(s.memo.as_deref()),
        req_text(s.action.as_deref()),
        text(&s.reconcile_state.as_char().to_string()),
        time(s.reconcile_date.unwrap_or_default()),
        value_num,
        value_denom,
        quantity_num,
        quantity_denom,
        opt_guid(s.lot),
    ]
}

/// A `prices` row.
pub fn price(p: &Price, refs: Refs<'_>) -> Vec<Cell> {
    let [value_num, value_denom] = numeric(p.value);
    vec![
        guid(p.guid),
        refs.commodity(Some(&p.commodity)),
        refs.commodity(Some(&p.currency)),
        time(p.time),
        opt_text(p.source.as_deref()),
        opt_text(p.price_type.as_deref()),
        value_num,
        value_denom,
    ]
}

/// A `budgets` row.
pub fn budget(b: &Budget) -> Vec<Cell> {
    vec![guid(b.guid), text(&b.name), text(&b.description), int(b.num_periods)]
}

/// A `budget_amounts` row, without its `id`.
pub fn budget_amount(budget: Guid, account: Guid, period: u32, amount: Numeric) -> Vec<Cell> {
    let [num, denom] = numeric(amount);
    vec![guid(budget), guid(account), int(period), num, denom]
}

/// A `recurrences` row of `owner`, without its `id`.
pub fn recurrence(owner: Guid, r: &Recurrence) -> Vec<Cell> {
    vec![
        guid(owner),
        int(r.mult()),
        text(r.period_type().as_str()),
        date(Some(r.start())),
        text(r.weekend_adjust().as_str()),
    ]
}

/// A `schedxactions` row. An occurrence count is written with no end date, as the engine holds one or the other.
pub fn scheduled(sx: &ScheduledTransaction) -> Vec<Cell> {
    let (total, remaining) = sx.occurrences.map_or((0, 0), |o| (o.total, o.remaining));
    vec![
        guid(sx.guid),
        text(&sx.name),
        boolean(sx.enabled),
        date(Some(sx.start)),
        date(sx.end),
        date(sx.last),
        int(total),
        int(remaining),
        boolean(sx.auto_create),
        boolean(sx.auto_create_notify),
        int(sx.advance_create_days),
        int(sx.advance_remind_days),
        int(sx.instance_count),
        guid(sx.template_account.unwrap_or(Guid::NULL)),
    ]
}

/// The `books` row.
pub fn book(book: Guid, root: Guid, template_root: Guid) -> Vec<Cell> {
    vec![guid(book), guid(root), guid(template_root)]
}
