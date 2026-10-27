//! The few Unicode character classes GnuCash's amount parsing relies on, from Unicode 15.1 (the version of the ICU
//! and GLib GnuCash 5.17 builds against on current distributions).

/// General category `Sc` (currency symbols): ICU's `[:Sc:]`.
const CURRENCY_SYMBOLS: &[(u32, u32)] = &[
    (0x24, 0x24),
    (0xA2, 0xA5),
    (0x58F, 0x58F),
    (0x60B, 0x60B),
    (0x7FE, 0x7FF),
    (0x9F2, 0x9F3),
    (0x9FB, 0x9FB),
    (0xAF1, 0xAF1),
    (0xBF9, 0xBF9),
    (0xE3F, 0xE3F),
    (0x17DB, 0x17DB),
    (0x20A0, 0x20C0),
    (0xA838, 0xA838),
    (0xFDFC, 0xFDFC),
    (0xFE69, 0xFE69),
    (0xFF04, 0xFF04),
    (0xFFE0, 0xFFE1),
    (0xFFE5, 0xFFE6),
    (0x11FDD, 0x11FE0),
    (0x1E2FF, 0x1E2FF),
    (0x1ECB0, 0x1ECB0),
];

/// General category `Zs` (space separators).
const SPACE_SEPARATORS: &[(u32, u32)] = &[
    (0x20, 0x20),
    (0xA0, 0xA0),
    (0x1680, 0x1680),
    (0x2000, 0x200A),
    (0x202F, 0x202F),
    (0x205F, 0x205F),
    (0x3000, 0x3000),
];

/// The zero of every run of general category `Nd` (decimal digits). Each run is ten code points, except the
/// mathematical digits at U+1D7CE, which are five runs back to back.
const DIGIT_ZEROS: &[u32] = &[
    0x30, 0x660, 0x6F0, 0x7C0, 0x966, 0x9E6, 0xA66, 0xAE6, 0xB66, 0xBE6, 0xC66, 0xCE6, 0xD66, 0xDE6, 0xE50, 0xED0, 0xF20, 0x1040, 0x1090,
    0x17E0, 0x1810, 0x1946, 0x19D0, 0x1A80, 0x1A90, 0x1B50, 0x1BB0, 0x1C40, 0x1C50, 0xA620, 0xA8D0, 0xA900, 0xA9D0, 0xA9F0, 0xAA50, 0xABF0,
    0xFF10, 0x104A0, 0x10D30, 0x11066, 0x110F0, 0x11136, 0x111D0, 0x112F0, 0x11450, 0x114D0, 0x11650, 0x116C0, 0x11730, 0x118E0, 0x11950,
    0x11C50, 0x11D50, 0x11DA0, 0x11F50, 0x16A60, 0x16AC0, 0x16B50, 0x1D7CE, 0x1D7D8, 0x1D7E2, 0x1D7EC, 0x1D7F6, 0x1E140, 0x1E2F0, 0x1E4F0,
    0x1E950, 0x1FBF0,
];

fn in_ranges(c: char, ranges: &[(u32, u32)]) -> bool {
    let c = u32::from(c);
    ranges.iter().any(|&(lo, hi)| (lo..=hi).contains(&c))
}

/// ICU's `[:Sc:]`.
pub(crate) fn is_currency_symbol(c: char) -> bool {
    in_ranges(c, CURRENCY_SYMBOLS)
}

/// ICU's `[:blank:]`: space separators and tab.
pub(crate) fn is_blank(c: char) -> bool {
    c == '\t' || in_ranges(c, SPACE_SEPARATORS)
}

/// `g_unichar_isspace`: tab, newline, carriage return, form feed, and the space, line and paragraph separators.
/// Vertical tab is not a space to GLib.
pub(crate) fn is_glib_space(c: char) -> bool {
    matches!(c, '\t' | '\n' | '\r' | '\x0C' | '\u{2028}' | '\u{2029}') || in_ranges(c, SPACE_SEPARATORS)
}

/// `g_unichar_isdigit`: general category `Nd`.
pub(crate) fn is_glib_digit(c: char) -> bool {
    let c = u32::from(c);
    DIGIT_ZEROS.iter().any(|&zero| (zero..zero + 10).contains(&c))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classes() {
        assert!(is_currency_symbol('$') && is_currency_symbol('€') && is_currency_symbol('£') && !is_currency_symbol('+'));
        assert!(is_blank(' ') && is_blank('\u{202F}') && is_blank('\u{A0}') && is_blank('\t') && !is_blank('\n'));
        assert!(is_glib_space('\n') && !is_glib_space('\x0B'));
        assert!(is_glib_digit('7') && is_glib_digit('٣') && is_glib_digit('\u{1D7FF}') && !is_glib_digit('a') && !is_glib_digit('½'));
    }
}
