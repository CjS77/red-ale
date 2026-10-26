//! GnuCash's view of a locale's number conventions: `gnc_localeconv`
//! (`5.17:libgnucash/core-utils/gnc-locale-utils.c:75-108`), which takes `localeconv` and replaces every empty
//! string and every `CHAR_MAX` with a default. The defaults are those of the US, so under the C locale GnuCash prints
//! `-$1,234.56`.
//!
//! An empty value is replaced even where the locale means "none": a locale with no thousands separator still groups
//! with `,`, and one with no grouping still groups by three. That is what GnuCash prints, so red-ale keeps it.

use crate::data::{Monetary, Numeric};

/// `localeconv` with GnuCash's defaults applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(clippy::struct_field_names)]
pub struct Conventions {
    /// `decimal_point`, default `.`.
    pub decimal_point: &'static str,
    /// `thousands_sep`, default `,`.
    pub thousands_sep: &'static str,
    /// `grouping`, default three.
    pub grouping: &'static [u8],
    /// `int_curr_symbol`, default `USD `.
    pub int_curr_symbol: &'static str,
    /// `currency_symbol`, default `$`.
    pub currency_symbol: &'static str,
    /// `mon_decimal_point`, default `.`.
    pub mon_decimal_point: &'static str,
    /// `mon_thousands_sep`, default `,`.
    pub mon_thousands_sep: &'static str,
    /// `mon_grouping`, default three.
    pub mon_grouping: &'static [u8],
    /// `negative_sign`, default `-`.
    pub negative_sign: &'static str,
    /// `positive_sign`, default empty.
    pub positive_sign: &'static str,
    /// `frac_digits`, default 2.
    pub frac_digits: u8,
    /// `int_frac_digits`, default 2.
    pub int_frac_digits: u8,
    /// `p_cs_precedes`, default true.
    pub p_cs_precedes: bool,
    /// `p_sep_by_space`, default 0.
    pub p_sep_by_space: u8,
    /// `n_cs_precedes`, default true.
    pub n_cs_precedes: bool,
    /// `n_sep_by_space`, default 0.
    pub n_sep_by_space: u8,
    /// `p_sign_posn`, default 1 (sign first).
    pub p_sign_posn: u8,
    /// `n_sign_posn`, default 1 (sign first).
    pub n_sign_posn: u8,
}

/// Grouping by three, GnuCash's default.
const BY_THREE: &[u8] = &[3];

fn or(value: &'static str, default: &'static str) -> &'static str {
    if value.is_empty() { default } else { value }
}

fn or_group(value: &'static [u8]) -> &'static [u8] {
    if value.is_empty() { BY_THREE } else { value }
}

impl Conventions {
    /// `gnc_localeconv` over LC_NUMERIC and LC_MONETARY values.
    pub(crate) fn new(n: &Numeric, m: &Monetary) -> Self {
        Self {
            decimal_point: or(n.decimal_point, "."),
            thousands_sep: or(n.thousands_sep, ","),
            grouping: or_group(n.grouping),
            int_curr_symbol: or(m.int_curr_symbol, "USD "),
            currency_symbol: or(m.currency_symbol, "$"),
            mon_decimal_point: or(m.mon_decimal_point, "."),
            mon_thousands_sep: or(m.mon_thousands_sep, ","),
            mon_grouping: or_group(m.mon_grouping),
            negative_sign: or(m.negative_sign, "-"),
            positive_sign: m.positive_sign,
            frac_digits: m.frac_digits.unwrap_or(2),
            int_frac_digits: m.int_frac_digits.unwrap_or(2),
            p_cs_precedes: m.p_cs_precedes.unwrap_or(1) != 0,
            p_sep_by_space: m.p_sep_by_space.unwrap_or(0),
            n_cs_precedes: m.n_cs_precedes.unwrap_or(1) != 0,
            n_sep_by_space: m.n_sep_by_space.unwrap_or(0),
            p_sign_posn: m.p_sign_posn.unwrap_or(1),
            n_sign_posn: m.n_sign_posn.unwrap_or(1),
        }
    }

    /// `gnc_locale_default_iso_currency_code` (`5.17:libgnucash/core-utils/gnc-locale-utils.c:110-131`): the
    /// ISO 4217 code of the locale's currency, `int_curr_symbol` without its trailing space; `USD` under C.
    pub fn currency_code(&self) -> &'static str {
        self.int_curr_symbol.trim()
    }
}
