//! The QIF import's preview and its account map (`design/api/import.md`, "QIF"): the files decoded, the default
//! currency and the saved QIF account map read from the Configuration, and the map saved back after an apply, as
//! GnuCash saves its `qif-accounts-map` file when an import completes
//! (`5.17:gnucash/import-export/qif-imp/assistant-qif-import.c:3527-3535`).

use std::collections::BTreeMap;

use red_ale_core::ops::commodities::lookup;
use red_ale_core::{Book, CommodityRef, DomainError, Env, ReconcileState};
use red_ale_import::pipeline::{Proposal, review};
use red_ale_import::qif::DateFormat;
use red_ale_import::qif::guess_map::SavedMap;
use red_ale_import::qif::import::{QifError, QifFileInput, QifOptions, SecurityOverride, propose as qif_propose};

use super::file::{ImportFile, decode};
use crate::ServiceError;
use crate::config::{Config, ConfigStore, invalid};

/// Where the Configuration keeps the QIF account map.
const MAP_SETTING: &str = "import.qifAccountMap";

/// The params of a `qif` preview.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct QifParams {
    /// Each file's account for its transactions read under no `!Account`, by the file's index.
    pub default_accounts: Vec<Option<String>>,
    /// The date format, when the dates are ambiguous.
    pub date_format: Option<DateFormat>,
    /// The default currency; the Configuration's `defaultCurrency` when `None`.
    pub currency: Option<CommodityRef>,
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
}

/// Read the files and build the proposal.
pub(super) fn propose(
    book: &Book,
    env: &mut dyn Env,
    config: &Config,
    files: &[ImportFile],
    p: &QifParams,
) -> Result<Proposal, ServiceError> {
    let inputs = files
        .iter()
        .enumerate()
        .map(|(i, f)| {
            let default_account = p.default_accounts.get(i).cloned().flatten();
            Ok(QifFileInput {
                text: decode(i, f, "utf-8")?,
                path: f.name.clone(),
                default_account,
            })
        })
        .collect::<Result<Vec<_>, ServiceError>>()?;
    let opts = QifOptions {
        date_format: p.date_format,
        currency: currency(book, config, p.currency.as_ref())?,
        status_pref: p.status_pref,
        accounts: p.accounts.clone(),
        categories: p.categories.clone(),
        payees: p.payees.clone(),
        securities: p.securities.clone(),
        saved: saved_map(config)?,
    };
    let today = env.today().ok_or_else(|| invalid("the session's clock is outside the calendar"))?;
    let now = env.now();
    qif_propose(book, now, env.zone(), &inputs, &opts, review::threshold(book, Some(today))).map_err(|e| match e {
        QifError::File {
            file,
            line,
            reason,
            message,
        } => ServiceError::ImportFile {
            file,
            line,
            reason,
            message,
        },
        QifError::Domain(e) => ServiceError::Domain(e),
    })
}

/// The caller's currency, which must be one the Book knows, else GnuCash's default currency.
fn currency(book: &Book, config: &Config, given: Option<&CommodityRef>) -> Result<CommodityRef, ServiceError> {
    let Some(id) = given else { return config.default_currency(book) };
    let found = lookup(book, id).ok_or_else(|| DomainError::not_found("commodity", id))?;
    if !found.id.is_currency() {
        return Err(invalid(format!("{id} is not a currency")));
    }
    Ok(found.id.clone())
}

/// The QIF account map the Configuration holds.
pub fn saved_map(config: &Config) -> Result<SavedMap, ServiceError> {
    serde_json::from_value(config.checked(MAP_SETTING)?).map_err(|e| invalid(format!("the Configuration's {MAP_SETTING}: {e}")))
}

/// Save the map an import used over the one the Configuration holds: `qif-import:save-map-prefs`
/// (`5.17:gnucash/import-export/qif-imp/qif-guess-map.scm:304-344`).
pub fn save_map(store: &ConfigStore, used: &SavedMap) -> Result<(), ServiceError> {
    let lock = store.lock()?;
    let mut config = store.load()?;
    let mut saved = saved_map(&config)?;
    saved.extend(used.clone());
    let value = serde_json::to_value(&saved).map_err(|e| invalid(format!("the QIF account map: {e}")))?;
    config
        .apply(&[(MAP_SETTING.to_owned(), value)])
        .map_err(|e| invalid(e.to_string()))?;
    store.save(&lock, &config)
}
