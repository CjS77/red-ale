//! Reading a scheduled transaction's template: its template account and transactions, each split's real account,
//! the transaction currency and the Variables (`5.17:libgnucash/app-utils/gnc-sx-instance-model.c:200-395`,
//! `:1060-1085`, `:1245-1308`).

use std::collections::BTreeSet;
use std::sync::Arc;

use super::error::{AccountProblem, ScheduledError, Side, Site};
use crate::book_options::uses_split_action_for_num;
use crate::formula::{NumberFormat, free_variables};
use crate::ops::transactions::order::split_order;
use crate::{Account, Book, CommodityRef, ScheduledTransaction, Split, TemplateSplit, Transaction};

/// The name of the predefined variable holding the instance's Instance count.
pub const INSTANCE_COUNT_VARIABLE: &str = "i";

/// The template account, found as GnuCash finds it: by its name, the scheduled transaction's GUID, under the template
/// root (`gnc_sx_get_template_transaction_account`, `gnc-sx-instance-model.c:378-388`; `gnc_account_lookup_by_name`
/// looks at the root's children before their descendants). `None` when there is none, and then the scheduled
/// transaction has no template transactions.
pub fn template_account<'a>(book: &'a Book, sx: &ScheduledTransaction) -> Option<&'a Account> {
    let root = book.templates.root?;
    let name = sx.guid.to_string();
    let named = |a: &&Arc<Account>| a.name == name && a.guid != root;
    let accounts = || book.templates.accounts.values().filter(named);
    accounts()
        .find(|a| a.parent == Some(root))
        .or_else(|| accounts().next())
        .map(AsRef::as_ref)
}

/// The template transactions, in the template account's split order, each once at its first split
/// (`xaccAccountForEachTransaction`, `5.17:libgnucash/engine/Account.cpp:5050-5074`, `:5138-5144`, over the account's
/// splits sorted by `xaccSplitOrder`).
pub fn template_transactions<'a>(book: &'a Book, sx: &ScheduledTransaction) -> Vec<&'a Transaction> {
    let Some(account) = template_account(book, sx).map(|a| a.guid) else {
        return Vec::new();
    };
    let num_action = uses_split_action_for_num(book);
    let mut splits: Vec<(&Transaction, &Split)> = book
        .templates
        .transactions
        .values()
        .flat_map(|t| t.splits.iter().filter(move |s| s.account == Some(account)).map(move |s| (t.as_ref(), s)))
        .collect();
    splits.sort_by(|(ta, sa), (tb, sb)| split_order(ta, sa, tb, sb, num_action));
    let mut seen = BTreeSet::new();
    splits.into_iter().filter(|(t, _)| seen.insert(t.guid)).map(|(t, _)| t).collect()
}

/// A template split with neither formula: it is still created, with value 0 (`split_is_marker`,
/// `gnc-sx-instance-model.c:200-219`).
pub fn is_marker(split: &Split) -> bool {
    let present = |f: Option<&str>| f.is_some_and(|f| !f.is_empty());
    TemplateSplit::of(split).is_none_or(|t| !present(t.debit_formula()) && !present(t.credit_formula()))
}

/// Where a template split is.
pub(crate) fn site(sx: &ScheduledTransaction, t: &Transaction, split: &Split) -> Site {
    Site {
        scheduled: sx.guid,
        template_transaction: t.guid,
        split: split.guid,
    }
}

/// The real account a template split's `sched-xaction` Frame names (`_get_template_split_account`,
/// `gnc-sx-instance-model.c:1060-1085`), with its commodity.
pub(crate) fn split_account<'a>(
    book: &'a Book,
    sx: &ScheduledTransaction,
    t: &Transaction,
    split: &Split,
) -> Result<(&'a Account, &'a CommodityRef), ScheduledError> {
    let named = TemplateSplit::of(split).and_then(|f| f.account());
    let problem = |problem| ScheduledError::Account {
        site: site(sx, t, split),
        problem,
        index: None,
    };
    let account = named.and_then(|g| book.account(g)).ok_or_else(|| problem(AccountProblem::Missing(named)))?;
    let commodity = account.commodity.as_ref().ok_or_else(|| problem(AccountProblem::NoCommodity(account.guid)))?;
    Ok((account, commodity))
}

/// The currency a template transaction's instances are created in (`get_transaction_currency`,
/// `gnc-sx-instance-model.c:1245-1308`): the template's currency `T` unless the first currency among the non-marker
/// splits' account commodities is to be preferred (when `T` is in none of them, or is not a currency), or else the
/// first of those commodities (when `T` is in none of them). An unknown split account aborts it.
pub fn transaction_currency(book: &Book, sx: &ScheduledTransaction, t: &Transaction) -> Result<CommodityRef, ScheduledError> {
    let commodities = t
        .splits
        .iter()
        .map(|s| split_account(book, sx, t, s).map(|(_, c)| (s, c)))
        .collect::<Result<Vec<_>, _>>()?;
    let counted: Vec<&CommodityRef> = commodities.iter().filter(|(s, _)| !is_marker(s)).map(|(_, c)| *c).collect();
    let template = &t.currency;
    let in_splits = counted.contains(&template);
    let first_currency = counted.iter().find(|c| c.is_currency());
    Ok(match (first_currency, counted.first()) {
        (Some(c), _) if !in_splits || !template.is_currency() => (*c).clone(),
        (_, Some(c)) if !in_splits => (*c).clone(),
        _ => template.clone(),
    })
}

/// The exchange-rate variable for a split in `split` commodity of a transaction in `currency`: `EUR -> USD`
/// (`var_name_from_commodities`, `gnc-sx-instance-model.c:300-311`).
pub fn exchange_rate_variable(split: &CommodityRef, currency: &CommodityRef) -> String {
    format!("{} -> {}", split.mnemonic, currency.mnemonic)
}

/// Every Variable a scheduled transaction's instances need, sorted bytewise: `i`, every name its template formulas
/// use, and an exchange-rate variable for each non-marker split in another commodity than its transaction's currency
/// (`gnc_sx_get_variables`, `gnc-sx-instance-model.c:313-395`). With them, the problems found reading the template:
/// formulas that don't parse and splits whose account is unknown.
pub fn variables(book: &Book, sx: &ScheduledTransaction, format: NumberFormat) -> (BTreeSet<String>, Vec<ScheduledError>) {
    let mut names = BTreeSet::from([INSTANCE_COUNT_VARIABLE.to_owned()]);
    let mut errors = Vec::new();
    template_transactions(book, sx).into_iter().for_each(|t| {
        let currency = transaction_currency(book, sx, t);
        t.splits.iter().for_each(|split| {
            formulas(split).into_iter().for_each(|(side, text)| match free_variables(text, format) {
                Ok(found) => names.extend(found),
                Err(error) => errors.push(ScheduledError::formula(site(sx, t, split), side, text, error)),
            });
            match (split_account(book, sx, t, split), &currency) {
                (Ok((_, c)), Ok(currency)) if c != currency && !is_marker(split) => {
                    names.insert(exchange_rate_variable(c, currency));
                }
                (Err(e), _) => errors.push(e),
                _ => {}
            }
        });
    });
    (names, errors)
}

/// A template split's non-empty formulas, credit first as GnuCash reads them.
pub(crate) fn formulas(split: &Split) -> Vec<(Side, &str)> {
    TemplateSplit::of(split).map_or_else(Vec::new, |t| {
        [(Side::Credit, t.credit_formula()), (Side::Debit, t.debit_formula())]
            .into_iter()
            .filter_map(|(side, f)| f.filter(|f| !f.is_empty()).map(|f| (side, f)))
            .collect()
    })
}
