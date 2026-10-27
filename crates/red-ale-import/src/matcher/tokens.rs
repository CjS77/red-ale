//! The tokens a transaction is known by in the Bayesian Import map (`tokenize_string` and `TransactionGetTokens`,
//! `5.17:gnucash/import-export/import-backend.cpp:434-496`).

use red_ale_core::Transaction;
use red_ale_locale::{CivilDate, Locale};

/// `tokenize_string` (`import-backend.cpp:434-454`): split on single spaces, drop empty pieces, and put each piece
/// not yet in `tokens` (compared exactly) in front.
fn tokenize_into(tokens: &mut Vec<String>, text: &str) {
    text.split(' ').filter(|t| !t.is_empty()).for_each(|t| {
        if !tokens.iter().any(|existing| existing == t) {
            tokens.insert(0, t.to_owned());
        }
    });
}

/// The weekday name of the posted time in UTC, `%A` in `locale` (`gnc_gmtime` then `qof_strftime`,
/// `import-backend.cpp:476-485`). A posted time outside the calendar gives none.
fn weekday_token(t: &Transaction, locale: &Locale) -> Option<String> {
    let date = t.date_posted.utc_date()?;
    let civil = CivilDate::new(date.year(), u8::try_from(date.month()).ok()?, u8::try_from(date.day()).ok()?)?;
    Some(locale.day_name(civil.weekday()).to_owned())
}

/// `TransactionGetTokens` (`import-backend.cpp:457-496`), in GnuCash's list order: each new token goes in front, so
/// the last split's memo tokens come first and the description's last. The tokens are, in the order added:
///
/// 1. the description's;
/// 2. the posted day's weekday name, added **without** the duplicate check, so a description holding the weekday's
///    name gives that token twice;
/// 3. each split's memo's, in split order.
///
/// GnuCash formats the weekday into a 16-byte buffer and fails on a longer name; no glibc day name is that long in
/// UTF-8 in the locales red-ale has, so the failure is not modelled.
pub fn transaction_tokens(t: &Transaction, locale: &Locale) -> Vec<String> {
    let mut tokens = Vec::new();
    tokenize_into(&mut tokens, t.description.as_deref().unwrap_or(""));
    if let Some(day) = weekday_token(t, locale) {
        tokens.insert(0, day);
    }
    t.splits
        .iter()
        .for_each(|s| tokenize_into(&mut tokens, s.memo.as_deref().unwrap_or("")));
    tokens
}
