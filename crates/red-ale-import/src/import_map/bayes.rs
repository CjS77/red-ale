//! The Bayesian Import map (`5.17:libgnucash/engine/Account.cpp:5228-5346`, `:5509-5608`, `:5644-5688`, `:5762-5777`).
//!
//! Each entry is one Slot on the source account whose key is the whole flat path `import-map-bayes/<token>/<guid>`,
//! with an integer count: how many times a transaction with that token went to that account.

use red_ale_core::ops::business::contacts::set_feature_used;
use red_ale_core::{Book, DomainResult, Frame, Guid, SlotValue};

use super::convert::{FLAT_BAYESIAN_FEATURE, bayes_view, check_import_map_data};
use super::{IMAP_FRAME_BAYES, ImapInfo, account, slots_mut};

/// Probabilities are integers out of this (`probability_factor`, `Account.cpp:5280`).
pub const PROBABILITY_FACTOR: i32 = 100_000;

/// The least probability an account needs to be chosen: `.90 * probability_factor` (`Account.cpp:5509`), which is
/// exactly 90000.0 as a double, compared with the integer probability.
pub const THRESHOLD: i32 = 90_000;

/// The length of a GUID as text (`GUID_ENCODING_LENGTH`).
const GUID_LENGTH: usize = 32;

/// One token's entries (`TokenAccountsInfo`, `Account.cpp:5251-5255`): each account's count, in key order, and their
/// total.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct TokenAccounts {
    accounts: Vec<(String, i64)>,
    total: i64,
}

/// `get_first_pass_probabilities`' scan for one token (`Account.cpp:5318-5320`, with `build_token_info`, `:5266-5275`):
/// every key `import-map-bayes/<token>/` followed by exactly 32 bytes. A value that is not an integer counts 0, as
/// `KvpValue::get<int64_t>` gives.
fn token_accounts(slots: &Frame, token: &str) -> TokenAccounts {
    let prefix = format!("{IMAP_FRAME_BAYES}/{token}/");
    slots
        .iter_prefix(&prefix)
        .filter_map(|(key, value)| key.get(prefix.len()..).filter(|s| s.len() == GUID_LENGTH).map(|s| (s.to_owned(), value.as_int().unwrap_or(0))))
        .fold(TokenAccounts::default(), |mut info, (guid, count)| {
            info.total = info.total.wrapping_add(count);
            info.accounts.push((guid, count));
            info
        })
}

/// Each account's probability for the tokens, in order of first appearance: `get_first_pass_probabilities` and
/// `build_probabilities` (`Account.cpp:5282-5346`). An account is combined over only the tokens it has an entry for.
///
/// GnuCash computes this in doubles, and red-ale must pick the same account at the threshold, so this is the one place
/// red-ale computes in floating point; no amount of money passes through it.
#[allow(clippy::float_arithmetic, clippy::cast_precision_loss)]
fn probabilities(tokens: &[TokenAccounts]) -> Vec<(String, i32)> {
    let mut first_pass: Vec<(String, f64, f64)> = Vec::new();
    tokens.iter().for_each(|info| {
        info.accounts.iter().for_each(|(guid, count)| {
            let p = *count as f64 / info.total as f64;
            match first_pass.iter_mut().find(|(g, _, _)| g == guid) {
                Some((_, product, difference)) => {
                    *product *= p;
                    *difference *= 1.0 - p;
                }
                None => first_pass.push((guid.clone(), p, 1.0 - p)),
            }
        });
    });
    first_pass
        .into_iter()
        .map(|(guid, product, difference)| (guid, truncate_to_i32(product / (product + difference) * f64::from(PROBABILITY_FACTOR))))
        .collect()
}

/// C++'s conversion of a double to `int32_t`, truncating toward zero. GnuCash leaves NaN and out-of-range values
/// undefined; red-ale gives them `i32::MIN` as x86's `cvttsd2si` does, which no account can win with.
#[allow(clippy::cast_possible_truncation)]
fn truncate_to_i32(x: f64) -> i32 {
    if x.is_finite() && x > -2_147_483_649.0 && x < 2_147_483_648.0 { x as i32 } else { i32::MIN }
}

/// `highest_probability` (`Account.cpp:5300-5308`): the first account with the highest probability.
fn highest(probabilities: &[(String, i32)]) -> (&str, i32) {
    probabilities.iter().fold(("", i32::MIN), |best, (guid, p)| if *p > best.1 { (guid.as_str(), *p) } else { best })
}

/// Each account's probability for `tokens` in `acc`'s map, in GnuCash's order, before the threshold: what
/// [`find_account_bayes`] decides by.
pub fn account_probabilities(book: &Book, acc: Guid, tokens: &[String]) -> Vec<(String, i32)> {
    let Some(account) = book.account(acc) else { return Vec::new() };
    let slots = bayes_view(book, account);
    probabilities(&tokens.iter().map(|t| token_accounts(&slots, t)).collect::<Vec<_>>())
}

/// The account `acc`'s Bayesian map sends `tokens` to (`gnc_account_imap_find_account_bayes`,
/// `Account.cpp:5511-5538`): the most probable account, if its probability is at least [`THRESHOLD`] and its GUID
/// names an account in the Book.
pub fn find_account_bayes(book: &Book, acc: Guid, tokens: &[String]) -> Option<Guid> {
    let probabilities = account_probabilities(book, acc, tokens);
    let (guid, probability) = highest(&probabilities);
    if guid.is_empty() || probability < THRESHOLD {
        return None;
    }
    Guid::parse(guid).ok().filter(|g| book.account(*g).is_some())
}

/// Count each non-empty token once more for `added` in `acc`'s map (`gnc_account_imap_add_account_bayes` and
/// `change_imap_entry`, `Account.cpp:5540-5608`); a token listed twice counts twice. Old data is converted first, and
/// the flat feature is set every time, as GnuCash sets it.
pub fn add_account_bayes(book: &mut Book, acc: Guid, tokens: &[String], added: Guid) -> DomainResult<()> {
    account(book, acc)?;
    account(book, added)?;
    check_import_map_data(book)?;
    let slots = slots_mut(book, acc)?;
    tokens.iter().filter(|t| !t.is_empty()).for_each(|token| {
        let key = format!("{IMAP_FRAME_BAYES}/{token}/{added}");
        let existing = slots.get_key(&key).map_or(0, |v| v.as_int().unwrap_or(0));
        slots.insert_key(key, SlotValue::Integer(existing.wrapping_add(1)));
    });
    set_feature_used(book, FLAT_BAYESIAN_FEATURE);
    Ok(())
}

/// The Bayesian entries of `acc`, in key order (`gnc_account_imap_get_info_bayes` and `build_bayes`,
/// `Account.cpp:5644-5688`), old data seen converted. A key too short to hold a token and a GUID, on which GnuCash
/// reads out of bounds, is left out.
pub fn bayes_info(book: &Book, acc: Guid) -> Vec<ImapInfo> {
    let Some(account) = book.account(acc) else { return Vec::new() };
    let slots = bayes_view(book, account);
    slots.iter_prefix(IMAP_FRAME_BAYES).filter_map(|(key, value)| bayes_entry(book, acc, key, value)).collect()
}

fn bayes_entry(book: &Book, acc: Guid, key: &str, value: &SlotValue) -> Option<ImapInfo> {
    let suffix = key.get(IMAP_FRAME_BAYES.len()..)?;
    let guid_start = suffix.len().checked_sub(GUID_LENGTH).filter(|start| *start >= 2)?;
    Some(ImapInfo {
        source_account: acc,
        map_account: suffix.get(guid_start..).and_then(|g| Guid::parse(g).ok()).filter(|g| book.account(*g).is_some()),
        head: key.to_owned(),
        category: None,
        match_string: suffix.get(1..guid_start - 1)?.to_owned(),
        count: Some(value.as_int().unwrap_or(0)),
    })
}

/// Delete every Bayesian entry of `acc`: every top-level key starting `import-map-bayes`
/// (`gnc_account_delete_all_bayes_maps`, `Account.cpp:5762-5777`). Returns how many went.
pub fn delete_all_bayes_maps(book: &mut Book, acc: Guid) -> DomainResult<usize> {
    account(book, acc)?;
    check_import_map_data(book)?;
    let slots = slots_mut(book, acc)?;
    let keys: Vec<String> = slots.iter_prefix(IMAP_FRAME_BAYES).map(|(k, _)| k.clone()).collect();
    keys.iter().for_each(|k| {
        slots.remove_key(k);
    });
    Ok(keys.len())
}
