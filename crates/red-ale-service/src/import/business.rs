//! The business imports' previews: invoices and bills, customers and vendors, against the open Book
//! (`design/api/import.md`, "Business documents", "Customers and vendors").

use red_ale_core::business::OwnerKind;
use red_ale_core::ops::business::documents::DocumentKind;
use red_ale_core::{Book, Env};
use red_ale_import::business::{self, AccountNames, Context, DateFormat, DocumentOptions, OwnerOptions, Records, Separator};
use red_ale_import::matcher::ImportSettings;
use red_ale_import::pipeline::{Format, Proposal, review};
use red_ale_locale::Locale;

use super::file::{ImportFile, decode};
use crate::ServiceError;
use crate::config::{Config, invalid};

/// The params of a `csvDocuments` preview.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DocumentsParams {
    /// Bills or invoices.
    pub kind: DocumentKind,
    /// How fields are separated.
    pub separator: Separator,
    /// The date format; `None` takes `reports.dateFormat`.
    pub date_format: Option<DateFormat>,
    /// Full or leaf account names.
    pub account_names: AccountNames,
    /// Update existing unposted documents.
    pub update_existing: bool,
    /// Apply open payments after posting.
    pub auto_pay: bool,
}

/// The params of a `csvOwners` preview.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OwnersParams {
    /// Customers or vendors.
    pub kind: OwnerKind,
    /// How fields are separated.
    pub separator: Separator,
}

/// What a business preview reads from the Configuration.
pub(super) struct Choices {
    locale: Locale,
    date_format: DateFormat,
}

impl Choices {
    pub(super) fn read(config: &Config) -> Result<Self, ServiceError> {
        let name = config.checked("reports.dateFormat")?;
        let date_format =
            name.as_str().and_then(DateFormat::from_name).ok_or_else(|| invalid(format!("reports.dateFormat {name} is not a date format")))?;
        Ok(Self { locale: config.locale()?, date_format })
    }
}

/// The one file's text.
fn one_file(files: &[ImportFile], format: &str) -> Result<String, ServiceError> {
    let [file] = files else { return Err(invalid(format!("{format} takes exactly one file, not {}", files.len()))) };
    decode(0, file, "utf-8")
}

/// A proposal holding business records.
fn proposal(book: &Book, format: Format, today: red_ale_core::Date, records: Records, parts: business::documents::Proposed) -> Proposal {
    Proposal {
        format,
        settings: ImportSettings::default(),
        threshold: review::threshold(book, Some(today)),
        ignored_lines: parts.ignored,
        errors: parts.errors,
        warnings: parts.warnings,
        transactions: Vec::new(),
        csv: None,
        business: Some(records),
    }
}

/// Read a `csvDocuments` file and build the proposal.
pub(super) fn propose_documents(
    book: &Book,
    env: &mut dyn Env,
    files: &[ImportFile],
    params: DocumentsParams,
    choices: &Choices,
) -> Result<Proposal, ServiceError> {
    let text = one_file(files, Format::CsvDocuments.as_str())?;
    let today = env.today().ok_or_else(|| invalid("the session's clock is outside the calendar"))?;
    let now = env.now();
    let zone = env.zone().clone();
    let ctx = Context { book, locale: &choices.locale, today, now, zone: &zone, file: 0, threshold: review::threshold(book, Some(today)) };
    let opts = DocumentOptions {
        kind: params.kind,
        separator: params.separator,
        date_format: params.date_format.unwrap_or(choices.date_format),
        account_names: params.account_names,
        update_existing: params.update_existing,
        auto_pay: params.auto_pay,
    };
    let mut found = business::documents::propose(&text, &ctx, &opts).map_err(|e| invalid(e.to_string()))?;
    let documents = std::mem::take(&mut found.documents);
    Ok(proposal(book, Format::CsvDocuments, today, Records::Documents(documents), found))
}

/// Read a `csvOwners` file and build the proposal. New Owners take `defaultCurrency`.
pub(super) fn propose_owners(
    book: &Book,
    env: &mut dyn Env,
    files: &[ImportFile],
    params: OwnersParams,
    choices: &Choices,
    config: &Config,
) -> Result<Proposal, ServiceError> {
    let text = one_file(files, Format::CsvOwners.as_str())?;
    let today = env.today().ok_or_else(|| invalid("the session's clock is outside the calendar"))?;
    let now = env.now();
    let zone = env.zone().clone();
    let ctx = Context { book, locale: &choices.locale, today, now, zone: &zone, file: 0, threshold: None };
    let opts = OwnerOptions { kind: params.kind, separator: params.separator, currency: config.default_currency(book)? };
    let found = business::owners::propose(&text, &ctx, &opts).map_err(|e| invalid(e.to_string()))?;
    let parts =
        business::documents::Proposed { documents: Vec::new(), errors: found.errors, ignored: found.ignored, warnings: found.warnings };
    Ok(proposal(book, Format::CsvOwners, today, Records::Owners(found.owners), parts))
}
