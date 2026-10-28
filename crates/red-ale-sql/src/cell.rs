//! Cells and the column readers: one SQLite value at a time into the value the core holds
//! (`design/sql-fidelity.md` §3, "Values").
//!
//! Pure: rows come in as [`Row`]s of [`Cell`]s, so every decoder can be fuzzed without a database. A cell a reader
//! refuses is recorded as a finding naming the table, the `rowid` and the column, and still gives the value GnuCash's
//! reader would leave (the engine's default when it skips the setter), so a read-only session can show the book.

use red_ale_core::{Date, Guid, Numeric, Time64};

/// One SQLite value as stored: its storage class and its content.
#[derive(Debug, Clone, PartialEq)]
pub enum Cell {
    /// SQL NULL.
    Null,
    /// A 64-bit signed integer.
    Integer(i64),
    /// An 8-byte IEEE double, kept as read. Never computed with.
    Real(f64),
    /// Text, as its bytes: SQLite does not guarantee them to be UTF-8.
    Text(Vec<u8>),
    /// A blob.
    Blob(Vec<u8>),
}

impl Cell {
    /// The storage class, as SQLite's `typeof()` names it.
    pub fn class(&self) -> &'static str {
        match self {
            Self::Null => "null",
            Self::Integer(_) => "integer",
            Self::Real(_) => "real",
            Self::Text(_) => "text",
            Self::Blob(_) => "blob",
        }
    }
}

/// One row of a table: its `rowid` and its cells, in the order of the table's columns as [`crate::tables`] lists
/// them.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    /// The row's `rowid`.
    pub rowid: i64,
    /// The cells, one per column.
    pub cells: Vec<Cell>,
}

/// A table red-ale reads: its name and the columns it selects, in order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Table {
    /// The table's name.
    pub name: &'static str,
    /// The columns, in the order the row's cells hold them.
    pub columns: &'static [&'static str],
}

/// Reads the cells of one row, recording each refusal.
pub(crate) struct Cells<'a> {
    table: &'a Table,
    row: &'a Row,
    findings: &'a mut Vec<String>,
}

const NULL: Cell = Cell::Null;

impl<'a> Cells<'a> {
    pub(crate) fn new(table: &'a Table, row: &'a Row, findings: &'a mut Vec<String>) -> Self {
        Self { table, row, findings }
    }

    /// The row's `rowid`.
    pub(crate) fn rowid(&self) -> i64 {
        self.row.rowid
    }

    /// Record a finding about the whole row.
    pub(crate) fn row_finding(&mut self, what: impl std::fmt::Display) {
        let (table, rowid) = (self.table.name, self.row.rowid);
        self.findings.push(format!("{table} row {rowid}: {what}"));
    }

    /// Record a finding about one column.
    pub(crate) fn refuse(&mut self, column: &str, what: impl std::fmt::Display) {
        let (table, rowid) = (self.table.name, self.row.rowid);
        self.findings.push(format!("{table} row {rowid}, column {column}: {what}"));
    }

    fn cell(&self, column: &str) -> &'a Cell {
        let row: &'a Row = self.row;
        self.table
            .columns
            .iter()
            .position(|c| *c == column)
            .and_then(|i| row.cells.get(i))
            .unwrap_or(&NULL)
    }

    fn wrong_class(&mut self, column: &str, cell: &Cell, wanted: &str) {
        self.refuse(column, format!("{} where GnuCash writes {wanted}", cell.class()));
    }

    /// A text column; NULL is `None`. Invalid UTF-8 is a finding, read lossily.
    pub(crate) fn text(&mut self, column: &str) -> Option<String> {
        match self.cell(column) {
            Cell::Null => None,
            Cell::Text(bytes) => Some(String::from_utf8(bytes.clone()).unwrap_or_else(|e| {
                self.refuse(column, "text that is not valid UTF-8");
                String::from_utf8_lossy(e.as_bytes()).into_owned()
            })),
            other => {
                self.wrong_class(column, other, "text");
                None
            }
        }
    }

    /// A text column GnuCash always fills; NULL is a finding and reads as empty.
    pub(crate) fn req_text(&mut self, column: &str) -> String {
        self.text(column).unwrap_or_else(|| {
            if self.cell(column) == &Cell::Null {
                self.refuse(column, "NULL in a column GnuCash always fills");
            }
            String::new()
        })
    }

    /// A text column read as GnuCash's XML writer would leave it: NULL and empty are `None`.
    pub(crate) fn nonempty_text(&mut self, column: &str) -> Option<String> {
        self.text(column).filter(|s| !s.is_empty())
    }

    /// An integer column; NULL is `None`.
    pub(crate) fn int(&mut self, column: &str) -> Option<i64> {
        match self.cell(column) {
            Cell::Null => None,
            Cell::Integer(i) => Some(*i),
            other => {
                self.wrong_class(column, other, "an integer");
                None
            }
        }
    }

    /// An integer column GnuCash always fills; NULL is a finding and reads as 0.
    pub(crate) fn req_int(&mut self, column: &str) -> i64 {
        if self.cell(column) == &Cell::Null {
            self.refuse(column, "NULL in a column GnuCash always fills");
        }
        self.int(column).unwrap_or(0)
    }

    /// An integer the engine keeps in a narrower type `T`; out of range is a finding and reads as `T`'s default.
    pub(crate) fn narrow<T: TryFrom<i64> + Default>(&mut self, column: &str) -> T {
        let value = self.req_int(column);
        T::try_from(value).unwrap_or_else(|_| {
            self.refuse(column, format!("{value} is out of the engine's range"));
            T::default()
        })
    }

    /// A boolean column: 0 or 1, NULL as `None`.
    pub(crate) fn bool(&mut self, column: &str) -> Option<bool> {
        match self.int(column)? {
            0 => Some(false),
            1 => Some(true),
            other => {
                self.refuse(column, format!("{other} is not a boolean"));
                Some(other != 0)
            }
        }
    }

    /// A boolean column GnuCash always fills.
    pub(crate) fn req_bool(&mut self, column: &str) -> bool {
        if self.cell(column) == &Cell::Null {
            self.refuse(column, "NULL in a column GnuCash always fills");
        }
        self.bool(column).unwrap_or(false)
    }

    /// A GUID or reference column: 32 lower-case hex digits, as `guid_to_string` writes them. NULL is `None`;
    /// anything else is a finding and reads as `None`, as GnuCash's reader skips it.
    pub(crate) fn guid(&mut self, column: &str) -> Option<Guid> {
        let text = self.text(column)?;
        parse_guid(&text).or_else(|| {
            self.refuse(column, format!("{text:?} is not a GUID as GnuCash writes one"));
            None
        })
    }

    /// A numeric stored as `<prefix>_num` and `<prefix>_denom`, unreduced; `None` when both are NULL.
    pub(crate) fn numeric(&mut self, prefix: &str) -> Option<Numeric> {
        self.numeric_halves(prefix).ok().flatten()
    }

    /// Both halves of a numeric: `Ok(None)` when both are NULL, `Err` once a refusal is recorded.
    fn numeric_halves(&mut self, prefix: &str) -> Result<Option<Numeric>, ()> {
        let (num_col, denom_col) = (format!("{prefix}_num"), format!("{prefix}_denom"));
        let (num, denom) = (self.cell(&num_col), self.cell(&denom_col));
        match (num, denom) {
            (Cell::Integer(n), Cell::Integer(d)) => Ok(Some(Numeric::new(*n, *d))),
            (Cell::Null, Cell::Null) => Ok(None),
            (Cell::Integer(_) | Cell::Null, Cell::Integer(_) | Cell::Null) => {
                self.refuse(prefix, "a numeric with only one half");
                Err(())
            }
            _ => {
                [(num_col, num), (denom_col, denom)]
                    .into_iter()
                    .filter(|(_, c)| !matches!(c, Cell::Integer(_) | Cell::Null))
                    .for_each(|(col, c)| self.wrong_class(&col, c, "an integer"));
                Err(())
            }
        }
    }

    /// A numeric GnuCash always fills; missing is a finding and reads as zero.
    pub(crate) fn req_numeric(&mut self, prefix: &str) -> Numeric {
        match self.numeric_halves(prefix) {
            Ok(Some(n)) => n,
            Ok(None) => {
                self.refuse(prefix, "NULL in a column GnuCash always fills");
                Numeric::ZERO
            }
            Err(()) => Numeric::ZERO,
        }
    }

    /// A numeric GnuCash's XML writer leaves out when zero, so the XML reader gives `0/1`: zero and NULL read as
    /// [`Numeric::ZERO`].
    pub(crate) fn numeric_or_zero(&mut self, prefix: &str) -> Numeric {
        self.numeric(prefix).filter(|n| !n.is_zero()).unwrap_or(Numeric::ZERO)
    }

    /// A timestamp, NULL or empty as time 0, as GnuCash reads them.
    pub(crate) fn time(&mut self, column: &str) -> Time64 {
        self.opt_time(column).unwrap_or_default()
    }

    /// A timestamp whose NULL means "none": GnuCash writes NULL for a time outside its range, which is how it stores
    /// the `INT64_MAX` of an unposted invoice or an open order.
    pub(crate) fn opt_time(&mut self, column: &str) -> Option<Time64> {
        let text = self.text(column)?;
        if text.is_empty() {
            return Some(Time64(0));
        }
        parse_timestamp(&text).or_else(|| {
            self.refuse(column, format!("{text:?} is not a timestamp in either form GnuCash writes"));
            Some(Time64(0))
        })
    }

    /// A date: `YYYYMMDD`; NULL, empty and `00000000` are an unset date.
    pub(crate) fn date(&mut self, column: &str) -> Option<Date> {
        let text = self.text(column)?;
        parse_date(&text).unwrap_or_else(|()| {
            self.refuse(column, format!("{text:?} is not a date as GnuCash writes one"));
            None
        })
    }

    /// A double column: a REAL, NULL as `None`.
    pub(crate) fn real(&mut self, column: &str) -> Option<f64> {
        match self.cell(column) {
            Cell::Null => None,
            Cell::Real(r) => Some(*r),
            other => {
                self.wrong_class(column, other, "a real");
                None
            }
        }
    }
}

/// 32 lower-case hex digits, the only form `guid_to_string` writes.
pub fn parse_guid(text: &str) -> Option<Guid> {
    let well_formed = text.len() == 32 && text.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
    if well_formed { Guid::parse(text).ok() } else { None }
}

/// The digits of `text[range]` as a number, if they are all ASCII digits.
fn digits(text: &[u8], start: usize, len: usize) -> Option<i64> {
    text.get(start..start + len)
        .filter(|d| d.iter().all(u8::is_ascii_digit))
        .map(|d| d.iter().fold(0_i64, |n, b| n * 10 + i64::from(b - b'0')))
}

/// A timestamp in either form GnuCash 5.17 writes or has written, both UTC: `YYYY-MM-DD HH:MM:SS`
/// (`5.17:libgnucash/engine/gnc-datetime.cpp:630-636`) or the older `YYYYMMDDHHMMSS`. Every other form is refused,
/// even those GnuCash's parser accepts (`design/notes/sql-backend.md` §3.4).
pub fn parse_timestamp(text: &str) -> Option<Time64> {
    let b = text.as_bytes();
    let fields = match b.len() {
        19 if b[4] == b'-' && b[7] == b'-' && b[10] == b' ' && b[13] == b':' && b[16] == b':' => [0, 5, 8, 11, 14, 17],
        14 => [0, 4, 6, 8, 10, 12],
        _ => return None,
    };
    let year = digits(b, fields[0], 4)?;
    let [month, day, hour, minute, second] = [1, 2, 3, 4, 5].map(|i| digits(b, fields[i], 2));
    let (month, day, hour, minute, second) = (month?, day?, hour?, minute?, second?);
    if hour > 23 || minute > 59 || second > 59 {
        return None;
    }
    let date = Date::from_ymd(i32::try_from(year).ok()?, u32::try_from(month).ok()?, u32::try_from(day).ok()?)?;
    Some(date.start_utc().plus_seconds(hour * 3600 + minute * 60 + second))
}

/// A date as GnuCash writes one, `YYYYMMDD`: `Ok(None)` for the empty text and `00000000`, which GnuCash reads as an
/// unset date (`5.17:libgnucash/backend/sql/gnc-sql-column-table-entry.cpp:461-503`).
pub(crate) fn parse_date(text: &str) -> Result<Option<Date>, ()> {
    if text.is_empty() || text == "00000000" {
        return Ok(None);
    }
    let b = text.as_bytes();
    if b.len() != 8 {
        return Err(());
    }
    let (year, month, day) = (digits(b, 0, 4).ok_or(())?, digits(b, 4, 2).ok_or(())?, digits(b, 6, 2).ok_or(())?);
    let ymd = (i32::try_from(year), u32::try_from(month), u32::try_from(day));
    let (Ok(year), Ok(month), Ok(day)) = ymd else { return Err(()) };
    Date::from_ymd(year, month, day).map(Some).ok_or(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_timestamp_forms_read_as_utc() {
        let iso = parse_timestamp("2024-06-03 10:59:00").unwrap();
        assert_eq!(iso, Time64(1_717_412_340));
        assert_eq!(parse_timestamp("20240603105900"), Some(iso));
        assert_eq!(parse_timestamp("1970-01-01 00:00:00"), Some(Time64(0)));
    }

    #[test]
    fn other_timestamp_forms_are_refused() {
        [
            "2024-06-03 10:59:00 +0000",
            "2024-06-03 10:59:00.5",
            "2024-06-03T10:59:00",
            "2024-6-03 10:59:00",
            "20240603105900.0",
            "2024-02-30 10:59:00",
            "2024-06-03 24:00:00",
            "garbage",
        ]
        .iter()
        .for_each(|t| assert_eq!(parse_timestamp(t), None, "{t}"));
    }

    #[test]
    fn dates_read_unset_forms_and_refuse_others() {
        assert_eq!(parse_date("20240229"), Ok(Date::from_ymd(2024, 2, 29)));
        assert_eq!(parse_date(""), Ok(None));
        assert_eq!(parse_date("00000000"), Ok(None));
        ["2024-02-29", "20230229", "2024022", "2024a229"].iter().for_each(|d| assert_eq!(parse_date(d), Err(()), "{d}"));
    }

    #[test]
    fn guids_must_be_lower_case_hex() {
        assert!(parse_guid("0123456789abcdef0123456789abcdef").is_some());
        assert!(parse_guid("0123456789ABCDEF0123456789abcdef").is_none());
        assert!(parse_guid("0123456789abcdef0123456789abcde").is_none());
        assert!(parse_guid(" 123456789abcdef0123456789abcdef").is_none());
    }

    const T: Table = Table {
        name: "t",
        columns: &["a", "b_num", "b_denom", "c"],
    };

    #[test]
    fn refusals_name_table_row_and_column_and_leave_a_value() {
        let row = Row {
            rowid: 7,
            cells: vec![Cell::Real(1.5), Cell::Integer(1), Cell::Null, Cell::Text(vec![0xff])],
        };
        let mut findings = Vec::new();
        let mut cells = Cells::new(&T, &row, &mut findings);
        assert_eq!(cells.int("a"), None);
        assert_eq!(cells.numeric("b"), None);
        assert_eq!(cells.text("c").as_deref(), Some("\u{fffd}"));
        assert_eq!(cells.time("c"), Time64(0));
        assert_eq!(
            findings,
            [
                "t row 7, column a: real where GnuCash writes an integer",
                "t row 7, column b: a numeric with only one half",
                "t row 7, column c: text that is not valid UTF-8",
                "t row 7, column c: text that is not valid UTF-8",
                "t row 7, column c: \"\u{fffd}\" is not a timestamp in either form GnuCash writes",
            ]
        );
    }
}
