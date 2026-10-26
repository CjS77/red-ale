//! The CSV importers' cell parsers: amounts and prices in each currency format, and dates in each date format,
//! including the `locale` format through a locale's date parser.

#![no_main]

use libfuzzer_sys::fuzz_target;
use red_ale_core::Date;
use red_ale_import::csv::{CurrencyFormat, DateContext, DateFormat, MonetaryLocale, locale_parser, parse_amount_price, parse_date, parse_monetary};
use red_ale_locale::Locale;

fuzz_target!(|data: &[u8]| {
    let text = String::from_utf8_lossy(data);
    let Some(today) = Date::from_ymd(2026, 10, 26) else { return };
    (0..3).filter_map(CurrencyFormat::from_index).for_each(|format| {
        let _ = parse_monetary(&text, format, &MonetaryLocale::default());
        let _ = parse_amount_price(&text, format, &MonetaryLocale::default());
    });
    let french = Locale::by_name("fr_FR").ok().map(|l| locale_parser(&l, today));
    (0..6).filter_map(DateFormat::from_index).for_each(|format| {
        let _ = parse_date(&text, format, &DateContext { today, locale_date: None });
        if let Some(parser) = &french {
            let _ = parse_date(&text, format, &DateContext { today, locale_date: Some(parser) });
        }
    });
});
