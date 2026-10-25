//! `TZ` strings as GnuCash reads them.
//!
//! When `TZ` names no zone file, GnuCash parses it with Boost.Date_Time's `posix_time_zone`
//! (`5.17:libgnucash/engine/gnc-timezone.cpp:722-738`), which reads the POSIX syntax with its own rules (boost 1.83,
//! `boost/date_time/local_time/posix_time_zone.hpp`, `calc_zone` and `calc_rules`):
//!
//! - offsets are hours **east** of UTC, the opposite of POSIX: `XYZ11` is UTC+11;
//! - the standard offset must be within −12 h and +14 h;
//! - names are letters only (no `<...>` quoting) and may be empty;
//! - the DST offset is the *adjustment* to the standard offset (default one hour), not the DST zone's offset;
//! - with a DST name, both rule fields must follow, each `Mm.w.d`, `Jn` or `n` with an optional `/time` from 0 to 24 h
//!   (default 02:00).
//!
//! The string is turned into a standard POSIX string with the same meaning, which `jiff` evaluates. Boost counts a
//! `Jn` start day that falls on a month's last day one day off; red-ale reads `Jn` as POSIX does.

const DAY: i64 = 86_400;

/// Translate a Boost-style `TZ` string into a POSIX one, or say why Boost would refuse it.
pub(super) fn to_posix(text: &str) -> Result<String, String> {
    // Boost's tokenizer drops empty fields.
    let fields: Vec<&str> = text.split(',').filter(|f| !f.is_empty()).collect();
    let first = fields.first().ok_or("no zone name")?;
    let zone = parse_zone(first)?;
    let Some(adjust) = zone.dst_adjust else { return Ok(format!("<STD>{}", posix_offset(zone.std))) };
    let (start, end) = match (fields.get(1), fields.get(2)) {
        (Some(s), Some(e)) => (parse_rule(s)?, parse_rule(e)?),
        _ => return Err("a DST name needs a start and an end rule".to_owned()),
    };
    Ok(format!("<STD>{}<DST>{},{start},{end}", posix_offset(zone.std), posix_offset(zone.std + adjust)))
}

struct Zone {
    /// Seconds east of UTC.
    std: i64,
    /// The DST adjustment in seconds, when the zone has DST.
    dst_adjust: Option<i64>,
}

/// Split off the leading run of letters.
fn letters(s: &str) -> (&str, &str) {
    s.split_at(s.find(|c: char| !c.is_ascii_alphabetic()).unwrap_or(s.len()))
}

/// Split off the leading run of non-letters.
fn non_letters(s: &str) -> (&str, &str) {
    s.split_at(s.find(|c: char| c.is_ascii_alphabetic()).unwrap_or(s.len()))
}

fn parse_zone(s: &str) -> Result<Zone, String> {
    let (_std_name, rest) = letters(s);
    let (offset, rest) = non_letters(rest);
    let std = if offset.is_empty() { 0 } else { duration(offset)? };
    if !(-12 * 3600..=14 * 3600).contains(&std) {
        return Err(format!("offset {offset} is outside -12 to +14 hours"));
    }
    if rest.is_empty() {
        return Ok(Zone { std, dst_adjust: None });
    }
    let (_dst_name, rest) = letters(rest);
    let (adjust, _ignored) = non_letters(rest);
    let adjust = if adjust.is_empty() { 3600 } else { duration(adjust)? };
    if adjust.abs() >= DAY {
        return Err(format!("DST adjustment {adjust}s is a day or more"));
    }
    Ok(Zone { std, dst_adjust: Some(adjust) })
}

/// `[+|-]h[h][:mm[:ss]]` in seconds.
fn duration(s: &str) -> Result<i64, String> {
    let bad = || format!("not a duration: {s:?}");
    let (sign, digits) = match s.as_bytes().first() {
        Some(b'-') => (-1, &s[1..]),
        Some(b'+') => (1, &s[1..]),
        _ => (1, s),
    };
    let parts: Vec<&str> = digits.split(':').collect();
    if parts.len() > 3 || parts.iter().any(|p| p.is_empty() || !p.bytes().all(|b| b.is_ascii_digit())) {
        return Err(bad());
    }
    let number = |i: usize| parts.get(i).map_or(Ok(0), |p| p.parse::<i64>().map_err(|_| bad()));
    let (h, m, sec) = (number(0)?, number(1)?, number(2)?);
    if m > 59 || sec > 59 {
        return Err(bad());
    }
    Ok(sign * (h * 3600 + m * 60 + sec))
}

/// A rule field, `date[/time]`, checked as Boost checks it and written as POSIX.
fn parse_rule(s: &str) -> Result<String, String> {
    let (date, time) = s.split_once('/').map_or((s, None), |(d, t)| (d, Some(t)));
    let at = time.map_or(Ok(2 * 3600), duration)?;
    if !(0..DAY).contains(&at) {
        return Err(format!("rule time {s:?} is not within the day"));
    }
    Ok(format!("{}/{}", rule_date(date)?, clock(at)))
}

fn rule_date(s: &str) -> Result<String, String> {
    let bad = || format!("not a DST rule: {s:?}");
    let number = |t: &str| t.parse::<u32>().map_err(|_| bad());
    if let Some(m) = s.strip_prefix('M') {
        let parts = m.split('.').map(number).collect::<Result<Vec<_>, _>>()?;
        match parts.as_slice() {
            [month @ 1..=12, week @ 1..=5, weekday @ 0..=6] => Ok(format!("M{month}.{week}.{weekday}")),
            _ => Err(bad()),
        }
    } else if let Some(j) = s.strip_prefix('J') {
        number(j).ok().filter(|n| (1..=365).contains(n)).map(|n| format!("J{n}")).ok_or_else(bad)
    } else {
        number(s).ok().filter(|n| *n <= 365).map(|n| n.to_string()).ok_or_else(bad)
    }
}

fn clock(secs: i64) -> String {
    format!("{:02}:{:02}:{:02}", secs / 3600, secs % 3600 / 60, secs % 60)
}

/// Seconds east of UTC as a POSIX offset (hours west).
fn posix_offset(east: i64) -> String {
    let west = -east;
    let sign = if west < 0 { "-" } else { "+" };
    format!("{sign}{}", clock(west.abs()))
}

#[cfg(test)]
mod tests {
    use super::super::{Date, Time64, TimeZone};
    use super::*;

    fn offset(tz: &str, at: &str) -> i32 {
        TimeZone::from_gnucash_posix(tz).unwrap().offset_at(Time64::parse_file_string(at).unwrap())
    }

    #[test]
    fn offsets_have_boosts_sign() {
        assert_eq!(offset("XYZ11", "2024-06-20 00:00:00"), 11 * 3600, "the oracle's day starts at 13:00 UTC under XYZ11");
        assert_eq!(offset("XYZ-11", "2024-06-20 00:00:00"), -11 * 3600);
        assert_eq!(offset("XYZ+14", "2024-06-20 00:00:00"), 14 * 3600);
        assert_eq!(offset("UTC0", "2024-06-20 00:00:00"), 0);
        assert_eq!(offset("UTC", "2024-06-20 00:00:00"), 0);
        assert_eq!(offset("IST5:30", "2024-06-20 00:00:00"), 5 * 3600 + 1800);
        assert_eq!(offset("+3", "2024-06-20 00:00:00"), 3 * 3600, "an empty name is allowed");
    }

    #[test]
    fn dst_adjusts_the_standard_offset() {
        assert_eq!(offset("EST-5EDT,M3.2.0,M11.1.0", "2024-07-01 00:00:00"), -4 * 3600);
        assert_eq!(offset("EST-5EDT,M3.2.0,M11.1.0", "2024-01-01 00:00:00"), -5 * 3600);
        assert_eq!(offset("EST-5EDT2,M3.2.0/1:30,M11.1.0", "2024-07-01 00:00:00"), -3 * 3600);
        let ny = TimeZone::from_gnucash_posix("EST-5EDT,M3.2.0,M11.1.0").unwrap();
        let start = ny.day_start(Date::from_ymd(2024, 3, 10).unwrap());
        assert_eq!(start.to_file_string(), "2024-03-10 05:00:00 +0000");
    }

    #[test]
    fn what_boost_refuses_is_refused() {
        ["", "<-11>11", "XYZ-13", "XYZ-14", "XYZ15", "Nowhere/Land", "XYZ+", "EST5EDT", "EST5EDT,M3.2.0", "EST5EDT,M13.1.0,M1.1.0", "A5B,M3.2.0/24,M11.1.0"]
            .iter()
            .for_each(|tz| assert!(TimeZone::from_gnucash_posix(tz).is_err(), "{tz:?}"));
    }

    #[test]
    fn translation() {
        assert_eq!(to_posix("XYZ11").unwrap(), "<STD>-11:00:00");
        assert_eq!(to_posix("EST-5EDT,M3.2.0,J60/3").unwrap(), "<STD>+05:00:00<DST>+04:00:00,M3.2.0/02:00:00,J60/03:00:00");
    }
}
