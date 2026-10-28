//! `lib/dates` (`5.17:bindings/guile/date-utilities.scm:176-499`, DU): calendar arithmetic, the date fractions, the
//! deltas, stepping by days and months, and date lists. Times are made and printed in UTC, the zone of GnuCash's own
//! report tests, except where a zone is the point.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;
mod libs;

use common::{Host, empty_book};
use libs::{check, lines};
use red_ale_core::TimeZone;
use red_ale_report::settings::WeekStart;

const PRELUDE: &str = r#"
import "lib/dates" as dates;
let t = |y, m, d| ctx.make_time(#{ year: y, month: m, day: d, hour: 10, minute: 59 });
let iso = |x| ctx.print_time_iso(x);
let isos = |xs| xs.map(|x| ctx.print_time_iso(x));
"#;

fn run(host: &Host, body: &str) -> Vec<String> {
    lines(host, &format!("{PRELUDE}\n{body}"))
}

#[test]
fn leap_years_and_month_lengths() {
    let got = run(
        &Host::new(empty_book()),
        r"
        print(`leap=${dates::leap_year(1900)} ${dates::leap_year(2000)} ${dates::leap_year(2023)} ${dates::leap_year(2024)}`);
        print(`days-in-year=${dates::days_in_year(2023)} ${dates::days_in_year(2024)}`);
        print(`days-in-month=${dates::days_in_month(1, 2023)} ${dates::days_in_month(4, 2023)} ${dates::days_in_month(2, 2023)} ` +
              `${dates::days_in_month(2, 2024)} ${dates::days_in_month(2, 1900)} [${dates::days_in_month(13, 2024)}]`);
    ",
    );
    check(
        &got,
        &[
            "leap=false true false true",
            "days-in-year=365 366",
            "days-in-month=31 30 28 29 28 []",
        ],
    );
}

/// At 2024-03-15 12:00 UTC (`ctx.now`): yearday 75 of a leap year, 1710504000 s = 39595/2 days. DU:194-259.
#[test]
fn fractions_are_exact() {
    let got = run(
        &Host::new(empty_book()),
        r#"
        let n = ctx.now;
        print(`year=${dates::date_to_year_fraction(ctx, n)} half=${dates::date_to_halfyear_fraction(ctx, n)}`);
        print(`month=${dates::date_to_month_fraction(ctx, n)} quarter=${dates::date_to_quarter_fraction(ctx, n)}`);
        print(`week=${dates::date_to_week_fraction(ctx, n)} twoweek=${dates::date_to_twoweek_fraction(ctx, n)}`);
        print(`day=${dates::date_to_day_fraction(n)}`);
        let f = dates::date_get_fraction_func(ctx, "MonthDelta");
        print(`func=${dates::apply_fraction_func(f, n)} unknown=[${dates::date_get_fraction_func(ctx, "SecDelta")}] ` +
              `closure=${dates::apply_fraction_func(|x| x + 1, 1)}`);
        let all = ["YearDelta", "HalfYearDelta", "QuarterDelta", "MonthDelta", "TwoWeekDelta", "WeekDelta", "DayDelta"];
        print(`all=${all.map(|i| dates::apply_fraction_func(dates::date_get_fraction_func(ctx, i), n))}`);
    "#,
    );
    check(
        &got,
        &[
            // 54 + 74/366; twice that.
            "year=9919/183 half=19838/183",
            // 12 × 54 + 3 − 1 + 14/31; a third of that.
            "month=20164/31 quarter=20164/93",
            // (39595/2 − 1 − 1) / 7 with the week starting on Sunday (1); half of that.
            "week=39591/14 twoweek=39591/28",
            // 39595/2 − 59, where GnuCash computes a double.
            "day=39477/2",
            "func=20164/31 unknown=[] closure=2",
            "all=[rational(\"9919/183\"), rational(\"19838/183\"), rational(\"20164/93\"), rational(\"20164/31\"), \
         rational(\"39591/28\"), rational(\"39591/14\"), rational(\"39477/2\")]",
        ],
    );
}

#[test]
fn week_fractions_follow_the_first_day_of_the_week() {
    let mut host = Host::new(empty_book());
    host.settings.week_start = Some(WeekStart::Monday);
    let got = run(&host, "print(`${dates::date_to_week_fraction(ctx, ctx.now)}`);");
    check(&got, &["39589/14"]);
}

#[test]
fn deltas_and_stepping() {
    let got = run(
        &Host::new(empty_book()),
        r#"
        print(`month-delta=${dates::MonthDelta} year-delta=${dates::YearDelta}`);
        print(`deltasym=${dates::deltasym_to_delta("TwoWeekDelta")} [${dates::deltasym_to_delta("FortnightDelta")}]`);
        print(`incdate=${iso.call(dates::incdate(ctx, t.call(2024, 1, 31), dates::MonthDelta))} ` +
              `${iso.call(dates::incdate(ctx, t.call(2024, 12, 31), dates::DayDelta))} ` +
              `${iso.call(dates::incdate(ctx, t.call(2024, 2, 29), dates::YearDelta))}`);
        print(`decdate=${iso.call(dates::decdate(ctx, t.call(2024, 3, 31), dates::MonthDelta))} ` +
              `${iso.call(dates::decdate(ctx, t.call(2024, 3, 1), dates::SecDelta))}`);
        print(`months=${isos.call([dates::incdate_months(ctx, t.call(2024, 1, 31), 1), dates::incdate_months(ctx, t.call(2024, 1, 31), 2),
                                    dates::incdate_months(ctx, t.call(2024, 1, 30), 1), dates::incdate_months(ctx, t.call(2024, 2, 29), 12),
                                    dates::incdate_months(ctx, t.call(2024, 4, 30), 1), dates::incdate_months(ctx, t.call(2024, 1, 15), -1),
                                    dates::incdate_months(ctx, t.call(2024, 3, 31), -13)])}`);
        let day = ctx.make_time(#{ year: 2024, month: 3, day: 15, hour: 13, minute: 14, second: 15 });
        print(`day=${iso.call(dates::time64_start_day_time(ctx, day))} ${iso.call(dates::time64_end_day_time(ctx, day))}`);
    "#,
    );
    check(
        &got,
        &[
            concat!(
                r#"month-delta=#{"day": 0, "hour": 0, "minute": 0, "month": 1, "second": 0, "year": 0} "#,
                r#"year-delta=#{"day": 0, "hour": 0, "minute": 0, "month": 0, "second": 0, "year": 1}"#
            ),
            r#"deltasym=#{"day": 14, "hour": 0, "minute": 0, "month": 0, "second": 0, "year": 0} []"#,
            // moddate normalises as mktime does: 31 February 2024 is 2 March (DU:276-287).
            "incdate=2024-03-02 10:59:00 2025-01-01 10:59:00 2025-03-01 10:59:00",
            "decdate=2024-03-02 10:59:00 2024-03-01 10:58:59",
            // incdate-months keeps the day, steps back a day that slips past the month's end, and keeps month ends
            // (DU:299-318).
            "months=[\"2024-02-29 10:59:00\", \"2024-03-31 10:59:00\", \"2024-02-29 10:59:00\", \"2025-02-28 10:59:00\", \
         \"2024-05-31 10:59:00\", \"2023-12-15 10:59:00\", \"2023-02-28 10:59:00\"]",
            "day=2024-03-15 00:00:00 2024-03-15 23:59:59",
        ],
    );
}

/// gnc:make-date-list (DU:356-381): steps from the start, then the end itself; months count from the start.
#[test]
fn date_lists() {
    let got = run(
        &Host::new(empty_book()),
        r"
        print(`months=${isos.call(dates::make_date_list(ctx, t.call(2024, 1, 31), t.call(2024, 5, 15), dates::MonthDelta))}`);
        print(`weeks=${isos.call(dates::make_date_list(ctx, t.call(2024, 3, 1), t.call(2024, 3, 20), dates::WeekDelta))}`);
        print(`exact=${isos.call(dates::make_date_list(ctx, t.call(2024, 3, 1), t.call(2024, 3, 15), dates::WeekDelta))}`);
        print(`swapped=${isos.call(dates::make_date_list(ctx, t.call(2024, 3, 3), t.call(2024, 3, 1), dates::DayDelta))}`);
        print(`quarters=${isos.call(dates::make_date_list(ctx, t.call(2023, 11, 30), t.call(2024, 6, 1), dates::QuarterDelta))}`);
        print(`same=${isos.call(dates::make_date_list(ctx, t.call(2024, 3, 1), t.call(2024, 3, 1), dates::DayDelta))}`);
        print(`thirty=${isos.call(dates::make_date_list(ctx, t.call(2024, 1, 31), t.call(2024, 3, 2), dates::ThirtyDayDelta))}`);
    ",
    );
    check(
        &got,
        &[
            "months=[\"2024-01-31 10:59:00\", \"2024-02-29 10:59:00\", \"2024-03-31 10:59:00\", \"2024-04-30 10:59:00\", \
         \"2024-05-15 10:59:00\"]",
            "weeks=[\"2024-03-01 10:59:00\", \"2024-03-08 10:59:00\", \"2024-03-15 10:59:00\", \"2024-03-20 10:59:00\"]",
            "exact=[\"2024-03-01 10:59:00\", \"2024-03-08 10:59:00\", \"2024-03-15 10:59:00\"]",
            "swapped=[\"2024-03-01 10:59:00\", \"2024-03-02 10:59:00\", \"2024-03-03 10:59:00\"]",
            // November's 30th is a month end, so every quarter lands on one.
            "quarters=[\"2023-11-30 10:59:00\", \"2024-02-29 10:59:00\", \"2024-05-31 10:59:00\", \"2024-06-01 10:59:00\"]",
            "same=[\"2024-03-01 10:59:00\"]",
            // Thirty days is not a month delta: plain day steps.
            "thirty=[\"2024-01-31 10:59:00\", \"2024-03-01 10:59:00\", \"2024-03-02 10:59:00\"]",
        ],
    );
}

/// gnc:make-date-interval-list (DU:323-349): each interval ends a second before the next starts and the last at the
/// end date; months count from the start; an empty range has no intervals, where make-date-list gives one date.
#[test]
fn date_interval_lists() {
    let got = run(
        &Host::new(empty_book()),
        r"
        let ivs = |xs| xs.map(|p| `${ctx.print_time_iso(p[0])}..${ctx.print_time_iso(p[1])}`);
        print(`months=${ivs.call(dates::make_date_interval_list(ctx, t.call(2024, 1, 31), t.call(2024, 5, 15), dates::MonthDelta))}`);
        print(`weeks=${ivs.call(dates::make_date_interval_list(ctx, t.call(2024, 3, 1), t.call(2024, 3, 20), dates::WeekDelta))}`);
        print(`exact=${ivs.call(dates::make_date_interval_list(ctx, t.call(2024, 3, 1), t.call(2024, 3, 15), dates::WeekDelta))}`);
        print(`swapped=${ivs.call(dates::make_date_interval_list(ctx, t.call(2024, 3, 3), t.call(2024, 3, 1), dates::DayDelta))}`);
        print(`same=${ivs.call(dates::make_date_interval_list(ctx, t.call(2024, 3, 1), t.call(2024, 3, 1), dates::DayDelta))}`);
        let from = dates::time64_start_day_time(ctx, t.call(2024, 3, 1));
        let to = dates::time64_end_day_time(ctx, t.call(2024, 3, 3));
        print(`days=${ivs.call(dates::make_date_interval_list(ctx, from, to, dates::DayDelta))}`);
    ",
    );
    check(
        &got,
        &[
            "months=[\"2024-01-31 10:59:00..2024-02-29 10:58:59\", \"2024-02-29 10:59:00..2024-03-31 10:58:59\", \
         \"2024-03-31 10:59:00..2024-04-30 10:58:59\", \"2024-04-30 10:59:00..2024-05-15 10:59:00\"]",
            "weeks=[\"2024-03-01 10:59:00..2024-03-08 10:58:59\", \"2024-03-08 10:59:00..2024-03-15 10:58:59\", \
         \"2024-03-15 10:59:00..2024-03-20 10:59:00\"]",
            "exact=[\"2024-03-01 10:59:00..2024-03-08 10:58:59\", \"2024-03-08 10:59:00..2024-03-15 10:59:00\"]",
            "swapped=[\"2024-03-01 10:59:00..2024-03-02 10:58:59\", \"2024-03-02 10:59:00..2024-03-03 10:59:00\"]",
            "same=[]",
            // The Balance Forecast's daily intervals, from the start of the first day to the end of the last
            // (`standard/balance-forecast.scm:143-146`).
            "days=[\"2024-03-01 00:00:00..2024-03-01 23:59:59\", \"2024-03-02 00:00:00..2024-03-02 23:59:59\", \
         \"2024-03-03 00:00:00..2024-03-03 23:59:59\"]",
        ],
    );
}

/// The day boundaries and fractions are local: in UTC+14, 2024-03-15 12:00 UTC is 02:00 on 16 March.
#[test]
fn local_days_follow_the_zone() {
    let mut host = Host::new(empty_book());
    host.zone = TimeZone::fixed(14 * 3600).unwrap();
    let got = run(
        &host,
        r"
        print(`${iso.call(dates::time64_start_day_time(ctx, ctx.now))} ${dates::time64_start_day_time(ctx, ctx.now)}`);
        print(`${dates::date_to_month_fraction(ctx, ctx.now)}`);
    ",
    );
    check(&got, &["2024-03-16 00:00:00 1710496800", "20165/31"]);
}

/// gnc:time64-previous-day and gnc:time64-next-day (DU:495-499) step one local day with `decdate` and `incdate`,
/// across month, leap-day and year ends; the Equity Statement's start bound is the previous day's 23:59:59
/// (`5.17:gnucash/report/reports/standard/equity-statement.scm:205-206`).
#[test]
fn previous_and_next_day() {
    let got = run(
        &Host::new(empty_book()),
        r"
        print(`previous=${isos.call([dates::time64_previous_day(ctx, t.call(2024, 3, 1)),
                                      dates::time64_previous_day(ctx, t.call(2024, 1, 1)),
                                      dates::time64_previous_day(ctx, t.call(2023, 3, 1))])}`);
        print(`next=${isos.call([dates::time64_next_day(ctx, t.call(2024, 2, 28)), dates::time64_next_day(ctx, t.call(2024, 2, 29)),
                                  dates::time64_next_day(ctx, t.call(2024, 12, 31))])}`);
        print(`start-bound=${iso.call(dates::time64_end_day_time(ctx, dates::time64_previous_day(ctx, t.call(1970, 1, 1))))}`);
    ",
    );
    check(
        &got,
        &[
            r#"previous=["2024-02-29 10:59:00", "2023-12-31 10:59:00", "2023-02-28 10:59:00"]"#,
            r#"next=["2024-02-29 10:59:00", "2024-03-01 10:59:00", "2025-01-01 10:59:00"]"#,
            "start-bound=1969-12-31 23:59:59",
        ],
    );
}

/// A day is a local day, not 86,400 seconds: across New York's change to summer time on 10 March 2024 the step is
/// 23 hours, and across its end on 3 November 25, as `gnc-mktime` with `isdst` -1 gives (DU:276-287).
#[test]
fn a_day_step_keeps_the_local_time_across_daylight_saving() {
    let mut host = Host::new(empty_book());
    host.zone = TimeZone::from_gnucash_posix("EST-5EDT,M3.2.0,M11.1.0").unwrap();
    let got = run(
        &host,
        r"
        let spring = t.call(2024, 3, 10);
        let autumn = t.call(2024, 11, 2);
        print(`${iso.call(dates::time64_previous_day(ctx, spring))} ${spring - dates::time64_previous_day(ctx, spring)}`);
        print(`${iso.call(dates::time64_next_day(ctx, autumn))} ${dates::time64_next_day(ctx, autumn) - autumn}`);
    ",
    );
    check(&got, &["2024-03-09 10:59:00 82800", "2024-11-03 10:59:00 90000"]);
}

/// gnc:date-to-week (DU:253-254) is the floor of the week fraction, before and after the epoch and on a week's
/// boundary; gnc:date-get-week (DU:123-125) counts from the start of the local day. At 2024-03-15 that is week 2827,
/// whose fourth day is Sunday 10 March and tenth Saturday 16 March: gnc:date-get-week-year-string (DU:169-173).
#[test]
fn weeks_and_their_labels() {
    let got = run(
        &Host::new(empty_book()),
        r#"
        let times = [ctx.now, 0, 2 * 86400, 2 * 86400 - 1, 2 * 86400 - 604800, 2 * 86400 - 604800 - 1, -10000000];
        print(`${times.map(|x| dates::date_to_week(ctx, x))}`);
        print(`${times.map(|x| dates::date_to_week_fraction(ctx, x).round_to(1, "floor").to_string())}`);
        print(`${dates::date_get_week(ctx, ctx.now)} ${dates::date_get_week_year_string(ctx, ctx.now)}`);
        print(`${dates::date_get_week_year_string(ctx, t.call(2024, 3, 17))}`);
    "#,
    );
    check(
        &got,
        &[
            "[2827, -1, 0, -1, -1, -2, -17]",
            r#"["2827", "-1", "0", "-1", "-1", "-2", "-17"]"#,
            "2827 03/10/24 to 03/16/24",
            "03/17/24 to 03/23/24",
        ],
    );
}

/// gnc:date-year-delta (DU:201-217), exact: 15 January 2023 (day 15) to 15 March 2024 (day 75 of a leap year) is
/// 1 + 60/366; a non-leap year's days from 28 February (day 59) on count one more, so 1 March to 1 March is a year.
#[test]
fn year_deltas_count_days_as_366ths() {
    let got = run(
        &Host::new(empty_book()),
        r"
        let d = |a, b| dates::date_year_delta(ctx, a, b);
        print(`${d.call(t.call(2023, 1, 15), t.call(2024, 3, 15))} ${d.call(t.call(2024, 3, 15), t.call(2023, 1, 15))}`);
        print(`${d.call(t.call(2023, 3, 1), t.call(2024, 3, 1))} ${d.call(t.call(2023, 2, 27), t.call(2023, 2, 28))}`);
        print(`${d.call(t.call(2024, 2, 28), t.call(2024, 2, 29))} ${d.call(t.call(2024, 5, 5), t.call(2024, 5, 5))}`);
    ",
    );
    check(&got, &["71/61 -71/61", "1 1/183", "1/366 0"]);
}

/// time64CanonicalDayTime (`5.17:libgnucash/engine/gnc-date.cpp:404-410`) is local noon: in UTC+14, 2024-03-15
/// 12:00 UTC is 02:00 on 16 March, whose noon is 22:00 UTC on the 15th.
#[test]
fn the_canonical_day_time_is_local_noon() {
    let mut host = Host::new(empty_book());
    host.zone = TimeZone::fixed(14 * 3600).unwrap();
    let got = run(
        &host,
        "print(`${iso.call(dates::time64_canonical_day_time(ctx, ctx.now))} ${dates::time64_canonical_day_time(ctx, ctx.now)}`);",
    );
    check(&got, &["2024-03-16 12:00:00 1710540000"]);
}

/// `ctx.print_month_year` and `ctx.print_year` print `%B %Y` and `%Y` (DU:151-152, :166-167) with the Locale's month
/// names.
#[test]
fn month_and_year_labels_use_the_locale() {
    let script = "print(`${ctx.print_month_year(ctx.now)}|${ctx.print_year(ctx.now)}|${ctx.print_month_year(t.call(1999, 12, 31))}`);";
    let mut host = Host::new(empty_book());
    let c = run(&host, script);
    host.locale = red_ale_locale::Locale::by_name("fr_FR").unwrap();
    let fr = run(&host, script);
    check(&c, &["March 2024|2024|December 1999"]);
    check(&fr, &["mars 2024|2024|décembre 1999"]);
}
