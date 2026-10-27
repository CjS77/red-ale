//! QIF records to GnuCash accounts, transactions and prices.
//!
//! Translated from `5.17:gnucash/import-export/qif-imp/qif-to-gnc.scm` ("this is where QIF transactions are
//! transformed into a Gnucash account tree"), Copyright 2000-2001 Bill Gribble <grib@billgribble.com>.
//! GPL-2.0-or-later; this translation is GPL-3.0-or-later.
//!
//! [`qif_to_gnc`] is `qif-import:qif-to-gnc` (`:257-548`): it makes the accounts the maps show, shallowest first;
//! marks the halves of transfers between the files' accounts ([`transfers`]); converts every other transaction, the
//! files with the most transactions first ([`xtn`]), committing each, so an unbalanced one gets an Imbalance split,
//! and recording its prices (`xaccTransRecordPrice` with `user:split-import`); then adds the `!Type:Prices` lists'
//! prices that are not on a day with a price already. The progress dialog, pause and cancel, and
//! `qif-import:qif-to-gnc-undo` (a cancelled import is a dropped Mutation here) are not translated.
//!
//! GnuCash walks its maps, which are hash tables, in hash order; red-ale walks the payee map, then the category map,
//! then the account map (the order GnuCash's three `hash-fold`s leave the list in), each by QIF name.

pub mod accounts;
pub mod transfers;
pub mod xtn;

use std::collections::BTreeMap;
use std::sync::Arc;

use red_ale_core::book_options::uses_split_action_for_num;
use red_ale_core::ops::commit::{account_scu, sort_splits};
use red_ale_core::ops::prices::SPLIT_IMPORT_SOURCE;
use red_ale_core::ops::prices::{Nearest, USER_PRICE_SOURCE, nearest};
use red_ale_core::ops::scrub::{Repair, Scope, scrub_imbalance};
use red_ale_core::ops::transactions::transfer::{PriceChange, record_prices};
use red_ale_core::{
    AccountType, Book, CommodityRef, DomainError, DomainResult, Env, Guid, How, Numeric, Price, ReconcileState, Round, Time64, TimeZone,
};

use super::dialog_utils::{SEP, get_account_name};
use super::guess_map::MapHash;
use super::objects::{Field, QifFile, QifPrice};
use super::string::substring_count;
use accounts::{Check, Maker, Tree};
use transfers::{mark_transfers, markable_xtns};
use xtn::{XtnContext, qif_date};

/// The three QIF maps: `qif-acct-map`, `qif-cat-map` and `qif-memo-map`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Maps {
    /// QIF account name → entry.
    pub accounts: MapHash,
    /// QIF category → entry.
    pub categories: MapHash,
    /// QIF payee or memo → entry.
    pub payees: MapHash,
}

/// What converting takes besides the files and maps.
#[derive(Debug, Clone, Copy)]
pub struct Options<'a> {
    /// `stock-map`: QIF security name → its commodity, which the Book has.
    pub stock_map: &'a BTreeMap<String, CommodityRef>,
    /// The default currency.
    pub default_currency: &'a CommodityRef,
    /// The reconcile state of a split with no `C` line.
    pub status_pref: ReconcileState,
}

/// One transaction converted.
#[derive(Debug, Clone, PartialEq)]
pub struct ConvertedXtn {
    /// Its file.
    pub file: usize,
    /// Its place in the file's transactions.
    pub xtn: usize,
    /// The transaction in the Book.
    pub transaction: Guid,
    /// What committing it did: an Imbalance split, an orphan split's account.
    pub repairs: Vec<Repair>,
    /// The prices it recorded.
    pub prices: Vec<PriceChange>,
}

/// What [`qif_to_gnc`] did.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Converted {
    /// The transactions, in the order converted.
    pub transactions: Vec<ConvertedXtn>,
    /// Transactions that got no split (a reminder, or a stock action naming no accounts), which GnuCash destroys
    /// when it commits them: `(file, transaction)`.
    pub dropped: Vec<(usize, usize)>,
    /// The `!Type:Prices` prices added.
    pub prices: Vec<Guid>,
    /// GnuCash's log messages.
    pub log: Vec<String>,
}

/// Which map an entry is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Payee,
    Category,
    Account,
}

impl Maps {
    fn map_mut(&mut self, kind: Kind) -> &mut MapHash {
        match kind {
            Kind::Payee => &mut self.payees,
            Kind::Category => &mut self.categories,
            Kind::Account => &mut self.accounts,
        }
    }

    /// `sorted-accounts-list`: the shown entries, shallowest first. `qif-to-gnc.scm:304-371`.
    fn sorted_accounts(&self) -> Vec<(Kind, String)> {
        let shown = |kind: Kind, map: &MapHash| -> Vec<(Kind, String, usize)> {
            map.iter()
                .filter(|(_, e)| e.display)
                .map(|(k, e)| (kind, k.clone(), substring_count(e.gnc_name.as_deref().unwrap_or_default(), SEP)))
                .collect()
        };
        let mut list: Vec<(Kind, String, usize)> = [
            shown(Kind::Payee, &self.payees),
            shown(Kind::Category, &self.categories),
            shown(Kind::Account, &self.accounts),
        ]
        .concat();
        list.sort_by_key(|(_, _, depth)| *depth);
        list.into_iter().map(|(kind, key, _)| (kind, key)).collect()
    }
}

/// `qif-import:qif-to-gnc`: convert the files into the Book. `files` keep the transfer marks it sets, and `maps`
/// the types of the nested accounts it made. `qif-to-gnc.scm:257-548`.
pub fn qif_to_gnc(
    book: &mut Book,
    env: &mut dyn Env,
    files: &mut [QifFile],
    maps: &mut Maps,
    opts: Options<'_>,
) -> DomainResult<Converted> {
    let old = book.clone();
    let mut tree = Tree::default();
    let markable = markable_xtns(files);
    make_accounts(book, env, &old, &mut tree, maps, opts)?;
    let mut out = Converted {
        log: mark_transfers(files, &markable),
        ..Converted::default()
    };
    let mut order: Vec<usize> = (0..files.len()).collect();
    order.sort_by_key(|f| std::cmp::Reverse(files.get(*f).map_or(0, |file| file.xtns.len())));
    let num_action = uses_split_action_for_num(book);
    order.into_iter().try_for_each(|f| {
        let count = files.get(f).map_or(0, |file| file.xtns.len());
        (0..count).try_for_each(|i| convert_xtn(book, env, &files[f].xtns[i], (f, i), &mut tree, maps, opts, num_action, &mut out))
    })?;
    add_prices(book, env, files, opts, &mut out);
    Ok(out)
}

/// Make every shown entry's account, in the commodity its security or the default currency. `qif-to-gnc.scm:373-402`.
fn make_accounts(book: &mut Book, env: &mut dyn Env, old: &Book, tree: &mut Tree, maps: &mut Maps, opts: Options<'_>) -> DomainResult<()> {
    let mut maker = Maker {
        book,
        env,
        old,
        tree,
        default_currency: opts.default_currency,
    };
    maps.sorted_accounts().into_iter().try_for_each(|(kind, key)| {
        let Some(entry) = maps.map_mut(kind).get_mut(&key) else {
            return Ok(());
        };
        let security = opts.stock_map.get(get_account_name(entry.qif_name.as_deref().unwrap_or_default()));
        let types = &entry.allowed_types;
        let equity = types.contains(&AccountType::Equity);
        let stock = types.iter().any(|t| matches!(t, AccountType::Stock | AccountType::Mutual));
        let commodity = match security {
            Some(s) if equity || stock || s.is_currency() => s,
            _ => opts.default_currency,
        };
        maker
            .find_or_make_acct(
                entry,
                Check {
                    types: false,
                    commodity,
                    commodity_matters: true,
                },
            )
            .map(|_| ())
    })
}

/// Convert one transaction unless it is a marked half or has no split, commit it and record its prices.
/// `qif-to-gnc.scm:428-459`.
#[allow(clippy::too_many_arguments)] // The loop body of `qif-to-gnc`, with its state.
fn convert_xtn(
    book: &mut Book,
    env: &mut dyn Env,
    x: &super::objects::QifXtn,
    at: (usize, usize),
    tree: &mut Tree,
    maps: &Maps,
    opts: Options<'_>,
    num_action: bool,
    out: &mut Converted,
) -> DomainResult<()> {
    if x.mark || x.splits.is_empty() {
        return Ok(());
    }
    let zone = env.zone().clone();
    let ctx = XtnContext {
        maps,
        tree,
        currency: opts.default_currency,
        status_pref: opts.status_pref,
        num_action,
        zone: &zone,
        now: env.now(),
    };
    let built = ctx.convert(book, x, &mut || env.new_guid())?;
    if built.transaction.splits.is_empty() {
        out.dropped.push(at);
        return Ok(());
    }
    let guid = built.transaction.guid;
    book.transactions.insert(guid, Arc::new(built.transaction));
    let repairs = commit(book, env, guid)?;
    let prices = record_prices(book, env, guid, SPLIT_IMPORT_SOURCE)?;
    add_balances(book, tree, guid, &built.nodes)?;
    out.transactions.push(ConvertedXtn {
        file: at.0,
        xtn: at.1,
        transaction: guid,
        repairs,
        prices,
    });
    Ok(())
}

/// `xaccTransCommitEdit`'s part that changes data: the imbalance scrub, each split's amount converted to its
/// account's SCU as the split enters the account (`xaccSplitCommitEdit` calls `xaccSplitSetAmount`,
/// `5.17:libgnucash/engine/Split.cpp:1016-1031`, which rounds half up and, unlike setting a value, lets a share count
/// round to zero), then the splits sorted.
fn commit(book: &mut Book, env: &mut dyn Env, guid: Guid) -> DomainResult<Vec<Repair>> {
    let repairs = scrub_imbalance(book, env, Scope::Transaction(guid))?;
    let amounts: Vec<Numeric> = {
        let t = book.transaction(guid).ok_or_else(|| DomainError::not_found("transaction", &guid))?;
        t.splits
            .iter()
            .map(|s| {
                s.account
                    .map_or(Ok(s.amount), |a| Ok(s.amount.to_denom(account_scu(book, a)?, Round::HalfUp)?))
            })
            .collect::<DomainResult<_>>()?
    };
    let t = Arc::make_mut(
        book.transactions
            .get_mut(&guid)
            .ok_or_else(|| DomainError::not_found("transaction", &guid))?,
    );
    t.splits.iter_mut().zip(amounts).for_each(|(s, a)| s.amount = a);
    sort_splits(t);
    Ok(repairs)
}

/// Add the committed splits' amounts to the import's balance of their accounts, which a later stock split reads.
fn add_balances(book: &Book, tree: &mut Tree, guid: Guid, nodes: &[(Guid, usize)]) -> DomainResult<()> {
    let t = book.transaction(guid).ok_or_else(|| DomainError::not_found("transaction", &guid))?;
    nodes.iter().try_for_each(|(split, node)| {
        let (Some(s), Some(n)) = (t.split(*split), tree.nodes.get_mut(*node)) else {
            return Ok(());
        };
        n.balance = n.balance.add(s.amount, 0, How::LCD_NEVER)?;
        Ok(())
    })
}

/// `make-symbol-hash`: ticker symbol → commodity, over the security map. `qif-to-gnc.scm:1371-1378`.
fn symbol_hash(stock_map: &BTreeMap<String, CommodityRef>) -> BTreeMap<&str, &CommodityRef> {
    stock_map.values().map(|c| (c.mnemonic.as_str(), c)).collect()
}

/// `qif-date-to-time64`: 10:59 local on the date. `qif-to-gnc.scm:1403-1412`.
fn qif_date_to_time64(price: &QifPrice, zone: &TimeZone) -> Option<Time64> {
    price.date.value().and_then(qif_date).and_then(|d| zone.instant(d, 10, 59, 0))
}

/// `duplicate-price?`: the nearest price at or before the time is on the same local day (GnuCash takes the start
/// of 1970 when there is none). `qif-to-gnc.scm:1385-1396`.
fn duplicate_price(book: &Book, commodity: &CommodityRef, currency: &CommodityRef, t: Time64, zone: &TimeZone) -> bool {
    let near = nearest(book, commodity, currency, t, Nearest::Before, zone).map_or(Time64(0), |p| p.time);
    zone.local_date(t) == zone.local_date(near)
}

/// The prices grouped by symbol, ignoring case, each group with the commodity its first symbol names.
/// `qif-to-gnc.scm:493-509`, `:524-540`.
fn by_symbol<'p>(prices: &[&'p QifPrice], symbols: &BTreeMap<&str, &CommodityRef>) -> Vec<(&'p QifPrice, Option<CommodityRef>)> {
    let mut current: Option<(&str, Option<CommodityRef>)> = None;
    prices
        .iter()
        .map(|p| {
            if current.as_ref().is_none_or(|(s, _)| s.to_lowercase() != p.symbol.to_lowercase()) {
                current = Some((p.symbol.as_str(), symbols.get(p.symbol.as_str()).map(|c| (*c).clone())));
            }
            (*p, current.as_ref().and_then(|(_, c)| c.clone()))
        })
        .collect()
}

/// The `!Type:Prices` prices: sorted by symbol, those on a day with a price already dropped, the rest added in the
/// default currency, source `user:price`, type `unknown`, at 10:59 local. GnuCash adds them in bulk, without the
/// same-day rules of `gnc_pricedb_add_price`. `qif-to-gnc.scm:462-542`, `:1419-1455`.
fn add_prices(book: &mut Book, env: &mut dyn Env, files: &[QifFile], opts: Options<'_>, out: &mut Converted) {
    let zone = env.zone().clone();
    let symbols = symbol_hash(opts.stock_map);
    let mut sorted: Vec<&QifPrice> = files.iter().flat_map(|f| &f.prices).collect();
    sorted.sort_by_key(|p| p.symbol.to_lowercase());
    let mut missing: Vec<&str> = Vec::new();
    let mut skipped = 0_usize;
    let pruned: Vec<&QifPrice> = by_symbol(&sorted, &symbols)
        .into_iter()
        .filter_map(|(p, commodity)| {
            let Some(commodity) = commodity else {
                if missing.last().is_none_or(|m| !m.eq_ignore_ascii_case(&p.symbol)) {
                    missing.push(&p.symbol);
                }
                return None;
            };
            let duplicate =
                qif_date_to_time64(p, &zone).is_some_and(|t| duplicate_price(book, &commodity, opts.default_currency, t, &zone));
            skipped += usize::from(duplicate);
            (!duplicate).then_some(p)
        })
        .collect();
    out.log
        .extend(missing.iter().map(|s| format!("Warning: cannot find commodity for symbol {s}.")));
    if skipped > 0 {
        out.log
            .push(format!("Warning: skipped {skipped} prices on days that already had prices."));
    }
    by_symbol(&pruned, &symbols).into_iter().for_each(|(p, commodity)| {
        let (Some(commodity), Some(time), Some(value)) = (commodity, qif_date_to_time64(p, &zone), p.share_price.value().copied()) else {
            return;
        };
        let price = Price {
            guid: env.new_guid(),
            commodity,
            currency: opts.default_currency.clone(),
            time,
            source: Some(USER_PRICE_SOURCE.to_owned()),
            price_type: Some("unknown".to_owned()),
            value,
        };
        out.prices.push(price.guid);
        book.prices.insert(price.guid, Arc::new(price));
    });
}

/// Whether a price line's fields were all parsed.
pub fn price_is_parsed(p: &QifPrice) -> bool {
    matches!((&p.date, &p.share_price), (Field::Value(_), Field::Value(_)))
}
