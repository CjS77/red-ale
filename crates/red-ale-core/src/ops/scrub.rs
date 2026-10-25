//! Data-integrity scrubs: GnuCash's orphan, split, currency and imbalance scrubs and its Check & Repair
//! (`5.17:libgnucash/engine/Scrub.cpp`). Each scrub may deliberately change data, so each returns the list of
//! [`Repair`]s it made. See `design/api/scrub.md` and `design/notes/engine-accounts-transactions.md` §9.
//!
//! GnuCash runs `xaccTransScrubImbalance` whenever a transaction is committed
//! (`5.17:libgnucash/engine/Transaction.cpp:1549-1566`), so every transaction a scrub changes is also balanced; these
//! functions do the same.

use std::sync::Arc;

use crate::book_options::uses_trading_accounts;
use crate::ops::commit::{commodity_fraction, sort_splits};
use crate::ops::trading::{self, lookup, set_value};
use crate::{
    Account, AccountType, Book, CommodityRef, DENOM_AUTO, DomainError, DomainResult, Env, Guid, How, Numeric, Round, Split, Transaction,
};

/// The name GnuCash gives balancing accounts, before `-<currency>`. GnuCash translates it (`_("Imbalance")`,
/// `Scrub.cpp:464`); red-ale always uses the English name.
pub const IMBALANCE: &str = "Imbalance";
/// The name GnuCash gives the accounts orphaned splits go to, before `-<currency>` (`_("Orphan")`, `Scrub.cpp:121`).
pub const ORPHAN: &str = "Orphan";

/// What a scrub works on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// One transaction.
    Transaction(Guid),
    /// The transactions with a split in this account.
    Account(Guid),
    /// The transactions with a split in this account or a descendant.
    Tree(Guid),
    /// Every transaction in the book.
    Book,
}

/// One change a scrub made.
#[derive(Debug, Clone, PartialEq)]
pub enum Repair {
    /// A balancing or orphan account was created under the root.
    AccountCreated {
        /// The new account.
        account: Guid,
        /// Its name.
        name: String,
        /// Its commodity.
        commodity: CommodityRef,
    },
    /// A split with no account was put in an `Orphan-<CUR>` account.
    OrphanAssigned {
        /// The transaction.
        transaction: Guid,
        /// The split.
        split: Guid,
        /// The account it now belongs to.
        account: Guid,
    },
    /// An invalid value or amount was set to zero.
    InvalidNumberZeroed {
        /// The transaction.
        transaction: Guid,
        /// The split.
        split: Guid,
        /// `value` or `amount`.
        field: &'static str,
    },
    /// A split in an account of the transaction currency had an amount different from its value.
    AmountSetToValue {
        /// The transaction.
        transaction: Guid,
        /// The split.
        split: Guid,
        /// The amount before.
        from: Numeric,
        /// The amount after.
        to: Numeric,
    },
    /// A transaction whose currency was not a currency got one.
    CurrencySet {
        /// The transaction.
        transaction: Guid,
        /// The commodity before.
        from: CommodityRef,
        /// The currency after.
        to: CommodityRef,
    },
    /// A split's value was re-expressed in the transaction's new currency.
    ValueChanged {
        /// The transaction.
        transaction: Guid,
        /// The split.
        split: Guid,
        /// The value before.
        from: Numeric,
        /// The value after.
        to: Numeric,
    },
    /// A balancing split was added.
    BalanceSplitAdded {
        /// The transaction.
        transaction: Guid,
        /// The new split.
        split: Guid,
        /// Its account.
        account: Guid,
        /// Its value.
        value: Numeric,
    },
    /// An existing balancing split's value was changed.
    BalanceSplitChanged {
        /// The transaction.
        transaction: Guid,
        /// The split.
        split: Guid,
        /// Its account.
        account: Guid,
        /// The value before.
        from: Numeric,
        /// The value after.
        to: Numeric,
    },
    /// A balancing split that would have been zero, with no memo or action, was deleted.
    BalanceSplitRemoved {
        /// The transaction.
        transaction: Guid,
        /// The deleted split.
        split: Guid,
        /// Its account.
        account: Guid,
        /// Its value before.
        value: Numeric,
    },
    /// The lot scrub changed an account's lots or splits.
    LotsRepaired {
        /// The account.
        account: Guid,
    },
    /// A trading split was removed before the transaction's trading splits were rebuilt.
    TradingSplitRemoved {
        /// The transaction.
        transaction: Guid,
        /// The removed split.
        split: Guid,
        /// Its account.
        account: Guid,
        /// Its value.
        value: Numeric,
        /// Its amount.
        amount: Numeric,
    },
    /// A trading split was added.
    TradingSplitAdded {
        /// The transaction.
        transaction: Guid,
        /// The new split.
        split: Guid,
        /// Its account.
        account: Guid,
        /// Its value.
        value: Numeric,
        /// Its amount.
        amount: Numeric,
    },
}

/// The book being scrubbed, its environment, and the repairs made so far.
struct Scrubber<'a> {
    book: &'a mut Book,
    env: &'a mut dyn Env,
    repairs: Vec<Repair>,
}

fn txn_mut(book: &mut Book, guid: Guid) -> DomainResult<&mut Transaction> {
    book.transactions.get_mut(&guid).map(Arc::make_mut).ok_or_else(|| DomainError::not_found("transaction", &guid))
}

fn txn(book: &Book, guid: Guid) -> DomainResult<&Transaction> {
    book.transaction(guid).ok_or_else(|| DomainError::not_found("transaction", &guid))
}

fn fraction(book: &Book, commodity: &CommodityRef) -> DomainResult<i64> {
    commodity_fraction(book, commodity).ok_or_else(|| DomainError::not_found("commodity", commodity))
}

fn scu(book: &Book, account: &Account) -> DomainResult<i64> {
    let commodity = account.commodity.as_ref().ok_or_else(|| DomainError::invalid(format!("account {} has no commodity", account.name)))?;
    Ok(account.scu(fraction(book, commodity)?))
}

impl Scrubber<'_> {
    /// `xaccScrubUtilityGetOrMakeAccount(root, currency, "<prefix>-<CUR>", BANK, FALSE, TRUE)`
    /// (`5.17:libgnucash/engine/Scrub.cpp:1457-1540`): the account found anywhere in the tree, else a new BANK account
    /// in the currency appended under the root, with the currency's fraction as its SCU (`xaccAccountSetCommodity`,
    /// `Account.cpp:2683`) and no placeholder flag.
    fn get_or_make(&mut self, prefix: &str, currency: &CommodityRef) -> DomainResult<Guid> {
        let name = format!("{prefix}-{}", currency.mnemonic);
        if let Some(found) = lookup(self.book, self.book.root, Some(&name), AccountType::Bank, Some(currency)).first() {
            return Ok(*found);
        }
        let mut account = Account::new(self.env.new_guid(), name.clone(), AccountType::Bank, Some(currency.clone()), Some(self.book.root));
        account.commodity_scu = Some(fraction(self.book, currency)?);
        let guid = account.guid;
        self.book.accounts.insert(guid, Arc::new(account));
        self.repairs.push(Repair::AccountCreated { account: guid, name, commodity: currency.clone() });
        Ok(guid)
    }

    /// `TransScrubOrphansFast` (`Scrub.cpp:110-136`): every split with no account goes to `Orphan-<CUR>`. In red-ale
    /// there is one account tree, so `xaccTransScrubOrphans`, which first looks for a root, is the same. A split whose
    /// account GUID names no account is an orphan too: GnuCash's reader leaves such a split with no account
    /// (`5.17:libgnucash/backend/xml/gnc-transaction-xml-v2.cpp:273-292`).
    fn orphans(&mut self, guid: Guid) -> DomainResult<bool> {
        let book: &Book = self.book;
        let t = txn(book, guid)?;
        let orphans: Vec<Guid> = t.splits.iter().filter(|s| is_orphan(book, s)).map(|s| s.guid).collect();
        if orphans.is_empty() {
            return Ok(false);
        }
        let currency = t.currency.clone();
        let account = self.get_or_make(ORPHAN, &currency)?;
        let t = txn_mut(self.book, guid)?;
        t.splits.iter_mut().filter(|s| orphans.contains(&s.guid)).for_each(|s| s.account = Some(account));
        self.repairs.extend(orphans.into_iter().map(|split| Repair::OrphanAssigned { transaction: guid, split, account }));
        Ok(true)
    }

    /// `xaccTransScrubSplits` (`Scrub.cpp:236-430`): orphans first, then invalid numbers to zero, then, for splits whose
    /// account holds the transaction currency, the amount set to the value when the two differ at
    /// `min(account SCU, currency fraction)`. GnuCash also gives an account with no commodity one
    /// (`xaccAccountScrubCommodity`); red-ale leaves such accounts alone.
    fn splits(&mut self, guid: Guid) -> DomainResult<()> {
        self.orphans(guid)?;
        let t = txn(self.book, guid)?;
        let currency = t.currency.clone();
        let currency_fraction = fraction(self.book, &currency)?;
        let fixes = t.splits.iter().map(|s| self.split_fixes(s, &currency, currency_fraction)).collect::<DomainResult<Vec<_>>>()?;
        let t = txn_mut(self.book, guid)?;
        let repairs: Vec<Repair> = t.splits.iter_mut().zip(fixes).flat_map(|(s, fix)| fix.apply(guid, s)).collect();
        self.repairs.extend(repairs);
        Ok(())
    }

    fn split_fixes(&self, s: &Split, currency: &CommodityRef, currency_fraction: i64) -> DomainResult<SplitFix> {
        let Some(account) = s.account.and_then(|a| self.book.account(a)) else { return Ok(SplitFix::default()) };
        let account_scu = account.commodity.as_ref().map(|_| scu(self.book, account)).transpose()?;
        let value = if s.value.is_valid() { None } else { Some(Numeric::new(0, currency_fraction)) };
        let amount = if s.amount.is_valid() { None } else { Some(Numeric::new(0, account_scu.unwrap_or(1))) };
        let (v, a) = (value.unwrap_or(s.value), amount.unwrap_or(s.amount));
        let to_value = match account_scu {
            Some(scu) if account.commodity.as_ref() == Some(currency) && !same(a, v, scu.min(currency_fraction)) => {
                Some(v.to_denom(scu, Round::HalfUp)?)
            }
            _ => None,
        };
        Ok(SplitFix { value, amount, to_value })
    }

    /// `xaccTransScrubCurrency` (`Scrub.cpp:1120-1235`): orphans first; then a transaction whose currency is not a
    /// currency gets the most common one among its splits' accounts, and splits in that currency get amount = value.
    fn currency(&mut self, guid: Guid) -> DomainResult<()> {
        self.orphans(guid)?;
        let t = txn(self.book, guid)?;
        if t.currency.is_currency() {
            return Ok(());
        }
        if let Some(found) = common_currency(self.book, t) {
            self.set_currency(guid, &found)?;
            return self.amounts_to_values(guid, &found);
        }
        let first = t.splits.iter().find_map(|s| s.account).and_then(|a| self.book.account(a)).and_then(|a| a.commodity.clone());
        match first {
            Some(c) => self.set_currency(guid, &c),
            None => Ok(()),
        }
    }

    fn amounts_to_values(&mut self, guid: Guid, currency: &CommodityRef) -> DomainResult<()> {
        let book: &Book = self.book;
        let t = txn(book, guid)?;
        let changes = t
            .splits
            .iter()
            .filter(|s| s.amount != s.value)
            .filter_map(|s| s.account.and_then(|a| book.account(a)).filter(|a| a.commodity.as_ref() == Some(currency)).map(|a| (s, a)))
            .map(|(s, a)| Ok((s.guid, s.amount, s.value.to_denom(scu(book, a)?, Round::HalfUp)?)))
            .collect::<DomainResult<Vec<_>>>()?;
        let t = txn_mut(self.book, guid)?;
        changes.iter().for_each(|(split, _, to)| {
            if let Some(s) = t.split_mut(*split) {
                s.amount = *to;
            }
        });
        self.repairs.extend(changes.into_iter().map(|(split, from, to)| Repair::AmountSetToValue { transaction: guid, split, from, to }));
        Ok(())
    }

    /// `xaccTransSetCurrency` (`5.17:libgnucash/engine/Transaction.cpp:1257-1335`): re-express each value in the new
    /// currency through the rate a split in that currency implies, or re-round the values when there is none.
    fn set_currency(&mut self, guid: Guid, currency: &CommodityRef) -> DomainResult<()> {
        let new_fraction = fraction(self.book, currency)?;
        let book: &Book = self.book;
        let t = txn(book, guid)?;
        let old = t.currency.clone();
        let commodity_of = |s: &Split| s.account.and_then(|a| book.account(a)).and_then(|a| a.commodity.clone());
        let rate = t
            .splits
            .iter()
            .find(|s| commodity_of(s).as_ref() == Some(currency))
            .and_then(|s| s.amount.div(s.value, DENOM_AUTO, How::round(Round::Never)).ok())
            .filter(|r| !r.is_zero());
        let values = t
            .splits
            .iter()
            .map(|s| {
                let target = revalued(s, commodity_of(s).as_ref(), currency, &old, rate, new_fraction)?;
                Ok((s.guid, s.value, target.map(|v| set_value(s.value, v, new_fraction)).transpose()?))
            })
            .collect::<DomainResult<Vec<_>>>()?;
        let t = txn_mut(self.book, guid)?;
        t.currency = currency.clone();
        self.repairs.push(Repair::CurrencySet { transaction: guid, from: old, to: currency.clone() });
        let changed: Vec<Repair> = values
            .into_iter()
            .filter_map(|(split, from, to)| to.filter(|v| !v.same_repr(from)).map(|to| (split, from, to)))
            .map(|(split, from, to)| {
                if let Some(s) = t.split_mut(split) {
                    s.value = to;
                }
                Repair::ValueChanged { transaction: guid, split, from, to }
            })
            .collect();
        self.repairs.extend(changed);
        Ok(())
    }

    /// `xaccTransScrubImbalance(trans, root, account)` (`Scrub.cpp:844-895`): scrub the splits, then balance the
    /// values with a split in `account` (when it holds the transaction currency) or in `Imbalance-<CUR>`. With
    /// trading accounts, the trading branch instead.
    fn imbalance(&mut self, guid: Guid, account: Option<Guid>) -> DomainResult<()> {
        self.splits(guid)?;
        if uses_trading_accounts(self.book) {
            return self.imbalance_trading(guid, account);
        }
        let t = txn(self.book, guid)?;
        let imbalance = t.splits.iter().try_fold(Numeric::ZERO, |acc, s| acc.add_exact(s.value))?;
        if imbalance.is_zero() {
            return Ok(());
        }
        self.add_balance_split(guid, imbalance, account)
    }

    /// The trading branch of `xaccTransScrubImbalance` (`Scrub.cpp:864-894`): unless the transaction is balanced with
    /// trading accounts, remove its trading splits, balance the values with an Imbalance split, and rebuild the
    /// trading splits.
    fn imbalance_trading(&mut self, guid: Guid, account: Option<Guid>) -> DomainResult<()> {
        if trading::is_balanced(self.book, txn(self.book, guid)?)? {
            return Ok(());
        }
        let mut t = txn(self.book, guid)?.clone();
        let removed = trading::clear(self.book, &mut t);
        let imbalance = t.imbalance()?;
        *txn_mut(self.book, guid)? = t;
        self.repairs.extend(removed.into_iter().map(|s| Repair::TradingSplitRemoved {
            transaction: guid,
            split: s.guid,
            account: s.account.unwrap_or_default(),
            value: s.value,
            amount: s.amount,
        }));
        if !imbalance.is_zero() {
            self.add_balance_split(guid, imbalance, account)?;
        }
        let mut t = txn(self.book, guid)?.clone();
        let rebalanced = trading::rebalance(self.book, self.env, &mut t)?;
        sort_splits(&mut t);
        let book: &Book = self.book;
        let created = rebalanced.accounts_created.iter().filter_map(|g| book.account(*g)).filter_map(|a| {
            // A trading parent made in a Book with no root currency has no commodity, and no repair to describe it.
            a.commodity.clone().map(|commodity| Repair::AccountCreated { account: a.guid, name: a.name.clone(), commodity })
        });
        let added = rebalanced.added.iter().filter_map(|g| t.split(*g)).map(|s| Repair::TradingSplitAdded {
            transaction: guid,
            split: s.guid,
            account: s.account.unwrap_or_default(),
            value: s.value,
            amount: s.amount,
        });
        let repairs: Vec<Repair> = created.chain(added).collect();
        self.repairs.extend(repairs);
        *txn_mut(self.book, guid)? = t;
        Ok(())
    }

    /// `add_balance_split` (`Scrub.cpp:582-636`). A balancing split that comes to zero and has no memo or action is
    /// deleted (`:607-619`, GnuCash 5.13 and later).
    fn add_balance_split(&mut self, guid: Guid, imbalance: Numeric, hint: Option<Guid>) -> DomainResult<()> {
        let currency = txn(self.book, guid)?.currency.clone();
        let account = match hint.and_then(|h| self.book.account(h)) {
            Some(a) if a.commodity.as_ref() == Some(&currency) => a.guid,
            _ => self.get_or_make(IMBALANCE, &currency)?,
        };
        let currency_fraction = fraction(self.book, &currency)?;
        let account_scu = self.book.account(account).map(|a| scu(self.book, a)).transpose()?.unwrap_or(currency_fraction);
        let new_guid = self.env.new_guid();
        let t = txn_mut(self.book, guid)?;
        let existing = t.splits.iter().position(|s| s.account == Some(account));
        let index = existing.unwrap_or_else(|| {
            t.splits.push(Split::new(new_guid, account, Numeric::ZERO, Numeric::ZERO));
            t.splits.len() - 1
        });
        let split = &t.splits[index];
        let (split_guid, old) = (split.guid, split.value);
        let new = old.sub(imbalance, currency_fraction, How::round(Round::HalfUp))?;
        let blank = |text: &Option<String>| text.as_deref().is_none_or(str::is_empty);
        if new.is_zero() && blank(&split.memo) && blank(&split.action) {
            t.splits.remove(index);
            if existing.is_some() {
                self.repairs.push(Repair::BalanceSplitRemoved { transaction: guid, split: split_guid, account, value: old });
            }
            return Ok(());
        }
        let split = &mut t.splits[index];
        split.value = set_value(split.value, new, currency_fraction)?;
        split.amount = new.to_denom(account_scu, Round::HalfUp)?;
        sort_splits(t);
        self.repairs.push(match existing {
            Some(_) => Repair::BalanceSplitChanged { transaction: guid, split: split_guid, account, from: old, to: new },
            None => Repair::BalanceSplitAdded { transaction: guid, split: split_guid, account, value: new },
        });
        Ok(())
    }

    /// Commit a transaction a scrub changed: GnuCash's commit runs the imbalance scrub.
    fn commit(&mut self, guid: Guid) -> DomainResult<()> {
        self.imbalance(guid, None)
    }
}

/// What the split scrub changes on one split.
#[derive(Debug, Default)]
struct SplitFix {
    value: Option<Numeric>,
    amount: Option<Numeric>,
    to_value: Option<Numeric>,
}

impl SplitFix {
    fn apply(self, transaction: Guid, s: &mut Split) -> Vec<Repair> {
        let mut out = Vec::new();
        let split = s.guid;
        if let Some(v) = self.value {
            s.value = v;
            out.push(Repair::InvalidNumberZeroed { transaction, split, field: "value" });
        }
        if let Some(a) = self.amount {
            s.amount = a;
            out.push(Repair::InvalidNumberZeroed { transaction, split, field: "amount" });
        }
        if let Some(to) = self.to_value {
            out.push(Repair::AmountSetToValue { transaction, split, from: s.amount, to });
            s.amount = to;
        }
        out
    }
}

/// The value `split_set_new_value` gives a split when its transaction's currency changes
/// (`5.17:libgnucash/engine/Transaction.cpp:1281-1301`): without a rate, the value itself, re-rounded; a split in the
/// new currency takes its amount; a split in the old currency is priced at the rate; any other at its own price over
/// the rate.
fn revalued(
    s: &Split,
    commodity: Option<&CommodityRef>,
    new: &CommodityRef,
    old: &CommodityRef,
    rate: Option<Numeric>,
    fraction: i64,
) -> DomainResult<Option<Numeric>> {
    let div = |a: Numeric, b: Numeric| a.div(b, DENOM_AUTO, How::round(Round::Never)).ok();
    match rate {
        None => Ok(Some(s.value)),
        Some(_) if commodity == Some(new) => Ok(Some(s.amount)),
        Some(r) if commodity == Some(old) => price_value(s.amount, r, fraction),
        Some(r) => div(s.value, s.amount).and_then(|own| div(own, r)).map_or(Ok(None), |p| price_value(s.amount, p, fraction)),
    }
}

/// A split with no account, or whose account is not in the book.
fn is_orphan(book: &Book, s: &Split) -> bool {
    s.account.is_none_or(|a| book.account(a).is_none())
}

/// `gnc_numeric_same(a, b, denom, ROUND_HALF_UP)`: equal once both are rounded to `denom`.
fn same(a: Numeric, b: Numeric, denom: i64) -> bool {
    matches!((a.to_denom(denom, Round::HalfUp), b.to_denom(denom, Round::HalfUp)), (Ok(x), Ok(y)) if x == y)
}

/// `xaccSplitSetSharePrice` (`Split.cpp:1205-1224`): value = amount × price; a zero price changes nothing.
fn price_value(amount: Numeric, price: Numeric, fraction: i64) -> DomainResult<Option<Numeric>> {
    if price.is_zero() {
        return Ok(None);
    }
    Ok(Some(amount.mul(price, fraction, How::round(Round::HalfUp))?))
}

/// `gnc_account_get_currency_or_parent` (`Account.cpp:3403-3412`).
fn currency_or_parent(book: &Book, account: &Account) -> Option<CommodityRef> {
    std::iter::successors(Some(account), |a| a.parent.and_then(|p| book.account(p)))
        .find_map(|a| a.commodity.clone().filter(CommodityRef::is_currency))
}

/// `xaccTransFindCommonCurrency` (`Scrub.cpp:1055-1118`): each non-trading split weighs 3 for its account's currency,
/// else 1 for the nearest ancestor's currency. GnuCash sorts the tallies ascending with a stable sort and takes the
/// head, so the lowest tally wins and ties go to the first seen; red-ale does the same. The fallback through the
/// obsolete per-account currency slots is not modelled.
fn common_currency(book: &Book, t: &Transaction) -> Option<CommodityRef> {
    let mut tallies: Vec<(CommodityRef, u32)> = Vec::new();
    t.splits
        .iter()
        .filter_map(|s| s.account.and_then(|a| book.account(a)))
        .filter(|a| a.account_type != AccountType::Trading)
        .filter_map(|a| match &a.commodity {
            Some(c) if c.is_currency() => Some((c.clone(), 3)),
            _ => currency_or_parent(book, a).map(|c| (c, 1)),
        })
        .for_each(|(c, weight)| match tallies.iter_mut().find(|(k, _)| *k == c) {
            Some((_, count)) => *count += weight,
            None => tallies.push((c, weight)),
        });
    tallies.sort_by_key(|(_, count)| *count);
    tallies.into_iter().next().map(|(c, _)| c)
}

/// The transactions a scope covers, in book order.
fn transactions_in(book: &Book, scope: Scope) -> DomainResult<Vec<Guid>> {
    let accounts: Vec<Guid> = match scope {
        Scope::Transaction(g) => return txn(book, g).map(|t| vec![t.guid]),
        Scope::Book => return Ok(book.transactions.keys().copied().collect()),
        Scope::Account(a) => vec![checked_account(book, a)?],
        Scope::Tree(a) => std::iter::once(checked_account(book, a)?).chain(book.descendants(a).iter().map(|d| d.guid)).collect(),
    };
    Ok(book
        .transactions
        .values()
        .filter(|t| t.splits.iter().any(|s| s.account.is_some_and(|a| accounts.contains(&a))))
        .map(|t| t.guid)
        .collect())
}

fn checked_account(book: &Book, account: Guid) -> DomainResult<Guid> {
    book.account(account).map(|a| a.guid).ok_or_else(|| DomainError::not_found("account", &account))
}

/// The orphan scrub (`xaccTransScrubOrphans`, `xaccAccountScrubOrphans`, `xaccAccountTreeScrubOrphans`,
/// `Scrub.cpp:138-210`). Each transaction changed is committed, which balances it as GnuCash's commit does.
pub fn scrub_orphans(book: &mut Book, env: &mut dyn Env, scope: Scope) -> DomainResult<Vec<Repair>> {
    let transactions = transactions_in(book, scope)?;
    let mut s = Scrubber { book, env, repairs: Vec::new() };
    transactions.into_iter().try_for_each(|t| if s.orphans(t)? { s.commit(t) } else { Ok(()) })?;
    Ok(s.repairs)
}

/// The imbalance scrub. For a transaction, the register's Check & Repair: orphans, then `xaccTransScrubImbalance`
/// (`5.17:gnucash/gnome/gnc-plugin-page-register.cpp:3542-3680`). For an account, a tree or the book,
/// `AccountScrubImbalance` (`Scrub.cpp:353-391`): orphans, currency, then imbalance, for each transaction in book order
/// (GnuCash's order is unspecified).
pub fn scrub_imbalance(book: &mut Book, env: &mut dyn Env, scope: Scope) -> DomainResult<Vec<Repair>> {
    let transactions = transactions_in(book, scope)?;
    let by_account = !matches!(scope, Scope::Transaction(_));
    let mut s = Scrubber { book, env, repairs: Vec::new() };
    transactions.into_iter().try_for_each(|t| {
        s.orphans(t)?;
        if by_account {
            s.currency(t)?;
        }
        s.imbalance(t, None)
    })?;
    Ok(s.repairs)
}

/// The currency scrub alone (`xaccTransScrubCurrency`), with the commit-time imbalance scrub on each transaction it
/// changes.
pub fn scrub_currency(book: &mut Book, env: &mut dyn Env, scope: Scope) -> DomainResult<Vec<Repair>> {
    let transactions = transactions_in(book, scope)?;
    let mut s = Scrubber { book, env, repairs: Vec::new() };
    transactions.into_iter().try_for_each(|t| {
        let before = s.repairs.len();
        s.currency(t)?;
        if s.repairs.len() > before { s.commit(t) } else { Ok(()) }
    })?;
    Ok(s.repairs)
}

/// GnuCash's Check & Repair (`5.17:gnucash/gnome/gnc-plugin-page-account-tree.cpp:1940-2027`): the orphan scrub, the
/// imbalance scrub, and, only when `lots` is set (GnuCash: the `GNC_AUTO_SCRUB_LOTS` environment variable), the lot
/// scrub of each account, descendants before the account (`xaccAccountTreeScrubLots`, `Scrub3.cpp:190-196`). A
/// transaction scope is the register's Check & Repair, which has no lot step.
pub fn check_and_repair(book: &mut Book, env: &mut dyn Env, scope: Scope, lots: bool) -> DomainResult<Vec<Repair>> {
    let mut repairs = scrub_orphans(book, env, scope)?;
    repairs.extend(scrub_imbalance(book, env, scope)?);
    if lots {
        let accounts: Vec<Guid> = match scope {
            Scope::Transaction(_) => return Err(DomainError::invalid("the lot scrub applies to accounts, not to one transaction")),
            Scope::Account(a) => vec![a],
            Scope::Tree(a) => book.descendants(a).iter().map(|d| d.guid).chain(std::iter::once(a)).collect(),
            Scope::Book => book.descendants(book.root).iter().map(|d| d.guid).collect(),
        };
        accounts.into_iter().try_for_each(|account| {
            let before = book.clone();
            crate::ops::lots::scrub_lots(book, env, account)?;
            if *book != before {
                repairs.push(Repair::LotsRepaired { account });
            }
            Ok::<(), DomainError>(())
        })?;
    }
    Ok(repairs)
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;
    use crate::{Date, FixedEnv, Frame, Time64};

    struct Fixture {
        book: Book,
        env: FixedEnv,
        bank: Guid,
        food: Guid,
    }

    fn usd() -> CommodityRef {
        CommodityRef::currency("USD")
    }

    fn add_account(f: &mut Fixture, name: &str, ty: AccountType, commodity: CommodityRef, parent: Guid) -> Guid {
        let a = Account::new(f.env.new_guid(), name, ty, Some(commodity), Some(parent));
        let g = a.guid;
        f.book.accounts.insert(g, Arc::new(a));
        g
    }

    fn fixture() -> Fixture {
        let mut env = FixedEnv::new(Time64(1_719_835_200), 0);
        let book = Book::new_empty(&mut env);
        let mut f = Fixture { book, env, bank: Guid::NULL, food: Guid::NULL };
        let root = f.book.root;
        f.bank = add_account(&mut f, "Bank", AccountType::Bank, usd(), root);
        f.food = add_account(&mut f, "Food", AccountType::Expense, usd(), root);
        f
    }

    /// A transaction written as given, balanced or not: `(account, value cents, amount cents)`.
    fn raw(book: &mut Book, env: &mut FixedEnv, currency: CommodityRef, splits: &[(Option<Guid>, i64, i64)]) -> Guid {
        let splits = splits
            .iter()
            .map(|(account, v, a)| {
                let mut s = Split::new(env.new_guid(), Guid::NULL, Numeric::new(*v, 100), Numeric::new(*a, 100));
                s.account = *account;
                s
            })
            .collect();
        let t = Transaction {
            guid: env.new_guid(),
            currency,
            num: None,
            date_posted: Date::from_ymd(2024, 6, 1).unwrap().neutral_time(),
            date_entered: Time64(0),
            description: None,
            slots: Frame::new(),
            splits,
        };
        let g = t.guid;
        book.transactions.insert(g, Arc::new(t));
        g
    }

    fn named(book: &Book, name: &str) -> Option<Guid> {
        book.accounts.values().find(|a| a.name == name).map(|a| a.guid)
    }

    fn values(book: &Book, t: Guid) -> Vec<(Option<Guid>, Numeric)> {
        book.transaction(t).unwrap().splits.iter().map(|s| (s.account, s.value)).collect()
    }

    #[test]
    fn imbalance_goes_to_a_new_imbalance_account_under_the_root() {
        let mut f = fixture();
        let t = raw(&mut f.book, &mut f.env, usd(), &[(Some(f.bank), -500, -500)]);
        let t2 = raw(&mut f.book, &mut f.env, usd(), &[(Some(f.bank), -100, -100), (Some(f.food), 40, 40)]);
        let repairs = scrub_imbalance(&mut f.book, &mut f.env, Scope::Book).unwrap();
        let imb = named(&f.book, "Imbalance-USD").unwrap();
        let acc = f.book.account(imb).unwrap();
        let shape = (acc.account_type, acc.parent, acc.commodity_scu, acc.is_placeholder());
        assert_eq!(shape, (AccountType::Bank, Some(f.book.root), Some(100), false));
        assert_eq!(f.book.accounts.last().map(|(g, _)| *g), Some(imb), "appended last");
        let expected = [(Some(imb), Numeric::new(500, 100)), (Some(f.bank), Numeric::new(-500, 100))];
        assert_eq!(values(&f.book, t), expected, "sorted debit first");
        assert!(f.book.transaction(t2).unwrap().is_balanced());
        let created = repairs.iter().filter(|r| matches!(r, Repair::AccountCreated { .. })).count();
        let added = repairs.iter().filter(|r| matches!(r, Repair::BalanceSplitAdded { .. })).count();
        assert_eq!((created, added), (1, 2), "one account, reused");
    }

    #[test]
    fn a_zero_balancing_split_is_removed_unless_it_has_a_memo() {
        let mut f = fixture();
        let root = f.book.root;
        let imb = add_account(&mut f, "Imbalance-USD", AccountType::Bank, usd(), root);
        let t = raw(&mut f.book, &mut f.env, usd(), &[(Some(f.bank), -500, -500), (Some(f.food), 500, 500), (Some(imb), 300, 300)]);
        let repairs = scrub_imbalance(&mut f.book, &mut f.env, Scope::Transaction(t)).unwrap();
        assert_eq!(values(&f.book, t).len(), 2);
        assert!(matches!(repairs.as_slice(), [Repair::BalanceSplitRemoved { .. }]));
        let t = raw(&mut f.book, &mut f.env, usd(), &[(Some(f.bank), -500, -500), (Some(f.food), 500, 500), (Some(imb), 300, 300)]);
        Arc::make_mut(f.book.transactions.get_mut(&t).unwrap()).splits[2].memo = Some("keep".into());
        scrub_imbalance(&mut f.book, &mut f.env, Scope::Transaction(t)).unwrap();
        assert!(values(&f.book, t).contains(&(Some(imb), Numeric::new(0, 100))));
        let balanced = raw(&mut f.book, &mut f.env, usd(), &[(Some(f.bank), -5, -5), (Some(f.food), 5, 5), (Some(imb), 0, 0)]);
        assert!(scrub_imbalance(&mut f.book, &mut f.env, Scope::Transaction(balanced)).unwrap().is_empty(), "balanced is left alone");
    }

    #[test]
    fn an_imbalance_account_anywhere_in_the_tree_is_found() {
        let mut f = fixture();
        let bank = f.bank;
        let nested = add_account(&mut f, "Imbalance-USD", AccountType::Bank, usd(), bank);
        let t = raw(&mut f.book, &mut f.env, usd(), &[(Some(f.bank), -500, -500)]);
        scrub_imbalance(&mut f.book, &mut f.env, Scope::Transaction(t)).unwrap();
        assert_eq!(values(&f.book, t)[0].0, Some(nested));
        let eur = CommodityRef::currency("EUR");
        let wrong_type = add_account(&mut f, "Imbalance-EUR", AccountType::Asset, eur.clone(), bank);
        let t = raw(&mut f.book, &mut f.env, eur, &[(Some(wrong_type), -500, -500)]);
        scrub_imbalance(&mut f.book, &mut f.env, Scope::Transaction(t)).unwrap();
        assert_ne!(values(&f.book, t)[0].0, Some(wrong_type), "only BANK accounts match");
    }

    #[test]
    fn orphans_go_to_an_orphan_account_and_the_transaction_is_balanced() {
        let mut f = fixture();
        let t = raw(&mut f.book, &mut f.env, usd(), &[(Some(f.bank), -500, -500), (None, 400, 400)]);
        let other = raw(&mut f.book, &mut f.env, usd(), &[(Some(f.food), -1, -1)]);
        let repairs = scrub_orphans(&mut f.book, &mut f.env, Scope::Account(f.bank)).unwrap();
        let orphan = named(&f.book, "Orphan-USD").unwrap();
        let imb = named(&f.book, "Imbalance-USD").unwrap();
        assert!(values(&f.book, t).contains(&(Some(orphan), Numeric::new(400, 100))));
        assert!(values(&f.book, t).contains(&(Some(imb), Numeric::new(100, 100))), "the commit balances it");
        assert!(!f.book.transaction(other).unwrap().is_balanced(), "outside the scope");
        assert!(matches!(repairs[0], Repair::AccountCreated { .. }));
        assert!(matches!(repairs[1], Repair::OrphanAssigned { .. }));
    }

    #[test]
    fn a_split_whose_account_is_missing_is_an_orphan() {
        let mut f = fixture();
        let missing = Guid::from_bytes([7; 16]);
        let t = raw(&mut f.book, &mut f.env, usd(), &[(Some(f.bank), -500, -500), (Some(missing), 500, 500)]);
        scrub_orphans(&mut f.book, &mut f.env, Scope::Book).unwrap();
        assert!(values(&f.book, t).contains(&(named(&f.book, "Orphan-USD"), Numeric::new(500, 100))));
    }

    #[test]
    fn amounts_follow_values_in_the_transaction_currency() {
        let mut f = fixture();
        let t = raw(&mut f.book, &mut f.env, usd(), &[(Some(f.bank), -500, -400), (Some(f.food), 500, 500)]);
        let repairs = scrub_imbalance(&mut f.book, &mut f.env, Scope::Transaction(t)).unwrap();
        assert!(matches!(repairs.as_slice(), [Repair::AmountSetToValue { .. }]));
        assert_eq!(f.book.transaction(t).unwrap().splits[0].amount, Numeric::new(-500, 100));
        let mut tiny = Split::new(f.env.new_guid(), f.food, Numeric::new(500, 100), Numeric::new(5001, 1000));
        tiny.memo = None;
        let t2 = raw(&mut f.book, &mut f.env, usd(), &[(Some(f.bank), -500, -500)]);
        Arc::make_mut(f.book.transactions.get_mut(&t2).unwrap()).splits.push(tiny);
        assert!(scrub_imbalance(&mut f.book, &mut f.env, Scope::Transaction(t2)).unwrap().is_empty(), "equal at the SCU");
    }

    #[test]
    fn a_non_currency_transaction_gets_a_currency() {
        let mut f = fixture();
        let acme = CommodityRef::new("NASDAQ", "ACME");
        let root = f.book.root;
        f.book.commodities.insert(acme.clone(), Arc::new(crate::Commodity::new(acme.clone(), None, 10_000)));
        let stock = add_account(&mut f, "ACME", AccountType::Stock, acme.clone(), root);
        let t = raw(&mut f.book, &mut f.env, acme.clone(), &[(Some(f.bank), -1000, -1000), (Some(stock), 1000, 200)]);
        let repairs = scrub_currency(&mut f.book, &mut f.env, Scope::Book).unwrap();
        assert_eq!(f.book.transaction(t).unwrap().currency, usd());
        assert!(matches!(&repairs[0], Repair::CurrencySet { from, .. } if *from == acme));
        assert!(f.book.transaction(t).unwrap().is_balanced());
    }

    proptest! {
        /// After the book's imbalance scrub every transaction balances, and only balancing splits changed value.
        #[test]
        fn every_transaction_balances(txns in proptest::collection::vec(proptest::collection::vec(-100_000i64..100_000, 1..5), 1..6)) {
            let mut f = fixture();
            let (bank, food) = (f.bank, f.food);
            let made: Vec<(Guid, Vec<i64>)> = txns
                .iter()
                .map(|cents| {
                    let account = |i: usize| Some(if i.is_multiple_of(2) { bank } else { food });
                    let splits: Vec<_> = cents.iter().enumerate().map(|(i, c)| (account(i), *c, *c)).collect();
                    (raw(&mut f.book, &mut f.env, usd(), &splits), cents.clone())
                })
                .collect();
            scrub_imbalance(&mut f.book, &mut f.env, Scope::Book).unwrap();
            let imb = named(&f.book, IMBALANCE_USD);
            for (t, cents) in made {
                let t = f.book.transaction(t).unwrap();
                prop_assert!(t.is_balanced());
                let mut kept: Vec<i64> = t.splits.iter().filter(|s| s.account != imb || imb.is_none()).map(|s| s.value.num()).collect();
                let mut want = cents.clone();
                kept.sort_unstable();
                want.sort_unstable();
                prop_assert_eq!(kept, want);
            }
        }
    }

    const IMBALANCE_USD: &str = "Imbalance-USD";
}
