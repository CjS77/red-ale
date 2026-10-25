//! Book-level options and Book features, read from the Book's slots, and the two options red-ale sets and follows:
//! the num field source and the auto read-only threshold.
//!
//! Paths from `5.17:libgnucash/engine/qofbookslots.h:55-70` and `5.17:libgnucash/engine/qofbook.cpp:919-1019`;
//! see `design/notes/storage-and-safety.md` §10-11 and `design/api/book-options.md`.

use crate::ops::business::contacts::set_feature_used;
use crate::ops::transactions::posted_date;
use crate::{Book, Date, DomainError, DomainResult, SlotValue, Split, Transaction};

/// The frame holding Book features: one string slot per feature, keyed by name.
pub const FEATURES: &str = "features";
/// "Use Trading Accounts": `"t"` when on.
pub const OPT_TRADING_ACCOUNTS: &str = "options/Accounts/Use Trading Accounts";
/// "Use Split Action Field for Number": `"t"` when on.
pub const OPT_NUM_SOURCE: &str = "options/Accounts/Use Split Action Field for Number";
/// "Day Threshold for Read-Only Transactions (red line)": a double, 0 when off.
pub const OPT_AUTO_READONLY_DAYS: &str = "options/Accounts/Day Threshold for Read-Only Transactions (red line)";

/// Book feature names known to GnuCash 5.17, with the description GnuCash writes for each
/// (`5.17:libgnucash/engine/gnc-features.cpp:36-48`).
pub const GNUCASH_FEATURES: [(&str, &str); 10] = [
    ("Credit Notes", "Customer and vendor credit notes (requires at least GnuCash 2.5.0)"),
    (
        "Number Field Source",
        "User specifies source of 'num' field'; either transaction number or split action (requires at least GnuCash 2.5.0)",
    ),
    ("Extra data in addresses, jobs or invoice entries", "Extra data for addresses, jobs or invoice entries (requires at least GnuCash 2.6.4)"),
    ("Account GUID based Bayesian data", "Use account GUID as key for Bayesian data (requires at least GnuCash 2.6.12)"),
    ("Account GUID based bayesian with flat KVP", "Use account GUID as key for bayesian data and store KVP flat (requires at least Gnucash 2.6.19)"),
    (
        "ISO-8601 formatted date strings in SQLite3 databases.",
        "Use ISO formatted date-time strings in SQLite3 databases (requires at least GnuCash 2.6.20)",
    ),
    (
        "Register sort and filter settings stored in .gcm file",
        "Store the register sort and filter settings in .gcm metadata file (requires at least GnuCash 3.3)",
    ),
    ("Use natural signs in budget amounts", "Store budget amounts unreversed (i.e. natural) signs (requires at least Gnucash 3.8)"),
    ("Show extra account columns in the Budget View", "Show extra account columns in the Budget View (requires at least Gnucash 3.8)"),
    (
        "Use a dedicated opening balance account identified by an 'equity-type' slot",
        "Use a dedicated opening balance account identified by an 'equity-type' slot (requires at least Gnucash 4.3)",
    ),
];

/// The obsolete feature GnuCash removes when it finds it (`5.17:libgnucash/engine/gnc-features.cpp:61-63`).
pub const OBSOLETE_BOOK_CURRENCY: &str = "Use a Book-Currency";

/// The Book features a book declares: name and description, in key order.
pub fn features(book: &Book) -> Vec<(String, String)> {
    book.slots
        .get(FEATURES)
        .and_then(SlotValue::as_frame)
        .map(|f| f.iter().map(|(k, v)| (k.clone(), v.as_str().unwrap_or_default().to_owned())).collect())
        .unwrap_or_default()
}

fn option_is_t(book: &Book, path: &str) -> bool {
    book.slots.get_str(path) == Some("t")
}

/// True when the book balances multi-commodity transactions through trading accounts.
pub fn uses_trading_accounts(book: &Book) -> bool {
    option_is_t(book, OPT_TRADING_ACCOUNTS)
}

/// Turn "Use Trading Accounts" on or off as GnuCash's book options dialog does: an unchanged value writes nothing; a
/// change writes `"t"` or `"f"` (`5.17:libgnucash/engine/gnc-optiondb.cpp:404-410`). No transaction changes
/// (`qof_book_set_option`, `5.17:libgnucash/engine/qofbook.cpp:1379-1389`).
pub fn set_trading_accounts(book: &mut Book, on: bool) {
    if uses_trading_accounts(book) != on {
        book.slots.set(OPT_TRADING_ACCOUNTS, SlotValue::String(if on { "t" } else { "f" }.to_owned()));
    }
}

/// True when the book uses split actions as transaction numbers.
pub fn uses_split_action_for_num(book: &Book) -> bool {
    option_is_t(book, OPT_NUM_SOURCE)
}

/// The auto read-only threshold in days, truncated as GnuCash does (`(int)value`); 0 when off.
pub fn auto_readonly_days(book: &Book) -> i64 {
    match book.slots.get(OPT_AUTO_READONLY_DAYS) {
        Some(SlotValue::Integer(i)) => *i,
        Some(SlotValue::Double(text)) => leading_integer(text),
        _ => 0,
    }
}

/// The Book feature GnuCash sets when the num field source is turned on (`5.17:libgnucash/engine/gnc-features.h:55`).
pub const NUM_FIELD_SOURCE_FEATURE: &str = "Number Field Source";

/// The largest day threshold GnuCash's book options dialog accepts (`5.17:libgnucash/engine/gnc-optiondb.cpp:1206-1209`).
pub const MAX_AUTO_READONLY_DAYS: i64 = 3650;

/// Turn the num field source on or off as GnuCash's book options dialog does: an unchanged value writes nothing; a
/// change writes `"t"` or `"f"` (`5.17:libgnucash/engine/gnc-optiondb.cpp:404-410`), and turning it on sets the
/// `Number Field Source` Book feature, which is never removed (`5.17:gnucash/gnome-utils/gnc-main-window.cpp:4709-4721`).
pub fn set_split_action_for_num(book: &mut Book, on: bool) {
    if uses_split_action_for_num(book) == on {
        return;
    }
    book.slots.set(OPT_NUM_SOURCE, SlotValue::String(if on { "t" } else { "f" }.to_owned()));
    if on {
        set_feature_used(book, NUM_FIELD_SOURCE_FEATURE);
    }
}

/// Set the auto read-only threshold in days, as GnuCash's book options dialog does: 0 to 3650, written as a double
/// (whole days print as integers, `5.17:libgnucash/backend/xml/sixtp-dom-generators.cpp:189-203`) and only when
/// it changes.
pub fn set_auto_readonly_days(book: &mut Book, days: i64) -> DomainResult<()> {
    if !(0..=MAX_AUTO_READONLY_DAYS).contains(&days) {
        return Err(DomainError::invalid(format!("the read-only threshold must be 0 to {MAX_AUTO_READONLY_DAYS} days, not {days}")));
    }
    if auto_readonly_days(book) != days {
        book.slots.set(OPT_AUTO_READONLY_DAYS, SlotValue::Double(days.to_string()));
    }
    Ok(())
}

/// The threshold date, `today` minus the threshold's days, when the threshold is on (`qof_book_get_autoreadonly_gdate`,
/// `5.17:libgnucash/engine/qofbook.cpp:1006-1019`). A negative count, which GnuCash's dialog cannot enter, is off.
pub fn auto_readonly_threshold(book: &Book, today: Date) -> Option<Date> {
    let days = auto_readonly_days(book);
    (days > 0).then(|| today.add_days(-days))
}

/// Refuse a change to a transaction posted strictly before the threshold date (`xaccTransIsReadonlyByPostedDate`,
/// `5.17:libgnucash/engine/Transaction.cpp:2373-2409`), as GnuCash's register does.
pub fn check_not_read_only_by_date(book: &Book, txn: &Transaction, today: Date) -> DomainResult<()> {
    let Some(threshold) = auto_readonly_threshold(book, today) else { return Ok(()) };
    match posted_date(txn) {
        Some(date) if date < threshold => Err(DomainError::ReadOnlyByDate { date, threshold }),
        _ => Ok(()),
    }
}

/// Refuse a posted date before the threshold date, as GnuCash's register date cell does
/// (`5.17:gnucash/register/register-gnome/datecell-gnome.c:96-129`).
pub fn check_date_not_before_threshold(book: &Book, date: Date, today: Date) -> DomainResult<()> {
    match auto_readonly_threshold(book, today) {
        Some(threshold) if date < threshold => Err(DomainError::BeforeReadOnlyThreshold { date, threshold }),
        _ => Ok(()),
    }
}

/// A split's Number: its action when the book uses the split action for num, otherwise its transaction's num
/// (`gnc_get_num_action(trans, split)`, `5.17:libgnucash/engine/engine-helpers.c:56-76`).
pub fn number<'a>(book: &Book, txn: &'a Transaction, split: &'a Split) -> Option<&'a str> {
    if uses_split_action_for_num(book) { split.action.as_deref() } else { txn.num.as_deref() }
}

/// Where `gnc_set_num_action(trans, split, num, action)` puts a number and an action given together: the value for
/// the transaction's num, then the value for the split's action. With the num field source on they swap
/// (`5.17:libgnucash/engine/engine-helpers.c:134-148`).
pub fn num_action_targets<'a>(book: &Book, num: &'a str, action: &'a str) -> (&'a str, &'a str) {
    if uses_split_action_for_num(book) { (action, num) } else { (num, action) }
}

/// The integer part of a decimal text such as `30` or `30.000000000000000` or `1e2`; exponents are honoured for
/// whole numbers only. No floating-point arithmetic is used.
fn leading_integer(text: &str) -> i64 {
    let t = text.trim();
    let (mantissa, exponent) = match t.split_once(['e', 'E']) {
        Some((m, e)) => (m, e.parse::<u32>().unwrap_or(0)),
        None => (t, 0),
    };
    let int_part = mantissa.split('.').next().unwrap_or("0");
    let base: i64 = int_part.parse().unwrap_or(0);
    base.saturating_mul(10_i64.saturating_pow(exponent))
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::{CommodityRef, FixedEnv, Frame, Guid, Numeric, Time64};

    fn book() -> Book {
        Book::new_empty(&mut FixedEnv::new(Time64(0), 1))
    }

    fn txn(date: Date, num: Option<&str>, action: Option<&str>) -> Transaction {
        let mut split = Split::new(Guid::from_bytes([4; 16]), Guid::from_bytes([5; 16]), Numeric::new(0, 1), Numeric::new(0, 1));
        split.action = action.map(str::to_owned);
        Transaction {
            guid: Guid::from_bytes([3; 16]),
            currency: CommodityRef::currency("USD"),
            num: num.map(str::to_owned),
            date_posted: date.neutral_time(),
            date_entered: Time64(0),
            description: None,
            slots: Frame::new(),
            splits: vec![split],
        }
    }

    fn day(s: &str) -> Date {
        Date::parse(s).unwrap()
    }

    #[test]
    fn turning_the_num_source_on_sets_the_feature_and_off_keeps_it() {
        let mut b = book();
        set_split_action_for_num(&mut b, false);
        assert_eq!(b.slots.get(OPT_NUM_SOURCE), None, "an unchanged option writes nothing");
        set_split_action_for_num(&mut b, true);
        assert_eq!(b.slots.get_str(OPT_NUM_SOURCE), Some("t"));
        assert!(features(&b).iter().any(|(n, d)| n == NUM_FIELD_SOURCE_FEATURE && d.contains("requires at least GnuCash 2.5.0")));
        set_split_action_for_num(&mut b, false);
        assert_eq!(b.slots.get_str(OPT_NUM_SOURCE), Some("f"));
        assert!(!uses_split_action_for_num(&b));
        assert!(features(&b).iter().any(|(n, _)| n == NUM_FIELD_SOURCE_FEATURE));
    }

    #[test]
    fn days_are_written_as_doubles_within_the_dialog_range() {
        let mut b = book();
        set_auto_readonly_days(&mut b, 0).unwrap();
        assert_eq!(b.slots.get(OPT_AUTO_READONLY_DAYS), None);
        set_auto_readonly_days(&mut b, 30).unwrap();
        assert_eq!(b.slots.get(OPT_AUTO_READONLY_DAYS), Some(&SlotValue::Double("30".into())));
        assert_eq!(auto_readonly_days(&b), 30);
        set_auto_readonly_days(&mut b, 0).unwrap();
        assert_eq!(b.slots.get(OPT_AUTO_READONLY_DAYS), Some(&SlotValue::Double("0".into())));
        assert!(matches!(set_auto_readonly_days(&mut b, 3651), Err(DomainError::Invalid(_))));
        assert!(matches!(set_auto_readonly_days(&mut b, -1), Err(DomainError::Invalid(_))));
    }

    #[test]
    fn transactions_strictly_before_the_threshold_are_read_only() {
        let mut b = book();
        let today = day("2024-07-01");
        assert_eq!(auto_readonly_threshold(&b, today), None);
        set_auto_readonly_days(&mut b, 10).unwrap();
        assert_eq!(auto_readonly_threshold(&b, today), Some(day("2024-06-21")));
        assert!(check_not_read_only_by_date(&b, &txn(day("2024-06-21"), None, None), today).is_ok());
        let err = check_not_read_only_by_date(&b, &txn(day("2024-06-20"), None, None), today).unwrap_err();
        assert_eq!(err, DomainError::ReadOnlyByDate { date: day("2024-06-20"), threshold: day("2024-06-21") });
        assert!(check_date_not_before_threshold(&b, day("2024-06-21"), today).is_ok());
        assert!(matches!(check_date_not_before_threshold(&b, day("2024-06-20"), today), Err(DomainError::BeforeReadOnlyThreshold { .. })));
        b.slots.set(OPT_AUTO_READONLY_DAYS, SlotValue::Double("-5".into()));
        assert_eq!(auto_readonly_threshold(&b, today), None, "a negative count is off");
    }

    #[test]
    fn the_number_follows_the_num_source() {
        let mut b = book();
        let t = txn(day("2024-07-01"), Some("101"), Some("Buy"));
        assert_eq!(number(&b, &t, &t.splits[0]), Some("101"));
        assert_eq!(num_action_targets(&b, "INV-1", "Invoice"), ("INV-1", "Invoice"));
        set_split_action_for_num(&mut b, true);
        assert_eq!(number(&b, &t, &t.splits[0]), Some("Buy"));
        assert_eq!(num_action_targets(&b, "INV-1", "Invoice"), ("Invoice", "INV-1"));
    }

    #[test]
    fn trading_accounts_option_is_written_as_gnucash_writes_it() {
        let mut book = Book::new_empty(&mut crate::FixedEnv::new(crate::Time64(0), 0));
        set_trading_accounts(&mut book, false);
        assert!(book.slots.is_empty(), "off with no slot writes nothing");
        set_trading_accounts(&mut book, true);
        assert!(uses_trading_accounts(&book));
        set_trading_accounts(&mut book, false);
        assert_eq!(book.slots.get_str(OPT_TRADING_ACCOUNTS), Some("f"));
        assert!(!uses_trading_accounts(&book));
    }

    #[test]
    fn red_line_text_truncates() {
        assert_eq!(leading_integer("30"), 30);
        assert_eq!(leading_integer("30.9"), 30);
        assert_eq!(leading_integer("0"), 0);
        assert_eq!(leading_integer("1e2"), 100);
    }
}
