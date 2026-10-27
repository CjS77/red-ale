//! Column types and the properties one line of a transaction import holds.
//!
//! Translated from `5.17:gnucash/import-export/csv-imp/gnc-imp-props-tx.cpp` and `gnc-imp-props-tx.hpp` (no
//! copyright line in either; GPL-2.0-or-later, as their headers say). This translation is GPL-3.0-or-later.
//!
//! GnuCash parses each cell into a `GncPreTrans` (the transaction's properties) or a `GncPreSplit` (the split's) as
//! soon as a column gets a type, recording one error per property. Two kinds of cell need the Book, which this crate
//! does not have, so they are kept as written for the import pipeline:
//!
//! - **Account and Transfer Account**: GnuCash looks the text up in the `csv-account-map` import map, then as an
//!   account full name, and records "Account value can't be mapped back to an account." when both fail
//!   (`gnc-imp-props-tx.cpp:448-469`). Only the empty-cell error is recorded here.
//! - **Transaction Commodity**: GnuCash looks the text up as a unique name `NAMESPACE::MNEMONIC`, then as a currency
//!   code, then as a mnemonic in each other namespace (`parse_commodity`, `:188-226`), and records "Value can't be
//!   parsed into a valid commodity." when all fail. Here the text is kept.
//!
//! The pipeline adds those errors with [`PreTrans::add_error`] and [`PreSplit::add_error`] before drafts are made,
//! so a line it can't resolve is reported and skipped as GnuCash would.

use std::collections::BTreeMap;

use red_ale_core::{Date, Numeric, NumericError, ReconcileState};
use serde::{Deserialize, Serialize};

use super::date::{DateContext, DateFormat, parse_date};
use super::monetary::{CurrencyFormat, MonetaryLocale, parse_monetary};

/// What a column holds: GnuCash's `GncTransPropType` (`gnc-imp-props-tx.hpp:49-78`), in its order. The derived
/// ordering is that order, which sorts errors as GnuCash's `ErrMap` does.
///
/// In JSON a column type is GnuCash's saved name ([`ColumnType::gnucash_name`]), and the legacy names are read too.
/// GnuCash loads an unknown name as `None` with a warning; red-ale refuses it, so a typo in a layout is reported.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize)]
#[serde(try_from = "String", into = "&'static str")]
pub enum ColumnType {
    /// Ignored.
    #[default]
    None,
    /// Groups the lines of one transaction in multi-split mode ("Transaction ID").
    UniqueId,
    /// The posted date.
    Date,
    /// The transaction number ("Number").
    Num,
    /// The description.
    Description,
    /// The transaction notes.
    Notes,
    /// The transaction's currency or commodity ("Transaction Commodity").
    Commodity,
    /// The reason the transaction was voided.
    VoidReason,
    /// The split's action.
    Action,
    /// The split's account.
    Account,
    /// The split's amount, in the account's commodity; may be given in several columns, which are added.
    Amount,
    /// The negated amount ("Amount (Negated)"), for example a withdrawals column.
    AmountNeg,
    /// The split's value, in the transaction's currency.
    Value,
    /// The negated value ("Value (Negated)").
    ValueNeg,
    /// The price: value per unit of amount.
    Price,
    /// The split's memo.
    Memo,
    /// The split's reconcile state ("Reconciled").
    RecState,
    /// The split's reconcile date ("Reconcile Date").
    RecDate,
    /// The transfer split's action (two-split mode).
    TransferAction,
    /// The transfer split's account (two-split mode).
    TransferAccount,
    /// The transfer split's amount (two-split mode).
    TransferAmount,
    /// The transfer split's negated amount (two-split mode).
    TransferAmountNeg,
    /// The transfer split's memo (two-split mode).
    TransferMemo,
    /// The transfer split's reconcile state (two-split mode).
    TransferRecState,
    /// The transfer split's reconcile date (two-split mode).
    TransferRecDate,
}

impl ColumnType {
    /// Every column type, in GnuCash's order.
    pub const ALL: [Self; 25] = [
        Self::None,
        Self::UniqueId,
        Self::Date,
        Self::Num,
        Self::Description,
        Self::Notes,
        Self::Commodity,
        Self::VoidReason,
        Self::Action,
        Self::Account,
        Self::Amount,
        Self::AmountNeg,
        Self::Value,
        Self::ValueNeg,
        Self::Price,
        Self::Memo,
        Self::RecState,
        Self::RecDate,
        Self::TransferAction,
        Self::TransferAccount,
        Self::TransferAmount,
        Self::TransferAmountNeg,
        Self::TransferMemo,
        Self::TransferRecState,
        Self::TransferRecDate,
    ];

    /// The English name GnuCash shows and saves in presets (`gnc_csv_col_type_strs`, `gnc-imp-props-tx.cpp:57-83`).
    pub fn gnucash_name(self) -> &'static str {
        match self {
            Self::None => "None",
            Self::UniqueId => "Transaction ID",
            Self::Date => "Date",
            Self::Num => "Number",
            Self::Description => "Description",
            Self::Notes => "Notes",
            Self::Commodity => "Transaction Commodity",
            Self::VoidReason => "Void Reason",
            Self::Action => "Action",
            Self::Account => "Account",
            Self::Amount => "Amount",
            Self::AmountNeg => "Amount (Negated)",
            Self::Value => "Value",
            Self::ValueNeg => "Value (Negated)",
            Self::Price => "Price",
            Self::Memo => "Memo",
            Self::RecState => "Reconciled",
            Self::RecDate => "Reconcile Date",
            Self::TransferAction => "Transfer Action",
            Self::TransferAccount => "Transfer Account",
            Self::TransferAmount => "Transfer Amount",
            Self::TransferAmountNeg => "Transfer Amount (Negated)",
            Self::TransferMemo => "Transfer Memo",
            Self::TransferRecState => "Transfer Reconciled",
            Self::TransferRecDate => "Transfer Reconcile Date",
        }
    }

    /// The column type a GnuCash preset names, with the legacy names `Deposit`, `Withdrawal` and `Num`
    /// (`gnc-imp-settings-csv-tx.cpp:255-264`). GnuCash loads an unknown name as `None`.
    pub fn from_gnucash_name(name: &str) -> Option<Self> {
        let name = match name {
            "Deposit" => "Amount",
            "Withdrawal" => "Amount (Negated)",
            "Num" => "Number",
            other => other,
        };
        Self::ALL.into_iter().find(|t| t.gnucash_name() == name)
    }

    /// A transaction property: after `None`, up to `VoidReason` (`TRANS_PROPS`).
    pub fn is_trans_prop(self) -> bool {
        self > Self::None && self <= Self::VoidReason
    }

    /// A split property: after `VoidReason`, up to `TransferRecDate` (`SPLIT_PROPS`).
    pub fn is_split_prop(self) -> bool {
        self > Self::VoidReason
    }

    /// May be given to several columns, whose values are added (`multi_col_props`, `gnc-imp-props-tx.cpp:101-114`).
    pub fn is_multi_col(self) -> bool {
        matches!(
            self,
            Self::Amount | Self::AmountNeg | Self::TransferAmount | Self::TransferAmountNeg | Self::Value | Self::ValueNeg
        )
    }

    /// `None` for a type the mode doesn't allow: the transaction ID in two-split mode, the transfer columns in
    /// multi-split mode (`sanitize_trans_prop`, `gnc-imp-props-tx.cpp:85-123`).
    #[must_use]
    pub fn sanitize(self, multi_split: bool) -> Self {
        let banned = if multi_split {
            matches!(
                self,
                Self::TransferAction
                    | Self::TransferAccount
                    | Self::TransferAmount
                    | Self::TransferAmountNeg
                    | Self::TransferMemo
                    | Self::TransferRecState
                    | Self::TransferRecDate
            )
        } else {
            self == Self::UniqueId
        };
        if banned { Self::None } else { self }
    }
}

impl From<ColumnType> for &'static str {
    fn from(t: ColumnType) -> Self {
        t.gnucash_name()
    }
}

impl TryFrom<String> for ColumnType {
    type Error = String;

    fn try_from(name: String) -> Result<Self, Self::Error> {
        Self::from_gnucash_name(&name).ok_or_else(|| format!("unknown CSV column type {name:?}"))
    }
}

/// Errors per property, ordered by column type as GnuCash's `ErrMap`. Each message is `"<column name>: <reason>"`.
pub type ErrMap = BTreeMap<ColumnType, String>;

/// What parsing a cell needs besides the cell.
#[derive(Debug)]
pub struct ParseContext<'a> {
    /// For dates.
    pub dates: DateContext<'a>,
    /// For amounts in the `Locale` currency format.
    pub monetary: MonetaryLocale,
}

impl ParseContext<'_> {
    /// Today's date, the C locale's number format and no locale date parser.
    pub fn new(today: Date) -> Self {
        Self {
            dates: DateContext { today, locale_date: None },
            monetary: MonetaryLocale::default(),
        }
    }
}

/// The formats the cells are read with.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Formats {
    pub date: DateFormat,
    pub currency: CurrencyFormat,
    pub multi_split: bool,
}

/// The `"{1}: {2}"` GnuCash puts in front of every property error (`gnc-imp-props-tx.cpp:294-297`).
fn prop_error(prop: ColumnType, reason: &str) -> String {
    format!("{}: {reason}", prop.gnucash_name())
}

fn non_empty(value: &str) -> Option<String> {
    (!value.is_empty()).then(|| value.to_owned())
}

/// The transaction properties of one line: GnuCash's `GncPreTrans` (`gnc-imp-props-tx.hpp:154-219`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PreTrans {
    /// Transaction ID (`m_differ`).
    pub unique_id: Option<String>,
    /// Posted date.
    pub date: Option<Date>,
    /// Number.
    pub num: Option<String>,
    /// Description.
    pub description: Option<String>,
    /// Notes.
    pub notes: Option<String>,
    /// Transaction commodity, as written; GnuCash holds the commodity it resolves to (see the module notes).
    pub commodity: Option<String>,
    /// Void reason.
    pub void_reason: Option<String>,
    errors: ErrMap,
}

impl PreTrans {
    /// `GncPreTrans::set` (`gnc-imp-props-tx.cpp:228-300`): parse one cell into the property, replacing any
    /// earlier value and error for it.
    pub(crate) fn set(&mut self, prop: ColumnType, value: &str, formats: Formats, ctx: &ParseContext) {
        self.errors.remove(&prop);
        let text = non_empty(value);
        let result = match prop {
            ColumnType::Date => self.set_date(value, formats, ctx),
            ColumnType::Description => self.set_description(value, formats.multi_split),
            ColumnType::UniqueId => {
                self.unique_id = text;
                Ok(())
            }
            ColumnType::Num => {
                self.num = text;
                Ok(())
            }
            ColumnType::Notes => {
                self.notes = text;
                Ok(())
            }
            ColumnType::Commodity => {
                self.commodity = text;
                Ok(())
            }
            ColumnType::VoidReason => {
                self.void_reason = text;
                Ok(())
            }
            _ => Ok(()),
        };
        if let Err(reason) = result {
            self.errors.insert(prop, prop_error(prop, &reason));
        }
    }

    fn set_date(&mut self, value: &str, formats: Formats, ctx: &ParseContext) -> Result<(), String> {
        self.date = None;
        if !value.is_empty() {
            self.date = Some(parse_date(value, formats.date, &ctx.dates).map_err(|e| e.to_string())?);
        } else if !formats.multi_split {
            return Err("Date field can not be empty if 'Multi-split' option is unset.\n".to_owned());
        }
        Ok(())
    }

    fn set_description(&mut self, value: &str, multi_split: bool) -> Result<(), String> {
        self.description = non_empty(value);
        if value.is_empty() && !multi_split {
            return Err("Description field can not be empty if 'Multi-split' option is unset.\n".to_owned());
        }
        Ok(())
    }

    /// Record an error the pipeline found for a property, such as a commodity it can't resolve. The message is
    /// prefixed with the column name as GnuCash's are; the property is cleared, as GnuCash leaves it unset.
    pub fn add_error(&mut self, prop: ColumnType, reason: &str) {
        if prop == ColumnType::Commodity {
            self.commodity = None;
        }
        self.errors.insert(prop, prop_error(prop, reason));
    }

    /// The errors, by property.
    pub fn errors(&self) -> &ErrMap {
        &self.errors
    }

    /// `GncPreTrans::is_part_of` (`gnc-imp-props-tx.cpp:364-377`): whether this line continues the transaction
    /// `parent` started. Every property this line has must equal the parent's, and the parent must have no errors.
    /// Not symmetrical: a line with no transaction properties is part of any parent.
    pub fn is_part_of(&self, parent: &Self) -> bool {
        fn fits<T: PartialEq>(own: Option<&T>, parent: Option<&T>) -> bool {
            own.is_none() || own == parent
        }
        fits(self.unique_id.as_ref(), parent.unique_id.as_ref())
            && fits(self.date.as_ref(), parent.date.as_ref())
            && fits(self.num.as_ref(), parent.num.as_ref())
            && fits(self.description.as_ref(), parent.description.as_ref())
            && fits(self.notes.as_ref(), parent.notes.as_ref())
            && fits(self.commodity.as_ref(), parent.commodity.as_ref())
            && fits(self.void_reason.as_ref(), parent.void_reason.as_ref())
            && parent.errors.is_empty()
    }

    /// `GncPreTrans::verify_essentials` (`gnc-imp-props-tx.cpp:310-321`): a transaction needs a date and a
    /// description.
    pub fn has_essentials(&self) -> bool {
        self.date.is_some() && self.description.is_some()
    }
}

/// Where a split goes, before the pipeline resolves it to an account.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SplitAccount {
    /// The text of the Account column.
    Named(String),
    /// The layout's base account.
    Base,
}

/// The split properties of one line: GnuCash's `GncPreSplit` (`gnc-imp-props-tx.hpp:221-266`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PreSplit {
    /// Action.
    pub action: Option<String>,
    /// Account.
    pub account: Option<SplitAccount>,
    /// Sum of the Amount columns.
    pub amount: Option<Numeric>,
    /// Sum of the Amount (Negated) columns.
    pub amount_neg: Option<Numeric>,
    /// Sum of the Value columns.
    pub value: Option<Numeric>,
    /// Sum of the Value (Negated) columns.
    pub value_neg: Option<Numeric>,
    /// Price.
    pub price: Option<Numeric>,
    /// Memo.
    pub memo: Option<String>,
    /// Reconcile state.
    pub rec_state: Option<ReconcileState>,
    /// Reconcile date.
    pub rec_date: Option<Date>,
    /// Transfer action.
    pub transfer_action: Option<String>,
    /// Transfer account, as written.
    pub transfer_account: Option<String>,
    /// Sum of the Transfer Amount columns.
    pub transfer_amount: Option<Numeric>,
    /// Sum of the Transfer Amount (Negated) columns.
    pub transfer_amount_neg: Option<Numeric>,
    /// Transfer memo.
    pub transfer_memo: Option<String>,
    /// Transfer reconcile state.
    pub transfer_rec_state: Option<ReconcileState>,
    /// Transfer reconcile date.
    pub transfer_rec_date: Option<Date>,
    errors: ErrMap,
}

impl PreSplit {
    /// `GncPreSplit::set` (`gnc-imp-props-tx.cpp:426-562`) for the single-column properties, and the reset that
    /// starts a multi-column sum (`GncPreSplit::reset`, `:564-570`: an empty cell, whose error is dropped).
    pub(crate) fn set(&mut self, prop: ColumnType, value: &str, formats: Formats, ctx: &ParseContext) {
        self.errors.remove(&prop);
        self.clear(prop);
        if let Err(reason) = self.set_inner(prop, value, formats, ctx) {
            self.errors.insert(prop, prop_error(prop, &reason));
        }
    }

    fn set_inner(&mut self, prop: ColumnType, value: &str, formats: Formats, ctx: &ParseContext) -> Result<(), String> {
        let money = |v: &str| parse_monetary(v, formats.currency, &ctx.monetary).map_err(|e| e.to_string());
        let date = |v: &str| {
            (!v.is_empty())
                .then(|| parse_date(v, formats.date, &ctx.dates).map_err(|e| e.to_string()))
                .transpose()
        };
        match prop {
            ColumnType::Action => self.action = non_empty(value),
            ColumnType::TransferAction => self.transfer_action = non_empty(value),
            ColumnType::Account => self.account = Some(SplitAccount::Named(account_text(value, "Account")?)),
            ColumnType::TransferAccount => self.transfer_account = Some(account_text(value, "Transfer account")?),
            ColumnType::Memo => self.memo = non_empty(value),
            ColumnType::TransferMemo => self.transfer_memo = non_empty(value),
            ColumnType::Price => self.price = Some(money(value)?),
            ColumnType::RecState => self.rec_state = Some(parse_reconciled(value)?),
            ColumnType::TransferRecState => self.transfer_rec_state = Some(parse_reconciled(value)?),
            ColumnType::RecDate => self.rec_date = date(value)?,
            ColumnType::TransferRecDate => self.transfer_rec_date = date(value)?,
            prop if prop.is_multi_col() => *self.sum_slot(prop) = Some(money(value)?),
            _ => {}
        }
        Ok(())
    }

    /// Clear a property, as GnuCash's `reset` does by setting it from an empty cell; a multi-column sum restarts
    /// from zero.
    pub(crate) fn reset(&mut self, prop: ColumnType, formats: Formats, ctx: &ParseContext) {
        self.set(prop, "", formats, ctx);
        self.errors.remove(&prop);
    }

    /// GnuCash resets each property before parsing, so a failed parse leaves it unset.
    fn clear(&mut self, prop: ColumnType) {
        match prop {
            ColumnType::Account => self.account = None,
            ColumnType::TransferAccount => self.transfer_account = None,
            ColumnType::Price => self.price = None,
            ColumnType::RecState => self.rec_state = None,
            ColumnType::TransferRecState => self.transfer_rec_state = None,
            ColumnType::RecDate => self.rec_date = None,
            ColumnType::TransferRecDate => self.transfer_rec_date = None,
            prop if prop.is_multi_col() => *self.sum_slot(prop) = None,
            _ => {}
        }
    }

    /// `GncPreSplit::add` (`gnc-imp-props-tx.cpp:572-638`): add one more column to a multi-column sum. Nothing is
    /// added once the property has an error.
    pub(crate) fn add(&mut self, prop: ColumnType, value: &str, formats: Formats, ctx: &ParseContext) {
        if self.errors.contains_key(&prop) || !prop.is_multi_col() {
            return;
        }
        let parsed = parse_monetary(value, formats.currency, &ctx.monetary).map_err(|e| e.to_string());
        let summed = parsed.and_then(|n| {
            let slot = *self.sum_slot(prop);
            slot.map_or(Ok(n), |existing| gnc_add(n, existing).map_err(|e| e.to_string()))
        });
        match summed {
            Ok(total) => *self.sum_slot(prop) = Some(total),
            Err(reason) => {
                self.errors.insert(prop, prop_error(prop, &reason));
            }
        }
    }

    fn sum_slot(&mut self, prop: ColumnType) -> &mut Option<Numeric> {
        match prop {
            ColumnType::Amount => &mut self.amount,
            ColumnType::AmountNeg => &mut self.amount_neg,
            ColumnType::Value => &mut self.value,
            ColumnType::ValueNeg => &mut self.value_neg,
            ColumnType::TransferAmount => &mut self.transfer_amount,
            _ => &mut self.transfer_amount_neg,
        }
    }

    /// Record an error the pipeline found for a property, such as an account it can't resolve
    /// ("Account value can't be mapped back to an account."). The property is cleared, as GnuCash leaves it unset.
    pub fn add_error(&mut self, prop: ColumnType, reason: &str) {
        self.clear(prop);
        self.errors.insert(prop, prop_error(prop, reason));
    }

    /// The errors, by property.
    pub fn errors(&self) -> &ErrMap {
        &self.errors
    }

    /// Record as line errors the reconcile-date essentials GnuCash checks only when it builds the split, and then
    /// drops the split silently (`gnc-imp-props-tx.cpp:647-651`, `:720-728`). red-ale makes such a line an error row
    /// instead (`design/api/import.md`, "Deviations"). An existing error on the date column is kept.
    pub(crate) fn flag_missing_reconcile_dates(&mut self) {
        let checks = [
            (
                self.rec_state,
                self.rec_date,
                ColumnType::RecDate,
                "Split is reconciled but reconcile date column is missing or invalid.",
            ),
            (
                self.transfer_rec_state,
                self.transfer_rec_date,
                ColumnType::TransferRecDate,
                "Transfer split is reconciled but transfer reconcile date column is missing or invalid.",
            ),
        ];
        checks
            .into_iter()
            .filter(|(state, date, _, _)| *state == Some(ReconcileState::Reconciled) && date.is_none())
            .for_each(|(_, _, prop, reason)| {
                self.errors.entry(prop).or_insert_with(|| prop_error(prop, reason));
            });
    }

    /// `GncPreSplit::verify_essentials` (`gnc-imp-props-tx.cpp:640-673`) without its multi-currency check, which
    /// needs the accounts' commodities: the reasons GnuCash would refuse to create this line's split.
    pub fn missing_essentials(&self) -> Vec<&'static str> {
        let mut missing = Vec::new();
        if self.amount.is_none() && self.amount_neg.is_none() {
            missing.push("No amount or negated amount column.");
        }
        if self.rec_state == Some(ReconcileState::Reconciled) && self.rec_date.is_none() {
            missing.push("Split is reconciled but reconcile date column is missing or invalid.");
        }
        if self.transfer_rec_state == Some(ReconcileState::Reconciled) && self.transfer_rec_date.is_none() {
            missing.push("Transfer split is reconciled but transfer reconcile date column is missing or invalid.");
        }
        missing
    }
}

/// An account cell must not be empty (`gnc-imp-props-tx.cpp:449-451`, `:461-462`).
fn account_text(value: &str, what: &str) -> Result<String, String> {
    non_empty(value).ok_or_else(|| format!("{what} value can't be empty."))
}

/// `parse_reconciled` (`gnc-imp-props-tx.cpp:172-186`) with GnuCash's English flags. `v` (voided) reads as not
/// reconciled, since voiding is done at the transaction level; anything else, the empty cell included, is an error.
fn parse_reconciled(value: &str) -> Result<ReconcileState, String> {
    match value {
        "n" | "v" => Ok(ReconcileState::New),
        "c" => Ok(ReconcileState::Cleared),
        "y" => Ok(ReconcileState::Reconciled),
        "f" => Ok(ReconcileState::Frozen),
        _ => Err("Value can't be parsed into a valid reconcile state.".to_owned()),
    }
}

/// `operator+(GncNumeric, GncNumeric)` (`5.17:libgnucash/engine/gnc-numeric.cpp`): a zero operand gives the other
/// unchanged; otherwise the exact sum over the least common denominator.
pub(crate) fn gnc_add(a: Numeric, b: Numeric) -> Result<Numeric, NumericError> {
    if a.is_zero() {
        return Ok(b);
    }
    if b.is_zero() {
        return Ok(a);
    }
    a.add_exact(b)
}

/// `operator-(GncNumeric, GncNumeric)`: `a + -b`.
pub(crate) fn gnc_sub(a: Numeric, b: Numeric) -> Result<Numeric, NumericError> {
    gnc_add(a, b.neg())
}

/// `operator*(GncNumeric, GncNumeric)`: zero for a zero operand (as `0/1`), otherwise the exact product.
pub(crate) fn gnc_mul(a: Numeric, b: Numeric) -> Result<Numeric, NumericError> {
    if a.is_zero() || b.is_zero() {
        return Ok(Numeric::ZERO);
    }
    a.mul(b, red_ale_core::DENOM_AUTO, red_ale_core::How::EXACT)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn formats(multi_split: bool) -> Formats {
        Formats {
            date: DateFormat::Ymd,
            currency: CurrencyFormat::Period,
            multi_split,
        }
    }

    fn ctx() -> ParseContext<'static> {
        ParseContext::new(Date::from_ymd(2026, 1, 1).unwrap())
    }

    #[test]
    fn names_round_trip_with_legacy() {
        assert!(
            ColumnType::ALL
                .iter()
                .all(|&t| ColumnType::from_gnucash_name(t.gnucash_name()) == Some(t))
        );
        assert_eq!(ColumnType::from_gnucash_name("Deposit"), Some(ColumnType::Amount));
        assert_eq!(ColumnType::from_gnucash_name("Withdrawal"), Some(ColumnType::AmountNeg));
        assert_eq!(ColumnType::from_gnucash_name("Num"), Some(ColumnType::Num));
        assert_eq!(ColumnType::from_gnucash_name("Bogus"), None);
    }

    #[test]
    fn prop_ranges() {
        assert!(ColumnType::VoidReason.is_trans_prop() && !ColumnType::Action.is_trans_prop() && !ColumnType::None.is_trans_prop());
        assert!(ColumnType::Action.is_split_prop() && ColumnType::TransferRecDate.is_split_prop());
    }

    #[test]
    fn empty_date_and_description_need_multi_split() {
        let mut t = PreTrans::default();
        t.set(ColumnType::Date, "", formats(false), &ctx());
        t.set(ColumnType::Description, "", formats(false), &ctx());
        assert_eq!(t.errors().len(), 2);
        assert!(t.errors()[&ColumnType::Date].starts_with("Date: Date field can not be empty"));
        let mut t = PreTrans::default();
        t.set(ColumnType::Date, "", formats(true), &ctx());
        assert!(t.errors().is_empty());
    }

    #[test]
    fn bad_date_message() {
        let mut t = PreTrans::default();
        t.set(ColumnType::Date, "soon", formats(false), &ctx());
        assert_eq!(
            t.errors()[&ColumnType::Date],
            "Date: Value can't be parsed into a date using the selected date format."
        );
        assert!(t.date.is_none());
    }

    #[test]
    fn reconcile_cells() {
        let mut s = PreSplit::default();
        s.set(ColumnType::RecState, "v", formats(false), &ctx());
        assert_eq!(s.rec_state, Some(ReconcileState::New));
        s.set(ColumnType::RecState, "", formats(false), &ctx());
        assert_eq!(
            s.errors()[&ColumnType::RecState],
            "Reconciled: Value can't be parsed into a valid reconcile state."
        );
    }

    #[test]
    fn multi_column_sum_keeps_denominators() {
        let mut s = PreSplit::default();
        s.reset(ColumnType::Amount, formats(false), &ctx());
        ["1.5", "", "2.25"]
            .iter()
            .for_each(|v| s.add(ColumnType::Amount, v, formats(false), &ctx()));
        assert!(s.amount.unwrap().same_repr(Numeric::new(375, 100)));
        s.add(ColumnType::Amount, "x", formats(false), &ctx());
        s.add(ColumnType::Amount, "1", formats(false), &ctx());
        assert!(s.errors().contains_key(&ColumnType::Amount));
        assert!(s.amount.unwrap().same_repr(Numeric::new(375, 100)));
    }

    #[test]
    fn gnc_operators() {
        assert!(
            gnc_add(Numeric::new(0, 100), Numeric::new(5, 10))
                .unwrap()
                .same_repr(Numeric::new(5, 10))
        );
        assert!(
            gnc_add(Numeric::new(1, 10), Numeric::new(1, 10))
                .unwrap()
                .same_repr(Numeric::new(2, 10))
        );
        assert!(
            gnc_add(Numeric::new(1, 2), Numeric::new(1, 3))
                .unwrap()
                .same_repr(Numeric::new(5, 6))
        );
        assert!(
            gnc_mul(Numeric::new(15, 10), Numeric::new(2, 100))
                .unwrap()
                .same_repr(Numeric::new(30, 1000))
        );
        assert!(
            gnc_mul(Numeric::new(0, 100), Numeric::new(2, 100))
                .unwrap()
                .same_repr(Numeric::ZERO)
        );
        assert!(gnc_sub(Numeric::ZERO, Numeric::new(5, 10)).unwrap().same_repr(Numeric::new(-5, 10)));
    }
}
