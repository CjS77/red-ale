//! The local time zone, read from the environment as GnuCash 5.17 reads it (`TimeZoneProvider`,
//! `5.17:libgnucash/engine/gnc-timezone.cpp:743-761`; see `design/notes/time-zones.md` §1):
//!
//! 1. `TZ` as a zone file: a leading `:` is dropped, an absolute path is read as given, any other name from `TZDIR`
//!    or `/usr/share/zoneinfo` (`find_tz_file`, `:354-389`);
//! 2. `TZ` as a POSIX string, with Boost's reading of it ([`TimeZone::from_gnucash_posix`]);
//! 3. `/etc/localtime`;
//! 4. UTC.
//!
//! GnuCash reads the zone once, when the engine loads; so does red-ale.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use red_ale_core::TimeZone;

const ZONEINFO: &str = "/usr/share/zoneinfo";
const LOCALTIME: &str = "/etc/localtime";

/// The process's local time zone, read from the environment on first use.
pub fn local_zone() -> &'static TimeZone {
    static ZONE: OnceLock<TimeZone> = OnceLock::new();
    ZONE.get_or_init(|| {
        let tz = std::env::var("TZ").ok();
        let tzdir = std::env::var_os("TZDIR").map(PathBuf::from);
        zone_from(tz.as_deref(), tzdir.as_deref(), &|p| std::fs::read(p).ok())
    })
}

/// The zone GnuCash would use given `TZ` and `TZDIR`, reading zone files through `read`.
pub fn zone_from(tz: Option<&str>, tzdir: Option<&Path>, read: &dyn Fn(&Path) -> Option<Vec<u8>>) -> TimeZone {
    tz.filter(|t| !t.is_empty())
        .and_then(|t| zone_file(t, tzdir, read).or_else(|| TimeZone::from_gnucash_posix(t).ok()))
        .or_else(|| read(Path::new(LOCALTIME)).and_then(|data| TimeZone::from_tzif(&localtime_name(), &data).ok()))
        .unwrap_or_else(TimeZone::utc)
}

/// `TZ` read as a zone file name.
fn zone_file(tz: &str, tzdir: Option<&Path>, read: &dyn Fn(&Path) -> Option<Vec<u8>>) -> Option<TimeZone> {
    let name = tz.strip_prefix(':').unwrap_or(tz);
    if name.is_empty() {
        return None;
    }
    let path = if name.starts_with('/') { PathBuf::from(name) } else { tzdir.unwrap_or(Path::new(ZONEINFO)).join(name) };
    read(&path).and_then(|data| TimeZone::from_tzif(name, &data).ok())
}

/// `/etc/localtime`'s IANA name when it links into a zoneinfo tree, for display.
fn localtime_name() -> String {
    std::fs::read_link(LOCALTIME)
        .ok()
        .and_then(|target| target.to_str().and_then(|t| t.split_once("zoneinfo/").map(|(_, name)| name.to_owned())))
        .unwrap_or_else(|| LOCALTIME.to_owned())
}

#[cfg(test)]
mod tests {
    use red_ale_core::Time64;

    use super::*;

    /// A TZif version 1 file for a zone fixed at `offset` seconds east, with no transitions.
    fn tzif(offset: i32) -> Vec<u8> {
        let mut data = b"TZif\0".to_vec();
        data.extend_from_slice(&[0; 15]);
        [0_u32, 0, 0, 0, 1, 4].iter().for_each(|n| data.extend_from_slice(&n.to_be_bytes()));
        data.extend_from_slice(&offset.to_be_bytes());
        data.extend_from_slice(&[0, 0]);
        data.extend_from_slice(b"ZZZ\0");
        data
    }

    /// Zone files: `Pacific/Kiritimati` (+14) under the default tree, `Local/Zone` (+3) under `/tz`, and
    /// `/etc/localtime` (+1).
    fn files(path: &Path) -> Option<Vec<u8>> {
        match path.to_str()? {
            "/usr/share/zoneinfo/Pacific/Kiritimati" => Some(tzif(14 * 3600)),
            "/tz/Local/Zone" => Some(tzif(3 * 3600)),
            "/etc/localtime" => Some(tzif(3600)),
            "/usr/share/zoneinfo/Broken" => Some(b"not a zone".to_vec()),
            _ => None,
        }
    }

    fn offset(tz: Option<&str>, tzdir: Option<&str>) -> i32 {
        zone_from(tz, tzdir.map(Path::new), &files).offset_at(Time64(1_718_880_000))
    }

    #[test]
    fn gnucash_order() {
        assert_eq!(offset(Some("Pacific/Kiritimati"), None), 14 * 3600, "a zone file");
        assert_eq!(offset(Some(":Pacific/Kiritimati"), None), 14 * 3600, "a leading colon is dropped");
        assert_eq!(offset(Some("/usr/share/zoneinfo/Pacific/Kiritimati"), None), 14 * 3600, "an absolute path");
        assert_eq!(offset(Some("Local/Zone"), Some("/tz")), 3 * 3600, "TZDIR");
        assert_eq!(offset(Some("Pacific/Kiritimati"), Some("/tz")), 3600, "TZDIR replaces the default tree");
        assert_eq!(offset(Some("XYZ11"), None), 11 * 3600, "a POSIX string, with Boost's sign");
        assert_eq!(offset(Some("XYZ-13"), None), 3600, "refused by Boost: /etc/localtime");
        assert_eq!(offset(Some("Nowhere"), None), 0, "no such file: a POSIX name with no offset is UTC, as the oracle gives");
        assert_eq!(offset(Some("Broken"), None), 0, "not TZif (GnuCash aborts): read as a POSIX name, UTC");
        assert_eq!(offset(Some("Nowhere/Land"), None), 3600, "neither a file nor a POSIX string: /etc/localtime");
        assert_eq!(offset(Some(""), None), 3600, "empty");
        assert_eq!(offset(None, None), 3600, "unset");
        assert_eq!(zone_from(None, None, &|_| None), TimeZone::utc(), "nothing readable: UTC");
    }
}
