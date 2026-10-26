//! The CSV price import's preview: the file through GnuCash's price importer, against the open Book
//! (`design/api/import.md`, "CSV prices").

use red_ale_core::{Book, Env};
use red_ale_import::csv::prices::{self, Defaults, PriceContext, PriceImport};
use red_ale_import::csv::{DateContext, MonetaryLocale, PriceLayout, locale_parser, tokenize};
use red_ale_import::matcher::ImportSettings;
use red_ale_import::pipeline::{Format, Proposal};
use red_ale_locale::Locale;

use super::file::{ImportFile, decode};
use super::settings::PriceLayoutChoice;
use crate::ServiceError;
use crate::config::{Config, invalid};

/// The params of a `csvPrices` preview.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PricesParams {
    /// The layout.
    pub layout: PriceLayoutChoice,
    /// GnuCash's "Allow existing prices to be over written": a same-day price of the pair is replaced rather than
    /// kept. GnuCash does not save it with the layout.
    pub overwrite: bool,
}

/// What a preview reads from the Configuration.
pub(super) struct Choices {
    layout: PriceLayout,
    settings: ImportSettings,
    locale: Locale,
}

impl Choices {
    pub(super) fn read(config: &Config, layout: &PriceLayoutChoice) -> Result<Self, ServiceError> {
        Ok(Self { layout: config.csv_price_layout(layout)?, settings: config.import_settings()?, locale: config.locale()? })
    }
}

/// Read the file and build the proposal.
pub(super) fn propose(book: &Book, env: &mut dyn Env, files: &[ImportFile], params: &PricesParams, choices: &Choices) -> Result<Proposal, ServiceError> {
    let [file] = files else { return Err(invalid(format!("csvPrices takes exactly one file, not {}", files.len()))) };
    let layout = &choices.layout;
    let text = decode(0, file, &layout.csv.encoding)?;
    let rows = tokenize(&text, &layout.csv).map_err(|e| ServiceError::ImportFile { file: 0, line: None, reason: "tokenize", message: e.to_string() })?;
    let defaults = Defaults::resolve(book, layout)?;
    let today = env.today().ok_or_else(|| invalid("the session's clock is outside the calendar"))?;
    let locale_date = locale_parser(&choices.locale, today);
    let monetary = choices.locale.monetary();
    let ctx = PriceContext {
        date_format: layout.csv.date_format,
        currency_format: layout.csv.currency_format,
        dates: DateContext { today, locale_date: Some(&locale_date) },
        monetary: MonetaryLocale::from_lconv(monetary.negative_sign, monetary.mon_decimal_point, monetary.mon_thousands_sep, monetary.positive_sign),
    };
    let import = PriceImport::parse(book, rows, layout, &defaults, &ctx);
    let problems = import.file_errors(&defaults);
    if !problems.is_empty() {
        return Err(ServiceError::ImportFile { file: 0, line: None, reason: "columns", message: problems.join(" ") });
    }
    let (records, errors, ignored_lines) = prices::propose(book, env.zone(), &import, params.overwrite, 0);
    Ok(Proposal { ignored_lines, errors, prices: records, ..Proposal::empty(Format::CsvPrices, choices.settings.clone(), None) })
}
