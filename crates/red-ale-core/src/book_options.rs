//! Book-level options and Book features, read from the Book's slots.
//!
//! Paths from `5.17:libgnucash/engine/qofbookslots.h:55-70` and `5.17:libgnucash/engine/qofbook.cpp:919-1019`;
//! see `design/notes/storage-and-safety.md` §10-11.

use crate::{Book, SlotValue};

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

    #[test]
    fn red_line_text_truncates() {
        assert_eq!(leading_integer("30"), 30);
        assert_eq!(leading_integer("30.9"), 30);
        assert_eq!(leading_integer("0"), 0);
        assert_eq!(leading_integer("1e2"), 100);
    }
}
