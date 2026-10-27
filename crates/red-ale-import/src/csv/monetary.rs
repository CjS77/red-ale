//! Amounts, values and prices in import files, parsed exactly.
//!
//! Translated from `parse_monetary` in `5.17:gnucash/import-export/csv-imp/gnc-imp-props-tx.cpp:126-170` and the
//! amount parser it calls, `xaccParseAmountInternal` and its wrappers in
//! `5.17:libgnucash/app-utils/gnc-ui-util.cpp:1667-2054` (Copyright (C) 2000 Dave Peticolas <dave@krondo.com>),
//! with the defaults of `gnc_localeconv` from `5.17:libgnucash/core-utils/gnc-locale-utils.c:76-109`. All
//! GPL-2.0-or-later; this translation is GPL-3.0-or-later.
//!
//! The result is a [`Numeric`] whose denominator is the power of ten the fraction's digits imply, as GnuCash's: the
//! text never passes through a float. Quirks kept from GnuCash:
//!
//! - an empty cell is zero; a cell with no ASCII digit is an error;
//! - currency symbols and horizontal spaces anywhere are removed first, so `1 000` and `1 000` are a thousand, and
//!   each `--` is removed, so `--5` is five;
//! - parsing stops at the first character that cannot continue a number and the rest is ignored: `12.34abc` is
//!   12.34;
//! - group separators are ignored wherever they appear before the decimal point;
//! - more than twelve fraction digits are truncated to twelve;
//! - a leading `-` or an opening parenthesis makes the amount negative, and an opening parenthesis must be closed;
//! - other Unicode decimal digits are accepted where a digit may appear but are not converted: the integer part and
//!   fraction stop at the first non-ASCII digit, though the fraction's denominator counts the bytes of every digit.
//!
//! One deviation: where the integer part, or the integer part scaled by the fraction, does not fit in 64 bits
//! GnuCash saturates or overflows silently; red-ale rejects the cell.

use red_ale_core::Numeric;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::unicode::{is_blank, is_currency_symbol, is_glib_digit, is_glib_space};

/// How amounts are written in the file: the assistant's currency format choice (`currency_format_user`,
/// `5.17:gnucash/import-export/csv-imp/gnc-import-tx.cpp:56-60`). GnuCash saves its index.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CurrencyFormat {
    /// The locale's monetary decimal point and group separator (index 0).
    #[default]
    Locale,
    /// `123,456.78` (index 1).
    Period,
    /// `123.456,78` (index 2).
    Comma,
}

impl CurrencyFormat {
    /// Every format, in GnuCash's index order.
    pub const ALL: [Self; 3] = [Self::Locale, Self::Period, Self::Comma];

    /// The index GnuCash saves (`CurrencyFormat` in the state file).
    pub fn index(self) -> usize {
        match self {
            Self::Locale => 0,
            Self::Period => 1,
            Self::Comma => 2,
        }
    }

    /// The format with GnuCash's index.
    pub fn from_index(index: usize) -> Option<Self> {
        Self::ALL.get(index).copied()
    }
}

/// The parts of the locale's `lconv` the amount parser reads, after `gnc_localeconv`'s defaults.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MonetaryLocale {
    /// The first character of `negative_sign`.
    pub negative_sign: char,
    /// The first character of `mon_decimal_point`.
    pub decimal_point: char,
    /// The first character of `mon_thousands_sep`.
    pub group_separator: char,
    /// `positive_sign`; its characters are ignored anywhere in an amount. Empty means `+`.
    pub positive_sign: String,
}

impl Default for MonetaryLocale {
    /// The C locale, whose empty fields `gnc_localeconv` fills with `-`, `.` and `,` (`gnc-locale-utils.c:86-95`).
    fn default() -> Self {
        Self::from_lconv("", "", "", "")
    }
}

impl MonetaryLocale {
    /// From the locale's `negative_sign`, `mon_decimal_point`, `mon_thousands_sep` and `positive_sign`, with
    /// `gnc_localeconv`'s defaults for empty strings.
    pub fn from_lconv(negative_sign: &str, mon_decimal_point: &str, mon_thousands_sep: &str, positive_sign: &str) -> Self {
        let first = |s: &str, default: char| s.chars().next().unwrap_or(default);
        Self {
            negative_sign: first(negative_sign, '-'),
            decimal_point: first(mon_decimal_point, '.'),
            group_separator: first(mon_thousands_sep, ','),
            positive_sign: positive_sign.to_owned(),
        }
    }
}

/// A cell that is not an amount. The messages are GnuCash's.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum MonetaryError {
    /// No ASCII digit in the cell (`gnc-imp-props-tx.cpp:138-141`).
    #[error("Value doesn't appear to contain a valid number.")]
    NoDigits,
    /// The amount parser refused the cell (`gnc-imp-props-tx.cpp:152-166`).
    #[error("Value can't be parsed into a number using the selected currency format.")]
    Unparsable,
}

/// `parse_monetary` (`gnc-imp-props-tx.cpp:132-170`): a cell of an amount, value or price column.
pub fn parse_monetary(text: &str, format: CurrencyFormat, locale: &MonetaryLocale) -> Result<Numeric, MonetaryError> {
    if text.is_empty() {
        return Ok(Numeric::ZERO);
    }
    if !text.bytes().any(|b| b.is_ascii_digit()) {
        return Err(MonetaryError::NoDigits);
    }
    let stripped = strip_symbols(text);
    parse_amount(&stripped, &rules(format, locale)).ok_or(MonetaryError::Unparsable)
}

/// `parse_amount_price` (`5.17:gnucash/import-export/csv-imp/gnc-imp-props-price.cpp:66-100`): a cell of a price
/// import's Amount column. Unlike [`parse_monetary`], an empty cell is an error (it has no digit), and only currency
/// symbols are removed: blanks and `--` stay, so `1 000` is 1 and `--5` is no number.
pub fn parse_amount_price(text: &str, format: CurrencyFormat, locale: &MonetaryLocale) -> Result<Numeric, MonetaryError> {
    if !text.bytes().any(|b| b.is_ascii_digit()) {
        return Err(MonetaryError::NoDigits);
    }
    let stripped: String = text.chars().filter(|c| !is_currency_symbol(*c)).collect();
    parse_amount(&stripped, &rules(format, locale)).ok_or(MonetaryError::Unparsable)
}

/// The characters each currency format calls the amount parser with (`gnc-imp-props-tx.cpp:149-167`).
fn rules(format: CurrencyFormat, locale: &MonetaryLocale) -> NumberRules<'_> {
    match format {
        CurrencyFormat::Locale => NumberRules {
            negative_sign: locale.negative_sign,
            decimal_point: locale.decimal_point,
            group_separator: locale.group_separator,
            ignore: if locale.positive_sign.is_empty() {
                "+"
            } else {
                &locale.positive_sign
            },
        },
        CurrencyFormat::Period => NumberRules {
            negative_sign: '-',
            decimal_point: '.',
            group_separator: ',',
            ignore: "$+",
        },
        CurrencyFormat::Comma => NumberRules {
            negative_sign: '-',
            decimal_point: ',',
            group_separator: '.',
            ignore: "$+",
        },
    }
}

/// `u32regex_replace(str, "[[:Sc:][:blank:]]|--", "")` (`gnc-imp-props-tx.cpp:143-145`).
fn strip_symbols(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if is_currency_symbol(c) || is_blank(c) {
            continue;
        }
        if c == '-' && chars.peek() == Some(&'-') {
            chars.next();
            continue;
        }
        out.push(c);
    }
    out
}

/// The characters `xaccParseAmountInternal` is called with.
struct NumberRules<'a> {
    negative_sign: char,
    decimal_point: char,
    group_separator: char,
    ignore: &'a str,
}

/// `ParseState` (`gnc-ui-util.cpp:1668-1676`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Start,
    Neg,
    Numer,
    Frac,
    Done,
    NoNum,
}

/// The parser's working state: the digits collected so far and the flags `xaccParseAmountInternal` keeps.
#[derive(Default)]
struct Machine {
    out: String,
    numer: i64,
    is_negative: bool,
    got_decimal: bool,
    need_paren: bool,
}

/// `xaccParseAmountInternal` without auto decimal places, as `xaccParseAmountImport` and `xaccParseAmountExtImport`
/// call it (`gnc-ui-util.cpp:1720-1972`). The text ends at its first NUL, as a C string would.
fn parse_amount(text: &str, rules: &NumberRules) -> Option<Numeric> {
    let text = text.split('\0').next().unwrap_or_default();
    let mut machine = Machine::default();
    let mut chars = text.chars().chain(std::iter::once('\0'));
    let mut state = State::Start;
    while !matches!(state, State::Done | State::NoNum) {
        let uc = chars.next().unwrap_or('\0');
        if uc != '\0' && rules.ignore.contains(uc) {
            continue;
        }
        let next = machine.step(state, uc, rules);
        state = machine.record_integer(state, next)?;
    }
    if machine.need_paren || state == State::NoNum {
        return None;
    }
    machine.finish()
}

impl Machine {
    /// One transition of the state machine (`gnc-ui-util.cpp:1774-1884`).
    fn step(&mut self, state: State, uc: char, rules: &NumberRules) -> State {
        match state {
            State::Start | State::Neg if is_glib_digit(uc) => {
                self.out.push(uc);
                State::Numer
            }
            State::Start | State::Neg if uc == rules.decimal_point => State::Frac,
            State::Start | State::Neg if is_glib_space(uc) => state,
            State::Start if uc == rules.negative_sign => {
                self.is_negative = true;
                State::Neg
            }
            State::Start if uc == '(' => {
                self.is_negative = true;
                self.need_paren = true;
                State::Neg
            }
            State::Numer | State::Frac if is_glib_digit(uc) => {
                self.out.push(uc);
                state
            }
            State::Numer if uc == rules.decimal_point => State::Frac,
            State::Numer if uc == rules.group_separator => state,
            State::Frac if uc == rules.decimal_point => {
                if is_glib_space(rules.decimal_point) {
                    State::Done
                } else {
                    State::NoNum
                }
            }
            State::Frac if uc == rules.group_separator => {
                if is_glib_space(rules.group_separator) {
                    State::Done
                } else {
                    state
                }
            }
            State::Numer | State::Frac if uc == ')' && self.need_paren => {
                self.need_paren = false;
                State::Done
            }
            State::Numer | State::Frac => State::Done,
            _ => State::NoNum,
        }
    }

    /// On entering the fraction, or finishing without one, convert the digits so far to the integer part
    /// (`gnc-ui-util.cpp:1886-1903`). `None` when it overflows 64 bits.
    fn record_integer(&mut self, state: State, mut next: State) -> Option<State> {
        let entering_fraction = next == State::Frac && state != State::Frac;
        if entering_fraction || (next == State::Done && !self.got_decimal) {
            match scan_digits(self.out.as_bytes()).ok()? {
                Some(value) => self.numer = value,
                None if !self.out.is_empty() => next = State::NoNum,
                None => {}
            }
            if next == State::Frac {
                self.out.clear();
                self.got_decimal = true;
            }
        }
        Some(next)
    }

    /// Scale in the fraction and apply the sign (`gnc-ui-util.cpp:1919-1964`).
    fn finish(self) -> Option<Numeric> {
        let (mut numer, mut denom) = (self.numer, 1_i64);
        if self.got_decimal && !self.out.is_empty() {
            let digits = &self.out.as_bytes()[..self.out.len().min(12)];
            let fraction = scan_digits(digits).ok()??;
            denom = 10_i64.pow(u32::try_from(digits.len()).ok()?);
            numer = numer.checked_mul(denom)?.checked_add(fraction)?;
        }
        Some(if self.is_negative {
            Numeric::new(numer, denom).neg()
        } else {
            Numeric::new(numer, denom)
        })
    }
}

/// The leading digits don't fit in 64 bits.
struct Overflow;

/// `sscanf(s, "%" SCNd64)` on a string of digits: the leading ASCII digits as a number, `None` when there are none.
fn scan_digits(s: &[u8]) -> Result<Option<i64>, Overflow> {
    let digits = &s[..s.iter().take_while(|b| b.is_ascii_digit()).count()];
    let value = digits
        .iter()
        .try_fold(0_i64, |acc, &b| acc.checked_mul(10)?.checked_add(i64::from(b - b'0')))
        .ok_or(Overflow)?;
    Ok((!digits.is_empty()).then_some(value))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str, format: usize) -> Result<Numeric, MonetaryError> {
        parse_monetary(text, CurrencyFormat::from_index(format).unwrap(), &MonetaryLocale::default())
    }

    fn same(result: Result<Numeric, MonetaryError>, num: i64, denom: i64) {
        let n = result.unwrap();
        assert!(n.same_repr(Numeric::new(num, denom)), "{n} is not {num}/{denom}");
    }

    /// `5.17:gnucash/import-export/csv-imp/test/gtest-gnc-imp-props-tx.cpp:61-102` (`ParseMonetary`).
    #[test]
    fn gnucash_parse_monetary() {
        same(parse("", 1), 0, 1);
        assert_eq!(parse("abc", 1), Err(MonetaryError::NoDigits));
        same(parse("1,000.00", 1), 100_000, 100);
        same(parse("-1,001.00", 1), -100_100, 100);
        same(parse("-1,002.00$", 1), -100_200, 100);
        same(parse("1 000 003.00", 1), 100_000_300, 100);
        same(parse("1\u{202F}000\u{202F}004.00", 1), 100_000_400, 100);
        same(parse("--1,005.00", 1), 100_500, 100);
        same(parse("2.000,00", 2), 200_000, 100);
        same(parse("2 000 001,00", 2), 200_000_100, 100);
        same(parse("2\u{202F}000\u{202F}002,00", 2), 200_000_200, 100);
        assert_eq!(parse("3000.00.01", 1), Err(MonetaryError::Unparsable));
    }

    #[test]
    fn signs_parentheses_and_trailing_text() {
        same(parse("(12.50)", 1), -1250, 100);
        assert_eq!(parse("(12.50", 1), Err(MonetaryError::Unparsable));
        same(parse("+7", 1), 7, 1);
        same(parse("12.34abc", 1), 1234, 100);
        same(parse(".5", 1), 5, 10);
        same(parse("3.", 1), 3, 1);
        same(parse("---5", 1), -5, 1);
        assert_eq!(parse("abc12", 1), Err(MonetaryError::Unparsable));
        same(parse("€ 1.234,5", 2), 12345, 10);
    }

    #[test]
    fn fraction_is_cut_at_twelve_digits() {
        same(parse("0.1234567890123", 1), 123_456_789_012, 1_000_000_000_000);
    }

    #[test]
    fn locale_format_uses_lconv() {
        let c = MonetaryLocale::default();
        let n = parse_monetary("1,234.5", CurrencyFormat::Locale, &c).unwrap();
        assert!(n.same_repr(Numeric::new(12345, 10)));
        let fr = MonetaryLocale::from_lconv("-", ",", "\u{202F}", "");
        let n = parse_monetary("1\u{202F}234,5", CurrencyFormat::Locale, &fr).unwrap();
        assert!(n.same_repr(Numeric::new(12345, 10)));
    }

    #[test]
    fn non_ascii_digits_follow_gnucash() {
        // The Arabic-Indic three is a digit to GLib but stops `sscanf`: the integer part is 1.
        same(parse("1٣.5", 1), 15, 10);
        // In the fraction it counts two bytes towards the denominator.
        same(parse("1.5٣", 1), 1005, 1000);
    }

    #[test]
    fn price_amounts() {
        let price = |text: &str, format: CurrencyFormat| parse_amount_price(text, format, &MonetaryLocale::default());
        assert_eq!(
            price("", CurrencyFormat::Period),
            Err(MonetaryError::NoDigits),
            "an empty price is an error"
        );
        same(price("$1,234.5678", CurrencyFormat::Period), 12_345_678, 10_000);
        same(price("€ 12,5", CurrencyFormat::Comma), 125, 10);
        same(price("1 000", CurrencyFormat::Period), 1, 1);
        assert_eq!(
            price("--5", CurrencyFormat::Period),
            Err(MonetaryError::Unparsable),
            "-- is not removed"
        );
        same(price("-0.5", CurrencyFormat::Period), -5, 10);
        same(price("0", CurrencyFormat::Period), 0, 1);
    }

    #[test]
    fn overflow_is_an_error() {
        assert_eq!(parse("99999999999999999999", 1), Err(MonetaryError::Unparsable));
        assert_eq!(parse("9999999999.999999999999", 1), Err(MonetaryError::Unparsable));
    }
}
