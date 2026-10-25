//! Find transactions: the criteria and operators of GnuCash's Find Transaction dialog, over splits.
//!
//! The dialog offers its criteria in `5.17:gnucash/gnome/dialog-find-transactions.c:105-158`, with the operators of
//! `5.17:gnucash/gnome-search/search-*.c`, evaluated by the predicates of `5.17:libgnucash/engine/qofquerycore.cpp`.
//! Criteria combine with all (AND), any (OR) and not, which covers the dialog's "all/any criteria" and its refine,
//! add and delete search modes (`5.17:gnucash/gnome-search/dialog-search.c:449-517`). Results are splits in
//! `xaccSplitOrder`. See `design/notes/engine-accounts-transactions.md` §7.
//!
//! Times are read in UTC where GnuCash uses the local time zone (red-ale has no local time zone yet).

use std::cmp::Ordering;

use regex::{Regex, RegexBuilder};

use super::order::split_order;
use crate::{Book, Date, DomainError, DomainResult, Guid, How, Numeric, ReconcileState, Round, Split, Time64, Transaction};

/// How a string criterion compares (`5.17:gnucash/gnome-search/search-string.c:235-242`, `search-string.h:32-37`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StringOp {
    /// Contains the text (the dialog's default).
    Contains,
    /// Does not contain the text.
    NotContains,
    /// Equals the text.
    Equals,
    /// Does not equal the text.
    NotEquals,
    /// Matches a regular expression anywhere in the text.
    MatchesRegex,
    /// Does not match a regular expression.
    NotMatchesRegex,
}

/// A string criterion.
#[derive(Debug, Clone)]
pub struct StringMatch {
    op: StringOp,
    text: String,
    folded: String,
    match_case: bool,
    regex: Option<Regex>,
}

impl StringMatch {
    /// A criterion. Empty text is refused, as the dialog refuses it; a regular expression must compile. Without
    /// `match_case` the comparison ignores case, the dialog's default.
    pub fn new(op: StringOp, text: &str, match_case: bool) -> DomainResult<Self> {
        if text.is_empty() {
            return Err(DomainError::invalid("a find criterion needs text to match"));
        }
        let regex = matches!(op, StringOp::MatchesRegex | StringOp::NotMatchesRegex)
            .then(|| RegexBuilder::new(text).case_insensitive(!match_case).build())
            .transpose()
            .map_err(|e| DomainError::invalid(format!("not a regular expression: {e}")))?;
        Ok(Self { op, text: text.to_owned(), folded: fold(text), match_case, regex })
    }

    /// `string_match_predicate` (`5.17:libgnucash/engine/qofquerycore.cpp:126-191`); a missing string reads as "".
    fn matches(&self, s: Option<&str>) -> bool {
        let s = s.unwrap_or("");
        let hit = match (&self.regex, self.op, self.match_case) {
            (Some(re), _, _) => re.is_match(s),
            (None, StringOp::Contains | StringOp::NotContains, true) => s.contains(&self.text),
            (None, StringOp::Contains | StringOp::NotContains, false) => fold(s).contains(&self.folded),
            (None, _, true) => s == self.text,
            (None, _, false) => fold(s) == self.folded,
        };
        match self.op {
            StringOp::Contains | StringOp::Equals | StringOp::MatchesRegex => hit,
            StringOp::NotContains | StringOp::NotEquals | StringOp::NotMatchesRegex => !hit,
        }
    }
}

/// Case folding for case-insensitive matching. GnuCash uses `g_utf8_casefold` and NFC normalisation
/// (`5.17:libgnucash/engine/qofutil.cpp:54-97`); red-ale lowercases, which agrees except for the few characters whose
/// case folding is not their lowercase (such as `ß`).
fn fold(s: &str) -> String {
    s.to_lowercase()
}

/// A comparison (`QofQueryCompare`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Compare {
    /// `<`
    Less,
    /// `<=`
    LessOrEqual,
    /// `==`
    Equal,
    /// `!=`
    NotEqual,
    /// `>`
    Greater,
    /// `>=`
    GreaterOrEqual,
}

impl Compare {
    fn holds(self, ord: Ordering) -> bool {
        match self {
            Self::Less => ord.is_lt(),
            Self::LessOrEqual => ord.is_le(),
            Self::Equal => ord.is_eq(),
            Self::NotEqual => ord.is_ne(),
            Self::Greater => ord.is_gt(),
            Self::GreaterOrEqual => ord.is_ge(),
        }
    }
}

/// Which signs a numeric criterion accepts (`QofNumericMatch`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Sign {
    /// Any sign (the default).
    #[default]
    Any,
    /// Debits: zero or more.
    Debit,
    /// Credits: zero or less.
    Credit,
}

/// A numeric criterion: compares the magnitude of the split's number with `amount`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NumericMatch {
    /// The comparison.
    pub compare: Compare,
    /// The amount compared with.
    pub amount: Numeric,
    /// The signs accepted.
    pub sign: Sign,
}

impl NumericMatch {
    /// `numeric_match_predicate` (`5.17:libgnucash/engine/qofquerycore.cpp:484-541`): the sign filter keeps credits
    /// at or below zero and debits at or above it; `==` and `!=` treat magnitudes within 1/10000 as equal; the
    /// other comparisons compare the magnitude with the amount as entered.
    fn matches(&self, n: Numeric) -> bool {
        let sign_ok = match self.sign {
            Sign::Any => true,
            Sign::Debit => !n.is_negative(),
            Sign::Credit => !n.is_positive(),
        };
        if !sign_ok {
            return false;
        }
        match self.compare {
            Compare::Equal | Compare::NotEqual => {
                let close = n.abs().sub(self.amount.abs(), 100_000, How::round(Round::HalfUp)).is_ok_and(|d| d.abs().cmp_value(Numeric::new(1, 10_000)).is_lt());
                close == (self.compare == Compare::Equal)
            }
            c => c.holds(n.abs().cmp_value(self.amount)),
        }
    }
}

/// A date criterion, with the dialog's operators (`5.17:gnucash/gnome-search/search-date.c:187-194`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DateMatch {
    /// The comparison: `Less` is "is before", `Equal` "is on", and so on.
    pub compare: Compare,
    /// The date.
    pub date: Date,
}

impl DateMatch {
    /// `date_match_predicate` with the dialog's boundaries (`5.17:gnucash/gnome-search/search-date.c:160-170`,
    /// `:255-264`): "is on" and "is not on" compare calendar days; "before or on" and "after" compare with the end
    /// of the day; "before" and "on or after" with the day's neutral time, 10:59.
    fn matches(self, t: Time64) -> bool {
        match self.compare {
            Compare::Equal | Compare::NotEqual => self.compare.holds(t.utc_date().cmp(&Some(self.date))),
            Compare::LessOrEqual | Compare::Greater => self.compare.holds(t.cmp(&self.date.end_utc())),
            Compare::Less | Compare::GreaterOrEqual => self.compare.holds(t.cmp(&self.date.neutral_time())),
        }
    }
}

/// How an account criterion matches (`QofGuidMatch`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccountMatch {
    /// The split's account is one of them.
    Any,
    /// The split's account is none of them.
    None,
    /// The split's transaction has a split in every one of them ("All Accounts").
    All,
}

/// A find query over splits.
#[derive(Debug, Clone)]
pub enum Query {
    /// Every sub-query holds; an empty list matches everything.
    All(Vec<Query>),
    /// Some sub-query holds; an empty list matches nothing.
    Any(Vec<Query>),
    /// The sub-query does not hold.
    Not(Box<Query>),
    /// Transaction description.
    Description(StringMatch),
    /// Transaction notes.
    Notes(StringMatch),
    /// Split memo.
    Memo(StringMatch),
    /// Description, notes or memo: any of the three matches, also for negated operators.
    DescriptionNotesOrMemo(StringMatch),
    /// Transaction num.
    Num(StringMatch),
    /// Split action.
    Action(StringMatch),
    /// Date posted.
    DatePosted(DateMatch),
    /// Date entered (registered as a query parameter, not offered by the dialog).
    DateEntered(DateMatch),
    /// Date reconciled; a split never reconciled has date zero.
    DateReconciled(DateMatch),
    /// Split value.
    Value(NumericMatch),
    /// Split amount ("Shares").
    Amount(NumericMatch),
    /// Share price: value / amount, zero when the amount is zero.
    SharePrice(NumericMatch),
    /// The reconcile state is (or, negated, is not) one of these.
    Reconcile {
        /// The states.
        states: Vec<ReconcileState>,
        /// "is not" rather than "is".
        negate: bool,
    },
    /// The transaction is (or is not) a book-closing entry.
    Closing(bool),
    /// The transaction is (or is not) balanced.
    Balanced(bool),
    /// The transaction is (or is not) void, by its void timestamp.
    Void(bool),
    /// Accounts.
    Accounts {
        /// How they match.
        how: AccountMatch,
        /// The accounts; at least one.
        accounts: Vec<Guid>,
    },
}

/// `xaccSplitGetSharePrice` (`5.17:libgnucash/engine/Split.cpp:1994-2026`).
pub fn share_price(s: &Split) -> Numeric {
    if s.amount.is_zero() {
        return Numeric::ZERO;
    }
    s.value.div(s.amount, crate::DENOM_AUTO, How::round(Round::HalfUp)).unwrap_or(Numeric::ZERO)
}

impl Query {
    /// True when the split `s` of transaction `t` matches.
    pub fn matches(&self, t: &Transaction, s: &Split) -> bool {
        match self {
            Self::All(qs) => qs.iter().all(|q| q.matches(t, s)),
            Self::Any(qs) => qs.iter().any(|q| q.matches(t, s)),
            Self::Not(q) => !q.matches(t, s),
            Self::Description(m) => m.matches(t.description.as_deref()),
            Self::Notes(m) => m.matches(t.notes()),
            Self::Memo(m) => m.matches(s.memo.as_deref()),
            Self::DescriptionNotesOrMemo(m) => [t.description.as_deref(), t.notes(), s.memo.as_deref()].into_iter().any(|x| m.matches(x)),
            Self::Num(m) => m.matches(t.num.as_deref()),
            Self::Action(m) => m.matches(s.action.as_deref()),
            Self::DatePosted(d) => d.matches(t.date_posted),
            Self::DateEntered(d) => d.matches(t.date_entered),
            Self::DateReconciled(d) => d.matches(s.reconcile_date.unwrap_or_default()),
            Self::Value(n) => n.matches(s.value),
            Self::Amount(n) => n.matches(s.amount),
            Self::SharePrice(n) => n.matches(share_price(s)),
            Self::Reconcile { states, negate } => states.contains(&s.reconcile_state) != *negate,
            Self::Closing(b) => t.is_closing() == *b,
            Self::Balanced(b) => t.is_balanced() == *b,
            Self::Void(b) => t.is_void() == *b,
            Self::Accounts { how, accounts } => match how {
                AccountMatch::Any => s.account.is_some_and(|a| accounts.contains(&a)),
                AccountMatch::None => !s.account.is_some_and(|a| accounts.contains(&a)),
                AccountMatch::All => accounts.iter().all(|a| t.splits.iter().any(|x| x.account == Some(*a))),
            },
        }
    }

    fn check(&self, book: &Book) -> DomainResult<()> {
        match self {
            Self::All(qs) | Self::Any(qs) => qs.iter().try_for_each(|q| q.check(book)),
            Self::Not(q) => q.check(book),
            Self::Accounts { accounts, .. } if accounts.is_empty() => Err(DomainError::invalid("an account criterion needs at least one account")),
            Self::Accounts { accounts, .. } => match accounts.iter().find(|a| book.account(**a).is_none()) {
                Some(a) => Err(DomainError::not_found("account", a)),
                None => Ok(()),
            },
            _ => Ok(()),
        }
    }
}

/// A split found, with its transaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Found {
    /// The transaction.
    pub transaction: Guid,
    /// The split.
    pub split: Guid,
}

/// Every split matching `query`, in `xaccSplitOrder` (the query's default sort, `5.17:libgnucash/engine/qofquery.cpp:168-170`).
/// With a limit, the last `limit` are kept, as GnuCash keeps them (`5.17:libgnucash/engine/qofquery.cpp:799-822`).
pub fn find(book: &Book, query: &Query, limit: Option<usize>) -> DomainResult<Vec<Found>> {
    query.check(book)?;
    let mut hits: Vec<(&Transaction, &Split)> =
        book.transactions.values().flat_map(|t| t.splits.iter().map(move |s| (t.as_ref(), s))).filter(|(t, s)| query.matches(t, s)).collect();
    let num_action = crate::book_options::uses_split_action_for_num(book);
    hits.sort_by(|(ta, sa), (tb, sb)| split_order(ta, sa, tb, sb, num_action));
    let skip = limit.map_or(0, |n| hits.len().saturating_sub(n));
    Ok(hits.into_iter().skip(skip).map(|(t, s)| Found { transaction: t.guid, split: s.guid }).collect())
}

#[cfg(test)]
mod tests {
    use super::super::tests::{fixture, simple, usd};
    use super::*;

    fn text(op: StringOp, s: &str) -> StringMatch {
        StringMatch::new(op, s, false).unwrap()
    }

    #[test]
    fn strings_match_as_the_dialog_does() {
        assert!(text(StringOp::Contains, "LUN").matches(Some("lunch")));
        assert!(!StringMatch::new(StringOp::Contains, "LUN", true).unwrap().matches(Some("lunch")));
        assert!(text(StringOp::NotContains, "x").matches(None));
        assert!(text(StringOp::Equals, "Lunch").matches(Some("lunch")));
        assert!(text(StringOp::MatchesRegex, "^l.n").matches(Some("Lunch")));
        assert!(text(StringOp::NotMatchesRegex, "z$").matches(Some("Lunch")));
        assert!(StringMatch::new(StringOp::Contains, "", false).is_err());
        assert!(StringMatch::new(StringOp::MatchesRegex, "(", false).is_err());
    }

    #[test]
    fn numbers_compare_magnitudes_with_a_tolerance() {
        let m = |compare, amount, sign| NumericMatch { compare, amount, sign };
        assert!(m(Compare::Equal, usd(1000), Sign::Any).matches(usd(-1000)));
        assert!(!m(Compare::Equal, usd(1000), Sign::Debit).matches(usd(-1000)));
        assert!(m(Compare::Equal, usd(0), Sign::Credit).matches(usd(0)), "zero is both a debit and a credit");
        assert!(m(Compare::Equal, Numeric::new(1_000_005, 100_000), Sign::Any).matches(usd(1000)), "within 1/10000");
        assert!(!m(Compare::Equal, Numeric::new(100_001, 10_000), Sign::Any).matches(usd(1000)), "1/10000 apart is not equal");
        assert!(m(Compare::Greater, usd(500), Sign::Any).matches(usd(-1000)));
        assert!(!m(Compare::Less, usd(-500), Sign::Any).matches(usd(-100)), "the magnitude is compared with the amount as entered");
    }

    #[test]
    fn dates_use_the_dialogs_boundaries() {
        let d = Date::from_ymd(2024, 6, 1).unwrap();
        let posted = d.neutral_time();
        let m = |compare| DateMatch { compare, date: d };
        assert!(m(Compare::Equal).matches(posted));
        assert!(!m(Compare::Less).matches(posted));
        assert!(m(Compare::LessOrEqual).matches(posted));
        assert!(!m(Compare::Greater).matches(posted));
        assert!(m(Compare::GreaterOrEqual).matches(posted));
    }

    #[test]
    fn find_combines_criteria_and_sorts_and_limits() {
        let mut f = fixture();
        let a = simple(&mut f, 100);
        let b = simple(&mut f, 200);
        let lunch = Query::Memo(text(StringOp::Contains, "lunch"));
        let found = find(&f.book, &lunch, None).unwrap();
        assert_eq!(found.iter().map(|x| x.transaction).collect::<Vec<_>>(), [a, b], "same date and num: date entered, then GUID");
        let q = Query::All(vec![lunch.clone(), Query::Value(NumericMatch { compare: Compare::Greater, amount: usd(150), sign: Sign::Any })]);
        assert_eq!(find(&f.book, &q, None).unwrap().len(), 1);
        let all = Query::Accounts { how: AccountMatch::All, accounts: vec![f.bank, f.food] };
        assert_eq!(find(&f.book, &all, None).unwrap().len(), 4);
        assert_eq!(find(&f.book, &all, Some(1)).unwrap().len(), 1);
        assert_eq!(find(&f.book, &Query::Not(Box::new(Query::All(vec![]))), None).unwrap().len(), 0);
        let none = Query::Accounts { how: AccountMatch::Any, accounts: vec![] };
        assert!(find(&f.book, &none, None).is_err());
    }
}
