//! The report log: what a script prints with `print` and `debug`. Standard output is the MCP stdio channel, so
//! nothing a script prints ever reaches it.

use std::cell::RefCell;
use std::rc::Rc;

use crate::result::{LogEntry, LogLevel};

/// The log of one run. It keeps at most `max` bytes of text, the string-size limit, so a script printing in a loop
/// cannot exhaust memory; past that, lines are dropped and one note says so.
#[derive(Debug, Clone)]
pub(crate) struct Log(Rc<RefCell<Entries>>);

#[derive(Debug)]
struct Entries {
    entries: Vec<LogEntry>,
    bytes: usize,
    max: usize,
    truncated: bool,
}

/// The note that ends a log that passed its size.
pub(crate) const TRUNCATED: &str = "the report log passed its size limit; later lines were dropped";

impl Log {
    /// An empty log keeping at most `max` bytes of text.
    pub(crate) fn new(max: usize) -> Self {
        Self(Rc::new(RefCell::new(Entries { entries: Vec::new(), bytes: 0, max, truncated: false })))
    }

    /// Add a line.
    pub(crate) fn push(&self, level: LogLevel, text: &str, source: Option<&str>, line: Option<u32>) {
        let mut log = self.0.borrow_mut();
        if log.truncated {
            return;
        }
        if log.bytes.saturating_add(text.len()) > log.max {
            log.truncated = true;
            log.entries.push(LogEntry { level, text: TRUNCATED.to_owned(), source: None, line: None });
            return;
        }
        log.bytes += text.len();
        log.entries.push(LogEntry { level, text: text.to_owned(), source: source.map(str::to_owned), line });
    }

    /// The lines so far.
    pub(crate) fn entries(&self) -> Vec<LogEntry> {
        self.0.borrow().entries.clone()
    }
}
