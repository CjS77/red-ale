//! `gnc-numeric-from-string`, which `qif-file:parse-price-line` reads a price with.
//!
//! Translated from `5.17:libgnucash/engine/gnc-numeric.cpp` (`gnc_numeric_from_string` and the
//! `GncNumeric(const std::string&)` constructor it calls), Copyright (C) 2000 Bill Gribble, Copyright (C) 2004 Linas
//! Vepstas <linas@linas.org>. GPL-2.0-or-later; this translation is GPL-3.0-or-later.
//!
//! Only the value matters to the QIF reader: GnuCash hands the result to Scheme, which reduces it
//! (`gnc_numeric_to_scm`, `5.17:bindings/guile/gnc-engine-guile.cpp:1729-1734`), so the result here is reduced
//! too. Where GnuCash throws or returns an error value, this returns `None`.

use std::sync::LazyLock;

use red_ale_core::Numeric;
use regex::{Captures, Regex};

use super::parse::reduce_to_numeric;

/// `max_leg_digits`. `gnc-numeric.cpp:51`.
const MAX_LEG_DIGITS: u32 = 18;

/// `powten`: 10 to `exp`, capped at 10^18. `gnc-numeric.cpp:63-69`.
fn powten(exp: usize) -> i64 {
    let exp = u32::try_from(exp).unwrap_or(MAX_LEG_DIGITS).min(MAX_LEG_DIGITS);
    10_i64.pow(exp)
}

const BEGIN: &str = "^[^-.0-9]*";
const END: &str = "[^0-9]*$";
const HEX_FRAG: &str = "(0[xX][A-Fa-f0-9]+)";
const SLASH: &str = "[ \\t]*/[ \\t]*";
const OPT_SIGNED_INT: &str = "(-?[0-9]*)";
const UNSIGNED_INT: &str = "([0-9]+)";

/// A pattern built from the constructor's fragments, wrapped in `begin` and `end`. `gnc-numeric.cpp:247-281`.
fn anchored(body: &str) -> Option<Regex> {
    Regex::new(&format!("{BEGIN}{body}{END}")).ok()
}

/// The constructor's patterns, in the order it tries them. `gnc-numeric.cpp:270-280`.
struct Patterns {
    numeral: Option<Regex>,
    hex: Option<Regex>,
    numeral_rational: Option<Regex>,
    integer_and_fraction: Option<Regex>,
    hex_rational: Option<Regex>,
    hex_over_num: Option<Regex>,
    num_over_hex: Option<Regex>,
    decimal: Option<Regex>,
    sep_decimal: Option<Regex>,
    scientific: Option<Regex>,
}

static PATTERNS: LazyLock<Patterns> = LazyLock::new(|| {
    let sep_int = |sep: &str| format!("(?:{sep}([0-9]{{3}}))?");
    let sep_decimal = |sep: &str, dec: &str| format!("(?:(-?[0-9]{{1,3}}){}(?:{dec}([0-9]+))?)", sep_int(sep).repeat(4));
    Patterns {
        numeral: anchored(OPT_SIGNED_INT),
        hex: anchored(HEX_FRAG),
        numeral_rational: anchored(&format!("{OPT_SIGNED_INT}{SLASH}{UNSIGNED_INT}")),
        integer_and_fraction: anchored(&format!("(-?){UNSIGNED_INT}[ \\t]+{UNSIGNED_INT}{SLASH}{UNSIGNED_INT}")),
        hex_rational: anchored(&format!("{HEX_FRAG}{SLASH}{HEX_FRAG}")),
        hex_over_num: anchored(&format!("{HEX_FRAG}{SLASH}{UNSIGNED_INT}")),
        num_over_hex: anchored(&format!("{OPT_SIGNED_INT}{SLASH}{HEX_FRAG}")),
        decimal: anchored(&format!("{OPT_SIGNED_INT}[.,]{UNSIGNED_INT}")),
        sep_decimal: anchored(&format!("(?:{}|{})", sep_decimal("[[:space:]'.]", ","), sep_decimal(",", "\\."))),
        scientific: Regex::new("(?:(-?[0-9]+[.,]?)|(-?[0-9]*)[.,]([0-9]+))[Ee](-?[0-9]+)").ok(),
    }
});

/// `std::stoll`: an optional sign and digits at the start of `s`; the rest is ignored. `None` where it throws (no
/// digits, or out of range).
fn stoll(s: &str) -> Option<i64> {
    let t = s.trim_start();
    let sign_len = usize::from(t.starts_with(['-', '+']));
    let digits = t[sign_len..].bytes().take_while(u8::is_ascii_digit).count();
    if digits == 0 {
        return None;
    }
    t[..sign_len + digits].parse().ok()
}

/// `std::stoll(s, nullptr, 16)` on a `0x` fragment.
fn stoll_hex(s: &str) -> Option<i64> {
    i64::from_str_radix(s.get(2..)?, 16).ok()
}

/// `fast_numeral_rational`: `num` or `num/denom` with a positive denominator, nothing else. `gnc-numeric.cpp:216-245`.
fn fast_numeral_rational(s: &str) -> Option<(i128, i128)> {
    if s.is_empty() || s.len() >= 48 {
        return None;
    }
    let int = |t: &str| (!t.starts_with('+')).then(|| t.parse::<i64>().ok()).flatten();
    match s.split_once('/') {
        None => Some((i128::from(int(s)?), 1)),
        Some((n, d)) => Some((i128::from(int(n)?), i128::from(int(d).filter(|d| *d > 0)?))),
    }
}

/// `numeric_from_decimal_match`: the integer and decimal digits as a numerator over a power of ten.
/// `gnc-numeric.cpp:154-173`.
fn numeric_from_decimal_match(integer: &str, decimal: &str) -> Option<(i128, i128)> {
    let neg = integer.starts_with('-');
    let high = if (neg && integer.len() > 1) || (!neg && !integer.is_empty()) { stoll(integer)? } else { 0 };
    let low = if decimal.is_empty() { 0 } else { stoll(decimal)? };
    let mut exp10 = decimal.len();
    let d = powten(exp10);
    let mut n = i128::from(high) * i128::from(d) + if neg { -i128::from(low) } else { i128::from(low) };
    while exp10 > MAX_LEG_DIGITS as usize {
        n /= i128::from(powten(exp10 - MAX_LEG_DIGITS as usize));
        exp10 -= MAX_LEG_DIGITS as usize;
    }
    Some((n, i128::from(d)))
}

/// `numeric_from_scientific_match`. `gnc-numeric.cpp:175-210`.
fn numeric_from_scientific_match(m: &Captures) -> Option<(i128, i128)> {
    let exp = m.get(4).map_or(Some(0), |e| e.as_str().parse::<i32>().ok())?;
    let neg_exp = exp < 0;
    let exp = exp.unsigned_abs();
    if exp >= MAX_LEG_DIGITS {
        return None;
    }
    let mult = i128::from(powten(exp as usize));
    if let Some(m1) = m.get(1) {
        let base = i128::from(stoll(m1.as_str())?);
        return Some(if neg_exp { (base, mult) } else { (mult * base, 1) });
    }
    let text = |i: usize| m.get(i).map_or("", |g| g.as_str());
    let (d_num, d_denom) = numeric_from_decimal_match(text(2), text(3))?;
    Some(if neg_exp || d_denom > mult {
        (d_num, if neg_exp { d_denom * mult } else { d_denom / mult })
    } else {
        (d_num * mult / d_denom, 1)
    })
}

/// `reduce_number_pair` without autorounding: a numerator beyond 64 bits throws. `gnc-numeric.cpp:127-152`.
fn reduce_number_pair((n, d): (i128, i128)) -> Option<(i128, i128)> {
    i64::try_from(n).ok().filter(|n| *n != i64::MIN)?;
    Some((n, d))
}

/// A `GncNumeric(num, denom)`: a zero denominator throws.
fn ratio(num: i64, denom: i64) -> Option<(i128, i128)> {
    (denom != 0).then_some((i128::from(num), i128::from(denom)))
}

/// `gnc_numeric_from_string`, reduced. `gnc-numeric.cpp:1405-1426`, `:247-389`.
pub fn gnc_numeric_from_string(s: &str) -> Option<Numeric> {
    if s.is_empty() {
        return None;
    }
    let (num, den) = fast_numeral_rational(s).or_else(|| from_patterns(s))?;
    reduce_to_numeric(num, den)
}

/// The constructor's patterns, from the most restrictive to the least. `gnc-numeric.cpp:296-389`.
fn from_patterns(s: &str) -> Option<(i128, i128)> {
    let p = &*PATTERNS;
    let caps = |re: &Option<Regex>| re.as_ref().and_then(|re| re.captures(s));
    let group = |c: &Captures, i: usize| c.get(i).map_or("", |m| m.as_str()).to_owned();
    if let Some(m) = caps(&p.hex_rational) {
        return ratio(stoll_hex(&group(&m, 1))?, stoll_hex(&group(&m, 2))?);
    }
    if let Some(m) = caps(&p.hex_over_num) {
        return ratio(stoll_hex(&group(&m, 1))?, stoll(&group(&m, 2))?);
    }
    if let Some(m) = caps(&p.num_over_hex) {
        return ratio(stoll(&group(&m, 1))?, stoll_hex(&group(&m, 2))?);
    }
    if let Some(m) = caps(&p.integer_and_fraction) {
        let (whole, num, den) = (i128::from(stoll(&group(&m, 2))?), stoll(&group(&m, 3))?, stoll(&group(&m, 4))?);
        let (num, den) = ratio(num, den)?;
        let value = whole * den + num;
        return Some((if group(&m, 1).is_empty() { value } else { -value }, den));
    }
    if let Some(m) = caps(&p.numeral_rational) {
        return ratio(stoll(&group(&m, 1))?, stoll(&group(&m, 2))?);
    }
    if let Some(m) = caps(&p.scientific).filter(|m| !has_hex_prefix(s, m)) {
        return reduce_number_pair(numeric_from_scientific_match(&m)?);
    }
    if let Some(m) = caps(&p.decimal) {
        return reduce_number_pair(numeric_from_decimal_match(&group(&m, 1), &group(&m, 2))?);
    }
    if let Some(m) = caps(&p.sep_decimal) {
        return reduce_number_pair(sep_decimal_match(&m)?);
    }
    if let Some(m) = caps(&p.hex) {
        return ratio(stoll_hex(&group(&m, 1))?, 1);
    }
    caps(&p.numeral).and_then(|m| ratio(stoll(&group(&m, 1))?, 1))
}

/// `has_hex_prefix` on the text before the scientific match: `.*0[xX]$`. `gnc-numeric.cpp:281`, `:334`.
fn has_hex_prefix(s: &str, m: &Captures) -> bool {
    m.get(0).is_some_and(|whole| s[..whole.start()].ends_with("0x") || s[..whole.start()].ends_with("0X"))
}

/// The groups of `sep_decimal`: captures 1 to 5 and 6 for the first form, 7 to 11 and 12 for the second.
/// `gnc-numeric.cpp:353-382`.
fn sep_decimal_match(m: &Captures) -> Option<(i128, i128)> {
    let join = |range: std::ops::Range<usize>| range.filter_map(|i| m.get(i)).map(|g| g.as_str()).collect::<String>();
    let (mut integer, mut decimal) = (join(1..6), join(6..7));
    if integer.is_empty() && decimal.is_empty() {
        (integer, decimal) = (join(7..12), join(12..13));
    }
    numeric_from_decimal_match(&integer, &decimal)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed(s: &str) -> Option<(i64, i64)> {
        gnc_numeric_from_string(s).map(|n| (n.num(), n.denom()))
    }

    #[test]
    fn every_pattern_compiles() {
        let p = &*PATTERNS;
        [
            &p.numeral,
            &p.hex,
            &p.numeral_rational,
            &p.integer_and_fraction,
            &p.hex_rational,
            &p.hex_over_num,
            &p.num_over_hex,
            &p.decimal,
            &p.sep_decimal,
            &p.scientific,
        ]
        .iter()
        .for_each(|re| assert!(re.is_some()));
    }

    #[test]
    fn price_values() {
        // test-qif-imp.scm:65-84 reads "1.0" as 1 and "1 3/4" as 7/4, and an empty value as no price.
        assert_eq!(parsed("1.0"), Some((1, 1)));
        assert_eq!(parsed("1 3/4"), Some((7, 4)));
        assert_eq!(parsed(""), None);
    }

    #[test]
    fn constructor_forms() {
        // gnc-numeric.cpp:290-389, form by form.
        assert_eq!(parsed("42"), Some((42, 1)));
        assert_eq!(parsed("-85/2"), Some((-85, 2)));
        assert_eq!(parsed(" 42.50 "), Some((85, 2)));
        assert_eq!(parsed("-1 1/2"), Some((-3, 2)));
        assert_eq!(parsed("3 / 4"), Some((3, 4)));
        assert_eq!(parsed("1.5e2"), Some((150, 1)));
        assert_eq!(parsed("15e-1"), Some((3, 2)));
        assert_eq!(parsed("1 234"), Some((1234, 1)));
        assert_eq!(parsed("1.234.567"), Some((1_234_567, 1)));
        assert_eq!(parsed("0x1F"), Some((31, 1)));
        assert_eq!(parsed("$12"), Some((12, 1)));
        assert_eq!(parsed("1/0"), None);
        assert_eq!(parsed("abc"), None);
        assert_eq!(parsed("99999999999999999999"), None);
    }
}
