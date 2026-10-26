//! Display rounding and GnuCash's amount formatting.
//!
//! The text is that of `xaccSPrintAmount` and `PrintAmountInternal` (`5.17:libgnucash/app-utils/gnc-ui-util.cpp:1083-1430`)
//! under the conventions GnuCash falls back to when no locale is set (`5.17:libgnucash/core-utils/gnc-locale-utils.c:75-108`),
//! which are those its report tests run under: grouping by three with `,`, decimal point `.`, minus sign first. It is
//! red-ale's only locale.
//!
//! Reports never round a figure before using it again; these functions round only to show it.

use num_bigint::BigInt;
use num_integer::Integer;
use num_traits::Zero;
use red_ale_core::ops::commodities::{display_symbol, is_iso_namespace, lookup};
use red_ale_core::{Book, CommodityRef};

use super::rational::split_decimal;
use super::{Rational, Rounding};

/// The most decimal places GnuCash prints a value with before falling back to `w + n/d`: `max_leg_digits`
/// (`5.17:libgnucash/engine/gnc-numeric.cpp:51`, used by `gnc_numeric_to_decimal`, `:1253-1271`).
const MAX_DECIMAL_PLACES: u64 = 18;

/// What printing needs to know about a commodity: its identity, its *display symbol* and its SCU.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommodityFormat {
    /// The commodity.
    pub id: CommodityRef,
    /// Its display symbol (GnuCash's nice symbol): the user symbol, else the ISO table's symbol, else the mnemonic.
    pub symbol: String,
    /// Its SCU, the denominator amounts are shown in.
    pub fraction: i64,
}

impl CommodityFormat {
    /// A commodity's format from its parts.
    pub fn new(id: CommodityRef, symbol: impl Into<String>, fraction: i64) -> Self {
        Self { id, symbol: symbol.into(), fraction }
    }

    /// The format of a commodity of `book`, or of a built-in currency; `None` for an unknown commodity. The symbol is
    /// core's display symbol (`5.17:libgnucash/engine/gnc-commodity.cpp:1095-1115`).
    pub fn from_book(book: &Book, id: &CommodityRef) -> Option<Self> {
        let commodity = lookup(book, id)?;
        let symbol = display_symbol(book, id)?;
        Some(Self::new(commodity.id.clone(), symbol, commodity.fraction))
    }

    /// True for a commodity in the `CURRENCY` namespace (`gnc_commodity_is_iso`).
    pub fn is_iso(&self) -> bool {
        is_iso_namespace(&self.id.namespace)
    }
}

/// How to print an amount: the fields of GnuCash's `GNCPrintAmountInfo` (`5.17:libgnucash/app-utils/gnc-ui-util.h:252-265`)
/// that reports use. Reports never set `force_fit` or `round`, so every decimal of an exact value is printed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrintInfo {
    /// The symbol to print, if any.
    pub symbol: Option<String>,
    /// Whether the symbol goes first with no space (`$5.00`, an ISO currency) or last after a space (`5. FUNDS`).
    pub symbol_first: bool,
    /// Decimals always printed, padding with zeros.
    pub min_decimal_places: u32,
    /// Whether to group the whole part by thousands.
    pub use_separators: bool,
}

impl PrintInfo {
    /// `gnc_commodity_print_info` (`5.17:libgnucash/app-utils/gnc-ui-util.cpp:885-917`): an ISO currency pads to its
    /// SCU's decimals; any other commodity pads none.
    pub fn commodity(c: &CommodityFormat, use_symbol: bool) -> Self {
        let places = decimal_fraction_places(c.fraction).unwrap_or(0);
        Self {
            symbol: use_symbol.then(|| c.symbol.clone()),
            symbol_first: c.is_iso(),
            min_decimal_places: if c.is_iso() { places } else { 0 },
            use_separators: true,
        }
    }

    /// `gnc_price_print_info` with the symbol, as the price renderer asks for it
    /// (`5.17:libgnucash/app-utils/gnc-ui-util.cpp:1025-1057`; `5.17:gnucash/report/html-style-info.scm:221-222`): padded to
    /// as many decimals as the currency's SCU has trailing zeros. The `force-price-decimal` preference is off.
    pub fn price(currency: &CommodityFormat) -> Self {
        Self {
            symbol: Some(currency.symbol.clone()),
            symbol_first: currency.is_iso(),
            min_decimal_places: trailing_zeros(currency.fraction),
            use_separators: true,
        }
    }

    /// No symbol, grouped, padded to `places` decimals. `plain(2)` is `gnc_default_print_info(#f)` in the C locale,
    /// whose `frac_digits` falls back to 2 (`5.17:libgnucash/app-utils/gnc-ui-util.cpp:833-860`).
    pub fn plain(places: u32) -> Self {
        Self { symbol: None, symbol_first: true, min_decimal_places: places, use_separators: true }
    }
}

/// `is_decimal_fraction`: the decimals of a power-of-ten SCU (`5.17:libgnucash/app-utils/gnc-ui-util.cpp:862-883`).
fn decimal_fraction_places(fraction: i64) -> Option<u32> {
    (fraction > 0).then_some(())?;
    let places = trailing_zeros(fraction);
    (fraction == 10_i64.checked_pow(places)?).then_some(places)
}

/// How many times ten divides `fraction`.
fn trailing_zeros(fraction: i64) -> u32 {
    std::iter::successors(Some(fraction), |f| (*f != 0 && f % 10 == 0).then_some(f / 10)).skip(1).fold(0, |n, _| n + 1)
}

/// `xaccPrintAmount`: the text of an exact value, with no rounding.
pub fn format_amount(value: &Rational, info: &PrintInfo) -> String {
    let sign = if value.is_negative() { "-" } else { "" };
    let number = print_magnitude(value, info);
    match &info.symbol {
        None => format!("{sign}{number}"),
        Some(symbol) if info.symbol_first => format!("{sign}{symbol}{number}"),
        Some(symbol) => format!("{sign}{number} {symbol}"),
    }
}

/// `PrintAmountInternal` (`5.17:libgnucash/app-utils/gnc-ui-util.cpp:1083-1300`): the magnitude with grouping and an
/// always-printed decimal point, or `w + n/d` when the value is not a decimal of at most 18 places. A negative
/// value's fraction is joined with `-`, as GnuCash does (`-$480 - 85/104`).
///
/// The point after a whole amount (`5. ACME`) matches the pin only. GnuCash added it in 5.12 to fix a register bug
/// red-ale cannot have, and red-ale will likely drop it later (`design/reports.md` §3).
fn print_magnitude(value: &Rational, info: &PrintInfo) -> String {
    let magnitude = value.abs();
    let (whole, rem) = magnitude.numer().div_rem(magnitude.denom());
    let whole_text = if info.use_separators { group_thousands(&whole.to_string()) } else { whole.to_string() };
    if let Some(places) = magnitude.decimal_places().filter(|p| *p <= MAX_DECIMAL_PLACES).and_then(|p| u32::try_from(p).ok()) {
        let places = places.max(info.min_decimal_places);
        let digits = &rem * BigInt::from(10).pow(places) / magnitude.denom();
        let (_, frac) = split_decimal(&digits, places);
        format!("{whole_text}.{frac}")
    } else {
        let fraction = format!("{rem}/{}", magnitude.denom());
        match (whole.is_zero(), value.is_negative()) {
            (true, _) => fraction,
            (false, true) => format!("{whole_text} - {fraction}"),
            (false, false) => format!("{whole_text} + {fraction}"),
        }
    }
}

/// Insert `,` between groups of three digits, from the right.
fn group_thousands(digits: &str) -> String {
    let len = digits.len();
    digits.chars().enumerate().fold(String::with_capacity(len + len / 3), |mut out, (i, c)| {
        if i > 0 && (len - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
        out
    })
}

/// A monetary amount as a report shows it: rounded to its commodity's SCU and printed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MonetaryDisplay {
    /// The amount at the commodity's SCU, banker's rounding.
    pub rounded: Rational,
    /// The text.
    pub display: String,
}

/// GnuCash's monetary renderer (`5.17:gnucash/report/html-style-info.scm:224-231`): round to the commodity's SCU (not
/// the account's) with `GNC-HOW-RND-ROUND`, banker's rounding, then print with the commodity's print info and symbol.
pub fn display_monetary(amount: &Rational, commodity: &CommodityFormat) -> MonetaryDisplay {
    let rounded = amount.round_to(commodity.fraction.max(1), Rounding::Bankers).unwrap_or_else(|_| amount.clone());
    let display = format_amount(&rounded, &PrintInfo::commodity(commodity, true));
    MonetaryDisplay { rounded, display }
}

/// GnuCash's price renderer (`5.17:gnucash/report/html-style-info.scm:221-222`): the exact price in `currency`, padded
/// to the currency's decimals and never rounded, so a rate that is not a terminating decimal prints as `$480 + 85/104`.
pub fn display_price(price: &Rational, currency: &CommodityFormat) -> String {
    format_amount(price, &PrintInfo::price(currency))
}

/// GnuCash's number renderer (`5.17:gnucash/report/html-style-info.scm:233-236`) generalised to `places` decimals:
/// rounded with banker's rounding, grouped, no symbol. GnuCash goes through a double here; red-ale rounds exactly.
pub fn display_number(value: &Rational, places: u32) -> String {
    let denom = 10_i64.checked_pow(places).unwrap_or(i64::MAX);
    let rounded = value.round_to(denom, Rounding::Bankers).unwrap_or_else(|_| value.clone());
    format_amount(&rounded, &PrintInfo::plain(places))
}

/// The exact value, grouped, with as many decimals as it needs and no trailing point, or `w + n/d`: how chart hover
/// labels show a `number` axis.
pub fn display_exact(value: &Rational) -> String {
    let text = format_amount(value, &PrintInfo::plain(0));
    text.strip_suffix('.').map_or_else(|| text.clone(), str::to_owned)
}

/// Guile's `~w,dF` directive on an exact number (`format:out-fixed`, Guile 3.0 `ice-9/format.scm`): rounded half away
/// from zero to `places` decimals and padded with spaces on the left to `width`. When the whole part is zero, its `0`
/// is printed only if there is no width or the width exceeds `places + 1`, so `fixed(1/2, Some(2), 1)` is `.5`, as
/// `5.17:gnucash/report/reports/standard/account-piecharts.scm:552-554` relies on. A number wider than `width` is
/// printed in full. GnuCash formats the double nearest the value; red-ale formats the value itself.
pub fn fixed(value: &Rational, width: Option<usize>, places: u32) -> String {
    let scaled = value.abs().scaled_half_up(places);
    let (whole, frac) = split_decimal(&scaled, places);
    let whole = if whole == "0" { String::new() } else { whole };
    let places = usize::try_from(places).unwrap_or(usize::MAX);
    let leading_zero = whole.is_empty() && width.is_none_or(|w| w > places.saturating_add(1));
    let sign = if value.is_negative() { "-" } else { "" };
    let body = format!("{sign}{}{whole}.{frac}", if leading_zero { "0" } else { "" });
    let pad = width.map_or(0, |w| w.saturating_sub(body.len()));
    format!("{:pad$}{body}", "")
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use red_ale_core::commodity::CURRENCY_NAMESPACE;

    fn r(s: &str) -> Rational {
        Rational::parse(s).unwrap()
    }

    fn usd() -> CommodityFormat {
        CommodityFormat::new(CommodityRef::currency("USD"), "$", 100)
    }

    /// GBP with the user symbol `#` of `5.17:gnucash/report/reports/standard/test/test-account-summary.scm:26-33`.
    fn gbp() -> CommodityFormat {
        CommodityFormat::new(CommodityRef::currency("GBP"), "#", 100)
    }

    /// The 1/1000 FUNDS commodity of `5.17:gnucash/report/reports/standard/test/test-balsheet-pnl.scm:55-61`.
    fn funds() -> CommodityFormat {
        CommodityFormat::new(CommodityRef::new("NASDAQ", "FUNDS"), "FUNDS", 1000)
    }

    fn money(s: &str, c: &CommodityFormat) -> String {
        display_monetary(&r(s), c).display
    }

    /// Expected strings of GnuCash 5.17's own report tests, which run in the C locale
    /// (`5.17:gnucash/report/reports/standard/test/test-balsheet-pnl.scm:181-568`,
    /// `5.17:gnucash/report/reports/standard/test/test-account-summary.scm:54-64`).
    #[test]
    fn monetary_cells_print_as_gnucash_tests_expect() {
        assert_eq!(money("116006.33", &usd()), "$116,006.33");
        assert_eq!(money("2186", &usd()), "$2,186.00");
        assert_eq!(money("0", &usd()), "$0.00");
        assert_eq!(money("-250", &usd()), "-$250.00");
        assert_eq!(money("-1020", &usd()), "-$1,020.00");
        assert_eq!(money("608", &gbp()), "#608.00");
        assert_eq!(money("-600", &gbp()), "-#600.00");
        assert_eq!(money("0", &gbp()), "#0.00");
        assert_eq!(money("30", &funds()), "30. FUNDS");
        assert_eq!(money("1", &funds()), "1. FUNDS");
        assert_eq!(money("10", &funds()), "10. FUNDS");
        assert_eq!(money("100000", &usd()), "$100,000.00");
        assert_eq!(money("14424.52", &usd()), "$14,424.52");
    }

    #[test]
    fn price_cells_keep_every_decimal_and_print_remainders() {
        let price = |s: &str| display_price(&r(s), &usd());
        assert_eq!(price("85/104"), "$85/104");
        assert_eq!(price("49925/104"), "$480 + 5/104");
        assert_eq!(display_price(&(&Rational::from_int(480) + &r("85/104")), &usd()), "$480 + 85/104");
        assert_eq!(display_price(&(&Rational::from_int(1) + &r("49/101")), &usd()), "$1 + 49/101");
        assert_eq!(display_price(&(&Rational::from_int(235) + &r("3/7")), &usd()), "$235 + 3/7");
        assert_eq!(display_price(&(&Rational::from_int(203) + &r("1/3")), &usd()), "$203 + 1/3");
        assert_eq!(price("1.7"), "$1.70");
        assert_eq!(price("1.499"), "$1.499");
        assert_eq!(price("1.455"), "$1.455");
        assert_eq!(price("300.01"), "$300.01");
        assert_eq!(display_price(&-(&Rational::from_int(480) + &r("85/104")), &usd()), "-$480 - 85/104");
    }

    #[test]
    fn amounts_follow_print_amount_internal() {
        let plain = |s: &str, places| format_amount(&r(s), &PrintInfo::plain(places));
        assert_eq!(plain("1234567.891", 2), "1,234,567.891");
        assert_eq!(plain("-0.5", 0), "-0.5");
        assert_eq!(plain("12", 0), "12.");
        assert_eq!(plain("123", 2), "123.00");
        assert_eq!(plain("1/3", 2), "1/3");
        assert_eq!(plain("-1/3", 2), "-1/3");
        let no_sep = PrintInfo { use_separators: false, ..PrintInfo::plain(2) };
        assert_eq!(format_amount(&r("1234567.5"), &no_sep), "1234567.50");
        // 2^-18 needs 18 places and prints as a decimal; 2^-19 needs 19 and prints as a fraction.
        assert_eq!(plain("1/262144", 0), "0.000003814697265625");
        assert_eq!(plain("1/524288", 0), "1/524288");
        let security = CommodityFormat::new(CommodityRef::new("NYSE", "ABC"), "ABC", 10000);
        assert_eq!(money("1.5", &security), "1.5 ABC");
        assert_eq!(money("-1.23456", &security), "-1.2346 ABC");
        let odd = CommodityFormat::new(CommodityRef::new("X", "THIRDS"), "TH", 3);
        assert_eq!(money("7/3", &odd), "2 + 1/3 TH");
    }

    #[test]
    fn display_rounding_is_bankers_at_the_scu() {
        let d = display_monetary(&r("0.125"), &usd());
        assert_eq!((d.rounded, d.display.as_str()), (r("0.12"), "$0.12"));
        assert_eq!(money("0.135", &usd()), "$0.14");
        assert_eq!(money("-0.125", &usd()), "-$0.12");
        assert_eq!(money("1/3", &usd()), "$0.33");
        assert_eq!(money("0.0005", &funds()), "0. FUNDS");
        assert_eq!(money("0.0015", &funds()), "0.002 FUNDS");
    }

    #[test]
    fn number_cells_round_bankers_to_places() {
        assert_eq!(display_number(&r("57.1428"), 2), "57.14");
        assert_eq!(display_number(&r("0.125"), 2), "0.12");
        assert_eq!(display_number(&r("1234.5"), 2), "1,234.50");
        assert_eq!(display_number(&r("2.5"), 0), "2.");
        assert_eq!(display_exact(&r("2500")), "2,500");
        assert_eq!(display_exact(&r("-0.25")), "-0.25");
        assert_eq!(display_exact(&r("1/3")), "1/3");
    }

    /// Outputs of Guile 3.0's `(format #f "~w,dF" x)` on exact numbers.
    #[test]
    fn fixed_matches_guile_format() {
        let f = |s: &str, w, d| fixed(&r(s), w, d);
        assert_eq!(f("1/2", Some(2), 1), ".5");
        assert_eq!(f("1/2", Some(2), 2), ".50");
        assert_eq!(f("1/2", Some(6), 2), "  0.50");
        assert_eq!(f("1/2", None, 2), "0.50");
        assert_eq!(f("5", Some(2), 1), "5.0");
        assert_eq!(f("57/7", Some(2), 1), "8.1");
        assert_eq!(f("-1/2", Some(2), 1), "-.5");
        assert_eq!(f("-1/2", Some(6), 2), " -0.50");
        assert_eq!(f("0", Some(2), 1), ".0");
        assert_eq!(f("1/20", Some(2), 1), ".1");
        assert_eq!(f("1/40", Some(2), 2), ".03");
        assert_eq!(f("0.25", Some(5), 1), "  0.3");
        assert_eq!(f("99999/1000", Some(2), 1), "100.0");
        assert_eq!(f("-12345/100", Some(2), 1), "-123.5");
        assert_eq!(f("95/100", Some(2), 1), "1.0");
        assert_eq!(f("-1/2000", Some(2), 1), "-.0");
        assert_eq!(f("1/3", Some(1), 0), ".");
        assert_eq!(f("1/3", Some(3), 0), " 0.");
        assert_eq!(f("5/2", Some(1), 0), "3.");
        assert_eq!(f("201/200", Some(2), 2), "1.01");
    }

    #[test]
    fn formats_come_from_the_book() {
        let mut env = red_ale_core::FixedEnv::new(red_ale_core::Time64(0), 1);
        let book = Book::new_empty(&mut env);
        let usd = CommodityFormat::from_book(&book, &CommodityRef::currency("USD")).unwrap();
        assert_eq!((usd.symbol.as_str(), usd.fraction, usd.is_iso()), ("$", 100, true));
        let jpy = CommodityFormat::from_book(&book, &CommodityRef::new(CURRENCY_NAMESPACE, "JPY")).unwrap();
        assert_eq!(money("1234.5", &jpy), "JP¥1,234.");
        assert!(CommodityFormat::from_book(&book, &CommodityRef::new("NASDAQ", "NONE")).is_none());
    }

    fn parse_printed(text: &str) -> Rational {
        let digits: String = text.chars().filter(|c| *c != ',').collect();
        Rational::parse(&digits).unwrap()
    }

    proptest! {
        /// After `5.17:libgnucash/app-utils/test/test-print-parse-amount.cpp`: a decimal printed and read back is the
        /// same value, with and without separators.
        #[test]
        fn printed_decimals_read_back(n in any::<i64>(), places in 0_u32..9, min in 0_u32..4, sep in any::<bool>()) {
            let value = Rational::new(n, 10_i64.pow(places)).unwrap();
            let info = PrintInfo { use_separators: sep, ..PrintInfo::plain(min) };
            let text = format_amount(&value, &info);
            prop_assert_eq!(parse_printed(&text), value);
            let decimals = text.split_once('.').map_or(0, |(_, f)| f.len());
            prop_assert!(decimals >= usize::try_from(min).unwrap());
        }

        #[test]
        fn remainders_read_back(whole in 0_i64..1_000_000, num in 1_i64..96, den in 97_i64..100) {
            let value = &Rational::from_int(whole) + &Rational::new(num, den).unwrap();
            let text = format_amount(&value, &PrintInfo::plain(2));
            let read = match text.split_once(" + ") {
                Some((w, f)) => &parse_printed(w) + &Rational::parse(f).unwrap(),
                None => parse_printed(&text),
            };
            prop_assert_eq!(read, value);
        }

        #[test]
        fn monetary_display_is_the_rounded_amount(n in any::<i64>(), d in 1_i64..1_000_000) {
            let amount = Rational::new(n, d).unwrap();
            let shown = display_monetary(&amount, &usd());
            prop_assert!((&shown.rounded - &amount).abs() <= Rational::new(1, 200).unwrap());
            let text = shown.display.replace('$', "");
            prop_assert_eq!(parse_printed(&text), shown.rounded);
        }
    }
}
