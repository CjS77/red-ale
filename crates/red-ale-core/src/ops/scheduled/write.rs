//! Writing a scheduled transaction's template: the template root, the template account and the template
//! transactions, as `xaccSchedXactionInit` and `xaccSchedXactionSetTemplateTrans` make them
//! (`5.17:libgnucash/engine/SchedXaction.cpp:384-406`, `:966-1029`) and the template register saves their splits
//! (`5.17:gnucash/register/ledger-core/split-register-model-save.c:687-733`).

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use super::edit::{SplitSpec, TemplateSpec};
use super::error::ScheduledError;
use super::template::template_account;
use crate::commodity::TEMPLATE_NAMESPACE;
use crate::formula::{NumberFormat, Variables, evaluate, free_variables};
use crate::ops::commit::{currency_fraction, set_date_posted, set_notes};
use crate::scheduled::SPLIT_FRAME;
use crate::{
    Account, AccountType, Book, CommodityRef, DomainError, Env, Frame, Guid, Numeric, ScheduledTransaction, SlotValue, Split, Time64,
    Transaction,
};

/// The template root's name (`sxtg_book_begin`, `5.17:libgnucash/engine/SX-book.cpp:113-125`).
pub const TEMPLATE_ROOT_NAME: &str = "Template Root";

/// The template root, made as `sxtg_book_begin` makes it if the Book has none: type ROOT, named `Template Root`, no
/// commodity. GnuCash makes one in every book; an XML book that never had a scheduled transaction has none in its
/// file, and a new SQLite book names one in its `books` row from the start.
pub fn template_root(book: &mut Book, env: &mut dyn Env) -> Guid {
    if let Some(root) = book.templates.root {
        return root;
    }
    let root = Account::new(env.new_guid(), TEMPLATE_ROOT_NAME, AccountType::Root, None, None);
    let guid = root.guid;
    book.templates.accounts.insert(guid, Arc::new(root));
    book.templates.root = Some(guid);
    guid
}

/// A new template account for scheduled transaction `sx`: named by its GUID in 32 hex digits, type BANK, commodity
/// `template:template` with that commodity's fraction 1 as its SCU, a child of the template root
/// (`SchedXaction.cpp:393-406`, `xaccAccountSetCommodity` at `5.17:libgnucash/engine/Account.cpp:2666-2701`).
pub(crate) fn new_template_account(book: &mut Book, env: &mut dyn Env, sx: Guid) -> Guid {
    let root = template_root(book, env);
    let commodity = CommodityRef::new(TEMPLATE_NAMESPACE, "template");
    let mut account = Account::new(env.new_guid(), sx.to_string(), AccountType::Bank, Some(commodity), Some(root));
    account.commodity_scu = Some(1);
    let guid = account.guid;
    book.templates.accounts.insert(guid, Arc::new(account));
    guid
}

/// The template account a scheduled transaction's template splits are in: the one Since Last Run finds by name, else
/// the one the scheduled transaction names.
pub(crate) fn account_of(book: &Book, sx: &ScheduledTransaction) -> Option<Guid> {
    template_account(book, sx)
        .map(|a| a.guid)
        .or_else(|| sx.template_account.filter(|g| book.templates.accounts.contains_key(g)))
}

/// Replace the template transactions of scheduled transaction `guid` with `specs`, in order. A spec or split naming
/// the GUID of one of its template transactions or splits keeps it, with its dates and other Slots, in its place in
/// the Book; the others are new, posted today and entered now (`SchedXaction.cpp:1007-1029`); those left out are
/// deleted.
pub(crate) fn set_templates(
    book: &mut Book,
    env: &mut dyn Env,
    guid: Guid,
    specs: Vec<TemplateSpec>,
    format: NumberFormat,
) -> Result<(), ScheduledError> {
    let account = template_account_for(book, env, guid)?;
    let old: BTreeMap<Guid, Arc<Transaction>> = book
        .templates
        .transactions
        .values()
        .filter(|t| t.splits.iter().any(|s| s.account == Some(account)))
        .map(|t| (t.guid, Arc::clone(t)))
        .collect();
    let old_splits: BTreeMap<Guid, &Split> = old.values().flat_map(|t| t.splits.iter().map(|s| (s.guid, s))).collect();
    check_kept(&specs, &old, &old_splits)?;
    let built = specs
        .into_iter()
        .map(|spec| build(book, env, account, spec, &old, &old_splits, format))
        .collect::<Result<Vec<_>, _>>()?;
    let kept: BTreeSet<Guid> = built.iter().map(|t| t.guid).collect();
    old.keys().filter(|g| !kept.contains(g)).for_each(|g| {
        book.templates.transactions.shift_remove(g);
    });
    built.into_iter().for_each(|t| {
        book.templates.transactions.insert(t.guid, Arc::new(t));
    });
    Ok(())
}

/// The scheduled transaction's template account, made if it has none.
fn template_account_for(book: &mut Book, env: &mut dyn Env, guid: Guid) -> Result<Guid, ScheduledError> {
    let sx = book
        .scheduled
        .get(&guid)
        .ok_or_else(|| DomainError::not_found("scheduledTransaction", &guid))?;
    if let Some(account) = account_of(book, sx) {
        return Ok(account);
    }
    let account = new_template_account(book, env, guid);
    if let Some(sx) = book.scheduled.get_mut(&guid) {
        Arc::make_mut(sx).template_account = Some(account);
    }
    Ok(account)
}

/// Each GUID a spec keeps is one of this scheduled transaction's, given once.
fn check_kept(
    specs: &[TemplateSpec],
    old: &BTreeMap<Guid, Arc<Transaction>>,
    old_splits: &BTreeMap<Guid, &Split>,
) -> Result<(), ScheduledError> {
    let mut seen = BTreeSet::new();
    let transactions = specs.iter().filter_map(|t| t.guid).map(|g| (g, old.contains_key(&g)));
    let splits = specs
        .iter()
        .flat_map(|t| &t.splits)
        .filter_map(|s| s.guid)
        .map(|g| (g, old_splits.contains_key(&g)));
    transactions.chain(splits).try_for_each(|(g, known)| {
        if !known {
            return Err(ScheduledError::invalid(
                "template-guid",
                format!("{g} is not a template transaction or split of this scheduled transaction"),
            ));
        }
        if !seen.insert(g) {
            return Err(ScheduledError::invalid("template-guid", format!("{g} is given twice")));
        }
        Ok(())
    })
}

fn build(
    book: &Book,
    env: &mut dyn Env,
    account: Guid,
    spec: TemplateSpec,
    old: &BTreeMap<Guid, Arc<Transaction>>,
    old_splits: &BTreeMap<Guid, &Split>,
    format: NumberFormat,
) -> Result<Transaction, ScheduledError> {
    let currency = currency(book, &spec)?;
    let fraction = currency_fraction(book, &currency)?;
    let mut t = match spec.guid.and_then(|g| old.get(&g)) {
        Some(kept) => kept.as_ref().clone(),
        None => new_transaction(env, currency.clone())?,
    };
    t.currency = currency;
    t.num = spec.num.filter(|n| !n.is_empty());
    t.description = Some(spec.description.unwrap_or_default());
    set_notes(&mut t, spec.notes.as_deref());
    t.splits = spec
        .splits
        .into_iter()
        .map(|s| {
            let kept = s.guid.and_then(|g| old_splits.get(&g)).map(|s| (*s).clone());
            build_split(env, account, fraction, s, kept, format)
        })
        .collect();
    Ok(t)
}

fn new_transaction(env: &mut dyn Env, currency: CommodityRef) -> Result<Transaction, ScheduledError> {
    let today = env
        .today()
        .ok_or_else(|| DomainError::invalid("the clock is outside the calendar"))?;
    let mut t = Transaction {
        guid: env.new_guid(),
        currency,
        num: None,
        date_posted: Time64(0),
        date_entered: env.now(),
        description: None,
        slots: Frame::new(),
        splits: Vec::new(),
    };
    set_date_posted(&mut t, today, env.zone());
    Ok(t)
}

/// The spec's currency, else its first split's account commodity when that is a currency, as `transaction.post`
/// defaults it.
fn currency(book: &Book, spec: &TemplateSpec) -> Result<CommodityRef, ScheduledError> {
    let first = || {
        spec.splits
            .first()
            .and_then(|s| book.account(s.account))
            .and_then(|a| a.commodity.clone())
    };
    let currency = spec
        .currency
        .clone()
        .or_else(|| first().filter(CommodityRef::is_currency))
        .ok_or_else(|| ScheduledError::invalid("currency", "give the template transaction's currency"))?;
    if currency.is_currency() {
        Ok(currency)
    } else {
        Err(ScheduledError::invalid("currency", format!("{currency} is not a currency")))
    }
}

/// A template split in the template account: value 0 in the currency's fraction and amount 0, as the register leaves
/// them (`split-register-model-save.c:731-733`), the memo and action, and the `sched-xaction` Frame. A kept split
/// keeps its other fields and the Frame's other keys, such as the register's `shares`.
fn build_split(env: &mut dyn Env, account: Guid, fraction: i64, spec: SplitSpec, kept: Option<Split>, format: NumberFormat) -> Split {
    let mut split = kept.unwrap_or_else(|| Split::new(env.new_guid(), account, Numeric::new(0, fraction), Numeric::new(0, 1)));
    split.account = Some(account);
    split.memo = spec.memo.filter(|m| !m.is_empty());
    split.action = spec.action.filter(|a| !a.is_empty());
    let mut frame = split
        .slots
        .get_key(SPLIT_FRAME)
        .and_then(SlotValue::as_frame)
        .cloned()
        .unwrap_or_default();
    frame.insert_key("account", SlotValue::Guid(spec.account));
    set_formula(&mut frame, "credit", spec.credit_formula.unwrap_or_default(), format);
    set_formula(&mut frame, "debit", spec.debit_formula.unwrap_or_default(), format);
    split.slots.insert_key(SPLIT_FRAME, SlotValue::Frame(frame));
    split
}

/// Write a formula and its numeric, unless the Frame already holds that formula with a numeric: a numeric read from a
/// file is kept as read, even a stale one.
fn set_formula(frame: &mut Frame, side: &str, text: String, format: NumberFormat) {
    let (formula, numeric) = (format!("{side}-formula"), format!("{side}-numeric"));
    let unchanged = frame.get_key(&formula).and_then(SlotValue::as_str) == Some(text.as_str()) && frame.get_key(&numeric).is_some();
    if !unchanged {
        frame.insert_key(numeric, SlotValue::Numeric(numeric_of(&text, format)));
        frame.insert_key(formula, SlotValue::String(text));
    }
}

/// The numeric the template register stores beside a formula: its value when it parses and names no variable, else
/// 0 (`save_cell`, `split-register-model-save.c:687-713`). GnuCash's open-time scrub sets the same
/// (`gnc_sx_scrub_split_numerics`, `5.17:libgnucash/app-utils/gnc-sx-instance-model.c:113-174`).
pub fn numeric_of(text: &str, format: NumberFormat) -> Numeric {
    let zero = Numeric::new(0, 1);
    match free_variables(text, format) {
        Ok(names) if names.is_empty() => evaluate(text, format, &Variables::new()).unwrap_or(zero),
        _ => zero,
    }
}
