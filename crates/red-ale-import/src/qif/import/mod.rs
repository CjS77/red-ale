//! The QIF import as one preview and one apply (`design/api/import.md`, "QIF"): GnuCash's QIF assistant
//! (`5.17:gnucash/import-export/qif-imp/assistant-qif-import.c`) without its pages. What the user chooses on them
//! arrives in [`QifOptions`]; the steps between are the translated Scheme.
//!
//! A preview loads the files ([`read`]), builds the account, category and payee maps with their guesses and the
//! caller's choices, picks the securities, then converts everything into a copy of the Book and looks for
//! duplicates there ([`convert`]), so the proposal shows what applying makes. Applying converts again into the Book
//! itself, inside the service's Mutation, and removes the transactions the caller skipped ([`apply`]). Conversion is
//! deterministic, so the two runs make the same transactions in the same order.

pub mod apply;
pub mod convert;
pub mod read;

use std::collections::{BTreeMap, BTreeSet};

use red_ale_core::ops::commodities::lookup;
use red_ale_core::{AccountType, Book, CommodityRef, Date, DomainError, ReconcileState, Time64, TimeZone};

use super::dialog_utils::{
    SecurityChoice, get_account_name, make_account_display, make_category_display, make_memo_display, update_security_hash,
};
use super::guess_map::{GncAcctInfo, LoadedMaps, MapHash, SavedMap, account_info};
use super::objects::QifFile;
use super::parse::DateFormat;
use super::to_gnc::Maps;
use crate::matcher::ImportSettings;
use crate::pipeline::{Format, Proposal, Review};

pub use apply::apply;

/// One QIF file to import.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QifFileInput {
    /// Its text.
    pub text: String,
    /// Its name, which gives the default account (`qif-file:path-to-accountname`).
    pub path: Option<String>,
    /// The account for its transactions read under no `!Account`: the assistant's account page.
    pub default_account: Option<String>,
}

/// A new security as the caller sets it: the assistant's commodity page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecurityOverride {
    /// Namespace.
    pub namespace: String,
    /// Mnemonic.
    pub mnemonic: String,
    /// Full name.
    pub fullname: String,
}

/// What the caller chose on the assistant's pages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QifOptions {
    /// The date format, when the dates are ambiguous.
    pub date_format: Option<DateFormat>,
    /// The default currency.
    pub currency: CommodityRef,
    /// The reconcile state of a transaction with no `C` line.
    pub status_pref: ReconcileState,
    /// QIF account name → full name.
    pub accounts: BTreeMap<String, String>,
    /// QIF category → full name.
    pub categories: BTreeMap<String, String>,
    /// QIF payee or memo → full name.
    pub payees: BTreeMap<String, String>,
    /// QIF security name → the commodity to create.
    pub securities: BTreeMap<String, SecurityOverride>,
    /// The QIF account map the Configuration holds.
    pub saved: SavedMap,
}

/// Why a QIF preview can't be made.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum QifError {
    /// A file can't be read as QIF at all.
    #[error("{message}")]
    File {
        /// The file's index.
        file: usize,
        /// The line, when one is at fault.
        line: Option<usize>,
        /// `qif`, `format` or `date-format`.
        reason: &'static str,
        /// What is wrong.
        message: String,
    },
    /// A choice the import refuses, or a conversion that failed.
    #[error(transparent)]
    Domain(#[from] DomainError),
}

/// One row of a map, as the assistant's account, category and payee pages show it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapRow {
    /// The QIF name.
    pub qif_name: String,
    /// The account's full name.
    pub account: String,
    /// The types it may have.
    pub types: Vec<AccountType>,
    /// True when the account does not exist yet.
    pub new: bool,
    /// True when neither the saved map nor the caller named it.
    pub guessed: bool,
}

/// One security the import uses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecurityRow {
    /// The QIF security name.
    pub name: String,
    /// Its commodity.
    pub id: CommodityRef,
    /// The full name of a commodity to create.
    pub fullname: Option<String>,
    /// True when applying creates the commodity.
    pub new: bool,
}

/// A commodity applying creates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewCommodity {
    /// Its ID.
    pub id: CommodityRef,
    /// Its full name.
    pub fullname: String,
    /// Its fraction.
    pub fraction: i64,
}

/// What a QIF proposal keeps beyond the transactions: everything applying needs to convert again.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct QifState {
    /// The date formats to choose between; while there are some, the proposal holds no transactions.
    pub date_formats: Vec<DateFormat>,
    /// The files, loaded, without their error rows.
    pub files: Vec<QifFile>,
    /// The maps, guessed and chosen.
    pub maps: Maps,
    /// The account rows.
    pub accounts: Vec<MapRow>,
    /// The category rows.
    pub categories: Vec<MapRow>,
    /// The payee rows.
    pub payees: Vec<MapRow>,
    /// QIF security name → commodity.
    pub security_hash: BTreeMap<String, SecurityChoice>,
    /// The securities the import uses.
    pub securities: Vec<SecurityRow>,
    /// Commodities applying creates.
    pub new_commodities: Vec<NewCommodity>,
    /// The default currency.
    pub currency: Option<CommodityRef>,
    /// The reconcile state of a transaction with no `C` line.
    pub status_pref: ReconcileState,
    /// The full names of the accounts the preview's copy made, by their GUID there: the split accounts the proposal
    /// shows that are still to be created.
    pub new_account_names: BTreeMap<red_ale_core::Guid, String>,
    /// What applying as proposed does.
    pub review: Review,
}

/// Preview QIF files: load them, build the maps and securities, convert into a copy of `book`, find duplicates.
pub fn propose(
    book: &Book,
    now: Time64,
    zone: &TimeZone,
    inputs: &[QifFileInput],
    opts: &QifOptions,
    threshold: Option<Date>,
) -> Result<Proposal, QifError> {
    if inputs.is_empty() {
        return Err(DomainError::invalid("qif takes one or more files").into());
    }
    let loaded = read::load(inputs, opts.date_format)?;
    let mut proposal = Proposal {
        ignored_lines: loaded.ignored,
        errors: loaded.errors,
        warnings: loaded.warnings,
        ..Proposal::empty(Format::Qif, ImportSettings::default(), threshold)
    };
    if !loaded.date_formats.is_empty() {
        let names: Vec<&str> = loaded.date_formats.iter().map(|f| f.symbol()).collect();
        let message = format!("the dates are ambiguous: preview again with dateFormat one of {}", names.join(", "));
        proposal.warnings.push(crate::pipeline::Warning {
            code: "date-format",
            message,
            index: None,
        });
        proposal.qif = Some(Box::new(QifState {
            date_formats: loaded.date_formats,
            ..QifState::default()
        }));
        return Ok(proposal);
    }
    let mut state = choose(book, loaded.files, &loaded.ticker_map, opts)?;
    let preview = convert::preview(book, now, zone, &state, threshold)?;
    proposal.transactions = preview.transactions;
    state.new_commodities = preview.new_commodities;
    state.new_account_names = preview.new_account_names;
    state.review = preview.review;
    proposal.qif = Some(Box::new(state));
    Ok(proposal)
}

/// The maps and securities, guessed, then as the caller chose.
fn choose(book: &Book, files: Vec<QifFile>, ticker_map: &super::objects::TickerMap, opts: &QifOptions) -> Result<QifState, QifError> {
    let loaded = opts.saved.load(book);
    let info = account_info(book);
    let mut maps = Maps {
        accounts: loaded.accounts.clone(),
        categories: loaded.categories.clone(),
        payees: loaded.payees.clone(),
    };
    let shown = display(&files, &mut maps, &info);
    override_map(&mut maps.accounts, &shown[0], &opts.accounts, "account")?;
    override_map(&mut maps.categories, &shown[1], &opts.categories, "category")?;
    override_map(&mut maps.payees, &shown[2], &opts.payees, "payee")?;
    let shown = display(&files, &mut maps, &info);
    let rows = |map: &MapHash, keys: &[String], saved: &MapHash, chosen: &BTreeMap<String, String>| -> Vec<MapRow> {
        keys.iter()
            .filter_map(|k| map.get(k).map(|e| map_row(k, e, saved.contains_key(k) || chosen.contains_key(k))))
            .collect()
    };
    let (security_hash, securities) = securities(book, &maps, ticker_map, &loaded, opts)?;
    Ok(QifState {
        accounts: rows(&maps.accounts, &shown[0], &loaded.accounts, &opts.accounts),
        categories: rows(&maps.categories, &shown[1], &loaded.categories, &opts.categories),
        payees: rows(&maps.payees, &shown[2], &loaded.payees, &opts.payees),
        files,
        maps,
        security_hash,
        securities,
        currency: Some(opts.currency.clone()),
        status_pref: opts.status_pref,
        ..QifState::default()
    })
}

/// `qif-dialog:make-account-display`, `make-category-display` and `make-memo-display`: the keys each shows.
fn display(files: &[QifFile], maps: &mut Maps, info: &[GncAcctInfo]) -> [Vec<String>; 3] {
    [
        make_account_display(files, &mut maps.accounts, info),
        make_category_display(files, &mut maps.categories, info),
        make_memo_display(files, &mut maps.payees, info),
    ]
}

fn map_row(key: &str, e: &super::objects::QifMapEntry, named: bool) -> MapRow {
    MapRow {
        qif_name: e.qif_name.clone().unwrap_or_else(|| key.to_owned()),
        account: e.gnc_name.clone().unwrap_or_default(),
        types: e.allowed_types.clone(),
        new: e.new_acct,
        guessed: !named,
    }
}

/// The caller's choice of account for shown names: `qif-map-entry:set-gnc-name!` from the account picker
/// (`5.17:gnucash/import-export/qif-imp/assistant-qif-import.c:620-690`).
fn override_map(map: &mut MapHash, shown: &[String], chosen: &BTreeMap<String, String>, what: &str) -> Result<(), QifError> {
    chosen.iter().try_for_each(|(qif_name, full_name)| {
        let entry = map
            .get_mut(qif_name)
            .filter(|_| shown.contains(qif_name))
            .ok_or_else(|| DomainError::invalid(format!("the files use no QIF {what} {qif_name:?}")))?;
        if full_name.split(super::dialog_utils::SEP).any(str::is_empty) {
            return Err(DomainError::invalid(format!("{full_name:?} is not an account's full name")).into());
        }
        entry.gnc_name = Some(full_name.clone());
        Ok(())
    })
}

/// `qif-import:update-security-hash` over the saved securities, then the caller's choices for the new ones.
fn securities(
    book: &Book,
    maps: &Maps,
    ticker_map: &super::objects::TickerMap,
    loaded: &LoadedMaps,
    opts: &QifOptions,
) -> Result<(BTreeMap<String, SecurityChoice>, Vec<SecurityRow>), QifError> {
    let mut hash: BTreeMap<String, SecurityChoice> = loaded
        .securities
        .iter()
        .map(|(n, c)| (n.clone(), SecurityChoice::Existing(c.clone())))
        .collect();
    let existing = |full_name: &str| book.account_by_full_name(full_name).map(|a| (a.account_type, a.commodity.clone()));
    let new_names = update_security_hash(&mut hash, ticker_map, &maps.accounts, &loaded.security_prefs, existing);
    opts.securities.iter().try_for_each(|(name, s)| {
        if !new_names.contains(name) {
            return Err(DomainError::invalid(format!("{name:?} is not a new security of this import")));
        }
        if [&s.namespace, &s.mnemonic, &s.fullname].iter().any(|t| t.trim().is_empty()) {
            return Err(DomainError::invalid(format!(
                "security {name:?} needs a namespace, a mnemonic and a full name"
            )));
        }
        let id = CommodityRef::new(s.namespace.trim(), s.mnemonic.trim());
        hash.insert(
            name.clone(),
            SecurityChoice::New {
                id,
                fullname: s.fullname.clone(),
            },
        );
        Ok(())
    })?;
    let used = used_securities(&maps.accounts);
    let rows = hash
        .iter()
        .filter(|(name, _)| used.contains(name.as_str()))
        .map(|(name, choice)| security_row(book, name, choice))
        .collect();
    Ok((hash, rows))
}

/// The security names of the shown stock and mutual fund accounts.
fn used_securities(accounts: &MapHash) -> BTreeSet<&str> {
    accounts
        .iter()
        .filter(|(_, e)| {
            e.display
                && e.allowed_types
                    .iter()
                    .any(|t| matches!(t, AccountType::Stock | AccountType::Mutual))
        })
        .map(|(k, _)| get_account_name(k))
        .collect()
}

fn security_row(book: &Book, name: &str, choice: &SecurityChoice) -> SecurityRow {
    let fullname = match choice {
        SecurityChoice::New { fullname, .. } => Some(fullname.clone()),
        SecurityChoice::Existing(_) => None,
    };
    let new = fullname.is_some() && lookup(book, choice.id()).is_none();
    SecurityRow {
        name: name.to_owned(),
        id: choice.id().clone(),
        fullname: fullname.filter(|_| new),
        new,
    }
}
