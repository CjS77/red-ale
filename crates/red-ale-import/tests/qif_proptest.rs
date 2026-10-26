//! No input makes the QIF reader panic: arbitrary text, valid QIF mutated line by line and character by character,
//! and arbitrary values for every field parser (`design/milestone3.md`, "Testing").

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use proptest::prelude::*;
use red_ale_import::qif::numeric_string::gnc_numeric_from_string;
use red_ale_import::qif::parse::{
    DATE_FORMATS, NumberFormat, check_date_format, check_number_format, exact_number, fix_year, parse_acct_type, parse_action_field,
    parse_bang_field, parse_category, parse_cleared_field, parse_date_format, parse_number_format,
};
use red_ale_import::qif::{ReadOptions, check_from_acct, parse_fields, read_file, reparse_dates, reparse_price_dates};

const NUMBER_FORMATS: [NumberFormat; 4] = [NumberFormat::Decimal, NumberFormat::Comma, NumberFormat::Rational, NumberFormat::Integer];

/// A valid file touching every section, to mutate.
const SEED: &str = "!Option:AutoSwitch\n!Account\nNChecking\nTBank\nL1,000.00\n^\n!Clear:AutoSwitch\n!Type:Class\nNHome\nDHouse\n^\n\
!Type:Cat\nNFood\nE\nR123\nB50,00\n^\n!Type:Security\nNAcme\nSACM\nTStock\n^\n!Type:Bank\nD1/2'03\nT-1,234.50\nPShop\nAStreet\nN101\nC*\n\
MMemo\nLFood/Home\nSFood\nE1\n$-1,000.00\nS[Savings]\n$-234.50\n^\n!Type:Invst\nD19990131\nNBuyX\nYAcme\nI12.5\nQ10\nO1.00\nU126.00\n\
L[Checking]|[Savings]/Cls\n^\n!Type:Memorized\nKC\nT1\n^\n!Type:Prices\n\"ACM\",12 1/2,\"1/31/99\"\n^\n";

/// Run every step a caller runs, with every date format; only panics matter.
fn exercise(text: &str) {
    let Ok(result) = read_file(text, &ReadOptions::default()) else { return };
    let mut file = result.file;
    let _ = check_from_acct(&file);
    let _ = file.path_to_accountname();
    let report = parse_fields(&mut file);
    if let Ok(report) = report {
        let _ = report.date_formats();
    }
    DATE_FORMATS.iter().for_each(|format| {
        let mut copy = file.clone();
        let _ = reparse_dates(&mut copy, *format);
        let _ = reparse_price_dates(&mut copy, *format);
    });
}

/// One edit to a list of lines.
#[derive(Debug, Clone)]
enum Mutation {
    Delete(usize),
    Duplicate(usize),
    Swap(usize, usize),
    Replace(usize, String),
    InsertChar(usize, usize, char),
    Truncate(usize, usize),
}

fn mutation() -> impl Strategy<Value = Mutation> {
    let line = 0..80_usize;
    prop_oneof![
        line.clone().prop_map(Mutation::Delete),
        line.clone().prop_map(Mutation::Duplicate),
        (line.clone(), line.clone()).prop_map(|(a, b)| Mutation::Swap(a, b)),
        (line.clone(), "[!A-Z$^\\[\\]/|0-9.,' -]{0,12}").prop_map(|(i, s)| Mutation::Replace(i, s)),
        (line.clone(), 0..20_usize, any::<char>()).prop_map(|(i, at, c)| Mutation::InsertChar(i, at, c)),
        (line, 0..20_usize).prop_map(|(i, at)| Mutation::Truncate(i, at)),
    ]
}

fn apply(lines: &mut Vec<String>, m: &Mutation) {
    let len = lines.len().max(1);
    match m {
        Mutation::Delete(i) if !lines.is_empty() => {
            lines.remove(i % len);
        }
        Mutation::Duplicate(i) if !lines.is_empty() => lines.insert(i % len, lines[i % len].clone()),
        Mutation::Swap(a, b) if !lines.is_empty() => lines.swap(a % len, b % len),
        Mutation::Replace(i, s) if !lines.is_empty() => lines[i % len].clone_from(s),
        Mutation::InsertChar(i, at, c) if !lines.is_empty() => {
            let line = &mut lines[i % len];
            let at = line.char_indices().nth(*at).map_or(line.len(), |(b, _)| b);
            line.insert(at, *c);
        }
        Mutation::Truncate(i, at) if !lines.is_empty() => {
            let line = &mut lines[i % len];
            let at = line.char_indices().nth(*at).map_or(line.len(), |(b, _)| b);
            line.truncate(at);
        }
        _ => {}
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    #[test]
    fn arbitrary_text(text in any::<String>()) {
        exercise(&text);
    }

    #[test]
    fn qif_shaped_text(text in "(![A-Za-z: ]{0,14}\n|[A-Z$^!][-0-9/.,' $\\[\\]|A-Za-z]{0,14}\r?\n){0,40}") {
        exercise(&text);
    }

    #[test]
    fn mutated_valid_qif(mutations in prop::collection::vec(mutation(), 1..12)) {
        let mut lines: Vec<String> = SEED.lines().map(str::to_owned).collect();
        mutations.iter().for_each(|m| apply(&mut lines, m));
        exercise(&lines.join("\n"));
    }

    #[test]
    fn field_parsers(s in any::<String>()) {
        let _ = parse_category(&s);
        let _ = fix_year(&s, 50);
        let _ = parse_acct_type(&s, |_| {});
        let _ = parse_action_field(&s, |_| {});
        let _ = parse_cleared_field(&s, |_| {});
        let _ = parse_bang_field(&s);
        let _ = check_date_format(&s, &DATE_FORMATS);
        DATE_FORMATS.iter().for_each(|f| { let _ = parse_date_format(&s, *f); });
        let _ = check_number_format(&s, &NUMBER_FORMATS);
        NUMBER_FORMATS.iter().for_each(|f| { let _ = parse_number_format(&s, *f); });
        let _ = exact_number(&s);
        let _ = gnc_numeric_from_string(&s);
    }

    #[test]
    fn numeric_text(s in "[-+$ 0-9.,'/eExX]{0,40}") {
        let _ = gnc_numeric_from_string(&s);
        let _ = exact_number(&s);
        NUMBER_FORMATS.iter().for_each(|f| { let _ = parse_number_format(&s, *f); });
        let _ = check_date_format(&s, &DATE_FORMATS);
        DATE_FORMATS.iter().for_each(|f| { let _ = parse_date_format(&s, *f); });
    }
}

#[test]
fn seed_is_valid() {
    let mut file = read_file(SEED, &ReadOptions::default()).unwrap().file;
    assert_eq!(file.xtns.len(), 2);
    parse_fields(&mut file).unwrap();
}
