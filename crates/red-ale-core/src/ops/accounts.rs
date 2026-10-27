//! Domain operations: the account tree.
//!
//! Creating, editing, moving, renumbering and deleting accounts as GnuCash 5.17's account dialogs do. Balances are in
//! [`balances`](crate::ops::balances). The GnuCash engine accepts almost anything; its rules live in the dialogs,
//! so this module follows `gnucash/gnome-utils/dialog-account.c` and `gnucash/gnome/gnc-plugin-page-account-tree.cpp`.
//! See `design/notes/engine-accounts-transactions.md` §2 and §3.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use crate::account::slot_paths;
use crate::ops::account_properties::apply_properties;
use crate::ops::commit::{commodity_fraction, round_amount, sort_splits};
use crate::ops::commodities::account_commodity_changed;
use crate::transaction::slot_paths as txn_slots;
use crate::{Account, AccountType, Book, CommodityRef, DomainError, DomainResult, Env, Guid, Numeric, SlotValue, Transaction};

/// The account separator. GnuCash makes it a user preference (default `:`,
/// `5.17:gnucash/gschemas/org.gnucash.GnuCash.gschema.xml.in:18-22`); red-ale always uses `:`.
pub const SEPARATOR: char = ':';

/// The account types GnuCash 5.17 offers, in enum order (`NUM_ACCOUNT_TYPES` = 15, `5.17:libgnucash/engine/Account.h`).
pub const ACCOUNT_TYPES: [AccountType; 15] = [
    AccountType::Bank,
    AccountType::Cash,
    AccountType::Asset,
    AccountType::Credit,
    AccountType::Liability,
    AccountType::Stock,
    AccountType::Mutual,
    AccountType::Currency,
    AccountType::Income,
    AccountType::Expense,
    AccountType::Equity,
    AccountType::Receivable,
    AccountType::Payable,
    AccountType::Root,
    AccountType::Trading,
];

/// The label GnuCash shows for a type, untranslated (`account_type_name`, `5.17:libgnucash/engine/Account.cpp:4319`).
pub fn type_label(t: AccountType) -> &'static str {
    use AccountType as T;
    match t {
        T::Bank => "Bank",
        T::Cash => "Cash",
        T::Asset => "Asset",
        T::Credit => "Credit Card",
        T::Liability => "Liability",
        T::Stock => "Stock",
        T::Mutual => "Mutual Fund",
        T::Currency => "Currency",
        T::Income => "Income",
        T::Expense => "Expense",
        T::Equity => "Equity",
        T::Receivable => "A/Receivable",
        T::Payable => "A/Payable",
        T::Root => "Root",
        T::Trading => "Trading",
        T::Checking => "Checking",
        T::Savings => "Savings",
        T::MoneyMarket => "Money Market",
        T::CreditLine => "Credit Line",
        T::None => "",
    }
}

/// The fundamental type: ASSET, LIABILITY, INCOME, EXPENSE or EQUITY (`xaccAccountTypeGetFundamental`,
/// `5.17:libgnucash/engine/Account.cpp:4483`).
pub fn fundamental_type(t: AccountType) -> Option<AccountType> {
    use AccountType as T;
    match t {
        T::Bank | T::Stock | T::MoneyMarket | T::Checking | T::Savings | T::Mutual | T::Currency | T::Cash | T::Asset | T::Receivable => {
            Some(T::Asset)
        }
        T::Credit | T::Liability | T::Payable | T::CreditLine => Some(T::Liability),
        T::Income => Some(T::Income),
        T::Expense => Some(T::Expense),
        T::Equity => Some(T::Equity),
        T::Trading | T::Root | T::None => None,
    }
}

/// Types a user may choose: every type but the deprecated CURRENCY and ROOT (`xaccAccountTypesValid`,
/// `5.17:libgnucash/engine/Account.cpp:4460`).
pub fn is_choosable(t: AccountType) -> bool {
    ACCOUNT_TYPES.contains(&t) && !matches!(t, AccountType::Currency | AccountType::Root)
}

/// Whether an account that has splits may change from `old` to `new` (`xaccAccountTypesCompatibleWith`,
/// `5.17:libgnucash/engine/Account.cpp:4355`; used by `5.17:gnucash/gnome-utils/dialog-account.c:1714-1728`). A
/// legacy type with no group may change to any choosable type, as the dialog falls back to `xaccAccountTypesValid`.
pub fn type_change_allowed_with_splits(old: AccountType, new: AccountType) -> bool {
    use AccountType as T;
    let group = |t: AccountType| match t {
        T::Bank | T::Cash | T::Asset | T::Credit | T::Liability | T::Income | T::Expense | T::Equity => 1,
        T::Stock | T::Mutual | T::Currency => 2,
        T::Receivable => 3,
        T::Payable => 4,
        T::Trading => 5,
        _ => 0,
    };
    match group(old) {
        0 => is_choosable(new),
        g => g == group(new),
    }
}

/// What kind of commodity an account type holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommodityRule {
    /// A currency.
    Currency,
    /// A security (any commodity outside the `CURRENCY` namespace).
    Security,
    /// Anything.
    Any,
}

/// The commodity an account type takes: STOCK and MUTUAL a security, TRADING anything, the rest a currency
/// (`gnc_account_commodity_from_type`, `5.17:gnucash/gnome-utils/dialog-account.c:224-241`).
pub fn commodity_rule(t: AccountType) -> CommodityRule {
    match t {
        AccountType::Stock | AccountType::Mutual => CommodityRule::Security,
        AccountType::Trading => CommodityRule::Any,
        _ => CommodityRule::Currency,
    }
}

fn check_commodity(book: &Book, t: AccountType, c: &CommodityRef) -> DomainResult<i64> {
    if c.namespace == crate::commodity::TEMPLATE_NAMESPACE {
        return Err(DomainError::invalid("the template commodity cannot be held by an account"));
    }
    let fraction = commodity_fraction(book, c).ok_or_else(|| DomainError::not_found("commodity", c))?;
    match (commodity_rule(t), c.is_currency()) {
        (CommodityRule::Currency, false) => Err(DomainError::invalid(format!("a {t} account holds a currency, not {c}"))),
        (CommodityRule::Security, true) => Err(DomainError::invalid(format!(
            "a {t} account holds a security, not the currency {c}"
        ))),
        _ => Ok(fraction),
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Names and order

/// Check a new account name: not empty (`gnc_common_ok`, `5.17:gnucash/gnome-utils/dialog-account.c:954-961`) and
/// free of the separator, which GnuCash's name entry strips as it is typed (`dialog-account.c:1420-1447`).
pub fn check_name(name: &str) -> DomainResult<()> {
    if name.is_empty() {
        return Err(DomainError::invalid("an account must have a name"));
    }
    if name.contains(SEPARATOR) {
        return Err(DomainError::invalid(format!(
            "account names may not contain the separator '{SEPARATOR}': {name:?}"
        )));
    }
    Ok(())
}

/// Accounts whose names contain the separator, which make full names ambiguous. GnuCash warns about them when a
/// book is opened (`gnc_account_list_name_violations`, `5.17:libgnucash/engine/Account.cpp:262-281`).
pub fn name_violations(book: &Book) -> Vec<Guid> {
    book.descendants(book.root)
        .into_iter()
        .filter(|a| a.name.contains(SEPARATOR))
        .map(|a| a.guid)
        .collect()
}

/// Refuse a name another child of `parent` already has: GnuCash refuses an account whose full name exists
/// (`dialog-account.c:963-986`).
fn check_no_sibling(book: &Book, parent: Guid, name: &str, except: Option<Guid>) -> DomainResult<()> {
    match book.children(parent).find(|a| a.name == name && Some(a.guid) != except) {
        Some(clash) => {
            let full = book.full_name(clash.guid).unwrap_or_default();
            Err(DomainError::conflict(format!("there is already an account named {full:?}")))
        }
        None => Ok(()),
    }
}

/// Rank of a type in the display order, as GnuCash 5.17 actually computes it. `typeorder` lists BANK first, but it
/// has 15 slots and 14 initialisers, so its last slot is 0 (BANK) and the loop that fills `revorder` ranks BANK 14,
/// after TRADING; ROOT keeps rank 0 (`5.17:libgnucash/engine/Account.cpp:2348-2400`). The oracle confirms it.
fn type_rank(t: AccountType) -> usize {
    use AccountType as T;
    const ORDER: [AccountType; 15] = [
        T::Root,
        T::Stock,
        T::Mutual,
        T::Currency,
        T::Cash,
        T::Asset,
        T::Receivable,
        T::Credit,
        T::Liability,
        T::Payable,
        T::Income,
        T::Expense,
        T::Equity,
        T::Trading,
        T::Bank,
    ];
    ORDER.iter().position(|o| *o == t).unwrap_or(ORDER.len())
}

/// GnuCash's display order of sibling accounts (`xaccAccountOrder`, `5.17:libgnucash/engine/Account.cpp:2363`): code
/// bytewise, then type rank, then name, then GUID. Names compare bytewise where GnuCash uses the locale's collation.
pub fn account_order(a: &Account, b: &Account) -> Ordering {
    let code = |x: &Account| x.code.clone().unwrap_or_default();
    code(a)
        .cmp(&code(b))
        .then_with(|| type_rank(a.account_type).cmp(&type_rank(b.account_type)))
        .then_with(|| a.name.cmp(&b.name))
        .then_with(|| a.guid.cmp(&b.guid))
}

/// The children of an account in display order (`gnc_account_get_children_sorted`).
pub fn sorted_children(book: &Book, parent: Guid) -> Vec<&Account> {
    let mut children: Vec<&Account> = book.children(parent).collect();
    children.sort_by(|a, b| account_order(a, b));
    children
}

/// Every descendant of an account in display order, depth first: each child, in display order among its siblings,
/// followed by its own descendants (`gnc_account_get_descendants_sorted`, `5.17:libgnucash/engine/Account.cpp:2807-2822`,
/// `:3039-3045`).
pub fn descendants_sorted(book: &Book, parent: Guid) -> Vec<&Account> {
    sorted_children(book, parent)
        .into_iter()
        .flat_map(|child| std::iter::once(child).chain(descendants_sorted(book, child.guid)))
        .collect()
}

/// How many levels an account is below the root account: 0 for the root, 1 for its children
/// (`gnc_account_get_current_depth`, `5.17:libgnucash/engine/Account.cpp:2997-3013`).
pub fn depth(book: &Book, guid: Guid) -> usize {
    book.ancestors(guid).len() + usize::from(guid != book.root)
}

/// The number of levels in an account's subtree, counting the account itself: 1 for an account without children
/// (`gnc_account_get_tree_depth`, `5.17:libgnucash/engine/Account.cpp:3016-3029`).
pub fn tree_depth(book: &Book, guid: Guid) -> usize {
    1 + book.children(guid).map(|child| tree_depth(book, child.guid)).max().unwrap_or(0)
}

/// True when the account or one of its ancestors is hidden (`xaccAccountIsHidden`,
/// `5.17:libgnucash/engine/Account.cpp:4190`).
pub fn is_hidden_in_tree(book: &Book, guid: Guid) -> bool {
    book.account(guid).is_some_and(Account::is_hidden) || book.ancestors(guid).iter().any(|a| a.is_hidden())
}

/// True when the account has splits.
pub fn has_splits(book: &Book, guid: Guid) -> bool {
    book.splits_of(guid).next().is_some()
}

fn subtree(book: &Book, guid: Guid) -> Vec<Guid> {
    std::iter::once(guid)
        .chain(book.descendants(guid).into_iter().map(|a| a.guid))
        .collect()
}

/// Put an account and its descendants just after the last account of its parent's subtree, keeping their relative
/// order. GnuCash appends a moved or new account to its parent's children (`gnc_account_append_child`,
/// `5.17:libgnucash/engine/Account.cpp:2824`) and writes the tree in pre-order
/// (`5.17:libgnucash/backend/xml/io-utils.cpp:59-82`); its reader refuses an account whose parent comes later
/// (`5.17:libgnucash/backend/xml/gnc-account-xml-v2.cpp:349-365`).
fn place_subtree(book: &mut Book, guid: Guid) {
    let Some(parent) = book.account(guid).and_then(|a| a.parent) else {
        return;
    };
    let moving: BTreeSet<Guid> = subtree(book, guid).into_iter().collect();
    let entries: Vec<(Guid, Arc<Account>)> = book
        .accounts
        .iter()
        .filter(|(g, _)| moving.contains(*g))
        .map(|(g, a)| (*g, Arc::clone(a)))
        .collect();
    book.accounts.retain(|g, _| !moving.contains(g));
    let host: BTreeSet<Guid> = subtree(book, parent).into_iter().collect();
    let at = book
        .accounts
        .keys()
        .rposition(|g| host.contains(g))
        .map_or(book.accounts.len(), |i| i + 1);
    entries.into_iter().enumerate().for_each(|(i, (g, a))| {
        book.accounts.shift_insert(at + i, g, a);
    });
}

fn account(book: &Book, guid: Guid) -> DomainResult<&Account> {
    book.account(guid).ok_or_else(|| DomainError::not_found("account", &guid))
}

fn account_mut(book: &mut Book, guid: Guid) -> DomainResult<&mut Account> {
    book.accounts
        .get_mut(&guid)
        .map(Arc::make_mut)
        .ok_or_else(|| DomainError::not_found("account", &guid))
}

fn not_root(book: &Book, guid: Guid, what: &str) -> DomainResult<()> {
    if guid == book.root {
        Err(DomainError::invalid(format!("the root account cannot be {what}")))
    } else {
        Ok(())
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Create and edit

/// An account's smallest fraction, as the account dialog offers it (`dialog-account.c:525-545`; choices at
/// `5.17:gnucash/gtkbuilder/dialog-account.glade:1066-1105`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scu {
    /// "Use Commodity Value": the commodity's fraction.
    Commodity,
    /// A non-standard fraction: a power of ten from 1 to 10^9.
    Fraction(i64),
}

/// Changes to an account. `None` leaves a field as it is; an empty string clears a text field.
#[derive(Debug, Clone, PartialEq, Default)]
#[allow(clippy::option_option)] // Absent, cleared and set limits are three different requests.
pub struct AccountEdit {
    /// Name.
    pub name: Option<String>,
    /// Account type.
    pub account_type: Option<AccountType>,
    /// Give the descendants the new type when the old one cannot sit under it (GnuCash asks first).
    pub retype_children: bool,
    /// Commodity; only while the account has no splits.
    pub commodity: Option<CommodityRef>,
    /// Smallest fraction.
    pub scu: Option<Scu>,
    /// Account code.
    pub code: Option<String>,
    /// Description.
    pub description: Option<String>,
    /// Notes.
    pub notes: Option<String>,
    /// Placeholder flag.
    pub placeholder: Option<bool>,
    /// Hidden flag.
    pub hidden: Option<bool>,
    /// Tax-related flag.
    pub tax_related: Option<bool>,
    /// US TXF code (`tax-US/code`).
    pub tax_us_code: Option<String>,
    /// US TXF payer name source (`tax-US/payer-name-source`): GnuCash offers `current` and `parent`.
    pub tax_us_payer_name_source: Option<String>,
    /// US TXF copy number (`tax-US/copy-number`); 0 removes it.
    pub tax_us_copy_number: Option<i64>,
    /// Account colour (see `account_properties::parse_color`); empty removes it.
    pub color: Option<String>,
    /// Higher balance limit; `Some(None)` clears it.
    pub higher_balance_limit: Option<Option<Numeric>>,
    /// Lower balance limit; `Some(None)` clears it.
    pub lower_balance_limit: Option<Option<Numeric>>,
    /// Balance limits apply with sub-accounts.
    pub include_subaccount_balances: Option<bool>,
    /// Holds the opening balances of its commodity (EQUITY only).
    pub opening_balance: Option<bool>,
    /// Associated accounts by tag; `None` removes the tag. Tags not named are kept.
    pub associated_accounts: BTreeMap<String, Option<Guid>>,
}

/// Create an account under `parent` with `edit.name`, `edit.account_type` and `edit.commodity` (by default the
/// parent's, when it suits the type), then apply the other fields. It is placed last among its parent's children.
/// Checks follow `gnc_common_ok` (`5.17:gnucash/gnome-utils/dialog-account.c:938`).
pub fn create_account(book: &mut Book, env: &mut dyn Env, parent: Guid, edit: &AccountEdit) -> DomainResult<Guid> {
    let parent_account = account(book, parent)?;
    let name = edit
        .name
        .as_deref()
        .ok_or_else(|| DomainError::invalid("an account must have a name"))?;
    let account_type = edit
        .account_type
        .ok_or_else(|| DomainError::invalid("an account must have a type"))?;
    check_name(name)?;
    check_type(account_type, parent_account.account_type)?;
    check_no_sibling(book, parent, name, None)?;
    let commodity = match (&edit.commodity, &parent_account.commodity) {
        (Some(c), _) => c.clone(),
        (None, Some(c)) if check_commodity(book, account_type, c).is_ok() => c.clone(),
        _ => return Err(DomainError::invalid(format!("a {account_type} account needs a commodity"))),
    };
    let fraction = check_commodity(book, account_type, &commodity)?;
    let mut new = Account::new(env.new_guid(), name, account_type, Some(commodity.clone()), Some(parent));
    new.commodity_scu = Some(fraction);
    let guid = new.guid;
    book.accounts.insert(guid, Arc::new(new));
    place_subtree(book, guid);
    account_commodity_changed(book, None, Some(&commodity));
    if let Some(scu) = edit.scu {
        apply_scu(book, guid, scu, false)?;
    }
    apply_details(book, guid, edit)?;
    apply_properties(book, guid, edit, None)?;
    Ok(guid)
}

fn check_type(t: AccountType, parent_type: AccountType) -> DomainResult<()> {
    if !is_choosable(t) {
        return Err(DomainError::invalid(format!("{t} is not an account type that can be chosen")));
    }
    if !t.compatible_with_parent(parent_type) {
        return Err(DomainError::invalid(format!(
            "a {t} account cannot be placed under a {parent_type} account"
        )));
    }
    Ok(())
}

/// Edit an account as the account dialog's OK does (`gnc_ui_to_account`, `5.17:gnucash/gnome-utils/dialog-account.c:458`).
pub fn update_account(book: &mut Book, guid: Guid, edit: &AccountEdit) -> DomainResult<()> {
    not_root(book, guid, "edited")?;
    let current = account(book, guid)?.clone();
    let parent = current.parent.unwrap_or(book.root);
    if let Some(name) = edit.name.as_deref().filter(|n| *n != current.name) {
        check_name(name)?;
        check_no_sibling(book, parent, name, Some(guid))?;
        name.clone_into(&mut account_mut(book, guid)?.name);
    }
    if let Some(t) = edit.account_type.filter(|t| *t != current.account_type) {
        change_type(book, guid, t, edit.retype_children)?;
    }
    let commodity_changed = match edit.commodity.as_ref().filter(|c| current.commodity.as_ref() != Some(*c)) {
        Some(c) => {
            change_commodity(book, guid, c)?;
            true
        }
        None => false,
    };
    let after = account(book, guid)?;
    if let Some(c) = after.commodity.clone().filter(|_| commodity_changed || edit.account_type.is_some()) {
        check_commodity(book, after.account_type, &c)?;
    }
    let current_choice = if current.non_standard_scu {
        current.commodity_scu.map(Scu::Fraction)
    } else {
        Some(Scu::Commodity)
    };
    if let Some(scu) = edit.scu.or(if commodity_changed { current_choice } else { None }) {
        apply_scu(book, guid, scu, commodity_changed)?;
    }
    apply_details(book, guid, edit)?;
    apply_properties(book, guid, edit, Some(&current))
}

/// Change an account's type. With splits only a compatible type is allowed (`dialog-account.c:1714-1728`). When the
/// old type cannot sit under the new one, the children are given the new type too, recursively, after the caller
/// confirms (`verify_children_compatible`, `set_children_types`, `dialog-account.c:684-726`, `:824-899`). red-ale also
/// refuses to retype a descendant whose splits would forbid the change.
fn change_type(book: &mut Book, guid: Guid, new: AccountType, retype_children: bool) -> DomainResult<()> {
    let acc = account(book, guid)?;
    let old = acc.account_type;
    let parent_type = acc
        .parent
        .and_then(|p| book.account(p))
        .map_or(AccountType::Root, |p| p.account_type);
    check_type(new, parent_type)?;
    if has_splits(book, guid) && !type_change_allowed_with_splits(old, new) {
        return Err(DomainError::conflict(format!(
            "{} has splits, so its type cannot change from {old} to {new}",
            acc.name
        )));
    }
    let retyped = if old.compatible_with_parent(new) {
        Vec::new()
    } else {
        children_to_retype(book, guid, new)
    };
    if !retyped.is_empty() && !retype_children {
        return Err(DomainError::conflict(format!(
            "the sub-accounts of {} must become {new} too; pass retypeChildren to confirm",
            acc.name
        )));
    }
    if let Some(blocked) = retyped
        .iter()
        .filter_map(|g| book.account(*g))
        .find(|a| has_splits(book, a.guid) && !type_change_allowed_with_splits(a.account_type, new))
    {
        return Err(DomainError::conflict(format!(
            "sub-account {} has splits, so it cannot become {new}",
            blocked.name
        )));
    }
    std::iter::once(guid)
        .chain(retyped)
        .try_for_each(|g| account_mut(book, g).map(|a| a.account_type = new))
}

/// `set_children_types`: children not already of the new type, and their children, recursively.
fn children_to_retype(book: &Book, guid: Guid, new: AccountType) -> Vec<Guid> {
    book.children(guid)
        .filter(|c| c.account_type != new)
        .flat_map(|c| std::iter::once(c.guid).chain(children_to_retype(book, c.guid, new)))
        .collect()
}

/// `xaccAccountSetCommodity` (`5.17:libgnucash/engine/Account.cpp:2666`): the SCU returns to the commodity's
/// fraction. The dialog forbids it once the account has splits (`dialog-account.c:1620-1625`).
fn change_commodity(book: &mut Book, guid: Guid, c: &CommodityRef) -> DomainResult<()> {
    if has_splits(book, guid) {
        return Err(DomainError::conflict("the commodity of an account with splits cannot change"));
    }
    let fraction = commodity_fraction(book, c).ok_or_else(|| DomainError::not_found("commodity", c))?;
    let acc = account_mut(book, guid)?;
    let old = acc.commodity.replace(c.clone());
    acc.commodity_scu = Some(fraction);
    acc.non_standard_scu = false;
    account_commodity_changed(book, old.as_ref(), Some(c));
    Ok(())
}

fn is_scu_choice(f: i64) -> bool {
    (0..=9).any(|k| 10_i64.pow(k) == f)
}

/// Apply the SCU choice as `gnc_ui_to_account` does (`dialog-account.c:525-545`): set the non-standard flag, then
/// `xaccAccountSetCommoditySCU` when the SCU changes, which sets the flag again if the SCU is not the commodity's
/// fraction (`5.17:libgnucash/engine/Account.cpp:2710`).
fn apply_scu(book: &mut Book, guid: Guid, scu: Scu, commodity_changed: bool) -> DomainResult<()> {
    if let Scu::Fraction(f) = scu
        && !is_scu_choice(f)
    {
        return Err(DomainError::invalid(format!(
            "the smallest fraction must be a power of ten from 1 to 1000000000, not {f}"
        )));
    }
    let acc = account(book, guid)?;
    let commodity = acc
        .commodity
        .clone()
        .ok_or_else(|| DomainError::invalid(format!("{} has no commodity", acc.name)))?;
    let fraction = commodity_fraction(book, &commodity).ok_or_else(|| DomainError::not_found("commodity", &commodity))?;
    let old_scu = if commodity_changed { 0 } else { acc.scu(fraction) };
    let (nonstd, new_scu) = match scu {
        Scu::Commodity => (false, fraction),
        Scu::Fraction(f) => (true, f),
    };
    let acc = account_mut(book, guid)?;
    acc.non_standard_scu = nonstd;
    if old_scu != new_scu {
        acc.commodity_scu = Some(new_scu);
        acc.non_standard_scu |= new_scu != fraction;
    }
    Ok(())
}

fn text(s: &str) -> Option<String> {
    (!s.is_empty()).then(|| s.to_owned())
}

/// The text and flag fields, each changed only when it differs, so slots GnuCash wrote another way (such as an
/// integer `tax-related`) stay as they are unless asked.
fn apply_details(book: &mut Book, guid: Guid, edit: &AccountEdit) -> DomainResult<Guid> {
    let acc = account_mut(book, guid)?;
    if let Some(code) = &edit.code {
        acc.code = text(code);
    }
    if let Some(d) = &edit.description {
        acc.description = text(d);
    }
    let set_str = |acc: &mut Account, path: &str, value: &Option<String>| {
        if let Some(v) = value.as_deref().filter(|v| acc.slots.get_str(path).unwrap_or_default() != *v) {
            acc.slots.set_opt_str(path, Some(v));
        }
    };
    set_str(acc, slot_paths::NOTES, &edit.notes);
    set_str(acc, slot_paths::TAX_US_CODE, &edit.tax_us_code);
    set_str(acc, slot_paths::TAX_US_PAYER_NAME_SOURCE, &edit.tax_us_payer_name_source);
    if let Some(on) = edit.placeholder.filter(|on| *on != acc.is_placeholder()) {
        acc.set_placeholder(on);
    }
    if let Some(on) = edit.hidden.filter(|on| *on != acc.is_hidden()) {
        acc.set_hidden(on);
    }
    if let Some(on) = edit.tax_related.filter(|on| *on != acc.is_tax_related()) {
        acc.set_tax_related(on);
    }
    if let Some(n) = edit.tax_us_copy_number {
        set_copy_number(acc, n)?;
    }
    Ok(guid)
}

/// `xaccAccountSetTaxUSCopyNumber` (`5.17:libgnucash/engine/Account.cpp:4066`): 0 deletes the slot.
fn set_copy_number(acc: &mut Account, n: i64) -> DomainResult<()> {
    if n < 0 {
        return Err(DomainError::invalid(format!("the TXF copy number cannot be negative: {n}")));
    }
    let stored = acc.slots.get(slot_paths::TAX_US_COPY_NUMBER).and_then(SlotValue::as_int);
    match (n, stored) {
        (0, Some(_)) => {
            acc.slots.remove(slot_paths::TAX_US_COPY_NUMBER);
        }
        (0, None) => {}
        (n, s) if s != Some(n) => acc.slots.set(slot_paths::TAX_US_COPY_NUMBER, SlotValue::Integer(n)),
        _ => {}
    }
    Ok(())
}

/// Move an account, with its sub-accounts, under a new parent. It becomes the parent's last child. The checks are
/// those of the account dialog's OK (`gnc_common_ok`, `dialog-account.c:963-1015`).
pub fn move_account(book: &mut Book, guid: Guid, new_parent: Guid) -> DomainResult<()> {
    not_root(book, guid, "moved")?;
    let acc = account(book, guid)?;
    let parent = account(book, new_parent)?;
    if subtree(book, guid).contains(&new_parent) {
        return Err(DomainError::invalid(format!(
            "{} cannot be moved under itself or one of its sub-accounts",
            acc.name
        )));
    }
    if acc.parent == Some(new_parent) {
        return Ok(());
    }
    if !acc.account_type.compatible_with_parent(parent.account_type) {
        return Err(DomainError::invalid(format!(
            "a {} account cannot be placed under a {} account",
            acc.account_type, parent.account_type
        )));
    }
    check_no_sibling(book, new_parent, &acc.name, Some(guid))?;
    account_mut(book, guid)?.parent = Some(new_parent);
    place_subtree(book, guid);
    Ok(())
}

// ---------------------------------------------------------------------------------------------------------------
// Renumber

/// The renumber dialog's inputs (`5.17:gnucash/gnome-utils/dialog-account.c:2214-2375`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Renumber {
    /// Prefix; `None` uses the parent's own code, as the dialog does.
    pub prefix: Option<String>,
    /// Step between codes, 1 to 10000 (default 10).
    pub interval: i64,
    /// Minimum number of digits, 1 to 100 (default 3).
    pub digits: i64,
}

impl Default for Renumber {
    fn default() -> Self {
        Self {
            prefix: None,
            interval: 10,
            digits: 3,
        }
    }
}

fn decimal_digits(n: i64) -> i64 {
    std::iter::successors(Some(n), |x| (*x >= 10).then_some(x / 10))
        .count()
        .try_into()
        .unwrap_or(i64::MAX)
}

/// Give the immediate children of `parent`, in display order, the codes `prefix-NNN` (or `NNN` with no prefix)
/// for `interval`, `2 × interval`, …, zero-padded to at least `digits` and never fewer than the last number needs
/// (`gnc_account_renumber_update_examples` and `gnc_account_renumber_response_cb`, `dialog-account.c:2214-2330`).
/// Returns each child with its new code.
pub fn renumber_children(book: &mut Book, parent: Guid, r: &Renumber) -> DomainResult<Vec<(Guid, String)>> {
    let parent_account = account(book, parent)?;
    if !(1..=10_000).contains(&r.interval) {
        return Err(DomainError::invalid(format!(
            "the interval must be from 1 to 10000, not {}",
            r.interval
        )));
    }
    if !(1..=100).contains(&r.digits) {
        return Err(DomainError::invalid(format!("the digits must be from 1 to 100, not {}", r.digits)));
    }
    let children: Vec<Guid> = sorted_children(book, parent).iter().map(|a| a.guid).collect();
    if children.is_empty() {
        return Err(DomainError::invalid(format!(
            "{} has no sub-accounts to renumber",
            parent_account.name
        )));
    }
    let prefix = r.prefix.clone().unwrap_or_else(|| parent_account.code.clone().unwrap_or_default());
    let n = i64::try_from(children.len()).unwrap_or(i64::MAX);
    let width = usize::try_from(r.digits.max(decimal_digits(n.saturating_mul(r.interval)))).unwrap_or(1);
    let codes: Vec<(Guid, String)> = children
        .into_iter()
        .zip(1_i64..)
        .map(|(g, i)| {
            let number = i * r.interval;
            (
                g,
                if prefix.is_empty() {
                    format!("{number:0width$}")
                } else {
                    format!("{prefix}-{number:0width$}")
                },
            )
        })
        .collect();
    codes
        .iter()
        .try_for_each(|(g, code)| account_mut(book, *g).map(|a| a.code = Some(code.clone())))?;
    Ok(codes)
}

// ---------------------------------------------------------------------------------------------------------------
// Delete with move

/// What to do with an account's transactions or sub-account when it is deleted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Disposal {
    /// Move the splits to this account (transactions), or re-parent the sub-account under it.
    MoveTo(Guid),
    /// Delete the transactions (whole, including other accounts' splits), or the sub-account.
    Delete,
}

/// The choices of GnuCash's delete-account dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DeleteOptions {
    /// The account's own transactions; required when it has splits.
    pub transactions: Option<Disposal>,
    /// Its sub-account; required when it has one.
    pub subaccount: Option<Disposal>,
    /// The transactions of a deleted sub-account and its descendants; required when they have splits.
    pub subaccount_transactions: Option<Disposal>,
    /// Allow moving splits to an account in another commodity. GnuCash only warns: the amounts keep their numbers
    /// but now count the target's commodity.
    pub allow_commodity_mismatch: bool,
}

/// What a delete did.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DeleteReport {
    /// Accounts deleted, the account first.
    pub deleted_accounts: Vec<Guid>,
    /// The sub-account moved to another parent, if any.
    pub reparented: Option<Guid>,
    /// Splits moved to another account.
    pub moved_splits: Vec<Guid>,
    /// Transactions deleted.
    pub deleted_transactions: Vec<Guid>,
    /// Lots deleted: those of the deleted accounts and those left empty.
    pub deleted_lots: Vec<Guid>,
}

/// Delete an account as GnuCash's delete dialog does (`gnc_plugin_page_account_tree_cmd_delete_account` and
/// `do_delete_account`, `5.17:gnucash/gnome/gnc-plugin-page-account-tree.cpp:1503-1745`).
///
/// Refused: the root; an account (or a sub-account to be deleted) that business objects use; an account with more
/// than one sub-account; a missing choice; deleting read-only transactions; a target that is not of the same type,
/// is being deleted, or (for splits) is a placeholder or hidden; a commodity mismatch unless allowed.
pub fn delete_account(book: &mut Book, guid: Guid, opts: &DeleteOptions) -> DomainResult<DeleteReport> {
    not_root(book, guid, "deleted")?;
    let plan = Plan::new(book, guid, opts)?;
    plan.check(book, opts)?;
    plan.execute(book)
}

struct Plan {
    account: Account,
    child: Option<Guid>,
    descendants: Vec<Guid>,
    transactions: Option<Disposal>,
    subaccount: Option<Disposal>,
    subaccount_transactions: Option<Disposal>,
}

fn required(choice: Option<Disposal>, what: &str) -> DomainResult<Disposal> {
    choice.ok_or_else(|| DomainError::invalid(format!("say what to do with {what}: move or delete")))
}

impl Plan {
    fn new(book: &Book, guid: Guid, opts: &DeleteOptions) -> DomainResult<Self> {
        let account = account(book, guid)?.clone();
        let children: Vec<Guid> = book.children(guid).map(|a| a.guid).collect();
        if children.len() > 1 {
            return Err(DomainError::conflict(format!(
                "{} has more than one sub-account; move or delete them before deleting it",
                account.name
            )));
        }
        let descendants: Vec<Guid> = book.descendants(guid).iter().map(|a| a.guid).collect();
        let transactions = has_splits(book, guid)
            .then(|| required(opts.transactions, "its transactions"))
            .transpose()?;
        let subaccount = children.first().map(|_| required(opts.subaccount, "its sub-account")).transpose()?;
        let sub_splits = descendants.iter().any(|g| has_splits(book, *g));
        let subaccount_transactions = (subaccount == Some(Disposal::Delete) && sub_splits)
            .then(|| required(opts.subaccount_transactions, "the sub-account's transactions"))
            .transpose()?;
        Ok(Self {
            account,
            child: children.first().copied(),
            descendants,
            transactions,
            subaccount,
            subaccount_transactions,
        })
    }

    fn doomed(&self) -> Vec<Guid> {
        let subs = if self.subaccount == Some(Disposal::Delete) {
            self.descendants.clone()
        } else {
            Vec::new()
        };
        std::iter::once(self.account.guid).chain(subs).collect()
    }

    fn check(&self, book: &Book, opts: &DeleteOptions) -> DomainResult<()> {
        let doomed = self.doomed();
        check_referrers(book, &doomed)?;
        check_budgets(book, &doomed)?;
        if let (Some(Disposal::MoveTo(p)), Some(child)) = (self.subaccount, self.child) {
            let below: Vec<Guid> = std::iter::once(self.account.guid).chain(self.descendants.iter().copied()).collect();
            self.check_target(book, p, &below)?;
            let name = account(book, child)?.name.clone();
            check_no_sibling(book, p, &name, Some(child))?;
        }
        let own = [self.account.guid];
        match self.transactions {
            Some(Disposal::MoveTo(t)) => self.check_split_target(book, t, &doomed, &own, opts.allow_commodity_mismatch)?,
            Some(Disposal::Delete) => check_deletable(book, &own)?,
            None => {}
        }
        match self.subaccount_transactions {
            Some(Disposal::MoveTo(s)) => self.check_split_target(book, s, &doomed, &self.descendants, opts.allow_commodity_mismatch)?,
            Some(Disposal::Delete) => check_deletable(book, &self.descendants)?,
            None => {}
        }
        Ok(())
    }

    /// Every selector offers only accounts of the deleted account's type (`account_delete_dialog`, "FIXME: Same
    /// account type used for subaccount", `gnc-plugin-page-account-tree.cpp:1386-1400`), excluding the accounts
    /// being deleted (`gppat_get_exclude_list`).
    fn check_target(&self, book: &Book, target: Guid, excluded: &[Guid]) -> DomainResult<()> {
        let t = account(book, target)?;
        if excluded.contains(&target) {
            return Err(DomainError::invalid(format!(
                "{} is being deleted and cannot receive anything",
                t.name
            )));
        }
        if t.account_type != self.account.account_type {
            return Err(DomainError::invalid(format!(
                "the target {} must be a {} account like {}",
                t.name, self.account.account_type, self.account.name
            )));
        }
        Ok(())
    }

    /// Split targets also hide placeholder and hidden accounts (`account_is_visible_func`,
    /// `5.17:gnucash/gnome-utils/gnc-account-sel.c:560-582`), and a commodity mismatch asks for confirmation
    /// (`adopter_set_account_and_match`, `gnc-plugin-page-account-tree.cpp:1332-1364`).
    fn check_split_target(&self, book: &Book, target: Guid, excluded: &[Guid], sources: &[Guid], allow_mismatch: bool) -> DomainResult<()> {
        self.check_target(book, target, excluded)?;
        let t = account(book, target)?;
        if t.is_placeholder() || is_hidden_in_tree(book, target) {
            return Err(DomainError::invalid(format!(
                "splits cannot be moved to {}: it is a placeholder or hidden",
                t.name
            )));
        }
        let mismatch = sources
            .iter()
            .filter_map(|g| book.account(*g))
            .find(|a| has_splits(book, a.guid) && a.commodity != t.commodity);
        match mismatch {
            Some(a) if !allow_mismatch => Err(DomainError::conflict(format!(
                "{} holds {} but {} holds {}; pass allowCommodityMismatch to move the amounts unconverted",
                a.name,
                a.commodity.as_ref().map(ToString::to_string).unwrap_or_default(),
                t.name,
                t.commodity.as_ref().map(ToString::to_string).unwrap_or_default()
            ))),
            _ => Ok(()),
        }
    }

    /// `do_delete_account` (`gnc-plugin-page-account-tree.cpp:1674-1745`): sub-account first, then the account's
    /// own splits, then the account and whatever remains below it.
    fn execute(&self, book: &mut Book) -> DomainResult<DeleteReport> {
        let mut report = DeleteReport::default();
        match (self.subaccount, self.child) {
            (Some(Disposal::MoveTo(p)), Some(child)) => {
                account_mut(book, child)?.parent = Some(p);
                place_subtree(book, child);
                report.reparented = Some(child);
            }
            (Some(Disposal::Delete), _) => {
                self.descendants
                    .iter()
                    .try_for_each(|g| dispose(book, *g, self.subaccount_transactions, &mut report))?;
            }
            _ => {}
        }
        dispose(book, self.account.guid, self.transactions, &mut report)?;
        let doomed: BTreeSet<Guid> = self.doomed().into_iter().collect();
        let released: Vec<CommodityRef> = doomed
            .iter()
            .filter_map(|g| book.account(*g))
            .filter_map(|a| a.commodity.clone())
            .collect();
        book.accounts.retain(|g, _| !doomed.contains(g));
        released.iter().for_each(|c| account_commodity_changed(book, Some(c), None));
        let lots: Vec<Guid> = book.lots.values().filter(|l| doomed.contains(&l.account)).map(|l| l.guid).collect();
        book.lots.retain(|g, _| !lots.contains(g));
        report.deleted_lots.extend(lots);
        report.deleted_accounts = self.doomed();
        Ok(report)
    }
}

fn dispose(book: &mut Book, from: Guid, how: Option<Disposal>, report: &mut DeleteReport) -> DomainResult<()> {
    match how {
        Some(Disposal::MoveTo(to)) => report.moved_splits.extend(move_all_splits(book, from, to)?),
        Some(Disposal::Delete) => {
            let (txns, lots) = destroy_all_transactions(book, from);
            report.deleted_transactions.extend(txns);
            report.deleted_lots.extend(lots);
        }
        None => {}
    }
    Ok(())
}

/// Business objects that use an account stop its deletion (`qof_instance_get_referring_object_list`; the
/// `impl_refers_to_object` of `5.17:libgnucash/engine/gncInvoice.c:230`, `gncEntry.c:345`, `gncEmployee.c:242`,
/// `gncTaxTable.c:310`). GnuCash checks only the account itself; red-ale also checks a sub-account it would delete.
fn check_referrers(book: &Book, accounts: &[Guid]) -> DomainResult<()> {
    let uses = |g: Option<Guid>| g.is_some_and(|g| accounts.contains(&g));
    let b = &book.business;
    let invoices = b
        .invoices
        .values()
        .filter(|i| uses(i.post_acc))
        .map(|i| format!("document {}", i.id));
    let entries = b
        .entries
        .values()
        .filter(|e| uses(e.i_account) || uses(e.b_account))
        .map(|e| format!("entry {}", e.guid));
    let employees = b.employees.values().filter(|e| uses(e.ccard)).map(|e| format!("employee {}", e.id));
    let tables = b
        .tax_tables
        .values()
        .filter(|t| t.entries.iter().any(|e| uses(e.account)))
        .map(|t| format!("tax table {}", t.name));
    let referrers: Vec<String> = invoices.chain(entries).chain(employees).chain(tables).collect();
    if referrers.is_empty() {
        Ok(())
    } else {
        Err(DomainError::conflict(format!(
            "the account is used by business objects; change them first: {}",
            referrers.join(", ")
        )))
    }
}

/// Budgets with amounts or notes for an account stop its deletion. GnuCash deletes the account and keeps the budget's
/// Slots for it, which then name no account (`design/notes/engine-budgets.md` §1.7); red-ale refuses instead
/// (`design/milestone4.md`, "Compatibility in v0.4").
fn check_budgets(book: &Book, accounts: &[Guid]) -> DomainResult<()> {
    let budgets: Vec<String> = book
        .budgets
        .values()
        .filter(|b| accounts.iter().any(|a| b.mentions(*a)))
        .map(|b| format!("budget {:?}", b.name))
        .collect();
    if budgets.is_empty() {
        Ok(())
    } else {
        Err(DomainError::conflict(format!(
            "the account has amounts or notes in budgets; change them first: {}",
            budgets.join(", ")
        )))
    }
}

fn transactions_touching(book: &Book, account: Guid) -> Vec<Guid> {
    book.transactions
        .values()
        .filter(|t| t.splits.iter().any(|s| s.account == Some(account)))
        .map(|t| t.guid)
        .collect()
}

/// Deleting read-only transactions is not offered (`delete_account_helper`, `gnc-plugin-page-account-tree.cpp:1165`,
/// and the insensitive "Delete" buttons at `:1415-1475`).
fn check_deletable(book: &Book, accounts: &[Guid]) -> DomainResult<()> {
    let read_only = accounts.iter().flat_map(|a| transactions_touching(book, *a)).find_map(|g| {
        book.transaction(g)
            .and_then(|t| t.slots.get_str(txn_slots::READ_ONLY).map(|reason| (g, reason.to_owned())))
    });
    match read_only {
        Some((g, reason)) => Err(DomainError::conflict(format!(
            "transaction {g} is read-only ({reason}); move its splits instead"
        ))),
        None => Ok(()),
    }
}

/// `xaccAccountMoveAllSplits` (`5.17:libgnucash/engine/Account.cpp:2184`): each split's account becomes `to` and
/// its amount is re-rounded to `to`'s SCU; the value is untouched and the split leaves a lot of another account
/// (`xaccSplitCommitEdit`, `5.17:libgnucash/engine/Split.cpp:995-1003`). The commit re-sorts the splits.
pub fn move_all_splits(book: &mut Book, from: Guid, to: Guid) -> DomainResult<Vec<Guid>> {
    if from == to {
        return Ok(Vec::new());
    }
    account(book, to)?;
    let mut moved = Vec::new();
    transactions_touching(book, from)
        .into_iter()
        .try_for_each(|g| -> DomainResult<()> {
            let Some(original) = book.transactions.get(&g) else { return Ok(()) };
            let mut t = Transaction::clone(original);
            t.splits
                .iter_mut()
                .filter(|s| s.account == Some(from))
                .try_for_each(|s| -> DomainResult<()> {
                    s.account = Some(to);
                    s.amount = round_amount(book, to, s.amount)?;
                    if s.lot.is_some_and(|l| book.lot(l).is_none_or(|lot| lot.account != to)) {
                        s.lot = None;
                    }
                    moved.push(s.guid);
                    Ok(())
                })?;
            sort_splits(&mut t);
            book.transactions.insert(g, Arc::new(t));
            Ok(())
        })?;
    Ok(moved)
}

/// `xaccAccountDestroyAllTransactions` (`5.17:libgnucash/engine/Account.cpp:1588`): every transaction with a split in
/// the account goes, other accounts' splits included. A lot left with no splits is dropped from its account
/// (`gnc_lot_remove_split`, `5.17:libgnucash/engine/gnc-lot.cpp:631-652`). Returns the transactions and lots removed.
pub fn destroy_all_transactions(book: &mut Book, account: Guid) -> (Vec<Guid>, Vec<Guid>) {
    let doomed = transactions_touching(book, account);
    let touched_lots: BTreeSet<Guid> = doomed
        .iter()
        .filter_map(|g| book.transaction(*g))
        .flat_map(|t| t.splits.iter().filter_map(|s| s.lot))
        .collect();
    book.transactions.retain(|g, _| !doomed.contains(g));
    let still_used: BTreeSet<Guid> = book
        .transactions
        .values()
        .flat_map(|t| t.splits.iter().filter_map(|s| s.lot))
        .collect();
    let empty: Vec<Guid> = touched_lots.into_iter().filter(|l| !still_used.contains(l)).collect();
    book.lots.retain(|g, _| !empty.contains(g));
    (doomed, empty)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ops::balances::balances;
    use crate::{Date, FixedEnv, Time64, ops::commit::NewSplit, ops::commit::NewTransaction, ops::commit::add_transaction};

    struct Fx {
        book: Book,
        env: FixedEnv,
    }

    impl Fx {
        fn new() -> Self {
            let mut env = FixedEnv::new(Time64(1_700_000_000), 0);
            let book = Book::new_empty(&mut env);
            Self { book, env }
        }

        fn add(&mut self, parent: Guid, name: &str, t: AccountType) -> Guid {
            let root = self.book.root;
            let edit = AccountEdit {
                name: Some(name.into()),
                account_type: Some(t),
                commodity: Some(CommodityRef::currency("USD")),
                ..AccountEdit::default()
            };
            create_account(
                &mut self.book,
                &mut self.env,
                if parent == Guid::NULL { root } else { parent },
                &edit,
            )
            .unwrap()
        }

        fn post(&mut self, a: Guid, b: Guid, cents: i64, day: u32) -> Guid {
            let mut t = NewTransaction::new(CommodityRef::currency("USD"), Date::from_ymd(2024, 1, day).unwrap());
            t.splits = vec![
                NewSplit::new(a, Numeric::new(cents, 100)),
                NewSplit::new(b, Numeric::new(-cents, 100)),
            ];
            add_transaction(&mut self.book, &mut self.env, t).unwrap()
        }

        fn order(&self) -> Vec<String> {
            self.book.accounts.values().skip(1).map(|a| a.name.clone()).collect()
        }
    }

    #[test]
    fn create_checks_names_types_and_commodities() {
        let mut f = Fx::new();
        let assets = f.add(Guid::NULL, "Assets", AccountType::Asset);
        let bank = f.add(assets, "Bank", AccountType::Bank);
        assert_eq!(f.book.full_name(bank).unwrap(), "Assets:Bank");
        assert_eq!(f.book.account(bank).unwrap().commodity_scu, Some(100));
        let mut edit = AccountEdit {
            name: Some("Bank".into()),
            account_type: Some(AccountType::Bank),
            ..AccountEdit::default()
        };
        let root = f.book.root;
        assert!(
            matches!(
                create_account(&mut f.book, &mut f.env, assets, &edit),
                Err(DomainError::Conflict(_))
            ),
            "sibling clash"
        );
        edit.name = Some("A:B".into());
        assert!(matches!(
            create_account(&mut f.book, &mut f.env, assets, &edit),
            Err(DomainError::Invalid(_))
        ));
        edit.name = Some("Food".into());
        edit.account_type = Some(AccountType::Expense);
        assert!(
            create_account(&mut f.book, &mut f.env, assets, &edit).is_err(),
            "EXPENSE under ASSET"
        );
        edit.account_type = Some(AccountType::Stock);
        assert!(
            create_account(&mut f.book, &mut f.env, assets, &edit).is_err(),
            "a stock needs a security; the parent's USD does not do"
        );
        edit.account_type = Some(AccountType::Currency);
        edit.commodity = Some(CommodityRef::currency("USD"));
        assert!(
            create_account(&mut f.book, &mut f.env, root, &edit).is_err(),
            "CURRENCY is deprecated"
        );
        edit.account_type = Some(AccountType::Bank);
        edit.commodity = None;
        edit.scu = Some(Scu::Fraction(1000));
        edit.placeholder = Some(true);
        let g = create_account(&mut f.book, &mut f.env, assets, &edit).unwrap();
        let a = f.book.account(g).unwrap();
        assert_eq!((a.commodity_scu, a.non_standard_scu, a.is_placeholder()), (Some(1000), true, true));
        assert_eq!(a.slots.get_str("placeholder"), Some("true"));
    }

    #[test]
    fn moves_keep_parents_before_children_in_storage_order() {
        let mut f = Fx::new();
        let a = f.add(Guid::NULL, "A", AccountType::Asset);
        let a1 = f.add(a, "A1", AccountType::Asset);
        f.add(a1, "A1x", AccountType::Asset);
        let b = f.add(Guid::NULL, "B", AccountType::Asset);
        f.add(b, "B1", AccountType::Asset);
        let c = f.add(a, "A2", AccountType::Asset);
        assert_eq!(
            f.order(),
            ["A", "A1", "A1x", "A2", "B", "B1"],
            "a new account goes after its parent's subtree"
        );
        move_account(&mut f.book, a1, b).unwrap();
        assert_eq!(f.order(), ["A", "A2", "B", "B1", "A1", "A1x"]);
        assert_eq!(f.book.full_name(a1).unwrap(), "B:A1");
        assert!(move_account(&mut f.book, b, a1).is_err(), "under its own descendant");
        move_account(&mut f.book, b, c).unwrap();
        assert_eq!(f.order(), ["A", "A2", "B", "B1", "A1", "A1x"]);
        assert_eq!(f.book.children(c).map(|x| x.name.as_str()).collect::<Vec<_>>(), ["B"]);
    }

    #[test]
    fn type_changes_follow_the_dialog() {
        let mut f = Fx::new();
        let a = f.add(Guid::NULL, "A", AccountType::Asset);
        let child = f.add(a, "Child", AccountType::Bank);
        let other = f.add(Guid::NULL, "Other", AccountType::Equity);
        let mut edit = AccountEdit {
            account_type: Some(AccountType::Equity),
            ..AccountEdit::default()
        };
        assert!(
            matches!(update_account(&mut f.book, a, &edit), Err(DomainError::Conflict(_))),
            "children need confirming"
        );
        edit.retype_children = true;
        update_account(&mut f.book, a, &edit).unwrap();
        assert_eq!(f.book.account(child).unwrap().account_type, AccountType::Equity);
        f.post(child, other, 500, 2);
        edit.account_type = Some(AccountType::Receivable);
        edit.retype_children = false;
        assert!(
            update_account(&mut f.book, child, &edit).is_err(),
            "EQUITY cannot become A/R under EQUITY"
        );
        assert!(!type_change_allowed_with_splits(AccountType::Receivable, AccountType::Asset));
        assert!(type_change_allowed_with_splits(AccountType::Bank, AccountType::Income));
        let commodity = AccountEdit {
            commodity: Some(CommodityRef::currency("EUR")),
            ..AccountEdit::default()
        };
        assert!(
            matches!(update_account(&mut f.book, child, &commodity), Err(DomainError::Conflict(_))),
            "has splits"
        );
    }

    #[test]
    fn details_change_only_what_differs() {
        let mut f = Fx::new();
        let a = f.add(Guid::NULL, "A", AccountType::Expense);
        Arc::make_mut(f.book.accounts.get_mut(&a).unwrap())
            .slots
            .set("tax-related", SlotValue::Integer(1));
        let edit = AccountEdit {
            tax_related: Some(true),
            tax_us_code: Some("N123".into()),
            tax_us_copy_number: Some(2),
            notes: Some("n".into()),
            code: Some("5100".into()),
            ..AccountEdit::default()
        };
        update_account(&mut f.book, a, &edit).unwrap();
        let acc = f.book.account(a).unwrap();
        assert_eq!(
            acc.slots.get("tax-related"),
            Some(&SlotValue::Integer(1)),
            "already true: left alone"
        );
        assert_eq!(acc.slots.get_str("tax-US/code"), Some("N123"));
        assert_eq!(acc.code.as_deref(), Some("5100"));
        let clear = AccountEdit {
            notes: Some(String::new()),
            tax_us_copy_number: Some(0),
            tax_related: Some(false),
            ..AccountEdit::default()
        };
        update_account(&mut f.book, a, &clear).unwrap();
        let acc = f.book.account(a).unwrap();
        assert!(acc.notes().is_none() && acc.slots.get("tax-US/copy-number").is_none() && acc.slots.get("tax-related").is_none());
    }

    #[test]
    fn renumber_as_the_dialog() {
        let mut f = Fx::new();
        let p = f.add(Guid::NULL, "P", AccountType::Expense);
        update_account(
            &mut f.book,
            p,
            &AccountEdit {
                code: Some("1000".into()),
                ..AccountEdit::default()
            },
        )
        .unwrap();
        ["C", "A", "B"].iter().for_each(|n| {
            f.add(p, n, AccountType::Expense);
        });
        let codes: Vec<String> = renumber_children(&mut f.book, p, &Renumber::default())
            .unwrap()
            .into_iter()
            .map(|(_, c)| c)
            .collect();
        assert_eq!(codes, ["1000-010", "1000-020", "1000-030"]);
        let r = Renumber {
            prefix: Some(String::new()),
            interval: 400,
            digits: 1,
        };
        let named: Vec<(String, String)> = renumber_children(&mut f.book, p, &r)
            .unwrap()
            .into_iter()
            .map(|(g, c)| (f.book.account(g).unwrap().name.clone(), c))
            .collect();
        assert_eq!(
            named,
            [
                ("A".into(), "0400".into()),
                ("B".into(), "0800".into()),
                ("C".into(), "1200".into())
            ],
            "sorted by the current codes"
        );
        assert!(
            renumber_children(
                &mut f.book,
                p,
                &Renumber {
                    interval: 0,
                    ..Renumber::default()
                }
            )
            .is_err()
        );
    }

    #[test]
    fn delete_moves_or_deletes() {
        let mut f = Fx::new();
        let bank = f.add(Guid::NULL, "Bank", AccountType::Bank);
        let sub = f.add(bank, "Sub", AccountType::Bank);
        let other = f.add(Guid::NULL, "Other", AccountType::Bank);
        let food = f.add(Guid::NULL, "Food", AccountType::Expense);
        f.post(food, bank, 300, 2);
        let t2 = f.post(food, sub, 200, 3);
        let none = DeleteOptions::default();
        assert!(matches!(delete_account(&mut f.book, bank, &none), Err(DomainError::Invalid(_))));
        let opts = DeleteOptions {
            transactions: Some(Disposal::MoveTo(other)),
            subaccount: Some(Disposal::Delete),
            subaccount_transactions: Some(Disposal::Delete),
            ..DeleteOptions::default()
        };
        let wrong = DeleteOptions {
            transactions: Some(Disposal::MoveTo(food)),
            ..opts
        };
        assert!(delete_account(&mut f.book, bank, &wrong).is_err(), "a target of another type");
        let r = delete_account(&mut f.book, bank, &opts).unwrap();
        assert_eq!(r.deleted_accounts, [bank, sub]);
        assert_eq!(r.deleted_transactions, [t2]);
        assert_eq!(balances(&f.book, other, None).unwrap().balance, Numeric::new(-300, 100));
        assert!(f.book.transactions.values().all(|t| t.is_balanced()));
    }

    #[test]
    fn delete_reparents_a_single_subaccount_and_refuses_read_only() {
        let mut f = Fx::new();
        let first = f.add(Guid::NULL, "A", AccountType::Asset);
        let child = f.add(first, "Child", AccountType::Asset);
        let second = f.add(Guid::NULL, "B", AccountType::Asset);
        let spend = f.add(Guid::NULL, "E", AccountType::Expense);
        let txn = f.post(spend, first, 100, 2);
        Arc::make_mut(f.book.transactions.get_mut(&txn).unwrap())
            .slots
            .set(txn_slots::READ_ONLY, SlotValue::String("posted".into()));
        let opts = DeleteOptions {
            transactions: Some(Disposal::Delete),
            subaccount: Some(Disposal::MoveTo(second)),
            ..DeleteOptions::default()
        };
        assert!(matches!(delete_account(&mut f.book, first, &opts), Err(DomainError::Conflict(_))));
        let opts = DeleteOptions {
            transactions: Some(Disposal::MoveTo(child)),
            ..opts
        };
        let r = delete_account(&mut f.book, first, &opts).unwrap();
        assert_eq!(r.reparented, Some(child));
        assert_eq!(f.book.full_name(child).unwrap(), "B:Child");
        assert_eq!(f.order(), ["B", "Child", "E"]);
    }

    #[test]
    fn sorted_descendants_and_depths_walk_the_tree() {
        let mut f = Fx::new();
        let z = f.add(Guid::NULL, "Z", AccountType::Asset);
        let zb = f.add(z, "B", AccountType::Asset);
        f.add(zb, "C", AccountType::Asset);
        f.add(z, "A", AccountType::Asset);
        f.add(Guid::NULL, "Y", AccountType::Asset);
        let names = |v: Vec<&Account>| v.iter().map(|a| a.name.clone()).collect::<Vec<_>>();
        assert_eq!(names(descendants_sorted(&f.book, f.book.root)), ["Y", "Z", "A", "B", "C"]);
        assert_eq!(
            names(f.book.descendants(f.book.root)),
            ["Z", "B", "C", "A", "Y"],
            "book order for comparison"
        );
        let root = f.book.root;
        assert_eq!((depth(&f.book, root), depth(&f.book, z), depth(&f.book, zb)), (0, 1, 2));
        assert_eq!(
            (tree_depth(&f.book, root), tree_depth(&f.book, z), tree_depth(&f.book, zb)),
            (4, 3, 2)
        );
    }

    #[test]
    fn display_order_puts_bank_after_trading_as_gnucash_does() {
        let usd = Some(CommodityRef::currency("USD"));
        let acc = |name: &str, t, code: Option<&str>| {
            let mut a = Account::new(Guid::NULL, name, t, usd.clone(), None);
            a.code = code.map(Into::into);
            a
        };
        let mut v = [
            acc("A", AccountType::Bank, None),
            acc("B", AccountType::Trading, None),
            acc("C", AccountType::Cash, None),
            acc("D", AccountType::Bank, Some("1")),
        ];
        v.sort_by(account_order);
        assert_eq!(v.iter().map(|a| a.name.as_str()).collect::<Vec<_>>(), ["C", "B", "A", "D"]);
    }

    #[test]
    fn delete_refuses_an_account_a_budget_has_amounts_for() {
        use crate::{Budget, Date, Frame, PeriodType, Recurrence, SlotValue, WeekendAdjust};
        let mut f = Fx::new();
        let expenses = f.add(Guid::NULL, "Expenses", AccountType::Expense);
        let food = f.add(expenses, "Food", AccountType::Expense);
        let mut slots = Frame::new();
        slots.set_in(&["notes", &food.to_string(), "13"], SlotValue::String("past the last period".into()));
        let start = Date::from_ymd(2024, 1, 1).unwrap();
        let budget = Budget {
            guid: Guid::from_bytes([9; 16]),
            name: "Plan".into(),
            description: String::new(),
            num_periods: 12,
            recurrence: Recurrence::new(1, PeriodType::Month, start, WeekendAdjust::None).unwrap(),
            slots,
        };
        f.book.budgets.insert(budget.guid, std::sync::Arc::new(budget));
        let refused = |book: &mut Book, guid, subaccount| {
            let opts = DeleteOptions {
                subaccount,
                ..DeleteOptions::default()
            };
            delete_account(book, guid, &opts).unwrap_err().to_string()
        };
        let message = "the account has amounts or notes in budgets; change them first: budget \"Plan\"";
        assert_eq!(refused(&mut f.book, food, None), message);
        assert_eq!(refused(&mut f.book, expenses, Some(Disposal::Delete)), message, "a sub-account it would delete");
        let other = f.add(Guid::NULL, "Other", AccountType::Expense);
        let keep_food = DeleteOptions {
            subaccount: Some(Disposal::MoveTo(other)),
            ..DeleteOptions::default()
        };
        assert!(delete_account(&mut f.book, expenses, &keep_food).is_ok(), "a sub-account it keeps");
    }
}
