//! The values of one locale as glibc compiles them from its locale sources: what `localeconv`, `nl_langinfo` and
//! `strftime` give a program running under it. `None` is glibc's `CHAR_MAX`, "not available in this locale".

/// LC_NUMERIC: how numbers that are not money are written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Numeric {
    /// `decimal_point`.
    pub decimal_point: &'static str,
    /// `thousands_sep`; empty when the locale sets none.
    pub thousands_sep: &'static str,
    /// `grouping`: group sizes from the right, the last repeating; `127` (`CHAR_MAX`) stops grouping. Empty when the
    /// locale groups nothing.
    pub grouping: &'static [u8],
}

/// LC_MONETARY: how money is written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(clippy::struct_field_names)]
pub struct Monetary {
    /// `int_curr_symbol`: the ISO 4217 code and a separator, `"EUR "`.
    pub int_curr_symbol: &'static str,
    /// `currency_symbol`, `"€"`.
    pub currency_symbol: &'static str,
    /// `mon_decimal_point`.
    pub mon_decimal_point: &'static str,
    /// `mon_thousands_sep`.
    pub mon_thousands_sep: &'static str,
    /// `mon_grouping`, as [`Numeric::grouping`].
    pub mon_grouping: &'static [u8],
    /// `positive_sign`.
    pub positive_sign: &'static str,
    /// `negative_sign`.
    pub negative_sign: &'static str,
    /// `int_frac_digits`.
    pub int_frac_digits: Option<u8>,
    /// `frac_digits`: the decimals of the local currency.
    pub frac_digits: Option<u8>,
    /// `p_cs_precedes`: 1 when the symbol goes before a non-negative amount.
    pub p_cs_precedes: Option<u8>,
    /// `p_sep_by_space`: 0 no space between symbol and amount, 1 a space, 2 a space next to the sign.
    pub p_sep_by_space: Option<u8>,
    /// `n_cs_precedes`, for a negative amount.
    pub n_cs_precedes: Option<u8>,
    /// `n_sep_by_space`, for a negative amount.
    pub n_sep_by_space: Option<u8>,
    /// `p_sign_posn`: 0 parentheses, 1 sign before amount and symbol, 2 after both, 3 just before the symbol, 4 just
    /// after it.
    pub p_sign_posn: Option<u8>,
    /// `n_sign_posn`, for a negative amount.
    pub n_sign_posn: Option<u8>,
}

/// LC_TIME: the date format and the names `strftime` prints.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Time {
    /// `D_FMT`, the date format `nl_langinfo` gives: what GnuCash's `locale` date format prints.
    pub d_fmt: &'static str,
    /// `day`: full day names from Sunday, `%A`.
    pub day: [&'static str; 7],
    /// `abday`: abbreviated day names from Sunday, `%a`.
    pub abday: [&'static str; 7],
    /// `mon`: full month names from January, `%B`.
    pub mon: [&'static str; 12],
    /// `abmon`: abbreviated month names from January, `%b`.
    pub abmon: [&'static str; 12],
}

/// One supported locale.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocaleData {
    /// The glibc name, `fr_FR`; `C` for the C locale.
    pub name: &'static str,
    /// The locale ICU takes from the same environment, as a BCP 47 tag (`fr-FR`); ICU reads `C` as `en-US-posix`.
    pub icu: &'static str,
    /// LC_NUMERIC.
    pub numeric: Numeric,
    /// LC_MONETARY.
    pub monetary: Monetary,
    /// LC_TIME.
    pub time: Time,
}
