//! From tokenized lines to draft transactions.
//!
//! Translated from `5.17:gnucash/import-export/csv-imp/gnc-import-tx.cpp` and `gnc-import-tx.hpp`
//! (Copyright (c) 2015 Geert Janssens <geert@kobaltwit.be>; GPL-2.0-or-later), and from `GncPreTrans::create_trans`
//! and `GncPreSplit::create_split` in `gnc-imp-props-tx.cpp:323-362` and `:675-867`. This translation is
//! GPL-3.0-or-later.
//!
//! GnuCash's `GncTxImport` keeps the parsed lines and reparses them as the user changes settings in the assistant.
//! red-ale has the final settings up front, so [`TxImport::parse`] does once what GnuCash's `settings()` and
//! `tokenize()` leave behind, and the steps that follow are methods:
//!
//! 1. [`TxImport::parse`]: one [`ParsedLine`] per non-empty row, each with its transaction and split properties.
//! 2. The pipeline resolves account and commodity text against the Book and records failures with
//!    [`PreSplit::add_error`] and [`PreTrans::add_error`] through [`TxImport::line_mut`].
//! 3. [`TxImport::verify`] reports what GnuCash's assistant would refuse to go on with.
//! 4. [`TxImport::create_drafts`] makes the [`DraftTransaction`]s, as `GncTxImport::create_transactions` makes
//!    GnuCash's draft transactions, without a Book.

use std::collections::BTreeSet;

use red_ale_core::{Date, Numeric, NumericError, ReconcileState};
use thiserror::Error;

use super::props_tx::{ColumnType, ErrMap, Formats, ParseContext, PreSplit, PreTrans, SplitAccount, gnc_add, gnc_mul, gnc_sub};
use super::settings::TxLayout;
use super::tokenizer::Row;

/// One non-empty row of the file with what GnuCash parsed from it (`parse_line_t`, `gnc-import-tx.hpp:61-78`).
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedLine {
    /// The row's index among all rows the tokenizer produced, empty ones included.
    pub row: usize,
    /// The fields.
    pub cells: Row,
    /// The transaction properties this row's own cells give.
    pub pre_trans: PreTrans,
    /// The split properties.
    pub pre_split: PreSplit,
}

/// The parsed file: GnuCash's `GncTxImport` after `settings()` and `tokenize()`.
#[derive(Debug, Clone)]
pub struct TxImport {
    layout: TxLayout,
    has_base_account: bool,
    lines: Vec<ParsedLine>,
}

/// How lines relate once linked: which line's transaction properties each uses, its errors, and whether it is
/// skipped.
#[derive(Debug, Clone, PartialEq)]
pub struct LineState {
    /// The index of the line whose transaction properties this line uses: itself, or in multi-split mode the line
    /// that started its transaction.
    pub trans_line: usize,
    /// The errors of those transaction properties and of this line's split properties (`PL_ERROR`).
    pub errors: ErrMap,
    /// Skipped by the skip-lines settings or, when asked, for having errors (`PL_SKIP`).
    pub skip: bool,
}

/// Why no drafts could be made.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CreateError {
    /// [`TxImport::verify`] found problems; GnuCash's messages, one per problem.
    #[error("{}", bullets(.0))]
    Verify(Vec<String>),
    /// A line has no transaction to join in two-split mode (`gnc-import-tx.cpp:663-666`, `:723-729`).
    #[error("line {line}: Problem creating preliminary transaction")]
    Transaction {
        /// The line's index.
        line: usize,
    },
    /// A line with no account and no base account (`gnc-import-tx.cpp:699-710`); verification normally catches it.
    #[error("line {line}: No account column selected and no base account specified either.")]
    NoAccount {
        /// The line's index.
        line: usize,
    },
    /// A line's split fails its essentials, which GnuCash drops silently (`gnc-imp-props-tx.cpp:720-728`).
    #[error("line {line}: {reason}")]
    Split {
        /// The line's index.
        line: usize,
        /// GnuCash's reason.
        reason: &'static str,
    },
    /// Adding up a line's amounts overflowed (GnuCash throws `std::overflow_error`).
    #[error("line {line}: {source}")]
    Numeric {
        /// The line's index.
        line: usize,
        /// The arithmetic error.
        source: NumericError,
    },
}

/// `ErrorList::str` (`gnc-import-tx.cpp:485-496`): each message after a bullet, one per line.
pub fn bullets(messages: &[String]) -> String {
    messages.iter().map(|m| format!("• {m}")).collect::<Vec<_>>().join("\n")
}

impl TxImport {
    /// Parse every row with the layout's column types, as `GncTxImport::settings` followed by `tokenize`
    /// (`gnc-import-tx.cpp:329-363`, `:421-468`) leaves the lines.
    ///
    /// Empty rows are dropped. The column types are fitted to the widest row, types the mode doesn't allow become
    /// `None`, a type other than the amount and value types keeps only its first column, and in two-split mode a
    /// base account replaces the first Account column, but an Account column that remains drops the base account
    /// (`set_column_type`, `:888-918`; `base_account`, `:180-209`). In multi-split mode the base account is ignored.
    pub fn parse(rows: Vec<Row>, layout: &TxLayout, ctx: &ParseContext) -> Self {
        let max_cols = rows.iter().map(Vec::len).max().unwrap_or(0);
        let (columns, has_base_account) = effective_columns(layout, max_cols);
        let layout = TxLayout { columns, ..layout.clone() };
        let formats = Formats { date: layout.csv.date_format, currency: layout.csv.currency_format, multi_split: layout.multi_split };
        let lines = rows
            .into_iter()
            .enumerate()
            .filter(|(_, cells)| !cells.is_empty())
            .map(|(row, cells)| parse_line(row, cells, &layout.columns, has_base_account, formats, ctx))
            .collect();
        Self { layout, has_base_account, lines }
    }

    /// The layout in effect, with the column types as parsing used them.
    pub fn layout(&self) -> &TxLayout {
        &self.layout
    }

    /// Whether every line's first split goes to the layout's base account.
    pub fn uses_base_account(&self) -> bool {
        self.has_base_account
    }

    /// The parsed lines.
    pub fn lines(&self) -> &[ParsedLine] {
        &self.lines
    }

    /// One line, for the pipeline to record the errors it finds when resolving accounts and commodities.
    pub fn line_mut(&mut self, index: usize) -> Option<&mut ParsedLine> {
        self.lines.get_mut(index)
    }

    /// Link the lines and decide which are skipped (`update_pre_split_props`, `gnc-import-tx.cpp:831-885`, and
    /// `update_skipped_lines`, `:279-300`). `skip_errors` is the assistant's "Skip errors" check box.
    ///
    /// In multi-split mode a line joins the transaction of the last line that started one when its own transaction
    /// properties are empty or equal to that line's and that line has no errors; skipped lines take part in this
    /// too, as in GnuCash. A joining line reports the starting line's transaction errors, not its own, so a
    /// continuation line with, say, an unreadable date is quietly taken as part of the transaction above it.
    pub fn line_states(&self, skip_errors: bool) -> Vec<LineState> {
        let owners = self.link();
        let count = self.lines.len();
        let csv = &self.layout.csv;
        owners
            .iter()
            .enumerate()
            .map(|(i, &owner)| {
                let mut errors = self.lines[owner].pre_trans.errors().clone();
                self.lines[i].pre_split.errors().iter().for_each(|(k, v)| {
                    errors.entry(*k).or_insert_with(|| v.clone());
                });
                let start = usize::try_from(csv.skip_start_lines).unwrap_or(usize::MAX);
                let end = usize::try_from(csv.skip_end_lines).unwrap_or(usize::MAX);
                let skip = i < start
                    || i >= count.wrapping_sub(end)
                    || (csv.skip_alt_lines && (i - start) % 2 == 1)
                    || (skip_errors && !errors.is_empty());
                LineState { trans_line: owner, errors, skip }
            })
            .collect()
    }

    /// For each line, the line whose transaction properties it uses.
    fn link(&self) -> Vec<usize> {
        let mut parent: Option<usize> = None;
        (0..self.lines.len())
            .map(|i| match parent {
                Some(p) if self.layout.multi_split && self.lines[i].pre_trans.is_part_of(&self.lines[p].pre_trans) => p,
                _ => {
                    parent = Some(i);
                    i
                }
            })
            .collect()
    }

    /// `GncTxImport::verify` (`gnc-import-tx.cpp:568-620`): GnuCash's messages for everything that stops the
    /// import, empty when it can go on. With `with_account_errors` false, errors in the Account and Transfer
    /// Account properties are allowed, as on the assistant's preview page before accounts are matched.
    ///
    /// GnuCash also checks here whether the accounts make transactions multi-currency and, if so, that a price or
    /// value column is selected (`:542-556`); that needs the accounts' commodities and is the pipeline's to add.
    pub fn verify(&self, with_account_errors: bool, skip_errors: bool) -> Vec<String> {
        if self.lines.is_empty() {
            return vec!["No valid data found in the selected file. It may be empty or the selected encoding is wrong.".to_owned()];
        }
        let csv = &self.layout.csv;
        let skipped = csv.skip_start_lines.wrapping_add(csv.skip_end_lines).wrapping_add(u32::from(csv.skip_alt_lines));
        if usize::try_from(skipped).unwrap_or(usize::MAX) >= self.lines.len() {
            return vec!["No lines are selected for importing. Please reduce the number of lines to skip.".to_owned()];
        }
        let mut messages = self.column_selection_errors();
        let account_error = |t: &ColumnType| matches!(t, ColumnType::Account | ColumnType::TransferAccount);
        let line_error = self
            .line_states(skip_errors)
            .iter()
            .filter(|s| !s.skip)
            .any(|s| if with_account_errors { !s.errors.is_empty() } else { s.errors.keys().any(|t| !account_error(t)) });
        if line_error {
            messages.push("Not all fields could be parsed. Please correct the issues reported for each line or adjust the lines to skip.".to_owned());
        }
        messages
    }

    /// `verify_column_selections` (`gnc-import-tx.cpp:503-557`) without the multi-currency part.
    fn column_selection_errors(&self) -> Vec<String> {
        let has = |t| self.layout.columns.contains(&t);
        let account = match (has(ColumnType::Account), self.layout.multi_split, self.has_base_account) {
            (true, _, _) | (false, false, true) => None,
            (false, true, _) => Some("Please select an account column."),
            (false, false, false) => Some("Please select an account column or set a base account in the Account field."),
        };
        [
            (!has(ColumnType::Date)).then_some("Please select a date column."),
            account,
            (!has(ColumnType::Description)).then_some("Please select a description column."),
            (!has(ColumnType::Amount) && !has(ColumnType::AmountNeg)).then_some("Please select a (negated) amount column."),
        ]
        .into_iter()
        .flatten()
        .map(str::to_owned)
        .collect()
    }

    /// `GncTxImport::accounts` (`gnc-import-tx.cpp:925-955`): the distinct non-empty texts of the Account and
    /// Transfer Account columns on lines not skipped, sorted bytewise. These are what the assistant asks the user
    /// to match to accounts.
    pub fn account_names(&self, skip_errors: bool) -> BTreeSet<String> {
        let column = |t| self.layout.columns.iter().position(|&c| c == t);
        let columns: Vec<usize> = [column(ColumnType::Account), column(ColumnType::TransferAccount)].into_iter().flatten().collect();
        self.lines
            .iter()
            .zip(self.line_states(skip_errors))
            .filter(|(_, state)| !state.skip)
            .flat_map(|(line, _)| columns.iter().filter_map(|&c| line.cells.get(c)).filter(|s| !s.is_empty()).cloned().collect::<Vec<_>>())
            .collect()
    }

    /// `GncTxImport::create_transactions` (`gnc-import-tx.cpp:740-764`): verify with account errors, then turn every
    /// line not skipped into a draft transaction or a split of one, ordered by date as GnuCash's multimap orders
    /// them (lines of the same date in file order).
    pub fn create_drafts(&self, skip_errors: bool) -> Result<Vec<DraftTransaction>, CreateError> {
        let problems = self.verify(true, skip_errors);
        if !problems.is_empty() {
            return Err(CreateError::Verify(problems));
        }
        let states = self.line_states(skip_errors);
        let mut builder = DraftBuilder { multi_split: self.layout.multi_split, created: vec![false; self.lines.len()], drafts: Vec::new() };
        states
            .iter()
            .enumerate()
            .filter(|(_, s)| !s.skip)
            .try_for_each(|(i, state)| builder.add_line(i, state.trans_line, &self.lines[state.trans_line].pre_trans, &self.lines[i].pre_split))?;
        let mut drafts = builder.drafts;
        drafts.sort_by_key(|d| d.date);
        Ok(drafts)
    }
}

/// The column types as GnuCash ends up using them, and whether the base account survives.
fn effective_columns(layout: &TxLayout, max_cols: usize) -> (Vec<ColumnType>, bool) {
    let mut types: Vec<ColumnType> = layout.columns.iter().map(|t| t.sanitize(layout.multi_split)).collect();
    types.resize(max_cols, ColumnType::None);
    let mut has_base = !layout.multi_split && layout.base_account.is_some();
    if has_base && let Some(first) = types.iter_mut().find(|t| **t == ColumnType::Account) {
        *first = ColumnType::None;
    }
    let mut seen = BTreeSet::new();
    types.iter_mut().filter(|t| !t.is_multi_col() && **t != ColumnType::None).for_each(|t| {
        if !seen.insert(*t) {
            *t = ColumnType::None;
        }
    });
    has_base &= !types.contains(&ColumnType::Account);
    (types, has_base)
}

/// Parse one row: every column's cell into its property, multi-column sums restarted from zero and added in column
/// order, then the base account (`gnc-import-tx.cpp:776-829`, `:454-460`).
fn parse_line(row: usize, cells: Row, types: &[ColumnType], base: bool, formats: Formats, ctx: &ParseContext) -> ParsedLine {
    let mut pre_trans = PreTrans::default();
    let mut pre_split = PreSplit::default();
    let cell = |i: usize| cells.get(i).map_or("", String::as_str);
    types.iter().enumerate().filter(|(_, t)| t.is_trans_prop()).for_each(|(i, &t)| pre_trans.set(t, cell(i), formats, ctx));
    types.iter().enumerate().filter(|(_, t)| t.is_split_prop() && !t.is_multi_col()).for_each(|(i, &t)| pre_split.set(t, cell(i), formats, ctx));
    let sums: BTreeSet<ColumnType> = types.iter().copied().filter(|t| t.is_multi_col()).collect();
    sums.iter().for_each(|&t| pre_split.reset(t, formats, ctx));
    types.iter().enumerate().filter(|(_, t)| t.is_multi_col()).for_each(|(i, &t)| pre_split.add(t, cell(i), formats, ctx));
    if base {
        pre_split.account = Some(SplitAccount::Base);
    }
    pre_split.flag_missing_reconcile_dates();
    ParsedLine { row, cells, pre_trans, pre_split }
}

/// The state `create_transactions` keeps between lines: the drafts so far (the last is GnuCash's
/// `m_current_draft`) and which lines' transaction properties already made a transaction.
struct DraftBuilder {
    multi_split: bool,
    created: Vec<bool>,
    drafts: Vec<DraftTransaction>,
}

impl DraftBuilder {
    /// `GncTxImport::create_transaction` and `trans_properties_to_trans` (`gnc-import-tx.cpp:629-730`).
    /// `owner` is the line whose transaction properties this line uses.
    fn add_line(&mut self, line: usize, owner: usize, pre_trans: &PreTrans, pre_split: &PreSplit) -> Result<(), CreateError> {
        if pre_split.account.is_none() {
            return Err(CreateError::NoAccount { line });
        }
        let new_draft = if self.created[owner] { None } else { DraftTransaction::new(line, pre_trans) };
        if let Some(draft) = new_draft {
            self.created[owner] = true;
            self.drafts.push(draft);
        } else if !self.multi_split {
            return Err(CreateError::Transaction { line });
        }
        let Some(draft) = self.drafts.last_mut() else {
            return Ok(());
        };
        // GnuCash drops a split that fails its essentials with only a log line (`gnc-imp-props-tx.cpp:720-728`).
        // red-ale makes it an error row (`design/api/import.md`, "Deviations"): the reconcile-date cases are line
        // errors from parsing on, and the rest can't get past verification, so this is a last guard.
        if let Some(reason) = pre_split.missing_essentials().first() {
            return Err(CreateError::Split { line, reason });
        }
        draft.splits.push(DraftSplit::new(line, pre_split).map_err(|source| CreateError::Numeric { line, source })?);
        Ok(())
    }
}

/// A transaction as the CSV importer drafts it, before accounts are resolved: what GnuCash's `DraftTransaction`
/// (`gnc-imp-props-tx.hpp:137-152`) and its unbalanced `Transaction` hold, as values.
#[derive(Debug, Clone, PartialEq)]
pub struct DraftTransaction {
    /// The index of the line that started it.
    pub line: usize,
    /// The Transaction ID column, which grouped its lines.
    pub unique_id: Option<String>,
    /// Posted date; GnuCash posts at neutral time on this date (`GncDateTime(date, DayPart::neutral)`, `:348-349`).
    pub date: Date,
    /// Number. GnuCash sets it with `xaccTransSetNum`, into the transaction's num whatever the book's num-field
    /// source (`gnc-imp-props-tx.cpp:351-352`).
    pub num: Option<String>,
    /// Description.
    pub description: Option<String>,
    /// Notes.
    pub notes: Option<String>,
    /// The Transaction Commodity column as written. When it resolves to a currency that is the transaction's
    /// currency; otherwise the first split's account commodity if a currency, else that account's currency or its
    /// parent's (`gnc-import-tx.cpp:637-642`, `gnc-imp-props-tx.cpp:344-347`).
    pub commodity: Option<String>,
    /// The Void Reason column: the transaction is voided with this reason once built (`xaccTransVoid`). GnuCash
    /// voids a draft only when it starts the next one, so never the file's last (`gnc-import-tx.cpp:646-660`);
    /// red-ale voids every draft that has a reason (`design/api/import.md`, "Deviations").
    pub void_reason: Option<String>,
    /// The splits, one per line, in line order.
    pub splits: Vec<DraftSplit>,
}

impl DraftTransaction {
    /// `GncPreTrans::create_trans` (`gnc-imp-props-tx.cpp:323-362`): `None` without a date and a description.
    fn new(line: usize, pre_trans: &PreTrans) -> Option<Self> {
        pre_trans.description.as_ref()?;
        Some(Self {
            line,
            unique_id: pre_trans.unique_id.clone(),
            date: pre_trans.date?,
            num: pre_trans.num.clone(),
            description: pre_trans.description.clone(),
            notes: pre_trans.notes.clone(),
            commodity: pre_trans.commodity.clone(),
            void_reason: pre_trans.void_reason.clone(),
            splits: Vec::new(),
        })
    }
}

/// One line's split, before its account is resolved and its value worked out.
#[derive(Debug, Clone, PartialEq)]
pub struct DraftSplit {
    /// The index of the line it comes from.
    pub line: usize,
    /// Its account.
    pub account: SplitAccount,
    /// The amount: the Amount columns less the Amount (Negated) columns (`gnc-imp-props-tx.cpp:739-742`).
    pub amount: Numeric,
    /// The value when the file gives one: the Value columns less the Value (Negated) columns (`:762-768`).
    pub value: Option<Numeric>,
    /// The Price column.
    pub price: Option<Numeric>,
    /// Action.
    pub action: Option<String>,
    /// Memo.
    pub memo: Option<String>,
    /// Reconcile state, set only when not `n` (`:704-705`).
    pub reconcile: Option<ReconcileState>,
    /// Reconcile date, set only for a reconciled split (`:706-708`).
    pub reconcile_date: Option<Date>,
    /// What the line says about the other side of a two-split transaction.
    pub transfer: DraftTransfer,
}

/// The transfer columns of a two-split line, as read. When the account is known and its amount can be worked out,
/// GnuCash adds the second split itself (`gnc-imp-props-tx.cpp:801-845`, see [`DraftSplit::transfer_amount`]);
/// otherwise it hands these to the generic matcher as `GNCImportLastSplitInfo`, with the price (`:847-864`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DraftTransfer {
    /// The Transfer Account column as written.
    pub account: Option<String>,
    /// The Transfer Amount columns less the Transfer Amount (Negated) columns, when either is given.
    pub amount: Option<Numeric>,
    /// Transfer action.
    pub action: Option<String>,
    /// Transfer memo.
    pub memo: Option<String>,
    /// Transfer reconcile state, as read (GnuCash sets it on a split it creates only when not `n`).
    pub reconcile: Option<ReconcileState>,
    /// Transfer reconcile date, as read (set on a created split only when it is reconciled).
    pub reconcile_date: Option<Date>,
}

/// The price GnuCash finds when the file gives none: `gnc_pricedb_lookup_nearest_in_time64` between the account's
/// commodity and the transaction's currency at the posted date.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NearestPrice {
    /// The price's value.
    pub value: Numeric,
    /// Whether the price is quoted in the transaction's currency (else in the account's commodity).
    pub in_transaction_currency: bool,
}

impl DraftSplit {
    fn new(line: usize, pre: &PreSplit) -> Result<Self, NumericError> {
        let net = |plus: Option<Numeric>, minus: Option<Numeric>| -> Result<Numeric, NumericError> {
            let total = plus.map_or(Ok(Numeric::ZERO), |p| gnc_add(Numeric::ZERO, p))?;
            minus.map_or(Ok(total), |m| gnc_sub(total, m))
        };
        let either = |plus: Option<Numeric>, minus: Option<Numeric>| (plus.is_some() || minus.is_some()).then(|| net(plus, minus)).transpose();
        let reconcile = pre.rec_state.filter(|&s| s != ReconcileState::New);
        Ok(Self {
            line,
            account: pre.account.clone().unwrap_or(SplitAccount::Base),
            amount: net(pre.amount, pre.amount_neg)?,
            value: either(pre.value, pre.value_neg)?,
            price: pre.price,
            action: pre.action.clone(),
            memo: pre.memo.clone(),
            reconcile,
            reconcile_date: pre.rec_date.filter(|_| reconcile == Some(ReconcileState::Reconciled)),
            transfer: DraftTransfer {
                account: pre.transfer_account.clone(),
                amount: either(pre.transfer_amount, pre.transfer_amount_neg)?,
                action: pre.transfer_action.clone(),
                memo: pre.transfer_memo.clone(),
                reconcile: pre.transfer_rec_state,
                reconcile_date: pre.transfer_rec_date,
            },
        })
    }

    /// The split's value as `GncPreSplit::create_split` works it out (`gnc-imp-props-tx.cpp:754-795`), preferring
    /// the way with the least rounding: the file's value; the amount when the account is in the transaction's
    /// currency; the negated transfer amount; amount times the file's price; amount times the nearest price from
    /// the price database (inverted when quoted the other way). With none of these the value is zero, and GnuCash
    /// still adds the split.
    pub fn split_value(&self, account_in_transaction_currency: bool, nearest_price: impl FnOnce() -> Option<NearestPrice>) -> Result<Numeric, NumericError> {
        if let Some(value) = self.value {
            return Ok(value);
        }
        if account_in_transaction_currency {
            return Ok(self.amount);
        }
        if let Some(amount) = self.transfer.amount {
            return Ok(amount.neg());
        }
        if let Some(price) = self.price {
            return gnc_mul(self.amount, price);
        }
        match nearest_price().filter(|p| !p.value.is_zero()) {
            Some(p) if p.in_transaction_currency => gnc_mul(self.amount, p.value),
            Some(p) => gnc_mul(self.amount, p.value.invert()?),
            None => Ok(Numeric::ZERO),
        }
    }

    /// The transfer split's amount when the line names a transfer account (`gnc-imp-props-tx.cpp:801-837`): its value
    /// is always the negated split value; its amount is that value when the transfer account is in the
    /// transaction's currency, else the file's transfer amount, else the value over the file's price, else the
    /// value converted with the nearest price. `None` when nothing gives it: GnuCash then leaves the split to the
    /// matcher.
    pub fn transfer_amount(
        &self,
        split_value: Numeric,
        transfer_account_in_transaction_currency: bool,
        nearest_price: impl FnOnce() -> Option<NearestPrice>,
    ) -> Result<Option<Numeric>, NumericError> {
        let transfer_value = split_value.neg();
        if transfer_account_in_transaction_currency {
            return Ok(Some(transfer_value));
        }
        if let Some(amount) = self.transfer.amount {
            return Ok(Some(amount));
        }
        if let Some(price) = self.price {
            return gnc_mul(transfer_value, price.invert()?).map(Some);
        }
        match nearest_price().filter(|p| !p.value.is_zero()) {
            Some(p) if p.in_transaction_currency => gnc_mul(transfer_value, p.value.invert()?).map(Some),
            Some(p) => gnc_mul(transfer_value, p.value).map(Some),
            None => Ok(None),
        }
    }
}
