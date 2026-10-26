//! Spot checks of the Locale table against glibc's sources, and the lookups over it.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use proptest::prelude::*;
use red_ale_locale::{Category, CivilDate, Locale, LocaleError, Weekday, day_name};

fn locale(name: &str) -> Locale {
    Locale::by_name(name).unwrap()
}

fn date(y: i32, m: u8, d: u8) -> CivilDate {
    CivilDate::new(y, m, d).unwrap()
}

#[test]
fn the_required_locales_are_supported() {
    let required = [
        "C", "en_US", "en_GB", "en_AU", "en_CA", "en_IN", "fr_FR", "fr_CA", "de_DE", "de_CH", "es_ES", "es_MX", "it_IT",
        "nl_NL", "pt_BR", "pt_PT", "ja_JP", "zh_CN", "ru_RU", "sv_SE", "nb_NO", "da_DK", "fi_FI", "pl_PL", "cs_CZ",
    ];
    let supported: Vec<&str> = Locale::supported().collect();
    required.iter().for_each(|r| assert!(supported.contains(r), "{r} is not supported"));
    assert!(supported.windows(2).all(|w| w[0] < w[1]), "names are sorted and unique");
    assert_eq!(supported.len(), 75);
}

#[test]
fn names_take_a_utf8_codeset() {
    assert_eq!(locale("fr_FR.UTF-8"), locale("fr_FR"));
    assert_eq!(locale("fr_FR.utf8"), locale("fr_FR"));
    assert_eq!(locale("POSIX"), Locale::c());
    assert_eq!(locale("C.UTF-8"), Locale::c());
    assert_eq!(Locale::by_name("fr_FR.ISO-8859-1"), Err(LocaleError::Unsupported("fr_FR.ISO-8859-1".to_owned())));
    assert!(Locale::by_name("sr_RS@latin").is_err());
    assert!(Locale::by_name("xx_YY").is_err());
    assert!(Locale::by_name("").is_err());
    assert_eq!(locale("de_DE").name(), "de_DE");
}

/// `fr_FR` in glibc's sources: `decimal_point ","`, `thousands_sep "<U202F>"`, `d_fmt "%d//%m//%Y"`, `day "dimanche";
/// "lundi";…`, `int_curr_symbol "EUR "`, symbol after the amount with a space.
#[test]
fn french_conventions() {
    let fr = locale("fr_FR");
    assert_eq!((fr.numeric().decimal_point, fr.numeric().thousands_sep, fr.numeric().grouping), (",", "\u{202f}", &[3_u8][..]));
    let m = fr.monetary();
    assert_eq!((m.mon_decimal_point, m.mon_thousands_sep, m.currency_symbol, m.int_curr_symbol), (",", "\u{202f}", "€", "EUR "));
    assert_eq!((m.p_cs_precedes, m.p_sep_by_space, m.n_sign_posn), (Some(0), Some(1), Some(1)));
    assert_eq!(fr.time().d_fmt, "%d/%m/%Y");
    assert_eq!(fr.day_name(Weekday::Monday), "lundi");
    assert_eq!(day_name(&fr, Weekday::Sunday), "dimanche");
    assert_eq!(fr.abbreviated_day_name(Weekday::Saturday), "sam.");
    assert_eq!(fr.conventions().currency_code(), "EUR");
}

#[test]
fn other_spot_checks() {
    assert_eq!(locale("en_US").time().d_fmt, "%m/%d/%Y");
    assert_eq!(locale("en_GB").time().d_fmt, "%d/%m/%y");
    let de = locale("de_DE");
    assert_eq!((de.numeric().decimal_point, de.numeric().thousands_sep, de.monetary().mon_thousands_sep), (",", ".", "."));
    assert_eq!(de.time().d_fmt, "%d.%m.%Y");
    assert_eq!(locale("de_CH").numeric().thousands_sep, "’");
    assert_eq!(locale("en_IN").numeric().grouping, &[3, 2]);
    assert_eq!(locale("ja_JP").time().d_fmt, "%Y年%m月%d日");
    assert_eq!(locale("ja_JP").monetary().frac_digits, Some(0));
    assert_eq!(locale("ru_RU").day_name(Weekday::Wednesday), "Среда");
    assert_eq!(locale("en_US").day_name(Weekday::Friday), "Friday");
}

/// glibc's built-in C locale, as `localeconv` reports it, and GnuCash's defaults over it.
#[test]
fn the_c_locale_is_glibcs_with_gnucash_defaults() {
    let c = Locale::c();
    assert_eq!((c.numeric().thousands_sep, c.numeric().grouping, c.monetary().frac_digits), ("", &[][..], None));
    let conv = c.conventions();
    assert_eq!((conv.mon_decimal_point, conv.mon_thousands_sep, conv.mon_grouping), (".", ",", &[3_u8][..]));
    assert_eq!((conv.negative_sign, conv.frac_digits, conv.n_cs_precedes, conv.n_sign_posn), ("-", 2, true, 1));
    assert_eq!(conv.currency_code(), "USD");
    assert_eq!(c.time().d_fmt, "%m/%d/%y");
    assert_eq!(c.day_name(Weekday::Thursday), "Thursday");
}

#[test]
fn dates_print_in_the_locale_format() {
    let d = date(2026, 12, 31);
    assert_eq!(Locale::c().print_date(d), "12/31/26");
    assert_eq!(locale("en_US").print_date(d), "12/31/2026");
    assert_eq!(locale("fr_FR").print_date(d), "31/12/2026");
    assert_eq!(locale("de_DE").print_date(date(2026, 3, 5)), "05.03.2026");
    assert_eq!(locale("ja_JP").print_date(d), "2026年12月31日");
    // `%-d/%-m/%y`: GnuCash drops the `-`, so the day and month are zero-padded.
    assert_eq!(locale("en_AU").time().d_fmt, "%d/%m/%y");
    let padded = Locale::supported().map(locale).find(|l| l.time().d_fmt == "%-d/%-m/%y").unwrap();
    assert_eq!(padded.print_date(date(2026, 3, 5)), "05/03/26");
    assert_eq!(locale("eu_ES").print_date(date(2026, 3, 5)), "og., 2026.eko marren 05a");
}

#[test]
fn the_first_day_of_the_week_comes_from_icu() {
    assert_eq!(Locale::c().first_weekday(), Weekday::Sunday);
    assert_eq!(locale("en_US").first_weekday(), Weekday::Sunday);
    assert_eq!(locale("en_GB").first_weekday(), Weekday::Monday);
    assert_eq!(locale("fr_FR").first_weekday(), Weekday::Monday);
    assert_eq!(locale("fa_IR").first_weekday(), Weekday::Saturday);
}

/// ICU 74's `DateIntervalFormat` for `yMd` gives the same for these (checked against the oracle's ICU).
#[test]
fn date_intervals_come_from_icu() {
    let (from, to) = (date(2026, 1, 1), date(2026, 12, 31));
    assert_eq!(locale("en_US").date_interval(from, to).unwrap(), "1/1/2026\u{2009}–\u{2009}12/31/2026");
    assert_eq!(locale("en_US").date_interval(to, from).unwrap(), "1/1/2026\u{2009}–\u{2009}12/31/2026");
    assert_eq!(locale("en_GB").date_interval(from, to).unwrap(), "01/01/2026\u{2009}–\u{2009}31/12/2026");
    assert_eq!(locale("de_DE").date_interval(from, to).unwrap(), "01.01.\u{2009}–\u{2009}31.12.2026");
    assert_eq!(locale("de_DE").date_interval(date(2026, 3, 1), date(2026, 3, 31)).unwrap(), "01.–31.03.2026");
    assert_eq!(locale("ja_JP").date_interval(from, to).unwrap(), "2026/01/01～2026/12/31");
    Locale::supported().map(locale).for_each(|l| assert!(l.date_interval(from, to).is_some(), "{l:?}"));
}

fn env(vars: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
    let vars: Vec<(String, String)> = vars.iter().map(|(k, v)| ((*k).to_owned(), (*v).to_owned())).collect();
    move |name| vars.iter().find(|(k, _)| k == name).map(|(_, v)| v.clone())
}

#[test]
fn the_environment_selects_a_locale_per_category() {
    assert_eq!(Locale::from_environment(env(&[])).unwrap(), Locale::c());
    assert_eq!(Locale::from_environment(env(&[("LANG", "fr_FR.UTF-8")])).unwrap(), locale("fr_FR"));
    let all = env(&[("LANG", "fr_FR.UTF-8"), ("LC_TIME", "en_GB.UTF-8"), ("LC_ALL", "de_DE.utf8")]);
    assert_eq!(Locale::from_environment(all).unwrap(), locale("de_DE"));
    let mixed = Locale::from_environment(env(&[("LANG", "fr_FR.UTF-8"), ("LC_TIME", "en_GB.UTF-8"), ("LC_ALL", "")])).unwrap();
    assert_eq!(mixed.time().d_fmt, "%d/%m/%y");
    assert_eq!(mixed.numeric().decimal_point, ",");
    assert!(!mixed.is_c(Category::Messages));
    assert_eq!(mixed.name(), "LC_NUMERIC=fr_FR;LC_MONETARY=fr_FR;LC_TIME=en_GB;LC_MESSAGES=fr_FR");
    let bad = Locale::from_environment(env(&[("LANG", "xx_YY.UTF-8")]));
    assert_eq!(bad, Err(LocaleError::UnsupportedInEnvironment { variable: "LANG", name: "xx_YY.UTF-8".to_owned() }));
}

/// ICU is asked only about the years GnuCash's dates hold; outside them the caller falls back to `<from> to <to>`.
#[test]
fn every_locale_formats_intervals_across_gnucash_years() {
    let edges = [(1400, 1, 1), (1582, 10, 15), (1900, 2, 28), (2000, 2, 29), (2026, 12, 31), (9999, 12, 31)];
    Locale::supported().map(locale).for_each(|l| {
        edges.iter().for_each(|&(y, m, d)| {
            let to = date(y, m, d);
            [date(y, 1, 1), date(1400, 1, 1), to].iter().for_each(|from| {
                assert!(l.date_interval(*from, to).is_some_and(|s| !s.is_empty()), "{l:?} {from:?} {to:?}");
            });
        });
    });
    assert_eq!(locale("es_MX").date_interval(date(0, 1, 1), date(0, 1, 3)), None);
    assert_eq!(locale("en_US").date_interval(date(1399, 12, 31), date(2026, 1, 1)), None);
    assert_eq!(locale("en_US").date_interval(date(2026, 1, 1), date(10_000, 1, 1)), None);
}

proptest! {
    /// Every locale prints any date and any date range without failing.
    #[test]
    fn every_locale_prints_any_date(index in 0_usize..75, y in -9999_i32..=99_999, m in 1_u8..=12, d in 1_u8..=28, later in 0_u8..28) {
        let l = locale(Locale::supported().nth(index).unwrap());
        let from = date(y, m, d);
        prop_assert!(!l.print_date(from).is_empty());
        let to = date(y, m, d.max(later));
        prop_assert!(l.date_interval(from, to).is_none_or(|s| !s.is_empty()));
        prop_assert!(!l.day_name(from.weekday()).is_empty());
    }
}
