//! `book.sx_cashflow(from, to)` (`design/reports.md` §5, "Book") on the rent of `fixtures/synthetic/rich.gnucash`:
//! monthly from 2024-08-01 adjusted back off weekends, last occurrence 2024-09-01, a Deferred instance saved at
//! 2024-09-01, ending 2025-07-01, 456 from Checking to Food.
//!
//! The counts are GnuCash 5.17's own, from `gnc_sx_all_instantiate_cashflow_all` on this book under `TZ=UTC`
//! (`design/notes/engine-budgets.md` §7.6, exp6): 2024-09-01 to 2024-12-31 gives 3, 2024-08-01 to 2024-09-30 none,
//! 2024-11-29 alone 1 (December's occurrence moves back to Friday 29 November), 2024-12-01 to 2024-12-31 none and
//! 2025-01-01 to 2026-12-31 gives 7. The other cases are worked from
//! `5.17:libgnucash/app-utils/gnc-sx-instance-model.c:1087-1148` (a zero numeric falls back to the formula),
//! `:1902-1907` (disabled ones add nothing) and `5.17:bindings/guile/expressions.i:27-32` (each time's local date).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::{GUID, Host, registry, template_with};
use red_ale_core::{Book, TimeZone};
use serde_json::json;

const FOOD: &str = "1feca2abf92347288c268ebf93753ec4";
const CHECKING: &str = "c080059e1aec4c0b9ad4671e80965686";

/// 2024-09-01, 2024-08-01, 2024-09-30, 2024-11-29, 2024-12-01, 2024-12-31, 2025-01-01 and 2026-12-31 at 00:00 UTC.
const SEP_1: i64 = 1_725_148_800;
const AUG_1: i64 = 1_722_470_400;
const SEP_30: i64 = 1_727_654_400;
const NOV_29: i64 = 1_732_838_400;
const DEC_1: i64 = 1_733_011_200;
const DEC_31: i64 = 1_735_603_200;
const JAN_1_2025: i64 = 1_735_689_600;
const DEC_31_2026: i64 = 1_798_675_200;

/// The rich fixture's text, with `edit` applied.
fn rich(edit: impl Fn(String) -> String) -> Book {
    let path = format!("{}/../../fixtures/synthetic/rich.gnucash", env!("CARGO_MANIFEST_DIR"));
    let text = edit(std::fs::read_to_string(path).unwrap());
    red_ale_xml::read_book(text.as_bytes()).unwrap().0
}

/// `book.sx_cashflow(from, to)` for each range, as `account=amount` pairs in GUID order, one range per line.
fn flows(book: Book, zone: TimeZone, ranges: &[(i64, i64)]) -> Vec<String> {
    let calls: Vec<String> = ranges
        .iter()
        .map(|(from, to)| format!("show(book.sx_cashflow({from}, {to}))"))
        .collect();
    let body = format!(
        "let lines = [{}];\n#{{ title: lines.reduce(|a, b| a + \"|\" + b, \"\"), sections: [] }}",
        calls.join(", ")
    );
    let show = "fn show(m) { let out = \"\"; for k in m.keys() { if out != \"\" { out += \" \"; } out += `${k}=${m[k]}`; } out }";
    let script = template_with(show, &body);
    let reg = registry(&[("test.rhai", &script)]);
    assert!(reg.problems().is_empty(), "{:?}", reg.problems());
    let mut host = Host::new(book);
    host.zone = zone;
    let title = host.run(&reg, GUID, &json!({})).unwrap().title;
    title.split('|').skip(1).map(str::to_owned).collect()
}

/// Food's and Checking's sums over `n` occurrences, Food's at `food` each: keys in GUID order, as Rhai's maps keep them.
fn both(n: i64, food: i64) -> String {
    format!("{FOOD}={} {CHECKING}={}", food * n, -456 * n)
}

/// exp6's ranges: the last occurrence and the Deferred instance are never counted, both ends count, and the
/// weekend adjustment moves December's occurrence into November.
#[test]
fn counts_match_gnucash() {
    let got = flows(
        rich(|t| t),
        TimeZone::utc(),
        &[
            (SEP_1, DEC_31),
            (AUG_1, SEP_30),
            (NOV_29, NOV_29),
            (DEC_1, DEC_31),
            (JAN_1_2025, DEC_31_2026),
        ],
    );
    assert_eq!(got, [both(3, 456), String::new(), both(1, 456), String::new(), both(7, 456)]);
}

/// A zero numeric falls back to the formula, read with every variable 0 (`:1087-1148`): `rent * 2 + 10` is 10 a
/// month, while the Checking side keeps its numeric 456.
#[test]
fn a_formula_variable_counts_as_zero() {
    let formula = |t: String| {
        t.replacen(
            "<slot:value type=\"string\">456</slot:value>",
            "<slot:value type=\"string\">rent * 2 + 10</slot:value>",
            1,
        )
        .replacen(
            "<slot:value type=\"numeric\">456/1</slot:value>",
            "<slot:value type=\"numeric\">0/1</slot:value>",
            1,
        )
    };
    assert_eq!(flows(rich(formula), TimeZone::utc(), &[(SEP_1, DEC_31)]), [both(3, 10)]);
}

/// A disabled scheduled transaction adds nothing (`:1902-1907`).
#[test]
fn a_disabled_one_adds_nothing() {
    let disabled = |t: String| t.replacen("<sx:enabled>y</sx:enabled>", "<sx:enabled>n</sx:enabled>", 1);
    assert_eq!(flows(rich(disabled), TimeZone::utc(), &[(SEP_1, DEC_31)]), [String::new()]);
}

/// The range is the local dates of the two times: 28 November 14:00 UTC is already 29 November at UTC+11, so the
/// day holds December's occurrence there and not in UTC.
#[test]
fn times_are_taken_as_local_dates() {
    let nov_28_14h = NOV_29 - 10 * 3600;
    let ranges = [(nov_28_14h, nov_28_14h)];
    assert_eq!(flows(rich(|t| t), TimeZone::utc(), &ranges), [String::new()]);
    assert_eq!(flows(rich(|t| t), TimeZone::fixed(11 * 3600).unwrap(), &ranges), [both(1, 456)]);
}
