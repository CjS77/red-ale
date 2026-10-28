//! The GnuCash preferences reports read (`design/reports.md` §4). They are not in the Book; red-ale keeps them in its
//! Configuration under `reports`, with GnuCash's out-of-the-box values as defaults, and passes them to each run.

use red_ale_core::ops::commodities::lookup;
use red_ale_core::{AccountType, Book, CommodityRef, Date};
use red_ale_locale::{Category, CivilDate, Locale, Weekday};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::wire::CommodityId;

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
    /// The Locale's `D_FMT`: `%m/%d/%y` under the C locale GnuCash's report tests use, `%m/%d/%Y` under en_US.
    #[default]
    Locale,
}

/// A core date as the locale crate takes it.
pub(crate) fn civil(d: Date) -> Option<CivilDate> {
    CivilDate::new(d.year(), u8::try_from(d.month()).ok()?, u8::try_from(d.day()).ok()?)
}

impl DateFormat {
    /// `qof-print-date` of a date under `locale`.
    pub fn print(self, d: Date, locale: &Locale) -> String {
        let (y, m, day) = (d.year(), d.month(), d.day());
        match self {
            Self::Us => format!("{m:02}/{day:02}/{y:04}"),
            Self::Uk => format!("{day:02}/{m:02}/{y:04}"),
            Self::Ce => format!("{day:02}.{m:02}.{y:04}"),
            Self::Iso => format!("{y:04}-{m:02}-{day:02}"),
            Self::Locale => civil(d).map_or_else(|| format!("{m:02}/{day:02}/{:02}", y.rem_euclid(100)), |c| locale.print_date(c)),
        }
    }

    /// `gnc-date-interval-format` (`5.17:libgnucash/engine/gnc-date.cpp:661-680`): under the `locale` format, ICU's
    /// interval form for the Locale (`1/1/2026 – 12/31/2026` under en_US); otherwise, or when ICU has no answer,
    /// `<from> to <to>`. Under the C locale red-ale keeps `<from> to <to>`, where GnuCash asks ICU for en_US_POSIX
    /// (`design/reports.md` §14).
    pub fn print_interval(self, from: Date, to: Date, locale: &Locale) -> String {
        let icu = (self == Self::Locale && !locale.is_c(Category::Messages))
            .then(|| locale.date_interval(civil(from)?, civil(to)?))
            .flatten();
        icu.unwrap_or_else(|| self.print_range(from, to, locale))
    }

    /// `<from> to <to>`, each as `qof-print-date` prints it: the Multicolumn P&L's column headers
    /// (`5.17:gnucash/report/reports/standard/balsheet-pnl.scm:1140-1153`), and the interval form when ICU has none.
    pub fn print_range(self, from: Date, to: Date, locale: &Locale) -> String {
        format!("{} to {}", self.print(from, locale), self.print(to, locale))
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

impl From<Weekday> for WeekStart {
    fn from(w: Weekday) -> Self {
        match w {
            Weekday::Sunday => Self::Sunday,
            Weekday::Monday => Self::Monday,
            Weekday::Tuesday => Self::Tuesday,
            Weekday::Wednesday => Self::Wednesday,
            Weekday::Thursday => Self::Thursday,
            Weekday::Friday => Self::Friday,
            Weekday::Saturday => Self::Saturday,
        }
    }
}

/// The report settings of the Configuration's `reports` key that a run reads. The accounting period, also read, is
/// the existing `accountingPeriod` key. What is unset follows the Locale, as in GnuCash.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", default)]
pub struct ReportSettings {
    /// The default of every "Report's currency" option; when unset, the Locale's currency (see
    /// [`ReportSettings::report_currency`]).
    #[serde(with = "optional_commodity_ref", skip_serializing_if = "Option::is_none")]
    #[schemars(with = "Option<CommodityId>")]
    pub currency: Option<CommodityRef>,
    /// Which accounts show reversed balances.
    pub reverse_balanced_accounts: ReverseBalanced,
    /// How dates print.
    pub date_format: DateFormat,
    /// The first day of the week; when unset, the Locale's.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub week_start: Option<WeekStart>,
}

impl ReportSettings {
    /// `gnc_default_report_currency` (`5.17:libgnucash/app-utils/gnc-ui-util.cpp:748-822`): the `currency` setting,
    /// else the Locale's currency (`int_curr_symbol`) if `book` knows it, else USD. EUR under fr_FR, USD under C.
    pub fn report_currency(&self, book: &Book, locale: &Locale) -> CommodityRef {
        self.currency.clone().unwrap_or_else(|| {
            let local = CommodityRef::currency(locale.conventions().currency_code());
            if lookup(book, &local).is_some() {
                local
            } else {
                CommodityRef::currency("USD")
            }
        })
    }

    /// The `weekStart` setting, else the Locale's first day of the week: Sunday under C.
    pub fn week_start(&self, locale: &Locale) -> WeekStart {
        self.week_start.unwrap_or_else(|| locale.first_weekday().into())
    }
}

/// An optional commodity in its wire form.
mod optional_commodity_ref {
    use red_ale_core::CommodityRef;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    use crate::wire::CommodityId;

    #[allow(clippy::ref_option)]
    pub(super) fn serialize<S: Serializer>(value: &Option<CommodityRef>, s: S) -> Result<S::Ok, S::Error> {
        value.clone().map(CommodityId).serialize(s)
    }

    pub(super) fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<CommodityRef>, D::Error> {
        Option::<CommodityId>::deserialize(d).map(|c| c.map(|c| c.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn locale(name: &str) -> Locale {
        Locale::by_name(name).unwrap()
    }

    #[test]
    fn dates_print_in_each_format() {
        let d = Date::from_ymd(2024, 3, 5).unwrap();
        let formats = [DateFormat::Us, DateFormat::Uk, DateFormat::Ce, DateFormat::Iso, DateFormat::Locale];
        let printed: Vec<String> = formats.iter().map(|f| f.print(d, &Locale::c())).collect();
        assert_eq!(printed, ["03/05/2024", "05/03/2024", "05.03.2024", "2024-03-05", "03/05/24"]);
        let end = Date::from_ymd(2024, 12, 31).unwrap();
        assert_eq!(DateFormat::Iso.print_interval(d, end, &Locale::c()), "2024-03-05 to 2024-12-31");
        assert_eq!(DateFormat::Locale.print_interval(d, end, &Locale::c()), "03/05/24 to 12/31/24");
    }

    /// The `locale` format follows the Locale's `D_FMT`, and its intervals ICU; the others do not change.
    #[test]
    fn dates_follow_the_locale() {
        let (d, end) = (Date::from_ymd(2026, 3, 5).unwrap(), Date::from_ymd(2026, 12, 31).unwrap());
        assert_eq!(DateFormat::Locale.print(end, &locale("en_US")), "12/31/2026");
        assert_eq!(DateFormat::Locale.print(d, &locale("fr_FR")), "05/03/2026");
        assert_eq!(DateFormat::Locale.print(d, &locale("de_DE")), "05.03.2026");
        assert_eq!(DateFormat::Iso.print(d, &locale("fr_FR")), "2026-03-05");
        assert_eq!(
            DateFormat::Locale.print_interval(d, end, &locale("en_US")),
            "3/5/2026\u{2009}–\u{2009}12/31/2026"
        );
        assert_eq!(
            DateFormat::Locale.print_interval(end, d, &locale("en_US")),
            "3/5/2026\u{2009}–\u{2009}12/31/2026"
        );
        assert_eq!(DateFormat::Uk.print_interval(d, end, &locale("en_US")), "05/03/2026 to 31/12/2026");
    }

    #[test]
    fn unset_settings_follow_the_locale() {
        let mut env = red_ale_core::FixedEnv::new(red_ale_core::Time64(0), 1);
        let book = Book::new_empty(&mut env);
        let unset = ReportSettings::default();
        assert_eq!(unset.report_currency(&book, &Locale::c()), CommodityRef::currency("USD"));
        assert_eq!(unset.report_currency(&book, &locale("fr_FR")), CommodityRef::currency("EUR"));
        assert_eq!(unset.report_currency(&book, &locale("ja_JP")), CommodityRef::currency("JPY"));
        assert_eq!(
            (unset.week_start(&Locale::c()), unset.week_start(&locale("fr_FR"))),
            (WeekStart::Sunday, WeekStart::Monday)
        );
        let set = ReportSettings {
            currency: Some(CommodityRef::currency("GBP")),
            week_start: Some(WeekStart::Friday),
            ..unset
        };
        assert_eq!(set.report_currency(&book, &locale("fr_FR")), CommodityRef::currency("GBP"));
        assert_eq!(set.week_start(&locale("fr_FR")), WeekStart::Friday);
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
        assert_eq!(s.currency, Some(CommodityRef::currency("EUR")));
        let read = (s.reverse_balanced_accounts, s.date_format, s.week_start);
        assert_eq!(read, (ReverseBalanced::None, DateFormat::Locale, None));
        assert_eq!(serde_json::to_value(&s).unwrap()["currency"], json!("CURRENCY::EUR"));
        assert_eq!(serde_json::to_value(ReportSettings::default()).unwrap().get("currency"), None);
        let s: ReportSettings = serde_json::from_value(json!({ "currency": null, "weekStart": "monday" })).unwrap();
        assert_eq!((s.currency, s.week_start), (None, Some(WeekStart::Monday)));
    }
}
