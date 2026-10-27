//! Methods: the account tree, account settings and balances. See `design/api/accounts.md`.

use std::collections::BTreeMap;

use red_ale_core::account::slot_paths;
use red_ale_core::ops::accounts::{
    self as ops, ACCOUNT_TYPES, AccountEdit, CommodityRule, DeleteOptions, Disposal, Renumber, Scu, account_order, commodity_rule,
};
use red_ale_core::ops::balances;
use red_ale_core::{Account, AccountType, Book, DomainError, Guid, SlotValue};
use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};

use crate::dispatch::{Context, Effect, Method, method};
use crate::error::RpcError;
use crate::wire::{AccountRef, Amount, CommodityId, Day, Id, NoParams};

/// The accounts methods.
pub fn methods() -> Vec<Method> {
    vec![
        method(
            "account.types",
            "List the account types with GnuCash's labels and rules.",
            Effect::Read,
            types,
        ),
        method(
            "account.list",
            "List accounts depth first in the book's order, and names that contain the separator.",
            Effect::Read,
            list,
        ),
        method("account.get", "Describe one account.", Effect::Read, get),
        method(
            "account.tree",
            "The account tree as nested nodes, in the book's order or GnuCash's display order.",
            Effect::Read,
            tree,
        ),
        method(
            "account.create",
            "Create an account under a parent (the root by default).",
            Effect::Write,
            create,
        ),
        method(
            "account.update",
            "Change an account's name, type, commodity, smallest fraction, code, notes, flags, tax settings, colour, balance limits, \
             opening balance flag or associated accounts.",
            Effect::Write,
            update,
        ),
        method(
            "account.move",
            "Move an account, with its sub-accounts, under another parent.",
            Effect::Write,
            move_account,
        ),
        method(
            "account.renumber",
            "Renumber the codes of an account's immediate sub-accounts.",
            Effect::Write,
            renumber,
        ),
        method(
            "account.delete",
            "Delete an account, moving or deleting its transactions and sub-account.",
            Effect::Destructive,
            delete,
        ),
        method(
            "account.balance",
            "An account's balance, cleared and reconciled balances, optionally as of a date and with sub-accounts.",
            Effect::Read,
            balance,
        ),
    ]
}

fn parse_type(s: &str) -> Result<AccountType, RpcError> {
    s.parse()
        .map_err(|_| RpcError::invalid_params(format!("unknown account type {s:?}")))
}

fn names(types: impl Iterator<Item = AccountType>) -> Vec<String> {
    types.map(|t| t.as_str().to_owned()).collect()
}

/// An account type and GnuCash's rules for it.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TypeInfo {
    /// The name GnuCash writes (`BANK`).
    pub name: String,
    /// GnuCash's label (`Bank`).
    pub label: String,
    /// `ASSET`, `LIABILITY`, `INCOME`, `EXPENSE` or `EQUITY`; none for ROOT and TRADING.
    pub fundamental: Option<String>,
    /// Can be chosen for an account (CURRENCY is deprecated; ROOT is the tree's top).
    pub choosable: bool,
    /// What the account holds: `currency`, `security` or `any`.
    pub commodity: String,
    /// The types its parent may have.
    pub parent_types: Vec<String>,
    /// The types an account of this type may change to once it has splits.
    pub changes_with_splits: Vec<String>,
}

#[allow(clippy::unnecessary_wraps)] // Every handler returns a Result.
fn types(_: &mut Context, _: NoParams) -> Result<Vec<TypeInfo>, RpcError> {
    Ok(ACCOUNT_TYPES
        .iter()
        .map(|t| TypeInfo {
            name: t.as_str().to_owned(),
            label: ops::type_label(*t).to_owned(),
            fundamental: ops::fundamental_type(*t).map(|f| f.as_str().to_owned()),
            choosable: ops::is_choosable(*t),
            commodity: match commodity_rule(*t) {
                CommodityRule::Currency => "currency",
                CommodityRule::Security => "security",
                CommodityRule::Any => "any",
            }
            .to_owned(),
            parent_types: names(ACCOUNT_TYPES.iter().copied().filter(|p| t.compatible_with_parent(*p))),
            changes_with_splits: names(
                ACCOUNT_TYPES
                    .iter()
                    .copied()
                    .filter(|n| ops::is_choosable(*n) && ops::type_change_allowed_with_splits(*t, *n)),
            ),
        })
        .collect())
}

/// US tax (TXF) settings.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TaxUs {
    /// TXF code (`tax-US/code`), such as `N123`.
    pub code: Option<String>,
    /// Payer name source (`tax-US/payer-name-source`): `current` or `parent`.
    pub payer_name_source: Option<String>,
    /// Copy number (`tax-US/copy-number`); 1 when unset, as GnuCash reads it.
    pub copy_number: i64,
}

/// An account.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[allow(clippy::struct_excessive_bools)] // GnuCash's account flags, one field each on the wire.
pub struct AccountInfo {
    /// GUID.
    pub guid: Id,
    /// Name.
    pub name: String,
    /// Full name: ancestors' names joined by `:`, without the root.
    pub full_name: String,
    /// Account type, as GnuCash writes it.
    #[serde(rename = "type")]
    pub account_type: String,
    /// Parent; none only for the root.
    pub parent: Option<Id>,
    /// Commodity held.
    pub commodity: Option<CommodityId>,
    /// Smallest fraction the account's amounts are kept in.
    pub scu: i64,
    /// The smallest fraction was chosen instead of the commodity's.
    pub non_standard_scu: bool,
    /// Account code.
    pub code: Option<String>,
    /// Description.
    pub description: Option<String>,
    /// Notes.
    pub notes: Option<String>,
    /// Groups other accounts; GnuCash's register refuses splits in it.
    pub placeholder: bool,
    /// Hidden itself.
    pub hidden: bool,
    /// Hidden itself or through an ancestor, as GnuCash shows it.
    pub hidden_in_tree: bool,
    /// Tax-related.
    pub tax_related: bool,
    /// US tax (TXF) settings.
    pub tax_us: TaxUs,
    /// Levels below the root (top-level accounts are 1).
    pub depth: usize,
    /// Number of immediate sub-accounts.
    pub children: usize,
    /// The account has splits.
    pub has_splits: bool,
    /// Account colour (`rgb(r,g,b)` as GnuCash's dialog writes it, or as stored); null when unset or `Not Set`.
    pub color: Option<String>,
    /// Higher balance limit, in raw sign (debits positive).
    pub higher_balance_limit: Option<Amount>,
    /// Lower balance limit, in raw sign.
    pub lower_balance_limit: Option<Amount>,
    /// The balance limits apply to the balance with sub-accounts.
    pub include_subaccount_balances: bool,
    /// The EQUITY account holding the opening balances of its commodity.
    pub opening_balance: bool,
    /// Associated accounts by tag (such as `stock-dividends`), each naming an existing account.
    pub associated_accounts: BTreeMap<String, Id>,
}

/// Describe an account.
pub fn info(book: &Book, guid: Guid) -> Result<AccountInfo, RpcError> {
    let a = book.account(guid).ok_or_else(|| RpcError::not_found("account", guid))?;
    let fraction = a
        .commodity
        .as_ref()
        .and_then(|c| red_ale_core::ops::commit::commodity_fraction(book, c))
        .unwrap_or(0);
    let text = |s: Option<&str>| s.filter(|s| !s.is_empty()).map(ToOwned::to_owned);
    Ok(AccountInfo {
        guid: guid.into(),
        name: a.name.clone(),
        full_name: book.full_name(guid).unwrap_or_default(),
        account_type: a.account_type.as_str().to_owned(),
        parent: a.parent.map(Id),
        commodity: a.commodity.clone().map(CommodityId),
        scu: a.scu(fraction),
        non_standard_scu: a.non_standard_scu,
        code: text(a.code.as_deref()),
        description: text(a.description.as_deref()),
        notes: text(a.notes()),
        placeholder: a.is_placeholder(),
        hidden: a.is_hidden(),
        hidden_in_tree: ops::is_hidden_in_tree(book, guid),
        tax_related: a.is_tax_related(),
        tax_us: TaxUs {
            code: text(a.slots.get_str(slot_paths::TAX_US_CODE)),
            payer_name_source: text(a.slots.get_str(slot_paths::TAX_US_PAYER_NAME_SOURCE)),
            copy_number: a
                .slots
                .get(slot_paths::TAX_US_COPY_NUMBER)
                .and_then(SlotValue::as_int)
                .filter(|n| *n != 0)
                .unwrap_or(1),
        },
        depth: if guid == book.root { 0 } else { book.ancestors(guid).len() + 1 },
        children: book.children(guid).count(),
        has_splits: ops::has_splits(book, guid),
        color: a.color().map(ToOwned::to_owned),
        higher_balance_limit: a.higher_balance_limit().map(Amount),
        lower_balance_limit: a.lower_balance_limit().map(Amount),
        include_subaccount_balances: a.includes_subaccount_balances(),
        opening_balance: a.is_opening_balance(),
        associated_accounts: a
            .associated_accounts()
            .into_iter()
            .filter(|(_, g)| book.account(*g).is_some())
            .map(|(tag, g)| (tag.to_owned(), Id(g)))
            .collect(),
    })
}

fn resolve_or_root(book: &Book, r: Option<&AccountRef>) -> Result<Guid, RpcError> {
    r.map_or(Ok(book.root), |r| r.resolve(book))
}

/// Params of `account.list`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ListParams {
    /// List only the descendants of this account; by default every account but the root.
    #[serde(default)]
    pub under: Option<AccountRef>,
    /// Leave out accounts hidden themselves or through an ancestor.
    #[serde(default)]
    pub exclude_hidden: bool,
}

/// The result of `account.list`.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ListResult {
    /// Accounts, depth first: each followed by its sub-accounts, siblings in the book's order.
    pub accounts: Vec<AccountInfo>,
    /// Accounts whose names contain the separator `:`, which makes full names ambiguous (GnuCash warns about them
    /// when it opens a book).
    pub name_violations: Vec<Id>,
}

fn list(ctx: &mut Context, p: ListParams) -> Result<ListResult, RpcError> {
    let book = ctx.book()?;
    let under = resolve_or_root(book, p.under.as_ref())?;
    let accounts = book
        .descendants(under)
        .iter()
        .filter(|a| !(p.exclude_hidden && ops::is_hidden_in_tree(book, a.guid)))
        .map(|a| info(book, a.guid))
        .collect::<Result<_, _>>()?;
    Ok(ListResult {
        accounts,
        name_violations: ops::name_violations(book).into_iter().map(Id).collect(),
    })
}

/// Params naming one account.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AccountParams {
    /// The account.
    pub account: AccountRef,
}

fn get(ctx: &mut Context, p: AccountParams) -> Result<AccountInfo, RpcError> {
    let book = ctx.book()?;
    info(book, p.account.resolve(book)?)
}

/// Params of `account.tree`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct TreeParams {
    /// The top of the tree; the root by default.
    #[serde(default)]
    pub under: Option<AccountRef>,
    /// Order siblings as GnuCash displays them (code, type, name) instead of the book's order.
    #[serde(default)]
    pub sorted: bool,
    /// Leave out hidden accounts and everything below them.
    #[serde(default)]
    pub exclude_hidden: bool,
}

/// A node of the account tree.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TreeNode {
    /// GUID.
    pub guid: Id,
    /// Name.
    pub name: String,
    /// Full name.
    pub full_name: String,
    /// Account type.
    #[serde(rename = "type")]
    pub account_type: String,
    /// Account code.
    pub code: Option<String>,
    /// Commodity held.
    pub commodity: Option<CommodityId>,
    /// Placeholder.
    pub placeholder: bool,
    /// Hidden itself.
    pub hidden: bool,
    /// Sub-accounts.
    pub children: Vec<TreeNode>,
}

fn node(book: &Book, a: &Account, p: &TreeParams) -> TreeNode {
    let mut children: Vec<&Account> = book.children(a.guid).filter(|c| !(p.exclude_hidden && c.is_hidden())).collect();
    if p.sorted {
        children.sort_by(|x, y| account_order(x, y));
    }
    TreeNode {
        guid: a.guid.into(),
        name: a.name.clone(),
        full_name: book.full_name(a.guid).unwrap_or_default(),
        account_type: a.account_type.as_str().to_owned(),
        code: a.code.clone().filter(|c| !c.is_empty()),
        commodity: a.commodity.clone().map(CommodityId),
        placeholder: a.is_placeholder(),
        hidden: a.is_hidden(),
        children: children.into_iter().map(|c| node(book, c, p)).collect(),
    }
}

fn tree(ctx: &mut Context, p: TreeParams) -> Result<TreeNode, RpcError> {
    let book = ctx.book()?;
    let top = resolve_or_root(book, p.under.as_ref())?;
    let account = book.account(top).ok_or_else(|| RpcError::not_found("account", top))?;
    Ok(node(book, account, &p))
}

/// A smallest fraction: `"commodity"` for the commodity's own, or a power of ten from 1 to 1000000000.
#[derive(Debug, Clone, Copy, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum ScuParam {
    /// `"commodity"`.
    Commodity(CommodityKeyword),
    /// A power of ten.
    Fraction(i64),
}

/// The keyword `commodity`.
#[derive(Debug, Clone, Copy, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum CommodityKeyword {
    /// Use the commodity's fraction.
    Commodity,
}

impl From<ScuParam> for Scu {
    fn from(s: ScuParam) -> Self {
        match s {
            ScuParam::Commodity(_) => Scu::Commodity,
            ScuParam::Fraction(f) => Scu::Fraction(f),
        }
    }
}

/// The settable fields shared by `account.create` and `account.update`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Fields {
    /// Commodity: a currency for most types, a security for STOCK and MUTUAL. On create, defaults to the parent's
    /// when it suits the type. Cannot change once the account has splits.
    #[serde(default)]
    pub commodity: Option<CommodityId>,
    /// Smallest fraction.
    #[serde(default)]
    pub scu: Option<ScuParam>,
    /// Account code; empty clears it.
    #[serde(default)]
    pub code: Option<String>,
    /// Description; empty clears it.
    #[serde(default)]
    pub description: Option<String>,
    /// Notes; empty clears them.
    #[serde(default)]
    pub notes: Option<String>,
    /// Placeholder flag.
    #[serde(default)]
    pub placeholder: Option<bool>,
    /// Hidden flag.
    #[serde(default)]
    pub hidden: Option<bool>,
    /// Tax-related flag.
    #[serde(default)]
    pub tax_related: Option<bool>,
    /// US TXF code; empty clears it.
    #[serde(default)]
    pub tax_us_code: Option<String>,
    /// US TXF payer name source (`current` or `parent`); empty clears it.
    #[serde(default)]
    pub tax_us_payer_name_source: Option<String>,
    /// US TXF copy number; 0 clears it.
    #[serde(default)]
    pub tax_us_copy_number: Option<i64>,
    /// Account colour: `#rrggbb`, `#rgb` or `rgb(r,g,b)`, stored as `rgb(r,g,b)`; empty (or GnuCash's default
    /// `rgb(237,236,235)`) clears it.
    #[serde(default)]
    pub color: Option<String>,
    /// Higher balance limit, in raw sign; null clears it.
    #[serde(default, deserialize_with = "present")]
    pub higher_balance_limit: Option<Option<Amount>>,
    /// Lower balance limit, in raw sign; null clears it.
    #[serde(default, deserialize_with = "present")]
    pub lower_balance_limit: Option<Option<Amount>>,
    /// Apply the balance limits to the balance with sub-accounts. Cleared when no limit is left.
    #[serde(default)]
    pub include_subaccount_balances: Option<bool>,
    /// Hold the opening balances of the account's commodity (EQUITY only; one per commodity; not once it has
    /// splits). Setting it declares GnuCash's opening balance account Book feature.
    #[serde(default)]
    pub opening_balance: Option<bool>,
    /// Associated accounts by tag; null removes a tag, tags not named are kept.
    #[serde(default)]
    pub associated_accounts: Option<BTreeMap<String, Option<AccountRef>>>,
}

/// Read a present field as `Some(value)`, so that `null` (clear) and absent (keep) differ.
#[allow(clippy::option_option, clippy::unnecessary_wraps)]
fn present<'de, D: Deserializer<'de>, T: Deserialize<'de>>(d: D) -> Result<Option<Option<T>>, D::Error> {
    Option::<T>::deserialize(d).map(Some)
}

impl Fields {
    fn edit(
        self,
        book: &Book,
        name: Option<String>,
        account_type: Option<AccountType>,
        retype_children: bool,
    ) -> Result<AccountEdit, RpcError> {
        let associated_accounts = self
            .associated_accounts
            .unwrap_or_default()
            .into_iter()
            .map(|(tag, r)| Ok((tag, r.map(|r| r.resolve(book)).transpose()?)))
            .collect::<Result<_, RpcError>>()?;
        Ok(AccountEdit {
            name,
            account_type,
            retype_children,
            commodity: self.commodity.map(|c| c.0),
            scu: self.scu.map(Scu::from),
            code: self.code,
            description: self.description,
            notes: self.notes,
            placeholder: self.placeholder,
            hidden: self.hidden,
            tax_related: self.tax_related,
            tax_us_code: self.tax_us_code,
            tax_us_payer_name_source: self.tax_us_payer_name_source,
            tax_us_copy_number: self.tax_us_copy_number,
            color: self.color,
            higher_balance_limit: self.higher_balance_limit.map(|l| l.map(|a| a.0)),
            lower_balance_limit: self.lower_balance_limit.map(|l| l.map(|a| a.0)),
            include_subaccount_balances: self.include_subaccount_balances,
            opening_balance: self.opening_balance,
            associated_accounts,
        })
    }
}

/// Params of `account.create`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct CreateParams {
    /// Name: not empty, without `:`, and unlike its siblings'.
    pub name: String,
    /// Account type (see `account.types`); it must suit the parent's.
    #[serde(rename = "type")]
    pub account_type: String,
    /// Parent; the root by default.
    #[serde(default)]
    pub parent: Option<AccountRef>,
    /// The other fields.
    #[serde(flatten)]
    pub fields: Fields,
}

fn create(ctx: &mut Context, p: CreateParams) -> Result<AccountInfo, RpcError> {
    let account_type = parse_type(&p.account_type)?;
    ctx.mutate_rpc(|book, env| {
        let parent = resolve_or_root(book, p.parent.as_ref())?;
        let edit = p.fields.edit(book, Some(p.name), Some(account_type), false)?;
        let guid = ops::create_account(book, env, parent, &edit)?;
        info(book, guid)
    })
}

/// Params of `account.update`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct UpdateParams {
    /// The account.
    pub account: AccountRef,
    /// New name.
    #[serde(default)]
    pub name: Option<String>,
    /// New type. With splits, only a compatible type (see `account.types`).
    #[serde(default, rename = "type")]
    pub account_type: Option<String>,
    /// Confirm that sub-accounts whose type cannot sit under the new type take the new type too.
    #[serde(default)]
    pub retype_children: bool,
    /// The other fields.
    #[serde(flatten)]
    pub fields: Fields,
}

fn update(ctx: &mut Context, p: UpdateParams) -> Result<AccountInfo, RpcError> {
    let account_type = p.account_type.as_deref().map(parse_type).transpose()?;
    ctx.mutate_rpc(|book, _| {
        let guid = p.account.resolve(book)?;
        let edit = p.fields.edit(book, p.name, account_type, p.retype_children)?;
        ops::update_account(book, guid, &edit)?;
        info(book, guid)
    })
}

/// Params of `account.move`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct MoveParams {
    /// The account to move, with its sub-accounts.
    pub account: AccountRef,
    /// The new parent; it becomes the last of the parent's children.
    pub parent: AccountRef,
}

fn move_account(ctx: &mut Context, p: MoveParams) -> Result<AccountInfo, RpcError> {
    ctx.mutate_rpc(|book, _| {
        let guid = p.account.resolve(book)?;
        let parent = p.parent.resolve(book)?;
        ops::move_account(book, guid, parent)?;
        info(book, guid)
    })
}

/// Params of `account.renumber`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct RenumberParams {
    /// The account whose immediate sub-accounts are renumbered.
    pub account: AccountRef,
    /// Prefix, joined to the number by `-`; by default the account's own code. Empty means none.
    #[serde(default)]
    pub prefix: Option<String>,
    /// Step between codes, 1 to 10000; default 10.
    #[serde(default)]
    pub interval: Option<i64>,
    /// Minimum digits, 1 to 100; default 3. Never fewer than the largest number needs.
    #[serde(default)]
    pub digits: Option<i64>,
}

/// A sub-account's new code.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct NewCode {
    /// The sub-account.
    pub account: Id,
    /// Its full name.
    pub full_name: String,
    /// Its new code.
    pub code: String,
}

fn renumber(ctx: &mut Context, p: RenumberParams) -> Result<Vec<NewCode>, RpcError> {
    let defaults = Renumber::default();
    let r = Renumber {
        prefix: p.prefix,
        interval: p.interval.unwrap_or(defaults.interval),
        digits: p.digits.unwrap_or(defaults.digits),
    };
    ctx.mutate_rpc(|book, _| {
        let parent = p.account.resolve(book)?;
        let codes = ops::renumber_children(book, parent, &r)?;
        Ok(codes
            .into_iter()
            .map(|(g, code)| NewCode {
                account: g.into(),
                full_name: book.full_name(g).unwrap_or_default(),
                code,
            })
            .collect())
    })
}

/// What to do with transactions or a sub-account: `"delete"`, or `{"moveTo": <account>}`.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum DisposalParam {
    /// Delete them (whole transactions, other accounts' splits included).
    Delete,
    /// Move the splits to this account, or re-parent the sub-account under it. It must have the deleted account's
    /// type; a split target must not be a placeholder or hidden.
    MoveTo(AccountRef),
}

fn disposal(book: &Book, d: Option<&DisposalParam>) -> Result<Option<Disposal>, RpcError> {
    d.map(|d| match d {
        DisposalParam::Delete => Ok(Disposal::Delete),
        DisposalParam::MoveTo(r) => r.resolve(book).map(Disposal::MoveTo),
    })
    .transpose()
}

/// Params of `account.delete`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct DeleteParams {
    /// The account. It may have at most one sub-account.
    pub account: AccountRef,
    /// Its transactions; required when it has splits.
    #[serde(default)]
    pub transactions: Option<DisposalParam>,
    /// Its sub-account; required when it has one.
    #[serde(default)]
    pub subaccount: Option<DisposalParam>,
    /// The deleted sub-account's transactions (and its descendants'); required when they have splits.
    #[serde(default)]
    pub subaccount_transactions: Option<DisposalParam>,
    /// Move splits to an account in another commodity, keeping their amounts' numbers. GnuCash warns and asks.
    #[serde(default)]
    pub allow_commodity_mismatch: bool,
}

/// What `account.delete` did.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DeleteResult {
    /// Accounts deleted, the account first.
    pub deleted_accounts: Vec<Id>,
    /// The sub-account re-parented, if any.
    pub reparented: Option<Id>,
    /// Splits moved.
    pub moved_splits: Vec<Id>,
    /// Transactions deleted.
    pub deleted_transactions: Vec<Id>,
    /// Lots deleted.
    pub deleted_lots: Vec<Id>,
}

/// Template and scheduled transactions are kept as read, not modelled; an account they mention must not be deleted
/// under them, nor a sub-account deleted with it (`design/file-fidelity.md`, "References to out-of-scope data").
fn refuse_if_residue_mentions(ctx: &mut Context, p: &DeleteParams) -> Result<(), RpcError> {
    let session = ctx.session()?;
    let book = session.book();
    let guid = p.account.resolve(book)?;
    let subaccounts = if matches!(p.subaccount, Some(DisposalParam::Delete)) {
        book.descendants(guid)
    } else {
        Vec::new()
    };
    let sections: std::collections::BTreeSet<String> = std::iter::once(guid)
        .chain(subaccounts.iter().map(|a| a.guid))
        .flat_map(|g| session.references(red_ale_service::Reference::Guid(g)))
        .collect();
    if sections.is_empty() {
        Ok(())
    } else {
        let names: Vec<String> = sections.into_iter().collect();
        Err(DomainError::conflict(format!("the account is referred to by {}; it cannot be deleted", names.join(", "))).into())
    }
}

fn delete(ctx: &mut Context, p: DeleteParams) -> Result<DeleteResult, RpcError> {
    refuse_if_residue_mentions(ctx, &p)?;
    ctx.mutate_rpc(|book, _| {
        let guid = p.account.resolve(book)?;
        let opts = DeleteOptions {
            transactions: disposal(book, p.transactions.as_ref())?,
            subaccount: disposal(book, p.subaccount.as_ref())?,
            subaccount_transactions: disposal(book, p.subaccount_transactions.as_ref())?,
            allow_commodity_mismatch: p.allow_commodity_mismatch,
        };
        let r = ops::delete_account(book, guid, &opts)?;
        let ids = |v: Vec<Guid>| v.into_iter().map(Id).collect();
        Ok(DeleteResult {
            deleted_accounts: ids(r.deleted_accounts),
            reparented: r.reparented.map(Id),
            moved_splits: ids(r.moved_splits),
            deleted_transactions: ids(r.deleted_transactions),
            deleted_lots: ids(r.deleted_lots),
        })
    })
}

/// Params of `account.balance`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct BalanceParams {
    /// The account.
    pub account: AccountRef,
    /// Balances at the end of this day (transactions posted before 23:59:59 local time); by default every transaction.
    #[serde(default)]
    pub as_of: Option<Day>,
    /// Add the sub-accounts' balances.
    #[serde(default)]
    pub include_subaccounts: bool,
}

/// Balances in one commodity.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CommodityBalances {
    /// The commodity.
    pub commodity: CommodityId,
    /// Balance of every split.
    pub balance: Amount,
    /// Balance of splits cleared, reconciled, frozen or voided (state other than `n`).
    pub cleared: Amount,
    /// Balance of reconciled and frozen splits.
    pub reconciled: Amount,
}

/// The result of `account.balance`.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BalanceResult {
    /// The account.
    pub account: Id,
    /// The date, if one was given.
    pub as_of: Option<Day>,
    /// Sub-accounts are included.
    pub includes_subaccounts: bool,
    /// The balances, in the account's commodity (amounts, not values).
    pub balances: Option<CommodityBalances>,
    /// Sub-accounts in other commodities, summed by commodity and not converted (GnuCash converts them at the
    /// latest price; red-ale v0.1 leaves that to the caller).
    pub unconverted: Vec<CommodityBalances>,
}

fn commodity_balances(c: red_ale_core::CommodityRef, b: balances::Balances) -> CommodityBalances {
    CommodityBalances {
        commodity: CommodityId(c),
        balance: b.balance.into(),
        cleared: b.cleared.into(),
        reconciled: b.reconciled.into(),
    }
}

fn balance(ctx: &mut Context, p: BalanceParams) -> Result<BalanceResult, RpcError> {
    let zone = ctx.zone();
    let book = ctx.book()?;
    let guid = p.account.resolve(book)?;
    let before = p.as_of.map(|d| zone.day_end(d.0));
    let commodity = book.account(guid).and_then(|a| a.commodity.clone());
    let (total, others) = if p.include_subaccounts {
        balances::balances_with_subaccounts(book, guid, before)?
    } else {
        (balances::balances(book, guid, before)?, std::collections::BTreeMap::new())
    };
    Ok(BalanceResult {
        account: guid.into(),
        as_of: p.as_of,
        includes_subaccounts: p.include_subaccounts,
        balances: commodity.map(|c| commodity_balances(c, total)),
        unconverted: others.into_iter().map(|(c, b)| commodity_balances(c, b)).collect(),
    })
}
