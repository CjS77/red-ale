//! Business import: invoices and bills (`csvDocuments`), customers and vendors (`csvOwners`), as GnuCash's Import
//! Bills & Invoices and Import Customers & Vendors read them.
//!
//! Translated from `5.17:gnucash/import-export/bi-import/` and `5.17:gnucash/import-export/customer-import/`
//! (GPL-2.0-or-later; copyright lines and line citations are in each module). The dialogs are not translated: what the
//! user chooses there arrives as [`DocumentOptions`] and [`OwnerOptions`]; GnuCash's question whether to update
//! existing documents is answered up front (`updateExisting`). Spec: `design/api/import.md`, "Business documents" and
//! "Customers and vendors".
//!
//! | Module | GnuCash source |
//! |--------|----------------|
//! | [`reader`] | `gnc_bi_import_read_file`, `gnc_customer_import_read_file`, the dialogs' regular expressions |
//! | [`date`] | `isDateValid` (`dialog-bi-import-helper.c`), `qof_scan_date` (`libgnucash/engine/gnc-date.cpp`) |
//! | [`expr`] | `gnc_exp_parser_parse` (`libgnucash/app-utils/gnc-exp-parser.c`) |
//! | [`documents`] | `gnc_bi_import_fix_bis`, `gnc_bi_import_create_bis` (`dialog-bi-import.c`), `dialog-bi-import-helper.c` |
//! | [`owners`] | `gnc_customer_import_fix_customers`, `gnc_customer_import_create_customers` (`dialog-customer-import.c`) |
//!
//! Neither importer uses the matcher: records are found by ID (`gncIDSearch.c`). A preview works each record out on
//! a copy of the Book with the same code [`apply`] runs, so the proposal shows what applying does: a record the
//! Book's rules refuse is an error row, and a document that can't be posted says why.

pub mod date;
pub mod documents;
pub mod expr;
pub mod owners;
pub mod reader;

pub use date::DateFormat;
pub use documents::{DocumentOptions, PostPlan, ProposedDocument, ProposedEntry, Totals};
pub use owners::{OwnerFields, OwnerOptions, ProposedOwner};
pub use reader::Separator;

use std::collections::BTreeMap;

use red_ale_core::{Book, Date, DomainError, DomainResult, Env, Guid, Time64, TimeZone};
use red_ale_locale::Locale;

use crate::matcher::ImportAction;
use crate::pipeline::{Change, Warning};
use expr::NumberFormat;

/// What a business proposal holds: its records, by format.
#[derive(Debug, Clone, PartialEq)]
pub enum Records {
    /// `csvDocuments`.
    Documents(Vec<ProposedDocument>),
    /// `csvOwners`.
    Owners(Vec<ProposedOwner>),
}

impl Records {
    /// How many records there are.
    pub fn len(&self) -> usize {
        match self {
            Self::Documents(d) => d.len(),
            Self::Owners(o) => o.len(),
        }
    }

    /// True when there are none.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// What a business preview reads besides the file: the Book, the Locale and the session's clock.
#[derive(Debug, Clone, Copy)]
pub struct Context<'a> {
    /// The Book.
    pub book: &'a Book,
    /// The Locale: monetary separators and the `locale` date format.
    pub locale: &'a Locale,
    /// Today, in the session's time zone.
    pub today: Date,
    /// Now.
    pub now: Time64,
    /// The session's time zone.
    pub zone: &'a TimeZone,
    /// The file's index among the import files.
    pub file: usize,
    /// The auto read-only threshold date, if any.
    pub threshold: Option<Date>,
}

impl Context<'_> {
    /// The Locale's monetary decimal point and group separator, with `gnc_localeconv`'s defaults.
    fn numbers(&self) -> NumberFormat {
        let m = self.locale.monetary();
        let first = |s: &str, default: char| s.chars().next().unwrap_or(default);
        NumberFormat { decimal_point: first(m.mon_decimal_point, '.'), group_separator: first(m.mon_thousands_sep, ',') }
    }
}

/// How account names in a file are read: GnuCash's "show leaf account names" preference, which
/// `gnc_account_lookup_for_register` follows (`5.17:libgnucash/app-utils/gnc-ui-util.cpp:448-459`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AccountNames {
    /// Full names, `Expenses:Office` (the preference's default).
    #[default]
    Full,
    /// Leaf names, found breadth first (`gnc_account_lookup_by_name`, `5.17:libgnucash/engine/Account.cpp:3080-3084`).
    Leaf,
}

impl AccountNames {
    /// The account a file names.
    pub fn lookup(self, book: &Book, name: &str) -> Option<Guid> {
        match self {
            Self::Full => book.account_by_full_name(name).filter(|a| a.guid != book.root).map(|a| a.guid),
            Self::Leaf => lookup_by_name(book, book.root, name),
        }
    }
}

/// `account_foreach_descendant_breadthfirst_until` with a name test: the children first, then each child's
/// descendants the same way (`5.17:libgnucash/engine/Account.cpp:3052-3084`).
fn lookup_by_name(book: &Book, parent: Guid, name: &str) -> Option<Guid> {
    book.children(parent).find(|c| c.name == name).map(|c| c.guid).or_else(|| book.children(parent).find_map(|c| lookup_by_name(book, c.guid, name)))
}

/// What applying a business proposal did, by record index.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RecordsApplied {
    /// Records created.
    pub created: Vec<(usize, Guid)>,
    /// Existing records updated.
    pub updated: Vec<(usize, Guid)>,
    /// Documents posted.
    pub posted: Vec<(usize, Guid)>,
    /// Indexes skipped.
    pub skipped: Vec<usize>,
}

/// The actions the caller's changes leave: only `skip`, or for an Owner header row the action GnuCash would take.
fn chosen_actions(actions: &[(ImportAction, Option<ImportAction>)], changes: &[Change]) -> DomainResult<Vec<ImportAction>> {
    let mut out: Vec<ImportAction> = actions.iter().map(|(a, _)| *a).collect();
    changes.iter().try_for_each(|c| {
        let (_, alternative) = actions.get(c.index).ok_or_else(|| DomainError::invalid(format!("no record {} in the proposal", c.index)))?;
        if c.match_split.is_some() || c.account.is_some() || c.amount.is_some() {
            return Err(DomainError::invalid(format!("record {}: business imports take only an action", c.index)));
        }
        match c.action {
            None => Ok(()),
            Some(ImportAction::Skip) => {
                out[c.index] = ImportAction::Skip;
                Ok(())
            }
            Some(a) if Some(a) == *alternative => {
                out[c.index] = a;
                Ok(())
            }
            Some(a) => Err(DomainError::invalid(format!("record {} can only be skipped, not changed to {}", c.index, a.as_str()))),
        }
    })?;
    Ok(out)
}

/// A record that updates what an earlier record creates can't stay when that one is skipped.
fn check_dependencies(same_as: &[Option<usize>], actions: &[ImportAction]) -> DomainResult<()> {
    same_as.iter().enumerate().try_for_each(|(i, dep)| match dep {
        Some(d) if actions[i] != ImportAction::Skip && actions.get(*d) == Some(&ImportAction::Skip) => {
            Err(DomainError::invalid(format!("record {i} updates what record {d} creates; skip both or neither")))
        }
        _ => Ok(()),
    })
}

/// Apply business records with the caller's changes. The caller holds `book` inside one Mutation.
pub fn apply(book: &mut Book, env: &mut dyn Env, records: &Records, changes: &[Change], threshold: Option<Date>) -> DomainResult<(RecordsApplied, Vec<Warning>)> {
    match records {
        Records::Documents(docs) => {
            let actions = chosen_actions(&docs.iter().map(|d| (d.action, None)).collect::<Vec<_>>(), changes)?;
            check_dependencies(&docs.iter().map(|d| d.same_as).collect::<Vec<_>>(), &actions)?;
            documents::apply_all(book, env, docs, &actions, threshold)
        }
        Records::Owners(owners) => {
            let actions = chosen_actions(&owners.iter().map(|o| (o.action, o.alternative)).collect::<Vec<_>>(), changes)?;
            check_dependencies(&owners.iter().map(|o| o.same_as).collect::<Vec<_>>(), &actions)?;
            owners::apply_all(book, env, owners, &actions)
        }
    }
}

/// Records created earlier in the same run, by index, for records that update them.
type Created = BTreeMap<usize, Guid>;
