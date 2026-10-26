//! CSV price import: a file of prices into the Book's price database, as GnuCash's price importer reads it
//! (`design/api/import.md`, "CSV prices").
//!
//! Translated from `5.17:gnucash/import-export/csv-imp/gnc-import-price.cpp` (`GncPriceImport`; Copyright (c) 2015
//! Geert Janssens, (c) 2017 Robert Fewell) and `GncImportPrice::create_price` in `gnc-imp-props-price.cpp:295-366`.
//! GPL-2.0-or-later; this translation is GPL-3.0-or-later. The tokenizer, skipping, date and currency formats are the
//! transaction importer's; the cells are parsed by [`props`].
//!
//! Each line becomes a price of the From commodity in the To currency, at the day-neutral time of its date, source
//! `user:price`, type `last`, and its amount converted to the To currency's fraction × 10000 rounding half up
//! (`COMMODITY_DENOM_MULT`, `5.17:libgnucash/engine/gnc-pricedb.h:286`). A price of the pair already on that day, in
//! either direction (`gnc_pricedb_lookup_day_t64`), is replaced when the user allows over-writing, and otherwise
//! the line is a duplicate and adds nothing. Lines run in file order, so a second line for the same day meets the
//! first's price. The new price is added through `price.add`'s code ([`add_price`]), so the database's own same-day
//! rules apply after GnuCash's.
//!
//! **Deviations** (`design/api/import.md`): a line failing its essentials (a short line, say), which GnuCash skips
//! with only a log line, is an error row; so is a line whose price `gnc_pricedb_add_price` would refuse after the old
//! one was removed (a same-day price from a better-ranked source), where GnuCash loses the old price; a layout with
//! both a default and the columns it stands for is refused; an unknown default commodity is an error, where GnuCash's
//! preset loader throws.

pub mod props;

use std::collections::BTreeMap;

use red_ale_core::ops::commit::commodity_fraction;
use red_ale_core::ops::prices::{Nearest, USER_PRICE_SOURCE, add_price, nearest};
use red_ale_core::{Book, CommodityRef, Date, DomainError, DomainResult, Env, FixedEnv, Guid, Numeric, Price, Round, Time64, TimeZone};

pub use props::{PriceColumn, PriceContext, PriceLine, has_namespace, parse_commodity};

use super::settings::PriceLayout;
use super::tokenizer::Row;
use crate::matcher::ImportAction;
use crate::pipeline::{Applied, Change, IgnoredLine, Proposal, RowError};

/// GnuCash's `PRICE_TYPE_LAST` (`5.17:libgnucash/engine/gnc-pricedb.h:184`).
pub const PRICE_TYPE_LAST: &str = "last";

/// `COMMODITY_DENOM_MULT` (`5.17:libgnucash/engine/gnc-pricedb.h:286`).
pub const COMMODITY_DENOM_MULT: i64 = 10_000;

/// The first GUID the preview's copy of the Book counts up from: far from any a Book holds.
const PREVIEW_GUID_SEED: u128 = 0x7265_642d_616c_6520_7072_6963_6573_0000;

/// A same-day price a proposed price meets: one in the Book, or one an earlier line of the file adds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Existing {
    /// A price in the Book.
    Book(Guid),
    /// The price of the proposal's price with this index.
    Index(usize),
}

/// One price of the proposal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProposedPrice {
    /// Its position in the proposal: the key `import.apply` changes it by.
    pub index: usize,
    /// Its line, from 1.
    pub line: usize,
    /// The commodity priced.
    pub commodity: CommodityRef,
    /// The currency it is priced in.
    pub currency: CommodityRef,
    /// The date.
    pub date: Date,
    /// The day-neutral time of the date, the price's time.
    pub time: Time64,
    /// The amount as the file writes it.
    pub amount: Numeric,
    /// The value stored: the amount at the currency's fraction × 10000, rounded half up.
    pub value: Numeric,
    /// `add`; `update` to replace the same-day price; `skip`, a duplicate of it.
    pub action: ImportAction,
    /// The same-day price it meets, in either direction.
    pub existing: Option<Existing>,
}

/// The parsed file: the layout in effect and every non-empty line's properties.
#[derive(Debug, Clone)]
pub struct PriceImport {
    layout: PriceLayout,
    columns: Vec<PriceColumn>,
    lines: Vec<(usize, PriceLine)>,
}

/// The layout's defaults, resolved: GnuCash's "Commodity From" and "Currency To" combos.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Defaults {
    /// The From commodity.
    pub from: Option<CommodityRef>,
    /// The To currency.
    pub to: Option<CommodityRef>,
}

impl Defaults {
    /// Resolve the layout's unique names as GnuCash's preset loader does (`parse_commodity_price_comm (str, "")`,
    /// `gnc-imp-settings-csv-price.cpp:131-145`); the To currency must be a currency, as its combo lists only those.
    pub fn resolve(book: &Book, layout: &PriceLayout) -> Result<Self, String> {
        let find = |name: &Option<String>, what: &str| {
            name.as_deref().map(|n| parse_commodity(book, n, "").map_err(|_| format!("the layout's {what} {n:?} is not a commodity of the book"))).transpose()
        };
        let from = find(&layout.from_commodity, "fromCommodity")?;
        let to = find(&layout.to_currency, "toCurrency")?;
        if let Some(to) = to.as_ref().filter(|c| !c.is_currency()) {
            return Err(format!("the layout's toCurrency {to} is not a currency"));
        }
        let has = |c: PriceColumn| layout.columns.contains(&c);
        if from.is_some() && (has(PriceColumn::FromSymbol) || has(PriceColumn::FromNamespace)) {
            return Err("fromCommodity and the From Symbol and From Namespace columns exclude each other, as GnuCash clears one when the other is chosen".to_owned());
        }
        if to.is_some() && has(PriceColumn::ToCurrency) {
            return Err("toCurrency and the Currency To column exclude each other, as GnuCash clears one when the other is chosen".to_owned());
        }
        Ok(Self { from, to })
    }
}

impl PriceImport {
    /// Parse every non-empty row (`GncPriceImport::tokenize`, `gnc-import-price.cpp:380-426`): the column types fitted
    /// to the widest row, each type kept at its first column only (`set_column_type_price`, `:733-744`), each cell set
    /// in column order, the layout's defaults standing in where GnuCash sets them (`update_price_props`, `:690-716`).
    pub fn parse(book: &Book, rows: Vec<Row>, layout: &PriceLayout, defaults: &Defaults, ctx: &PriceContext) -> Self {
        let max_cols = rows.iter().map(Vec::len).max().unwrap_or(0);
        let columns = effective_columns(&layout.columns, max_cols);
        let lines = rows
            .into_iter()
            .enumerate()
            .filter(|(_, cells)| !cells.is_empty())
            .map(|(row, cells)| (row, parse_line(book, &cells, &columns, defaults, ctx)))
            .collect();
        Self { layout: layout.clone(), columns, lines }
    }

    /// What GnuCash's assistant would not go on with (`GncPriceImport::verify`, `gnc-import-price.cpp:503-543`,
    /// without the line errors, which red-ale lists as error rows): no data, every line skipped, a required column
    /// missing (`verify_column_selections`, `:452-495`).
    pub fn file_errors(&self, defaults: &Defaults) -> Vec<String> {
        if self.lines.is_empty() {
            return vec!["No valid data found in the selected file. It may be empty or the selected encoding is wrong.".to_owned()];
        }
        let csv = &self.layout.csv;
        let skipped = u64::from(csv.skip_start_lines) + u64::from(csv.skip_end_lines) + u64::from(csv.skip_alt_lines);
        if usize::try_from(skipped).unwrap_or(usize::MAX) >= self.lines.len() {
            return vec!["No lines are selected for importing. Please reduce the number of lines to skip.".to_owned()];
        }
        let has = |c: PriceColumn| self.columns.contains(&c);
        [
            (!has(PriceColumn::Date)).then_some("Please select a date column."),
            (!has(PriceColumn::Amount)).then_some("Please select an amount column."),
            (!has(PriceColumn::ToCurrency) && defaults.to.is_none())
                .then_some("Please select a 'Currency to' column or set a Currency in the 'Currency To' field."),
            (!has(PriceColumn::FromSymbol) && defaults.from.is_none())
                .then_some("Please select a 'From Symbol' column or set a Commodity in the 'Commodity From' field."),
            (!has(PriceColumn::FromNamespace) && defaults.from.is_none())
                .then_some("Please select a 'From Namespace' column or set a Commodity in the 'Commodity From' field."),
            (defaults.from.is_some() && defaults.from == defaults.to).then_some("'Commodity From' can not be the same as 'Currency To'."),
        ]
        .into_iter()
        .flatten()
        .map(str::to_owned)
        .collect()
    }

    /// Whether the line at this position among the non-empty lines is skipped (`update_skipped_lines`, `:262-279`).
    fn skip_reason(&self, i: usize) -> Option<&'static str> {
        let csv = &self.layout.csv;
        let start = usize::try_from(csv.skip_start_lines).unwrap_or(usize::MAX);
        let end = usize::try_from(csv.skip_end_lines).unwrap_or(usize::MAX);
        if i < start {
            Some("skipStartLines")
        } else if end > 0 && i >= self.lines.len().saturating_sub(end) {
            Some("skipEndLines")
        } else if csv.skip_alt_lines && (i - start) % 2 == 1 {
            Some("skipAltLines")
        } else {
            None
        }
    }
}

/// The column types fitted to `max_cols`, each type at its first column only.
fn effective_columns(columns: &[PriceColumn], max_cols: usize) -> Vec<PriceColumn> {
    let mut out = columns.to_vec();
    out.resize(max_cols, PriceColumn::None);
    let mut seen = std::collections::BTreeSet::new();
    out.iter_mut().for_each(|c| {
        if *c != PriceColumn::None && !seen.insert(*c) {
            *c = PriceColumn::None;
        }
    });
    out
}

/// One line's properties: each column's cell in column order. A cell beyond the line's end leaves its property
/// unset, without an error (`GncImportPrice::reset`).
fn parse_line(book: &Book, cells: &[String], columns: &[PriceColumn], defaults: &Defaults, ctx: &PriceContext) -> PriceLine {
    let mut line = PriceLine::default();
    columns.iter().enumerate().filter(|(_, c)| **c != PriceColumn::None).for_each(|(i, &column)| {
        let Some(cell) = cells.get(i) else { return };
        if column == PriceColumn::ToCurrency && defaults.from.is_some() {
            line.from_commodity.clone_from(&defaults.from);
        }
        if column == PriceColumn::FromSymbol && defaults.to.is_some() {
            line.to_currency.clone_from(&defaults.to);
        }
        line.set(book, column, cell, ctx);
    });
    if line.to_currency.is_none() {
        line.to_currency.clone_from(&defaults.to);
    }
    if line.from_commodity.is_none() {
        line.from_commodity.clone_from(&defaults.from);
    }
    line
}

/// What one price line did to the price database.
enum Created {
    Added(Guid),
    Replaced { new: Guid, old: Guid },
    Duplicate(Guid),
}

/// A price ready to create.
struct Ready {
    commodity: CommodityRef,
    currency: CommodityRef,
    date: Date,
    time: Time64,
    amount: Numeric,
    value: Numeric,
}

/// The proposal's prices, error rows and skipped lines: every line run in order on a copy of `book`, with
/// `overwrite` as GnuCash's "Allow existing prices to be over written".
pub fn propose(book: &Book, zone: &TimeZone, import: &PriceImport, overwrite: bool, file: usize) -> (Vec<ProposedPrice>, Vec<RowError>, Vec<IgnoredLine>) {
    let mut copy = book.clone();
    let mut env = FixedEnv::new(Time64(0), PREVIEW_GUID_SEED).with_zone(zone.clone());
    let mut made: BTreeMap<Guid, usize> = BTreeMap::new();
    let (mut prices, mut errors, mut ignored) = (Vec::new(), Vec::new(), Vec::new());
    import.lines.iter().enumerate().for_each(|(i, (row, line))| {
        let line_no = row + 1;
        if let Some(reason) = import.skip_reason(i) {
            ignored.push(IgnoredLine { file, line: line_no, reason });
            return;
        }
        let ready = match ready(&copy, zone, &import.columns, line, file, line_no) {
            Ok(r) => r,
            Err(e) => return errors.extend(e),
        };
        let action = if overwrite { ImportAction::Update } else { ImportAction::Add };
        match create(&mut copy, &mut env, &ready, action == ImportAction::Update) {
            Ok(created) => {
                let index = prices.len();
                let (action, existing, new) = match created {
                    Created::Added(g) => (ImportAction::Add, None, Some(g)),
                    Created::Replaced { new, old } => (ImportAction::Update, Some(old), Some(new)),
                    Created::Duplicate(g) => (ImportAction::Skip, Some(g), None),
                };
                let existing = existing.map(|g| made.get(&g).map_or(Existing::Book(g), |i| Existing::Index(*i)));
                new.into_iter().for_each(|g| {
                    made.insert(g, index);
                });
                let Ready { commodity, currency, date, time, amount, value } = ready;
                prices.push(ProposedPrice { index, line: line_no, commodity, currency, date, time, amount, value, action, existing });
            }
            Err(message) => errors.push(RowError { file, line: line_no, column: None, message }),
        }
    });
    (prices, errors, ignored)
}

/// A line's price, or its error rows: GnuCash's line errors with their columns, then its essentials, then the value's
/// conversion.
fn ready(book: &Book, zone: &TimeZone, columns: &[PriceColumn], line: &PriceLine, file: usize, line_no: usize) -> Result<Ready, Vec<RowError>> {
    let column = |c: &PriceColumn| columns.iter().position(|k| k == c).map(|p| p + 1);
    if !line.errors().is_empty() {
        return Err(line.errors().iter().map(|(c, m)| RowError { file, line: line_no, column: column(c), message: m.clone() }).collect());
    }
    let one = |message: String| vec![RowError { file, line: line_no, column: None, message }];
    if let Some(message) = line.missing_essentials() {
        return Err(one(message.to_owned()));
    }
    let (Some(date), Some(amount), Some(currency), Some(commodity)) = (line.date, line.amount, line.to_currency.clone(), line.from_commodity.clone()) else {
        return Err(one("No date column.".to_owned()));
    };
    let value = price_value(book, &currency, amount).map_err(one)?;
    Ok(Ready { commodity, currency, date, time: zone.neutral_time(date), amount, value })
}

/// `amount.convert<RoundType::half_up>(scu * COMMODITY_DENOM_MULT)` (`gnc-imp-props-price.cpp:342-343`).
pub fn price_value(book: &Book, currency: &CommodityRef, amount: Numeric) -> Result<Numeric, String> {
    let scu = commodity_fraction(book, currency).ok_or_else(|| format!("{currency} has no fraction"))?;
    let denom = scu.checked_mul(COMMODITY_DENOM_MULT).ok_or_else(|| format!("{currency}'s fraction is too large"))?;
    amount.to_denom(denom, Round::HalfUp).map_err(|e| format!("the price {} can't be stored at {denom}ths: {e}", amount.to_decimal_string()))
}

/// `GncImportPrice::create_price` (`gnc-imp-props-price.cpp:295-366`): the same-day price of the pair in either
/// direction is removed when `overwrite`, else the line is a duplicate; then the new price is added. When the price
/// database then refuses it (a same-day price from a better-ranked source remains), the old price is put back and
/// the line fails, where GnuCash would have lost the old price.
fn create(book: &mut Book, env: &mut dyn Env, ready: &Ready, overwrite: bool) -> Result<Created, String> {
    let zone = env.zone().clone();
    let old = nearest(book, &ready.commodity, &ready.currency, ready.time, Nearest::SameDay, &zone).map(|p| p.guid);
    let removed = match (old, overwrite) {
        (Some(old), false) => return Ok(Created::Duplicate(old)),
        (Some(old), true) => book.prices.shift_remove_full(&old),
        (None, _) => None,
    };
    let price = Price {
        guid: env.new_guid(),
        commodity: ready.commodity.clone(),
        currency: ready.currency.clone(),
        time: ready.time,
        source: Some(USER_PRICE_SOURCE.to_owned()),
        price_type: Some(PRICE_TYPE_LAST.to_owned()),
        value: ready.value,
    };
    let result = add_price(book, price, &zone).map_err(|e| e.to_string())?;
    match (result.added, result.rejected_by, result.duplicate_of) {
        (Some(new), ..) if removed.is_some() => Ok(Created::Replaced { new, old: old.unwrap_or(new) }),
        (Some(new), ..) => Ok(Created::Added(new)),
        (None, None, Some(same)) => Ok(Created::Duplicate(same)),
        (None, rejected, _) => {
            if let Some((i, g, p)) = removed {
                book.prices.shift_insert(i, g, p);
            }
            let by = rejected.map(|r| format!(": price {r} of that day has a source that ranks higher")).unwrap_or_default();
            Err(format!("Failed to create price from selected columns{by}."))
        }
    }
}

/// Apply a CSV price proposal with the caller's changes: any price may become `add` (added unless a same-day price
/// is there by then), `update` (replacing a same-day price) or `skip`. Prices run in order.
pub fn apply(book: &mut Book, env: &mut dyn Env, proposal: &Proposal, changes: &[Change]) -> DomainResult<Applied> {
    let actions = actions(proposal, changes)?;
    let mut applied = Applied::default();
    proposal.prices.iter().zip(actions).try_for_each(|(p, action)| {
        if action == ImportAction::Skip {
            applied.skipped.push(p.index);
            return Ok(());
        }
        let ready = Ready { commodity: p.commodity.clone(), currency: p.currency.clone(), date: p.date, time: p.time, amount: p.amount, value: p.value };
        match create(book, env, &ready, action == ImportAction::Update).map_err(|e| DomainError::invalid(format!("line {}: {e}", p.line)))? {
            Created::Added(g) => applied.prices.0.push(g),
            Created::Replaced { new, .. } => applied.prices.1.push(new),
            Created::Duplicate(g) => applied.prices.2.push(g),
        }
        Ok::<(), DomainError>(())
    })?;
    Ok(applied)
}

/// Each price's action after the caller's changes.
fn actions(proposal: &Proposal, changes: &[Change]) -> DomainResult<Vec<ImportAction>> {
    let mut actions: Vec<ImportAction> = proposal.prices.iter().map(|p| p.action).collect();
    changes.iter().try_for_each(|c| {
        let slot = actions.get_mut(c.index).ok_or_else(|| DomainError::invalid(format!("no price {} in the proposal", c.index)))?;
        if c.match_split.is_some() || c.account.is_some() || c.amount.is_some() {
            return Err(DomainError::invalid(format!("price {}: only its action can change", c.index)));
        }
        match c.action {
            Some(ImportAction::Clear) => Err(DomainError::invalid(format!("price {}: clear is for transactions; a price is add, update or skip", c.index))),
            Some(a) => {
                *slot = a;
                Ok(())
            }
            None => Ok(()),
        }
    })?;
    Ok(actions)
}

#[cfg(test)]
mod tests;
