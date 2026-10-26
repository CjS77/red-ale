//! Reading a QIF file into records, then parsing their fields.
//!
//! Translated from `5.17:gnucash/import-export/qif-imp/qif-file.scm` ("Read a QIF file into a <qif-file>
//! object"), Bill Gribble <grib@billgribble.com> 20 Feb 2000. GPL-2.0-or-later; this translation is
//! GPL-3.0-or-later.
//!
//! As in GnuCash, an import reads a file in two steps:
//!
//! 1. [`read_file`] (`qif-file:read-file`) splits the text into records and keeps every field as text, apart from
//!    category lines, which it splits at once.
//! 2. [`parse_fields`] (`qif-file:parse-fields`) finds, for each kind of field across the whole file, the date or
//!    number formats every value fits, and parses the values in place. When several date formats fit and they read
//!    some date differently, the dates stay unparsed and the formats come back in the report
//!    ([`ParseFieldsReport::date_formats`]); the caller picks one (GnuCash asks the user) and calls
//!    [`reparse_dates`].
//!
//! The progress dialog, pause and cancel of GnuCash's functions, and of `qif-utils.scm`, have no counterpart: the
//! reader is a pure function. GnuCash's log lines become the warnings and errors returned.

use std::fmt;

use red_ale_core::{AccountType, Numeric};
use thiserror::Error;

use super::numeric_string::gnc_numeric_from_string;
use super::objects::{Field, QifAcct, QifCat, QifClass, QifFile, QifPrice, QifSplit, QifStockSymbol, QifXtn};
use super::parse::{
    self, Action, CategoryParseError, Cleared, DATE_FORMATS, DateFormat, NumberFormat, NumberOverflow, QifDate, QifSection,
};

/// What [`read_file`] needs besides the text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadOptions {
    /// The file's path or name as the user gave it, kept in [`QifFile::path`] for
    /// [`QifFile::path_to_accountname`].
    pub path: Option<String>,
    /// The full name of the equity account an opening balance comes from: GnuCash's `default-equity-account`,
    /// "Equity" and "Retained Earnings" (translated) joined by the account separator (`qif-dialog-utils.scm:112-114`).
    pub default_equity_account: String,
}

impl Default for ReadOptions {
    fn default() -> Self {
        Self { path: None, default_equity_account: "Equity:Retained Earnings".to_owned() }
    }
}

/// A file read into records: what `qif-file:read-file` leaves in its `<qif-file>` and ticker map.
#[derive(Debug, Clone, PartialEq)]
pub struct ReadResult {
    /// The records.
    pub file: QifFile,
    /// The `!Type:Security` records in file order. GnuCash adds them to the ticker map it shares across every
    /// file of an import; the caller adds them to its [`TickerMap`](super::objects::TickerMap) in this order.
    pub securities: Vec<QifStockSymbol>,
    /// Every warning, in order. GnuCash logs each one and returns only the last (its FIXME at
    /// `qif-file.scm:75-76` asks for all of them).
    pub warnings: Vec<ReadWarning>,
}

/// A line `read_file` skipped or changed, with its line number.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("Line {line}: {kind}")]
pub struct ReadWarning {
    /// The line, counting a CR LF pair as one line break.
    pub line: usize,
    /// What happened.
    pub kind: ReadWarningKind,
}

/// The warnings of `qif-file:read-file`, worded as GnuCash words them. `qif-file.scm:248-249`, `:386-387`, `:419`,
/// `:487`, `:518`, `:532`.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ReadWarningKind {
    /// An `!Option:` header GnuCash does not know; the section before it continues.
    #[error("Ignoring unknown option '{0}'")]
    UnknownOption(String),
    /// A transaction without a `D` line, which is dropped.
    #[error("Date required. Discarding this transaction.")]
    DateRequired,
    /// A line of a class record with an unknown field.
    #[error("Ignoring class line: {0}")]
    IgnoringClassLine(String),
    /// A line of a category record with an unknown field.
    #[error("Ignoring category line: {0}")]
    IgnoringCategoryLine(String),
    /// A line of a security record with an unknown field.
    #[error("Ignoring security line: {0}")]
    IgnoringSecurityLine(String),
    /// A price line that is not `"SYMBOL",value,"DATE"`.
    #[error("Could not parse price line: {0}")]
    UnparsablePriceLine(String),
}

/// Why `read_file` gave up on a file.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ReadError {
    /// A line outside any `!` section: GnuCash's "File does not appear to be in QIF format" (`qif-file.scm:535-541`).
    #[error("Line {line}: File does not appear to be in QIF format: {text}")]
    NotQif {
        /// The line.
        line: usize,
        /// Its text.
        text: String,
    },
    /// A category line the category pattern does not match; GnuCash throws here (`qif-parse.scm:104-108`).
    #[error("Line {line}: {source}")]
    Category {
        /// The line.
        line: usize,
        /// The category error.
        source: CategoryParseError,
    },
}

/// `qif-bad-numeric-rexp`: an amount starting `...` is skipped. `qif-file.scm:55-59`.
fn not_bad_numeric_string(input: &str) -> bool {
    !input.starts_with("...")
}

/// The lines of `text` with their numbers, empty lines left out. GnuCash reads up to each CR or LF
/// (`read-delimited`) and skips the empty strings between them (`qif-file.scm:162-166`, `:559-562`); its line count
/// counts both characters of a CR LF, where this counts the pair once.
fn numbered_lines(text: &str) -> impl Iterator<Item = (usize, &str)> {
    text.split_inclusive(['\r', '\n'])
        .scan((1_usize, false), |(next, after_cr), piece| {
            let crlf_tail = *after_cr && piece == "\n";
            *after_cr = piece.ends_with('\r');
            let number = *next;
            if !crlf_tail {
                *next += 1;
            }
            Some((number, crlf_tail, piece.trim_end_matches(['\r', '\n'])))
        })
        .filter(|(_, crlf_tail, line)| !crlf_tail && !line.is_empty())
        .map(|(number, _, line)| (number, line))
}

/// `qif-file:read-file`: read the text of a QIF file into records, keeping every field as text. A UTF-8 byte order
/// mark at the start is dropped. `qif-file.scm:62-582`.
///
/// GnuCash reads the file as UTF-8 and, for a line that is not, tries the locale's character set and then drops
/// the bytes it cannot read, with a warning (`qif-file.scm:176-192`). Here the text is already decoded: the caller
/// converts the file's bytes with the encoding the user chose.
pub fn read_file(text: &str, options: &ReadOptions) -> Result<ReadResult, ReadError> {
    let mut reader = Reader::new(options);
    reader.file.path.clone_from(&options.path);
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    numbered_lines(text).try_for_each(|(number, line)| {
        reader.line_num = number;
        reader.read_line(line)
    })?;
    Ok(ReadResult { file: reader.file, securities: reader.securities, warnings: reader.warnings })
}

/// The state of `private-read`. `qif-file.scm:82-100`.
///
/// GnuCash keeps one `current-xtn` whose kind follows the section; here each kind has its own slot. Every header
/// that switches GnuCash's `current-xtn` resets the slot of its kind, and only that kind's lines reach a slot, so
/// both keep the same records.
struct Reader<'a> {
    options: &'a ReadOptions,
    file: QifFile,
    securities: Vec<QifStockSymbol>,
    warnings: Vec<ReadWarning>,
    line_num: usize,
    qstate: Option<QifSection>,
    xtn: QifXtn,
    /// Whether `xtn.splits[0]` is GnuCash's `current-split`.
    has_current_split: bool,
    default_split: Option<QifSplit>,
    current_account_name: Option<String>,
    last_seen_account_name: Option<String>,
    first_xtn: bool,
    ignore_accounts: bool,
    class: QifClass,
    cat: QifCat,
    acct: QifAcct,
    security: QifStockSymbol,
}

/// Note the line a record starts on, the first time one of its fields is read.
fn touch(record_line: &mut usize, line: usize) {
    if *record_line == 0 {
        *record_line = line;
    }
}

impl<'a> Reader<'a> {
    fn new(options: &'a ReadOptions) -> Self {
        Self {
            options,
            file: QifFile::default(),
            securities: Vec::new(),
            warnings: Vec::new(),
            line_num: 0,
            qstate: None,
            xtn: QifXtn::new(0),
            has_current_split: false,
            default_split: None,
            current_account_name: None,
            last_seen_account_name: None,
            first_xtn: false,
            ignore_accounts: false,
            class: QifClass::default(),
            cat: QifCat::default(),
            acct: QifAcct::new(0),
            security: QifStockSymbol::default(),
        }
    }

    /// `mywarn`. `qif-file.scm:102-107`.
    fn warn(&mut self, kind: ReadWarningKind) {
        self.warnings.push(ReadWarning { line: self.line_num, kind });
    }

    /// One line: the one-character tag, then the value. `qif-file.scm:172-174`, `:194-541`.
    fn read_line(&mut self, line: &str) -> Result<(), ReadError> {
        let mut chars = line.chars();
        let Some(tag) = chars.next() else { return Ok(()) };
        let value = chars.as_str();
        if tag == '!' {
            self.bang(value);
            return Ok(());
        }
        match self.qstate.clone() {
            Some(section) if section.is_transactions() => self.xtn_line(tag, value, &section)?,
            Some(QifSection::Class) => self.class_line(tag, value, line),
            Some(QifSection::Account) => self.account_line(tag, value),
            Some(QifSection::Cat) => self.cat_line(tag, value, line),
            Some(QifSection::Security) => self.security_line(tag, value, line),
            Some(QifSection::Prices) => self.price_line(tag, line),
            // Lines of memorised transactions and of unknown sections are skipped. `qif-file.scm:227-230`.
            Some(_) => {}
            None if line.trim().is_empty() => {}
            None => return Err(ReadError::NotQif { line: self.line_num, text: line.to_owned() }),
        }
        Ok(())
    }

    /// A `!` header switches the section. `qif-file.scm:194-250`.
    fn bang(&mut self, value: &str) {
        let section = QifSection::from_symbol(&parse::parse_bang_field(value));
        let old_qstate = self.qstate.replace(section.clone());
        match section {
            s if s.is_transactions() => {
                if self.ignore_accounts {
                    self.current_account_name.clone_from(&self.last_seen_account_name);
                }
                self.ignore_accounts = false;
                self.xtn = QifXtn::new(0);
                self.has_current_split = false;
                self.default_split = Some(QifSplit::new());
                self.first_xtn = true;
            }
            QifSection::Class => self.class = QifClass::default(),
            QifSection::Cat => self.cat = QifCat::default(),
            QifSection::Account => self.acct = QifAcct::new(0),
            QifSection::Security => self.security = QifStockSymbol::default(),
            QifSection::OptionAutoSwitch => self.ignore_accounts = true,
            QifSection::ClearAutoSwitch => self.ignore_accounts = false,
            QifSection::Other(symbol) if symbol.starts_with("option:") => {
                self.warn(ReadWarningKind::UnknownOption(symbol));
                self.qstate = old_qstate;
            }
            _ => {}
        }
    }

    /// A line of a transaction list. `qif-file.scm:257-392`.
    fn xtn_line(&mut self, tag: char, value: &str, section: &QifSection) -> Result<(), ReadError> {
        if tag != '^' {
            touch(&mut self.xtn.line, self.line_num);
        }
        let text = || Some(value.to_owned());
        match tag {
            'D' => self.xtn.date = Some(Field::text_of(value)),
            'T' => self.set_default_amount(value, false),
            // Quicken 2005 writes larger totals in U (bug 798085).
            'U' => self.set_default_amount(value, true),
            'P' => self.xtn.payee = text(),
            'A' => self.xtn.address = Some(format!("{}\n{value}", self.xtn.address.as_deref().unwrap_or(""))),
            'N' if section.is_investment() => self.xtn.action = Some(Field::text_of(value)),
            'N' => self.xtn.number = text(),
            'C' => self.xtn.cleared = Some(Field::text_of(value)),
            'M' => self.default_split.iter_mut().for_each(|s| s.memo = text()),
            'I' => self.xtn.share_price = Some(Field::text_of(value)),
            'Q' => self.xtn.num_shares = Some(Field::text_of(value)),
            'Y' => self.xtn.security_name = text(),
            'O' => self.xtn.commission = Some(Field::text_of(value)),
            'L' => self.set_default_category(value)?,
            'S' => self.start_split(value)?,
            'E' => self.current_split().into_iter().for_each(|s| s.memo = text()),
            '$' if not_bad_numeric_string(value) => self.current_split().into_iter().for_each(|s| s.amount = Some(Field::text_of(value))),
            '^' => self.end_xtn(section),
            _ => {}
        }
        Ok(())
    }

    /// `qif-split-set-amount` on the transaction's own split: `U` overrides, `T` only fills. `qif-file.scm:139-143`.
    fn set_default_amount(&mut self, value: &str, override_amount: bool) {
        if let Some(split) = self.default_split.as_mut().filter(|s| override_amount || s.amount.is_none())
            && not_bad_numeric_string(value)
        {
            split.amount = Some(Field::text_of(value));
        }
    }

    fn set_default_category(&mut self, value: &str) -> Result<(), ReadError> {
        let line = self.line_num;
        self.default_split
            .as_mut()
            .map_or(Ok(()), |s| s.set_category(value))
            .map_err(|source| ReadError::Category { line, source })
    }

    /// `S` starts a split. From here on the transaction's own split is kept aside, so later `T`, `M` and `L` lines
    /// of the record are ignored. `qif-file.scm:333-347`.
    fn start_split(&mut self, value: &str) -> Result<(), ReadError> {
        let mut split = QifSplit::new();
        if let Some(default) = self.default_split.take() {
            self.xtn.default_split = Some(default);
        }
        split.set_category(value).map_err(|source| ReadError::Category { line: self.line_num, source })?;
        self.xtn.splits.insert(0, split);
        self.has_current_split = true;
        Ok(())
    }

    fn current_split(&mut self) -> Option<&mut QifSplit> {
        if self.has_current_split { self.xtn.splits.first_mut() } else { None }
    }

    /// `^` ends a transaction. `qif-file.scm:360-392`.
    fn end_xtn(&mut self, section: &QifSection) {
        let mut xtn = std::mem::replace(&mut self.xtn, QifXtn::new(0));
        if xtn.splits.is_empty() {
            xtn.splits = vec![self.default_split.take().unwrap_or_default()];
        }
        if self.first_xtn {
            let account = process_opening_balance_xtn(self.current_account_name.as_deref(), &mut xtn, self.options);
            if self.current_account_name.is_none() {
                self.current_account_name = account;
            }
            self.first_xtn = false;
        }
        if section.is_investment() && xtn.security_name.is_none() {
            xtn.security_name = Some(String::new());
        }
        xtn.from_acct.clone_from(&self.current_account_name);
        xtn.last_line = self.line_num;
        if xtn.date.is_some() {
            self.file.add_xtn(xtn);
        } else {
            self.warn(ReadWarningKind::DateRequired);
        }
        self.has_current_split = false;
        self.default_split = Some(QifSplit::new());
    }

    /// A line of a class list. `qif-file.scm:395-419`.
    fn class_line(&mut self, tag: char, value: &str, line: &str) {
        match tag {
            'N' => self.class.name = Some(value.to_owned()),
            'D' => self.class.description = Some(value.to_owned()),
            // The tax copy designator is ignored.
            'R' => {}
            '^' => {
                let class = std::mem::take(&mut self.class);
                self.file.add_class(class);
                return;
            }
            _ => return self.warn(ReadWarningKind::IgnoringClassLine(line.to_owned())),
        }
        touch(&mut self.class.line, self.line_num);
    }

    /// A line of an account list. Outside an auto-switch block, an account's `^` makes it the account the following
    /// transactions belong to. `qif-file.scm:422-444`.
    fn account_line(&mut self, tag: char, value: &str) {
        match tag {
            'N' => {
                value.clone_into(&mut self.acct.name);
                self.last_seen_account_name = Some(value.to_owned());
            }
            'D' => self.acct.description = Some(value.to_owned()),
            'T' => self.acct.acct_type = Field::text_of(value),
            'L' => self.acct.limit = Some(Field::text_of(value)),
            'B' => self.acct.budget = Some(Field::text_of(value)),
            '^' => {
                if !self.ignore_accounts {
                    self.current_account_name = Some(self.acct.name.clone());
                }
                let acct = std::mem::replace(&mut self.acct, QifAcct::new(0));
                self.file.add_account(acct);
                return;
            }
            _ => return,
        }
        touch(&mut self.acct.line, self.line_num);
    }

    /// A line of a category list. `qif-file.scm:447-487`.
    fn cat_line(&mut self, tag: char, value: &str, line: &str) {
        match tag {
            'N' => self.cat.name = Some(value.to_owned()),
            'D' => self.cat.description = Some(value.to_owned()),
            'T' => self.cat.taxable = true,
            'E' => self.cat.expense_cat = true,
            'I' => self.cat.income_cat = true,
            'R' => self.cat.tax_class = Some(Field::text_of(value)),
            'B' => self.cat.budget_amt = Some(Field::text_of(value)),
            '^' => {
                let cat = std::mem::take(&mut self.cat);
                self.file.add_cat(cat);
                return;
            }
            _ => return self.warn(ReadWarningKind::IgnoringCategoryLine(line.to_owned())),
        }
        touch(&mut self.cat.line, self.line_num);
    }

    /// A line of a security list. `qif-file.scm:490-518`.
    fn security_line(&mut self, tag: char, value: &str, line: &str) {
        match tag {
            'N' => value.clone_into(&mut self.security.name),
            'S' => value.clone_into(&mut self.security.symbol),
            'T' => value.clone_into(&mut self.security.security_type),
            // The asset class is ignored.
            'G' => {}
            '^' => self.securities.push(std::mem::take(&mut self.security)),
            _ => self.warn(ReadWarningKind::IgnoringSecurityLine(line.to_owned())),
        }
    }

    /// A line of a price list: every line but `^` is a price. `qif-file.scm:520-533`.
    fn price_line(&mut self, tag: char, line: &str) {
        if tag == '^' {
            return;
        }
        match parse_price_line(line, self.line_num) {
            Some(price) => self.file.add_price(price),
            None => self.warn(ReadWarningKind::UnparsablePriceLine(line.to_owned())),
        }
    }
}

/// `qif-file:process-opening-balance-xtn`, for the first transaction after a `!Type:` header. Without a known
/// account, a transaction to `[account]` with payee "Opening Balance" names the account the following transactions
/// belong to (Microsoft Money writes these instead of account records); with a known account, a transfer to that
/// same account is an opening balance whatever the payee. Either way the transfer becomes one from the default
/// equity account. Returns the account name, which the caller takes only if it had none. `qif-file.scm:585-629`.
pub fn process_opening_balance_xtn(acct_name: Option<&str>, xtn: &mut QifXtn, options: &ReadOptions) -> Option<String> {
    let no_security = xtn.security_name.is_none();
    let opening_payee = xtn.payee.as_deref().is_some_and(|p| p.trim_end() == "Opening Balance");
    let Some(split) = xtn.splits.first_mut() else { return acct_name.map(str::to_owned) };
    let named = acct_name.is_none() && no_security && opening_payee && split.category_is_account;
    let same_account = acct_name.is_some_and(|a| a == split.category) && no_security;
    if named || same_account {
        let category = std::mem::replace(&mut split.category, options.default_equity_account.clone());
        split.category_is_account = true;
        return Some(category);
    }
    acct_name.map(str::to_owned)
}

/// `qif-file:check-from-acct`: whether every transaction knows its account. When one does not, GnuCash asks the
/// user for an account name, offering [`QifFile::path_to_accountname`]. `qif-file.scm:631-640`.
pub fn check_from_acct(file: &QifFile) -> bool {
    file.xtns.iter().all(|x| x.from_acct.is_some())
}

/// Whether `c` is in Guile's `char-set:punctuation` (Unicode's P categories). ASCII exactly; beyond ASCII, the
/// common punctuation blocks.
fn is_punctuation(c: char) -> bool {
    if c.is_ascii() {
        return c.is_ascii_punctuation() && !"$+<=>^`|~".contains(c);
    }
    matches!(c, '\u{a1}' | '\u{a7}' | '\u{ab}' | '\u{b6}' | '\u{b7}' | '\u{bb}' | '\u{bf}')
        || ('\u{2010}'..='\u{2027}').contains(&c)
        || ('\u{2030}'..='\u{205e}').contains(&c)
        || ('\u{3001}'..='\u{3003}').contains(&c)
        || ('\u{3008}'..='\u{3011}').contains(&c)
}

/// `qif-file:parse-price-line`: a price line `"SYMBOL",value,"DATE"`, where the value is anything
/// `gnc-numeric-from-string` reads, such as `42.50` or `42 1/2`. The value is kept as Scheme prints the exact
/// number (`85/2`), and the date as written; [`parse_fields`] parses both. `qif-file.scm:1139-1165`.
pub fn parse_price_line(line: &str, line_num: usize) -> Option<QifPrice> {
    let remove_punctuation = |s: &str| s.trim_matches(is_punctuation).to_owned();
    let parts: Vec<&str> = line.split(',').collect();
    let [symbol, value, date] = parts.as_slice() else { return None };
    let (symbol, date) = (remove_punctuation(symbol), remove_punctuation(date));
    let value = gnc_numeric_from_string(value)?;
    if symbol.is_empty() || date.is_empty() {
        return None;
    }
    Some(QifPrice { line: line_num, symbol, share_price: Field::Text(scheme_number_string(value)), date: Field::Text(date) })
}

/// Scheme's `number->string` of an exact number: `n` or `n/d`.
fn scheme_number_string(value: Numeric) -> String {
    if value.denom() == 1 { value.num().to_string() } else { format!("{}/{}", value.num(), value.denom()) }
}

/// The kind of record a field belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RecordKind {
    /// A `!Type:Cat` record.
    Category,
    /// An `!Account` record.
    Account,
    /// A `!Type:Prices` line.
    Price,
    /// A transaction.
    Transaction,
}

/// The fields `parse-fields` parses, by the type names of its errors. `qif-file.scm:709-733`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FieldKind {
    /// `date`.
    Date,
    /// `split-amounts`.
    SplitAmounts,
    /// `share-price`.
    SharePrice,
    /// `num-shares`.
    NumShares,
    /// `action`.
    Action,
    /// `cleared`.
    Cleared,
    /// `commission`.
    Commission,
    /// `acct-type`.
    AcctType,
    /// `tax-class`.
    TaxClass,
    /// `budget-amt`.
    BudgetAmt,
    /// `budget`.
    Budget,
    /// `limit`.
    Limit,
}

impl fmt::Display for FieldKind {
    /// The names GnuCash logs. `qif-file.scm:715-728`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Date => "Transaction date",
            Self::SplitAmounts => "Transaction amount",
            Self::SharePrice => "Share price",
            Self::NumShares => "Share quantity",
            Self::Action => "Investment action",
            Self::Cleared => "Reconciliation status",
            Self::Commission => "Commission",
            Self::AcctType => "Account type",
            Self::TaxClass => "Tax class",
            Self::BudgetAmt => "Category budget amount",
            Self::Budget => "Account budget amount",
            Self::Limit => "Credit limit",
        })
    }
}

/// A date or number format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Format {
    /// A date format.
    Date(DateFormat),
    /// A number format.
    Number(NumberFormat),
}

impl From<DateFormat> for Format {
    fn from(f: DateFormat) -> Self {
        Self::Date(f)
    }
}

impl From<NumberFormat> for Format {
    fn from(f: NumberFormat) -> Self {
        Self::Number(f)
    }
}

impl fmt::Display for Format {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Date(d) => d.symbol(),
            Self::Number(n) => n.symbol(),
        })
    }
}

/// What went wrong with a field.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum FieldProblem {
    /// No format fits every value of the field. `qif-file.scm:1021-1027`.
    #[error("Unrecognized or inconsistent format.")]
    UnrecognizedFormat,
    /// Several formats fit and read some value differently; the field is left unparsed. Only dates are checked
    /// this way. `qif-file.scm:1029-1046`.
    #[error("Parse ambiguity between formats {}", list(.0))]
    Ambiguous(Vec<Format>),
    /// A value does not parse in the format chosen. `qif-file.scm:1064-1067`.
    #[error("Parsing failed.")]
    ParsingFailed,
    /// A number too large for GnuCash's 64-bit numbers.
    #[error("{0}")]
    Overflow(NumberOverflow),
    /// A number that is not one in the format chosen, which GnuCash reads as zero (`qif-parse.scm:449`). The value
    /// is set to zero, as GnuCash sets it; red-ale reports it so the record can become an error
    /// (`design/api/import.md`, QIF deviations).
    #[error("Not a number: {0:?}")]
    Unreadable(String),
    /// A message from a field parser: an unknown account type, action or reconciliation status.
    #[error("{0}")]
    Message(String),
}

fn list(formats: &[Format]) -> String {
    let names: Vec<String> = formats.iter().map(ToString::to_string).collect();
    format!("({})", names.join(" "))
}

/// One error of `parse-fields`: the `(type . error)` pairs GnuCash collects. `qif-file.scm:709-733`.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("{}{field}: {problem}", .line.map(|l| format!("Line {l}: ")).unwrap_or_default())]
pub struct FieldError {
    /// The kind of record.
    pub record: RecordKind,
    /// The field.
    pub field: FieldKind,
    /// The line of the record the problem was found in, when one record is at fault.
    pub line: Option<usize>,
    /// What went wrong.
    pub problem: FieldProblem,
}

/// Several number formats fit a field and read it differently, and GnuCash took the first without asking
/// (`'guess-on-ambiguity`, `qif-file.scm:1035-1038`). GnuCash says nothing of it; red-ale reports it so a client can
/// show it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormatGuess {
    /// The kind of record.
    pub record: RecordKind,
    /// The field.
    pub field: FieldKind,
    /// The format used: the first candidate.
    pub chosen: Format,
    /// Every format that fits, in GnuCash's order.
    pub candidates: Vec<Format>,
}

/// What [`parse_fields`] found besides the parsed values.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ParseFieldsReport {
    /// Warnings, in the order found. GnuCash keeps them newest first.
    pub errors: Vec<FieldError>,
    /// Number formats taken by guess.
    pub guesses: Vec<FormatGuess>,
}

impl ParseFieldsReport {
    /// `qif-file:parse-fields-results` for `'date`: the date formats of the newest ambiguous date field, which the
    /// caller must choose between and pass to [`reparse_dates`]. GnuCash shows them in this order with none chosen
    /// (`assistant-qif-import.c:2043-2075`). `qif-file.scm:1120-1136`.
    pub fn date_formats(&self) -> Option<Vec<DateFormat>> {
        let newest = self.errors.iter().rev().find(|e| e.field == FieldKind::Date)?;
        let FieldProblem::Ambiguous(formats) = &newest.problem else { return None };
        Some(formats.iter().filter_map(|f| if let Format::Date(d) = f { Some(*d) } else { None }).collect())
    }

    fn error(&mut self, record: RecordKind, field: FieldKind, line: Option<usize>, problem: FieldProblem) {
        self.errors.push(FieldError { record, field, line, problem });
    }
}

/// `parse-fields` stopped at a field no format fits, or a value that does not parse: GnuCash's `(#f . errors)`,
/// after which the assistant unloads the file.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("{}", .report.errors.first().map(ToString::to_string).unwrap_or_default())]
pub struct ParseFieldsError {
    /// Everything found up to the field that failed.
    pub report: ParseFieldsReport,
}

/// How `check-and-parse-field` treats several formats that fit. `qif-file.scm:1015-1019`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OnAmbiguity {
    /// `'guess-on-ambiguity`: take the first.
    Guess,
    /// `'error-on-ambiguity`: report the formats unless they read every value the same.
    Error,
}

/// The fixed arguments of one `check-and-parse-field` call.
struct Check<G, P, F> {
    record: RecordKind,
    field: FieldKind,
    formats: Vec<F>,
    on_ambiguity: OnAmbiguity,
    checker: fn(&G, &[F]) -> Option<Vec<F>>,
    parser: fn(&G, F) -> Result<Option<P>, NumberOverflow>,
    /// red-ale's: the values a parser reads as zero because they are not numbers.
    unreadable: fn(&G, F) -> Vec<String>,
}

/// How to reach one field of a record: read its unparsed value, set its parsed value, and find the record's line.
struct Slot<R, G, P> {
    get: fn(&R) -> Option<G>,
    set: fn(&mut R, P),
    line: fn(&R) -> usize,
}

/// `check-and-parse-field`: find the formats every value of a field fits, choose one, and parse every value with
/// it. Returns `false` when parsing must stop. `qif-file.scm:972-1075`.
fn check_and_parse_field<R, G, P: PartialEq, F: Copy + Into<Format>>(
    objects: &mut [R],
    slot: &Slot<R, G, P>,
    check: &Check<G, P, F>,
    report: &mut ParseFieldsReport,
) -> bool {
    let (formats, do_parsing) = match find_formats(objects, slot, check) {
        Ok(found) => found,
        Err(line) => {
            report.error(check.record, check.field, line, FieldProblem::UnrecognizedFormat);
            return false;
        }
    };
    let Some(format) = choose_format(objects, slot, check, &formats, do_parsing, report) else { return true };
    if !do_parsing {
        return true;
    }
    let mut ok = true;
    objects.iter_mut().for_each(|obj| {
        let Some(val) = (slot.get)(obj) else { return };
        let line = Some((slot.line)(obj));
        (check.unreadable)(&val, format).into_iter().for_each(|text| report.error(check.record, check.field, line, FieldProblem::Unreadable(text)));
        let problem = match (check.parser)(&val, format) {
            Ok(Some(parsed)) => return (slot.set)(obj, parsed),
            Ok(None) => FieldProblem::ParsingFailed,
            Err(overflow) => FieldProblem::Overflow(overflow),
        };
        ok = false;
        report.error(check.record, check.field, line, problem);
    });
    ok
}

/// The first loop of `check-and-parse-field`: narrow the formats value by value, stopping when none is left.
/// Returns the formats and whether any value was present, or the line of the record where no format was left.
/// GnuCash raises instead when a date is empty or unreadable before the last record; the file fails either way.
/// `qif-file.scm:993-1009`.
fn find_formats<R, G, P, F: Copy>(objects: &[R], slot: &Slot<R, G, P>, check: &Check<G, P, F>) -> Result<(Vec<F>, bool), Option<usize>> {
    let mut formats = check.formats.clone();
    let mut do_parsing = false;
    for obj in objects {
        if let Some(val) = (slot.get)(obj) {
            do_parsing = true;
            formats = (check.checker)(&val, &formats).ok_or(Some((slot.line)(obj)))?;
        }
        if formats.is_empty() {
            return Err(Some((slot.line)(obj)));
        }
    }
    if formats.is_empty() { Err(None) } else { Ok((formats, do_parsing)) }
}

/// The `cond` of `check-and-parse-field`: one format fits, or several do and GnuCash guesses or reports them.
/// `None` when the field is left unparsed. `qif-file.scm:1021-1048`.
fn choose_format<R, G, P: PartialEq, F: Copy + Into<Format>>(
    objects: &[R],
    slot: &Slot<R, G, P>,
    check: &Check<G, P, F>,
    formats: &[F],
    do_parsing: bool,
    report: &mut ParseFieldsReport,
) -> Option<F> {
    let first = *formats.first()?;
    if formats.len() == 1 || !do_parsing {
        return Some(first);
    }
    let equivalent = all_formats_equivalent(objects, slot, check, formats);
    let candidates = || formats.iter().map(|f| (*f).into()).collect::<Vec<Format>>();
    match check.on_ambiguity {
        OnAmbiguity::Guess => {
            if !equivalent {
                let guess = FormatGuess { record: check.record, field: check.field, chosen: first.into(), candidates: candidates() };
                report.guesses.push(guess);
            }
            Some(first)
        }
        OnAmbiguity::Error if equivalent => Some(first),
        OnAmbiguity::Error => {
            report.error(check.record, check.field, None, FieldProblem::Ambiguous(candidates()));
            None
        }
    }
}

/// `all-formats-equivalent?`: whether every format reads every value the same as the first format does. GnuCash
/// only asks this with `'error-on-ambiguity`; red-ale also asks it for guesses, to report only the guesses that
/// matter. `qif-file.scm:1078-1117`.
fn all_formats_equivalent<R, G, P: PartialEq, F: Copy>(objects: &[R], slot: &Slot<R, G, P>, check: &Check<G, P, F>, formats: &[F]) -> bool {
    let Some((first, rest)) = formats.split_first() else { return true };
    objects.iter().filter_map(|obj| (slot.get)(obj)).all(|val| {
        let parsed = (check.parser)(&val, *first);
        rest.iter().all(|f| (check.parser)(&val, *f) == parsed)
    })
}

/// `parse-field`: parse every value of a field with a parser that cannot fail, collecting its messages.
/// `qif-file.scm:949-969`.
fn parse_field<R, P>(
    objects: &mut [R],
    slot: &Slot<R, String, P>,
    parser: fn(&str, &mut Vec<String>) -> P,
    (record, field): (RecordKind, FieldKind),
    report: &mut ParseFieldsReport,
) -> bool {
    objects.iter_mut().for_each(|obj| {
        let Some(unparsed) = (slot.get)(obj) else { return };
        let mut messages = Vec::new();
        let parsed = parser(&unparsed, &mut messages);
        (slot.set)(obj, parsed);
        let line = Some((slot.line)(obj));
        messages.into_iter().for_each(|m| report.error(record, field, line, FieldProblem::Message(m)));
    });
    true
}

fn text_of<T>(field: Option<&Field<T>>) -> Option<String> {
    field.and_then(Field::text).map(str::to_owned)
}

// The checkers and parsers in the shape `Check` takes: the unparsed value as its owned type, the result as
// `check-and-parse-field` expects it.
#[allow(clippy::ptr_arg, clippy::unnecessary_wraps)]
fn check_dates(val: &String, formats: &[DateFormat]) -> Option<Vec<DateFormat>> {
    parse::check_date_format(val, formats)
}

#[allow(clippy::ptr_arg, clippy::unnecessary_wraps)]
fn parse_date(val: &String, format: DateFormat) -> Result<Option<QifDate>, NumberOverflow> {
    Ok(parse::parse_date_format(val, format))
}

#[allow(clippy::ptr_arg, clippy::unnecessary_wraps)]
fn check_number(val: &String, formats: &[NumberFormat]) -> Option<Vec<NumberFormat>> {
    Some(parse::check_number_format(val, formats))
}

#[allow(clippy::ptr_arg)]
fn unreadable_number(val: &String, format: NumberFormat) -> Vec<String> {
    if parse::number_is_readable(val, format) { Vec::new() } else { vec![val.clone()] }
}

fn no_unreadable<G, F>(_: &G, _: F) -> Vec<String> {
    Vec::new()
}

#[allow(clippy::ptr_arg)]
fn parse_number(val: &String, format: NumberFormat) -> Result<Option<Numeric>, NumberOverflow> {
    parse::parse_number_format(val, format).map(Some)
}

/// A `check-and-parse-field` of numbers with `'guess-on-ambiguity`, as every number field of `parse-fields` is.
fn numbers<R>(objects: &mut [R], slot: &Slot<R, String, Numeric>, target: (RecordKind, FieldKind), formats: &[NumberFormat], report: &mut ParseFieldsReport) -> bool {
    let (record, field) = target;
    let check = Check {
        record,
        field,
        formats: formats.to_vec(),
        on_ambiguity: OnAmbiguity::Guess,
        checker: check_number,
        parser: parse_number,
        unreadable: unreadable_number,
    };
    check_and_parse_field(objects, slot, &check, report)
}

/// A `check-and-parse-field` of dates with `'error-on-ambiguity`.
fn dates<R>(objects: &mut [R], slot: &Slot<R, String, QifDate>, record: RecordKind, formats: &[DateFormat], report: &mut ParseFieldsReport) -> bool {
    let check =
        Check { record, field: FieldKind::Date, formats: formats.to_vec(), on_ambiguity: OnAmbiguity::Error, checker: check_dates, parser: parse_date, unreadable: no_unreadable };
    check_and_parse_field(objects, slot, &check, report)
}

const DECIMAL_COMMA: [NumberFormat; 2] = [NumberFormat::Decimal, NumberFormat::Comma];
const DECIMAL_COMMA_RATIONAL: [NumberFormat; 3] = [NumberFormat::Decimal, NumberFormat::Comma, NumberFormat::Rational];

type Step = fn(&mut QifFile, &mut ParseFieldsReport) -> bool;

/// The fields of `parse-fields`, in its order. `qif-file.scm:736-934`.
const STEPS: [Step; 14] = [
    |f, r| {
        let slot = Slot { get: |c: &QifCat| text_of(c.tax_class.as_ref()), set: |c, v| c.tax_class = Some(Field::Value(v)), line: |c| c.line };
        numbers(f.cats.make_contiguous(), &slot, (RecordKind::Category, FieldKind::TaxClass), &DECIMAL_COMMA, r)
    },
    |f, r| {
        let slot = Slot { get: |c: &QifCat| text_of(c.budget_amt.as_ref()), set: |c, v| c.budget_amt = Some(Field::Value(v)), line: |c| c.line };
        numbers(f.cats.make_contiguous(), &slot, (RecordKind::Category, FieldKind::BudgetAmt), &DECIMAL_COMMA, r)
    },
    |f, r| {
        let slot = Slot { get: |a: &QifAcct| text_of(a.limit.as_ref()), set: |a, v| a.limit = Some(Field::Value(v)), line: |a| a.line };
        numbers(f.accounts.make_contiguous(), &slot, (RecordKind::Account, FieldKind::Limit), &DECIMAL_COMMA, r)
    },
    |f, r| {
        let slot = Slot { get: |a: &QifAcct| text_of(a.budget.as_ref()), set: |a, v| a.budget = Some(Field::Value(v)), line: |a| a.line };
        numbers(f.accounts.make_contiguous(), &slot, (RecordKind::Account, FieldKind::Budget), &DECIMAL_COMMA, r)
    },
    |f, r| {
        let slot = Slot { get: |a: &QifAcct| text_of(Some(&a.acct_type)), set: |a, v| a.acct_type = Field::Value(v), line: |a| a.line };
        parse_field(f.accounts.make_contiguous(), &slot, parse_acct_type, (RecordKind::Account, FieldKind::AcctType), r)
    },
    |f, r| dates(f.prices.make_contiguous(), &PRICE_DATE, RecordKind::Price, &DATE_FORMATS, r),
    |f, r| {
        let slot =
            Slot { get: |p: &QifPrice| text_of(Some(&p.share_price)), set: |p, v| p.share_price = Field::Value(v), line: |p| p.line };
        numbers(f.prices.make_contiguous(), &slot, (RecordKind::Price, FieldKind::SharePrice), &DECIMAL_COMMA_RATIONAL, r)
    },
    |f, r| dates(&mut f.xtns, &XTN_DATE, RecordKind::Transaction, &DATE_FORMATS, r),
    |f, r| {
        let slot = Slot { get: |x: &QifXtn| text_of(x.cleared.as_ref()), set: |x, v: Option<Cleared>| x.cleared = v.map(Field::Value), line: |x| x.line };
        parse_field(&mut f.xtns, &slot, parse_cleared, (RecordKind::Transaction, FieldKind::Cleared), r)
    },
    |f, r| {
        let slot = Slot { get: |x: &QifXtn| text_of(x.action.as_ref()), set: |x, v| x.action = Some(Field::Value(v)), line: |x| x.line };
        parse_field(&mut f.xtns, &slot, parse_action, (RecordKind::Transaction, FieldKind::Action), r)
    },
    |f, r| {
        let slot =
            Slot { get: |x: &QifXtn| text_of(x.share_price.as_ref()), set: |x, v| x.share_price = Some(Field::Value(v)), line: |x| x.line };
        numbers(&mut f.xtns, &slot, (RecordKind::Transaction, FieldKind::SharePrice), &DECIMAL_COMMA_RATIONAL, r)
    },
    |f, r| {
        let slot =
            Slot { get: |x: &QifXtn| text_of(x.num_shares.as_ref()), set: |x, v| x.num_shares = Some(Field::Value(v)), line: |x| x.line };
        numbers(&mut f.xtns, &slot, (RecordKind::Transaction, FieldKind::NumShares), &DECIMAL_COMMA, r)
    },
    |f, r| {
        let slot =
            Slot { get: |x: &QifXtn| text_of(x.commission.as_ref()), set: |x, v| x.commission = Some(Field::Value(v)), line: |x| x.line };
        numbers(&mut f.xtns, &slot, (RecordKind::Transaction, FieldKind::Commission), &DECIMAL_COMMA, r)
    },
    split_amounts,
];

/// The transaction date slot, shared by `parse-fields` and `reparse-dates`.
const XTN_DATE: Slot<QifXtn, String, QifDate> =
    Slot { get: |x: &QifXtn| text_of(x.date.as_ref()), set: |x, v| x.date = Some(Field::Value(v)), line: |x| x.line };

/// The price date slot, shared by `parse-fields` and [`reparse_price_dates`].
const PRICE_DATE: Slot<QifPrice, String, QifDate> =
    Slot { get: |p: &QifPrice| text_of(Some(&p.date)), set: |p, v| p.date = Field::Value(v), line: |p| p.line };

fn parse_acct_type(value: &str, messages: &mut Vec<String>) -> Vec<AccountType> {
    parse::parse_acct_type(value, |m| messages.push(m))
}

fn parse_cleared(value: &str, messages: &mut Vec<String>) -> Option<Cleared> {
    parse::parse_cleared_field(value, |m| messages.push(m))
}

fn parse_action(value: &str, messages: &mut Vec<String>) -> Action {
    parse::parse_action_field(value, |m| messages.push(m))
}

/// The split amounts of every transaction, checked and set together: the own split's amount and the splits'
/// amounts must all fit one format. `qif-file.scm:916-927`.
fn split_amounts(file: &mut QifFile, report: &mut ParseFieldsReport) -> bool {
    let slot: Slot<QifXtn, Vec<Option<String>>, Vec<Numeric>> = Slot {
        get: |x| Some(x.split_amounts().into_iter().map(|a| a.map(str::to_owned)).collect()),
        set: |x, v| x.set_split_amounts(&v),
        line: |x| x.line,
    };
    let check = Check {
        record: RecordKind::Transaction,
        field: FieldKind::SplitAmounts,
        formats: DECIMAL_COMMA.to_vec(),
        on_ambiguity: OnAmbiguity::Guess,
        checker: |amounts: &Vec<Option<String>>, formats| Some(parse::check_number_formats(&borrowed(amounts), formats)),
        parser: |amounts, format| parse::parse_numbers_format(&borrowed(amounts), format).map(Some),
        unreadable: |amounts, format| amounts.iter().flatten().filter(|a| !parse::number_is_readable(a, format)).cloned().collect(),
    };
    check_and_parse_field(&mut file.xtns, &slot, &check, report)
}

fn borrowed(amounts: &[Option<String>]) -> Vec<Option<&str>> {
    amounts.iter().map(Option::as_deref).collect()
}

/// `qif-file:parse-fields`: parse the fields of every record in place, field by field, stopping at the first field
/// no format fits. Ambiguous dates are left unparsed and reported ([`ParseFieldsReport::date_formats`]); unknown
/// account types, actions and reconciliation statuses are reported and replaced as GnuCash replaces them.
/// `qif-file.scm:654-946`.
pub fn parse_fields(file: &mut QifFile) -> Result<ParseFieldsReport, ParseFieldsError> {
    let mut report = ParseFieldsReport::default();
    let all_ok = STEPS.iter().all(|step| step(file, &mut report));
    if all_ok { Ok(report) } else { Err(ParseFieldsError { report }) }
}

/// `qif-file:reparse-dates`: parse the transaction dates still unparsed with the format the caller chose. Price
/// dates are not reparsed, as in GnuCash ([`reparse_price_dates`] does them). Returns the errors when some
/// date does not parse in that format. `qif-file.scm:642-651`.
pub fn reparse_dates(file: &mut QifFile, format: DateFormat) -> Result<(), Vec<FieldError>> {
    reparse(&mut file.xtns, &XTN_DATE, RecordKind::Transaction, format)
}

/// Parse the price dates still unparsed with the format the caller chose. red-ale's: GnuCash never reparses them,
/// so an ambiguous price date stays text and its import fails; red-ale applies the caller's date format to them as
/// well (`design/api/import.md`, QIF deviations).
pub fn reparse_price_dates(file: &mut QifFile, format: DateFormat) -> Result<(), Vec<FieldError>> {
    reparse(file.prices.make_contiguous(), &PRICE_DATE, RecordKind::Price, format)
}

fn reparse<R>(objects: &mut [R], slot: &Slot<R, String, QifDate>, record: RecordKind, format: DateFormat) -> Result<(), Vec<FieldError>> {
    let mut report = ParseFieldsReport::default();
    if dates(objects, slot, record, &[format], &mut report) && report.errors.is_empty() { Ok(()) } else { Err(report.errors) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_price_line() {
        // test-qif-imp.scm:65-84
        let price = super::parse_price_line("\"ABC\",1.0,\"1/1/04\"", 1).unwrap();
        assert_eq!((price.symbol.as_str(), price.share_price.text(), price.date.text()), ("ABC", Some("1"), Some("1/1/04")));
        let price = super::parse_price_line("\"ABC\",1 3/4,\"1/1' 4\"", 1).unwrap();
        assert_eq!((price.symbol.as_str(), price.share_price.text(), price.date.text()), ("ABC", Some("7/4"), Some("1/1' 4")));
        assert_eq!(super::parse_price_line("\"ABC\",,\"1/1' 4\"", 1), None);
        assert_eq!(super::parse_price_line("\"ABC\",\"1/1' 4\"", 1), None);
    }

    #[test]
    fn numbered_lines_count_crlf_once() {
        let lines: Vec<_> = numbered_lines("a\r\nb\rc\n\nd").collect();
        assert_eq!(lines, vec![(1, "a"), (2, "b"), (3, "c"), (5, "d")]);
    }

    #[test]
    fn punctuation_is_guiles() {
        assert!(['"', '\'', '.', '/', '-', '«'].iter().all(|c| is_punctuation(*c)));
        assert!(!['$', '+', 'a', ' '].iter().any(|c| is_punctuation(*c)));
    }
}
