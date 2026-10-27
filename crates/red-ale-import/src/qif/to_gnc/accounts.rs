//! `qif-import:find-or-make-acct`: the account a QIF map entry names, made if needed, with its parents.
//! `5.17:gnucash/import-export/qif-imp/qif-to-gnc.scm:56-240`.
//!
//! GnuCash builds the import's accounts in a tree of their own: an account the Book already has, if compatible, is
//! copied there, and an incompatible one gets a new account named `<name> 2`. When the import is applied, the new
//! tree is joined to the Book's and `gnc_account_merge_children` merges each new account into a sibling with the
//! same name, code, description, colour, commodity, notes and type (`5.17:libgnucash/engine/Account.cpp:5970-6040`),
//! so a copy becomes the account it was copied from. red-ale decides that merge as each account is made: an
//! account's parent is known by then, so the account is either an existing sibling it would merge into, or a new
//! account. The import's own balance of each account it made (which a stock split reads) is kept per account made,
//! as GnuCash keeps it per account in the new tree.
//!
//! **Deviations.** GnuCash's merge also merges two accounts the Book already had if they are alike; red-ale merges
//! only the import's. Where GnuCash would leave two siblings of the same name (a renamed `<name> 2` unlike an existing
//! `<name> 2`), red-ale names the new one `<name> <n>` with the first free `n`: a Book keeps one account per full name.

use red_ale_core::ops::accounts::{AccountEdit, create_account};
use red_ale_core::{Account, AccountType, Book, CommodityRef, DomainError, DomainResult, Env, Guid, Numeric};

use super::super::dialog_utils::SEP;
use super::super::objects::QifMapEntry;
use super::super::string::string_rcontains;

/// The description of an account renamed for a conflict. `qif-to-gnc.scm:211-213`.
pub const NAME_CONFLICT: &str = "QIF import: Name conflict with another account.";

/// An account of the import's tree: the Book's account it became, and the import's balance in it so far.
#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    /// The account in the Book: existing, or made by the import.
    pub account: Guid,
    /// The sum of the amounts of the splits the import put in this account of its tree.
    pub balance: Numeric,
}

/// The accounts the import made, and `gnc-acct-hash`: full name → the account last made for it.
#[derive(Debug, Clone, Default)]
pub struct Tree {
    /// Every account made, in order.
    pub nodes: Vec<Node>,
    /// `gnc-acct-hash`.
    pub by_name: std::collections::BTreeMap<String, usize>,
}

/// The fields `gnc_account_merge_children` compares, and those an account is made with.
#[derive(Debug, Clone, PartialEq)]
struct Attrs {
    name: String,
    account_type: AccountType,
    commodity: Option<CommodityRef>,
    description: Option<String>,
    notes: Option<String>,
    code: Option<String>,
    color: Option<String>,
}

impl Attrs {
    /// The properties a copy takes from an existing account. `qif-to-gnc.scm:167-182`.
    fn copy_of(a: &Account) -> Self {
        Self {
            name: a.name.clone(),
            account_type: a.account_type,
            commodity: a.commodity.clone(),
            description: a.description.clone(),
            notes: a.notes().map(str::to_owned),
            code: a.code.clone(),
            color: a.color().map(str::to_owned),
        }
    }

    /// Whether an account would absorb one made with these attributes (`null_strcmp` takes a missing text as empty).
    fn merges_into(&self, a: &Account) -> bool {
        let text = |s: Option<&str>| s.unwrap_or_default().to_owned();
        a.name == self.name
            && text(a.code.as_deref()) == text(self.code.as_deref())
            && text(a.description.as_deref()) == text(self.description.as_deref())
            && text(a.color()) == text(self.color.as_deref())
            && a.commodity == self.commodity
            && text(a.notes()) == text(self.notes.as_deref())
            && a.account_type == self.account_type
    }
}

/// What `find-or-make-acct` needs besides the entry.
pub struct Maker<'a> {
    /// The Book being imported into.
    pub book: &'a mut Book,
    /// GUIDs.
    pub env: &'a mut dyn Env,
    /// The Book as it was before the import: GnuCash's `old-root`.
    pub old: &'a Book,
    /// The import's accounts.
    pub tree: &'a mut Tree,
    /// The default currency.
    pub default_currency: &'a CommodityRef,
}

/// How to look for an account.
#[derive(Debug, Clone, Copy)]
pub struct Check<'c> {
    /// `check-types?`: an account must have one of the entry's allowed types.
    pub types: bool,
    /// The commodity a new account gets.
    pub commodity: &'c CommodityRef,
    /// `check-commodity?`: an account must hold that commodity.
    pub commodity_matters: bool,
}

/// Whether an account passes the check for these allowed types.
fn compatible(a: &Account, allowed: &[AccountType], check: Check<'_>) -> bool {
    (!check.types || allowed.contains(&a.account_type)) && (!check.commodity_matters || a.commodity.as_ref() == Some(check.commodity))
}

/// The numbers to try in `<name> <n>`: from 2, as many as there are accounts, so one is always free.
fn variant_numbers(book: &Book) -> std::ops::RangeInclusive<usize> {
    2..=book.accounts.len() + 2
}

impl Maker<'_> {
    /// `qif-import:find-or-make-acct`: the import's account for the entry's full name. `qif-to-gnc.scm:62-240`.
    pub fn find_or_make_acct(&mut self, acct_info: &mut QifMapEntry, check: Check<'_>) -> DomainResult<usize> {
        let gnc_name = acct_info.gnc_name.clone().unwrap_or_default();
        let same = self.old.account_by_full_name(&gnc_name).cloned();
        let allowed = acct_info.allowed_types.clone();
        let incompatible = same.as_ref().is_some_and(|s| !compatible(s, &allowed, check));
        let existing = self.tree.by_name.get(&gnc_name).copied();
        if let Some(node) = existing.filter(|n| self.node_account(*n).is_some_and(|a| compatible(a, &allowed, check))) {
            return Ok(node);
        }
        let last_sep = string_rcontains(&gnc_name, SEP);
        let acct_name = last_sep.map_or(gnc_name.as_str(), |i| &gnc_name[i + SEP.len()..]);
        let mut attrs = Self::attrs(acct_info, same.as_ref(), incompatible, acct_name, check)?;
        if incompatible {
            attrs.name = self.unique_name_variant(&gnc_name, acct_name, &allowed, check);
            attrs.description = Some(NAME_CONFLICT.to_owned());
        }
        let parent = match last_sep {
            Some(i) => {
                let parent_name = &gnc_name[..i];
                let mut pinfo = QifMapEntry {
                    qif_name: Some(parent_name.to_owned()),
                    gnc_name: Some(parent_name.to_owned()),
                    ..QifMapEntry::default()
                };
                acct_info.allowed_types = vec![attrs.account_type];
                pinfo.allowed_types = acct_info.allowed_parent_types();
                let currency = self.default_currency.clone();
                let node = self.find_or_make_acct(
                    &mut pinfo,
                    Check {
                        types: true,
                        commodity: &currency,
                        commodity_matters: false,
                    },
                )?;
                self.tree.nodes.get(node).map(|n| n.account)
            }
            None => None,
        };
        let account = self.attach(parent.unwrap_or(self.book.root), attrs)?;
        let node = self.tree.nodes.len();
        self.tree.nodes.push(Node {
            account,
            balance: Numeric::ZERO,
        });
        self.tree.by_name.insert(gnc_name, node);
        Ok(node)
    }

    fn node_account(&self, node: usize) -> Option<&Account> {
        self.tree.nodes.get(node).and_then(|n| self.book.account(n.account))
    }

    /// The new account's attributes: a copy of a compatible existing account, else the entry's name, description,
    /// commodity and default type over whatever the existing account gave. `qif-to-gnc.scm:162-219`.
    fn attrs(info: &QifMapEntry, same: Option<&Account>, incompatible: bool, acct_name: &str, check: Check<'_>) -> DomainResult<Attrs> {
        let mut attrs = same.map_or_else(
            || Attrs {
                name: String::new(),
                account_type: AccountType::Bank,
                commodity: None,
                description: None,
                notes: None,
                code: None,
                color: None,
            },
            Attrs::copy_of,
        );
        if same.is_some() && !incompatible {
            return Ok(attrs);
        }
        acct_name.clone_into(&mut attrs.name);
        if info.description.is_some() {
            attrs.description.clone_from(&info.description);
        }
        attrs.commodity = Some(check.commodity.clone());
        attrs.account_type = default_account_type(&info.allowed_types, check.commodity.is_currency())
            .ok_or_else(|| DomainError::invalid(format!("no valid account types allowed for account {acct_name:?}")))?;
        Ok(attrs)
    }

    /// `make-unique-name-variant`: `<short> <n>` for the first `n` from 2 whose `<long> <n>` is free or compatible
    /// in the Book as it was. `qif-to-gnc.scm:87-97`.
    fn unique_name_variant(&self, long_name: &str, short_name: &str, allowed: &[AccountType], check: Check<'_>) -> String {
        if self.old.account_by_full_name(long_name).is_none() {
            return short_name.to_owned();
        }
        let free = variant_numbers(self.old)
            .find(|count| {
                self.old
                    .account_by_full_name(&format!("{long_name} {count}"))
                    .is_none_or(|a| compatible(a, allowed, check))
            })
            .unwrap_or(2);
        format!("{short_name} {free}")
    }

    /// Put the account under its parent as the merge would leave it: an alike sibling, or a new account (renamed
    /// when a sibling has its name, red-ale's deviation).
    fn attach(&mut self, parent: Guid, mut attrs: Attrs) -> DomainResult<Guid> {
        if let Some(alike) = self.book.children(parent).find(|a| attrs.merges_into(a)) {
            return Ok(alike.guid);
        }
        if self.book.children(parent).any(|a| a.name == attrs.name) {
            let taken = |name: &str| self.book.children(parent).any(|a| a.name == name);
            let base = attrs.name.clone();
            attrs.name = variant_numbers(self.book)
                .map(|n| format!("{base} {n}"))
                .find(|n| !taken(n))
                .unwrap_or(base);
        }
        let edit = AccountEdit {
            name: Some(attrs.name),
            account_type: Some(attrs.account_type),
            commodity: attrs.commodity,
            description: attrs.description,
            notes: attrs.notes,
            code: attrs.code,
            color: attrs.color,
            ..AccountEdit::default()
        };
        create_account(self.book, self.env, parent, &edit)
    }
}

/// `default-account-type`: the first allowed type that suits the commodity: a security type for a security, any
/// other for a currency. `None` is GnuCash's `'bug` throw. `qif-to-gnc.scm:137-158`.
pub fn default_account_type(allowed: &[AccountType], currency: bool) -> Option<AccountType> {
    allowed
        .iter()
        .copied()
        .find(|t| matches!(t, AccountType::Stock | AccountType::Mutual) != currency)
}

#[cfg(test)]
mod tests {
    use red_ale_core::{FixedEnv, Time64};

    use super::*;

    fn usd() -> CommodityRef {
        CommodityRef::currency("USD")
    }

    fn entry(gnc_name: &str, types: &[AccountType]) -> QifMapEntry {
        QifMapEntry {
            qif_name: Some(gnc_name.into()),
            gnc_name: Some(gnc_name.into()),
            allowed_types: types.to_vec(),
            ..QifMapEntry::default()
        }
    }

    fn book_with(env: &mut FixedEnv, accounts: &[(&str, AccountType)]) -> Book {
        let mut book = Book::new_empty(env);
        accounts.iter().for_each(|(name, t)| {
            let edit = AccountEdit {
                name: Some((*name).into()),
                account_type: Some(*t),
                commodity: Some(usd()),
                ..AccountEdit::default()
            };
            let root = book.root;
            create_account(&mut book, env, root, &edit).unwrap();
        });
        book
    }

    #[test]
    fn default_types_suit_the_commodity() {
        use AccountType as T;
        assert_eq!(default_account_type(&[T::Stock, T::Bank], true), Some(T::Bank));
        assert_eq!(default_account_type(&[T::Bank, T::Mutual], false), Some(T::Mutual));
        assert_eq!(default_account_type(&[T::Stock], true), None);
    }

    #[test]
    fn compatible_accounts_are_used_and_parents_made() {
        let mut env = FixedEnv::new(Time64(0), 1);
        let mut book = book_with(&mut env, &[("Expenses", AccountType::Expense)]);
        let old = book.clone();
        let mut tree = Tree::default();
        let currency = usd();
        let mut maker = Maker {
            book: &mut book,
            env: &mut env,
            old: &old,
            tree: &mut tree,
            default_currency: &currency,
        };
        let check = Check {
            types: false,
            commodity: &currency,
            commodity_matters: true,
        };
        let mut food = entry("Expenses:Food", &[AccountType::Expense, AccountType::Income]);
        let node = maker.find_or_make_acct(&mut food, check).unwrap();
        // The entry's types become the type made (`:227-228`).
        assert_eq!(food.allowed_types, [AccountType::Expense]);
        let account = tree.nodes[node].account;
        assert_eq!(book.full_name(account).as_deref(), Some("Expenses:Food"));
        assert_eq!(book.account(account).unwrap().account_type, AccountType::Expense);
        assert_eq!(book.accounts.len(), old.accounts.len() + 1, "the existing parent was used");
    }

    #[test]
    fn an_incompatible_account_gets_a_new_name() {
        let mut env = FixedEnv::new(Time64(0), 1);
        let mut book = book_with(&mut env, &[("Checking", AccountType::Income)]);
        let old = book.clone();
        let mut tree = Tree::default();
        let currency = usd();
        let mut maker = Maker {
            book: &mut book,
            env: &mut env,
            old: &old,
            tree: &mut tree,
            default_currency: &currency,
        };
        let check = Check {
            types: true,
            commodity: &currency,
            commodity_matters: true,
        };
        let node = maker
            .find_or_make_acct(&mut entry("Checking", &[AccountType::Bank]), check)
            .unwrap();
        let made = book.account(tree.nodes[node].account).unwrap();
        assert_eq!(
            (made.name.as_str(), made.account_type, made.description.as_deref()),
            ("Checking 2", AccountType::Bank, Some(NAME_CONFLICT))
        );
    }
}
