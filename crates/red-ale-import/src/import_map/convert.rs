//! Old Bayesian data to the flat form (`5.17:libgnucash/engine/Account.cpp:5348-5507`).
//!
//! Before GnuCash 2.7 the Bayesian map was nested frames, `import-map-bayes/<token…>/<account>` holding counts, with
//! the account by full name (by GUID in 2.7 betas) and a token holding `/` split into deeper frames. GnuCash converts
//! a Book the first time it reads or writes Bayesian data, unless the Book has the flat feature.
//!
//! GnuCash runs the conversion once per process (`imap_convert_bayes_to_flat_run`, `:61`). red-ale decides per Book
//! from the feature and the data: a Book with no old data is not changed, as in GnuCash.

use std::borrow::Cow;

use red_ale_core::book_options::features;
use red_ale_core::ops::accounts::descendants_sorted;
use red_ale_core::ops::business::contacts::set_feature_used;
use red_ale_core::{Account, Book, DomainResult, Frame, Guid, SlotValue};

use super::{IMAP_FRAME_BAYES, slots_mut};

/// The Book feature GnuCash sets with flat Bayesian data (`GNC_FEATURE_GUID_FLAT_BAYESIAN`,
/// `5.17:libgnucash/engine/gnc-features.h:59`).
pub const FLAT_BAYESIAN_FEATURE: &str = "Account GUID based bayesian with flat KVP";

/// GnuCash's account separator. GnuCash takes it from a preference; red-ale's account names always use `:`.
const SEPARATOR: u8 = b':';

/// True when the Book declares flat Bayesian data (`gnc_features_check_used`).
pub fn has_flat_feature(book: &Book) -> bool {
    features(book).iter().any(|(name, _)| name == FLAT_BAYESIAN_FEATURE)
}

/// `check_import_map_data` and `imap_convert_bayes_to_flat` (`Account.cpp:5458-5507`): unless the Book declares flat
/// data, convert every account's old data, in sorted tree order, and set the feature when any account was converted.
/// Returns whether anything changed.
pub fn check_import_map_data(book: &mut Book) -> DomainResult<bool> {
    if has_flat_feature(book) {
        return Ok(false);
    }
    let converted: Vec<(Guid, Frame)> = descendants_sorted(book, book.root)
        .into_iter()
        .filter_map(|a| converted_slots(book, a).map(|s| (a.guid, s)))
        .collect();
    if converted.is_empty() {
        return Ok(false);
    }
    converted
        .into_iter()
        .try_for_each(|(guid, slots)| slots_mut(book, guid).map(|s| *s = slots))?;
    set_feature_used(book, FLAT_BAYESIAN_FEATURE);
    Ok(true)
}

/// An account's Slots as GnuCash would see them after converting: the Slots themselves when the Book declares flat
/// data or the account has nothing to convert.
pub fn bayes_view<'a>(book: &Book, acc: &'a Account) -> Cow<'a, Frame> {
    if has_flat_feature(book) {
        return Cow::Borrowed(&acc.slots);
    }
    converted_slots(book, acc).map_or(Cow::Borrowed(&acc.slots), Cow::Owned)
}

/// `convert_imap_account_bayes_to_flat` (`Account.cpp:5438-5456`): the account's Slots with the nested
/// `import-map-bayes` frame replaced by flat keys, or `None` when it has no such frame or the frame holds no value.
/// Colliding keys overwrite, in the flattened order.
fn converted_slots(book: &Book, acc: &Account) -> Option<Frame> {
    let nested = acc.slots.get_key(IMAP_FRAME_BAYES)?.as_frame()?;
    let mut entries = Vec::new();
    flatten(nested, &mut Vec::new(), &mut entries);
    if entries.is_empty() {
        return None;
    }
    let mut slots = acc.slots.clone();
    slots.remove_key(IMAP_FRAME_BAYES);
    entries.into_iter().for_each(|(parts, value)| {
        slots.insert_key(convert_entry(book, parts), value);
    });
    Some(slots)
}

/// `KvpFrame::flatten_kvp` (`5.17:libgnucash/engine/kvp-frame.cpp:324-349`): every non-frame value with its path, each
/// key preceded by a `/` part.
fn flatten(frame: &Frame, path: &mut Vec<String>, out: &mut Vec<(Vec<String>, SlotValue)>) {
    frame.iter().for_each(|(key, value)| {
        path.push("/".to_owned());
        path.push(key.clone());
        match value {
            SlotValue::Frame(inner) => flatten(inner, path, out),
            other => out.push((path.clone(), other.clone())),
        }
        path.truncate(path.len() - 2);
    });
}

/// `convert_entry` (`Account.cpp:5397-5415`): a last part that is not a GUID is an account's full name, replaced by
/// that account's GUID; the parts are joined to `import-map-bayes/<token…>/<guid>`.
fn convert_entry(book: &Book, mut parts: Vec<String>) -> String {
    if let Some(last) = parts.last_mut().filter(|name| !is_valid_guid(name)) {
        *last = guid_from_account_name(book, last).to_string();
    }
    std::iter::once(IMAP_FRAME_BAYES.to_owned()).chain(parts).collect()
}

/// `gnc::GUID::is_valid_guid` (`5.17:libgnucash/engine/guid.cpp:373-388`): 32 hex digits, or boost's dashed form
/// (8-4-4-4-12), optionally in braces.
fn is_valid_guid(s: &str) -> bool {
    let inner = s.strip_prefix('{').and_then(|t| t.strip_suffix('}')).unwrap_or(s);
    let hex = |part: &str| part.bytes().all(|b| b.is_ascii_hexdigit());
    match inner.len() {
        32 => hex(inner),
        36 => inner.split('-').map(str::len).eq([8, 4, 4, 4, 12]) && inner.split('-').all(hex),
        _ => false,
    }
}

/// `get_guid_from_account_name` (`Account.cpp:5383-5395`): the account by full name, else after guessing and
/// replacing an old separator; a name that still resolves to nothing gives the all-zero GUID
/// (`xaccAccountGetGUID(nullptr)`, `5.17:libgnucash/engine/qofinstance.cpp:457-460`), which GnuCash keeps.
fn guid_from_account_name(book: &Book, name: &str) -> Guid {
    book.account_by_full_name(name)
        .or_else(|| with_current_separator(book, name).and_then(|n| book.account_by_full_name(&n)))
        .map_or(Guid::NULL, |a| a.guid)
}

/// `look_for_old_separator_descendants` (`Account.cpp:5348-5381`): over every account (not only the top level, despite
/// GnuCash's comment), the longest name that starts `full_name` and is followed there by a byte that is not ASCII
/// alphanumeric names the old separator; when that name is longer than one byte, every occurrence of the separator
/// becomes `:`. `None` when the bytes replaced leave no valid UTF-8, which then names no account.
fn with_current_separator(book: &Book, full_name: &str) -> Option<String> {
    let bytes = full_name.as_bytes();
    let found = book
        .descendants(book.root)
        .into_iter()
        .map(|a| a.name.as_bytes())
        .filter(|name| bytes.starts_with(name))
        .map(|name| (name.len(), bytes.get(name.len()).copied().unwrap_or(0)))
        .filter(|(_, sep)| !sep.is_ascii_alphanumeric())
        .fold((0, 0), |best, (len, sep)| if len > best.0 { (len, sep) } else { best });
    let (found_len, found_sep) = found;
    if found_len <= 1 {
        return Some(full_name.to_owned());
    }
    String::from_utf8(bytes.iter().map(|b| if *b == found_sep { SEPARATOR } else { *b }).collect()).ok()
}
