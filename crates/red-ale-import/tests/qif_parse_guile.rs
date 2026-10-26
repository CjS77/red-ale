//! red-ale's QIF field parsers against GnuCash's own, run in Guile.
//!
//! `data/qif-parse-guile.tsv` is the output of `scripts/qif-guile-oracle/run.sh`, which runs the parsers of
//! `5.17:gnucash/import-export/qif-imp/qif-parse.scm:56-471` unchanged in Guile 3 on the inputs of
//! `scripts/qif-guile-oracle/driver.scm`. Each line is one input and what GnuCash's functions return for it.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use red_ale_core::Numeric;
use red_ale_import::qif::parse::{
    CategoryInfo, DATE_FORMATS, DateFormat, NumberFormat, QifDate, check_date_format, check_number_format, fix_year, parse_category,
    parse_date_format, parse_number_format,
};

const TABLE: &str = include_str!("data/qif-parse-guile.tsv");
const NUMBER_FORMATS: [NumberFormat; 4] = [NumberFormat::Decimal, NumberFormat::Comma, NumberFormat::Rational, NumberFormat::Integer];

/// The tokens of a written Scheme list: strings (without escapes), `#t`, `#f` and bare atoms.
fn tokens(list: &str) -> Vec<String> {
    let inner = list.trim().strip_prefix('(').and_then(|l| l.strip_suffix(')')).unwrap_or(list);
    let mut out = Vec::new();
    let mut rest = inner.trim_start();
    while !rest.is_empty() {
        if let Some(quoted) = rest.strip_prefix('"') {
            let end = quoted.find('"').expect("closing quote");
            out.push(format!("\"{}\"", &quoted[..end]));
            rest = quoted[end + 1..].trim_start();
        } else {
            let end = rest.find(' ').unwrap_or(rest.len());
            out.push(rest[..end].to_owned());
            rest = rest[end..].trim_start();
        }
    }
    out
}

fn scheme_date(text: &str) -> Option<QifDate> {
    if text == "#f" {
        return None;
    }
    let n: Vec<i64> = tokens(text).iter().map(|t| t.parse().unwrap()).collect();
    Some(QifDate { day: u32::try_from(n[0]).unwrap(), month: u32::try_from(n[1]).unwrap(), year: n[2] })
}

fn scheme_date_formats(text: &str) -> Option<Vec<DateFormat>> {
    (text != "none").then(|| tokens(text).iter().map(|t| DateFormat::from_symbol(t).unwrap()).collect())
}

fn scheme_number(text: &str) -> Numeric {
    let (num, den) = text.split_once('/').unwrap_or((text, "1"));
    Numeric::new(num.parse().unwrap(), den.parse().unwrap())
}

fn scheme_number_formats(text: &str) -> Vec<NumberFormat> {
    tokens(text).iter().map(|t| *NUMBER_FORMATS.iter().find(|f| f.symbol() == t).unwrap()).collect()
}

fn scheme_string(token: &str) -> Option<String> {
    token.strip_prefix('"').and_then(|t| t.strip_suffix('"')).map(str::to_owned)
}

fn scheme_category(text: &str) -> CategoryInfo {
    let t = tokens(text);
    CategoryInfo {
        name: scheme_string(&t[0]).unwrap(),
        is_account: t[1] == "#t",
        class: scheme_string(&t[2]).unwrap(),
        miscx_name: scheme_string(&t[3]),
        miscx_is_account: t[4] == "#t",
        miscx_class: scheme_string(&t[5]),
    }
}

fn rows(kind: &str) -> impl Iterator<Item = Vec<&'static str>> + use<'_> {
    TABLE.lines().map(|l| l.split('\t').collect::<Vec<_>>()).filter(move |cols| cols[0] == kind)
}

#[test]
fn dates_match_guile() {
    let checked = rows("D")
        .inspect(|cols| {
            let input = cols[1];
            assert_eq!(check_date_format(input, &DATE_FORMATS), scheme_date_formats(cols[2]), "check-date-format {input:?}");
            DATE_FORMATS.iter().zip(&cols[3..]).for_each(|(format, expected)| {
                assert_eq!(parse_date_format(input, *format), scheme_date(expected), "parse-date/format {input:?} {format:?}");
            });
        })
        .count();
    assert!(checked > 20);
}

#[test]
fn numbers_match_guile() {
    let checked = rows("N")
        .inspect(|cols| {
            let input = cols[1];
            assert_eq!(check_number_format(input, &NUMBER_FORMATS), scheme_number_formats(cols[2]), "check-number-format {input:?}");
            NUMBER_FORMATS.iter().zip(&cols[3..]).for_each(|(format, expected)| {
                let parsed = parse_number_format(input, *format).unwrap();
                assert!(parsed.same_value(scheme_number(expected)), "parse-number/format {input:?} {format:?}: {parsed} vs {expected}");
            });
        })
        .count();
    assert!(checked > 30);
}

#[test]
fn categories_match_guile() {
    let checked = rows("C").inspect(|cols| assert_eq!(parse_category(cols[1]).unwrap(), scheme_category(cols[2]), "{:?}", cols[1])).count();
    assert!(checked > 15);
}

#[test]
fn years_match_guile() {
    let checked = rows("Y")
        .inspect(|cols| assert_eq!(fix_year(cols[1], 50), cols[2].parse().ok(), "fix-year {:?}", cols[1]))
        .count();
    assert!(checked > 10);
}
