//! GnuCash's `Locale` date format: ICU's default date pattern per locale, read strictly.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use icu_calendar::Date;
use icu_datetime::DateTimeFormatter;
use icu_datetime::fieldsets::YMD;
use proptest::prelude::*;
use red_ale_locale::{CivilDate, Locale, LocaleDateError};
use writeable::Writeable;

fn date(y: i32, m: u8, d: u8) -> CivilDate {
    CivilDate::new(y, m, d).unwrap()
}

fn today() -> CivilDate {
    date(2026, 10, 26)
}

fn parse(locale: &str, text: &str) -> Result<CivilDate, LocaleDateError> {
    Locale::by_name(locale).unwrap().date_parser().unwrap().parse(text, today())
}

/// The ICU tag of a supported locale, as the table maps it.
fn icu_tag(name: &str) -> String {
    if name == "C" {
        "en-US-posix".to_owned()
    } else {
        name.replace('_', "-")
    }
}

/// Every Gregorian locale reads back what ICU4X prints with the same pattern, in the locale's own digits.
#[test]
fn every_locale_reads_what_icu_prints() {
    let dates = [
        date(2024, 2, 5),
        date(1999, 12, 31),
        date(2031, 7, 14),
        date(1400, 1, 1),
        date(2000, 5, 29),
    ];
    let checked: usize = Locale::supported()
        .filter_map(|name| Some((name, Locale::by_name(name).unwrap().date_parser().ok()?)))
        .map(|(name, parser)| {
            let formatter =
                DateTimeFormatter::try_new((&icu_tag(name).parse::<icu_locale_core::Locale>().unwrap()).into(), YMD::medium()).unwrap();
            dates.iter().for_each(|d| {
                let iso = Date::try_new_iso(d.year(), d.month(), d.day()).unwrap();
                let text = formatter.format(&iso).write_to_string().into_owned();
                assert_eq!(parser.parse(&text, today()), Ok(*d), "{name}: {text:?} with {}", parser.pattern());
            });
            1
        })
        .sum();
    assert_eq!(checked, 74, "every supported locale but fa_IR");
}

#[test]
fn patterns() {
    let pattern = |name: &str| Locale::by_name(name).unwrap().icu_date_pattern().unwrap();
    assert_eq!(pattern("C"), "MMM d, y");
    assert_eq!(pattern("en_US"), "MMM d, y");
    assert_eq!(pattern("en_GB"), "d MMM y");
    assert_eq!(pattern("fr_FR"), "d MMM y");
    assert_eq!(pattern("de_DE"), "dd.MM.y");
    assert_eq!(pattern("ja_JP"), "y/MM/dd");
}

#[test]
fn english() {
    assert_eq!(parse("en_US", "Feb 5, 2024"), Ok(date(2024, 2, 5)));
    assert_eq!(parse("C", "feb 5, 2024"), Ok(date(2024, 2, 5)));
    assert_eq!(parse("en_US", "February 5, 2024"), Ok(date(2024, 2, 5)), "the wide name too");
    assert_eq!(parse("en_US", "  Feb  5,2024 "), Ok(date(2024, 2, 5)), "spaces are loose");
    assert_eq!(parse("en_US", "Feb 5, 24"), Ok(date(2024, 2, 5)));
    assert_eq!(parse("en_US", "Feb 5, 50"), Ok(date(1950, 2, 5)));
    assert_eq!(parse("en_GB", "5 Sept 2024"), Ok(date(2024, 9, 5)));
    assert_eq!(parse("en_GB", "5 September 2024"), Ok(date(2024, 9, 5)));
    let no_match = Err(LocaleDateError::NoMatch {
        pattern: "MMM d, y".to_owned(),
    });
    assert_eq!(parse("en_US", "Feb 5 2024"), no_match, "the comma is literal text");
    assert_eq!(parse("en_US", "2 5, 2024"), no_match, "a number for a month name");
    assert_eq!(parse("en_US", "Feb 5, 2024 x"), no_match, "trailing text");
    assert_eq!(parse("en_US", "Feb 123, 2024"), no_match);
    assert_eq!(
        parse("en_US", "Feb 30, 2024"),
        Err(LocaleDateError::Day),
        "ICU would roll over to March"
    );
    assert_eq!(parse("en_US", "Feb 5, 0024"), Err(LocaleDateError::Year));
}

#[test]
fn other_languages() {
    assert_eq!(parse("fr_FR", "5 févr. 2024"), Ok(date(2024, 2, 5)));
    assert_eq!(parse("fr_FR", "5 FÉVRIER 2024"), Ok(date(2024, 2, 5)));
    assert_eq!(parse("fr_FR", "5 juil. 2024"), Ok(date(2024, 7, 5)));
    assert_eq!(parse("fr_FR", "5 juin 2024"), Ok(date(2024, 6, 5)));
    assert!(
        parse("fr_FR", "5 févr 2024").is_err(),
        "the abbreviation's point is part of the name"
    );
    assert_eq!(parse("de_DE", "05.02.2024"), Ok(date(2024, 2, 5)));
    assert_eq!(parse("de_DE", "5.2.2024"), Ok(date(2024, 2, 5)));
    assert_eq!(parse("ja_JP", "2024/02/05"), Ok(date(2024, 2, 5)));
    assert_eq!(parse("zh_CN", "2024年2月5日"), Ok(date(2024, 2, 5)));
    assert_eq!(parse("pt_BR", "5 de fev. de 2024"), Ok(date(2024, 2, 5)));
    assert_eq!(parse("ca_ES", "5 de febr. del 2024"), Ok(date(2024, 2, 5)));
    assert_eq!(parse("ru_RU", "5 февр. 2024 г."), Ok(date(2024, 2, 5)));
    assert_eq!(parse("ar_EG", "05/02/2024"), Ok(date(2024, 2, 5)), "ASCII digits, no bidi marks");
    assert_eq!(
        parse(
            "ar_EG",
            "\u{0660}\u{0665}\u{200f}/\u{0660}\u{0662}\u{200f}/\u{0662}\u{0660}\u{0662}\u{0664}"
        ),
        Ok(date(2024, 2, 5))
    );
}

#[test]
fn a_calendar_that_is_not_gregorian_is_refused() {
    let err = Locale::by_name("fa_IR").unwrap().date_parser().unwrap_err();
    assert_eq!(
        err,
        LocaleDateError::Calendar {
            calendar: "Persian".to_owned()
        }
    );
}

proptest! {
    #[test]
    fn no_text_panics(text in "\\PC{0,30}", locale in prop::sample::select(vec!["C", "fr_FR", "ar_EG", "zh_CN", "pt_BR", "eu_ES"])) {
        let _ = parse(locale, &text);
    }
}
