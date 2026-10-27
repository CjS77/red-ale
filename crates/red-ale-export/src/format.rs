//! How the CSV exports print numbers and dates.
//!
//! GnuCash prints export amounts with `xaccPrintAmount` and dates with `qof_print_date_buff`, both of which follow the
//! user's locale and date format preference. red-ale prints through the report renderer's translation of
//! `xaccPrintAmount` ([`red_ale_report::number::format_amount`]), shared rather than copied, under a [`Locale`], and
//! dates in the `reports.dateFormat` setting ([`DateFormat`]), whose `locale` form is the Locale's `D_FMT`.
//!
//! [`Formats`] is the one place the Locale enters: every number and date an export writes goes through
//! [`Formats::amount`] or [`Formats::date`]. Under the `C` locale that is `-$1,234.56` and `%m/%d/%y`, as GnuCash
//! prints with `gnc_localeconv`'s defaults (`5.17:libgnucash/core-utils/gnc-locale-utils.c:76-109`).
//!
//! The print infos are those the exporters ask for (`5.17:gnucash/import-export/csv-exp/csv-transactions-export.cpp:161-206`),
//! built as `5.17:libgnucash/app-utils/gnc-ui-util.cpp:833-1063` builds them. None of them sets `force_fit` or `round`,
//! so every decimal of the exact value is printed.

use red_ale_core::ops::commodities::{display_symbol, lookup};
use red_ale_core::{Account, Book, CommodityRef, Date, Numeric};
use red_ale_locale::{Conventions, Locale};
use red_ale_report::number::{CommodityFormat, PrintInfo, Rational, format_amount};
use red_ale_report::settings::DateFormat;

/// The number and date conventions an export prints with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Formats {
    date_format: DateFormat,
    locale: Locale,
    conventions: Conventions,
}

impl Formats {
    /// The Locale's number conventions, with dates in `date_format` (GnuCash's date format preference, red-ale's
    /// `reports.dateFormat`).
    pub fn new(date_format: DateFormat, locale: Locale) -> Self {
        Self {
            date_format,
            conventions: locale.conventions(),
            locale,
        }
    }

    /// The Locale.
    pub fn locale(&self) -> &Locale {
        &self.locale
    }

    /// `qof_print_date_buff` of a local date (`5.17:libgnucash/engine/gnc-date.cpp:574-595`).
    pub fn date(&self, d: Date) -> String {
        self.date_format.print(d, &self.locale)
    }

    /// `xaccPrintAmount` of an exact value (`5.17:libgnucash/app-utils/gnc-ui-util.cpp:1306-1444`). A value that is not
    /// a valid numeric prints as nothing, as GnuCash's `PrintAmountInternal` prints nothing for one (`:1091-1097`).
    pub fn amount(&self, value: Numeric, info: &PrintInfo) -> String {
        Rational::from_numeric(value)
            .map(|r| format_amount(&r, info, &self.conventions))
            .unwrap_or_default()
    }

    /// `gnc_default_print_info` (`gnc-ui-util.cpp:833-860`): the Locale's `frac_digits` places, and the symbol of the
    /// Locale's currency when `symbol`. What GnuCash prints a split in no account with.
    pub fn default_info(&self, book: &Book, symbol: bool) -> PrintInfo {
        let places = u32::from(self.conventions.frac_digits);
        let currency = CommodityRef::currency(self.conventions.currency_code());
        PrintInfo {
            symbol: symbol.then(|| nice_symbol(book, &currency)),
            ..PrintInfo::plain(places)
        }
    }
}

impl Default for Formats {
    /// The `C` locale and the `locale` date format: GnuCash's out-of-the-box output.
    fn default() -> Self {
        Self::new(DateFormat::default(), Locale::c())
    }
}

/// `gnc_split_amount_print_info`, that is `gnc_account_print_info` (`gnc-ui-util.cpp:919-974`): places from the
/// account's SCU, padded only for a currency, with the account commodity's symbol when `symbol`.
pub fn account_info(book: &Book, account: &Account, symbol: bool) -> PrintInfo {
    let Some(id) = &account.commodity else {
        return PrintInfo {
            symbol: None,
            is_iso: false,
            use_locale: false,
            ..PrintInfo::plain(0)
        };
    };
    let fraction = lookup(book, id).map_or(0, |c| c.fraction);
    let format = CommodityFormat::new(id.clone(), nice_symbol(book, id), account.scu(fraction));
    PrintInfo::commodity(&format, symbol)
}

/// `gnc_commodity_print_info` (`gnc-ui-util.cpp:885-917`): places from the commodity's SCU, padded only for a
/// currency, with its symbol when `symbol`. `None` for a commodity the Book does not know.
pub fn commodity_info(book: &Book, id: &CommodityRef, symbol: bool) -> Option<PrintInfo> {
    CommodityFormat::from_book(book, id).map(|format| PrintInfo::commodity(&format, symbol))
}

/// `gnc_default_price_print_info` of the account's commodity (`gnc-ui-util.cpp:1025-1063`): padded to as many places as
/// the commodity's fraction has trailing zeros, no symbol, grouped. With no commodity, no padding.
pub fn price_info(book: &Book, account: Option<&Account>) -> PrintInfo {
    let format = account
        .and_then(|a| a.commodity.as_ref())
        .and_then(|id| CommodityFormat::from_book(book, id));
    format.map_or_else(
        || PrintInfo::plain(0),
        |f| PrintInfo {
            symbol: None,
            ..PrintInfo::price(&f)
        },
    )
}

/// The same print info without thousands separators: the `Num.` columns (`csv-transactions-export.cpp:167-168`,
/// `:179-180`).
pub fn without_separators(info: PrintInfo) -> PrintInfo {
    PrintInfo {
        use_separators: false,
        ..info
    }
}

fn nice_symbol(book: &Book, id: &CommodityRef) -> String {
    display_symbol(book, id).unwrap_or_else(|| id.mnemonic.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use red_ale_core::{AccountType, FixedEnv, Guid, Time64};

    fn book() -> Book {
        Book::new_empty(&mut FixedEnv::new(Time64(0), 1))
    }

    fn account(commodity: CommodityRef, scu: Option<i64>) -> Account {
        let mut a = Account::new(Guid::from_bytes([7; 16]), "A", AccountType::Bank, Some(commodity), None);
        a.commodity_scu = scu;
        a.non_standard_scu = scu.is_some();
        a
    }

    fn n(num: i64, denom: i64) -> Numeric {
        Numeric::new(num, denom)
    }

    /// A currency account pads to its SCU and puts the symbol first; the `Num.` form has no symbol and no grouping
    /// (`csv-transactions-export.cpp:162-170`).
    #[test]
    fn currency_amounts() {
        let b = book();
        let f = Formats::default();
        let usd = account(CommodityRef::currency("USD"), None);
        assert_eq!(f.amount(n(-123_456, 100), &account_info(&b, &usd, true)), "-$1,234.56");
        assert_eq!(
            f.amount(n(-123_456, 100), &without_separators(account_info(&b, &usd, false))),
            "-1234.56"
        );
        assert_eq!(f.amount(n(5, 1), &account_info(&b, &usd, false)), "5.00");
        let info = commodity_info(&b, &CommodityRef::currency("USD"), true).unwrap();
        assert_eq!(f.amount(Numeric::ZERO, &info), "$0.00");
        assert_eq!(f.amount(n(5, 1), &f.default_info(&b, true)), "$5.00");
    }

    /// A non-standard SCU sets the places (`gnc_account_print_info_helper`, `gnc-ui-util.cpp:933-943`).
    #[test]
    fn account_scu_sets_the_places() {
        let f = Formats::default();
        let usd = account(CommodityRef::currency("USD"), Some(1000));
        assert_eq!(f.amount(n(5, 1), &account_info(&book(), &usd, true)), "$5.000");
    }

    /// A price of a currency account is padded to its fraction's places, and never has a symbol
    /// (`gnc_price_print_info`, `gnc-ui-util.cpp:1033-1042`); an invalid numeric prints as nothing.
    #[test]
    fn prices_and_invalid_values() {
        let b = book();
        let f = Formats::default();
        let usd = account(CommodityRef::currency("USD"), None);
        assert_eq!(f.amount(Numeric::ZERO, &price_info(&b, Some(&usd))), "0.00");
        assert_eq!(f.amount(n(12_345, 10), &price_info(&b, Some(&usd))), "1,234.50");
        assert_eq!(f.amount(n(1, 3), &price_info(&b, Some(&usd))), "1/3");
        assert_eq!(f.amount(n(1, 0), &price_info(&b, Some(&usd))), "");
    }

    /// Another Locale changes separators and symbol placement, and the `locale` date format.
    #[test]
    fn the_locale_applies() {
        let b = book();
        let f = Formats::new(DateFormat::Locale, Locale::by_name("fr_FR").unwrap());
        let eur = account(CommodityRef::currency("EUR"), None);
        assert_eq!(
            f.amount(n(-123_456, 100), &without_separators(account_info(&b, &eur, false))),
            "-1234,56"
        );
        assert_eq!(f.date(Date::from_ymd(2024, 3, 5).unwrap()), "05/03/2024");
    }

    #[test]
    fn dates_follow_the_date_format() {
        let d = Date::from_ymd(2024, 3, 5).unwrap();
        assert_eq!(Formats::default().date(d), "03/05/24");
        assert_eq!(Formats::new(DateFormat::Iso, Locale::c()).date(d), "2024-03-05");
    }
}
