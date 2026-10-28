//! The sort of a split query: QofQuery's primary, secondary and tertiary sort, each a parameter path and a direction
//! (`qof_query_set_sort_order`, `qof_query_set_sort_increasing`), as the Transaction Report engine sets them
//! (`5.17:gnucash/report/trep-engine.scm:2541-2549`). See `design/notes/report-transaction-engine.md` §2.4.
//!
//! Text compares by byte where GnuCash's account-name comparison uses `g_utf8_collate` (`design/reports.md` §14,
//! "Name sorting"); every other text comparison of these keys is GnuCash's own `g_strcmp0`, a byte comparison.

use std::cmp::Ordering;

use super::order::split_order;
use super::{corr_account_code, corr_account_full_name};
use crate::book_options::uses_split_action_for_num;
use crate::{Book, Guid, Numeric, Split, Time64, Transaction};

/// A sort parameter of a split query, named for the QOF parameter path it stands for. How each compares is in
/// `5.17:libgnucash/engine/qofquery.cpp:295-342` (`cmp_func`) with the parameter tables of `Split.cpp:2302-2401`,
/// `Transaction.cpp:2875-2955` and `Account.cpp:5834-5845`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortKey {
    /// `SPLIT_ACCT_FULLNAME`: `xaccSplitCompareAccountFullNames` (`5.17:libgnucash/engine/Split.cpp:1687-1709`). The
    /// same account is equal; else the account paths are walked from the top and the first two accounts that differ
    /// compare by name; an account whose path ends first (an ancestor) is less.
    AccountFullName,
    /// `SPLIT_ACCOUNT`, `ACCOUNT_CODE_`: the account codes, `""` when unset (`Account.cpp:308`), with `g_strcmp0`
    /// (`5.17:libgnucash/engine/qofquerycore.cpp:194-207`).
    AccountCode,
    /// `SPLIT_CORR_ACCT_NAME`: [`corr_account_full_name`] with `g_strcmp0` (`Split.cpp:1725-1744`).
    CorrAccountFullName,
    /// `SPLIT_CORR_ACCT_CODE`: [`corr_account_code`] with `g_strcmp0` (`Split.cpp:1746-1757`).
    CorrAccountCode,
    /// `SPLIT_TRANS`, `TRANS_DATE_POSTED`: posted times, to the second (`qofquerycore.cpp:351-366`, `:402-413`).
    DatePosted,
    /// `SPLIT_TRANS`, `TRANS_DATE_ENTERED`: entered times.
    DateEntered,
    /// `SPLIT_DATE_RECONCILED`: reconcile times, 0 when never reconciled.
    DateReconciled,
    /// `SPLIT_VALUE`: values, exactly (`gnc_numeric_compare`, `qofquerycore.cpp:542-553`).
    Value,
    /// `SPLIT_AMOUNT`: amounts, exactly.
    Amount,
    /// `SPLIT_TRANS`, `TRANS_NUM`: the parameter's own `qof_string_number_compare_func` (`Transaction.cpp:2879-2883`,
    /// `qofquerycore.cpp:234-262`): the numbers `strtol` reads at the start, then the rest with `g_strcmp0`.
    Num,
    /// `SPLIT_ACTION`: actions with `g_strcmp0`, unlike `Num` (a plain string parameter).
    Action,
    /// `SPLIT_MEMO`: memos with `g_strcmp0`.
    Memo,
    /// `SPLIT_TRANS`, `TRANS_DESCRIPTION`: descriptions with `g_strcmp0`.
    Description,
    /// `QUERY_DEFAULT_SORT`: `xaccSplitOrder`, the split order (`qofquery.cpp:519-523`; `Split.cpp:1536-1597`).
    Default,
}

/// One level of the sort: a key, or none (GnuCash's empty parameter list), and its direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SortLevel {
    /// The key; `None` compares every two splits equal (`qofquery.cpp:310-311`).
    pub key: Option<SortKey>,
    /// Ascending (QOF's "increasing").
    pub ascending: bool,
}

/// What one split sorts by at one level.
#[derive(Debug, Clone, PartialEq)]
enum SortValue<'a> {
    /// No key.
    Nothing,
    /// The account and its ancestors below the root, top first.
    Path(Vec<(Guid, &'a str)>),
    Text(String),
    Time(Time64),
    Number(Numeric),
    /// What `strtol` reads at the start of a num, and the rest.
    Num(i64, String),
    /// The split order, compared on the splits themselves.
    SplitOrder,
}

/// Sort `found`, given in split order, as `qof_query_run` sorts by `levels` (`5.17:libgnucash/engine/qofquery.cpp:345-374`,
/// `:792-797`): the first level that tells two splits apart decides, negated when it is descending. Splits tied at
/// every level keep the given (split) order, where GnuCash's merge sort keeps the order the query collected them in,
/// which is hash order. With no levels, or no key at the first level, GnuCash does not sort at all (`:492-497`) and
/// the splits stay in split order. Both are deviations (`design/reports.md` §14).
pub fn sort_splits<'a>(book: &'a Book, levels: &[SortLevel], found: Vec<(&'a Transaction, &'a Split)>) -> Vec<(&'a Transaction, &'a Split)> {
    if levels.first().is_none_or(|l| l.key.is_none()) {
        return found;
    }
    let num_action = uses_split_action_for_num(book);
    let mut keyed: Vec<_> = found
        .into_iter()
        .map(|(t, s)| {
            let values: Vec<SortValue<'a>> = levels.iter().map(|l| sort_value(book, l.key, t, s)).collect();
            (t, s, values)
        })
        .collect();
    keyed.sort_by(|(ta, sa, va), (tb, sb, vb)| {
        levels
            .iter()
            .zip(va.iter().zip(vb))
            .map(|(level, (a, b))| {
                let o = compare(a, b, || split_order(ta, sa, tb, sb, num_action));
                if level.ascending { o } else { o.reverse() }
            })
            .find(|o| o.is_ne())
            .unwrap_or(Ordering::Equal)
    });
    keyed.into_iter().map(|(t, s, _)| (t, s)).collect()
}

fn sort_value<'a>(book: &'a Book, key: Option<SortKey>, t: &Transaction, s: &Split) -> SortValue<'a> {
    let text = |x: Option<&str>| SortValue::Text(x.unwrap_or("").to_owned());
    let Some(key) = key else {
        return SortValue::Nothing;
    };
    match key {
        SortKey::AccountFullName => SortValue::Path(account_path(book, s.account)),
        SortKey::AccountCode => text(s.account.and_then(|a| book.account(a)).and_then(|a| a.code.as_deref())),
        SortKey::CorrAccountFullName => SortValue::Text(corr_account_full_name(book, t, s.guid)),
        SortKey::CorrAccountCode => SortValue::Text(corr_account_code(book, t, s.guid)),
        SortKey::DatePosted => SortValue::Time(t.date_posted),
        SortKey::DateEntered => SortValue::Time(t.date_entered),
        SortKey::DateReconciled => SortValue::Time(s.reconcile_date.unwrap_or_default()),
        SortKey::Value => SortValue::Number(s.value),
        SortKey::Amount => SortValue::Number(s.amount),
        SortKey::Num => {
            let (n, rest) = strtol(t.num.as_deref().unwrap_or(""));
            SortValue::Num(n, rest.to_owned())
        }
        SortKey::Action => text(s.action.as_deref()),
        SortKey::Memo => text(s.memo.as_deref()),
        SortKey::Description => text(t.description.as_deref()),
        SortKey::Default => SortValue::SplitOrder,
    }
}

/// `gnc_account_get_all_parents`, top first: the account and its ancestors below the root (`Account.cpp:3283-3290`).
fn account_path(book: &Book, account: Option<Guid>) -> Vec<(Guid, &str)> {
    let Some(a) = account.and_then(|a| book.account(a)) else {
        return Vec::new();
    };
    let mut path: Vec<(Guid, &str)> = std::iter::once(a)
        .chain(book.ancestors(a.guid))
        .filter(|a| a.guid != book.root)
        .map(|a| (a.guid, a.name.as_str()))
        .collect();
    path.reverse();
    path
}

fn compare(a: &SortValue<'_>, b: &SortValue<'_>, split_order: impl FnOnce() -> Ordering) -> Ordering {
    match (a, b) {
        (SortValue::Path(a), SortValue::Path(b)) => compare_paths(a, b),
        (SortValue::Text(a), SortValue::Text(b)) => a.cmp(b),
        (SortValue::Time(a), SortValue::Time(b)) => a.cmp(b),
        (SortValue::Number(a), SortValue::Number(b)) => a.cmp_value(*b),
        (SortValue::Num(na, ra), SortValue::Num(nb, rb)) => na.cmp(nb).then_with(|| ra.cmp(rb)),
        (SortValue::SplitOrder, SortValue::SplitOrder) => split_order(),
        _ => Ordering::Equal,
    }
}

/// `xaccSplitCompareAccountFullNames` on two paths (`5.17:libgnucash/engine/Split.cpp:1687-1709`): equal for the same
/// account; at the first level whose accounts differ, their names; else the path that ends first is less.
fn compare_paths(a: &[(Guid, &str)], b: &[(Guid, &str)]) -> Ordering {
    if a.last().map(|x| x.0) == b.last().map(|x| x.0) {
        return Ordering::Equal;
    }
    match a.iter().zip(b).find(|(x, y)| x.0 != y.0) {
        Some((x, y)) => x.1.cmp(y.1),
        None if a.len() <= b.len() => Ordering::Less,
        None => Ordering::Greater,
    }
}

/// `strtol(s, &rest, 10)` as glibc reads it in the C locale: white space, a sign and decimal digits, clamped to the
/// `long` (64-bit) range; with no digits, 0 and the whole of `s` as the rest.
fn strtol(s: &str) -> (i64, &str) {
    let body = s.trim_start_matches([' ', '\t', '\n', '\x0b', '\x0c', '\r']);
    let (negative, unsigned) = match body.as_bytes().first() {
        Some(b'-') => (true, &body[1..]),
        Some(b'+') => (false, &body[1..]),
        _ => (false, body),
    };
    let digits = unsigned.bytes().take_while(u8::is_ascii_digit).count();
    if digits == 0 {
        return (0, s);
    }
    let n = unsigned[..digits].bytes().fold(0_i64, |acc, d| {
        let d = i64::from(d - b'0');
        let acc = acc.saturating_mul(10);
        if negative { acc.saturating_sub(d) } else { acc.saturating_add(d) }
    });
    (n, &unsigned[digits..])
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::super::tests::{Fixture, fixture, usd};
    use super::*;
    use crate::ops::commit::{NewSplit, NewTransaction};
    use crate::{Account, AccountType, CommodityRef, Date, Env};

    fn level(key: SortKey, ascending: bool) -> SortLevel {
        SortLevel { key: Some(key), ascending }
    }

    fn post(f: &mut Fixture, day: u32, edit: impl FnOnce(&mut NewTransaction)) -> Guid {
        let mut new = NewTransaction::new(CommodityRef::currency("USD"), Date::from_ymd(2024, 6, day).unwrap());
        new.splits = vec![NewSplit::new(f.bank, usd(-100)), NewSplit::new(f.food, usd(100))];
        edit(&mut new);
        super::super::post(&mut f.book, &mut f.env, new).unwrap()
    }

    /// The splits of `account` sorted by `levels`, as (transaction, split) GUIDs.
    fn sorted(f: &Fixture, levels: &[SortLevel], accounts: &[Guid]) -> Vec<(Guid, Guid)> {
        let num_action = uses_split_action_for_num(&f.book);
        let mut found: Vec<(&Transaction, &Split)> = f
            .book
            .transactions
            .values()
            .flat_map(|t| t.splits.iter().map(move |s| (t.as_ref(), s)))
            .filter(|(_, s)| s.account.is_some_and(|a| accounts.contains(&a)))
            .collect();
        found.sort_by(|(ta, sa), (tb, sb)| split_order(ta, sa, tb, sb, num_action));
        sort_splits(&f.book, levels, found).iter().map(|(t, s)| (t.guid, s.guid)).collect()
    }

    fn txns(v: &[(Guid, Guid)]) -> Vec<Guid> {
        v.iter().map(|x| x.0).collect()
    }

    fn add_account(f: &mut Fixture, name: &str, parent: Guid, code: Option<&str>) -> Guid {
        let mut a = Account::new(f.env.new_guid(), name, AccountType::Asset, Some(CommodityRef::currency("USD")), Some(parent));
        a.code = code.map(Into::into);
        let g = a.guid;
        f.book.accounts.insert(g, Arc::new(a));
        g
    }

    #[test]
    fn no_first_key_keeps_split_order() {
        let mut f = fixture();
        let late = post(&mut f, 2, |_| {});
        let early = post(&mut f, 1, |_| {});
        let none = SortLevel { key: None, ascending: true };
        assert_eq!(txns(&sorted(&f, &[], &[f.food])), [early, late]);
        assert_eq!(
            txns(&sorted(&f, &[none, level(SortKey::DatePosted, false)], &[f.food])),
            [early, late],
            "a second key does not sort when the first is none"
        );
        assert_eq!(txns(&sorted(&f, &[level(SortKey::Default, false)], &[f.food])), [late, early]);
    }

    #[test]
    fn account_full_names_compare_level_by_level_ancestors_first() {
        let mut f = fixture();
        let root = f.book.root;
        let assets = add_account(&mut f, "Assets", root, None);
        let bank = add_account(&mut f, "Bank", assets, None);
        let assets2 = add_account(&mut f, "Assets 2", root, None);
        let [in_bank, in_assets2, in_assets] = [bank, assets2, assets].map(|a| {
            post(&mut f, 1, |n| n.splits[0].account = a)
        });
        let by_name = sorted(&f, &[level(SortKey::AccountFullName, true)], &[bank, assets2, assets]);
        // "Assets 2" < "Assets:Bank" as whole strings, but "Assets" < "Assets 2" at the first level.
        assert_eq!(txns(&by_name), [in_assets, in_bank, in_assets2]);
        let down = sorted(&f, &[level(SortKey::AccountFullName, false)], &[bank, assets2, assets]);
        assert_eq!(txns(&down), [in_assets2, in_bank, in_assets]);
    }

    #[test]
    fn codes_and_texts_compare_bytewise_and_ties_fall_through() {
        let mut f = fixture();
        let root = f.book.root;
        let ten = add_account(&mut f, "Ten", root, Some("10"));
        let nine = add_account(&mut f, "Nine", root, Some("9"));
        let uncoded = add_account(&mut f, "Uncoded", root, None);
        let [in_ten, in_nine, in_uncoded] = [ten, nine, uncoded].map(|acc| post(&mut f, 1, |n| n.splits[0].account = acc));
        let by_code = sorted(&f, &[level(SortKey::AccountCode, true)], &[ten, nine, uncoded]);
        assert_eq!(txns(&by_code), [in_uncoded, in_ten, in_nine]);
        let memo = |m: &'static str| move |n: &mut NewTransaction| n.splits[1].memo = Some(m.into());
        let mut f = fixture();
        let b_2 = post(&mut f, 2, memo("b"));
        let a_1 = post(&mut f, 1, memo("a"));
        let a_3 = post(&mut f, 3, memo("a"));
        let levels = [level(SortKey::Memo, false), level(SortKey::DatePosted, false)];
        assert_eq!(txns(&sorted(&f, &levels, &[f.food])), [b_2, a_3, a_1], "each level negated on its own");
        let levels = [level(SortKey::Memo, true), SortLevel { key: None, ascending: false }, level(SortKey::Default, false)];
        assert_eq!(txns(&sorted(&f, &levels, &[f.food])), [a_3, a_1, b_2], "a none level compares equal");
    }

    #[test]
    fn nums_compare_by_their_leading_numbers_then_the_rest() {
        let mut f = fixture();
        let nums = ["10", "9", "abc", "", " 5", "-3", "5a", "+5"];
        let posted: Vec<Guid> = nums
            .iter()
            .map(|n| post(&mut f, 1, |t| t.num = Some((*n).into())))
            .collect();
        let order: Vec<&str> = txns(&sorted(&f, &[level(SortKey::Num, true)], &[f.food]))
            .iter()
            .map(|g| nums[posted.iter().position(|p| p == g).unwrap()])
            .collect();
        // -3; then 0 with rests "" and "abc"; then 5 with rests "", "", "a" (the two "" in split order); 9; 10.
        assert_eq!(order[..3], ["-3", "", "abc"]);
        assert_eq!(order[5..], ["5a", "9", "10"]);
        assert_eq!(strtol(" 5x"), (5, "x"));
        assert_eq!(strtol("-"), (0, "-"));
        assert_eq!(strtol("99999999999999999999"), (i64::MAX, ""));
        assert_eq!(strtol("-99999999999999999999z"), (i64::MIN, "z"));
    }

    #[test]
    fn numbers_and_times_compare_exactly() {
        let mut f = fixture();
        let small = post(&mut f, 3, |n| {
            n.splits[0].value = usd(-5);
            n.splits[1].value = usd(5);
            n.date_entered = Some(Time64(10));
        });
        let big = post(&mut f, 1, |n| n.date_entered = Some(Time64(20)));
        let reconciled = post(&mut f, 2, |n| {
            n.splits[1].reconcile_date = Some(Time64(1));
            n.date_entered = Some(Time64(30));
        });
        assert_eq!(txns(&sorted(&f, &[level(SortKey::Value, true)], &[f.food])), [small, big, reconciled]);
        assert_eq!(txns(&sorted(&f, &[level(SortKey::Amount, false)], &[f.food])), [big, reconciled, small]);
        assert_eq!(txns(&sorted(&f, &[level(SortKey::DatePosted, true)], &[f.food])), [big, reconciled, small]);
        assert_eq!(
            txns(&sorted(&f, &[level(SortKey::DateReconciled, false)], &[f.food])),
            [reconciled, big, small],
            "never reconciled is time 0"
        );
        assert_eq!(txns(&sorted(&f, &[level(SortKey::DateEntered, true)], &[f.food])), [small, big, reconciled]);
    }

    #[test]
    fn corresponding_accounts_label_split_transactions() {
        let mut f = fixture();
        let food = f.food;
        let plain = post(&mut f, 1, |_| {});
        let three = post(&mut f, 1, |n| {
            n.splits[1].value = usd(60);
            n.splits.push(NewSplit::new(food, usd(40)));
        });
        let by = |key| txns(&sorted(&f, &[level(key, true), level(SortKey::Default, true)], &[f.bank]));
        // "-- Split Transaction --" sorts before "Food"; the code "" (Food has none) before "Split".
        assert_eq!(by(SortKey::CorrAccountFullName), [three, plain]);
        assert_eq!(by(SortKey::CorrAccountCode), [plain, three]);
        let t = f.book.transaction(three).unwrap();
        assert_eq!(super::super::corr_account_code(&f.book, t, t.splits[0].guid), "Split");
    }
}
