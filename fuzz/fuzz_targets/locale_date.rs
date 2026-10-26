//! The locale date parser behind the `locale` date format, under locales with different patterns and scripts.

#![no_main]

use libfuzzer_sys::fuzz_target;
use red_ale_locale::{CivilDate, Locale};

const LOCALES: [&str; 8] = ["C", "en_US", "fr_FR", "de_DE", "ar_EG", "zh_CN", "pt_BR", "eu_ES"];

fuzz_target!(|data: &[u8]| {
    let Some((first, rest)) = data.split_first() else { return };
    let text = String::from_utf8_lossy(rest);
    let Ok(locale) = Locale::by_name(LOCALES[usize::from(first % 8)]) else { return };
    let Ok(parser) = locale.date_parser() else { return };
    let Some(today) = CivilDate::new(2026, 10, 26) else { return };
    let _ = parser.parse(&text, today);
});
