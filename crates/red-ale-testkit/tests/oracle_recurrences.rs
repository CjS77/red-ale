//! Recurrence arithmetic against GnuCash 5.17 (`oracle/recurrences.py`, which calls `recurrenceSet`,
//! `recurrenceNextInstance`, `recurrenceNthInstance`, `recurrenceGetPeriodTime` and `recurrenceListNextInstance` through
//! ctypes, with no Guile): for every period type, multiplier and weekend adjustment, from starts on the 28th to the
//! 31st and on weekends, red-ale gives GnuCash's dates after reference dates across month ends and leap days, both for
//! recurrences `recurrenceSet` normalised and for fields stored as a file holds them.

#![cfg(feature = "oracle")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use red_ale_core::recurrence::next_instance_of_list;
use red_ale_core::{Date, PeriodType, Recurrence, TimeZone, WeekendAdjust};
use red_ale_testkit::oracle;
use serde_json::{Value, json};

fn gnucash() -> Value {
    oracle::check_version().unwrap();
    let out = oracle::run_script(&oracle::scripts_dir().join("recurrences.py"), &[]).unwrap_or_else(|e| panic!("oracle: {e}"));
    serde_json::from_str(&out).unwrap()
}

fn date(v: &Value) -> Date {
    Date::parse(v.as_str().unwrap()).unwrap()
}

fn shown(d: Option<Date>) -> Value {
    d.map_or(Value::Null, |d| json!(d.to_file_string()))
}

/// `[period type, mult, start, weekend adjustment]`, made as `recurrenceSet` makes it or as stored.
fn recurrence(made: &Value, normalised: bool) -> Recurrence {
    let pt: PeriodType = made[0].as_str().unwrap().parse().unwrap();
    let mult = u16::try_from(made[1].as_u64().unwrap()).unwrap();
    let wadj: WeekendAdjust = made[3].as_str().unwrap().parse().unwrap();
    if normalised {
        Recurrence::normalised(mult, pt, date(&made[2]), wadj)
    } else {
        Recurrence::new(mult, pt, date(&made[2]), wadj).unwrap()
    }
}

fn fields(r: Recurrence) -> Value {
    json!([
        r.period_type().as_str(),
        r.mult(),
        r.start().to_file_string(),
        r.weekend_adjust().as_str()
    ])
}

fn first_instances(r: Recurrence, count: usize) -> Value {
    let from = r.start().add_days(-1);
    let dates = std::iter::successors(Some((from, r.next_instance(from).unwrap())), |(_, next)| {
        next.map(|d| (d, r.next_instance(d).unwrap()))
    });
    let mut out: Vec<Value> = dates.map(|(_, next)| shown(next)).take(count).collect();
    if let Some(stop) = out.iter().position(Value::is_null) {
        out.truncate(stop + 1);
    }
    Value::Array(out)
}

/// red-ale's answers to the questions the script asked GnuCash about one recurrence.
fn red_ale_case(case: &Value) -> Value {
    let r = recurrence(&case["made"], case["mode"] == "set");
    let next: serde_json::Map<String, Value> = case["next"]
        .as_object()
        .unwrap()
        .keys()
        .map(|reference| (reference.clone(), shown(r.next_instance(Date::parse(reference).unwrap()).unwrap())))
        .collect();
    let mut out = json!({
        "mode": case["mode"],
        "made": case["made"],
        "stored": fields(r),
        "next": next,
        "first": first_instances(r, 14),
        "nth": (0..5).map(|n| shown(r.nth_instance(n).unwrap())).collect::<Vec<_>>(),
    });
    if r.period_type() != PeriodType::Once {
        let utc = TimeZone::utc();
        let bounds = (0..3).map(|n| r.period_bounds(n, &utc).unwrap().unwrap());
        out["periods"] = bounds.map(|b| json!([b.start.seconds(), b.end.seconds()])).collect();
    }
    out
}

#[test]
fn recurrence_arithmetic_matches_gnucash() {
    let expected = gnucash();
    let cases = expected["recurrences"].as_array().unwrap();
    let pairs: usize = cases.iter().map(|c| c["next"].as_object().unwrap().len()).sum();
    assert!(
        cases.len() > 4000 && pairs > 100_000,
        "{} recurrences, {pairs} (recurrence, reference) pairs",
        cases.len()
    );
    let wrong: Vec<(Value, Value)> = cases
        .iter()
        .map(|c| (c.clone(), red_ale_case(c)))
        .filter(|(gnucash, red_ale)| gnucash != red_ale)
        .collect();
    assert!(
        wrong.is_empty(),
        "{} of {} differ; the first:\nGnuCash {}\nred-ale {}",
        wrong.len(),
        cases.len(),
        wrong[0].0,
        wrong[0].1
    );

    expected["lists"].as_array().unwrap().iter().for_each(|list| {
        let recs: Vec<Recurrence> = list["made"].as_array().unwrap().iter().map(|m| recurrence(m, true)).collect();
        list["next"].as_object().unwrap().iter().for_each(|(reference, gnucash)| {
            let red_ale = next_instance_of_list(&recs, Date::parse(reference).unwrap()).unwrap();
            assert_eq!(&shown(red_ale), gnucash, "list {} after {reference}", list["made"]);
        });
    });
}
