//! CSV account import: GnuCash's account tree export read back into a Book, creating the accounts it lacks and
//! updating the ones it has (`design/api/import.md`, "CSV accounts").
//!
//! Translated from `csv_account_import` (`5.17:gnucash/import-export/csv-imp/csv-account-import.c:201-374`).
//! Copyright (C) 2012 Robert Fewell. GPL-2.0-or-later; this translation is GPL-3.0-or-later. The file is read by
//! [`read`]; the summary GnuCash's assistant shows (`assistant-csv-account-import.c:441-467`) is the proposal.
//!
//! Each row is found by its full name. **An existing account** gets the row's code, description, notes and colour,
//! each only when the cell is not empty; its type, name, commodity, hidden, placeholder and tax flags never change.
//! **A new account** is created under the account its full name names less its name, with the row's type and
//! commodity, code, description, notes and colour when given, and hidden and placeholder when the cell is `T`. The
//! Tax Info cell is read and never used. Rows run in file order, so a parent created by one row is there for the
//! rows after it, and a second row for an account a first one created updates it.
//!
//! **Deviations** (`design/api/import.md`), so that nothing lands where the file didn't say:
//!
//! - a missing parent, an unknown commodity, a type GnuCash doesn't know and a name that isn't the end of the full
//!   name are error rows (GnuCash creates a top-level account, skips the row, creates an `INVALID` account, or
//!   cuts the full name at the wrong place);
//! - a new account goes through `account.create`'s rules (type under its parent's type, commodity for the type,
//!   name without `:`), and a row breaking one is an error row;
//! - a colour GnuCash can't parse leaves the colour as it was, with a warning (GnuCash clears it); colours by name
//!   (`red`) are not recognised;
//! - an existing account whose cells change nothing is `skip`, where GnuCash counts it updated, and cells GnuCash
//!   doesn't update but that differ (type, commodity, hidden, placeholder, tax) get a `not-updated` warning.

pub mod read;

use red_ale_core::ops::accounts::{AccountEdit, create_account};
use red_ale_core::ops::commodities::lookup;
use red_ale_core::{Account, AccountType, Book, CommodityRef, DomainError, DomainResult, Env, FixedEnv, Guid, Time64};

pub use read::{AccountFile, Separator, read};

use crate::matcher::ImportAction;
use crate::pipeline::{Applied, Change, Proposal, RowError, Warning};

/// The first GUID the preview's copy of the Book counts up from: far from any a Book holds.
const PREVIEW_GUID_SEED: u128 = 0x7265_642d_616c_6520_6163_636f_756e_0000;

/// One row of an account file: the twelve fields of GnuCash's account tree export, as read
/// (`5.17:gnucash/import-export/csv-exp/csv-tree-export.cpp:59-95`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AccountRow {
    /// The record's line, from 1.
    pub line: usize,
    /// The type, as GnuCash names it (`BANK`).
    pub account_type: String,
    /// The full name, parts joined by `:`.
    pub full_name: String,
    /// The name: the last part of the full name.
    pub name: String,
    /// The account code.
    pub code: String,
    /// The description.
    pub description: String,
    /// The colour.
    pub color: String,
    /// The notes.
    pub notes: String,
    /// The commodity's mnemonic.
    pub symbol: String,
    /// The commodity's namespace.
    pub namespace: String,
    /// `T` when hidden.
    pub hidden: String,
    /// `T` when tax-related; GnuCash's import never uses it.
    pub tax: String,
    /// `T` when a placeholder.
    pub placeholder: String,
}

impl AccountRow {
    /// The row from its twelve fields, in the export's order; missing fields are empty.
    pub fn from_fields(line: usize, fields: Vec<String>) -> Self {
        let mut f = fields.into_iter();
        let mut next = || f.next().unwrap_or_default();
        Self {
            line,
            account_type: next(),
            full_name: next(),
            name: next(),
            code: next(),
            description: next(),
            color: next(),
            notes: next(),
            symbol: next(),
            namespace: next(),
            hidden: next(),
            tax: next(),
            placeholder: next(),
        }
    }
}

/// A field an update changes: its name (`code`, `description`, `notes`, `color`), the account's value and the row's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldChange {
    /// The field.
    pub field: &'static str,
    /// What the account holds.
    pub from: Option<String>,
    /// What the row gives.
    pub to: String,
}

/// One account row of the proposal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProposedAccount {
    /// Its position in the proposal: the key `import.apply` changes it by.
    pub index: usize,
    /// The row.
    pub row: AccountRow,
    /// `add`, `update`, or `skip` for an existing account the row doesn't change.
    pub action: ImportAction,
    /// The existing account an update changes; `None` for an add, or for an account an earlier row adds.
    pub account: Option<Guid>,
    /// What an update changes.
    pub changes: Vec<FieldChange>,
    /// Warnings for this row.
    pub warnings: Vec<Warning>,
}

/// What one row did.
enum Outcome {
    Added(Guid),
    Updated(Guid, Vec<FieldChange>),
    Unchanged(Guid),
}

/// The proposal's accounts and error rows: every row run in order on a copy of `book`, as applying will run them.
pub fn propose(book: &Book, rows: &[AccountRow], file: usize) -> (Vec<ProposedAccount>, Vec<RowError>) {
    let mut copy = book.clone();
    let mut env = FixedEnv::new(Time64(0), PREVIEW_GUID_SEED);
    let mut accounts = Vec::new();
    let mut errors = Vec::new();
    rows.iter().for_each(|row| {
        let mut warnings = Vec::new();
        match import_row(&mut copy, &mut env, row, &mut warnings) {
            Ok(outcome) => {
                let index = accounts.len();
                let warnings = warnings
                    .into_iter()
                    .map(|(code, message)| Warning::about(index, code, message))
                    .collect();
                accounts.push(proposed(book, index, row, outcome, warnings));
            }
            Err(message) => errors.push(RowError {
                file,
                line: row.line,
                column: None,
                message,
            }),
        }
    });
    (accounts, errors)
}

fn proposed(book: &Book, index: usize, row: &AccountRow, outcome: Outcome, warnings: Vec<Warning>) -> ProposedAccount {
    let existing = |guid: Guid| book.account(guid).map(|a| a.guid);
    let (action, account, changes) = match outcome {
        Outcome::Added(_) => (ImportAction::Add, None, Vec::new()),
        Outcome::Updated(guid, changes) => (ImportAction::Update, existing(guid), changes),
        Outcome::Unchanged(guid) => (ImportAction::Skip, existing(guid), Vec::new()),
    };
    ProposedAccount {
        index,
        row: row.clone(),
        action,
        account,
        changes,
        warnings,
    }
}

/// Apply a CSV account proposal with the caller's changes, which may only turn an `add` or `update` into `skip`.
/// Rows run in order, as GnuCash runs them; one that fails now (a skipped parent, say) fails the whole apply.
pub fn apply(book: &mut Book, env: &mut dyn Env, proposal: &Proposal, changes: &[Change]) -> DomainResult<Applied> {
    let actions = actions(proposal, changes)?;
    let mut applied = Applied::default();
    proposal.accounts.iter().zip(actions).try_for_each(|(p, action)| {
        if action == ImportAction::Skip {
            applied.skipped.push(p.index);
            return Ok(());
        }
        let mut warnings = Vec::new();
        let outcome =
            import_row(book, env, &p.row, &mut warnings).map_err(|e| DomainError::invalid(format!("line {}: {e}", p.row.line)))?;
        applied
            .warnings
            .extend(warnings.into_iter().map(|(code, message)| Warning::about(p.index, code, message)));
        match outcome {
            Outcome::Added(guid) => applied.accounts_created.push(guid),
            Outcome::Updated(guid, _) => applied.accounts_updated.push(guid),
            Outcome::Unchanged(_) => applied.skipped.push(p.index),
        }
        Ok::<(), DomainError>(())
    })?;
    let mut seen: std::collections::BTreeSet<Guid> = applied.accounts_created.iter().copied().collect();
    applied.accounts_updated.retain(|g| seen.insert(*g));
    Ok(applied)
}

/// Each row's action after the caller's changes.
fn actions(proposal: &Proposal, changes: &[Change]) -> DomainResult<Vec<ImportAction>> {
    let mut actions: Vec<ImportAction> = proposal.accounts.iter().map(|a| a.action).collect();
    changes.iter().try_for_each(|c| {
        let slot = actions
            .get_mut(c.index)
            .ok_or_else(|| DomainError::invalid(format!("no account row {} in the proposal", c.index)))?;
        if c.match_split.is_some() || c.account.is_some() || c.amount.is_some() {
            return Err(DomainError::invalid(format!("account row {}: only its action can change", c.index)));
        }
        match c.action {
            None => Ok(()),
            Some(ImportAction::Skip) => {
                *slot = ImportAction::Skip;
                Ok(())
            }
            Some(a) if a == *slot => Ok(()),
            Some(a) => Err(DomainError::invalid(format!(
                "account row {}: {} can only become skip, not {}",
                c.index,
                slot.as_str(),
                a.as_str()
            ))),
        }
    })?;
    Ok(actions)
}

/// One row, as `csv_account_import`'s loop body: update the account of that full name, or create it.
fn import_row(book: &mut Book, env: &mut dyn Env, row: &AccountRow, warnings: &mut Vec<(&'static str, String)>) -> Result<Outcome, String> {
    match book.account_by_full_name(&row.full_name).map(|a| a.guid) {
        Some(guid) => Ok(update_row(book, guid, row, warnings)),
        None => add_row(book, env, row, warnings),
    }
}

/// An existing account: code, description, notes and colour from the non-empty cells (`csv-account-import.c:334-355`).
fn update_row(book: &mut Book, guid: Guid, row: &AccountRow, warnings: &mut Vec<(&'static str, String)>) -> Outcome {
    let Some(account) = book.accounts.get_mut(&guid).map(std::sync::Arc::make_mut) else {
        return Outcome::Unchanged(guid);
    };
    if let Some(w) = not_updated(account, row) {
        warnings.push(w);
    }
    let color = color_cell(&row.color, warnings);
    let wanted = [
        ("code", account.code.clone(), &row.code),
        ("description", account.description.clone(), &row.description),
    ]
    .into_iter()
    .chain([
        ("notes", account.notes().map(str::to_owned), &row.notes),
        ("color", account.stored_color().map(str::to_owned), &color),
    ]);
    let changes: Vec<FieldChange> = wanted
        .filter(|(_, from, to)| !to.is_empty() && from.as_deref() != Some(to.as_str()))
        .map(|(field, from, to)| FieldChange {
            field,
            from,
            to: to.clone(),
        })
        .collect();
    changes.iter().for_each(|c| set_field(account, c.field, &c.to));
    if changes.is_empty() {
        Outcome::Unchanged(guid)
    } else {
        Outcome::Updated(guid, changes)
    }
}

fn set_field(account: &mut Account, field: &str, value: &str) {
    match field {
        "code" => account.code = Some(value.to_owned()),
        "description" => account.description = Some(value.to_owned()),
        "notes" => account.slots.set_opt_str(red_ale_core::account::slot_paths::NOTES, Some(value)),
        _ => account.set_color(Some(value)),
    }
}

/// The cells GnuCash reads but never applies to an existing account, where they differ from it.
fn not_updated(account: &Account, row: &AccountRow) -> Option<(&'static str, String)> {
    let flag = |cell: &str, on: bool| matches!((cell, on), ("T", false) | ("F", true));
    let commodity = account
        .commodity
        .as_ref()
        .is_some_and(|c| !row.symbol.is_empty() && (c.mnemonic != row.symbol || !same_namespace(&c.namespace, &row.namespace)));
    let differing: Vec<&str> = [
        (
            "type",
            !row.account_type.is_empty() && row.account_type != account.account_type.as_str(),
        ),
        ("commodity", commodity),
        ("hidden", flag(&row.hidden, account.is_hidden())),
        ("placeholder", flag(&row.placeholder, account.is_placeholder())),
        ("tax info", flag(&row.tax, account.is_tax_related())),
    ]
    .into_iter()
    .filter_map(|(name, differs)| differs.then_some(name))
    .collect();
    (!differing.is_empty()).then(|| {
        ("not-updated", format!("GnuCash's account import changes only code, description, notes and colour; the {} differ(s) and stay(s) as the book has it", differing.join(", ")))
    })
}

fn same_namespace(a: &str, b: &str) -> bool {
    let canon = |s: &str| if s == "ISO4217" { "CURRENCY" } else { s }.to_owned();
    canon(a) == canon(b)
}

/// The colour cell to set: the cell when GnuCash can parse it, else nothing, with a warning.
fn color_cell(cell: &str, warnings: &mut Vec<(&'static str, String)>) -> String {
    if cell.is_empty() || is_gdk_color(cell) {
        return cell.to_owned();
    }
    warnings.push((
        "color",
        format!(
            "the colour {cell:?} is not one red-ale recognises (#rgb, #rrggbb, rgb(r,g,b) or rgba(r,g,b,a)); the colour is left as it was"
        ),
    ));
    String::new()
}

/// A new account, as `csv-account-import.c:244-316` creates it.
fn add_row(book: &mut Book, env: &mut dyn Env, row: &AccountRow, warnings: &mut Vec<(&'static str, String)>) -> Result<Outcome, String> {
    let parent_name =
        parent_full_name(&row.full_name, &row.name).ok_or_else(|| format!("account {} not in {}", row.name, row.full_name))?;
    let parent = if parent_name.is_empty() {
        book.root
    } else {
        book.account_by_full_name(parent_name)
            .map(|a| a.guid)
            .ok_or_else(|| format!("path to account {} not found", row.name))?
    };
    let account_type: AccountType = row
        .account_type
        .parse()
        .map_err(|_| format!("account type {:?} is not one GnuCash knows", row.account_type))?;
    let id = CommodityRef::new(row.namespace.clone(), row.symbol.clone());
    let commodity = lookup(book, &id)
        .map(|c| c.id.clone())
        .ok_or_else(|| format!("commodity {} / {} not found", row.symbol, row.namespace))?;
    let text = |s: &str| (!s.is_empty()).then(|| s.to_owned());
    let edit = AccountEdit {
        name: Some(row.name.clone()),
        account_type: Some(account_type),
        commodity: Some(commodity),
        code: text(&row.code),
        description: text(&row.description),
        notes: text(&row.notes),
        hidden: (row.hidden == "T").then_some(true),
        placeholder: (row.placeholder == "T").then_some(true),
        ..AccountEdit::default()
    };
    let guid = create_account(book, env, parent, &edit).map_err(|e| e.to_string())?;
    let color = color_cell(&row.color, warnings);
    if let Some(account) = book
        .accounts
        .get_mut(&guid)
        .map(std::sync::Arc::make_mut)
        .filter(|_| !color.is_empty())
    {
        account.set_color(Some(&color));
    }
    Ok(Outcome::Added(guid))
}

/// The parent's full name: the full name less `:` and the name, or empty for a top-level account. `None` when the
/// name is not the full name's last part. GnuCash only checks that the name occurs somewhere in the full name and
/// cuts by length, so `Assets:Bank` with the name `Ass` would look for a parent `Asse`.
fn parent_full_name<'a>(full_name: &'a str, name: &str) -> Option<&'a str> {
    if full_name == name {
        return Some("");
    }
    full_name.strip_suffix(name)?.strip_suffix(':').filter(|p| !p.is_empty())
}

/// Whether `gdk_rgba_parse` reads `text`: `#` and 3, 6, 9 or 12 hex digits, or `rgb(r,g,b)` and `rgba(r,g,b,a)` with
/// numbers or percentages. GDK's colour names are not recognised.
pub fn is_gdk_color(text: &str) -> bool {
    let text = text.trim();
    if let Some(hex) = text.strip_prefix('#') {
        return matches!(hex.len(), 3 | 6 | 9 | 12) && hex.chars().all(|c| c.is_ascii_hexdigit());
    }
    let functional = |prefix: &str, count: usize| {
        text.strip_prefix(prefix)
            .and_then(|t| t.trim_start().strip_prefix('('))
            .and_then(|t| t.strip_suffix(')'))
            .is_some_and(|inner| {
                let parts: Vec<&str> = inner.split(',').map(str::trim).collect();
                parts.len() == count && parts.iter().all(|p| is_number(p.strip_suffix('%').unwrap_or(p)))
            })
    };
    functional("rgb", 3) || functional("rgba", 4)
}

fn is_number(s: &str) -> bool {
    let digits = s.strip_prefix(['-', '+']).unwrap_or(s);
    let mut parts = digits.splitn(2, '.');
    let whole = parts.next().unwrap_or_default();
    let fraction = parts.next();
    let all_digits = |p: &str| p.chars().all(|c| c.is_ascii_digit());
    all_digits(whole) && fraction.is_none_or(all_digits) && (!whole.is_empty() || fraction.is_some_and(|f| !f.is_empty()))
}

#[cfg(test)]
mod tests;
