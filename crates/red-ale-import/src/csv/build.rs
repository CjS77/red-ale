//! The CSV transaction import's steps that need the Book: resolving account and commodity texts, the multi-currency
//! checks, and building the transactions the matcher takes, as GnuCash's `GncTxImport` does once its lines are
//! parsed.
//!
//! Translated from `5.17:gnucash/import-export/csv-imp/gnc-imp-props-tx.cpp` (`GncPreSplit::set` for accounts,
//! `:448-469`; `parse_commodity`, `:188-226`; `UpdateCrossSplitCounters` and `is_multi_currency`, `:391-424`;
//! `verify_essentials`, `:654-670`; `GncPreTrans::create_trans`, `:323-362`; `trans_add_split` and
//! `GncPreSplit::create_split`, `:675-867`) and `gnc-import-tx.cpp` (`verify_column_selections`, `:542-556`;
//! `trans_properties_to_trans`, `:629-678`) and `assistant-csv-trans-import.cpp` (the account match page,
//! `:1803-1835`, and the hand-off to the matcher, `:2134-2218`). Copyright (c) 2015, 2016 Geert Janssens; Copyright
//! (C) 2012 Robert Fewell. GPL-2.0-or-later; this translation is GPL-3.0-or-later.
//!
//! **Deviations** (`design/api/import.md`): a line GnuCash would drop silently when building its split (the
//! multi-currency essentials, a value that rounds to nothing) is an error row; every draft's void reason applies; a
//! voided draft is voided as it was built, without first being committed, so an unbalanced voided line gets no
//! zero-valued Imbalance split.

use std::collections::BTreeMap;

use red_ale_core::currencies::iso_currency;
use red_ale_core::ops::commit::{round_amount, round_value, set_date_posted, set_notes};
use red_ale_core::ops::prices::{Nearest, nearest};
use red_ale_core::ops::scrub::currency_or_parent;
use red_ale_core::ops::transactions::void::void_detached;
use red_ale_core::{Book, CommodityRef, Date, DomainError, Guid, Numeric, ReconcileState, Split, Time64, TimeZone, Transaction};

use super::import_tx::{CreateError, DraftSplit, DraftTransaction, LineState, NearestPrice, TxImport};
use super::props_tx::{ColumnType, SplitAccount};
use crate::import_map::{CATEGORY_CSV_ACCOUNT_MAP, find_any};
use crate::matcher::{ImportedTransaction, LastSplitInfo};
use crate::pipeline::{AccountName, IgnoredLine, NameSource, RowError, Source};

/// What building needs from the Book and the session.
pub struct BuildContext<'a> {
    /// The Book.
    pub book: &'a Book,
    /// The local time zone: posted dates are at its day-neutral time.
    pub zone: &'a TimeZone,
    /// Now, for void times.
    pub now: Time64,
    /// The base account, already resolved, when the layout or the caller gives one.
    pub base_account: Option<Guid>,
    /// The caller's `accountNames`: what each name the file uses means.
    pub account_names: &'a BTreeMap<String, Guid>,
    /// The file's index among the import files.
    pub file: usize,
}

/// One transaction built, ready for the matcher.
#[derive(Debug, Clone, PartialEq)]
pub struct Built {
    /// The transaction and the balancing split's details.
    pub imported: ImportedTransaction,
    /// Its lines.
    pub source: Source,
}

/// Everything building found.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CsvBuild {
    /// Every account name the file uses, bytewise, with how it resolved.
    pub account_names: Vec<AccountName>,
    /// Rows that can't be imported.
    pub errors: Vec<RowError>,
    /// Lines the layout skips.
    pub ignored: Vec<IgnoredLine>,
    /// The transactions, by posted date, ties in file order.
    pub transactions: Vec<Built>,
}

/// Why the file can't be imported at all: GnuCash's assistant would not go on (`GncTxImport::verify`).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{}", .messages.join(" "))]
pub struct FileError {
    /// The line, from 1, when one line is the cause.
    pub line: Option<usize>,
    /// GnuCash's messages.
    pub messages: Vec<String>,
}

impl FileError {
    fn new(line: Option<usize>, messages: Vec<String>) -> Self {
        Self { line, messages }
    }
}

/// Resolve, check and build: GnuCash's steps from the parsed lines to the transactions it hands the matcher. Each
/// transaction and split gets a GUID from `new_guid`, in order.
pub fn build(mut import: TxImport, ctx: &BuildContext, new_guid: impl FnMut() -> Guid) -> Result<CsvBuild, FileError> {
    let account_names = resolve_accounts(&mut import, ctx);
    let commodities = resolve_commodities(&mut import, ctx.book);
    let mut messages = import.file_errors();
    messages.extend(check_multi_currency(&mut import, ctx, &account_names, &commodities));
    if !messages.is_empty() {
        return Err(FileError::new(None, messages));
    }
    let states = import.line_states(false);
    let errors = row_errors(&import, &states, ctx.file);
    let ignored = ignored_lines(&import, &states, ctx.file);
    let drafts = import.create_drafts(true).map_err(|e| create_error(&import, &e))?;
    let resolved: BTreeMap<String, Guid> = account_names
        .iter()
        .filter_map(|n| n.account.map(|a| (n.name.clone(), a)))
        .collect();
    let builder = Builder {
        ctx,
        import: &import,
        accounts: &resolved,
        commodities: &commodities,
    };
    let (transactions, build_errors): (Vec<_>, Vec<_>) = drafts.iter().map(|d| builder.transaction(d)).partition(Result::is_ok);
    let mut errors = errors;
    errors.extend(build_errors.into_iter().filter_map(Result::err));
    errors.sort_by_key(|e| (e.line, e.column));
    let mut transactions: Vec<Built> = transactions.into_iter().filter_map(Result::ok).collect();
    assign_guids(&mut transactions, new_guid);
    Ok(CsvBuild {
        account_names,
        errors,
        ignored,
        transactions,
    })
}

fn row(import: &TxImport, line: usize) -> usize {
    import.lines().get(line).map_or(line, |l| l.row) + 1
}

fn create_error(import: &TxImport, e: &CreateError) -> FileError {
    let line = match e {
        CreateError::Verify(_) => None,
        CreateError::Transaction { line }
        | CreateError::NoAccount { line }
        | CreateError::Split { line, .. }
        | CreateError::Numeric { line, .. } => Some(row(import, *line)),
    };
    let messages = match e {
        CreateError::Verify(m) => m.clone(),
        other => vec![other.to_string()],
    };
    FileError::new(line, messages)
}

/// The account a name means: the caller's choice, then the `csv-account-map` Import map
/// (`gnc_account_imap_find_any`), then the full name (`gnc-imp-props-tx.cpp:448-469`;
/// `assistant-csv-trans-import.cpp:1816-1829`).
fn resolve_name(ctx: &BuildContext, name: &str) -> AccountName {
    let mapped = find_any(ctx.book, Some(CATEGORY_CSV_ACCOUNT_MAP), name).map(|a| (a, NameSource::Map));
    let previous = mapped.or_else(|| ctx.book.account_by_full_name(name).map(|a| (a.guid, NameSource::FullName)));
    let chosen = ctx.account_names.get(name).map(|a| (*a, NameSource::Param)).or(previous);
    AccountName {
        name: name.to_owned(),
        account: chosen.map(|c| c.0),
        source: chosen.map(|c| c.1),
        previous: previous.map(|p| p.0),
    }
}

/// Resolve every Account and Transfer Account text and record GnuCash's error on each line whose name means no
/// account; a line using the base account gets an error when there is none.
fn resolve_accounts(import: &mut TxImport, ctx: &BuildContext) -> Vec<AccountName> {
    let names: Vec<AccountName> = import.account_names(false).iter().map(|n| resolve_name(ctx, n)).collect();
    let known = |text: &str| {
        names
            .iter()
            .find(|n| n.name == text)
            .and_then(|n| n.account)
            .or_else(|| resolve_name(ctx, text).account)
    };
    (0..import.lines().len()).for_each(|i| {
        let Some(line) = import.line_mut(i) else { return };
        let split = &mut line.pre_split;
        let missing = match &split.account {
            Some(SplitAccount::Named(text)) => known(text).is_none(),
            Some(SplitAccount::Base) => ctx.base_account.is_none(),
            None => false,
        };
        if missing {
            split.add_error(ColumnType::Account, "Account value can't be mapped back to an account.");
        }
        if split.transfer_account.as_deref().is_some_and(|text| known(text).is_none()) {
            split.add_error(
                ColumnType::TransferAccount,
                "Transfer account value can't be mapped back to an account.",
            );
        }
    });
    names
}

/// `parse_commodity` (`gnc-imp-props-tx.cpp:188-226`): by unique name, then as a currency, then by mnemonic in each
/// other namespace. GnuCash's commodity table holds every ISO currency; red-ale's built-in table stands in for those
/// the Book has not defined.
pub fn parse_commodity(book: &Book, text: &str) -> Option<CommodityRef> {
    let known = |c: &CommodityRef| book.commodity(c).is_some() || (c.is_currency() && iso_currency(&c.mnemonic).is_some());
    let unique = text.split_once("::").map(|(ns, m)| CommodityRef::new(ns, m)).filter(known);
    unique.or_else(|| Some(CommodityRef::currency(text)).filter(known)).or_else(|| {
        book.commodities
            .keys()
            .filter(|c| !c.is_currency())
            .find(|c| c.mnemonic == text)
            .cloned()
    })
}

/// Each line's Transaction Commodity, resolved; an unknown one is an error on the line.
fn resolve_commodities(import: &mut TxImport, book: &Book) -> Vec<Option<CommodityRef>> {
    (0..import.lines().len())
        .map(|i| {
            let line = import.line_mut(i)?;
            let text = line.pre_trans.commodity.clone()?;
            let found = parse_commodity(book, &text);
            if found.is_none() {
                line.pre_trans
                    .add_error(ColumnType::Commodity, "Value can't be parsed into a valid commodity.");
            }
            found
        })
        .collect()
}

/// The resolved account of a line's Account property (or the base account).
fn line_account(import: &TxImport, i: usize, ctx: &BuildContext, names: &[AccountName]) -> Option<Guid> {
    match &import.lines().get(i)?.pre_split.account {
        Some(SplitAccount::Named(text)) => names.iter().find(|n| &n.name == text).and_then(|n| n.account),
        Some(SplitAccount::Base) => ctx.base_account,
        None => None,
    }
}

/// `UpdateCrossSplitCounters` and `is_multi_currency` (`gnc-imp-props-tx.cpp:391-424`) over each transaction's lines,
/// then `verify_column_selections`' column check (`gnc-import-tx.cpp:542-556`) and, per split,
/// `verify_essentials`' (`gnc-imp-props-tx.cpp:654-670`), which GnuCash checks only when building the split and
/// then drops it silently; red-ale makes it an error on the line. Returns the column check's message, if any.
fn check_multi_currency(
    import: &mut TxImport,
    ctx: &BuildContext,
    names: &[AccountName],
    commodities: &[Option<CommodityRef>],
) -> Vec<String> {
    let states = import.line_states(false);
    let mut groups: BTreeMap<usize, (Vec<CommodityRef>, Vec<CommodityRef>)> = BTreeMap::new();
    states.iter().enumerate().for_each(|(i, s)| {
        let (alt, comms) = groups.entry(s.trans_line).or_default();
        let Some(acc) = line_account(import, i, ctx, names).and_then(|a| ctx.book.account(a)) else {
            return;
        };
        let (currency, commodity) = match &acc.commodity {
            Some(c) if c.is_currency() => (Some(c.clone()), None),
            other => (currency_or_parent(ctx.book, acc), other.clone()),
        };
        currency.filter(|c| !alt.contains(c)).into_iter().for_each(|c| alt.push(c));
        commodity.filter(|c| !comms.contains(c)).into_iter().for_each(|c| comms.push(c));
    });
    let multi = |owner: usize| {
        groups.get(&owner).is_some_and(|(alt, comms)| {
            let extra = commodities.get(owner).cloned().flatten().is_some_and(|c| !alt.contains(&c));
            alt.len() + comms.len() + usize::from(extra) > 1
        })
    };
    let multi_lines: Vec<usize> = states
        .iter()
        .enumerate()
        .filter(|(_, s)| multi(s.trans_line))
        .map(|(i, _)| i)
        .collect();
    if multi_lines.is_empty() {
        return Vec::new();
    }
    let multi_split = import.layout().multi_split;
    let has = |t: ColumnType| import.layout().columns.contains(&t);
    let priced = has(ColumnType::Price) || has(ColumnType::Value) || has(ColumnType::ValueNeg);
    let transferred = has(ColumnType::TransferAmount) || has(ColumnType::TransferAmountNeg);
    if multi_split && !priced {
        return vec!["The current account selections will generate multi-currency transactions. Please select one of the following columns: price, (negated) value.".to_owned()];
    }
    if !multi_split && !priced && !transferred {
        return vec!["The current account selections will generate multi-currency transactions. Please select one of the following columns: price, (negated) value, (negated) transfer amount.".to_owned()];
    }
    multi_lines.into_iter().for_each(|i| flag_unpriced(import, i, multi_split));
    Vec::new()
}

fn flag_unpriced(import: &mut TxImport, i: usize, multi_split: bool) {
    let Some(line) = import.line_mut(i) else { return };
    let s = &mut line.pre_split;
    let priced = s.price.is_some() || s.value.is_some() || s.value_neg.is_some();
    let transferred = s.transfer_amount.is_some() || s.transfer_amount_neg.is_some();
    if multi_split && !priced {
        s.add_error(
            ColumnType::Price,
            "Choice of accounts makes this a multi-currency transaction but price or (negated) value column is missing or invalid.",
        );
    } else if !multi_split && !priced && !transferred {
        s.add_error(
            ColumnType::Price,
            "Choice of accounts makes this a multi-currency transaction but price, (negated) value or (negated) transfer amount column is missing or invalid.",
        );
    }
}

/// One error row per line with errors that is not skipped, each error with its column.
fn row_errors(import: &TxImport, states: &[LineState], file: usize) -> Vec<RowError> {
    let columns = &import.layout().columns;
    states
        .iter()
        .enumerate()
        .filter(|(_, s)| !s.skip)
        .flat_map(|(i, s)| {
            s.errors.iter().map(move |(prop, message)| RowError {
                file,
                line: row(import, i),
                column: columns.iter().position(|c| c == prop).map(|c| c + 1),
                message: message.trim_end().to_owned(),
            })
        })
        .collect()
}

/// The lines the layout skips, with the setting that skips each (`update_skipped_lines`, `gnc-import-tx.cpp:279-300`).
fn ignored_lines(import: &TxImport, states: &[LineState], file: usize) -> Vec<IgnoredLine> {
    let csv = &import.layout().csv;
    let count = states.len();
    let start = usize::try_from(csv.skip_start_lines).unwrap_or(usize::MAX);
    let end = usize::try_from(csv.skip_end_lines).unwrap_or(usize::MAX);
    (0..count)
        .filter_map(|i| {
            let reason = if i < start {
                "skipStartLines"
            } else if i >= count.saturating_sub(end) && end > 0 {
                "skipEndLines"
            } else if csv.skip_alt_lines && (i - start) % 2 == 1 {
                "skipAltLines"
            } else {
                return None;
            };
            Some(IgnoredLine {
                file,
                line: row(import, i),
                reason,
            })
        })
        .collect()
}

/// Builds transactions from drafts.
struct Builder<'a> {
    ctx: &'a BuildContext<'a>,
    import: &'a TxImport,
    accounts: &'a BTreeMap<String, Guid>,
    commodities: &'a [Option<CommodityRef>],
}

impl Builder<'_> {
    fn account(&self, account: &SplitAccount) -> Option<Guid> {
        match account {
            SplitAccount::Named(text) => self.accounts.get(text).copied(),
            SplitAccount::Base => self.ctx.base_account,
        }
    }

    fn error(&self, line: usize, message: impl Into<String>) -> RowError {
        RowError {
            file: self.ctx.file,
            line: row(self.import, line),
            column: None,
            message: message.into(),
        }
    }

    /// `trans_properties_to_trans` and `GncPreTrans::create_trans` (`gnc-import-tx.cpp:629-678`,
    /// `gnc-imp-props-tx.cpp:323-362`), then each split (`GncPreSplit::create_split`), then the void.
    fn transaction(&self, draft: &DraftTransaction) -> Result<Built, RowError> {
        let book = self.ctx.book;
        let first = draft
            .splits
            .first()
            .ok_or_else(|| self.error(draft.line, "No split could be created for this transaction."))?;
        let account = self
            .account(&first.account)
            .and_then(|a| book.account(a))
            .ok_or_else(|| self.error(first.line, "Account value can't be mapped back to an account."))?;
        let fallback = account
            .commodity
            .clone()
            .filter(CommodityRef::is_currency)
            .or_else(|| currency_or_parent(book, account));
        let column = self
            .commodities
            .get(draft.line)
            .cloned()
            .flatten()
            .filter(CommodityRef::is_currency);
        let currency = column.or(fallback).ok_or_else(|| {
            self.error(
                draft.line,
                format!("Account {} has no currency to use for the transaction.", account.name),
            )
        })?;
        let mut t = self.new_transaction(draft, currency);
        let mut last = None;
        draft.splits.iter().try_for_each(|s| self.add_splits(&mut t, s, &mut last))?;
        if let Some(reason) = &draft.void_reason {
            void_detached(book, self.ctx.now, &mut t, reason).map_err(|e| self.error(draft.line, e.to_string()))?;
        }
        let lines = std::iter::once(draft.line)
            .chain(draft.splits.iter().map(|s| s.line))
            .map(|l| row(self.import, l));
        let (first_line, last_line) = lines.fold((usize::MAX, 0), |(lo, hi), l| (lo.min(l), hi.max(l)));
        Ok(Built {
            imported: ImportedTransaction {
                transaction: t,
                last_split: last,
            },
            source: Source {
                file: self.ctx.file,
                first_line,
                last_line,
            },
        })
    }

    fn new_transaction(&self, draft: &DraftTransaction, currency: CommodityRef) -> Transaction {
        let mut t = Transaction {
            guid: placeholder_guid(),
            currency,
            num: draft.num.clone(),
            date_posted: Time64(0),
            date_entered: Time64(0),
            description: draft.description.clone(),
            slots: red_ale_core::Frame::new(),
            splits: Vec::new(),
        };
        set_date_posted(&mut t, draft.date, self.ctx.zone);
        set_notes(&mut t, draft.notes.as_deref());
        t
    }

    /// `GncPreSplit::create_split` (`gnc-imp-props-tx.cpp:712-867`): the line's split, the transfer split when the
    /// transfer account and amount are known, else what the matcher needs for the balancing split.
    fn add_splits(&self, t: &mut Transaction, s: &DraftSplit, last: &mut Option<LastSplitInfo>) -> Result<(), RowError> {
        let book = self.ctx.book;
        let fail = |e: &dyn std::fmt::Display| self.error(s.line, e.to_string());
        let account = self
            .account(&s.account)
            .ok_or_else(|| self.error(s.line, "Account value can't be mapped back to an account."))?;
        let in_currency = |a: Guid| book.account(a).and_then(|a| a.commodity.as_ref()) == Some(&t.currency);
        let value = s
            .split_value(in_currency(account), || self.nearest(account, t))
            .map_err(|e| fail(&e))?;
        let split = self
            .split(
                t,
                account,
                s.amount,
                value,
                s.action.clone(),
                s.memo.clone(),
                s.reconcile,
                s.reconcile_date,
            )
            .map_err(|e| fail(&e))?;
        t.splits.push(split);
        let transfer = s.transfer.account.as_ref().and_then(|text| self.accounts.get(text).copied());
        let transfer_amount = match transfer {
            Some(ta) => s
                .transfer_amount(value, in_currency(ta), || self.nearest(ta, t))
                .map_err(|e| fail(&e))?,
            None => None,
        };
        match (transfer, transfer_amount) {
            (Some(ta), Some(amount)) => {
                let tr = &s.transfer;
                let state = tr.reconcile.filter(|r| *r != ReconcileState::New);
                let date = tr.reconcile_date.filter(|_| state == Some(ReconcileState::Reconciled));
                let split = self
                    .split(t, ta, amount, value.neg(), tr.action.clone(), tr.memo.clone(), state, date)
                    .map_err(|e| fail(&e))?;
                t.splits.push(split);
            }
            _ => {
                *last = Some(LastSplitInfo {
                    price: s.price,
                    amount: s.transfer.amount,
                    account: transfer,
                })
            }
        }
        Ok(())
    }

    /// `trans_add_split` (`gnc-imp-props-tx.cpp:675-710`): amount and value rounded as `xaccSplitSetAmount` and
    /// `xaccSplitSetValue` round them, memo, action (whatever the Book's num field source), and the reconcile state
    /// and date.
    #[allow(clippy::too_many_arguments)]
    fn split(
        &self,
        t: &Transaction,
        account: Guid,
        amount: Numeric,
        value: Numeric,
        action: Option<String>,
        memo: Option<String>,
        state: Option<ReconcileState>,
        date: Option<Date>,
    ) -> Result<Split, DomainError> {
        let book = self.ctx.book;
        let mut split = Split::new(
            placeholder_guid(),
            account,
            round_value(book, &t.currency, value)?,
            round_amount(book, account, amount)?,
        );
        split.action = action;
        split.memo = memo;
        if let Some(state) = state {
            split.reconcile_state = state;
        }
        split.reconcile_date = date.map(|d| self.ctx.zone.neutral_time(d));
        Ok(split)
    }

    /// `gnc_pricedb_lookup_nearest_in_time64` between the account's commodity and the transaction currency.
    fn nearest(&self, account: Guid, t: &Transaction) -> Option<NearestPrice> {
        let commodity = self.ctx.book.account(account)?.commodity.clone()?;
        let price = nearest(
            self.ctx.book,
            &commodity,
            &t.currency,
            t.date_posted,
            Nearest::Nearest,
            self.ctx.zone,
        )?;
        Some(NearestPrice {
            value: price.value,
            in_transaction_currency: price.currency == t.currency,
        })
    }
}

/// A placeholder GUID, replaced by [`assign_guids`] once every transaction is built.
fn placeholder_guid() -> Guid {
    Guid::NULL
}

/// Give each built transaction and split a fresh GUID, in order.
fn assign_guids(built: &mut [Built], mut new_guid: impl FnMut() -> Guid) {
    built.iter_mut().for_each(|b| {
        let t = &mut b.imported.transaction;
        t.guid = new_guid();
        t.splits.iter_mut().for_each(|s| s.guid = new_guid());
    });
}
