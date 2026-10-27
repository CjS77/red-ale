//! The Match score of one candidate split (`split_find_match`, `5.17:gnucash/import-export/import-backend.cpp:579-784`).

use red_ale_core::book_options::uses_split_action_for_num;
use red_ale_core::{Book, Numeric, Split, Transaction};

use super::{ImportSettings, MatchInfo};

/// Seconds in a day, as GnuCash divides by them (`import-backend.cpp:631`).
const SECS_PER_DAY: i64 = 86_400;

/// Score `candidate` (a split of `candidate_txn`, in the imported split's account) against the imported transaction
/// `imported`, whose split 0 is the imported split. `None` when the score is below `match_threshold`.
///
/// | Test | Points |
/// |------|-------:|
/// | Amounts equal | +3 |
/// | Else at most `atm_fee_threshold` apart | +2 |
/// | Else | −5 |
/// | Posted the same day (whole days of the seconds between, truncated) | +3 |
/// | Else at most `match_date_threshold` days apart | +2 |
/// | Else more than `match_date_not_threshold` days apart | −5 |
/// | Both numbers set and equal, as text or as `strtol` integers | +4 |
/// | Both set and different | −2 |
/// | Both memos set and equal, case-folded | +2, else +1 for a half-prefix match |
/// | Both descriptions set and equal, case-folded | +2, else +1 for a half-prefix match |
///
/// `update_proposed` is the score after the amount and date tests being below 6 (`:666`).
///
/// **Deviation** (`design/api/import.md`): GnuCash compares the amounts as doubles, equal within 1e-6; red-ale
/// compares them exactly, and the ATM fee threshold exactly.
pub fn split_find_match(
    book: &Book,
    imported: &Transaction,
    candidate_txn: &Transaction,
    candidate: &Split,
    settings: &ImportSettings,
) -> Option<MatchInfo> {
    let fsplit = imported.splits.first()?;
    let mut prob = amount_points(fsplit.amount, candidate.amount, settings.atm_fee_threshold);
    prob += date_points(imported, candidate_txn, settings);
    let update_proposed = prob < 6;
    let num_action = uses_split_action_for_num(book);
    prob += number_points(num(imported, fsplit, num_action), num(candidate_txn, candidate, num_action));
    prob += text_points(fsplit.memo.as_deref(), candidate.memo.as_deref());
    prob += text_points(imported.description.as_deref(), candidate_txn.description.as_deref());
    (prob >= settings.match_threshold).then_some(MatchInfo {
        split: candidate.guid,
        transaction: candidate_txn.guid,
        probability: prob,
        update_proposed,
    })
}

/// The amount test (`import-backend.cpp:590-626`), exact.
fn amount_points(imported: Numeric, existing: Numeric, atm_fee: Numeric) -> i32 {
    match abs_difference_at_most(imported, existing, Numeric::ZERO) {
        Some(true) => 3,
        _ if abs_difference_at_most(imported, existing, atm_fee) == Some(true) => 2,
        _ => -5,
    }
}

/// Whether `|a − b| ≤ limit`, exactly, in 128 bits; `None` for an invalid number or when the comparison would
/// overflow, which then scores as far apart.
fn abs_difference_at_most(a: Numeric, b: Numeric, limit: Numeric) -> Option<bool> {
    if !a.is_valid() || !b.is_valid() || !limit.is_valid() {
        return None;
    }
    let (an, ad, bn, bd) = (
        i128::from(a.num()),
        i128::from(a.denom()),
        i128::from(b.num()),
        i128::from(b.denom()),
    );
    let (ln, ld) = (i128::from(limit.num()), i128::from(limit.denom()));
    // |an/ad − bn/bd| ≤ ln/ld  ⇔  |an·bd − bn·ad| · ld ≤ ln · ad · bd, with every denominator positive.
    let difference = an.checked_mul(bd)?.checked_sub(bn.checked_mul(ad)?)?.checked_abs()?;
    Some(difference.checked_mul(ld)? <= ln.checked_mul(ad)?.checked_mul(bd)?)
}

/// The date test (`import-backend.cpp:628-663`).
fn date_points(imported: &Transaction, existing: &Transaction, settings: &ImportSettings) -> i32 {
    let days = existing.date_posted.0.abs_diff(imported.date_posted.0) / SECS_PER_DAY.unsigned_abs();
    if days == 0 {
        3
    } else if days <= u64::try_from(settings.match_date_threshold).unwrap_or(0) {
        2
    } else if i128::from(days) > i128::from(settings.match_date_not_threshold) {
        -5
    } else {
        0
    }
}

/// `gnc_get_num_action(trans, split)` (`5.17:libgnucash/engine/engine-helpers.c:56-72`): the split's action when the
/// Book uses split actions for numbers, else the transaction's num.
fn num<'a>(t: &'a Transaction, s: &'a Split, num_action: bool) -> &'a str {
    if num_action { s.action.as_deref() } else { t.num.as_deref() }.unwrap_or("")
}

/// The number test (`import-backend.cpp:668-701`). GnuCash's bug is kept: `conversion_ok` is overwritten by the
/// second conversion, so only the existing number's validity counts, and an imported `abc` (0) equals an existing `0`.
fn number_points(imported: &str, existing: &str) -> i32 {
    if imported.is_empty() || existing.is_empty() {
        return 0;
    }
    let (imported_number, _) = strtol(imported);
    let (existing_number, conversion_ok) = strtol(existing);
    if (conversion_ok && existing_number == imported_number) || imported == existing {
        4
    } else {
        -2
    }
}

/// C's `strtol(s, &end, 10)` with GnuCash's success test `!(errno || end == s)`: leading white space, an optional
/// sign, then decimal digits; no digit is a failure giving 0, and an overflow a failure giving the saturated value.
fn strtol(s: &str) -> (i64, bool) {
    let body = s.trim_start_matches([' ', '\t', '\n', '\x0b', '\x0c', '\r']);
    let (negative, digits) = match body.as_bytes().first() {
        Some(b'-') => (true, &body[1..]),
        Some(b'+') => (false, &body[1..]),
        _ => (false, body),
    };
    let digits: Vec<i64> = digits.bytes().take_while(u8::is_ascii_digit).map(|d| i64::from(d - b'0')).collect();
    if digits.is_empty() {
        return (0, false);
    }
    let sign = if negative { -1 } else { 1 };
    let value = digits.iter().try_fold(0_i64, |acc, d| acc.checked_mul(10)?.checked_add(sign * d));
    match value {
        Some(v) => (v, true),
        None if negative => (i64::MIN, false),
        None => (i64::MAX, false),
    }
}

/// The memo and description tests (`import-backend.cpp:703-761`): both set, then +2 when equal case-folded, else +1
/// when both are longer than one byte and the first half of the longer one's bytes match, ASCII case-insensitive.
///
/// **Deviation** (`design/api/import.md`): GnuCash's `safe_strcasecmp` folds with `g_utf8_casefold` and collates by
/// locale; red-ale lowercases and compares bytewise, which differs only for the few characters whose folding is not
/// their lowercase (such as `ß`) and for distinct strings a locale collates as equal.
fn text_points(imported: Option<&str>, existing: Option<&str>) -> i32 {
    let (Some(a), Some(b)) = (imported.filter(|s| !s.is_empty()), existing.filter(|s| !s.is_empty())) else {
        return 0;
    };
    if a.to_lowercase() == b.to_lowercase() {
        return 2;
    }
    i32::from(a.len() > 1 && b.len() > 1 && strncasecmp_equal(a.as_bytes(), b.as_bytes(), a.len().max(b.len()) / 2))
}

/// `strncasecmp(a, b, n) == 0` in the C locale: the first `n` bytes equal ignoring ASCII case, the end of a string
/// counting as a NUL byte that ends the comparison.
fn strncasecmp_equal(a: &[u8], b: &[u8], n: usize) -> bool {
    (0..n)
        .map(|i| {
            (
                a.get(i).copied().unwrap_or(0).to_ascii_lowercase(),
                b.get(i).copied().unwrap_or(0).to_ascii_lowercase(),
            )
        })
        .find(|(x, y)| x != y || *x == 0)
        .is_none_or(|(x, y)| x == y)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strtol_reads_as_c_does() {
        assert_eq!(strtol("123"), (123, true));
        assert_eq!(strtol("  -42abc"), (-42, true));
        assert_eq!(strtol("+7"), (7, true));
        assert_eq!(strtol("abc"), (0, false));
        assert_eq!(strtol("-"), (0, false));
        assert_eq!(strtol("99999999999999999999"), (i64::MAX, false));
        assert_eq!(strtol("-9223372036854775808"), (i64::MIN, true));
        assert_eq!(strtol("-99999999999999999999"), (i64::MIN, false));
    }

    #[test]
    fn number_test_keeps_gnucash_conversion_bug() {
        assert_eq!(number_points("abc", "0"), 4, "only the existing number is checked for validity");
        assert_eq!(number_points("0", "abc"), -2);
        assert_eq!(number_points("abc", "abc"), 4, "equal text");
        assert_eq!(number_points("0012", "12"), 4);
        assert_eq!(number_points("12", "13"), -2);
        assert_eq!(number_points("", "13"), 0);
    }

    #[test]
    fn half_prefix_is_bytewise_ascii_case_insensitive() {
        assert!(strncasecmp_equal(b"CARD 1234", b"card 9999", 4));
        assert!(!strncasecmp_equal(b"ab", b"abc", 3), "a shorter string ends before n");
        assert!(strncasecmp_equal(b"ab", b"AB", 3), "both end together");
        assert_eq!(text_points(Some("Coffee shop 123"), Some("coffee shop 999")), 1);
        assert_eq!(text_points(Some("Coffee"), Some("COFFEE")), 2);
        assert_eq!(text_points(Some("Ä"), Some("ä")), 2, "lowercased by Unicode");
        assert_eq!(
            text_points(Some("x"), Some("xy")),
            0,
            "both must be longer than one byte for the fuzzy test"
        );
        assert_eq!(text_points(None, Some("x")), 0);
    }

    #[test]
    fn amounts_compare_exactly() {
        let atm = Numeric::new(200, 100);
        assert_eq!(amount_points(Numeric::new(-10000, 100), Numeric::new(-100, 1), atm), 3);
        assert_eq!(amount_points(Numeric::new(-10000, 100), Numeric::new(-10125, 100), atm), 2);
        assert_eq!(
            amount_points(Numeric::new(-10000, 100), Numeric::new(-10200, 100), atm),
            2,
            "the threshold is inclusive"
        );
        assert_eq!(amount_points(Numeric::new(-10000, 100), Numeric::new(-10201, 100), atm), -5);
        assert_eq!(
            amount_points(Numeric::new(1, 1_000_000_000), Numeric::ZERO, atm),
            2,
            "GnuCash's 1e-6 tolerance is not kept"
        );
        assert_eq!(
            amount_points(Numeric::new(i64::MAX, 1), Numeric::new(i64::MIN, 1), atm),
            -5,
            "overflow is far apart"
        );
    }
}
