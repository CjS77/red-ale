//! `qif-import:qif-xtn-to-gnc-xtn`: one QIF transaction as a GnuCash transaction. `qif-to-gnc.scm:559-920`.
//!
//! GnuCash sets each split's value and amount before the split has a transaction or an account, so neither is
//! rounded then; joining the transaction rounds the value to the currency's fraction (`xaccSplitSetParent`,
//! `5.17:libgnucash/engine/Split.cpp:1905-1906`, with `xaccSplitSetValue`'s refusal of a non-zero value that rounds to
//! zero, `:1272-1309`), and the amount keeps the exact number read from the file. red-ale does the same.

use red_ale_core::ops::commit::{currency_fraction, set_date_posted, set_notes};
use red_ale_core::ops::transactions::void::void_detached;
use red_ale_core::{
    Book, CommodityRef, Date, DomainError, DomainResult, Frame, Guid, How, Numeric, ReconcileState, Round, Split, Time64, TimeZone,
    Transaction,
};

use super::super::dialog_utils::{action, default_unspec_acct, split_amount};
use super::super::objects::{Field, QifMapEntry, QifSplit, QifXtn};
use super::super::parse::{Action, Cleared, QifDate};
use super::Maps;
use super::accounts::Tree;
use super::transfers::accounts_affected;

/// What converting a transaction reads besides the transaction.
pub struct XtnContext<'a> {
    /// The QIF account, category and payee maps.
    pub maps: &'a Maps,
    /// The import's accounts.
    pub tree: &'a Tree,
    /// The default currency, every transaction's currency.
    pub currency: &'a CommodityRef,
    /// The reconcile state of a split with no `C` line (GnuCash's preference).
    pub status_pref: ReconcileState,
    /// The book's num field source: split actions hold the number.
    pub num_action: bool,
    /// The local time zone.
    pub zone: &'a TimeZone,
    /// Now: when the transaction is entered, and voided.
    pub now: Time64,
}

/// A transaction built and not yet committed, with the import's account behind each of its splits.
#[derive(Debug, Clone, PartialEq)]
pub struct Built {
    /// The transaction.
    pub transaction: Transaction,
    /// Each split's account in the import's tree, by split.
    pub nodes: Vec<(Guid, usize)>,
}

/// A QIF date as a calendar date; `None` for a day the month does not have.
pub fn qif_date(d: &QifDate) -> Option<Date> {
    Date::from_ymd(i32::try_from(d.year).ok()?, d.month, d.day)
}

/// `n+` (`GNC-DENOM-LCD`), `n-` and the like as GnuCash defines them at `qif-to-gnc.scm:49-53`.
fn n_add(a: Numeric, b: Numeric) -> DomainResult<Numeric> {
    Ok(a.add(b, 0, How::LCD_NEVER)?)
}

fn n_sub(a: Numeric, b: Numeric) -> DomainResult<Numeric> {
    Ok(a.sub(b, 0, How::LCD_NEVER)?)
}

fn n_mul(a: Numeric, b: Numeric) -> DomainResult<Numeric> {
    Ok(a.mul(b, 0, How::REDUCE_NEVER)?)
}

fn n_div(a: Numeric, b: Numeric) -> DomainResult<Numeric> {
    Ok(a.div(b, 0, How::REDUCE_NEVER)?)
}

/// The value a split keeps once it joins its transaction: rounded half up to the currency's fraction, unless that
/// gives zero for a non-zero value, which `xaccSplitSetValue` refuses.
fn joined_value(book: &Book, currency: &CommodityRef, value: Numeric) -> DomainResult<Numeric> {
    let rounded = value.to_denom(currency_fraction(book, currency)?, Round::HalfUp)?;
    Ok(if rounded.is_zero() && !value.is_zero() { value } else { rounded })
}

/// A split being built: its value and amount as set, its import account, memo, action and state.
struct Draft {
    value: Numeric,
    amount: Numeric,
    node: Option<usize>,
    memo: Option<String>,
    action: Option<String>,
    state: ReconcileState,
}

impl Draft {
    fn new() -> Self {
        Self {
            value: Numeric::ZERO,
            amount: Numeric::ZERO,
            node: None,
            memo: None,
            action: None,
            state: ReconcileState::New,
        }
    }

    fn both(mut self, n: Numeric) -> Self {
        self.value = n;
        self.amount = n;
        self
    }
}

/// The reconcile state a matching half's cleared flag gives a far split: `c` or `y`, else unchanged.
fn matching_state(cleared: Option<&Field<Cleared>>) -> Option<ReconcileState> {
    match cleared.and_then(Field::value) {
        Some(Cleared::Cleared) => Some(ReconcileState::Cleared),
        Some(Cleared::Reconciled) => Some(ReconcileState::Reconciled),
        _ => None,
    }
}

impl XtnContext<'_> {
    fn node(&self, gnc_name: &str) -> Option<usize> {
        self.tree.by_name.get(gnc_name).copied()
    }

    fn entry_node(&self, entry: Option<&QifMapEntry>) -> Option<usize> {
        entry.and_then(|e| e.gnc_name.as_deref()).and_then(|n| self.node(n))
    }

    /// `qif-import:qif-xtn-to-gnc-xtn`. `qif-to-gnc.scm:565-920`.
    pub fn convert(&self, book: &Book, x: &QifXtn, new_guid: &mut dyn FnMut() -> Guid) -> DomainResult<Built> {
        let date = x
            .date
            .as_ref()
            .and_then(Field::value)
            .ok_or_else(|| DomainError::invalid("Missing transaction date."))?;
        let date = qif_date(date).ok_or_else(|| DomainError::invalid(format!("line {}: no such day", x.line)))?;
        let mut t = Transaction {
            guid: new_guid(),
            currency: self.currency.clone(),
            num: None,
            date_posted: Time64(0),
            date_entered: self.now,
            description: x.payee.clone(),
            slots: Frame::default(),
            splits: Vec::new(),
        };
        set_date_posted(&mut t, date, self.zone);
        let mut near = Draft::new();
        if let Some(number) = &x.number {
            // `gnc-set-num-action` with no action (`5.17:gnucash/gnome-utils/gnc-ui-util.cpp`).
            if self.num_action {
                near.action = Some(number.clone())
            } else {
                t.num = Some(number.clone())
            }
        }
        let memo = x.default_split.as_ref().or(x.splits.first()).and_then(|s| s.memo.clone());
        if let Some(memo) = memo.as_deref() {
            if x.payee.as_deref().is_none_or(str::is_empty) {
                t.description = Some(memo.to_owned());
            } else {
                set_notes(&mut t, Some(memo));
            }
        }
        near.state = match x.cleared.as_ref().and_then(Field::value) {
            Some(Cleared::Cleared) => ReconcileState::Cleared,
            Some(Cleared::Reconciled) => ReconcileState::Reconciled,
            _ => self.status_pref,
        };
        let drafts = if x.security_name.is_none() {
            self.bank(x, near, memo.as_deref())?
        } else {
            self.stock(x, near)?
        };
        let mut nodes = Vec::new();
        t.splits = drafts
            .into_iter()
            .map(|d| {
                let s = Split {
                    guid: new_guid(),
                    memo: d.memo,
                    action: d.action,
                    reconcile_state: d.state,
                    reconcile_date: None,
                    value: joined_value(book, &t.currency, d.value)?,
                    amount: d.amount,
                    account: d.node.and_then(|n| self.tree.nodes.get(n)).map(|n| n.account),
                    lot: None,
                    slots: Frame::new(),
                };
                if let Some(node) = d.node {
                    nodes.push((s.guid, node));
                }
                Ok(s)
            })
            .collect::<DomainResult<_>>()?;
        if x.payee.as_deref().is_some_and(|p| p.starts_with("**VOID**")) {
            void_detached(book, self.now, &mut t, "QIF")?;
        }
        Ok(Built { transaction: t, nodes })
    }

    /// The far split's account: the account or category the split names, else a payee or memo mapping, else
    /// `Unspecified`. `qif-to-gnc.scm:670-703`.
    fn far_node(&self, x: &QifXtn, split: &QifSplit, xtn_memo: Option<&str>) -> Option<usize> {
        let maps = self.maps;
        let payee = |key: Option<&str>| key.filter(|k| !k.is_empty()).and_then(|k| maps.payees.get(k));
        let cat = split.category.as_str();
        let info = if !cat.is_empty() && split.category_is_account {
            maps.accounts.get(cat)
        } else if !cat.is_empty() {
            maps.categories.get(cat)
        } else if x.splits.len() == 1 {
            payee(x.payee.as_deref()).or_else(|| payee(xtn_memo))
        } else {
            payee(split.memo.as_deref().filter(|_| x.default_split.is_some()))
        };
        let name = info
            .and_then(|e| e.gnc_name.clone())
            .unwrap_or_else(|| default_unspec_acct().to_owned());
        self.node(&name)
    }

    /// A bank transaction: one far split per QIF split not marked as a transfer's other half, then the near split
    /// for their total. `qif-to-gnc.scm:633-719`.
    fn bank(&self, x: &QifXtn, mut near: Draft, xtn_memo: Option<&str>) -> DomainResult<Vec<Draft>> {
        let near_node = self.entry_node(x.from_acct.as_deref().and_then(|a| self.maps.accounts.get(a)));
        let mut total = Numeric::ZERO;
        let mut drafts = Vec::new();
        for split in x.splits.iter().filter(|s| !s.mark) {
            let amount = split_amount(split);
            total = n_add(total, amount)?;
            let mut far = Draft::new().both(amount.neg());
            far.memo = x.default_split.as_ref().and(split.memo.clone());
            far.node = self.far_node(x, split, xtn_memo);
            if let Some(state) = matching_state(split.matching_cleared.as_ref()) {
                far.state = state;
            }
            drafts.push(far);
        }
        near = near.both(total);
        near.node = near_node;
        drafts.push(near);
        Ok(drafts)
    }

    /// The account a stock transaction's QIF name maps to, through the account map then the category map.
    fn acct_or_cat(&self, name: &str) -> Option<&QifMapEntry> {
        self.maps.accounts.get(name).or_else(|| self.maps.categories.get(name))
    }

    /// A stock transaction: the near and far splits by action, and a commission split. `qif-to-gnc.scm:725-912`.
    fn stock(&self, x: &QifXtn, mut near: Draft) -> DomainResult<Vec<Draft>> {
        let Some(action) = action(x) else {
            return Err(DomainError::invalid("Missing QIF investment action."));
        };
        let value = |f: Option<&Field<Numeric>>| f.and_then(Field::value).copied();
        let num_shares = value(x.num_shares.as_ref()).unwrap_or(Numeric::ZERO);
        let xtn_amt = x.splits.first().map(split_amount);
        let commission = value(x.commission.as_ref());
        let split_amt = match (xtn_amt, commission) {
            (Some(amt), Some(c)) if matches!(action, Action::Sell | Action::SellX | Action::ShrsOut) => n_add(amt, c)?,
            (Some(amt), Some(c)) => n_sub(amt, c)?,
            (Some(amt), None) => amt,
            (None, _) => value(x.share_price.as_ref()).map_or(Ok(Numeric::ZERO), |p| n_mul(num_shares, p))?,
        };
        let xtn_amt = xtn_amt.unwrap_or(Numeric::ZERO);
        let Some(first) = x.splits.first() else { return Ok(Vec::new()) };
        let (qif_near, qif_far, qif_commission) = accounts_affected(first, x);
        let mut far = Draft::new();
        let ends = qif_near.as_deref().zip(qif_far.as_deref());
        if let Some((near_name, far_name)) = ends {
            near.node = self.entry_node(self.acct_or_cat(near_name));
            let payees = &self.maps.payees;
            let by_memo = || {
                x.payee
                    .as_deref()
                    .and_then(|p| payees.get(p))
                    .or_else(|| first.memo.as_deref().and_then(|m| payees.get(m)))
            };
            let far_info = if far_name.is_empty() { by_memo() } else { None };
            far.node = self.entry_node(far_info.or_else(|| self.acct_or_cat(far_name)));
        }
        self.amounts(action, &mut near, &mut far, num_shares, split_amt, xtn_amt)?;
        if let Some(state) = matching_state(first.matching_cleared.as_ref()) {
            far.state = state;
        }
        let commission_node = qif_commission.as_deref().and_then(|c| self.entry_node(self.acct_or_cat(c)));
        if ends.is_none() {
            return Ok(Vec::new());
        }
        let mut drafts = vec![near, far];
        if let (Some(amount), Some(node)) = (commission, commission_node) {
            let mut c = Draft::new().both(amount);
            c.node = Some(node);
            drafts.push(c);
        }
        Ok(drafts)
    }

    /// The amounts and signs of a stock transaction's two splits, by action. `qif-to-gnc.scm:817-876`.
    fn amounts(
        &self,
        action: Action,
        near: &mut Draft,
        far: &mut Draft,
        shares: Numeric,
        split_amt: Numeric,
        xtn_amt: Numeric,
    ) -> DomainResult<()> {
        use Action as A;
        match action {
            A::Buy | A::BuyX | A::ReinvInt | A::ReinvDiv | A::ReinvSg | A::ReinvSh | A::ReinvMd | A::ReinvLg | A::ShrsIn => {
                (near.amount, near.value) = (shares, split_amt);
                (far.value, far.amount) = (xtn_amt.neg(), xtn_amt.neg());
            }
            A::Sell | A::SellX | A::ShrsOut => {
                (near.amount, near.value) = (shares.neg(), split_amt.neg());
                (far.value, far.amount) = (xtn_amt, xtn_amt);
            }
            A::CgShort
            | A::CgShortX
            | A::CgMid
            | A::CgMidX
            | A::CgLong
            | A::CgLongX
            | A::IntInc
            | A::IntIncX
            | A::Div
            | A::DivX
            | A::MiscInc
            | A::MiscIncX
            | A::XIn
            | A::RtrnCap
            | A::RtrnCapX => {
                (near.value, near.amount) = (xtn_amt, xtn_amt);
                (far.value, far.amount) = (xtn_amt.neg(), xtn_amt.neg());
            }
            A::XOut | A::MiscExp | A::MiscExpX | A::MargInt | A::MargIntX => {
                (near.value, near.amount) = (xtn_amt.neg(), xtn_amt.neg());
                (far.value, far.amount) = (xtn_amt, xtn_amt);
            }
            A::StkSplit => {
                // The balance of the security's account in the import's tree, as converted so far.
                let ratio = n_div(shares, Numeric::from_int(10))?;
                let in_shares = near.node.and_then(|n| self.tree.nodes.get(n)).map_or(Numeric::ZERO, |n| n.balance);
                (near.amount, far.amount) = (n_mul(in_shares, ratio)?, in_shares.neg());
                (near.value, far.value) = (split_amt.neg(), split_amt);
            }
            A::Reminder | A::Unrecognized => {}
        }
        Ok(())
    }
}
