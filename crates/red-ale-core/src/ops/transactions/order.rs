//! GnuCash's sort orders for transactions and splits, used by listings, find results, ledgers and report queries.
//!
//! GnuCash compares text with `g_utf8_collate`, which follows the user's locale. red-ale compares by Unicode code
//! point (the C locale's order for UTF-8), so ties broken by text can sort differently from a GnuCash running in
//! another locale (`design/notes/engine-accounts-transactions.md` §11, question 2).
//!
//! GnuCash's orders are not total orders, and Rust's sorts may panic on a comparator that is not one. Two rules form
//! cycles: `order_by_int64_or_string` (see [`num_order`]) and, with the num field source on, `xaccSplitOrder`'s
//! comparison of two splits of one transaction (see [`split_order`]). The orders here are total, and differ from
//! GnuCash's only on the pairs those cycles run through (`design/reports.md` §14).

use std::cmp::Ordering;

use crate::{Numeric, Split, Transaction};

/// Where a num sorts: a total order that agrees with `order_by_int64_or_string`
/// (`5.17:libgnucash/engine/Transaction.cpp:1773-1791`) on every pair but one kind.
///
/// GnuCash compares two nums by their leading numbers (`strtoull`) and then the rest when both have a non-zero one,
/// and otherwise as whole strings. A num with a non-zero number that does not start with a digit from 1 to 9 (`" 10"`,
/// `"+5"`, `"-3"`, `"05"`) can sort below, as text, a num with none (`"0"`, `"0a"`, `" "`) that in turn sorts below,
/// as text, a num whose number is smaller: `"-3" < "0" < "5" < "-3"` (`strtoull` reads `"-3"` as 2⁶⁴ − 3). Here every
/// num with no non-zero number that sorts as text below `"1"` comes first, by text; then the nums with a number, by
/// number and then the rest; then the others (those starting above `"9"`), by text. That agrees with GnuCash on every
/// pair except a num with a number that starts below `"1"` against a num with none that starts below `"1"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum NumKey<'a> {
    /// No non-zero leading number, sorting as text below every num that starts with a digit from 1.
    Low(&'a str),
    /// A non-zero leading number and the text after it.
    Number(u64, &'a str),
    /// No non-zero leading number, sorting as text above every num with one.
    High(&'a str),
}

fn num_key(s: &str) -> NumKey<'_> {
    match leading_number(s) {
        Some((n, rest)) => NumKey::Number(n, rest),
        None if s.as_bytes().first().is_none_or(|b| *b < b'1') => NumKey::Low(s),
        None => NumKey::High(s),
    }
}

/// `order_by_int64_or_string`, made a total order (see [`NumKey`]): when both strings start with a non-zero number,
/// compare the numbers and then the rest; otherwise compare the whole strings
/// (`5.17:libgnucash/engine/Transaction.cpp:1773-1791`).
pub fn num_order(a: &str, b: &str) -> Ordering {
    num_key(a).cmp(&num_key(b))
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
    let n = body[..digits]
        .bytes()
        .fold(0_u64, |acc, d| acc.saturating_mul(10).saturating_add(u64::from(d - b'0')));
    let n = if negative { n.wrapping_neg() } else { n };
    (n != 0).then(|| (n, &body[digits..]))
}

/// `xaccTransOrder_num_action`'s keys (`5.17:libgnucash/engine/Transaction.cpp:1794-1838`) with `num_a` and `num_b`
/// standing for the two nums: posted time, closing transactions last, num, entered time, description, GUID.
fn trans_keys(ta: &Transaction, num_a: &str, tb: &Transaction, num_b: &str) -> Ordering {
    ta.date_posted
        .cmp(&tb.date_posted)
        .then_with(|| ta.is_closing().cmp(&tb.is_closing()))
        .then_with(|| num_order(num_a, num_b))
        .then_with(|| ta.date_entered.cmp(&tb.date_entered))
        .then_with(|| ta.description.as_deref().unwrap_or("").cmp(tb.description.as_deref().unwrap_or("")))
        .then_with(|| ta.guid.cmp(&tb.guid))
}

/// `xaccTransOrder`: posted time, closing transactions last, num, entered time, description, GUID
/// (`5.17:libgnucash/engine/Transaction.cpp:1757-1761`, `:1794-1838`).
pub fn trans_order(a: &Transaction, b: &Transaction) -> Ordering {
    trans_keys(a, a.num.as_deref().unwrap_or(""), b, b.num.as_deref().unwrap_or(""))
}

/// `xaccSplitOrder`, made a total order: transaction order, then memo, action, reconcile flag (as a byte), amount,
/// value, reconcile date and GUID (`5.17:libgnucash/engine/Split.cpp:1536-1597`). With `num_action` (the book's num
/// field source) each split's action stands for its transaction's num (`:1548-1556`).
///
/// GnuCash compares two splits of one transaction by the split keys alone (`xaccTransOrder_num_action` returns 0 for
/// the same transaction, `5.17:libgnucash/engine/Transaction.cpp:1800`). With `num_action` that is not a total order:
/// splits A1 < A2 of one transaction by memo, with A2's action sorting below A1's, form a cycle with a split B of
/// another transaction on the same day whose action sorts between theirs (A2 < B by date entered, B < A1 by action).
/// Here two splits of one transaction compare their actions as nums first, as splits of different transactions do,
/// and then the split keys. Pairs of splits from different transactions are ordered as GnuCash orders them; so are
/// two splits of one transaction without `num_action`, or when their actions are equal as nums.
pub fn split_order(ta: &Transaction, sa: &Split, tb: &Transaction, sb: &Split, num_action: bool) -> Ordering {
    let num = |t: &Transaction, s: &Split| text(if num_action { s.action.as_ref() } else { t.num.as_ref() }).to_owned();
    trans_keys(ta, &num(ta, sa), tb, &num(tb, sb))
        .then_with(|| text(sa.memo.as_ref()).cmp(text(sb.memo.as_ref())))
        .then_with(|| text(sa.action.as_ref()).cmp(text(sb.action.as_ref())))
        .then_with(|| sa.reconcile_state.as_char().cmp(&sb.reconcile_state.as_char()))
        .then_with(|| numeric_order(sa.amount, sb.amount))
        .then_with(|| numeric_order(sa.value, sb.value))
        .then_with(|| sa.reconcile_date.unwrap_or_default().cmp(&sb.reconcile_date.unwrap_or_default()))
        .then_with(|| sa.guid.cmp(&sb.guid))
}

fn text(s: Option<&String>) -> &str {
    s.map_or("", String::as_str)
}

/// `gnc_numeric_compare` for valid numbers; an invalid one (denominator 0) sorts first, where GnuCash's compare
/// treats it as equal to every number and so is not an order.
pub fn numeric_order(a: Numeric, b: Numeric) -> Ordering {
    match (a.is_valid(), b.is_valid()) {
        (true, true) => a.cmp_value(b),
        (va, vb) => va.cmp(&vb),
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;
    use crate::{CommodityRef, Frame, Guid, Numeric, ReconcileState, SlotValue, Time64};

    /// Nums that tell `order_by_int64_or_string`'s cases apart, those that make it cycle among them.
    const NUMS: [&str; 16] = [
        "", "0", "00", "0a", "05", "-3", "+5", " 10", "9", "10", "10a", "abc", "+0", "-", " ", "1",
    ];

    fn assert_total<T>(items: &[T], cmp: impl Fn(&T, &T) -> Ordering) -> Result<(), TestCaseError> {
        let n = items.len();
        (0..n).try_for_each(|i| {
            (0..n).try_for_each(|j| {
                prop_assert_eq!(
                    cmp(&items[i], &items[j]),
                    cmp(&items[j], &items[i]).reverse(),
                    "antisymmetric {} {}",
                    i,
                    j
                );
                (0..n).try_for_each(|k| {
                    let (ij, jk, ik) = (cmp(&items[i], &items[j]), cmp(&items[j], &items[k]), cmp(&items[i], &items[k]));
                    if ij.is_le() && jk.is_le() {
                        prop_assert!(ik.is_le(), "transitive {} {} {}", i, j, k);
                    }
                    if ij.is_eq() && jk.is_eq() {
                        prop_assert!(ik.is_eq(), "equal is transitive {} {} {}", i, j, k);
                    }
                    Ok(())
                })
            })
        })
    }

    #[test]
    fn nums_compare_as_numbers_when_both_are() {
        assert_eq!(num_order("10", "9"), Ordering::Greater);
        assert_eq!(num_order("10a", "10b"), Ordering::Less);
        assert_eq!(num_order("0", "1"), Ordering::Less, "zero collates as a string");
        assert_eq!(num_order("abc", "10"), Ordering::Greater);
        assert_eq!(num_order("", "1"), Ordering::Less);
        assert_eq!(num_order("-3", "5"), Ordering::Greater, "strtoull wraps a negative number");
    }

    #[test]
    fn num_order_is_a_total_order() {
        // GnuCash: "-3" < "0" and "0" < "5" as text, but "5" < "-3" as numbers.
        assert_total(&NUMS, |a, b| num_order(a, b)).unwrap();
    }

    proptest! {
        #[test]
        fn random_nums_are_totally_ordered(nums in prop::collection::vec("[ +\\-0-9a]{0,3}", 1..12)) {
            assert_total(&nums, |a, b| num_order(a, b))?;
        }
    }

    fn guid(n: u64) -> Guid {
        Guid::from_bytes(u128::from(n).to_be_bytes())
    }

    /// A transaction: posted, closing, num, entered, description; its splits: memo, action, reconcile, amount.
    type TxnSpec = (i64, bool, String, i64, String, Vec<(String, String, char, i64)>);

    fn transaction(i: usize, (posted, closing, num, entered, description, splits): &TxnSpec) -> Transaction {
        let n = u64::try_from(i).unwrap();
        let mut slots = Frame::new();
        if *closing {
            slots.set("book_closing", SlotValue::Integer(1));
        }
        let splits = splits
            .iter()
            .enumerate()
            .map(|(j, (memo, action, state, amount))| {
                let amount = Numeric::new(*amount, 1);
                let mut s = Split::new(guid(1000 + 10 * n + u64::try_from(j).unwrap()), guid(1), amount, amount);
                s.memo = Some(memo.clone());
                s.action = Some(action.clone());
                s.reconcile_state = ReconcileState::from_char(*state).unwrap();
                s
            })
            .collect();
        Transaction {
            guid: guid(n),
            currency: CommodityRef::currency("USD"),
            num: Some(num.clone()),
            date_posted: Time64(*posted),
            date_entered: Time64(*entered),
            description: Some(description.clone()),
            slots,
            splits,
        }
    }

    fn txn_spec() -> impl Strategy<Value = TxnSpec> {
        let text = || prop::sample::select(vec!["", "a", "b", "1", "2", "10", "-3", "0"]).prop_map(str::to_owned);
        let split = (text(), text(), prop::sample::select(vec!['n', 'c', 'y']), 0_i64..3);
        (
            0_i64..2,
            any::<bool>(),
            text(),
            0_i64..3,
            text(),
            prop::collection::vec(split, 1..4),
        )
    }

    fn pairs(txns: &[Transaction]) -> Vec<(&Transaction, &Split)> {
        txns.iter().flat_map(|t| t.splits.iter().map(move |s| (t, s))).collect()
    }

    proptest! {
        #[test]
        fn split_order_is_a_total_order(specs in prop::collection::vec(txn_spec(), 1..6), num_action in any::<bool>()) {
            let txns: Vec<Transaction> = specs.iter().enumerate().map(|(i, s)| transaction(i, s)).collect();
            let splits = pairs(&txns);
            assert_total(&splits, |(ta, sa), (tb, sb)| split_order(ta, sa, tb, sb, num_action))?;
            let ts: Vec<&Transaction> = txns.iter().collect();
            assert_total(&ts, |a, b| trans_order(a, b))?;
        }
    }

    /// GnuCash's `xaccSplitOrder` as written (`5.17:libgnucash/engine/Split.cpp:1536-1597`), with its
    /// `order_by_int64_or_string`, for comparing with the total order.
    fn gnucash_split_order(ta: &Transaction, sa: &Split, tb: &Transaction, sb: &Split, num_action: bool) -> Ordering {
        let text = |s: &Option<String>| s.as_deref().unwrap_or("").to_owned();
        let gnucash_num = |a: &str, b: &str| match (leading_number(a), leading_number(b)) {
            (Some((na, ra)), Some((nb, rb))) => na.cmp(&nb).then_with(|| ra.cmp(rb)),
            _ => a.cmp(b),
        };
        let by_txn = if ta.guid == tb.guid {
            Ordering::Equal
        } else {
            let (na, nb) = if num_action {
                (text(&sa.action), text(&sb.action))
            } else {
                (text(&ta.num), text(&tb.num))
            };
            ta.date_posted
                .cmp(&tb.date_posted)
                .then_with(|| ta.is_closing().cmp(&tb.is_closing()))
                .then_with(|| gnucash_num(&na, &nb))
                .then_with(|| ta.date_entered.cmp(&tb.date_entered))
                .then_with(|| text(&ta.description).cmp(&text(&tb.description)))
                .then_with(|| ta.guid.cmp(&tb.guid))
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

    /// True when `num_order` departs from `order_by_int64_or_string` on this pair: a num with a number that starts
    /// below "1" against one without that starts below "1".
    fn documented_num_exception(a: &str, b: &str) -> bool {
        let below = |s: &str| s.as_bytes().first().is_none_or(|c| *c < b'1');
        let odd = |x: &str, y: &str| leading_number(x).is_some() && leading_number(y).is_none() && below(x) && below(y);
        odd(a, b) || odd(b, a)
    }

    proptest! {
        #[test]
        fn split_order_agrees_with_gnucash_but_where_documented(
            specs in prop::collection::vec(txn_spec(), 1..6),
            num_action in any::<bool>(),
        ) {
            let txns: Vec<Transaction> = specs.iter().enumerate().map(|(i, s)| transaction(i, s)).collect();
            let splits = pairs(&txns);
            let text = |s: &Option<String>| s.as_deref().unwrap_or("").to_owned();
            let num = |t: &Transaction, s: &Split| if num_action { text(&s.action) } else { text(&t.num) };
            splits.iter().try_for_each(|(ta, sa)| {
                splits.iter().try_for_each(|(tb, sb)| {
                    let same_txn = ta.guid == tb.guid;
                    let excepted = documented_num_exception(&num(ta, sa), &num(tb, sb))
                        || (same_txn && num_action && num_order(&num(ta, sa), &num(tb, sb)).is_ne());
                    if !excepted {
                        prop_assert_eq!(split_order(ta, sa, tb, sb, num_action), gnucash_split_order(ta, sa, tb, sb, num_action));
                    }
                    Ok(())
                })
            })?;
        }
    }

    /// The cycle of `split_order`'s documentation, many times over on one day: A1 (action "a") < A2 (memo "veg") by
    /// memo, A2 < B by date entered, B (action "") < A1 by action.
    fn cycle_transactions(n: usize) -> Vec<Transaction> {
        let spec = |i: usize| -> TxnSpec {
            let entered = i64::try_from(i).unwrap();
            let split = |memo: &str, action: &str| (memo.to_owned(), action.to_owned(), 'n', 1);
            let splits = if i.is_multiple_of(2) {
                vec![split("", "a"), split("veg", "")]
            } else {
                vec![split("", "")]
            };
            (0, false, String::new(), entered, String::new(), splits)
        };
        (0..n).map(|i| transaction(i, &spec(i))).collect()
    }

    #[test]
    fn a_num_source_cycle_sorts_without_panicking_whatever_the_order_it_starts_in() {
        let txns = cycle_transactions(20);
        let splits = pairs(&txns);
        let sort = |mut v: Vec<(&Transaction, &Split)>| {
            v.sort_by(|(ta, sa), (tb, sb)| split_order(ta, sa, tb, sb, true));
            v.iter().map(|(_, s)| s.guid).collect::<Vec<_>>()
        };
        let first = sort(splits.clone());
        // A fixed linear congruential shuffle: the outcome must not depend on the input order.
        (0..300_u64).for_each(|seed| {
            let mut v = splits.clone();
            let mut x = seed;
            (1..v.len()).rev().for_each(|i| {
                x = x.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
                v.swap(i, usize::try_from(x >> 33).unwrap() % (i + 1));
            });
            assert_eq!(sort(v), first, "seed {seed}");
        });
    }

    #[test]
    fn every_sort_on_split_order_takes_a_num_source_cycle() {
        use crate::ops::commit::{NewSplit, NewTransaction};
        use crate::ops::transactions::find::{PeriodQuery, Query, find, period_splits};
        use crate::ops::transactions::sort::{SortKey, SortLevel, sort_splits};
        let mut f = super::super::tests::fixture();
        crate::book_options::set_split_action_for_num(&mut f.book, true);
        let (bank, food) = (f.bank, f.food);
        (0..20_i64).for_each(|i| {
            let day = crate::Date::from_ymd(2024, 6, 1).unwrap();
            let mut new = NewTransaction::new(CommodityRef::currency("USD"), day);
            new.date_entered = Some(Time64(i));
            let usd = |c: i64| Numeric::new(c, 100);
            new.splits = if i % 2 == 0 {
                vec![
                    NewSplit::new(bank, usd(-100)).with_action("a"),
                    NewSplit::new(bank, usd(-100)).with_memo("veg"),
                    NewSplit::new(food, usd(200)),
                ]
            } else {
                vec![NewSplit::new(bank, usd(-100)), NewSplit::new(food, usd(100))]
            };
            super::super::post(&mut f.book, &mut f.env, new).unwrap();
        });
        let book = &f.book;
        let in_order = |v: &[(&Transaction, &Split)]| v.is_sorted_by(|(ta, sa), (tb, sb)| split_order(ta, sa, tb, sb, true).is_le());
        let query = PeriodQuery {
            accounts: vec![bank],
            ..PeriodQuery::default()
        };
        let found = period_splits(book, &query).unwrap();
        assert_eq!(found.len(), 30);
        assert!(in_order(&found));
        let default = [SortLevel {
            key: Some(SortKey::Default),
            ascending: false,
        }];
        assert!(
            sort_splits(book, &default, found.clone())
                .iter()
                .rev()
                .is_sorted_by(|(ta, sa), (tb, sb)| split_order(ta, sa, tb, sb, true).is_le())
        );
        assert_eq!(find(book, &Query::All(vec![]), None, &crate::TimeZone::utc()).unwrap().len(), 50);
        assert!(in_order(&crate::ops::balances::account_splits(book, bank)));
        assert_eq!(crate::ops::balances::running_balances(book, bank).unwrap().len(), 30);
        assert_eq!(crate::ops::lots::Ledger::build(book, bank).unwrap().entries.len(), 30);
    }
}
