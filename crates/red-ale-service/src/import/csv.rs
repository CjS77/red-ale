//! The CSV transaction import's preview: the file through GnuCash's CSV importer and the matcher, against the open
//! Book (`design/api/import.md`, "CSV transactions").

use std::collections::BTreeMap;

use red_ale_core::{Book, Env, Guid};
use red_ale_import::csv::build::{BuildContext, CsvBuild, build};
use red_ale_import::csv::{BaseAccount, DateContext, MonetaryLocale, ParseContext, TxImport, TxLayout, locale_parser, tokenize};
use red_ale_import::matcher::{ImportSettings, ImportedTransaction, match_batch};
use red_ale_import::pipeline::{CsvState, Format, Proposal, ProposedTransaction, Warning, review};
use red_ale_locale::Locale;

use super::file::{ImportFile, decode};
use super::settings::LayoutChoice;
use crate::ServiceError;
use crate::config::{Config, invalid};

/// The params of a `csvTransactions` preview.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CsvParams {
    /// The layout.
    pub layout: LayoutChoice,
    /// Overrides the layout's base account.
    pub base_account: Option<Guid>,
    /// What each account name the file uses means: GnuCash's account match page.
    pub account_names: BTreeMap<String, Guid>,
}

/// What a preview reads from the Configuration.
pub(super) struct Choices {
    pub layout: TxLayout,
    pub settings: ImportSettings,
    pub locale: Locale,
}

impl Choices {
    pub(super) fn read(config: &Config, layout: &LayoutChoice) -> Result<Self, ServiceError> {
        Ok(Self {
            layout: config.csv_layout(layout)?,
            settings: config.import_settings()?,
            locale: config.locale()?,
        })
    }
}

/// The base account: the caller's, else the layout's by GUID then full name, as GnuCash loads it
/// (`5.17:gnucash/import-export/csv-imp/gnc-imp-settings-csv-tx.cpp:209-247`).
fn base_account(book: &Book, layout: &TxLayout, given: Option<Guid>) -> Result<Option<Guid>, ServiceError> {
    if given.is_some() || layout.multi_split {
        return Ok(given);
    }
    let Some(base) = &layout.base_account else { return Ok(None) };
    let by_guid = base
        .guid
        .as_deref()
        .and_then(|g| Guid::parse(g).ok())
        .filter(|g| book.account(*g).is_some());
    let by_name = || base.full_name.as_deref().and_then(|n| book.account_by_full_name(n)).map(|a| a.guid);
    by_guid.or_else(by_name).map(Some).ok_or_else(|| {
        let name = base.full_name.clone().or_else(|| base.guid.clone()).unwrap_or_default();
        ServiceError::Domain(red_ale_core::DomainError::not_found("account", &name))
    })
}

/// The layout with the caller's base account in place of its own, as choosing the account on GnuCash's assistant page
/// sets the parser's (`5.17:gnucash/import-export/csv-imp/gnc-import-tx.cpp:180-209`). A layout without one would
/// otherwise parse as if no base account were given.
fn with_base_account(layout: &TxLayout, given: Option<Guid>) -> TxLayout {
    let mut layout = layout.clone();
    if let Some(g) = given {
        layout.base_account = Some(BaseAccount {
            guid: Some(g.to_string()),
            full_name: None,
        });
    }
    layout
}

/// Read the file and build the proposal.
pub(super) fn propose(
    book: &Book,
    env: &mut dyn Env,
    files: &[ImportFile],
    params: &CsvParams,
    choices: &Choices,
) -> Result<Proposal, ServiceError> {
    let [file] = files else {
        return Err(invalid(format!("csvTransactions takes exactly one file, not {}", files.len())));
    };
    let layout = &with_base_account(&choices.layout, params.base_account);
    let text = decode(0, file, &layout.csv.encoding)?;
    let rows = tokenize(&text, &layout.csv).map_err(|e| ServiceError::ImportFile {
        file: 0,
        line: None,
        reason: "tokenize",
        message: e.to_string(),
    })?;
    let base = base_account(book, layout, params.base_account)?;
    let today = env.today().ok_or_else(|| invalid("the session's clock is outside the calendar"))?;
    let monetary = choices.locale.monetary();
    let locale_date = locale_parser(&choices.locale, today);
    let ctx = ParseContext {
        dates: DateContext {
            today,
            locale_date: Some(&locale_date),
        },
        monetary: MonetaryLocale::from_lconv(
            monetary.negative_sign,
            monetary.mon_decimal_point,
            monetary.mon_thousands_sep,
            monetary.positive_sign,
        ),
    };
    let import = TxImport::parse(rows, layout, &ctx);
    let zone = env.zone().clone();
    let now = env.now();
    let build_ctx = BuildContext {
        book,
        zone: &zone,
        now,
        base_account: base,
        account_names: &params.account_names,
        file: 0,
    };
    let built = build(import, &build_ctx, || env.new_guid()).map_err(|e| ServiceError::ImportFile {
        file: 0,
        line: e.line,
        reason: "columns",
        message: e.messages.join(" "),
    })?;
    matched(book, built, choices, review::threshold(book, Some(today)))
}

/// Hand the built transactions to the matcher, as GnuCash's assistant does (`assistant-csv-trans-import.cpp:2190-2213`).
fn matched(book: &Book, built: CsvBuild, choices: &Choices, threshold: Option<red_ale_core::Date>) -> Result<Proposal, ServiceError> {
    let CsvBuild {
        account_names,
        errors,
        ignored,
        transactions,
    } = built;
    let sources: Vec<_> = transactions.iter().map(|b| b.source).collect();
    let imported: Vec<ImportedTransaction> = transactions.into_iter().map(|b| b.imported).collect();
    let batch = match_batch(book, &choices.settings, imported, &choices.locale)?;
    let warnings = batch
        .duplicates
        .iter()
        .map(|i| Warning::about(*i, "online-id", "left out: its Online ID is already on a split in its account"))
        .collect();
    let transactions = batch
        .infos
        .into_iter()
        .filter_map(|info| sources.get(info.index).map(|source| ProposedTransaction { source: *source, info }))
        .collect();
    Ok(Proposal {
        ignored_lines: ignored,
        errors,
        warnings,
        transactions,
        csv: Some(CsvState { account_names }),
        ..Proposal::empty(Format::CsvTransactions, choices.settings.clone(), threshold)
    })
}
