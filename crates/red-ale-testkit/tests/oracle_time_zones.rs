//! Day boundaries, "today", relative dates, posted dates and same-day prices match GnuCash 5.17 run under the same
//! `TZ`: both extremes (UTC−11 and UTC+14), a zone with daylight saving, a POSIX string, UTC, and `TZ` unset.

#![cfg(feature = "oracle")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use red_ale_core::time::{PeriodBounds, RelativeDate, relative_date};
use red_ale_core::{Date, Time64, TimeZone};
use red_ale_testkit::{TempBook, call, dispatcher_in, oracle, oracle_check};
use serde_json::{Value, json};

/// The zones tested: `None` leaves `TZ` unset, so both programs read `/etc/localtime`.
const ZONES: [Option<&str>; 6] = [
    Some("UTC"),
    Some("Pacific/Pago_Pago"),
    Some("Pacific/Kiritimati"),
    Some("America/New_York"),
    Some("XYZ-11"),
    None,
];

/// The zone red-ale's service reads for this `TZ`.
fn zone(tz: Option<&str>) -> TimeZone {
    let tzdir = std::env::var_os("TZDIR").map(std::path::PathBuf::from);
    red_ale_service::zone::zone_from(tz, tzdir.as_deref(), &|p| std::fs::read(p).ok())
}

fn label(tz: Option<&str>) -> &str {
    tz.unwrap_or("(unset)")
}

fn day(s: &str) -> Date {
    Date::parse(s).unwrap()
}

/// Days around both extremes' day boundaries, New York's DST changes, a leap day, year ends, and a year after the
/// last transition in the zone files.
const DAYS: [&str; 8] = [
    "2024-01-01",
    "2024-02-29",
    "2024-03-10",
    "2024-06-20",
    "2024-11-03",
    "2024-12-31",
    "2040-07-01",
    "2000-03-26",
];

fn instants() -> Vec<i64> {
    DAYS.iter()
        .flat_map(|d| [0, 5, 10, 11, 12, 13, 23].map(|h| day(d).start_utc().0 + h * 3600 + 1234))
        .collect()
}

/// Where red-ale and GnuCash are expected to agree: GnuCash models a zone file as one rule per year and drops
/// daylight saving from 2037, when the transitions in the file end (`5.17:libgnucash/engine/gnc-timezone.cpp:712-719`).
/// [`gnucash_zone_model_deviations`] pins the difference.
fn agreed(z: &TimeZone, d: Date) -> bool {
    let offset = |m| z.offset_at(Date::from_ymd(d.year(), m, 1).unwrap().neutral_time());
    d.year() < 2037 || offset(1) == offset(7)
}

fn nows() -> Vec<i64> {
    [
        "2024-05-20 11:10:00",
        "2024-03-31 12:00:00",
        "2024-12-31 23:30:00",
        "2024-02-29 10:00:00",
        "2024-11-03 05:30:00",
        "2024-01-01 09:00:00",
    ]
    .iter()
    .map(|s| Time64::parse_file_string(s).unwrap().0)
    .collect()
}

fn gnucash(tz: Option<&str>) -> Value {
    oracle::check_version().unwrap();
    let dates: Vec<[u32; 3]> = DAYS
        .iter()
        .map(|d| day(d))
        .map(|d| [u32::try_from(d.year()).unwrap(), d.month(), d.day()])
        .collect();
    let spec = json!({ "instants": instants(), "dates": dates, "nows": nows() }).to_string();
    let out = oracle::run_script_in(&oracle::scripts_dir().join("time_zones.py"), &[&spec], tz).unwrap_or_else(|e| panic!("oracle: {e}"));
    serde_json::from_str(&out).unwrap()
}

fn ints(v: &Value) -> Vec<i64> {
    v.as_array().unwrap().iter().map(|x| x.as_i64().unwrap()).collect()
}

#[test]
fn day_boundaries_and_neutral_times_match_gnucash() {
    ZONES.iter().for_each(|tz| {
        let (g, z) = (gnucash(*tz), zone(*tz));
        let local = |t: i64| z.local_date(Time64(t)).unwrap();
        let compare = |what: &str, f: &dyn Fn(Date) -> Time64| {
            instants()
                .iter()
                .zip(ints(&g[what]))
                .filter(|(t, _)| agreed(&z, local(**t)))
                .for_each(|(t, theirs)| {
                    assert_eq!(f(local(*t)).0, theirs, "{what} of {} under TZ={}", Time64(*t), label(*tz));
                });
        };
        compare("dayStart", &|d| z.day_start(d));
        compare("dayEnd", &|d| z.day_end(d));
        compare("noon", &|d| z.noon(d));
        DAYS.iter()
            .zip(ints(&g["neutral"]))
            .filter(|(d, _)| agreed(&z, day(d)))
            .for_each(|(d, theirs)| {
                assert_eq!(z.neutral_time(day(d)).0, theirs, "neutral time of {d} under TZ={}", label(*tz));
            });
    });
}

#[test]
fn today_and_relative_dates_match_gnucash() {
    ZONES.iter().for_each(|tz| {
        let (g, z) = (gnucash(*tz), zone(*tz));
        let today = ints(&g["today"]);
        let local_today = z.local_date(Time64(today[0])).unwrap();
        let (start, end) = (z.day_start(local_today), z.day_end(local_today));
        let theirs = [Time64(today[1]), Time64(today[2])].map(|t| z.local_date(t));
        assert_eq!(
            theirs,
            [Some(local_today); 2],
            "today's bounds fall on today under TZ={}",
            label(*tz)
        );
        // On a daylight-saving change GnuCash's model of a zone with rules in UT is an hour out at midnight.
        if z.offset_at(start) == z.offset_at(end) {
            assert_eq!([start.0, end.0], [today[1], today[2]], "today under TZ={}", label(*tz));
        }
        let fiscal = ints(&g["fiscal"]);
        let bounds = PeriodBounds {
            start: Time64(fiscal[0]),
            end: Time64(fiscal[1]),
        };
        nows().iter().zip(g["relative"].as_array().unwrap()).for_each(|(now, theirs)| {
            let ours: Vec<i64> = RelativeDate::all()
                .map(|p| relative_date(p, Time64(*now), &z, bounds).unwrap().0)
                .collect();
            let theirs = ints(theirs);
            RelativeDate::all()
                .zip(ours.iter().zip(&theirs))
                .filter(|(_, (a, b))| a != b)
                .for_each(|(p, (a, b))| {
                    panic!(
                        "{p} at {} under TZ={}: red-ale {} GnuCash {}",
                        Time64(*now),
                        label(*tz),
                        Time64(*a),
                        Time64(*b)
                    )
                });
        });
    });
}

/// Adds each `COMMODITY,CURRENCY,EPOCH,SOURCE,NUM,DENOM` with `gnc_pricedb_add_price` and prints what each returned
/// and the prices left. Times go through the engine as `time64`: the bindings convert Python datetimes through local
/// time, which would shift them by the zone's offset.
const ADD: &str = r#"
import ctypes, json, sys
sys.path.insert(0, sys.argv[2])
import gnucash
from gnc_engine import engine, ptr
from gnucash import GncNumeric, GncPrice, Session, SessionOpenMode
engine.gnc_price_set_time64.argtypes = [ctypes.c_void_p, ctypes.c_int64]
engine.gnc_price_get_time64.restype = ctypes.c_int64
engine.gnc_price_get_time64.argtypes = [ctypes.c_void_p]
s = Session("xml://" + sys.argv[1], SessionOpenMode.SESSION_READ_ONLY)
try:
    b = s.book
    t, pdb = b.get_table(), b.get_price_db()
    look = lambda uid: t.lookup(*uid.split("::"))
    results = []
    for spec in sys.argv[3:]:
        c, cur, epoch, src, num, den = spec.split(",")
        p = GncPrice(b)
        p.begin_edit()
        p.set_commodity(look(c))
        p.set_currency(look(cur))
        engine.gnc_price_set_time64(ptr(p), int(epoch))
        p.set_source_string(src)
        p.set_value(GncNumeric(int(num), int(den)))
        p.commit_edit()
        results.append(bool(pdb.add_price(p)))
    left = []
    for cm in [look("CURRENCY::EUR"), look("CURRENCY::USD")]:
        for p in pdb.get_prices(cm, None):
            v = p.get_value()
            left.append([p.get_commodity().get_unique_name(), engine.gnc_price_get_time64(ptr(p)), f"{v.num()}/{v.denom()}"])
    print(json.dumps({"results": results, "left": sorted(left)}))
finally:
    s.end()
"#;

/// Prices straddling the local midnights of UTC−11, UTC and UTC+14 on 20 June 2024: whether they are "the same day"
/// depends on the zone.
fn price_adds() -> Vec<(&'static str, &'static str, i64, &'static str, i64)> {
    let base = day("2024-06-20").start_utc().0;
    let at = |minutes: i64| base + minutes * 60;
    vec![
        ("EUR", "USD", at(10 * 60 + 30), "user:price", 110),
        ("EUR", "USD", at(12 * 60), "user:price", 111),
        ("EUR", "USD", at(-30), "user:price", 112),
        ("EUR", "USD", at(23 * 60), "user:price", 113),
        ("USD", "EUR", at(9 * 60 + 30), "Finance::Quote", 90),
        ("EUR", "USD", at(13 * 60), "user:price", 113),
        ("EUR", "USD", at(24 * 60 + 600), "user:price-editor", 114),
    ]
}

#[test]
fn same_day_prices_follow_gnucash_in_every_zone() {
    ZONES.iter().for_each(|tz| {
        let book = TempBook::new_path();
        let mut d = dispatcher_in(zone(*tz));
        call(&mut d, "book.create", &json!({ "path": book.path_str() }));
        let ours: Vec<bool> = price_adds()
            .iter()
            .map(|(c, cur, t, src, cents)| {
                let r = call(
                    &mut d,
                    "price.add",
                    &json!({ "commodity": c, "currency": cur, "value": format!("{cents}/100"),
                    "time": Time64(*t).to_file_string().replace(' ', "T").replace("T+0000", "Z"), "source": src }),
                );
                r["outcome"] != "rejected"
            })
            .collect();
        let mut left: Vec<Value> = call(&mut d, "price.list", &json!({}))
            .as_array()
            .unwrap()
            .iter()
            .map(|p| {
                let t = Time64::parse_file_string(&p["time"].as_str().unwrap().replace('T', " ").replace('Z', " +0000")).unwrap();
                json!([
                    p["commodity"],
                    t.0,
                    format!("{}/{}", p["value"]["num"].as_str().unwrap(), p["value"]["denom"].as_str().unwrap())
                ])
            })
            .collect();
        left.sort_by_key(ToString::to_string);
        call(&mut d, "book.close", &json!({ "discard": true }));
        let specs: Vec<String> = price_adds()
            .iter()
            .map(|(c, cur, t, src, cents)| format!("CURRENCY::{c},CURRENCY::{cur},{t},{src},{cents},100"))
            .collect();
        let (path, scripts) = (book.path_str(), oracle::scripts_dir().display().to_string());
        let args: Vec<&str> = [path.as_str(), scripts.as_str()]
            .into_iter()
            .chain(specs.iter().map(String::as_str))
            .collect();
        let gnucash: Value = serde_json::from_str(&oracle::run_code_in(ADD, &args, *tz).unwrap()).unwrap();
        assert_eq!(gnucash["results"], json!(ours), "add_price results under TZ={}", label(*tz));
        let mut theirs = gnucash["left"].as_array().unwrap().clone();
        theirs.sort_by_key(ToString::to_string);
        assert_eq!(theirs, left, "prices left under TZ={}", label(*tz));
    });
}

/// Prints, for each transaction, its posted time, the `GDate` GnuCash reads as its posted date, and the neutral time
/// GnuCash computes for that date; and each price's time.
const POSTED: &str = r#"
import ctypes, json, sys
sys.path.insert(0, sys.argv[2])
import gnucash
from gnc_engine import engine, ptr
from gnucash import Session, SessionOpenMode

class GDate(ctypes.Structure):
    _fields_ = [("julian_days", ctypes.c_uint, 32), ("julian", ctypes.c_uint, 1), ("dmy", ctypes.c_uint, 1),
                ("day", ctypes.c_uint, 6), ("month", ctypes.c_uint, 4), ("year", ctypes.c_uint, 16)]

engine.xaccTransGetDatePostedGDate.restype = GDate
engine.xaccTransGetDatePostedGDate.argtypes = [ctypes.c_void_p]
engine.xaccTransGetDate.restype = ctypes.c_int64
engine.xaccTransGetDate.argtypes = [ctypes.c_void_p]
engine.gnc_dmy2time64_neutral.restype = ctypes.c_int64
engine.gnc_dmy2time64_neutral.argtypes = [ctypes.c_int] * 3
glib = ctypes.CDLL("libglib-2.0.so.0")
for f in ("g_date_get_day", "g_date_get_month", "g_date_get_year"):
    getattr(glib, f).argtypes = [ctypes.POINTER(GDate)]
s = Session("xml://" + sys.argv[1], SessionOpenMode.SESSION_READ_ONLY)
try:
    out = []
    for split in s.book.get_root_account().lookup_by_name("Bank").GetSplitList():
        t = split.GetParent()
        g = engine.xaccTransGetDatePostedGDate(ptr(t))
        y, m, d = (getattr(glib, "g_date_get_" + f)(ctypes.byref(g)) for f in ("year", "month", "day"))
        out.append([t.GetDescription(), engine.xaccTransGetDate(ptr(t)), f"{y:04}-{m:02}-{d:02}", engine.gnc_dmy2time64_neutral(d, m, y)])
    engine.gnc_price_get_time64.restype = ctypes.c_int64
    engine.gnc_price_get_time64.argtypes = [ctypes.c_void_p]
    eur = s.book.get_table().lookup("CURRENCY", "EUR")
    prices = [engine.gnc_price_get_time64(ptr(p)) for p in s.book.get_price_db().get_prices(eur, None)]
    print(json.dumps({"transactions": sorted(out), "prices": prices}))
finally:
    s.end()
"#;

#[test]
fn dates_red_ale_stores_read_back_on_the_same_day_in_gnucash() {
    [Some("Pacific/Pago_Pago"), Some("Pacific/Kiritimati"), Some("UTC")]
        .iter()
        .for_each(|tz| {
            let book = TempBook::new_path();
            let mut d = dispatcher_in(zone(*tz));
            call(&mut d, "book.create", &json!({ "path": book.path_str() }));
            call(
                &mut d,
                "account.create",
                &json!({ "name": "Bank", "type": "BANK", "commodity": "USD" }),
            );
            call(
                &mut d,
                "account.create",
                &json!({ "name": "Food", "type": "EXPENSE", "commodity": "USD" }),
            );
            ["2024-06-20", "2024-12-31", "2024-02-29"].iter().for_each(|date| {
                call(
                    &mut d,
                    "transaction.post",
                    &json!({ "description": date, "datePosted": date,
                "splits": [{ "account": "Bank", "value": "-1" }, { "account": "Food", "value": "1" }] }),
                );
            });
            call(
                &mut d,
                "price.add",
                &json!({ "commodity": "EUR", "currency": "USD", "value": "1.1", "date": "2024-06-20" }),
            );
            let diffs = oracle_check(&mut d, &book, &[]);
            assert!(diffs.is_empty(), "TZ={}: {}", label(*tz), diffs.join("\n"));
            let scripts = oracle::scripts_dir().display().to_string();
            let out = oracle::run_code_in(POSTED, &[&book.path_str(), &scripts], *tz).unwrap_or_else(|e| panic!("oracle: {e}"));
            let g: Value = serde_json::from_str(&out).unwrap();
            g["transactions"].as_array().unwrap().iter().for_each(|t| {
                assert_eq!(t[0], t[2], "GnuCash reads the posted date red-ale was given, TZ={}", label(*tz));
                assert_eq!(t[1], t[3], "stored at GnuCash's neutral time for that date, TZ={}", label(*tz));
            });
            assert_eq!(
                ints(&g["prices"]),
                vec![zone(*tz).neutral_time(day("2024-06-20")).0],
                "TZ={}",
                label(*tz)
            );
        });
}

/// GnuCash builds one Boost rule per year from a zone file (`TimeZoneProvider::parse_file`,
/// `5.17:libgnucash/engine/gnc-timezone.cpp:635-720`); red-ale follows the file's transitions. They differ by an hour
/// at local midnight on the autumn change in zones whose rules are in UT (most of Europe), and all summer from 2037,
/// when GnuCash drops daylight saving. Stored dates are not affected: neutral times and local noons agree.
#[test]
fn gnucash_zone_model_deviations() {
    let london = Some("Europe/London");
    let z = zone(london);
    let autumn = day("2024-10-27").start_utc().0 + 12 * 3600;
    let summer_2040 = day("2040-07-01").start_utc().0 + 12 * 3600;
    let spec = json!({ "instants": [autumn, summer_2040], "dates": [[2024, 10, 27]], "nows": [] }).to_string();
    let out = oracle::run_script_in(&oracle::scripts_dir().join("time_zones.py"), &[&spec], london).unwrap();
    let g: Value = serde_json::from_str(&out).unwrap();
    let starts = ints(&g["dayStart"]);
    assert_eq!(
        z.day_start(day("2024-10-27")).to_file_string(),
        "2024-10-26 23:00:00 +0000",
        "midnight BST"
    );
    assert_eq!(starts[0] - z.day_start(day("2024-10-27")).0, 3600, "GnuCash: midnight GMT");
    assert_eq!(starts[1] - z.day_start(day("2040-07-01")).0, 3600, "GnuCash: no BST in 2040");
    assert_eq!(ints(&g["noon"])[0], z.noon(day("2024-10-27")).0, "local noon agrees");
    assert_eq!(ints(&g["neutral"])[0], z.neutral_time(day("2024-10-27")).0, "neutral time agrees");
}
