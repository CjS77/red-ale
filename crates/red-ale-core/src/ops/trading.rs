//! Trading accounts: the splits GnuCash keeps in `Trading:<namespace>:<mnemonic>` so that, in a Book that uses
//! trading accounts, every commodity's amounts balance inside each transaction as well as its values.
//!
//! GnuCash adds them in the trading branch of `xaccTransScrubImbalance` (`5.17:libgnucash/engine/Scrub.cpp:844-895`),
//! which every transaction commit runs. [`commit`] is that branch for red-ale's commits, which refuse unbalanced values
//! instead of adding an Imbalance split; the scrubs use [`clear`] and [`rebalance`] around their own balancing split.
//! See `design/api/trading-accounts.md` and `design/notes/engine-accounts-transactions.md` §9.6.

use std::sync::Arc;

use crate::book_options::uses_trading_accounts;
use crate::ops::commit::{account_scu, commodity_fraction};
use crate::{Account, AccountType, Book, CommodityRef, DomainError, DomainResult, Env, Guid, How, Numeric, Round, Split, Transaction};

/// The name of the top trading account. GnuCash translates it (`_("Trading")`, `Scrub.cpp:532`); red-ale does not.
pub const TRADING: &str = "Trading";

/// What rebalancing a transaction's trading splits did.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Rebalanced {
    /// Trading accounts created, parents first.
    pub accounts_created: Vec<Guid>,
    /// The trading splits removed before rebalancing.
    pub removed: Vec<Split>,
    /// The trading splits the transaction ends with, all new.
    pub added: Vec<Guid>,
}

/// One commodity's amount imbalance. A split whose account has no commodity counts under `None`.
pub type Imbalance = (Option<CommodityRef>, Numeric);

/// The commodity of a split's account, if it has an account and the account a commodity.
fn commodity_of(book: &Book, s: &Split) -> Option<CommodityRef> {
    s.account.and_then(|a| book.account(a)).and_then(|a| a.commodity.clone())
}

/// True for a split in a TRADING account.
pub fn is_trading_split(book: &Book, s: &Split) -> bool {
    s.account
        .and_then(|a| book.account(a))
        .is_some_and(|a| a.account_type == AccountType::Trading)
}

/// `gnc_monetary_list_add_monetary` (`5.17:libgnucash/engine/gnc-commodity.cpp:2441-2464`): add to the entry for the
/// commodity, or prepend a new entry.
fn add_to(list: &mut Vec<Imbalance>, commodity: Option<CommodityRef>, n: Numeric) -> DomainResult<()> {
    match list.iter_mut().find(|(c, _)| *c == commodity) {
        Some((_, sum)) => *sum = sum.add_exact(n)?,
        None => list.insert(0, (commodity, n)),
    }
    Ok(())
}

/// `xaccTransGetImbalance` with trading accounts on (`5.17:libgnucash/engine/Transaction.cpp:1006-1076`): once a split
/// is outside the transaction currency or its amount differs from its value, every later split's amount counts under
/// its account's commodity, and the values summed so far under the transaction currency. Zero entries are dropped.
/// The order is GnuCash's: the newest commodity first, the transaction currency last.
pub fn imbalances(book: &Book, t: &Transaction) -> DomainResult<Vec<Imbalance>> {
    let currency = Some(t.currency.clone());
    let (list, value) = t
        .splits
        .iter()
        .try_fold((None::<Vec<Imbalance>>, Numeric::ZERO), |(mut list, value), s| -> DomainResult<_> {
            let commodity = commodity_of(book, s);
            if list.is_some() || commodity != currency || !s.amount.same_value(s.value) {
                let l = list.get_or_insert_with(|| vec![(currency.clone(), value)]);
                add_to(l, commodity, s.amount)?;
            }
            Ok((list, value.add_exact(s.value)?))
        })?;
    let mut list = list.unwrap_or_else(|| vec![(currency, value)]);
    list.retain(|(_, n)| !n.is_zero());
    Ok(list)
}

/// The sums of the values of the non-trading and of the trading splits.
fn value_sums(book: &Book, t: &Transaction) -> DomainResult<(Numeric, Numeric)> {
    t.splits.iter().try_fold((Numeric::ZERO, Numeric::ZERO), |(other, trading), s| {
        Ok(if is_trading_split(book, s) {
            (other, trading.add_exact(s.value)?)
        } else {
            (other.add_exact(s.value)?, trading)
        })
    })
}

/// Why a transaction is not balanced by `xaccTransIsBalanced`'s rule with trading accounts
/// (`5.17:libgnucash/engine/Transaction.cpp:1078-1121`): non-trading values, trading values, and each commodity's
/// amounts must all sum to zero.
pub fn check(book: &Book, t: &Transaction) -> DomainResult<()> {
    let (other, trading) = value_sums(book, t)?;
    let unbalanced = |n: Numeric, c: &str| DomainError::Unbalanced {
        imbalance: n.to_decimal_string(),
        currency: c.to_owned(),
    };
    if !other.is_zero() {
        return Err(unbalanced(other, &t.currency.mnemonic));
    }
    if !trading.is_zero() {
        return Err(unbalanced(trading, &t.currency.mnemonic));
    }
    match imbalances(book, t)?.into_iter().next() {
        Some((c, n)) => Err(unbalanced(n, c.as_ref().map_or("", |c| c.mnemonic.as_str()))),
        None => Ok(()),
    }
}

/// `xaccTransIsBalanced` with trading accounts on.
pub fn is_balanced(book: &Book, t: &Transaction) -> DomainResult<bool> {
    match check(book, t) {
        Ok(()) => Ok(true),
        Err(DomainError::Unbalanced { .. }) => Ok(false),
        Err(e) => Err(e),
    }
}

/// The commit-time trading scrub for red-ale's commits. With the Book's option off, or the transaction balanced,
/// nothing changes and `None` is returned. Otherwise the trading splits are removed, the remaining values must sum to
/// zero (else `Unbalanced`, where GnuCash would add an Imbalance split), and the trading splits are rebuilt.
pub fn commit(book: &mut Book, env: &mut dyn Env, t: &mut Transaction) -> DomainResult<Option<Rebalanced>> {
    if !uses_trading_accounts(book) || is_balanced(book, t)? {
        return Ok(None);
    }
    let removed = clear(book, t);
    let imbalance = t.imbalance()?;
    if !imbalance.is_zero() {
        return Err(DomainError::Unbalanced {
            imbalance: imbalance.to_decimal_string(),
            currency: t.currency.mnemonic.clone(),
        });
    }
    let mut out = rebalance(book, env, t)?;
    out.removed = removed;
    Ok(Some(out))
}

/// `xaccTransClearTradingSplits` (`5.17:libgnucash/engine/Scrub.cpp:684-712`): remove every split in a TRADING account.
pub fn clear(book: &Book, t: &mut Transaction) -> Vec<Split> {
    let (trading, other): (Vec<Split>, Vec<Split>) = std::mem::take(&mut t.splits).into_iter().partition(|s| is_trading_split(book, s));
    t.splits = other;
    trading
}

/// Steps 5 and 6 of the trading branch, on a transaction whose values balance and which has no trading splits:
/// `gnc_transaction_balance_trading` (`Scrub.cpp:714-782`), then, if the values no longer balance,
/// `gnc_transaction_balance_trading_more_splits` (`:784-836`). The splits are not sorted.
pub fn rebalance(book: &mut Book, env: &mut dyn Env, t: &mut Transaction) -> DomainResult<Rebalanced> {
    let mut out = Rebalanced::default();
    imbalances(book, t)?
        .into_iter()
        .try_for_each(|(c, n)| balance_commodity(book, env, t, c.as_ref(), n, &mut out))?;
    if !t.imbalance()?.is_zero() {
        let before: Vec<Guid> = t.splits.iter().map(|s| s.guid).collect();
        before.into_iter().try_for_each(|g| offset_gain(book, env, t, g, &mut out))?;
    }
    out.added = t.splits.iter().filter(|s| is_trading_split(book, s)).map(|s| s.guid).collect();
    Ok(out)
}

/// One entry of `gnc_transaction_balance_trading`: the commodity's trading split takes the amount that cancels the
/// imbalance (at the commodity's fraction, then the account's SCU) and the value that cancels the commodity's values,
/// or, for the transaction currency, a value equal to its amount.
fn balance_commodity(
    book: &mut Book,
    env: &mut dyn Env,
    t: &mut Transaction,
    c: Option<&CommodityRef>,
    imbalance: Numeric,
    out: &mut Rebalanced,
) -> DomainResult<()> {
    let c =
        c.ok_or_else(|| DomainError::invalid("a split's account has no commodity, so it cannot be balanced through trading accounts"))?;
    let index = trading_split(book, env, t, c, out)?;
    let currency_fraction = fraction(book, &t.currency)?;
    let split = &t.splits[index];
    let new_amount = split.amount.sub(imbalance, fraction(book, c)?, How::round(Round::HalfUp))?;
    let new_value = if *c == t.currency {
        new_amount
    } else {
        let values = t
            .splits
            .iter()
            .filter(|s| commodity_of(book, s).as_ref() == Some(c))
            .try_fold(Numeric::ZERO, |acc, s| acc.add_exact(s.value))?;
        split.value.sub(values, currency_fraction, How::round(Round::HalfUp))?
    };
    let account = split
        .account
        .ok_or_else(|| DomainError::invalid("a trading split has no account"))?;
    let amount = new_amount.to_denom(account_scu(book, account)?, Round::HalfUp)?;
    let split = &mut t.splits[index];
    split.amount = amount;
    split.value = set_value(split.value, new_value, currency_fraction)?;
    scrub_split(book, &t.currency, &mut t.splits[index])
}

/// One split of `gnc_transaction_balance_trading_more_splits`: a split with a value but no amount (a realized gain)
/// is offset in the value of its commodity's trading split; that split's amount is left alone.
fn offset_gain(book: &mut Book, env: &mut dyn Env, t: &mut Transaction, guid: Guid, out: &mut Rebalanced) -> DomainResult<()> {
    let Some(s) = t.split(guid).filter(|s| !s.value.is_zero() && s.amount.is_zero()) else {
        return Ok(());
    };
    let value = s.value;
    // GnuCash logs and skips a split whose account has no commodity.
    let Some(c) = commodity_of(book, s) else { return Ok(()) };
    let index = trading_split(book, env, t, &c, out)?;
    let currency_fraction = fraction(book, &t.currency)?;
    let split = &mut t.splits[index];
    let new_value = split.value.sub(value, currency_fraction, How::round(Round::HalfUp))?;
    split.value = set_value(split.value, new_value, currency_fraction)?;
    scrub_split(book, &t.currency, &mut t.splits[index])
}

fn fraction(book: &Book, c: &CommodityRef) -> DomainResult<i64> {
    commodity_fraction(book, c).ok_or_else(|| DomainError::not_found("commodity", c))
}

/// `xaccSplitSetValue`'s rounding (`5.17:libgnucash/engine/Split.cpp:1279-1312`): a non-zero value that would round
/// to zero keeps the old value.
pub(crate) fn set_value(old: Numeric, new: Numeric, fraction: i64) -> DomainResult<Numeric> {
    let rounded = new.to_denom(fraction, Round::HalfUp)?;
    Ok(if rounded.is_zero() && !new.is_zero() { old } else { rounded })
}

/// `xaccSplitScrub` on a trading split (`5.17:libgnucash/engine/Scrub.cpp:236-347`, `:423-427`): in an account of the transaction
/// currency, an amount that differs from the value at `min(SCU, currency fraction)` becomes the value.
fn scrub_split(book: &Book, currency: &CommodityRef, s: &mut Split) -> DomainResult<()> {
    let Some(account) = s
        .account
        .and_then(|a| book.account(a))
        .filter(|a| a.commodity.as_ref() == Some(currency))
    else {
        return Ok(());
    };
    let scu = account_scu(book, account.guid)?;
    let denom = scu.min(fraction(book, currency)?);
    if s.amount.to_denom(denom, Round::HalfUp)? != s.value.to_denom(denom, Round::HalfUp)? {
        s.amount = s.value.to_denom(scu, Round::HalfUp)?;
    }
    Ok(())
}

/// `get_trading_split` (`5.17:libgnucash/engine/Scrub.cpp:518-579`): the index of the transaction's split in the
/// commodity's trading account, appending a new zero split when there is none.
fn trading_split(book: &mut Book, env: &mut dyn Env, t: &mut Transaction, c: &CommodityRef, out: &mut Rebalanced) -> DomainResult<usize> {
    let account = trading_account(book, env, c, &mut out.accounts_created)?;
    if let Some(index) = t.splits.iter().position(|s| s.account == Some(account)) {
        return Ok(index);
    }
    let scu = account_scu(book, account)?;
    t.splits
        .push(Split::new(env.new_guid(), account, Numeric::ZERO, Numeric::new(0, scu)));
    Ok(t.splits.len() - 1)
}

/// The commodity's trading account, `Trading:<namespace>:<mnemonic>`, found or made level by level as
/// `get_trading_split` does. New accounts are appended to `created`.
pub fn trading_account(book: &mut Book, env: &mut dyn Env, c: &CommodityRef, created: &mut Vec<Guid>) -> DomainResult<Guid> {
    let root = book.root;
    let top_wanted = Wanted {
        parent: root,
        commodity: None,
        name: TRADING,
        placeholder: true,
        check_name: false,
    };
    let top = get_or_make(book, env, top_wanted, created)?;
    let ns_wanted = Wanted {
        parent: top,
        commodity: None,
        name: &c.namespace,
        placeholder: true,
        check_name: true,
    };
    let ns = get_or_make(book, env, ns_wanted, created)?;
    get_or_make(
        book,
        env,
        Wanted {
            parent: ns,
            commodity: Some(c),
            name: &c.mnemonic,
            placeholder: false,
            check_name: false,
        },
        created,
    )
}

/// What `xaccScrubUtilityGetOrMakeAccount` is asked for, always with type TRADING.
#[derive(Clone, Copy)]
struct Wanted<'a> {
    parent: Guid,
    commodity: Option<&'a CommodityRef>,
    name: &'a str,
    placeholder: bool,
    check_name: bool,
}

/// `xaccScrubUtilityGetOrMakeAccount` (`5.17:libgnucash/engine/Scrub.cpp:1507-1540`) for type TRADING.
fn get_or_make(book: &mut Book, env: &mut dyn Env, w: Wanted<'_>, created: &mut Vec<Guid>) -> DomainResult<Guid> {
    let found = lookup(book, w.parent, w.check_name.then_some(w.name), AccountType::Trading, w.commodity);
    let root_currency = root_currency(book);
    let pick = || {
        let in_root_currency = || {
            found
                .iter()
                .find(|g| w.commodity.is_none() && book.account(**g).is_some_and(|a| a.commodity == root_currency))
        };
        let named = || found.iter().find(|g| book.account(**g).is_some_and(|a| a.name == w.name));
        if found.len() > 1 {
            in_root_currency().or_else(named).or(found.first())
        } else {
            found.first()
        }
        .copied()
    };
    if let Some(g) = pick() {
        return Ok(g);
    }
    let commodity = w.commodity.cloned().or(root_currency);
    let mut account = Account::new(env.new_guid(), w.name, AccountType::Trading, commodity.clone(), Some(w.parent));
    account.commodity_scu = commodity.as_ref().map(|c| fraction(book, c)).transpose()?;
    account.set_placeholder(w.placeholder);
    let guid = account.guid;
    book.accounts.insert(guid, Arc::new(account));
    created.push(guid);
    Ok(guid)
}

/// `find_root_currency` (`5.17:libgnucash/engine/Scrub.cpp:493-516`): the root account's commodity, else that of the
/// first top-level INCOME account that has one.
pub fn root_currency(book: &Book) -> Option<CommodityRef> {
    book.account(book.root).and_then(|r| r.commodity.clone()).or_else(|| {
        book.children(book.root)
            .filter(|a| a.account_type == AccountType::Income)
            .find_map(|a| a.commodity.clone())
    })
}

/// The accounts GnuCash's lookup finds, in its order (`gnc_account_lookup_by_type_and_commodity`,
/// `5.17:libgnucash/engine/Account.cpp:3176-3210`): matching direct children, last first; failing those, each child's
/// matches, last child first. A `None` name or commodity matches any.
pub fn lookup(book: &Book, parent: Guid, name: Option<&str>, account_type: AccountType, commodity: Option<&CommodityRef>) -> Vec<Guid> {
    let matches = |a: &&Account| {
        a.account_type == account_type && commodity.is_none_or(|c| a.commodity.as_ref() == Some(c)) && name.is_none_or(|n| a.name == n)
    };
    let mut direct: Vec<Guid> = book.children(parent).filter(matches).map(|a| a.guid).collect();
    if !direct.is_empty() {
        direct.reverse();
        return direct;
    }
    let children: Vec<Guid> = book.children(parent).map(|a| a.guid).collect();
    children
        .into_iter()
        .rev()
        .flat_map(|c| lookup(book, c, name, account_type, commodity))
        .collect()
}

#[cfg(test)]
mod tests;
