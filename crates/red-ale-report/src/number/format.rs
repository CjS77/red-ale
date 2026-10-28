//! Display rounding and GnuCash's amount formatting.
//!
//! The text is that of `xaccSPrintAmount` and `PrintAmountInternal` (`5.17:libgnucash/app-utils/gnc-ui-util.cpp:1083-1430`)
//! under the Locale's conventions as GnuCash reads them (`gnc_localeconv`,
//! `5.17:libgnucash/core-utils/gnc-locale-utils.c:75-108`). Under the C locale, the one GnuCash's report tests run
//! under, that is grouping by three with `,`, decimal point `.`, minus sign first.
//!
//! Reports never round a figure before using it again; these functions round only to show it.

use num_bigint::BigInt;
use num_integer::Integer;
use num_traits::Zero;
use red_ale_core::ops::commodities::{display_symbol, is_iso_namespace, lookup};
use red_ale_core::{Book, CommodityRef};
use red_ale_locale::Conventions;

use super::rational::split_decimal;
use super::{Rational, Rounding};

/// The most decimal places GnuCash prints a value with before falling back to `w + n/d`: `max_leg_digits`
/// (`5.17:libgnucash/engine/gnc-numeric.cpp:51`, used by `gnc_numeric_to_decimal`, `:1253-1271`).
const MAX_DECIMAL_PLACES: u64 = 18;

/// `CHAR_MAX` in a grouping string: no further grouping.
const NO_MORE_GROUPS: u8 = 127;

/// The places GnuCash's number renderer rounds to (`double-to-gnc-numeric datum 100`).
const NUMBER_PLACES: u32 = 2;

/// What printing needs to know about a commodity: its identity, its *display symbol* and its SCU.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommodityFormat {
    /// The commodity.
    pub id: CommodityRef,
    /// Its display symbol (GnuCash's nice symbol): the user symbol, else the ISO table's symbol, else the mnemonic.
    /// GnuCash would take the locale's `currency_symbol` for a currency whose code equals the locale's
    /// `int_curr_symbol` (`5.17:libgnucash/engine/gnc-commodity.cpp:1105-1108`), but glibc's `int_curr_symbol` ends
    /// in a space, so no code ever equals it.
    pub symbol: String,
    /// Its SCU, the denominator amounts are shown in.
    pub fraction: i64,
}

impl CommodityFormat {
    /// A commodity's format from its parts.
    pub fn new(id: CommodityRef, symbol: impl Into<String>, fraction: i64) -> Self {
        Self {
            id,
            symbol: symbol.into(),
            fraction,
        }
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
#[allow(clippy::struct_excessive_bools, reason = "the flags of GnuCash's GNCPrintAmountInfo, one for one")]
pub struct PrintInfo {
    /// The symbol to print, if any (`use_symbol` with a commodity).
    pub symbol: Option<String>,
    /// Whether the commodity is a currency. Any other commodity's symbol goes after the amount with a space.
    pub is_iso: bool,
    /// Whether the locale places the symbol and the sign (`use_locale`); if not, the symbol goes first with a space.
    pub use_locale: bool,
    /// Whether the locale's monetary separators apply (`monetary`), else its numeric ones.
    pub monetary: bool,
    /// Decimals always printed, padding with zeros.
    pub min_decimal_places: u32,
    /// Whether to group the whole part by thousands.
    pub use_separators: bool,
}

impl PrintInfo {
    /// `gnc_commodity_print_info` (`5.17:libgnucash/app-utils/gnc-ui-util.cpp:885-917`): an ISO currency pads to its
    /// SCU's decimals and follows the locale; any other commodity pads none.
    pub fn commodity(c: &CommodityFormat, use_symbol: bool) -> Self {
        let places = decimal_fraction_places(c.fraction).unwrap_or(0);
        Self {
            symbol: use_symbol.then(|| c.symbol.clone()),
            is_iso: c.is_iso(),
            use_locale: c.is_iso(),
            monetary: true,
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
            is_iso: currency.is_iso(),
            use_locale: true,
            monetary: true,
            min_decimal_places: trailing_zeros(currency.fraction),
            use_separators: true,
        }
    }

    /// No symbol, grouped, padded to `places` decimals: `gnc_default_print_info(#f)`
    /// (`5.17:libgnucash/app-utils/gnc-ui-util.cpp:833-860`) pads to the locale's `frac_digits`.
    pub fn plain(places: u32) -> Self {
        Self {
            symbol: None,
            is_iso: true,
            use_locale: true,
            monetary: true,
            min_decimal_places: places,
            use_separators: true,
        }
    }

    /// `gnc_share_print_info_places` (`5.17:libgnucash/app-utils/gnc-ui-util.cpp:1013-1023`): no symbol, the locale's
    /// numeric separators, grouped, padded to `places` decimals.
    pub fn share(places: u32) -> Self {
        Self {
            monetary: false,
            ..Self::plain(places)
        }
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
    std::iter::successors(Some(fraction), |f| (*f != 0 && f % 10 == 0).then_some(f / 10))
        .skip(1)
        .fold(0, |n, _| n + 1)
}

/// The first character of a locale string, as GnuCash copies separators and decimal points (`g_utf8_strncpy(…, 1)`).
fn first_char(s: &str) -> Option<char> {
    s.chars().next()
}

/// Where the symbol and the sign go, from the locale (`xaccSPrintAmount`, `5.17:libgnucash/app-utils/gnc-ui-util.cpp:1306-1352`).
struct Placement<'a> {
    symbol_first: bool,
    space: bool,
    sign: &'a str,
    sign_position: u8,
}

impl<'a> Placement<'a> {
    fn of(value: &Rational, info: &PrintInfo, conv: &'a Conventions) -> Self {
        let negative = value.is_negative();
        let (locale_first, locale_space) = if negative {
            (conv.n_cs_precedes, conv.n_sep_by_space)
        } else {
            (conv.p_cs_precedes, conv.p_sep_by_space)
        };
        let (mut symbol_first, mut space) = if info.use_locale {
            (locale_first, locale_space != 0)
        } else {
            (true, true)
        };
        if info.symbol.is_some() && !info.is_iso {
            (symbol_first, space) = (false, true);
        }
        let (sign, sign_position) = if negative {
            (conv.negative_sign, conv.n_sign_posn)
        } else {
            (conv.positive_sign, conv.p_sign_posn)
        };
        let sign = if value.is_zero() { "" } else { sign };
        Self {
            symbol_first,
            space,
            sign,
            sign_position,
        }
    }

    /// The sign if it goes at `position` (0 parentheses, 1 first, 2 last, 3 before the symbol, 4 after it).
    fn sign_at(&self, position: u8) -> &'a str {
        if self.sign_position == position { self.sign } else { "" }
    }

    fn symbol(&self, symbol: Option<&str>) -> String {
        let (before, after) = (self.sign_at(3), self.sign_at(4));
        match symbol {
            Some(s) if self.space && self.symbol_first => format!("{before}{s} {after}"),
            Some(s) if self.space => format!("{before} {s}{after}"),
            Some(s) => format!("{before}{s}{after}"),
            None => format!("{before}{after}"),
        }
    }
}

/// `xaccPrintAmount`: the text of an exact value under `conv`, with no rounding.
pub fn format_amount(value: &Rational, info: &PrintInfo, conv: &Conventions) -> String {
    let at = Placement::of(value, info, conv);
    let symbol = at.symbol(info.symbol.as_deref());
    let (open, close) = if at.sign_at(0).is_empty() { ("", "") } else { ("(", ")") };
    let number = print_magnitude(value, info, conv);
    let (first, last) = if at.symbol_first {
        (symbol.as_str(), "")
    } else {
        ("", symbol.as_str())
    };
    format!("{}{first}{open}{number}{close}{last}{}", at.sign_at(1), at.sign_at(2))
}

/// `PrintAmountInternal` (`5.17:libgnucash/app-utils/gnc-ui-util.cpp:1083-1300`): the magnitude with grouping and an
/// always-printed decimal point, or `w + n/d` when the value is not a decimal of at most 18 places. A negative
/// value's fraction is joined with `-`, as GnuCash does (`-$480 - 85/104`).
///
/// The point after a whole amount (`5. ACME`) matches the pin only. GnuCash added it in 5.12 to fix a register bug
/// red-ale cannot have, and red-ale will likely drop it later (`design/reports.md` §3).
fn print_magnitude(value: &Rational, info: &PrintInfo, conv: &Conventions) -> String {
    let (point, separator, grouping) = if info.monetary {
        (conv.mon_decimal_point, conv.mon_thousands_sep, conv.mon_grouping)
    } else {
        (conv.decimal_point, conv.thousands_sep, conv.grouping)
    };
    let magnitude = value.abs();
    let (whole, rem) = magnitude.numer().div_rem(magnitude.denom());
    let digits = whole.to_string();
    let whole_text = if info.use_separators {
        group_digits(&digits, first_char(separator), grouping)
    } else {
        digits
    };
    if let Some(places) = magnitude
        .decimal_places()
        .filter(|p| *p <= MAX_DECIMAL_PLACES)
        .and_then(|p| u32::try_from(p).ok())
    {
        let places = places.max(info.min_decimal_places);
        let digits = &rem * BigInt::from(10).pow(places) / magnitude.denom();
        let (_, frac) = split_decimal(&digits, places);
        let point = first_char(point).map(String::from).unwrap_or_default();
        format!("{whole_text}{point}{frac}")
    } else {
        let fraction = format!("{rem}/{}", magnitude.denom());
        match (whole.is_zero(), value.is_negative()) {
            (true, _) => fraction,
            (false, true) => format!("{whole_text} - {fraction}"),
            (false, false) => format!("{whole_text} + {fraction}"),
        }
    }
}

/// Insert `separator` between groups of digits from the right, as `PrintAmountInternal` walks `grouping`: each size
/// in turn, the last one repeating, until `CHAR_MAX` stops grouping.
fn group_digits(digits: &str, separator: Option<char>, grouping: &[u8]) -> String {
    let Some(separator) = separator else { return digits.to_owned() };
    let count = digits.chars().count();
    let (reversed, _, _) = digits.chars().rev().enumerate().fold(
        (String::with_capacity(digits.len() * 2), 0_usize, 0_u8),
        |(mut out, group, filled), (i, c)| {
            out.push(c);
            let size = grouping.get(group).copied().unwrap_or(NO_MORE_GROUPS);
            if i + 1 == count || size == NO_MORE_GROUPS {
                return (out, group, filled);
            }
            let filled = filled.saturating_add(1);
            if filled < size {
                return (out, group, filled);
            }
            out.push(separator);
            let next = if group + 1 < grouping.len() { group + 1 } else { group };
            (out, next, 0)
        },
    );
    reversed.chars().rev().collect()
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
pub fn display_monetary(amount: &Rational, commodity: &CommodityFormat, conv: &Conventions) -> MonetaryDisplay {
    let rounded = amount
        .round_to(commodity.fraction.max(1), Rounding::Bankers)
        .unwrap_or_else(|_| amount.clone());
    let display = format_amount(&rounded, &PrintInfo::commodity(commodity, true), conv);
    MonetaryDisplay { rounded, display }
}

/// GnuCash's price renderer (`5.17:gnucash/report/html-style-info.scm:221-222`): the exact price in `currency`, padded
/// to the currency's decimals and never rounded, so a rate that is not a terminating decimal prints as `$480 + 85/104`.
pub fn display_price(price: &Rational, currency: &CommodityFormat, conv: &Conventions) -> String {
    format_amount(price, &PrintInfo::price(currency), conv)
}

/// GnuCash's number renderer (`5.17:gnucash/report/html-style-info.scm:233-236`): rounded to two places with banker's
/// rounding and padded to the locale's `frac_digits`, grouped, no symbol. red-ale generalises it to `places` decimals,
/// rounded and padded to. GnuCash goes through a double here; red-ale rounds exactly.
pub fn display_number(value: &Rational, places: Option<u32>, conv: &Conventions) -> String {
    display_number_rounded(value, places, Rounding::Bankers, conv)
}

/// [`display_number`] rounding with `mode`: `HalfUp` for the rates GnuCash prints with Guile's `~,nf`.
pub fn display_number_rounded(value: &Rational, places: Option<u32>, mode: Rounding, conv: &Conventions) -> String {
    let rounded = round_places(value, places.unwrap_or(NUMBER_PLACES), mode);
    format_amount(
        &rounded,
        &PrintInfo::plain(places.unwrap_or_else(|| u32::from(conv.frac_digits))),
        conv,
    )
}

/// A share count as GnuCash's share print info prints it (`5.17:libgnucash/app-utils/gnc-ui-util.cpp:1013-1023`): the
/// magnitude rounded half up to exactly `places` decimals, default 2 (`:1105-1145`), grouped, the point always
/// printed: `1,234.568` for 1234.5675 at 3 places, `100.` at none.
pub fn display_share(value: &Rational, places: Option<u32>, conv: &Conventions) -> String {
    let places = places.unwrap_or(NUMBER_PLACES);
    format_amount(&round_places(value, places, Rounding::HalfUp), &PrintInfo::share(places), conv)
}

/// `value` rounded to `places` decimals with `mode`; unchanged if that does not fit.
fn round_places(value: &Rational, places: u32, mode: Rounding) -> Rational {
    let denom = 10_i64.checked_pow(places).unwrap_or(i64::MAX);
    value.round_to(denom, mode).unwrap_or_else(|_| value.clone())
}

/// The exact value, grouped, with as many decimals as it needs and no trailing decimal point, or `w + n/d`: how chart
/// hover labels and ticks show a `number` axis.
pub fn display_exact(value: &Rational, conv: &Conventions) -> String {
    let text = format_amount(value, &PrintInfo::plain(0), conv);
    first_char(conv.mon_decimal_point)
        .and_then(|p| text.strip_suffix(p))
        .map_or_else(|| text.clone(), str::to_owned)
}

/// Guile's `~w,dF` directive on an exact number (`format:out-fixed`, Guile 3.0 `ice-9/format.scm`): rounded half away
/// from zero to `places` decimals and padded with spaces on the left to `width`. When the whole part is zero, its `0`
/// is printed only if there is no width or the width exceeds `places + 1`, so `fixed(1/2, Some(2), 1)` is `.5`, as
/// `5.17:gnucash/report/reports/standard/account-piecharts.scm:552-554` relies on. A number wider than `width` is
/// printed in full. GnuCash formats the double nearest the value; red-ale formats the value itself. Guile's `format`
/// does not follow the locale.
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

    /// The C locale's conventions, which GnuCash's report tests run under.
    fn c() -> Conventions {
        red_ale_locale::Locale::c().conventions()
    }

    fn money(s: &str, f: &CommodityFormat) -> String {
        display_monetary(&r(s), f, &c()).display
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
        let price = |s: &str| display_price(&r(s), &usd(), &c());
        let whole_and = |w: i64, f: &str| display_price(&(&Rational::from_int(w) + &r(f)), &usd(), &c());
        assert_eq!(price("85/104"), "$85/104");
        assert_eq!(price("49925/104"), "$480 + 5/104");
        assert_eq!(whole_and(480, "85/104"), "$480 + 85/104");
        assert_eq!(whole_and(1, "49/101"), "$1 + 49/101");
        assert_eq!(whole_and(235, "3/7"), "$235 + 3/7");
        assert_eq!(whole_and(203, "1/3"), "$203 + 1/3");
        assert_eq!(price("1.7"), "$1.70");
        assert_eq!(price("1.499"), "$1.499");
        assert_eq!(price("1.455"), "$1.455");
        assert_eq!(price("300.01"), "$300.01");
        assert_eq!(
            display_price(&-(&Rational::from_int(480) + &r("85/104")), &usd(), &c()),
            "-$480 - 85/104"
        );
    }

    #[test]
    fn amounts_follow_print_amount_internal() {
        let plain = |s: &str, places| format_amount(&r(s), &PrintInfo::plain(places), &c());
        assert_eq!(plain("1234567.891", 2), "1,234,567.891");
        assert_eq!(plain("-0.5", 0), "-0.5");
        assert_eq!(plain("12", 0), "12.");
        assert_eq!(plain("123", 2), "123.00");
        assert_eq!(plain("1/3", 2), "1/3");
        assert_eq!(plain("-1/3", 2), "-1/3");
        let no_sep = PrintInfo {
            use_separators: false,
            ..PrintInfo::plain(2)
        };
        assert_eq!(format_amount(&r("1234567.5"), &no_sep, &c()), "1234567.50");
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
        let d = display_monetary(&r("0.125"), &usd(), &c());
        assert_eq!((d.rounded, d.display.as_str()), (r("0.12"), "$0.12"));
        assert_eq!(money("0.135", &usd()), "$0.14");
        assert_eq!(money("-0.125", &usd()), "-$0.12");
        assert_eq!(money("1/3", &usd()), "$0.33");
        assert_eq!(money("0.0005", &funds()), "0. FUNDS");
        assert_eq!(money("0.0015", &funds()), "0.002 FUNDS");
    }

    #[test]
    fn number_cells_round_bankers_to_places() {
        let number = |s: &str, places| display_number(&r(s), places, &c());
        assert_eq!(number("57.1428", Some(2)), "57.14");
        assert_eq!(number("0.125", Some(2)), "0.12");
        assert_eq!(number("1234.5", Some(2)), "1,234.50");
        assert_eq!(number("2.5", Some(0)), "2.");
        assert_eq!(number("57.1428", None), "57.14");
        assert_eq!(number("3", None), "3.00");
        assert_eq!(display_exact(&r("2500"), &c()), "2,500");
        assert_eq!(display_exact(&r("-0.25"), &c()), "-0.25");
        assert_eq!(display_exact(&r("1/3"), &c()), "1/3");
    }

    /// `gnc_share_print_info_places` (`5.17:libgnucash/app-utils/gnc-ui-util.cpp:1013-1023`): exactly `places`
    /// decimals, the magnitude rounded half up (`:1105-1145`), so ties go away from zero; the numeric separators, not
    /// the monetary ones, which pt_PT tells apart.
    #[test]
    fn shares_round_half_away_from_zero_to_exactly_places() {
        let share = |s: &str, places| display_share(&r(s), places, &c());
        assert_eq!(share("1234.5675", Some(3)), "1,234.568");
        assert_eq!(share("-0.005", Some(2)), "-0.01");
        assert_eq!(share("0.125", None), "0.13");
        assert_eq!(share("100", Some(0)), "100.");
        assert_eq!(share("2/3", Some(4)), "0.6667");
        assert_eq!(share("7", Some(3)), "7.000");
        // pt_PT's LC_NUMERIC has no thousands separator, which `gnc_localeconv` fills with `,`; LC_MONETARY's is `.`.
        assert_eq!(display_share(&r("1234"), Some(1), &under("pt_PT")), "1,234,0");
        let rate = |s: &str| display_number_rounded(&r(s), Some(2), Rounding::HalfUp, &c());
        assert_eq!((rate("3.625"), rate("-3.625"), rate("3.615")), ("3.63".into(), "-3.63".into(), "3.62".into()));
    }

    fn under(locale: &str) -> Conventions {
        red_ale_locale::Locale::by_name(locale).unwrap().conventions()
    }

    fn eur() -> CommodityFormat {
        CommodityFormat::new(CommodityRef::currency("EUR"), "€", 100)
    }

    /// `xaccSPrintAmount` under glibc's fr_FR: `,` for the point, NARROW NO-BREAK SPACE between groups, the symbol
    /// after the amount with a space, the sign first.
    #[test]
    fn amounts_follow_the_locale() {
        let fr = under("fr_FR");
        assert_eq!(
            display_monetary(&r("-1234567.891"), &eur(), &fr).display,
            "-1\u{202f}234\u{202f}567,89 €"
        );
        assert_eq!(display_monetary(&r("30"), &funds(), &fr).display, "30, FUNDS");
        assert_eq!(display_number(&r("1234.5"), None, &fr), "1\u{202f}234,50");
        assert_eq!(display_exact(&r("2500"), &fr), "2\u{202f}500");
        assert_eq!(display_exact(&r("2.5"), &fr), "2,5");
        assert_eq!(
            display_price(&(&Rational::from_int(480) + &r("85/104")), &eur(), &fr),
            "480 + 85/104 €"
        );
        let de = under("de_DE");
        assert_eq!(display_monetary(&r("1234.5"), &eur(), &de).display, "1.234,50 €");
        let us = under("en_US");
        assert_eq!(display_monetary(&r("-1234.5"), &usd(), &us).display, "-$1,234.50");
        // en_IN groups by three, then by two.
        assert_eq!(display_exact(&r("123456789"), &under("en_IN")), "12,34,56,789");
        // fr_CA puts a negative amount in parentheses (`n_sign_posn 0`); GnuCash brackets the number, not the symbol.
        assert_eq!(display_monetary(&r("-5"), &usd(), &under("fr_CA")).display, "(5,00) $");
        // de_CH puts the sign after the symbol (`n_sign_posn 4`), which GnuCash writes after the space.
        let chf = CommodityFormat::new(CommodityRef::currency("CHF"), "CHF", 100);
        assert_eq!(display_monetary(&r("-1234.5"), &chf, &under("de_CH")).display, "CHF -1’234.50");
        // ja_JP has no decimals in its currency, so a number cell pads none, but still prints the point.
        assert_eq!(display_number(&r("2"), None, &under("ja_JP")), "2.");
        // pt_PT sets no thousands separator in LC_NUMERIC, but reports use LC_MONETARY's `.`.
        assert_eq!(display_exact(&r("1234"), &under("pt_PT")), "1.234");
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
            let text = format_amount(&value, &info, &c());
            prop_assert_eq!(parse_printed(&text), value);
            let decimals = text.split_once('.').map_or(0, |(_, f)| f.len());
            prop_assert!(decimals >= usize::try_from(min).unwrap());
        }

        #[test]
        fn remainders_read_back(whole in 0_i64..1_000_000, num in 1_i64..96, den in 97_i64..100) {
            let value = &Rational::from_int(whole) + &Rational::new(num, den).unwrap();
            let text = format_amount(&value, &PrintInfo::plain(2), &c());
            let read = match text.split_once(" + ") {
                Some((w, f)) => &parse_printed(w) + &Rational::parse(f).unwrap(),
                None => parse_printed(&text),
            };
            prop_assert_eq!(read, value);
        }

        #[test]
        fn monetary_display_is_the_rounded_amount(n in any::<i64>(), d in 1_i64..1_000_000) {
            let amount = Rational::new(n, d).unwrap();
            let shown = display_monetary(&amount, &usd(), &c());
            prop_assert!((&shown.rounded - &amount).abs() <= Rational::new(1, 200).unwrap());
            let text = shown.display.replace('$', "");
            prop_assert_eq!(parse_printed(&text), shown.rounded);
        }

        /// Under every locale, any amount prints, and its digits are the C locale's in the same order.
        #[test]
        fn every_locale_prints_any_amount(index in 0_usize..75, n in any::<i64>(), d in 1_i64..1_000_000, iso in any::<bool>()) {
            let name = red_ale_locale::Locale::supported().nth(index).unwrap();
            let conv = under(name);
            let amount = Rational::new(n, d).unwrap();
            let commodity = if iso { usd() } else { funds() };
            let digits = |s: &str| s.chars().filter(char::is_ascii_digit).collect::<String>();
            let shown = display_monetary(&amount, &commodity, &conv);
            prop_assert_eq!(digits(&shown.display), digits(&display_monetary(&amount, &commodity, &c()).display));
            prop_assert!(!display_number(&amount, None, &conv).is_empty());
            prop_assert!(!display_exact(&amount, &conv).is_empty());
            prop_assert!(!display_price(&amount, &commodity, &conv).is_empty());
        }
    }
}
