//! The journal: GnuCash's `.log` file of transaction changes, and replaying one.
//!
//! Format from `5.17:libgnucash/engine/TransLog.cpp:198-288` (see `design/notes/storage-and-safety.md` §8): a header
//! line, a dashed line, then one `===== START` … `===== END` record per change, one tab-separated line per split.
//! Fields are written raw, as GnuCash writes them; a tab inside a text field cannot be told apart from a separator,
//! so replay refuses such lines instead of guessing.

use std::fs::{File, OpenOptions};
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};

use chrono::DateTime;
use red_ale_core::{Book, Guid, Numeric, Time64, Transaction};

use crate::files::local_timestamp;

/// The header line, without its newline.
pub const HEADER: &str = "mod\ttrans_guid\tsplit_guid\ttime_now\tdate_entered\tdate_posted\tacc_guid\tacc_name\tnum\tdescription\tnotes\tmemo\taction\treconciled\tamount\tvalue\tdate_reconciled";

/// What a record says happened to a transaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordKind {
    /// `B`: the state before an edit.
    Begin,
    /// `C`: the state after a commit.
    Commit,
    /// `R`: the state after a rollback.
    Rollback,
    /// `D`: the transaction as it was deleted.
    Delete,
}

impl RecordKind {
    fn flag(self) -> char {
        match self {
            Self::Begin => 'B',
            Self::Commit => 'C',
            Self::Rollback => 'R',
            Self::Delete => 'D',
        }
    }
}

/// `gnc_time64_to_iso8601_buff`: UTC `YYYY-MM-DD hh:mm:ss`.
fn format_time(t: Time64) -> String {
    DateTime::from_timestamp(t.seconds(), 0).map_or_else(String::new, |dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
}

fn parse_time(s: &str) -> Option<Time64> {
    chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S").ok().map(|dt| Time64(dt.and_utc().timestamp()))
}

/// The lines of one record for a transaction as it stands in `book`.
pub fn record_lines(kind: RecordKind, txn: &Transaction, book: &Book, now: Time64) -> Vec<String> {
    let notes = txn.notes().unwrap_or("");
    txn.splits
        .iter()
        .map(|s| {
            let (acc_guid, acc_name) = s
                .account
                .map_or((String::new(), String::new()), |a| (a.to_string(), book.account(a).map(|acc| acc.name.clone()).unwrap_or_default()));
            [
                kind.flag().to_string(),
                txn.guid.to_string(),
                s.guid.to_string(),
                format_time(now),
                format_time(txn.date_entered),
                format_time(txn.date_posted),
                acc_guid,
                acc_name,
                txn.num.clone().unwrap_or_default(),
                txn.description.clone().unwrap_or_default(),
                notes.to_owned(),
                s.memo.clone().unwrap_or_default(),
                s.action.clone().unwrap_or_default(),
                s.reconcile_state.as_char().to_string(),
                s.amount.to_string(),
                s.value.to_string(),
                format_time(s.reconcile_date.unwrap_or_default()),
            ]
            .join("\t")
        })
        .collect()
}

/// An open journal file. Opened lazily on the first transaction change and replaced by a new one after each save,
/// as GnuCash's GUI does.
#[derive(Debug)]
pub struct Journal {
    book_path: PathBuf,
    file: Option<(PathBuf, BufWriter<File>)>,
}

impl Journal {
    /// A journal for the book at `book_path`; no file is created until something is written.
    pub fn new(book_path: &Path) -> Self {
        Self { book_path: book_path.to_owned(), file: None }
    }

    /// The path of the journal file currently open.
    pub fn current_path(&self) -> Option<&Path> {
        self.file.as_ref().map(|(p, _)| p.as_path())
    }

    fn open(&mut self) -> io::Result<&mut BufWriter<File>> {
        if self.file.is_none() {
            let mut name = self.book_path.as_os_str().to_owned();
            name.push(format!(".{}.log", local_timestamp()));
            let path = PathBuf::from(name);
            let mut writer = BufWriter::new(OpenOptions::new().create(true).append(true).open(&path)?);
            writeln!(writer, "{HEADER}")?;
            writeln!(writer, "-----------------")?;
            writer.flush()?;
            self.file = Some((path, writer));
        }
        match &mut self.file {
            Some((_, writer)) => Ok(writer),
            None => Err(io::Error::other("journal not open")),
        }
    }

    /// Append one record and flush it.
    pub fn write_record(&mut self, lines: &[String]) -> io::Result<()> {
        let writer = self.open()?;
        writeln!(writer, "===== START")?;
        lines.iter().try_for_each(|l| writeln!(writer, "{l}"))?;
        writeln!(writer, "===== END")?;
        writer.flush()
    }

    /// Close the current file so the next change starts a new one.
    pub fn rotate(&mut self) -> io::Result<()> {
        if let Some((_, mut writer)) = self.file.take() {
            writer.flush()?;
        }
        Ok(())
    }
}

/// One split line of a journal, with GnuCash's "empty means absent" rule applied.
#[derive(Debug, Clone, PartialEq)]
pub struct JournalLine {
    /// Record kind.
    pub kind: RecordKind,
    /// Transaction GUID.
    pub trans: Guid,
    /// Split GUID.
    pub split: Option<Guid>,
    /// Date entered.
    pub date_entered: Option<Time64>,
    /// Date posted.
    pub date_posted: Option<Time64>,
    /// Account GUID.
    pub account: Option<Guid>,
    /// Num.
    pub num: Option<String>,
    /// Description.
    pub description: Option<String>,
    /// Notes.
    pub notes: Option<String>,
    /// Memo.
    pub memo: Option<String>,
    /// Action.
    pub action: Option<String>,
    /// Reconcile state character.
    pub reconciled: Option<char>,
    /// Amount.
    pub amount: Option<Numeric>,
    /// Value.
    pub value: Option<Numeric>,
    /// Reconcile date.
    pub date_reconciled: Option<Time64>,
}

/// A parsed record.
#[derive(Debug, Clone, PartialEq)]
pub struct JournalRecord {
    /// 1-based line number of its `===== START`.
    pub start_line: usize,
    /// The record's split lines.
    pub lines: Vec<JournalLine>,
    /// False when some of its lines could not be read; replaying it would apply part of a transaction.
    pub complete: bool,
}

/// A line replay could not use, with why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkippedLine {
    /// 1-based line number in the file.
    pub line: usize,
    /// Why it was skipped.
    pub reason: String,
}

/// Why a journal could not be read at all.
#[derive(Debug, thiserror::Error)]
pub enum JournalError {
    /// The file is empty.
    #[error("the journal is empty")]
    Empty,
    /// The first line is not GnuCash's journal header.
    #[error("the journal header was not recognised")]
    BadHeader,
}

fn opt(s: &str) -> Option<String> {
    (!s.is_empty()).then(|| s.to_owned())
}

fn parse_line(text: &str) -> Result<JournalLine, String> {
    let fields: Vec<&str> = text.split('\t').collect();
    if fields.len() != 17 {
        return Err(format!("{} fields instead of 17 (a text field contains a tab)", fields.len()));
    }
    let kind = match fields[0] {
        "B" => RecordKind::Begin,
        "C" => RecordKind::Commit,
        "R" => RecordKind::Rollback,
        "D" => RecordKind::Delete,
        other => return Err(format!("unknown record flag {other:?}")),
    };
    let guid = |s: &str| if s.is_empty() { Ok(None) } else { Guid::parse(s).map(Some).map_err(|e| e.to_string()) };
    let time = |s: &str| if s.is_empty() { Ok(None) } else { parse_time(s).map(Some).ok_or_else(|| format!("bad time {s:?}")) };
    let num = |s: &str| if s.is_empty() { Ok(None) } else { Numeric::parse(s).map(Some).map_err(|e| e.to_string()) };
    Ok(JournalLine {
        kind,
        trans: guid(fields[1])?.ok_or("missing transaction GUID")?,
        split: guid(fields[2])?,
        date_entered: time(fields[4])?,
        date_posted: time(fields[5])?,
        account: guid(fields[6])?,
        num: opt(fields[8]),
        description: opt(fields[9]),
        notes: opt(fields[10]),
        memo: opt(fields[11]),
        action: opt(fields[12]),
        reconciled: fields[13].chars().next(),
        amount: num(fields[14])?,
        value: num(fields[15])?,
        date_reconciled: time(fields[16])?,
    })
}

/// Parse a journal into records, collecting the lines that could not be used.
pub fn parse(text: &str) -> Result<(Vec<JournalRecord>, Vec<SkippedLine>), JournalError> {
    let mut lines = text.lines().enumerate();
    let (_, first) = lines.next().ok_or(JournalError::Empty)?;
    if !first.starts_with(HEADER) {
        return Err(JournalError::BadHeader);
    }
    let mut records = Vec::new();
    let mut skipped = Vec::new();
    let mut current: Option<JournalRecord> = None;
    for (idx, line) in lines {
        match line {
            "===== START" => current = Some(JournalRecord { start_line: idx + 1, lines: Vec::new(), complete: true }),
            "===== END" => records.extend(current.take()),
            _ if line.starts_with(HEADER) || line.starts_with("-----") => {}
            _ => match (&mut current, parse_line(line)) {
                (Some(rec), Ok(parsed)) => rec.lines.push(parsed),
                (Some(rec), Err(reason)) => {
                    rec.complete = false;
                    skipped.push(SkippedLine { line: idx + 1, reason });
                }
                (None, _) => skipped.push(SkippedLine { line: idx + 1, reason: "outside a record".into() }),
            },
        }
    }
    records.extend(current.take());
    Ok((records, skipped))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "mod\ttrans_guid\tsplit_guid\ttime_now\tdate_entered\tdate_posted\tacc_guid\tacc_name\tnum\tdescription\tnotes\tmemo\taction\treconciled\tamount\tvalue\tdate_reconciled
-----------------
===== START
===== END
===== START
C\t65e21d8a290f4355b9555dfca9bfe888\t4b9a929657604caaa5a5f57b2a4b1917\t2026-10-25 13:04:17\t2026-10-25 13:04:17\t2024-03-15 10:59:00\td031a27171554d279107453116bb1dee\tBank\t101\tGroceries\ta note\tmemo1\t\tn\t-1234/100\t-1234/100\t1970-01-01 00:00:00
C\t65e21d8a290f4355b9555dfca9bfe888\tb7fa927032c94a4ea19da098987e9b42\t2026-10-25 13:04:17\t2026-10-25 13:04:17\t2024-03-15 10:59:00\t48e84d8dc4e24c538e3db22e8d33a5c1\tFood\t101\tGroceries\twith tab\ta note\t\tBuy\tn\t1234/100\t1234/100\t1970-01-01 00:00:00
===== END
";

    #[test]
    fn parses_oracle_journal_and_skips_ambiguous_tabs() {
        let (records, skipped) = parse(SAMPLE).unwrap();
        assert_eq!(records.len(), 2);
        assert!(records[0].lines.is_empty());
        assert_eq!(records[1].lines.len(), 1);
        let line = &records[1].lines[0];
        assert_eq!(line.kind, RecordKind::Commit);
        assert_eq!(line.description.as_deref(), Some("Groceries"));
        assert_eq!(line.action, None);
        assert!(line.amount.unwrap().same_repr(Numeric::new(-1234, 100)));
        assert_eq!(line.date_posted.unwrap(), Time64::parse_file_string("2024-03-15 10:59:00 +0000").unwrap());
        assert_eq!(skipped.len(), 1);
        assert!(skipped[0].reason.contains("18 fields"));
    }

    #[test]
    fn rejects_foreign_files() {
        assert!(matches!(parse(""), Err(JournalError::Empty)));
        assert!(matches!(parse("hello\n"), Err(JournalError::BadHeader)));
    }
}
