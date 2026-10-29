//! Option declarations and values: GnuCash's `GncOption` value classes (`5.17:libgnucash/engine/gnc-option-impl.hpp`)
//! for the option types reports and style sheets use.

use std::fmt;

use red_ale_core::time::{PeriodBounds, RelativeDate, relative_date};
use red_ale_core::{AccountType, CommodityRef, Guid, Time64, TimeZone};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::number::Rational;
use crate::result::PlotSize;

/// The earliest instant GnuCash accepts for a date option, exclusive (`MINTIME`, `5.17:libgnucash/engine/gnc-datetime.hpp:55`).
pub const MIN_TIME: i64 = -17_987_443_200;
/// The latest instant GnuCash accepts for a date option, exclusive (`MAXTIME`, `:56`).
pub const MAX_TIME: i64 = 253_402_214_400;

/// The smallest plot size (`gnc-register-number-plot-size-option`, `5.17:libgnucash/engine/gnc-optiondb.cpp:864-874`).
pub const PLOT_SIZE_MIN: u32 = 10;
/// The largest plot size, `UINT16_MAX`.
pub const PLOT_SIZE_MAX: u32 = 65_535;
/// The largest plot size read as a percentage; above it a size is in pixels (`5.17:bindings/guile/gnc-optiondb.i:1094-1108`).
pub const PLOT_SIZE_PERCENT_MAX: u32 = 100;

/// The largest column or row span of a report placement, as GnuCash's size dialog allows
/// (`5.17:gnucash/gtkbuilder/dialog-report.glade:5-10`).
pub const SPAN_MAX: u8 = 100;

/// What identifies and describes an option: GnuCash's `OptionClassifier` (`5.17:libgnucash/engine/gnc-option-impl.hpp:69-76`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Classifier {
    /// The section (GnuCash's page), such as `General`.
    pub section: String,
    /// The option's name within its section, GnuCash's English string.
    pub name: String,
    /// The sort tag: options are ordered by it within their section.
    pub key: String,
    /// Help text.
    pub help: String,
}

impl Classifier {
    /// A classifier from its four strings.
    pub fn new(section: impl Into<String>, name: impl Into<String>, key: impl Into<String>, help: impl Into<String>) -> Self {
        Self {
            section: section.into(),
            name: name.into(),
            key: key.into(),
            help: help.into(),
        }
    }
}

/// The key of a multichoice or list choice: a symbol or string, or a number such as an account level
/// (`GncOptionMultichoiceKeyType`, `5.17:bindings/guile/gnc-optiondb.i:623-655`). On the wire, a string or an integer.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum ChoiceKey {
    /// A symbol or string key.
    Text(String),
    /// A number key.
    Number(i64),
}

impl fmt::Display for ChoiceKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Text(s) => f.write_str(s),
            Self::Number(n) => write!(f, "{n}"),
        }
    }
}

impl From<&str> for ChoiceKey {
    fn from(s: &str) -> Self {
        Self::Text(s.to_owned())
    }
}

impl From<i64> for ChoiceKey {
    fn from(n: i64) -> Self {
        Self::Number(n)
    }
}

/// One choice of a multichoice or list option.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub struct Choice {
    /// The key a value names.
    pub key: ChoiceKey,
    /// What the choice is called.
    pub label: String,
    /// Help text; empty for none.
    #[serde(default)]
    pub help: String,
}

impl Choice {
    /// A choice.
    pub fn new(key: impl Into<ChoiceKey>, label: impl Into<String>, help: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            label: label.into(),
            help: help.into(),
        }
    }
}

/// Which values a date option offers (`RelativeDateUI`, `5.17:libgnucash/engine/gnc-optiondb.hpp:825-830`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum DateUi {
    /// Only an absolute date.
    Absolute,
    /// Only a relative date.
    Relative,
    /// Either.
    Both,
}

/// The value of a date option: an instant, or a relative date resolved when the report runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DateValue {
    /// An instant.
    Absolute(Time64),
    /// A relative date, such as the start of this month.
    Relative(RelativeDate),
}

impl DateValue {
    /// The instant the value names when the report runs at `now` in `zone`, with `accounting` the accounting period, as
    /// `gnc-relative-date-to-time64` resolves it (`5.17:libgnucash/engine/gnc-option-date.cpp:518-597`). `None` when it
    /// falls outside the supported range.
    pub fn resolve(self, now: Time64, zone: &TimeZone, accounting: PeriodBounds) -> Option<Time64> {
        match self {
            Self::Absolute(t) => Some(t),
            Self::Relative(r) => relative_date(r, now, zone, accounting),
        }
    }
}

/// GnuCash's Scheme name of a relative date (`5.17:bindings/guile/gnc-optiondb.i:828-899`), the name on red-ale's
/// wire: the storage string, except `start-accounting-period` and `end-accounting-period`, which GnuCash's C++ stores as
/// `start-prev-fin-year` and `end-prev-fin-year` (`5.17:libgnucash/engine/gnc-option-date.cpp:311`, `:319`).
pub fn relative_name(r: RelativeDate) -> &'static str {
    match r {
        RelativeDate::StartAccountingPeriod => "start-accounting-period",
        RelativeDate::EndAccountingPeriod => "end-accounting-period",
        other => other.as_str(),
    }
}

/// A relative date by its Scheme name, or by the C++ storage name GnuCash writes into saved reports.
pub fn parse_relative(name: &str) -> Option<RelativeDate> {
    RelativeDate::all()
        .find(|r| relative_name(*r) == name)
        .or_else(|| name.parse().ok())
}

/// An option's type, with the constraints its declaration sets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OptionKind {
    /// `GncOptionValue<bool>`.
    Boolean,
    /// A one-line string.
    String,
    /// A multi-line string.
    Text,
    /// A Pango font description such as `Arial Bold 15` (`5.17:gnucash/report/html-fonts.scm:53-98`).
    Font,
    /// A colour, hex `rrggbb` or `rrggbbaa` with no `#` (`5.17:bindings/guile/gnc-optiondb.i:1326-1344`).
    Color,
    /// One key of the choices (`GncOptionMultichoiceValue`).
    Multichoice {
        /// The choices, in order.
        choices: Vec<Choice>,
    },
    /// Several keys of the choices.
    List {
        /// The choices, in order.
        choices: Vec<Choice>,
    },
    /// A number between `min` and `max`; GnuCash holds it as a double (`5.17:bindings/guile/gnc-optiondb.i:1717`),
    /// red-ale exactly. The step is not enforced, as in GnuCash.
    NumberRange {
        /// Smallest value.
        min: Rational,
        /// Largest value.
        max: Rational,
        /// The step a UI moves by.
        step: Rational,
    },
    /// A plot size: 10 to 100 percent, or 101 to 65535 pixels.
    PlotSize,
    /// A date (`GncOptionDateValue`, `5.17:libgnucash/engine/gnc-option-impl.hpp:1045-1165`).
    Date {
        /// Which values it offers.
        ui: DateUi,
        /// The relative dates allowed; empty allows any.
        relative: Vec<RelativeDate>,
    },
    /// Accounts (`GncOptionAccountListValue`, `5.17:libgnucash/engine/gnc-option-impl.cpp:384-444`).
    AccountList {
        /// Allowed account types; empty allows any.
        types: Vec<AccountType>,
        /// Whether more than one account may be chosen.
        multiple: bool,
    },
    /// One account (`GncOptionAccountSelValue`).
    AccountSel {
        /// Allowed account types; empty allows any.
        types: Vec<AccountType>,
    },
    /// A currency: a commodity in the `CURRENCY` namespace.
    Currency,
    /// Any commodity.
    Commodity,
    /// A budget (`gnc-register-budget-option`, a `GncOptionQofInstanceValue` with the budget UI type,
    /// `5.17:libgnucash/engine/gnc-optiondb.cpp:657-665`).
    Budget,
    /// A business document (`gnc-register-invoice-option`, a `GncOptionQofInstanceValue` with UI type `INVOICE`,
    /// `5.17:libgnucash/engine/gnc-optiondb.cpp:915-923`) (v0.6).
    Invoice,
    /// A picture's file name (`gnc-register-pixmap-option`, a string with UI type `PIXMAP`,
    /// `5.17:libgnucash/engine/gnc-optiondb.cpp:725-732`) (v0.6). The server never reads the file.
    Image,
    /// An option scripts keep for themselves, never shown in GnuCash's dialog: a string or a boolean.
    Internal,
    /// The reports a Multicolumn View shows and their spans (`gnc-register-report-placement-option`,
    /// `5.17:libgnucash/engine/gnc-optiondb.cpp:1101-1112`).
    ReportPlacement,
}

impl OptionKind {
    /// The type's name in `report.options`.
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::Boolean => "boolean",
            Self::String => "string",
            Self::Text => "text",
            Self::Font => "font",
            Self::Color => "color",
            Self::Multichoice { .. } => "multichoice",
            Self::List { .. } => "list",
            Self::NumberRange { .. } => "numberRange",
            Self::PlotSize => "plotSize",
            Self::Date { .. } => "date",
            Self::AccountList { .. } => "accountList",
            Self::AccountSel { .. } => "accountSel",
            Self::Currency => "currency",
            Self::Commodity => "commodity",
            Self::Budget => "budget",
            Self::Invoice => "invoice",
            Self::Image => "image",
            Self::Internal => "internal",
            Self::ReportPlacement => "reportPlacement",
        }
    }

    /// The choices of a multichoice or list option.
    pub fn choices(&self) -> Option<&[Choice]> {
        match self {
            Self::Multichoice { choices } | Self::List { choices } => Some(choices),
            _ => None,
        }
    }
}

/// An option's value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OptionValue {
    /// Boolean, and boolean internal options.
    Bool(bool),
    /// String, text, font, colour, and string internal options.
    Text(String),
    /// Multichoice.
    Key(ChoiceKey),
    /// List.
    Keys(Vec<ChoiceKey>),
    /// Number range.
    Number(Rational),
    /// Plot size.
    PlotSize(PlotSize),
    /// Date.
    Date(DateValue),
    /// Account list, as GUIDs.
    Accounts(Vec<Guid>),
    /// One account; `None` when unset, which reads as the first account of an allowed type.
    Account(Option<Guid>),
    /// Currency or commodity.
    Commodity(CommodityRef),
    /// A budget's GUID; `None` when the Book has no budget to default to.
    Budget(Option<Guid>),
    /// A business document's GUID; `None` for no document, the default.
    Invoice(Option<Guid>),
    /// Report placements, in order.
    Placements(Vec<Placement>),
}

/// One child of a report placement option: GnuCash's `GncOptionReportPlacement` (`(id, wide, high)`,
/// `5.17:libgnucash/engine/gnc-option.hpp:71-72`), naming a report by GUID where GnuCash names a report instance of
/// its session (`design/notes/report-statements-charts.md` §8.4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Placement {
    /// A report template or a saved report configuration.
    pub report: Guid,
    /// Only with a template: changes over its defaults, by section and name in wire form. Never empty.
    pub options: Option<Map<String, Value>>,
    /// The columns it spans, 0 to [`SPAN_MAX`] (GnuCash's `wide`).
    pub colspan: u8,
    /// The rows it spans, 0 to [`SPAN_MAX`] (GnuCash's `high`).
    pub rowspan: u8,
}

/// A declared option with its default and current value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReportOption {
    /// Section, name, key and help.
    pub classifier: Classifier,
    /// Type and constraints.
    pub kind: OptionKind,
    default: OptionValue,
    value: OptionValue,
    made_internal: bool,
}

impl ReportOption {
    pub(super) fn declared(classifier: Classifier, kind: OptionKind, default: OptionValue) -> Self {
        Self {
            classifier,
            kind,
            value: default.clone(),
            default,
            made_internal: false,
        }
    }

    /// The section.
    pub fn section(&self) -> &str {
        &self.classifier.section
    }

    /// The name.
    pub fn name(&self) -> &str {
        &self.classifier.name
    }

    /// The default value.
    pub fn default_value(&self) -> &OptionValue {
        &self.default
    }

    /// The current value.
    pub fn value(&self) -> &OptionValue {
        &self.value
    }

    /// Whether the value differs from the default (`is_changed`).
    pub fn is_changed(&self) -> bool {
        self.value != self.default
    }

    pub(super) fn set(&mut self, value: OptionValue) {
        self.value = value;
    }

    pub(super) fn reset(&mut self) {
        self.value = self.default.clone();
    }

    /// Whether the script hid the option with `o.make_internal` (GnuCash's `GncOptionUIType::INTERNAL` set by
    /// `make_internal`, `5.17:libgnucash/engine/gnc-option-impl.hpp:110-111`).
    pub fn is_made_internal(&self) -> bool {
        self.made_internal
    }

    pub(super) fn set_default(&mut self, default: OptionValue) {
        self.value = default.clone();
        self.default = default;
    }

    pub(super) fn make_internal(&mut self) {
        self.made_internal = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_dates_use_scheme_names_and_read_storage_names() {
        assert_eq!(relative_name(RelativeDate::StartAccountingPeriod), "start-accounting-period");
        assert_eq!(relative_name(RelativeDate::StartThisMonth), "start-this-month");
        assert_eq!(parse_relative("end-accounting-period"), Some(RelativeDate::EndAccountingPeriod));
        assert_eq!(parse_relative("end-prev-fin-year"), Some(RelativeDate::EndAccountingPeriod));
        assert_eq!(parse_relative("one-week-ago"), Some(RelativeDate::OneWeekAgo));
        assert_eq!(parse_relative("yesterday"), None);
    }

    #[test]
    fn dates_resolve_through_core() {
        let utc = TimeZone::utc();
        let now = Time64::parse_file_string("2024-03-15 12:00:00 +0000").unwrap();
        let period = PeriodBounds {
            start: Time64(1_704_067_200),
            end: Time64(1_735_689_599),
        };
        let start = DateValue::Relative(RelativeDate::StartThisMonth)
            .resolve(now, &utc, period)
            .unwrap();
        assert_eq!(start.to_file_string(), "2024-03-01 00:00:00 +0000");
        assert_eq!(
            DateValue::Relative(RelativeDate::EndAccountingPeriod).resolve(now, &utc, period),
            Some(period.end)
        );
        assert_eq!(DateValue::Absolute(Time64(5)).resolve(now, &utc, period), Some(Time64(5)));
    }
}
