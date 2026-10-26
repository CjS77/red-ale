//! Transfers between QIF accounts: a split to `[Account]` in one file and the matching transaction in that account's
//! list are one transfer, so one of the two halves is marked and not imported, and what it knew is merged into the
//! other. `5.17:gnucash/import-export/qif-imp/qif-to-gnc.scm:923-1326`.
//!
//! The functions work on the files' records by position, `(file, transaction)`, since a match changes the marks,
//! payees and cleared flags of two records at once.

use red_ale_core::{How, Numeric};

use super::super::dialog_utils::{
    action, default_capital_return_acct, default_cglong_acct, default_cgmid_acct, default_cgshort_acct, default_commission_acct,
    default_dividend_acct, default_equity_holding, default_interest_acct, default_margin_interest_acct, default_stock_acct, split_amount,
};
use super::super::objects::{QifFile, QifSplit, QifXtn};
use super::super::parse::{Action, Action as A};

/// A transaction by position: its file, then its place in the file's list.
pub type XtnRef = (usize, usize);

/// How two transactions' splits matched. `qif-to-gnc.scm:1055-1058`.
#[derive(Debug, Clone, PartialEq, Eq)]
enum How2 {
    /// One split matches one split.
    OneToOne(usize),
    /// The split is one of several of its transaction that together match this one split.
    OneToMany(usize),
    /// These splits of the other transaction together match the split.
    ManyToOne(Vec<usize>),
}

fn xtn(files: &[QifFile], r: XtnRef) -> Option<&QifXtn> {
    files.get(r.0).and_then(|f| f.xtns.get(r.1))
}

fn xtn_mut(files: &mut [QifFile], r: XtnRef) -> Option<&mut QifXtn> {
    files.get_mut(r.0).and_then(|f| f.xtns.get_mut(r.1))
}

/// `n+`: `gnc-numeric-add` with `GNC-DENOM-LCD`; an overflow is GnuCash's error value, which equals nothing.
fn add(a: Option<Numeric>, b: Numeric) -> Option<Numeric> {
    a.and_then(|a| a.add(b, 0, How::LCD_NEVER).ok())
}

/// `gnc-numeric-equal`.
fn equal(a: Option<Numeric>, b: Option<Numeric>) -> bool {
    matches!((a, b), (Some(a), Some(b)) if a.same_value(b))
}

/// The markable transactions: those with a split to an account, newest first, as GnuCash conses them over the
/// files in the order loaded. `qif-to-gnc.scm:332-347`.
pub fn markable_xtns(files: &[QifFile]) -> Vec<XtnRef> {
    let mut list: Vec<XtnRef> = files
        .iter()
        .enumerate()
        .flat_map(|(f, file)| file.xtns.iter().enumerate().filter(|(_, x)| x.splits.iter().any(|s| s.category_is_account)).map(move |(i, _)| (f, i)))
        .collect();
    list.reverse();
    list
}

/// Run every markable transaction against those after it in the list, marking the halves of transfers.
/// `qif-to-gnc.scm:404-418`. Returns GnuCash's log messages.
pub fn mark_transfers(files: &mut [QifFile], markable: &[XtnRef]) -> Vec<String> {
    let mut log = Vec::new();
    if markable.len() > 1 {
        (0..markable.len() - 1).for_each(|k| {
            let here = markable[k];
            if xtn(files, here).is_some_and(|x| !x.mark) {
                mark_matching_xtns(files, here, &markable[k + 1..], &mut log);
            }
        });
    }
    log
}

/// `qif-import:mark-matching-xtns`: find the other half of each of the transaction's splits to an account.
/// `qif-to-gnc.scm:929-945`.
fn mark_matching_xtns(files: &mut [QifFile], here: XtnRef, candidates: &[XtnRef], log: &mut Vec<String>) {
    let Some(count) = xtn(files, here).map(|x| x.splits.len()) else { return };
    let mut splits_left: Vec<usize> = (0..count).collect();
    while let Some(&first) = splits_left.first() {
        let wanted = xtn(files, here).and_then(|x| x.splits.get(first)).is_some_and(|s| !s.mark && s.category_is_account);
        splits_left = if wanted { mark_some_splits(files, here, &splits_left, candidates, log) } else { splits_left[1..].to_vec() };
    }
}

/// What `mark-some-splits` looks for: the accounts at each end, the amount and, for a bank transaction, the splits
/// to the same account and their total.
struct Wanted {
    near: Option<String>,
    far: Option<String>,
    amount: Option<Numeric>,
    group_amount: Option<Numeric>,
    same: Vec<usize>,
    different: Vec<usize>,
}

/// The first half of `qif-import:mark-some-splits`: what the split must match. `qif-to-gnc.scm:954-1019`.
fn wanted(x: &QifXtn, splits: &[usize]) -> Option<Wanted> {
    let first = *splits.first()?;
    let split = x.splits.get(first)?;
    let amount = Some(split_amount(split).neg());
    if x.security_name.is_none() {
        let far = split.category.clone();
        let (same, different): (Vec<usize>, Vec<usize>) =
            splits.iter().partition(|i| x.splits.get(**i).is_some_and(|s| s.category_is_account && s.category == far));
        let group_amount = same.iter().filter_map(|i| x.splits.get(*i)).try_fold(Numeric::ZERO, |g, s| g.sub(split_amount(s), 0, How::LCD_NEVER).ok());
        return Some(Wanted { near: x.from_acct.clone(), far: Some(far), amount, group_amount, same, different });
    }
    let (mut near, mut far, _) = accounts_affected(split, x);
    let mut amount = amount;
    let negated = amount.map(Numeric::neg);
    let from = || x.from_acct.clone();
    match action(x) {
        Some(A::IntIncX | A::DivX | A::CgLongX | A::CgMidX | A::CgShortX | A::RtrnCapX | A::MargIntX | A::SellX) => {
            (amount, near, far) = (negated, from(), Some(split.category.clone()));
        }
        Some(A::MiscIncX | A::MiscExpX) => (amount, near, far) = (negated, from(), split.miscx_category.clone()),
        Some(A::BuyX) => (near, far) = (from(), Some(split.category.clone())),
        Some(A::XOut) => amount = negated,
        _ => {}
    }
    Some(Wanted { near, far, amount, group_amount: None, same: vec![first], different: Vec::new() })
}

/// `qif-import:mark-some-splits`: look through the candidates for the other half of the first of `splits`, merge
/// and mark when found, and return the splits still to look at. `qif-to-gnc.scm:954-1050`.
fn mark_some_splits(files: &mut [QifFile], here: XtnRef, splits: &[usize], candidates: &[XtnRef], log: &mut Vec<String>) -> Vec<usize> {
    let rest = || splits.get(1..).unwrap_or_default().to_vec();
    let Some((w, date)) = xtn(files, here).and_then(|x| Some((wanted(x, splits)?, x.date.clone()))) else { return rest() };
    let (Some(near), Some(far)) = (w.near.as_deref(), w.far.as_deref()) else {
        log.push("Transaction with no or only one associated account.".to_owned());
        return rest();
    };
    let found = candidates.iter().find_map(|c| {
        let cand = xtn(files, *c)?;
        if cand.mark || cand.from_acct.as_deref() != Some(far) {
            return None;
        }
        xtn_has_matches(cand, near, date.as_ref(), w.amount, w.group_amount).map(|how| (*c, how))
    });
    match found {
        None => rest(),
        Some((other, how)) => {
            merge_and_mark_xtns(files, here, &w.same, other, &how);
            match how {
                How2::OneToMany(_) => w.different,
                How2::OneToOne(_) | How2::ManyToOne(_) => rest(),
            }
        }
    }
}

/// Some investment actions' amounts are positive in QIF either way. `qif-to-gnc.scm:1082-1089`.
fn reverses_sign(a: Action) -> bool {
    use Action as A;
    matches!(a, A::XOut | A::SellX | A::IntIncX | A::DivX | A::CgLongX | A::CgShortX | A::MiscIncX | A::MiscExpX)
}

/// `qif-import:xtn-has-matches?`: on the same date, a split of the candidate to `acct_name` of the amount, or of the
/// group's amount, or several that add up to it. `qif-to-gnc.scm:1060-1119`.
fn xtn_has_matches(
    cand: &QifXtn,
    acct_name: &str,
    date: Option<&super::super::objects::Field<super::super::parse::QifDate>>,
    amount: Option<Numeric>,
    group_amt: Option<Numeric>,
) -> Option<How2> {
    let same_date = matches!((date.and_then(|d| d.value()), cand.date.as_ref().and_then(|d| d.value())), (Some(a), Some(b)) if a == b);
    if !same_date {
        return None;
    }
    let negate = cand.security_name.is_some() && action(cand).is_some_and(reverses_sign);
    let mut same: Vec<usize> = Vec::new();
    let mut group = Some(Numeric::ZERO);
    for (j, split) in cand.splits.iter().enumerate().filter(|(_, s)| s.category_is_account && s.category == acct_name) {
        let this = if negate { split_amount(split).neg() } else { split_amount(split) };
        if equal(Some(this), amount) {
            return Some(How2::OneToOne(j));
        }
        if group_amt.is_some() && equal(Some(this), group_amt) {
            return Some(How2::OneToMany(j));
        }
        same.insert(0, j);
        group = add(group, this);
    }
    equal(group, amount).then_some(How2::ManyToOne(same))
}

/// `qif-split:accounts-affected`: the near, far and commission accounts of a split. `qif-to-gnc.scm:1127-1203`.
pub fn accounts_affected(split: &QifSplit, x: &QifXtn) -> (Option<String>, Option<String>, Option<String>) {
    let from = x.from_acct.clone().unwrap_or_default();
    let Some(security) = x.security_name.as_deref() else { return (x.from_acct.clone(), Some(split.category.clone()), None) };
    let first = x.splits.first();
    let category = || first.map(|s| s.category.clone());
    let near = match action(x) {
        Some(
            A::Buy | A::BuyX | A::Sell | A::SellX | A::ReinvInt | A::ReinvDiv | A::ReinvSg | A::ReinvSh | A::ReinvLg | A::ReinvMd | A::ShrsIn | A::ShrsOut
            | A::StkSplit,
        ) => Some(default_stock_acct(&from, security)),
        Some(A::Div | A::CgShort | A::CgLong | A::CgMid | A::IntInc | A::MiscInc | A::MiscExp | A::RtrnCap | A::MargInt | A::XIn | A::XOut) => Some(from.clone()),
        Some(A::DivX | A::CgShortX | A::CgLongX | A::CgMidX | A::IntIncX | A::RtrnCapX | A::MargIntX) => category(),
        Some(A::MiscIncX | A::MiscExpX) => first.and_then(|s| s.miscx_category.clone()),
        _ => None,
    };
    let far = match action(x) {
        Some(A::Buy | A::Sell) => Some(from.clone()),
        Some(A::BuyX | A::SellX | A::MiscInc | A::MiscIncX | A::MiscExp | A::MiscExpX | A::XIn | A::XOut) => category(),
        Some(A::StkSplit) => Some(default_stock_acct(&from, security)),
        Some(A::CgShort | A::CgShortX | A::ReinvSg | A::ReinvSh) => Some(default_cgshort_acct(&from, security)),
        Some(A::CgLong | A::CgLongX | A::ReinvLg) => Some(default_cglong_acct(&from, security)),
        Some(A::CgMid | A::CgMidX | A::ReinvMd) => Some(default_cgmid_acct(&from, security)),
        Some(A::IntInc | A::IntIncX | A::ReinvInt) => Some(default_interest_acct(&from, security)),
        Some(A::MargInt | A::MargIntX) => Some(default_margin_interest_acct(&from)),
        Some(A::RtrnCap | A::RtrnCapX) => Some(default_capital_return_acct(&from, security)),
        Some(A::Div | A::DivX | A::ReinvDiv) => Some(default_dividend_acct(&from, security)),
        Some(A::ShrsIn | A::ShrsOut) => Some(default_equity_holding(security)),
        _ => None,
    };
    let commission = x.commission.is_some().then(|| default_commission_acct(&from));
    (near, far, commission)
}

/// Which half of a one-to-one match is marked. `qif-to-gnc.scm:1244-1302`.
fn mark_here(x: &QifXtn, other: &QifXtn) -> bool {
    use std::cmp::Ordering;
    let (a, oa, os) = (action(x), action(other), other.security_name.is_some());
    match x.splits.len().cmp(&other.splits.len()) {
        Ordering::Less => true,
        Ordering::Greater => false,
        // With equal split counts: the plain half of a plain/investment pair, or the investment half whose other
        // half is an `X` action. When this side is the investment and the other plain, the other is marked.
        Ordering::Equal => {
            os && ((a.is_none() && oa.is_some())
                || (a.is_some()
                    && matches!(
                        oa,
                        Some(
                            Action::BuyX
                                | Action::SellX
                                | Action::CgShortX
                                | Action::CgMidX
                                | Action::CgLongX
                                | Action::IntIncX
                                | Action::DivX
                                | Action::MargIntX
                                | Action::RtrnCapX
                                | Action::MiscIncX
                                | Action::MiscExpX
                        )
                    )))
        }
    }
}

/// `qif-import:merge-and-mark-xtns`: mark the half that goes, merge its payee, address and number into the other,
/// and give the other's matching splits its cleared flag. `qif-to-gnc.scm:1212-1302`.
fn merge_and_mark_xtns(files: &mut [QifFile], here: XtnRef, splits: &[usize], other: XtnRef, how: &How2) {
    let Some(&split) = splits.first() else { return };
    let (Some(x), Some(o)) = (xtn(files, here), xtn(files, other)) else { return };
    let here_cleared = x.cleared.clone();
    let other_cleared = o.cleared.clone();
    // (marked transaction, its split, the transaction kept, its splits that take the marked one's cleared flag)
    let (marked, marked_split, kept, kept_splits, cleared) = match how {
        How2::ManyToOne(matches) => (here, split, other, matches.clone(), here_cleared),
        How2::OneToMany(m) => (other, *m, here, splits.to_vec(), other_cleared),
        How2::OneToOne(m) if mark_here(x, o) => (here, split, other, vec![*m], here_cleared),
        How2::OneToOne(m) => (other, *m, here, vec![split], other_cleared),
    };
    mark_split(files, marked, marked_split);
    merge_xtn_info(files, marked, kept);
    if let Some(k) = xtn_mut(files, kept) {
        k.splits.iter_mut().enumerate().filter(|(i, _)| kept_splits.contains(i)).for_each(|(_, s)| s.matching_cleared.clone_from(&cleared));
    }
}

/// `qif-import:merge-xtn-info`: the payee, address and number the kept transaction lacks. `qif-to-gnc.scm:1305-1314`.
fn merge_xtn_info(files: &mut [QifFile], from: XtnRef, to: XtnRef) {
    let Some(f) = xtn(files, from).cloned() else { return };
    let Some(t) = xtn_mut(files, to) else { return };
    if t.payee.is_none() {
        t.payee = f.payee;
    }
    if t.address.is_none() {
        t.address = f.address;
    }
    if t.number.is_none() {
        t.number = f.number;
    }
}

/// `qif-xtn:mark-split`: mark the split, and the transaction once all its splits are. `qif-to-gnc.scm:1317-1326`.
fn mark_split(files: &mut [QifFile], r: XtnRef, split: usize) {
    let Some(x) = xtn_mut(files, r) else { return };
    if let Some(s) = x.splits.get_mut(split) {
        s.mark = true;
    }
    if x.splits.iter().all(|s| s.mark) {
        x.mark = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::qif::objects::Field;
    use crate::qif::parse::{Cleared, QifDate};

    fn bank(from: &str, to: &str, amount: i64, cleared: Option<Cleared>) -> QifXtn {
        let mut x = QifXtn::new(1);
        x.from_acct = Some(from.into());
        x.date = Some(Field::Value(QifDate { day: 1, month: 2, year: 2003 }));
        x.cleared = cleared.map(Field::Value);
        let mut s = QifSplit::new();
        s.category = to.into();
        s.category_is_account = true;
        s.amount = Some(Field::Value(Numeric::from_int(amount)));
        x.splits = vec![s];
        x
    }

    #[test]
    fn the_two_halves_of_a_transfer_become_one() {
        let checking = QifFile { xtns: vec![bank("Checking", "Savings", -100, Some(Cleared::Reconciled))], ..QifFile::default() };
        let mut savings = QifFile { xtns: vec![bank("Savings", "Checking", 100, None)], ..QifFile::default() };
        savings.xtns[0].payee = Some("Transfer".into());
        let mut files = [checking, savings];
        let markable = markable_xtns(&files);
        assert_eq!(markable, [(1, 0), (0, 0)], "newest first");
        assert!(mark_transfers(&mut files, &markable).is_empty());
        // Equal split counts and no actions: the candidate (Checking's) is marked (`:1297-1302`), and its cleared
        // flag passes to the kept split.
        assert!(files[0].xtns[0].mark && !files[1].xtns[0].mark);
        assert_eq!(files[1].xtns[0].splits[0].matching_cleared, Some(Field::Value(Cleared::Reconciled)));
        assert_eq!(files[0].xtns[0].payee.as_deref(), None, "the kept half had the payee already");
    }

    #[test]
    fn different_dates_or_amounts_do_not_match() {
        let mut other = bank("Savings", "Checking", 99, None);
        let mut files = [QifFile { xtns: vec![bank("Checking", "Savings", -100, None)], ..QifFile::default() }, QifFile::default()];
        files[1].xtns.push(other.clone());
        let markable = markable_xtns(&files);
        mark_transfers(&mut files, &markable);
        assert!(!files[0].xtns[0].mark && !files[1].xtns[0].mark);
        other.date = Some(Field::Value(QifDate { day: 2, month: 2, year: 2003 }));
        assert_eq!(xtn_has_matches(&other, "Checking", files[0].xtns[0].date.as_ref(), Some(Numeric::from_int(100)), None), None);
    }

    #[test]
    fn several_splits_match_one() {
        // A split of 100 against two splits of 60 and 40: many to one.
        let mut many = bank("Savings", "Checking", 60, None);
        let mut second = many.splits[0].clone();
        second.amount = Some(Field::Value(Numeric::from_int(40)));
        many.splits.push(second);
        let how = xtn_has_matches(&many, "Checking", many.date.as_ref(), Some(Numeric::from_int(100)), Some(Numeric::ZERO));
        assert_eq!(how, Some(How2::ManyToOne(vec![1, 0])));
    }
}
