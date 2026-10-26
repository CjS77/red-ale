//! The QIF field parsers: categories, years, dates, numbers, cleared flags, investment actions, account types and
//! section headers.
//!
//! Translated from `5.17:gnucash/import-export/qif-imp/qif-parse.scm` ("routines to parse values and dates in QIF
//! files"), Bill Gribble <grib@billgribble.com> 20 Feb 2000. GPL-2.0-or-later; this translation is
//! GPL-3.0-or-later.
//!
//! GnuCash's regular expressions are POSIX extended expressions; the ones here are the same patterns in Rust's syntax.
//! For these patterns the groups match the same text under POSIX's leftmost-longest rule and Rust's leftmost-first
//! one, since every group is followed by a character it cannot contain or ends the line. GnuCash reads the numbers in
//! a date with Scheme's `read` and the numbers of an amount with `string->number`; [`read_integer`] and
//! [`exact_number`] stand for those on the text the patterns let through.
//!
//! `qif-parse:print-date`, `print-number` and `print-numbers` format values for GnuCash's log messages only and are
//! not translated.

use std::sync::LazyLock;

use red_ale_core::{AccountType, Numeric};
use regex::Regex;
use thiserror::Error;

use super::string::{string_delete_chars, string_replace_char};

/// A pattern of this module, compiled once. The patterns are constants, so `None` (a pattern that does not
/// compile) cannot happen; a test compiles each one.
type Pattern = LazyLock<Option<Regex>>;

fn compile(pattern: &str) -> Option<Regex> {
    Regex::new(pattern).ok()
}

fn is_match(pattern: &Pattern, text: &str) -> bool {
    pattern.as_ref().is_some_and(|re| re.is_match(text))
}

/// `qif-category-compiled-rexp`. `(?s)` lets `.` match any character, as POSIX `.` does. `qif-parse.scm:71-73`.
static CATEGORY: Pattern = LazyLock::new(|| compile(r"(?s)^ *(\[)?([^\]/|]*)(\]?)(/?)([^|]*)(\|(\[)?([^\]/]*)(\]?)(/?)(.*))? *$"));

/// `qif-date-compiled-rexp`. `qif-parse.scm:304-306`.
static DATE: Pattern =
    LazyLock::new(|| compile(r"(?s)^ *([0-9]+) *[-/.'] *([0-9]+) *[-/.'] *([0-9]+).*$|^ *([0-9][0-9][0-9][0-9][0-9][0-9][0-9][0-9]).*$"));

/// `decimal-radix-regexp`: for example 1000.00, 1,500.00 or 2'000.00. `qif-parse.scm:397-400`.
static DECIMAL_RADIX: Pattern = LazyLock::new(|| {
    compile(r"^ *[$]?[+-]?[$]?[0-9]+[+-]?$|^ *[$]?[+-]?[$]?[0-9]?[0-9]?[0-9]?([,'][0-9][0-9][0-9])*(\.[0-9]*)?[+-]? *$|^ *[$]?[+-]?[$]?[0-9]+\.[0-9]*[+-]? *$")
});

/// `comma-radix-regexp`: for example 5.000,00 or 4'500,00. `qif-parse.scm:402-405`.
static COMMA_RADIX: Pattern = LazyLock::new(|| {
    compile(r"^ *[$]?[+-]?[$]?[0-9]+[+-]?$|^ *[$]?[+-]?[$]?[0-9]?[0-9]?[0-9]?([.'][0-9][0-9][0-9])*(,[0-9]*)?[+-]? *$|^ *[$]?[+-]?[$]?[0-9]+,[0-9]*[+-]? *$")
});

/// `rational-regexp`: for example 1/2, 32/15 or 4. `qif-parse.scm:407-410`.
static RATIONAL: Pattern = LazyLock::new(|| compile(r"^-?[0-9]+(/[0-9]+|)$"));

/// `integer-regexp`: for example 456. `qif-parse.scm:412-415`.
static INTEGER: Pattern = LazyLock::new(|| compile(r"^[$]?[+-]?[$]?[0-9]+[+-]? *$"));

/// A category line split into its parts: what `qif-split:parse-category` returns. `qif-parse.scm:58-69`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CategoryInfo {
    /// The category or account name, without brackets.
    pub name: String,
    /// Whether the name was written as `[account]`. Only the opening bracket counts, as in GnuCash.
    pub is_account: bool,
    /// The class after `/`; empty when there is none.
    pub class: String,
    /// The miscx category after `|`, if any.
    pub miscx_name: Option<String>,
    /// Whether the miscx category was written as `[account]`.
    pub miscx_is_account: bool,
    /// The class of the miscx category: `Some` (perhaps empty) when there is a miscx category.
    pub miscx_class: Option<String>,
}

/// A category line GnuCash's pattern does not match. GnuCash throws `'bug` here (`qif-parse.scm:104-108`); the
/// pattern matches every line without a newline, so this does not happen in practice.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("can't parse account or category {0:?}")]
pub struct CategoryParseError(pub String);

/// `qif-split:parse-category`: `field1`, `field1/field2`, `field1/|field3` or `field1/|field3/field4`, where field1
/// is a category or `[account]`, field2 its class, field3 a miscx category or `[miscx-account]` and field4 its class.
/// `qif-parse.scm:75-108`.
pub fn parse_category(value: &str) -> Result<CategoryInfo, CategoryParseError> {
    let caps = CATEGORY.as_ref().and_then(|re| re.captures(value)).ok_or_else(|| CategoryParseError(value.to_owned()))?;
    let group = |i: usize| caps.get(i).map(|m| m.as_str().to_owned());
    let has = |i: usize| caps.get(i).is_some();
    Ok(CategoryInfo {
        name: group(2).unwrap_or_default(),
        is_account: has(1) && has(3),
        class: if has(4) { group(5).unwrap_or_default() } else { String::new() },
        miscx_name: if has(6) { group(8) } else { None },
        miscx_is_account: has(7) && has(9),
        miscx_class: if has(10) { group(11) } else { None },
    })
}

/// Scheme's `read` on a string, for the integers GnuCash reads that way: leading whitespace, then a token that is
/// an optional sign and digits. Anything else (another kind of datum, or nothing) is `None`. Integers beyond 64 bits,
/// which Scheme keeps exactly, saturate; the years and days GnuCash compares them with are far smaller.
pub fn read_integer(s: &str) -> Option<i64> {
    let token: String = s.trim_start().chars().take_while(|c| !c.is_whitespace() && !"()\";".contains(*c)).collect();
    let (negative, digits) = match token.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, token.strip_prefix('+').unwrap_or(&token)),
    };
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let magnitude = digits.bytes().fold(0_i64, |acc, b| acc.saturating_mul(10).saturating_add(i64::from(b - b'0')));
    Some(if negative { -magnitude } else { magnitude })
}

/// `qif-parse:fix-year`: the year a QIF year string means. "00", "2000" and "19100" all mean 2000. Years below the
/// threshold are after 2000, years above 19000 are Quicken's "19100" bug, years below 1902 count from 1900, and the
/// rest are as written. A leading `'` and the character after it are dropped. `None` when the string holds no
/// integer. `qif-parse.scm:111-152`.
pub fn fix_year(year_string: &str, y2k_threshold: i64) -> Option<i64> {
    let fixed: String = if year_string.starts_with('\'') {
        // Scheme's `substring` raises on a string shorter than 2; there is no year in it either way.
        if year_string.chars().count() < 2 {
            return None;
        }
        year_string.chars().skip(2).collect()
    } else {
        year_string.to_owned()
    };
    let value = read_integer(&fixed)?;
    Some(if value < y2k_threshold {
        value.saturating_add(2000)
    } else if value > 19000 {
        value.saturating_sub(19000).saturating_add(1900)
    } else if value < 1902 {
        value + 1900
    } else {
        value
    })
}

/// `qif-parse:parse-acct-type`: the GnuCash account types a QIF account type may map to. An unknown type is
/// reported to `errorproc` and maps to Bank. `qif-parse.scm:154-177`.
pub fn parse_acct_type(read_value: &str, errorproc: impl FnOnce(String)) -> Vec<AccountType> {
    use AccountType::{Asset, Bank, Cash, Credit, Liability};
    match read_value.trim().to_lowercase().as_str() {
        "bank" | "port" | "invst" | "401(k)/403(b)" | "mutual" => vec![Bank],
        "cash" => vec![Cash],
        "ccard" => vec![Credit],
        "oth a" | "oth s" => vec![Asset, Bank, Cash],
        "oth l" => vec![Liability, Credit],
        _ => {
            errorproc(format!("The account type {read_value:?} is unknown, using Bank instead."));
            vec![Bank]
        }
    }
}

/// A section of a QIF file, from its `!` header: the symbol `qif-parse:parse-bang-field` returns, with the symbols
/// `qif-file:read-file` acts on as variants. `qif-file.scm:194-250`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QifSection {
    /// `type:bank`.
    Bank,
    /// `type:cash`.
    Cash,
    /// `type:ccard`.
    Ccard,
    /// `type:invst`.
    Invst,
    /// `type:port`.
    Port,
    /// `type:oth a`.
    OthA,
    /// `type:oth l`.
    OthL,
    /// `type:oth s`.
    OthS,
    /// `type:class`.
    Class,
    /// `type:cat`.
    Cat,
    /// `account`.
    Account,
    /// `type:security`.
    Security,
    /// `type:memorized`: memorised transactions, which GnuCash skips.
    Memorized,
    /// `type:prices`.
    Prices,
    /// `option:autoswitch`.
    OptionAutoSwitch,
    /// `clear:autoswitch`.
    ClearAutoSwitch,
    /// Any other header, by its symbol.
    Other(String),
}

impl QifSection {
    /// The section a header symbol names.
    pub fn from_symbol(symbol: &str) -> Self {
        match symbol {
            "type:bank" => Self::Bank,
            "type:cash" => Self::Cash,
            "type:ccard" => Self::Ccard,
            "type:invst" => Self::Invst,
            "type:port" => Self::Port,
            "type:oth a" => Self::OthA,
            "type:oth l" => Self::OthL,
            "type:oth s" => Self::OthS,
            "type:class" => Self::Class,
            "type:cat" => Self::Cat,
            "account" => Self::Account,
            "type:security" => Self::Security,
            "type:memorized" => Self::Memorized,
            "type:prices" => Self::Prices,
            "option:autoswitch" => Self::OptionAutoSwitch,
            "clear:autoswitch" => Self::ClearAutoSwitch,
            other => Self::Other(other.to_owned()),
        }
    }

    /// Whether the section is a transaction list. `qif-file.scm:201-202`, `:261-262`.
    pub fn is_transactions(&self) -> bool {
        matches!(self, Self::Bank | Self::Cash | Self::Ccard | Self::Invst | Self::Port | Self::OthA | Self::OthL | Self::OthS)
    }

    /// Whether the section is an investment transaction list, where `N` is the action. `qif-file.scm:297-301`.
    pub fn is_investment(&self) -> bool {
        matches!(self, Self::Invst | Self::Port)
    }
}

/// `qif-parse:parse-bang-field`: the header's symbol, lower case, trailing spaces trimmed, with "type " read as
/// "type:" (the files of Crédit Lyonnais's web site start with `!type bank`). `qif-parse.scm:179-192`.
pub fn parse_bang_field(read_value: &str) -> String {
    let field = read_value.trim_end().to_lowercase();
    match field.strip_prefix("type ") {
        Some(rest) => format!("type:{rest}"),
        None => field,
    }
}

/// A QIF investment action, by GnuCash's name for it. `qif-parse.scm:194-233`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[allow(missing_docs)] // Each variant is the action GnuCash's symbol names.
pub enum Action {
    Buy,
    BuyX,
    CgLong,
    CgLongX,
    CgMid,
    CgMidX,
    CgShort,
    CgShortX,
    Div,
    DivX,
    IntInc,
    IntIncX,
    MargInt,
    MargIntX,
    MiscExp,
    MiscExpX,
    MiscInc,
    MiscIncX,
    ReinvDiv,
    ReinvInt,
    ReinvLg,
    ReinvMd,
    ReinvSg,
    ReinvSh,
    Reminder,
    RtrnCap,
    RtrnCapX,
    Sell,
    SellX,
    ShrsIn,
    ShrsOut,
    StkSplit,
    XIn,
    XOut,
    /// An action GnuCash does not know. GnuCash stores the value its error handler returns, which is present (so
    /// `(unless qif-action ...)` passes) but matches no action in a `case`.
    Unrecognized,
}

/// `action-map`: each action with the names that mean it, GnuCash's name first. `qif-parse.scm:195-233`.
const ACTION_MAP: [(Action, &[&str]); 34] = [
    (Action::Buy, &["buy", "cvrshrt", "kauf"]),
    (Action::BuyX, &["buyx", "cvrshrtx", "kaufx"]),
    (Action::CgLong, &["cglong", "kapgew"]),
    (Action::CgLongX, &["cglongx", "kapgewx"]),
    (Action::CgMid, &["cgmid"]),
    (Action::CgMidX, &["cgmidx"]),
    (Action::CgShort, &["cgshort", "k.gewsp"]),
    (Action::CgShortX, &["cgshortx", "k.gewspx"]),
    (Action::Div, &["div"]),
    (Action::DivX, &["divx"]),
    (Action::IntInc, &["intinc", "int"]),
    (Action::IntIncX, &["intincx", "intx"]),
    (Action::MargInt, &["margint"]),
    (Action::MargIntX, &["margintx"]),
    (Action::MiscExp, &["miscexp"]),
    (Action::MiscExpX, &["miscexpx"]),
    (Action::MiscInc, &["miscinc", "cash"]),
    (Action::MiscIncX, &["miscincx"]),
    (Action::ReinvDiv, &["reinvdiv"]),
    (Action::ReinvInt, &["reinvint", "reinvzin"]),
    (Action::ReinvLg, &["reinvlg", "reinvkur"]),
    (Action::ReinvMd, &["reinvmd"]),
    (Action::ReinvSg, &["reinvsg", "reinvksp"]),
    (Action::ReinvSh, &["reinvsh"]),
    (Action::Reminder, &["reminder", "erinnerg"]),
    (Action::RtrnCap, &["rtrncap"]),
    (Action::RtrnCapX, &["rtrncapx"]),
    (Action::Sell, &["sell", "shtsell", "verkauf"]),
    (Action::SellX, &["sellx", "shtsellx", "verkaufx"]),
    (Action::ShrsIn, &["shrsin", "aktzu"]),
    (Action::ShrsOut, &["shrsout", "aktab"]),
    (Action::StkSplit, &["stksplit", "aktsplit"]),
    (Action::XIn, &["xin", "contribx"]),
    (Action::XOut, &["xout", "withdrwx"]),
];

impl Action {
    /// GnuCash's symbol for the action; `None` for [`Action::Unrecognized`].
    pub fn symbol(self) -> Option<&'static str> {
        ACTION_MAP.iter().find(|(a, _)| *a == self).and_then(|(_, names)| names.first().copied())
    }
}

/// `qif-parse:parse-action-field`: the action a QIF action name means, ignoring case and surrounding spaces. An
/// unknown name is reported to `errorproc`. `qif-parse.scm:194-239`.
pub fn parse_action_field(read_value: &str, errorproc: impl FnOnce(String)) -> Action {
    let name = read_value.trim().to_lowercase();
    ACTION_MAP.iter().find(|(_, names)| names.contains(&name.as_str())).map_or_else(
        || {
            errorproc(format!("Unrecognized action '{read_value}'."));
            Action::Unrecognized
        },
        |(action, _)| *action,
    )
}

/// A QIF cleared flag. `qif-parse.scm:241-251`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Cleared {
    /// `X` or `R`.
    Reconciled,
    /// `*` or `C`.
    Cleared,
    /// `?` or `!`: "some budget related stuff".
    Budgeted,
    /// Any other flag. As with [`Action::Unrecognized`], GnuCash stores a present value that matches no flag, so
    /// the split ends up uncleared.
    Unrecognized,
}

/// `qif-parse:parse-cleared-field`: the flag the first character of a `C` line means; `None` for an empty line. An
/// unknown flag is reported to `errorproc`. `qif-parse.scm:247-260`.
pub fn parse_cleared_field(read_value: &str, errorproc: impl FnOnce(String)) -> Option<Cleared> {
    let first = read_value.chars().next()?;
    Some(match first {
        'X' | 'x' | 'R' | 'r' => Cleared::Reconciled,
        '*' | 'C' | 'c' => Cleared::Cleared,
        '?' | '!' => Cleared::Budgeted,
        _ => {
            errorproc(format!("The unknown reconciliation status '{read_value}' will be replaced by 'uncleared'."));
            Cleared::Unrecognized
        }
    })
}

/// An order of day, month and year in a QIF date.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DateFormat {
    /// `m-d-y`.
    Mdy,
    /// `d-m-y`.
    Dmy,
    /// `y-m-d`.
    Ymd,
    /// `y-d-m`.
    Ydm,
}

/// The date formats GnuCash tries, in its order. `qif-file.scm:857`.
pub const DATE_FORMATS: [DateFormat; 4] = [DateFormat::Mdy, DateFormat::Dmy, DateFormat::Ymd, DateFormat::Ydm];

impl DateFormat {
    /// GnuCash's symbol for the format, which its date page lists.
    pub fn symbol(self) -> &'static str {
        match self {
            Self::Mdy => "m-d-y",
            Self::Dmy => "d-m-y",
            Self::Ymd => "y-m-d",
            Self::Ydm => "y-d-m",
        }
    }

    /// The format a symbol names.
    pub fn from_symbol(symbol: &str) -> Option<Self> {
        DATE_FORMATS.into_iter().find(|f| f.symbol() == symbol)
    }
}

/// A QIF date as GnuCash parses it: `(day month year)`. The day is only checked to be 1 to 31, whatever the month;
/// GnuCash's `gnc-dmy2time64` normalises it later.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct QifDate {
    /// 1 to 31.
    pub day: u32,
    /// 1 to 12.
    pub month: u32,
    /// The year, after [`fix_year`].
    pub year: i64,
}

/// The three parts of a date matched by [`DATE`], or of an eight-digit date split for `format`.
fn date_parts(date_string: &str, format: Option<DateFormat>) -> Option<[String; 3]> {
    let caps = DATE.as_ref()?.captures(date_string)?;
    let part = |i: usize| caps.get(i).map(|m| m.as_str().to_owned());
    if caps.get(1).is_some() {
        return Some([part(1)?, part(2)?, part(3)?]);
    }
    let digits = part(4)?;
    match format? {
        DateFormat::Dmy | DateFormat::Mdy => split_digits(&digits, 2, 4),
        DateFormat::Ymd | DateFormat::Ydm => split_digits(&digits, 4, 6),
    }
}

/// An eight-digit date split at two positions: `qif-date-mdy-compiled-rexp` (2, 4) and `qif-date-ymd-compiled-rexp`
/// (4, 6). `qif-parse.scm:308-314`.
fn split_digits(digits: &str, first: usize, second: usize) -> Option<[String; 3]> {
    Some([digits.get(..first)?.to_owned(), digits.get(first..second)?.to_owned(), digits.get(second..)?.to_owned()])
}

/// `parse-check-date-format`: the formats of `possible` the three parts of a date could be in. A four-digit year must
/// be after 1930. `qif-parse.scm:263-297`.
fn parse_check_date_format(parts: &[String; 3], possible: &[DateFormat]) -> Vec<DateFormat> {
    let n = parts.clone().map(|p| read_integer(&p));
    let date = |d: Option<i64>, m: Option<i64>, y: Option<i64>, ys: &str| {
        d.is_some_and(|d| (1..=31).contains(&d))
            && m.is_some_and(|m| (1..=12).contains(&m))
            && y.is_some_and(|y| ys.chars().count() != 4 || y > 1930)
    };
    possible
        .iter()
        .copied()
        .filter(|f| match f {
            DateFormat::Dmy => date(n[0], n[1], n[2], &parts[2]),
            DateFormat::Mdy => date(n[1], n[0], n[2], &parts[2]),
            DateFormat::Ymd => date(n[2], n[1], n[0], &parts[0]),
            DateFormat::Ydm => date(n[1], n[2], n[0], &parts[0]),
        })
        .collect()
}

/// `qif-parse:check-date-format`: the formats of `possible` a date string could be in. `None` for an empty or
/// unreadable string: GnuCash returns `#f` or an unspecified value there, which makes `check-and-parse-field` report
/// the field or raise. An eight-digit date is tried as `YYYYxxxx` and as `xxxxYYYY`; GnuCash raises when `possible`
/// lacks the y-first formats (only `reparse-dates` can pass such a list), and here that half is empty instead.
/// `qif-parse.scm:299-335`.
pub fn check_date_format(date_string: &str, possible: &[DateFormat]) -> Option<Vec<DateFormat>> {
    if date_string.is_empty() {
        return None;
    }
    let caps = DATE.as_ref()?.captures(date_string)?;
    if caps.get(1).is_some() {
        return date_parts(date_string, None).map(|parts| parse_check_date_format(&parts, possible));
    }
    let wants = |formats: [DateFormat; 2]| formats.iter().any(|f| possible.contains(f));
    let try_split = |format: DateFormat| {
        date_parts(date_string, Some(format)).map(|parts| parse_check_date_format(&parts, possible)).unwrap_or_default()
    };
    let mut formats = Vec::new();
    if wants([DateFormat::Ydm, DateFormat::Ymd]) {
        formats.extend(try_split(DateFormat::Ymd));
    }
    if wants([DateFormat::Dmy, DateFormat::Mdy]) {
        formats.extend(try_split(DateFormat::Mdy));
    }
    Some(formats)
}

/// `qif-parse:parse-date/format`: the date a string means in `format`; `None` when it is not a date in that format.
/// `qif-parse.scm:337-390`.
pub fn parse_date_format(date_string: &str, format: DateFormat) -> Option<QifDate> {
    let parts = date_parts(date_string, Some(format))?;
    let n = parts.clone().map(|p| read_integer(&p));
    let refs = |dd: usize, mm: usize, yy: usize| {
        let day = n[dd].filter(|d| (1..=31).contains(d))?;
        let month = n[mm].filter(|m| (1..=12).contains(m))?;
        let year = fix_year(&parts[yy], 50)?;
        Some(QifDate { day: u32::try_from(day).ok()?, month: u32::try_from(month).ok()?, year })
    };
    match format {
        DateFormat::Dmy => refs(0, 1, 2),
        DateFormat::Mdy => refs(1, 0, 2),
        DateFormat::Ymd => refs(2, 1, 0),
        DateFormat::Ydm => refs(1, 2, 0),
    }
}

/// A way of writing a number in a QIF file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NumberFormat {
    /// `decimal`: a dot before the decimals, commas or `'` between thousands.
    Decimal,
    /// `comma`: a comma before the decimals, dots or `'` between thousands.
    Comma,
    /// `rational`: an integer or a fraction such as `7/4`.
    Rational,
    /// `integer`: digits only.
    Integer,
}

impl NumberFormat {
    /// GnuCash's symbol for the format.
    pub fn symbol(self) -> &'static str {
        match self {
            Self::Decimal => "decimal",
            Self::Comma => "comma",
            Self::Rational => "rational",
            Self::Integer => "integer",
        }
    }

    fn pattern(self) -> &'static Pattern {
        match self {
            Self::Decimal => &DECIMAL_RADIX,
            Self::Comma => &COMMA_RADIX,
            Self::Rational => &RATIONAL,
            Self::Integer => &INTEGER,
        }
    }
}

/// `qif-parse:check-number-format`: the formats of `possible` a number string could be in. `qif-parse.scm:417-430`.
pub fn check_number_format(value_string: &str, possible: &[NumberFormat]) -> Vec<NumberFormat> {
    possible.iter().copied().filter(|f| is_match(f.pattern(), value_string)).collect()
}

/// A number whose exact value does not fit GnuCash's 64-bit numerator and denominator. Scheme reads it exactly,
/// and GnuCash fails later, when `qif-to-gnc.scm` hands it to the engine (`gnc_scm_to_numeric`,
/// `5.17:bindings/guile/gnc-engine-guile.cpp:1709-1727`); red-ale fails as it parses.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("the number {0:?} does not fit in 64 bits")]
pub struct NumberOverflow(pub String);

/// Scheme's `(string->number (string-append "#e" s))` for the text a number format leaves: digits with at most one
/// dot and an optional exponent (`1e5`), or a fraction of digits. `Ok(None)` where Scheme returns `#f`, for example
/// for an empty string, a lone dot, a zero denominator or any space. The value is reduced, as Scheme's exact numbers
/// are.
pub fn exact_number(s: &str) -> Result<Option<Numeric>, NumberOverflow> {
    let overflow = || NumberOverflow(s.to_owned());
    let parts = match s.split_once('/') {
        Some((n, d)) => exact_fraction(n, d),
        None => match s.split_once(['e', 'E']) {
            Some((mantissa, exponent)) => exact_decimal(mantissa, Some(exponent)),
            None => exact_decimal(s, None),
        },
    };
    match parts {
        Syntax::NotANumber | Syntax::Ratio(_, 0) => Ok(None),
        Syntax::Overflow => Err(overflow()),
        Syntax::Ratio(num, den) => reduce_to_numeric(num, den).map(Some).ok_or_else(overflow),
    }
}

/// What `string->number` makes of a number's text.
enum Syntax {
    NotANumber,
    Overflow,
    Ratio(i128, i128),
}

fn all_digits(t: &str) -> bool {
    t.bytes().all(|b| b.is_ascii_digit())
}

/// `n/d`, both non-empty digits.
fn exact_fraction(n: &str, d: &str) -> Syntax {
    if n.is_empty() || d.is_empty() || !all_digits(n) || !all_digits(d) {
        return Syntax::NotANumber;
    }
    match (parse_digits(n), parse_digits(d)) {
        (Some(n), Some(d)) => Syntax::Ratio(n, d),
        _ => Syntax::Overflow,
    }
}

/// `whole.frac`, either part possibly empty but not both, times ten to the exponent.
fn exact_decimal(mantissa: &str, exponent: Option<&str>) -> Syntax {
    let (whole, frac) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    if (whole.is_empty() && frac.is_empty()) || !all_digits(whole) || !all_digits(frac) {
        return Syntax::NotANumber;
    }
    let exponent = match exponent.map(read_exponent) {
        None => 0,
        Some(Some(e)) => e,
        Some(None) => return Syntax::NotANumber,
    };
    // Trailing zeros of the decimals do not change the value; dropping them keeps more numbers in range.
    let frac = frac.trim_end_matches('0');
    let digits = format!("{whole}{frac}");
    let scale = i64::try_from(frac.len()).unwrap_or(i64::MAX).saturating_sub(exponent);
    let ratio = || {
        let value = parse_digits(&digits)?;
        let power = 10_i128.checked_pow(u32::try_from(scale.unsigned_abs()).ok()?)?;
        if scale >= 0 { Some((value, power)) } else { Some((value.checked_mul(power)?, 1)) }
    };
    ratio().map_or(Syntax::Overflow, |(n, d)| Syntax::Ratio(n, d))
}

/// An exponent: optional sign and digits. One beyond 64 bits only matters as "huge", so it saturates.
fn read_exponent(e: &str) -> Option<i64> {
    let digits = e.strip_prefix(['+', '-']).unwrap_or(e);
    if digits.is_empty() || !all_digits(digits) {
        return None;
    }
    Some(e.parse().unwrap_or(if e.starts_with('-') { i64::MIN / 2 } else { i64::MAX / 2 }))
}

/// Digits as a 128-bit integer; `None` on overflow.
fn parse_digits(digits: &str) -> Option<i128> {
    digits.bytes().try_fold(0_i128, |acc, b| acc.checked_mul(10)?.checked_add(i128::from(b - b'0')))
}

fn gcd(a: i128, b: i128) -> i128 {
    let (mut a, mut b) = (a.abs(), b.abs());
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}

/// `num/den` reduced, as a [`Numeric`] when both fit in 64 bits.
pub(crate) fn reduce_to_numeric(num: i128, den: i128) -> Option<Numeric> {
    let g = gcd(num, den).max(1);
    let sign = if den < 0 { -1 } else { 1 };
    let (num, den) = (num / g * sign, den / g * sign);
    Some(Numeric::new(i64::try_from(num).ok()?, i64::try_from(den).ok()?))
}

/// `qif-parse:parse-number/format`: the value of a number string in `format`. `$`, `'`, `+` and `-` are dropped
/// and any `-` anywhere makes the number negative; the thousands separators of the format are dropped. Text that
/// is still not a number reads as zero, as in GnuCash: this includes a number with a space before or after it.
/// `qif-parse.scm:432-450`.
pub fn parse_number_format(value_string: &str, format: NumberFormat) -> Result<Numeric, NumberOverflow> {
    let has_minus = value_string.contains('-');
    let filtered = string_delete_chars(value_string, "$'+-");
    let read_string = match format {
        NumberFormat::Decimal => string_delete_chars(&filtered, ","),
        NumberFormat::Comma => string_replace_char(&string_delete_chars(&filtered, "."), ',', '.'),
        NumberFormat::Integer | NumberFormat::Rational => filtered,
    };
    let num = exact_number(&read_string)?.unwrap_or(Numeric::ZERO);
    Ok(if has_minus { num.neg() } else { num })
}

/// `qif-parse:check-number-formats`: the formats every present amount could be in. `qif-parse.scm:452-463`.
pub fn check_number_formats(amt_strings: &[Option<&str>], formats: &[NumberFormat]) -> Vec<NumberFormat> {
    amt_strings.iter().flatten().fold(formats.to_vec(), |formats, amt| check_number_format(amt, &formats))
}

/// `qif-parse:parse-numbers/format`: each amount parsed in `format`, a missing one as zero. `qif-parse.scm:465-471`.
pub fn parse_numbers_format(amt_strings: &[Option<&str>], format: NumberFormat) -> Result<Vec<Numeric>, NumberOverflow> {
    amt_strings.iter().map(|amt| amt.map_or(Ok(Numeric::ZERO), |a| parse_number_format(a, format))).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use AccountType::{Asset, Bank, Cash, Credit};
    use DateFormat::{Dmy, Mdy, Ydm, Ymd};
    use NumberFormat::{Comma, Decimal, Integer};

    fn no_error(_: String) {
        panic!("unexpected error")
    }

    #[test]
    fn every_pattern_compiles() {
        [&CATEGORY, &DATE, &DECIMAL_RADIX, &COMMA_RADIX, &RATIONAL, &INTEGER].iter().for_each(|p| assert!(p.is_some()));
    }

    #[test]
    fn fix_year() {
        // test-qif-parse.scm:53-77
        assert_eq!(super::fix_year("1998", 50), Some(1998));
        assert_eq!(super::fix_year("' 0", 50), Some(2000));
        assert_eq!(super::fix_year("98", 50), Some(1998));
        assert_eq!(super::fix_year("48", 50), Some(2048));
        assert_eq!(super::fix_year("19134", 50), Some(2034));
        assert_eq!(super::fix_year("102", 50), Some(2002));
        // qif-parse.scm:149-152: no integer, no year.
        assert_eq!(super::fix_year("abc", 50), None);
        assert_eq!(super::fix_year("'", 50), None);
    }

    #[test]
    fn parse_acct_type() {
        // test-qif-parse.scm:83-94: GNC-CCARD-TYPE is 3, GNC-ASSET-TYPE 2, GNC-BANK-TYPE 0, GNC-CASH-TYPE 1.
        assert_eq!(super::parse_acct_type("ccard", no_error), vec![Credit]);
        assert_eq!(super::parse_acct_type("oth s", no_error), vec![Asset, Bank, Cash]);
        let mut warned = None;
        assert_eq!(super::parse_acct_type("zzz", |m| warned = Some(m)), vec![Bank]);
        assert_eq!(warned.as_deref(), Some("The account type \"zzz\" is unknown, using Bank instead."));
        assert_eq!(super::parse_acct_type(" Oth L ", no_error), vec![AccountType::Liability, Credit]);
    }

    #[test]
    fn parse_cleared_field() {
        // test-qif-parse.scm:100-115
        assert_eq!(super::parse_cleared_field("xx", no_error), Some(Cleared::Reconciled));
        assert_eq!(super::parse_cleared_field("cc", no_error), Some(Cleared::Cleared));
        assert_eq!(super::parse_cleared_field("!!", no_error), Some(Cleared::Budgeted));
        let mut warned = false;
        assert_eq!(super::parse_cleared_field("qq", |_| warned = true), Some(Cleared::Unrecognized));
        assert!(warned);
        assert_eq!(super::parse_cleared_field("", no_error), None);
    }

    #[test]
    fn parse_action_field() {
        // test-qif-parse.scm:121-133
        assert_eq!(super::parse_action_field("BuY", no_error), Action::Buy);
        assert_eq!(super::parse_action_field("WithDrwX", no_error), Action::XOut);
        assert_eq!(super::parse_action_field("k.gewspx", no_error), Action::CgShortX);
        assert_eq!(super::parse_action_field("XIn     ", no_error), Action::XIn);
        let mut warned = None;
        assert_eq!(super::parse_action_field("Grant", |m| warned = Some(m)), Action::Unrecognized);
        assert_eq!(warned.as_deref(), Some("Unrecognized action 'Grant'."));
        assert_eq!(Action::IntIncX.symbol(), Some("intincx"));
    }

    #[test]
    fn check_date_format() {
        // test-qif-parse.scm:139-175
        let check = |s: &str, p: &[DateFormat]| super::check_date_format(s, p);
        assert_eq!(check("20/02/1981", &[Dmy, Ymd, Ydm, Mdy]), Some(vec![Dmy]));
        assert_eq!(check("12/02/1981", &[Dmy, Ymd, Ydm, Mdy]), Some(vec![Dmy, Mdy]));
        assert_eq!(check("1979/03/03", &[Dmy, Ymd, Mdy, Ydm]), Some(vec![Ymd, Ydm]));
        assert_eq!(check("03/03/79", &[Dmy, Ymd, Mdy, Ydm]), Some(vec![Dmy, Mdy]));
        assert_eq!(check("03121984", &[Dmy, Ymd, Mdy, Ydm]), Some(vec![Dmy, Mdy]));
        assert_eq!(check("19790303", &[Dmy, Ymd, Mdy, Ydm]), Some(vec![Ymd, Ydm]));
        // qif-parse.scm:316-319: empty and unreadable dates.
        assert_eq!(check("", &DATE_FORMATS), None);
        assert_eq!(check("tomorrow", &DATE_FORMATS), None);
    }

    #[test]
    fn check_date_format_separators() {
        // qif-parse.scm:304-306: `'` separates the year in Quicken's dates after 1999, and spaces may pad the parts.
        // A one-digit year fits the year-first formats too (qif-parse.scm:285-288).
        assert_eq!(super::check_date_format("1/ 2' 4", &DATE_FORMATS), Some(vec![Mdy, Dmy, Ymd, Ydm]));
        assert_eq!(super::check_date_format("12.31.1999", &DATE_FORMATS), Some(vec![Mdy]));
        // A four-digit year must be after 1930 (qif-parse.scm:273-274).
        assert_eq!(super::check_date_format("1/2/1930", &DATE_FORMATS), Some(vec![]));
    }

    #[test]
    fn parse_date_format() {
        // test-qif-parse.scm:180-188
        assert_eq!(super::parse_date_format("31/01/81", Dmy), Some(QifDate { day: 31, month: 1, year: 1981 }));
        assert_eq!(super::parse_date_format("31/01/81", Mdy), None);
        // Hand-worked from qif-parse.scm:343-390 and fix-year.
        assert_eq!(super::parse_date_format("1/ 2' 4", Mdy), Some(QifDate { day: 2, month: 1, year: 2004 }));
        assert_eq!(super::parse_date_format("03/25/099Share Withdrawal", Mdy), Some(QifDate { day: 25, month: 3, year: 1999 }));
        assert_eq!(super::parse_date_format("19790303", Ydm), Some(QifDate { day: 3, month: 3, year: 1979 }));
        assert_eq!(super::parse_date_format("03121984", Mdy), Some(QifDate { day: 12, month: 3, year: 1984 }));
        assert_eq!(super::parse_date_format("2/31/2001", Mdy), Some(QifDate { day: 31, month: 2, year: 2001 }));
        assert_eq!(super::parse_date_format("nonsense", Mdy), None);
    }

    #[test]
    fn check_number_format() {
        // test-qif-parse.scm:193-213
        let all = [Comma, Integer, Decimal];
        assert_eq!(super::check_number_format("1,00", &all), vec![Comma]);
        assert_eq!(super::check_number_format("999", &all), vec![Comma, Integer, Decimal]);
        assert_eq!(super::check_number_format("999.20", &all), vec![Decimal]);
        assert_eq!(super::check_number_format("9.200,99", &all), vec![Comma]);
        assert_eq!(super::check_number_format("$1000", &all), vec![Comma, Integer, Decimal]);
        assert_eq!(super::check_number_format("7/4", &[Decimal, Comma, NumberFormat::Rational]), vec![NumberFormat::Rational]);
    }

    #[test]
    fn parse_number_format() {
        // test-qif-parse.scm:218-239
        let n = Numeric::new;
        assert_eq!(super::parse_number_format("1,23", Comma), Ok(n(123, 100)));
        assert_eq!(super::parse_number_format("1,234.00", Decimal), Ok(n(1234, 1)));
        assert_eq!(super::parse_number_format("-1234", Integer), Ok(n(-1234, 1)));
        assert_eq!(super::parse_number_format("1234-", Integer), Ok(n(-1234, 1)));
        assert_eq!(super::parse_number_format("1234", Integer), Ok(n(1234, 1)));
    }

    #[test]
    fn parse_number_format_reads_as_scheme_does() {
        // qif-parse.scm:449: `string->number` of "#e" and the text, zero when that is not a number. Checked with
        // Guile 3.0: (string->number "#e1.5 ") is #f.
        let n = Numeric::new;
        assert_eq!(super::parse_number_format("1.50", Decimal).map(Numeric::denom), Ok(2));
        assert_eq!(super::parse_number_format("1.5 ", Decimal), Ok(Numeric::ZERO));
        assert_eq!(super::parse_number_format(".5", Decimal), Ok(n(1, 2)));
        assert_eq!(super::parse_number_format("5.", Decimal), Ok(n(5, 1)));
        assert_eq!(super::parse_number_format(".", Decimal), Ok(Numeric::ZERO));
        assert_eq!(super::parse_number_format("$-1'000.25", Decimal), Ok(n(-4001, 4)));
        assert_eq!(super::parse_number_format("1.000.000,5", Comma), Ok(n(2_000_001, 2)));
        assert_eq!(super::parse_number_format("1/0", NumberFormat::Rational), Ok(Numeric::ZERO));
        assert!(super::parse_number_format("99999999999999999999", Decimal).is_err());
        assert_eq!(super::parse_number_format("1.0000000000000000000000000000000000000000", Decimal), Ok(n(1, 1)));
        assert_eq!(exact_number("1e5"), Ok(Some(n(100_000, 1))));
        assert_eq!(exact_number("2.5E-3"), Ok(Some(n(1, 400))));
        assert_eq!(exact_number("e5"), Ok(None));
    }

    #[test]
    fn check_number_formats() {
        // test-qif-parse.scm:243-262
        let all = [Decimal, Comma, Integer];
        assert_eq!(super::check_number_formats(&[Some("1,00"), Some("2,00"), Some("300,00")], &all), vec![Comma]);
        assert_eq!(super::check_number_formats(&[Some("10.50"), Some("20.54")], &all), vec![Decimal]);
        assert_eq!(super::check_number_formats(&[Some("123,45"), None], &all), vec![Comma]);
        assert_eq!(super::check_number_formats(&[Some("1234"), Some("4567")], &all), vec![Decimal, Comma, Integer]);
    }

    #[test]
    fn parse_numbers_format() {
        // test-qif-parse.scm:264-293
        let ints = |v: &[i64]| v.iter().map(|i| Numeric::from_int(*i)).collect::<Vec<_>>();
        let parse = |a: &[Option<&str>], f| super::parse_numbers_format(a, f).unwrap();
        assert_eq!(parse(&[Some("1,00"), Some("2,00"), Some("300,00")], Comma), ints(&[1, 2, 300]));
        assert_eq!(
            parse(&[Some("1,00"), Some("2,50"), Some("3,99")], Comma),
            vec![Numeric::ONE, Numeric::new(5, 2), Numeric::new(399, 100)]
        );
        assert_eq!(parse(&[Some("1.00"), Some("2.00"), Some("300.00")], Decimal), ints(&[1, 2, 300]));
        assert_eq!(parse(&[Some("1"), Some("2"), Some("300")], Integer), ints(&[1, 2, 300]));
        assert_eq!(parse(&[Some("1"), Some("*"), Some("300")], Integer), ints(&[1, 0, 300]));
        assert_eq!(parse(&[Some("1"), None, Some("300")], Integer), ints(&[1, 0, 300]));
    }

    fn category(name: &str, is_account: bool, class: &str, miscx: Option<(&str, bool, &str)>) -> CategoryInfo {
        CategoryInfo {
            name: name.into(),
            is_account,
            class: class.into(),
            miscx_name: miscx.map(|m| m.0.into()),
            miscx_is_account: miscx.is_some_and(|m| m.1),
            miscx_class: miscx.map(|m| m.2.into()),
        }
    }

    #[test]
    fn parse_category() {
        // test-qif-parse.scm:295-339
        let parse = |s: &str| super::parse_category(s).unwrap();
        assert_eq!(parse("[Transfer]/Class"), category("Transfer", true, "Class", None));
        assert_eq!(parse("Category/Class"), category("Category", false, "Class", None));
        assert_eq!(parse("Category"), category("Category", false, "", None));
        assert_eq!(parse("[Transfer]"), category("Transfer", true, "", None));
        assert_eq!(parse("Category/|miscx-category"), category("Category", false, "", Some(("miscx-category", false, ""))));
        assert_eq!(parse("Category/Class|miscx-category"), category("Category", false, "Class", Some(("miscx-category", false, ""))));
        assert_eq!(parse("[Transfer]/Class|miscx-category"), category("Transfer", true, "Class", Some(("miscx-category", false, ""))));
        assert_eq!(
            parse("[Transfer]/Class|miscx-category/miscx-class"),
            category("Transfer", true, "Class", Some(("miscx-category", false, "miscx-class")))
        );
        assert_eq!(parse("Category/|[miscx-account]"), category("Category", false, "", Some(("miscx-account", true, ""))));
        assert_eq!(
            parse("Category/|miscx-category/miscx-class"),
            category("Category", false, "", Some(("miscx-category", false, "miscx-class")))
        );
        assert_eq!(
            parse("Category/|[miscx-account]/miscx-class"),
            category("Category", false, "", Some(("miscx-account", true, "miscx-class")))
        );
    }

    #[test]
    fn parse_category_keeps_gnucash_quirks() {
        // qif-parse.scm:89-92: only the opening bracket makes an account; text after `]` is the class.
        assert_eq!(super::parse_category("[Checking").unwrap(), category("Checking", true, "", None));
        assert_eq!(super::parse_category("Food]x").unwrap(), category("Food", false, "x", None));
        assert_eq!(super::parse_category("Food:Groceries ").unwrap(), category("Food:Groceries ", false, "", None));
    }

    #[test]
    fn parse_bang_field() {
        // qif-parse.scm:184-192
        assert_eq!(super::parse_bang_field("Type:Bank  "), "type:bank");
        assert_eq!(super::parse_bang_field("type bank"), "type:bank");
        assert_eq!(super::parse_bang_field("Type:Oth A"), "type:oth a");
        assert_eq!(QifSection::from_symbol(&super::parse_bang_field("Option:AutoSwitch")), QifSection::OptionAutoSwitch);
    }

    #[test]
    fn date_format_symbols_round_trip() {
        DATE_FORMATS.iter().for_each(|f| assert_eq!(DateFormat::from_symbol(f.symbol()), Some(*f)));
    }
}
