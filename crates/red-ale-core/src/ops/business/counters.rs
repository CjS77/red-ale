//! The book's counters: the last number issued for each kind of contact, document and order, and the format that
//! turns the next number into an ID (`qof_book_get_counter`, `qof_book_increment_and_format_counter`,
//! `qof_book_get_counter_format` and `qof_book_normalize_counter_format`, `5.17:libgnucash/engine/qofbook.cpp:592-917`,
//! GPL-2.0-or-later; the format check below follows that code). See `design/api/business-contacts.md`, "Counters".

use crate::ops::business::contacts::{COUNTER_CUSTOMER, COUNTER_EMPLOYEE, COUNTER_JOB, COUNTER_VENDOR};
use crate::ops::business::documents::{COUNTER_BILL, COUNTER_EXPENSE_VOUCHER, COUNTER_INVOICE, COUNTER_ORDER};
use crate::{Book, DomainError, DomainResult, SlotValue};

/// The counters GnuCash's book options offer, in their order (`5.17:libgnucash/engine/gnc-optiondb.cpp:1228-1297`).
pub const COUNTERS: [&str; 8] = [
    COUNTER_CUSTOMER,
    COUNTER_EMPLOYEE,
    COUNTER_INVOICE,
    COUNTER_BILL,
    COUNTER_EXPENSE_VOUCHER,
    COUNTER_JOB,
    COUNTER_ORDER,
    COUNTER_VENDOR,
];

/// The highest value the book options let a counter take (`5.17:libgnucash/engine/gnc-optiondb.cpp:947-956`).
pub const MAX_COUNTER: i64 = 999_999_999;

/// The format GnuCash numbers with when a counter has no valid one: six digits, zero-padded.
pub const DEFAULT_FORMAT: &str = "%.6li";

/// The frame holding the counters.
const COUNTERS_FRAME: &str = "counters";
/// The frame holding the counter formats.
const FORMATS_FRAME: &str = "counter_formats";

/// The conversions GnuCash tries in turn: `G_GINT64_FORMAT`, `lli`, `I64i`, `PRIi64` and `li`, of which `li` is
/// both the first and the last on 64-bit Linux (`5.17:libgnucash/engine/qofbook.cpp:755-761`).
const CONVERSIONS: [&str; 3] = ["li", "lli", "I64i"];
/// The printf flags the format check skips.
const FLAGS: &str = "#0- +'I";

/// The last number issued from a counter: `counters/<name>`, 0 when absent. A double (GnuCash bug 798930) is
/// truncated, as `qof_book_get_counter` does when the integer reads as zero.
pub fn counter_value(book: &Book, name: &str) -> i64 {
    match book.slots.get(&format!("{COUNTERS_FRAME}/{name}")) {
        Some(SlotValue::Integer(n)) => *n,
        Some(SlotValue::Double(text)) => text.trim().split('.').next().and_then(|i| i.parse().ok()).unwrap_or(0),
        _ => 0,
    }
}

/// The format stored for a counter, valid or not.
pub fn counter_format<'a>(book: &'a Book, name: &str) -> Option<&'a str> {
    book.slots.get_str(&format!("{FORMATS_FRAME}/{name}"))
}

/// Take the next number from a counter and format it with the book's format for it, as GnuCash's `gnc<Type>NextID`
/// does. The counter goes up even if the caller then discards the ID, as in GnuCash.
pub fn next_id(book: &mut Book, name: &str) -> String {
    let n = counter_value(book, name).saturating_add(1);
    book.slots.set(&format!("{COUNTERS_FRAME}/{name}"), SlotValue::Integer(n));
    format_counter(counter_format(book, name), n)
}

/// The ID the next object numbered from a counter would get, without taking it.
pub fn peek_next_id(book: &Book, name: &str) -> String {
    format_counter(counter_format(book, name), counter_value(book, name).saturating_add(1))
}

/// Every counter name: the eight of [`COUNTERS`], then any other the book's `counters` or `counter_formats` frames
/// hold, by name.
pub fn counter_names(book: &Book) -> Vec<String> {
    let mut others: Vec<String> = [COUNTERS_FRAME, FORMATS_FRAME]
        .iter()
        .filter_map(|frame| book.slots.get(frame).and_then(SlotValue::as_frame))
        .flat_map(|f| f.iter().map(|(k, _)| k.clone()))
        .filter(|k| !COUNTERS.contains(&k.as_str()))
        .collect();
    others.sort();
    others.dedup();
    COUNTERS.iter().map(|n| (*n).to_owned()).chain(others).collect()
}

/// Set a counter's last number and its format. `format` is `None` to keep it, `Some(None)` or `Some(Some(""))` to
/// remove it. Only the counters the book options offer can be set, a number from 0 to [`MAX_COUNTER`], and a format
/// GnuCash would number with.
pub fn set_counter(book: &mut Book, name: &str, last: Option<i64>, format: Option<Option<&str>>) -> DomainResult<()> {
    if !COUNTERS.contains(&name) {
        return Err(DomainError::invalid(format!("unknown counter {name}; the counters are {}", COUNTERS.join(", "))));
    }
    if let Some(n) = last.filter(|n| !(0..=MAX_COUNTER).contains(n)) {
        return Err(DomainError::invalid(format!("a counter must be from 0 to {MAX_COUNTER}, not {n}")));
    }
    if let Some(Some(f)) = format.filter(|f| f.is_some_and(|f| !f.is_empty())) {
        check_counter_format(f)?;
    }
    if let Some(n) = last {
        book.slots.set(&format!("{COUNTERS_FRAME}/{name}"), SlotValue::Integer(n));
    }
    if let Some(f) = format {
        book.slots.set_opt_str(&format!("{FORMATS_FRAME}/{name}"), f);
    }
    Ok(())
}

/// Refuse a format GnuCash would not number with, with its message, and one `printf` itself would not accept.
pub fn check_counter_format(format: &str) -> DomainResult<()> {
    normalize_counter_format(format).map_err(|e| DomainError::invalid(format!("invalid counter format {format:?}: {e}")))?;
    CounterFormat::parse(format)
        .map(|_| ())
        .ok_or_else(|| DomainError::invalid(format!("invalid counter format {format:?}: the width and precision have more than one dot")))
}

/// `qof_book_normalize_counter_format`: the format with its conversion rewritten as `li` (`PRIi64`), or GnuCash's
/// message saying why it is refused. GnuCash tries each conversion it knows and reports the last failure.
pub fn normalize_counter_format(format: &str) -> Result<String, String> {
    CONVERSIONS
        .iter()
        .find_map(|spec| split_format(format, spec).ok())
        .map(|parts| format!("{}{}li{}", parts.prefix, parts.modifiers, parts.suffix))
        .ok_or_else(|| split_format(format, "li").err().unwrap_or_default())
}

/// A counter format cut around its one conversion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FormatParts<'a> {
    /// The text before the conversion, up to and including its `%`.
    prefix: &'a str,
    /// Flags, width and precision.
    modifiers: &'a str,
    /// The text after the conversion.
    suffix: &'a str,
}

/// `qof_book_normalize_counter_format_internal` for one conversion `spec` (`5.17:libgnucash/engine/qofbook.cpp:784-917`).
/// The conversion is the first occurrence of `spec` after the first lone `%`; what lies between must be flags, then
/// width and precision characters.
fn split_format<'a>(format: &'a str, spec: &str) -> Result<FormatParts<'a>, String> {
    let start = find_conversion(format).ok_or("Format string ended without any conversion specification")?;
    let rest = &format[start + 1..];
    let at = rest.find(spec).ok_or_else(|| format!("Format string doesn't contain requested format specifier: {spec}"))?;
    let modifiers = &rest[..at];
    let flags_end = modifiers.find(|c: char| !FLAGS.contains(c)).unwrap_or(modifiers.len());
    let digits_end = modifiers[flags_end..].find(|c: char| !(c.is_ascii_digit() || c == '.')).map_or(modifiers.len(), |i| flags_end + i);
    if digits_end < modifiers.len() {
        return Err(format!("Garbage before length modifier and/or conversion specifier: '{}'", &modifiers[digits_end..]));
    }
    let suffix = &rest[at + spec.len()..];
    if let Some(i) = find_conversion(suffix) {
        return Err(format!("Format string contains unescaped % signs (or multiple conversion specifications) at '{}'", &suffix[i..]));
    }
    Ok(FormatParts { prefix: &format[..=start], modifiers, suffix })
}

/// Format a counter value with a user format, falling back to GnuCash's default `"%.6" PRIi64` when the format is
/// absent or invalid (`qof_book_get_counter_format`, `5.17:libgnucash/engine/qofbook.cpp:697-750`).
pub fn format_counter(format: Option<&str>, n: i64) -> String {
    let spec = format.and_then(CounterFormat::parse).unwrap_or(CounterFormat::DEFAULT);
    spec.format(n)
}

/// A counter format reduced to what `printf` does with one 64-bit integer conversion.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)] // One per printf flag that changes the output.
struct CounterFormat<'a> {
    prefix: &'a str,
    suffix: &'a str,
    left: bool,
    zero: bool,
    plus: bool,
    space: bool,
    width: usize,
    precision: Option<usize>,
}

impl<'a> CounterFormat<'a> {
    const DEFAULT: CounterFormat<'static> =
        CounterFormat { prefix: "", suffix: "", left: false, zero: false, plus: false, space: false, width: 0, precision: Some(6) };

    /// A format GnuCash accepts, as `printf` reads it. A width/precision run `printf` itself would not accept (two
    /// dots) is treated as invalid.
    fn parse(s: &'a str) -> Option<Self> {
        let parts = CONVERSIONS.iter().find_map(|spec| split_format(s, spec).ok())?;
        let m = parts.modifiers;
        let flags_end = m.find(|c: char| !FLAGS.contains(c)).unwrap_or(m.len());
        let (flags, digits) = m.split_at(flags_end);
        let (width, precision) = match digits.split_once('.') {
            Some((w, p)) => (w, Some(p)),
            None => (digits, None),
        };
        let number = |t: &str| if t.is_empty() { Some(0) } else { t.parse::<usize>().ok() };
        let precision = match precision {
            Some(p) => Some(number(p)?),
            None => None,
        };
        Some(Self {
            prefix: &parts.prefix[..parts.prefix.len() - 1],
            suffix: parts.suffix,
            left: flags.contains('-'),
            zero: flags.contains('0'),
            plus: flags.contains('+'),
            space: flags.contains(' '),
            width: number(width)?,
            precision,
        })
    }

    fn format(&self, n: i64) -> String {
        let mut digits = n.unsigned_abs().to_string();
        if let Some(p) = self.precision {
            digits = if p == 0 && n == 0 { String::new() } else { format!("{digits:0>p$}") };
        }
        let sign = if n < 0 {
            "-"
        } else if self.plus {
            "+"
        } else if self.space {
            " "
        } else {
            ""
        };
        let len = sign.len() + digits.len();
        let pad = self.width.saturating_sub(len);
        let body = if self.left {
            format!("{sign}{digits}{}", " ".repeat(pad))
        } else if self.zero && self.precision.is_none() {
            format!("{sign}{}{digits}", "0".repeat(pad))
        } else {
            format!("{}{sign}{digits}", " ".repeat(pad))
        };
        format!("{}{body}{}", unescape(self.prefix), unescape(self.suffix))
    }
}

/// The byte offset of the first `%` that is not part of `%%`.
fn find_conversion(s: &str) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match (bytes[i], bytes.get(i + 1)) {
            (b'%', Some(b'%')) => i += 2,
            (b'%', _) => return Some(i),
            _ => i += 1,
        }
    }
    None
}

fn unescape(s: &str) -> String {
    s.replace("%%", "%")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FixedEnv, Time64};

    fn book() -> Book {
        Book::new_empty(&mut FixedEnv::new(Time64(0), 1))
    }

    #[test]
    fn counters_count_up_and_use_the_default_format() {
        let mut b = book();
        assert_eq!(next_id(&mut b, COUNTER_CUSTOMER), "000001");
        assert_eq!(next_id(&mut b, COUNTER_CUSTOMER), "000002");
        assert_eq!(b.slots.get("counters/gncCustomer"), Some(&SlotValue::Integer(2)));
        b.slots.set("counters/gncVendor", SlotValue::Integer(1_234_567));
        assert_eq!(peek_next_id(&b, COUNTER_VENDOR), "1234568");
        assert_eq!(next_id(&mut b, COUNTER_VENDOR), "1234568");
        b.slots.set("counters/gncJob", SlotValue::Double("4.0".into()));
        assert_eq!(next_id(&mut b, COUNTER_JOB), "000005");
    }

    #[test]
    fn counter_formats_follow_gnucash() {
        // Oracle results in design/notes/engine-business.md §1.7.
        assert_eq!(format_counter(Some("INV-%04li"), 3), "INV-0003");
        assert_eq!(format_counter(Some("C-%li%%"), 2), "C-2%");
        assert_eq!(format_counter(Some("%d"), 1), "000001");
        assert_eq!(format_counter(Some("%lli"), 7), "7");
        assert_eq!(format_counter(Some("%I64i"), 7), "7");
        assert_eq!(format_counter(Some("%-5li|"), 7), "7    |");
        assert_eq!(format_counter(Some("%+li"), 7), "+7");
        assert_eq!(format_counter(Some("%5.3li"), 7), "  007");
        assert_eq!(format_counter(Some("%li-%li"), 7), "000007");
        assert_eq!(format_counter(Some("no conversion"), 7), "000007");
        assert_eq!(format_counter(Some("%1.2.3li"), 7), "000007");
        assert_eq!(format_counter(Some("%%li %li"), 9), "%li 9");
        assert_eq!(format_counter(Some("%I64li"), 9), format!("{:>64}", 9), "I is a flag, 64 the width");
    }

    /// The cases of `test_book_normalize_counter` (`5.17:libgnucash/engine/test/test-qofbook.cpp:127-255`), with
    /// `G_GINT64_FORMAT` and `PRIi64` as on 64-bit Linux (`li`).
    #[test]
    fn normalisation_matches_gnucash_tests() {
        assert!(normalize_counter_format("This string is missing the conversion specifier").is_err());
        assert_eq!(normalize_counter_format("Test - %li").as_deref(), Ok("Test - %li"));
        assert_eq!(normalize_counter_format("Test - %I64i").as_deref(), Ok("Test - %li"));
        assert_eq!(normalize_counter_format("Test - %.3li").as_deref(), Ok("Test - %.3li"));
        assert_eq!(normalize_counter_format("Test - %5li").as_deref(), Ok("Test - %5li"));
        assert_eq!(normalize_counter_format("Test - %5.4li").as_deref(), Ok("Test - %5.4li"));
        assert_eq!(split_format("Test - %li", "li").map(|p| p.suffix), Ok(""));
        assert!(split_format("Test - %li", "I64i").is_err(), "an erroneous Windows format");
        assert!(split_format("Test - %I64i", "li").is_err(), "an erroneous Linux format");
    }

    #[test]
    fn normalisation_reports_gnucash_messages() {
        assert_eq!(normalize_counter_format("plain"), Err("Format string ended without any conversion specification".to_owned()));
        assert_eq!(normalize_counter_format("%d"), Err("Format string doesn't contain requested format specifier: li".to_owned()));
        assert_eq!(normalize_counter_format("%5xli"), Err("Garbage before length modifier and/or conversion specifier: 'x'".to_owned()));
        assert_eq!(
            normalize_counter_format("%li-%li"),
            Err("Format string contains unescaped % signs (or multiple conversion specifications) at '%li'".to_owned())
        );
        assert_eq!(normalize_counter_format("%%%lli%%").as_deref(), Ok("%%%li%%"));
        assert_eq!(normalize_counter_format("%1.2.3li").as_deref(), Ok("%1.2.3li"), "GnuCash accepts two dots");
    }

    #[test]
    fn setting_checks_names_ranges_and_formats() {
        let mut b = book();
        set_counter(&mut b, COUNTER_INVOICE, Some(41), Some(Some("INV-%04li"))).unwrap();
        assert_eq!(peek_next_id(&b, COUNTER_INVOICE), "INV-0042");
        assert_eq!(counter_format(&b, COUNTER_INVOICE), Some("INV-%04li"));
        set_counter(&mut b, COUNTER_INVOICE, None, Some(None)).unwrap();
        assert_eq!(peek_next_id(&b, COUNTER_INVOICE), "000042", "the format is removed, the number kept");
        set_counter(&mut b, COUNTER_BILL, Some(0), Some(Some(""))).unwrap();
        assert_eq!(b.slots.get("counters/gncBill"), Some(&SlotValue::Integer(0)));
        let refused = [
            set_counter(&mut b, "gncNothing", Some(1), None),
            set_counter(&mut b, COUNTER_BILL, Some(-1), None),
            set_counter(&mut b, COUNTER_BILL, Some(MAX_COUNTER + 1), None),
            set_counter(&mut b, COUNTER_BILL, None, Some(Some("%d"))),
            set_counter(&mut b, COUNTER_BILL, Some(5), Some(Some("%1.2.3li"))),
        ];
        assert!(refused.iter().all(|r| matches!(r, Err(DomainError::Invalid(_)))));
        assert_eq!(counter_value(&b, COUNTER_BILL), 0, "a refused change changes nothing");
    }

    #[test]
    fn names_list_the_options_then_others() {
        let mut b = book();
        b.slots.set("counters/zzz", SlotValue::Integer(3));
        b.slots.set("counter_formats/aaa", SlotValue::String("%li".into()));
        b.slots.set("counters/gncVendor", SlotValue::Integer(3));
        let names = counter_names(&b);
        assert_eq!(&names[..8], COUNTERS.map(str::to_owned));
        assert_eq!(&names[8..], ["aaa", "zzz"]);
    }
}
