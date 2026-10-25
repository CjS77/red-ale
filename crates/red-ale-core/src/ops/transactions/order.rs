//! GnuCash's sort orders for transactions and splits, used by listings and find results.
//!
//! GnuCash compares text with `g_utf8_collate`, which follows the user's locale. red-ale compares by Unicode code
//! point (the C locale's order for UTF-8), so ties broken by text can sort differently from a GnuCash running in
//! another locale (`design/notes/engine-accounts-transactions.md` §11, question 2).

use std::cmp::Ordering;

use crate::{Split, Transaction};

/// `order_by_int64_or_string`: when both strings start with a non-zero number, compare the numbers and then the rest;
/// otherwise compare the whole strings (`5.17:libgnucash/engine/Transaction.cpp:1773-1791`).
pub fn num_order(a: &str, b: &str) -> Ordering {
    match (leading_number(a), leading_number(b)) {
        (Some((na, rest_a)), Some((nb, rest_b))) => na.cmp(&nb).then_with(|| rest_a.cmp(rest_b)),
        _ => a.cmp(b),
    }
}

/// The non-zero number `strtoull` reads at the start of `s` (after spaces and a `+`), with the text after it.
/// `strtoull` also accepts `-`, negating; GnuCash then compares such nums as huge unsigned numbers, which red-ale
/// does too by wrapping.
fn leading_number(s: &str) -> Option<(u64, &str)> {
    let trimmed = s.trim_start_matches([' ', '\t', '\n', '\r', '\x0b', '\x0c']);
    let (negative, body) = match trimmed.as_bytes().first() {
        Some(b'-') => (true, &trimmed[1..]),
        Some(b'+') => (false, &trimmed[1..]),
        _ => (false, trimmed),
    };
    let digits = body.bytes().take_while(u8::is_ascii_digit).count();
    if digits == 0 {
        return None;
    }
    let n = body[..digits].bytes().fold(0_u64, |acc, d| acc.saturating_mul(10).saturating_add(u64::from(d - b'0')));
    let n = if negative { n.wrapping_neg() } else { n };
    (n != 0).then(|| (n, &body[digits..]))
}

/// `xaccTransOrder`: posted time, closing transactions last, num, entered time, description, GUID
/// (`5.17:libgnucash/engine/Transaction.cpp:1793-1835`).
pub fn trans_order(a: &Transaction, b: &Transaction) -> Ordering {
    a.date_posted
        .cmp(&b.date_posted)
        .then_with(|| a.is_closing().cmp(&b.is_closing()))
        .then_with(|| num_order(a.num.as_deref().unwrap_or(""), b.num.as_deref().unwrap_or("")))
        .then_with(|| a.date_entered.cmp(&b.date_entered))
        .then_with(|| a.description.as_deref().unwrap_or("").cmp(b.description.as_deref().unwrap_or("")))
        .then_with(|| a.guid.cmp(&b.guid))
}

/// `xaccSplitOrder`: transaction order, then memo, action, reconcile flag (as a byte), amount, value, reconcile date
/// and GUID (`5.17:libgnucash/engine/Split.cpp:1535-1597`). With `num_action` (the book's num field source), the
/// transactions' nums are replaced by the two splits' actions (`:1550-1556`).
pub fn split_order(ta: &Transaction, sa: &Split, tb: &Transaction, sb: &Split, num_action: bool) -> Ordering {
    let text = |s: &Option<String>| s.as_deref().unwrap_or("").to_owned();
    let by_txn = if num_action {
        crate::ops::lots::trans_order(ta, Some(&text(&sa.action)), tb, Some(&text(&sb.action)))
    } else {
        trans_order(ta, tb)
    };
    by_txn
        .then_with(|| text(&sa.memo).cmp(&text(&sb.memo)))
        .then_with(|| text(&sa.action).cmp(&text(&sb.action)))
        .then_with(|| sa.reconcile_state.as_char().cmp(&sb.reconcile_state.as_char()))
        .then_with(|| sa.amount.cmp_value(sb.amount))
        .then_with(|| sa.value.cmp_value(sb.value))
        .then_with(|| sa.reconcile_date.unwrap_or_default().cmp(&sb.reconcile_date.unwrap_or_default()))
        .then_with(|| sa.guid.cmp(&sb.guid))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nums_compare_as_numbers_when_both_are() {
        assert_eq!(num_order("10", "9"), Ordering::Greater);
        assert_eq!(num_order("10a", "10b"), Ordering::Less);
        assert_eq!(num_order("0", "1"), Ordering::Less, "zero collates as a string");
        assert_eq!(num_order("abc", "10"), Ordering::Greater);
        assert_eq!(num_order("", "1"), Ordering::Less);
    }
}
