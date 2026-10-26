//! The run context `ctx` (`design/reports.md` §5, "Time"): now, the local time zone, the report settings and the
//! accounting period. Scripts read time only through it; Rhai's own clock is compiled out (`no_time`).

use std::rc::Rc;

use red_ale_core::time::PeriodBounds;
use red_ale_core::{Date, Time64, TimeZone};
use rhai::{Dynamic, Engine, INT, Map};

use crate::query::{BookHandle, Fallible, Snapshot, script_error, time_arg};
use crate::settings::{ReportSettings, WeekStart};

/// What a run knows about time and preferences: the context handed to a template's `render` as `ctx` (Rhai type
/// `Context`).
#[derive(Debug, Clone)]
pub struct ContextHandle(Rc<RunContext>);

#[derive(Debug)]
struct RunContext {
    snap: Rc<Snapshot>,
    zone: TimeZone,
    settings: ReportSettings,
    /// The accounting period at now, or why it cannot be computed (a fiscal period with no fiscal year end).
    period: Result<PeriodBounds, String>,
}

impl ContextHandle {
    /// The context of a run over `snap` in `zone`.
    pub(crate) fn new(snap: Rc<Snapshot>, zone: TimeZone, settings: ReportSettings, period: Result<PeriodBounds, String>) -> Self {
        Self(Rc::new(RunContext { snap, zone, settings, period }))
    }

    /// The report settings of the run.
    pub(crate) fn settings(&self) -> &ReportSettings {
        &self.0.settings
    }

    fn now(&self) -> Time64 {
        self.0.snap.now()
    }

    fn date(&self, t: Time64) -> Fallible<Date> {
        self.0.zone.local_date(t).ok_or_else(|| script_error(format!("time {} is outside the supported range", t.0)))
    }
}

pub(super) fn register(engine: &mut Engine) {
    engine.register_type_with_name::<ContextHandle>("Context");
    engine.register_fn("to_string", |_: &mut ContextHandle| "Context".to_owned());
    engine.register_get("now", |c: &mut ContextHandle| c.now().0);
    engine.register_fn("today", |c: &mut ContextHandle| c.now().0);
    engine.register_fn("start_of_day", |c: &mut ContextHandle, t: Dynamic| -> Fallible<INT> {
        Ok(c.0.zone.day_start(c.date(time_arg(&t)?)?).0)
    });
    engine.register_fn("end_of_day", |c: &mut ContextHandle, t: Dynamic| -> Fallible<INT> { Ok(c.0.zone.day_end(c.date(time_arg(&t)?)?).0) });
    engine.register_fn("local", local);
    engine.register_fn("make_time", make_time);
    engine.register_fn("print_date", |c: &mut ContextHandle, t: Dynamic| -> Fallible<String> {
        Ok(c.0.settings.date_format.print(c.date(time_arg(&t)?)?))
    });
    engine.register_fn("print_date_interval", |c: &mut ContextHandle, from: Dynamic, to: Dynamic| -> Fallible<String> {
        Ok(c.0.settings.date_format.print_interval(c.date(time_arg(&from)?)?, c.date(time_arg(&to)?)?))
    });
    engine.register_fn("print_time_iso", |c: &mut ContextHandle, t: Dynamic| -> Fallible<String> {
        let t = time_arg(&t)?;
        let l = c.0.zone.to_local(t).ok_or_else(|| script_error(format!("time {} is outside the supported range", t.0)))?;
        Ok(format!("{} {:02}:{:02}:{:02}", l.date.to_file_string(), l.hour, l.minute, l.second))
    });
    engine.register_fn("accounting_period", |c: &mut ContextHandle| -> Fallible<Map> {
        let p = c.0.period.as_ref().map_err(|e| script_error(format!("the accounting period: {e}")))?;
        Ok([("start".into(), Dynamic::from(p.start.0)), ("end".into(), Dynamic::from(p.end.0))].into_iter().collect())
    });
    engine.register_fn("report_currency", |c: &mut ContextHandle| BookHandle(Rc::clone(&c.0.snap)).commodity(&c.0.settings.currency));
    engine.register_fn("week_start", |c: &mut ContextHandle| week_start(c.0.settings.week_start));
}

/// `gnc-start-of-week`: 1 for Sunday to 7 for Saturday (`5.17:libgnucash/engine/gnc-date.cpp:195-215`).
fn week_start(w: WeekStart) -> INT {
    match w {
        WeekStart::Sunday => 1,
        WeekStart::Monday => 2,
        WeekStart::Tuesday => 3,
        WeekStart::Wednesday => 4,
        WeekStart::Thursday => 5,
        WeekStart::Friday => 6,
        WeekStart::Saturday => 7,
    }
}

/// A Sunday, to count weekdays from.
fn sunday() -> Option<Date> {
    Date::from_ymd(1970, 1, 4)
}

/// `gnc-localtime`, in the numbering of the `gnc:date-get-*` functions scripts call
/// (`5.17:bindings/guile/date-utilities.scm:110-127`): `year` in full, `month` 1 to 12, `day` 1 to 31, `weekday` 1
/// (Sunday) to 7, `yearday` 1 to 366, and `hour`, `minute`, `second`.
fn local(c: &mut ContextHandle, t: Dynamic) -> Fallible<Map> {
    let t = time_arg(&t)?;
    let l = c.0.zone.to_local(t).ok_or_else(|| script_error(format!("time {} is outside the supported range", t.0)))?;
    let d = l.date;
    let jan1 = Date::from_ymd(d.year(), 1, 1).ok_or_else(|| script_error("date out of range"))?;
    let weekday = sunday().map_or(0, |s| s.days_until(d).rem_euclid(7)) + 1;
    let fields: [(&str, INT); 8] = [
        ("year", INT::from(d.year())),
        ("month", INT::from(d.month())),
        ("day", INT::from(d.day())),
        ("hour", INT::from(l.hour)),
        ("minute", INT::from(l.minute)),
        ("second", INT::from(l.second)),
        ("weekday", weekday),
        ("yearday", jan1.days_until(d) + 1),
    ];
    Ok(fields.into_iter().map(|(k, v)| (k.into(), Dynamic::from(v))).collect())
}

/// `gnc-mktime` with `isdst = -1`: the local time of `#{ year, month, day, hour, minute, second }` (the last three
/// default to 0), normalised as C's `mktime` does, so 31 January + 1 month is 3 March and day 0 is the last day of
/// the month before. A local time in a gap is taken an hour later, as GnuCash makes local times.
fn make_time(c: &mut ContextHandle, fields: Map) -> Fallible<INT> {
    let known = ["year", "month", "day", "hour", "minute", "second"];
    if let Some(key) = fields.keys().find(|k| !known.contains(&k.as_str())) {
        return Err(script_error(format!("unknown time field {key:?}")));
    }
    let field = |name: &str, default: Option<INT>| -> Fallible<INT> {
        match (fields.get(name), default) {
            (Some(v), _) => v.as_int().map_err(|t| script_error(format!("{name} must be an integer, got {t}"))),
            (None, Some(d)) => Ok(d),
            (None, None) => Err(script_error(format!("make_time needs {name}"))),
        }
    };
    let overflow = || script_error("the time is outside the supported range");
    let (year, month) = (field("year", None)?, field("month", None)?);
    let months = year.checked_mul(12).and_then(|m| m.checked_add(month)).and_then(|m| m.checked_sub(1)).ok_or_else(overflow)?;
    let year = i32::try_from(months.div_euclid(12)).ok().filter(|y| (-9999..=9999).contains(y)).ok_or_else(overflow)?;
    let month = u32::try_from(months.rem_euclid(12) + 1).map_err(|_| overflow())?;
    let seconds = [("day", 86_400, None), ("hour", 3_600, Some(0)), ("minute", 60, Some(0)), ("second", 1, Some(0))]
        .into_iter()
        .try_fold(-86_400_i64, |sum, (name, unit, default)| -> Fallible<INT> {
            field(name, default)?.checked_mul(unit).and_then(|s| sum.checked_add(s)).ok_or_else(overflow)
        })?;
    let first = Date::from_ymd(year, month, 1).ok_or_else(overflow)?;
    let date = first.add_days(seconds.div_euclid(86_400));
    let rest = u32::try_from(seconds.rem_euclid(86_400)).map_err(|_| overflow())?;
    c.0.zone.instant(date, rest / 3600, rest / 60 % 60, rest % 60).map(|t| t.0).ok_or_else(overflow)
}
