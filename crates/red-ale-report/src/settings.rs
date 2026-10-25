//! The GnuCash preferences reports read (`design/reports.md` §4). They are not in the Book; red-ale keeps them in its
//! Configuration under `reports`, with GnuCash's out-of-the-box values as defaults, and passes them to each run.

use red_ale_core::{AccountType, CommodityRef, Date};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::wire::{CommodityId, commodity_ref};

/// How dates print (`general` `date-format`, `qof_date_format_get_string`, `5.17:libgnucash/engine/gnc-date.cpp:504-521`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum DateFormat {
    /// `%m/%d/%Y`.
    Us,
    /// `%d/%m/%Y`.
    Uk,
    /// `%d.%m.%Y`.
    Ce,
    /// `%Y-%m-%d`.
    Iso,
    /// What GnuCash prints under the C locale its report tests use: `%m/%d/%y`.
    #[default]
    Locale,
}

impl DateFormat {
    /// `qof-print-date` of a date.
    pub fn print(self, d: Date) -> String {
        let (y, m, day) = (d.year(), d.month(), d.day());
        match self {
            Self::Us => format!("{m:02}/{day:02}/{y:04}"),
            Self::Uk => format!("{day:02}/{m:02}/{y:04}"),
            Self::Ce => format!("{day:02}.{m:02}.{y:04}"),
            Self::Iso => format!("{y:04}-{m:02}-{day:02}"),
            Self::Locale => format!("{m:02}/{day:02}/{:02}", y.rem_euclid(100)),
        }
    }

    /// `gnc-date-interval-format`: `<from> to <to>`, GnuCash's form for every format but `locale`, where it asks ICU
    /// instead (`5.17:libgnucash/engine/gnc-date.cpp:618-680`).
    pub fn print_interval(self, from: Date, to: Date) -> String {
        format!("{} to {}", self.print(from), self.print(to))
    }
}

/// Which accounts show reversed balances (`general` `reversed-accounts-*`, `gnc_configure_reverse_balance`,
/// `5.17:libgnucash/app-utils/gnc-ui-util.cpp:128-173`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum ReverseBalanced {
    /// None, as in GnuCash's report tests, which run with no preferences.
    None,
    /// LIABILITY, PAYABLE, EQUITY, INCOME and CREDIT: the schema default
    /// (`5.17:gnucash/gschemas/org.gnucash.GnuCash.gschema.xml.in:93-107`).
    #[default]
    Credit,
    /// INCOME and EXPENSE.
    IncomeExpense,
}

impl ReverseBalanced {
    /// `gnc-reverse-balance` for an account of type `t`.
    pub fn reverses(self, t: AccountType) -> bool {
        use AccountType as T;
        match self {
            Self::None => false,
            Self::Credit => matches!(t, T::Liability | T::Payable | T::Equity | T::Income | T::Credit),
            Self::IncomeExpense => matches!(t, T::Income | T::Expense),
        }
    }
}

/// The first day of the week (`gnc-start-of-week`, `5.17:libgnucash/engine/gnc-date.cpp:195-215`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[allow(missing_docs)]
pub enum WeekStart {
    #[default]
    Sunday,
    Monday,
    Tuesday,
    Wednesday,
    Thursday,
    Friday,
    Saturday,
}

/// The report settings of the Configuration's `reports` key that a run reads. The accounting period, also read, is
/// the existing `accountingPeriod` key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", default)]
pub struct ReportSettings {
    /// The default of every "Report's currency" option (`gnc_default_report_currency`); USD, GnuCash's fallback.
    #[serde(with = "commodity_ref")]
    #[schemars(with = "CommodityId")]
    pub currency: CommodityRef,
    /// Which accounts show reversed balances.
    pub reverse_balanced_accounts: ReverseBalanced,
    /// How dates print.
    pub date_format: DateFormat,
    /// The first day of the week.
    pub week_start: WeekStart,
}

impl Default for ReportSettings {
    fn default() -> Self {
        Self {
            currency: CommodityRef::currency("USD"),
            reverse_balanced_accounts: ReverseBalanced::default(),
            date_format: DateFormat::default(),
            week_start: WeekStart::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn dates_print_in_each_format() {
        let d = Date::from_ymd(2024, 3, 5).unwrap();
        let formats = [DateFormat::Us, DateFormat::Uk, DateFormat::Ce, DateFormat::Iso, DateFormat::Locale];
        let printed: Vec<String> = formats.iter().map(|f| f.print(d)).collect();
        assert_eq!(printed, ["03/05/2024", "05/03/2024", "05.03.2024", "2024-03-05", "03/05/24"]);
        let end = Date::from_ymd(2024, 12, 31).unwrap();
        assert_eq!(DateFormat::Iso.print_interval(d, end), "2024-03-05 to 2024-12-31");
    }

    #[test]
    fn reversed_balances() {
        assert!(ReverseBalanced::Credit.reverses(AccountType::Income));
        assert!(!ReverseBalanced::Credit.reverses(AccountType::CreditLine));
        assert!(!ReverseBalanced::Credit.reverses(AccountType::Trading));
        assert!(ReverseBalanced::IncomeExpense.reverses(AccountType::Expense));
        assert!(!ReverseBalanced::None.reverses(AccountType::Liability));
    }

    #[test]
    fn settings_read_with_defaults() {
        let s: ReportSettings = serde_json::from_value(json!({ "reverseBalancedAccounts": "none", "currency": "EUR" })).unwrap();
        assert_eq!(s.currency, CommodityRef::currency("EUR"));
        let read = (s.reverse_balanced_accounts, s.date_format, s.week_start);
        assert_eq!(read, (ReverseBalanced::None, DateFormat::Locale, WeekStart::Sunday));
        assert_eq!(serde_json::to_value(ReportSettings::default()).unwrap()["currency"], json!("CURRENCY::USD"));
    }
}
