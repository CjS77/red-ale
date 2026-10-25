//! Domain operations: account colours, balance limits, the opening balance account, associated accounts and
//! cascading account properties.
//!
//! The slots are read and written by `Account`'s getters and setters, as GnuCash's engine does; the rules here are
//! those of GnuCash 5.17's account dialogs (`gnucash/gnome-utils/dialog-account.c`) and of the account tree's
//! balance-limit icons (`libgnucash/app-utils/gnc-ui-balances.cpp`). See `design/api/accounts.md`, "Account
//! properties".

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::sync::Arc;

use crate::ops::accounts::{AccountEdit, balances, has_splits};
use crate::ops::business::contacts::set_feature_used;
use crate::{Account, AccountType, Book, CommodityRef, DomainError, DomainResult, Guid, Numeric, Time64};

/// The colour GnuCash's account dialog shows for an account without one, and treats as none
/// (`DEFAULT_COLOR`, `5.17:gnucash/gnome-utils/dialog-account.c:57`).
pub const DEFAULT_COLOR: &str = "rgb(237,236,235)";

/// The Book feature declaring that the opening balance account is marked by its `equity-type` slot
/// (`GNC_FEATURE_EQUITY_TYPE_OPENING_BALANCE`, `5.17:libgnucash/engine/gnc-features.h:64`).
pub const OPENING_BALANCE_FEATURE: &str = "Use a dedicated opening balance account identified by an 'equity-type' slot";

/// The base name GnuCash looks up an opening balance account by (`equity_base_name`,
/// `5.17:libgnucash/app-utils/gnc-ui-util.cpp:547-561`).
pub const OPENING_BALANCES_NAME: &str = "Opening Balances";

// ---------------------------------------------------------------------------------------------------------------
// Colours

fn hex_digits(s: &str) -> Option<Vec<u8>> {
    s.chars().map(|c| c.to_digit(16).and_then(|d| u8::try_from(d).ok())).collect()
}

/// `#rgb` or `#rrggbb` as the three 0–255 components `gdk_rgba_parse` reads them as.
fn parse_hex(hex: &str) -> Option<[u8; 3]> {
    let d = hex_digits(hex)?;
    match d.len() {
        3 => Some([d[0] * 17, d[1] * 17, d[2] * 17]),
        6 => Some([d[0] * 16 + d[1], d[2] * 16 + d[3], d[4] * 16 + d[5]]),
        _ => None,
    }
}

/// `rgb(r,g,b)` with integer components 0–255.
fn parse_rgb(inner: &str) -> Option<[u8; 3]> {
    let parts: Vec<u8> = inner.split(',').map(|p| p.trim().parse::<u8>().ok()).collect::<Option<_>>()?;
    <[u8; 3]>::try_from(parts).ok()
}

/// Read a colour as the account dialog would store it: `#rgb`, `#rrggbb` or `rgb(r,g,b)` become `rgb(r,g,b)`, the
/// form `gdk_rgba_to_string` writes for an opaque colour (`dialog-account.c:507-524`). The empty string and the
/// dialog's default colour mean no colour (`None`). Anything else is refused.
pub fn parse_color(given: &str) -> DomainResult<Option<String>> {
    let text = given.trim();
    if text.is_empty() {
        return Ok(None);
    }
    let rgb = match text.strip_prefix('#') {
        Some(hex) => parse_hex(hex),
        None => text.strip_prefix("rgb(").and_then(|inner| inner.strip_suffix(')')).and_then(parse_rgb),
    };
    let [red, green, blue] = rgb.ok_or_else(|| DomainError::invalid(format!("not a colour: {given:?}; use #rrggbb, #rgb or rgb(r,g,b)")))?;
    let color = format!("rgb({red},{green},{blue})");
    Ok((color != DEFAULT_COLOR).then_some(color))
}

/// Set a colour unless the stored slot already says it.
fn apply_color(acc: &mut Account, color: Option<&str>) {
    if acc.stored_color() != color {
        acc.set_color(color);
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Edits through account.create and account.update

/// Apply the colour, balance limit, opening balance and associated account fields of `edit` to an account already
/// created or edited by `ops::accounts`. `before` is the account before an edit, `None` on create.
pub(crate) fn apply_properties(book: &mut Book, guid: Guid, edit: &AccountEdit, before: Option<&Account>) -> DomainResult<()> {
    let color = edit.color.as_deref().map(parse_color).transpose()?;
    check_associated(book, edit)?;
    let acc = account_mut(book, guid)?;
    if let Some(c) = &color {
        apply_color(acc, c.as_deref());
    }
    apply_limits(acc, edit)?;
    edit.associated_accounts.iter().for_each(|(tag, target)| {
        if acc.associated_accounts().iter().find(|(t, _)| t == tag).map(|(_, g)| *g) != *target {
            acc.set_associated_account(tag, *target);
        }
    });
    apply_opening_balance(book, guid, edit, before)
}

fn account_mut(book: &mut Book, guid: Guid) -> DomainResult<&mut Account> {
    book.accounts.get_mut(&guid).map(Arc::make_mut).ok_or_else(|| DomainError::not_found("account", &guid))
}

fn check_associated(book: &Book, edit: &AccountEdit) -> DomainResult<()> {
    edit.associated_accounts.iter().try_for_each(|(tag, target)| {
        if tag.is_empty() || tag.contains('/') {
            return Err(DomainError::invalid(format!("an associated account tag must be non-empty and without '/': {tag:?}")));
        }
        match target {
            Some(g) if book.account(*g).is_none() => Err(DomainError::not_found("account", g)),
            _ => Ok(()),
        }
    })
}

/// The limit fields, then the dialog's checks on what they leave (`gnc_common_ok`, `dialog-account.c:1029-1056`;
/// `gnc_ui_to_account`, `:589-640`).
fn apply_limits(acc: &mut Account, edit: &AccountEdit) -> DomainResult<()> {
    if let Some(h) = edit.higher_balance_limit.filter(|h| *h != acc.higher_balance_limit()) {
        acc.set_higher_balance_limit(h);
    }
    if let Some(l) = edit.lower_balance_limit.filter(|l| *l != acc.lower_balance_limit()) {
        acc.set_lower_balance_limit(l);
    }
    if let Some(on) = edit.include_subaccount_balances.filter(|on| *on != acc.includes_subaccount_balances()) {
        acc.set_include_subaccount_balances(on);
    }
    let touched = edit.higher_balance_limit.is_some() || edit.lower_balance_limit.is_some() || edit.include_subaccount_balances.is_some();
    if touched && acc.higher_balance_limit().is_none() && acc.lower_balance_limit().is_none() && acc.includes_subaccount_balances() {
        acc.set_include_subaccount_balances(false);
    }
    check_limits(acc.higher_balance_limit(), acc.lower_balance_limit())
}

/// Both limits set: the higher above the lower, or both zero.
pub fn check_limits(higher: Option<Numeric>, lower: Option<Numeric>) -> DomainResult<()> {
    let (Some(h), Some(l)) = (higher, lower) else { return Ok(()) };
    match h.cmp_value(l) {
        Ordering::Equal if !h.is_zero() => Err(DomainError::invalid("balance limits must be different unless they are both zero")),
        Ordering::Less => Err(DomainError::invalid("the lower balance limit must be less than the higher limit")),
        _ => Ok(()),
    }
}

/// The opening balance flag, with the account dialog's rules (`gnc_account_opening_balance_button_update`,
/// `dialog-account.c:245-276`), and the check that a commodity keeps at most one opening balance account.
fn apply_opening_balance(book: &mut Book, guid: Guid, edit: &AccountEdit, before: Option<&Account>) -> DomainResult<()> {
    let current = account(book, guid)?.is_opening_balance();
    if let Some(on) = edit.opening_balance.filter(|on| *on != current) {
        let acc = account(book, guid)?;
        if acc.account_type != AccountType::Equity {
            return Err(DomainError::invalid(format!("only an EQUITY account can hold opening balances; {} is {}", acc.name, acc.account_type)));
        }
        if before.is_some() && has_splits(book, guid) {
            return Err(DomainError::conflict(format!("{} has splits, so whether it holds opening balances cannot change", acc.name)));
        }
        account_mut(book, guid)?.set_opening_balance(on);
        if on {
            set_feature_used(book, OPENING_BALANCE_FEATURE);
        }
    }
    let changed = edit.opening_balance.is_some() || edit.account_type.is_some() || edit.commodity.is_some();
    let acc = account(book, guid)?;
    let unchanged = before.is_some_and(|b| b.is_opening_balance() && b.commodity == acc.commodity);
    if changed
        && acc.is_opening_balance()
        && !unchanged
        && let Some(other) = book.accounts.values().find(|a| a.guid != guid && a.is_opening_balance() && a.commodity == acc.commodity)
    {
        let c = acc.commodity.as_ref().map(ToString::to_string).unwrap_or_default();
        return Err(DomainError::conflict(format!("{} already holds the opening balances in {c}", other.name)));
    }
    Ok(())
}

fn account(book: &Book, guid: Guid) -> DomainResult<&Account> {
    book.account(guid).ok_or_else(|| DomainError::not_found("account", &guid))
}

// ---------------------------------------------------------------------------------------------------------------
// Balance limits

/// Which limit an account's balance has crossed, as the account tree's icon shows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LimitState {
    /// Above the higher limit (`go-top`).
    Above,
    /// Below the lower limit (`go-bottom`).
    Below,
    /// Both limits are zero and the balance is not (`dialog-warning`).
    ShouldBeZero,
}

/// The balance a limit check compares, the limits and the result.
#[derive(Debug, Clone, PartialEq)]
pub struct LimitCheck {
    /// The balance at the cut-off, with sub-accounts in the same commodity when the account's flag says so.
    pub balance: Numeric,
    /// The higher limit.
    pub higher: Option<Numeric>,
    /// The lower limit.
    pub lower: Option<Numeric>,
    /// Sub-accounts are included.
    pub includes_subaccounts: bool,
    /// The limit crossed, if any.
    pub state: Option<LimitState>,
    /// Sub-accounts in other commodities, by commodity: GnuCash converts them at the latest price, red-ale leaves
    /// them out of `balance`.
    pub unconverted: BTreeMap<CommodityRef, Numeric>,
}

/// Check an account's balance, of the splits posted before `before`, against its limits as
/// `get_balance_limit_info` does (`5.17:libgnucash/app-utils/gnc-ui-balances.cpp:254-394`), in Raw sign.
pub fn check_balance_limit(book: &Book, guid: Guid, before: Option<Time64>) -> DomainResult<LimitCheck> {
    let acc = account(book, guid)?;
    let includes_subaccounts = acc.includes_subaccount_balances();
    let (balance, unconverted) = limit_balance(book, acc, includes_subaccounts, before)?;
    let (higher, lower) = (acc.higher_balance_limit(), acc.lower_balance_limit());
    Ok(LimitCheck { balance, higher, lower, includes_subaccounts, state: limit_state(balance, higher, lower), unconverted })
}

/// `account_get_balance_as_of_date` (`gnc-ui-balances.cpp:184-226`): the account's balance plus each descendant's,
/// added exactly.
fn limit_balance(book: &Book, acc: &Account, subaccounts: bool, before: Option<Time64>) -> DomainResult<(Numeric, BTreeMap<CommodityRef, Numeric>)> {
    let own = balances(book, acc.guid, before)?.balance;
    let mut unconverted = BTreeMap::new();
    let subs = if subaccounts { book.descendants(acc.guid) } else { Vec::new() };
    let total = subs.iter().try_fold(own, |total, sub| -> DomainResult<Numeric> {
        let b = balances(book, sub.guid, before)?.balance;
        match &sub.commodity {
            _ if b.is_zero() => Ok(total),
            Some(c) if Some(c) != acc.commodity.as_ref() => {
                let entry = unconverted.entry(c.clone()).or_insert(Numeric::ZERO);
                *entry = entry.add_exact(b)?;
                Ok(total)
            }
            _ => Ok(total.add_exact(b)?),
        }
    })?;
    Ok((total, unconverted))
}

/// `account_balance_limit_reached` (`gnc-ui-balances.cpp:263-277`) and the precedence of `get_balance_limit_info`
/// (`:363-393`), with the higher limit tested first.
pub fn limit_state(balance: Numeric, higher: Option<Numeric>, lower: Option<Numeric>) -> Option<LimitState> {
    let crossed = |limit: Option<Numeric>, side: Ordering| limit.is_some_and(|l| !balance.is_zero() && balance.cmp_value(l) == side);
    let zero = |limit: Option<Numeric>| limit.is_some_and(Numeric::is_zero);
    let (above, below) = (crossed(higher, Ordering::Greater), crossed(lower, Ordering::Less));
    let (higher_zero, lower_zero) = (zero(higher), zero(lower));
    if above && !higher_zero {
        Some(LimitState::Above)
    } else if below && (!lower_zero || !higher_zero) {
        Some(LimitState::Below)
    } else if above && !lower_zero {
        Some(LimitState::Above)
    } else if (above || below) && lower_zero && higher_zero {
        Some(LimitState::ShouldBeZero)
    } else {
        None
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Opening balance account

/// How an opening balance account was found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FoundBy {
    /// Its `equity-type` slot (only with the Book feature).
    EquityType,
    /// The name `Opening Balances`.
    Name,
    /// The name `Opening Balances - <MNEMONIC>`.
    NameWithCurrency,
}

/// True when the book declares the opening balance account feature (`gnc_using_equity_type_opening_balance_account`,
/// `5.17:libgnucash/app-utils/gnc-ui-util.cpp:175-178`).
pub fn uses_opening_balance_feature(book: &Book) -> bool {
    crate::book_options::features(book).iter().any(|(name, _)| name == OPENING_BALANCE_FEATURE)
}

/// Descendants in the order `gnc_account_foreach_descendant_until` visits them: each child, then its descendants
/// (`5.17:libgnucash/engine/Account.cpp:3231-3251`).
fn depth_first(book: &Book, parent: Guid) -> Vec<&Account> {
    book.children(parent).flat_map(|c| std::iter::once(c).chain(depth_first(book, c.guid))).collect()
}

/// The first account named `name` as `gnc_account_lookup_by_name` finds it: the children, then each child's
/// descendants the same way (`account_foreach_descendant_breadthfirst_until`, `Account.cpp:3052-3084`).
fn lookup_by_name<'a>(book: &'a Book, parent: Guid, name: &str) -> Option<&'a Account> {
    book.children(parent).find(|c| c.name == name).or_else(|| book.children(parent).find_map(|c| lookup_by_name(book, c.guid, name)))
}

/// The lookup half of `gnc_find_or_create_equity_account` for opening balances
/// (`5.17:libgnucash/app-utils/gnc-ui-util.cpp:563-622`): by flag when the Book feature is used, then by name. An
/// account found by name counts only if it is EQUITY and in the currency.
pub fn opening_balance_account(book: &Book, currency: &CommodityRef) -> DomainResult<Option<(Guid, FoundBy)>> {
    if !currency.is_currency() {
        return Err(DomainError::invalid(format!("{currency} is not a currency")));
    }
    if uses_opening_balance_feature(book)
        && let Some(a) = depth_first(book, book.root).into_iter().find(|a| a.is_opening_balance() && a.commodity.as_ref() == Some(currency))
    {
        return Ok(Some((a.guid, FoundBy::EquityType)));
    }
    let by_name = |name: &str| {
        lookup_by_name(book, book.root, name).filter(|a| a.account_type == AccountType::Equity && a.commodity.as_ref() == Some(currency))
    };
    let with_currency = format!("{OPENING_BALANCES_NAME} - {}", currency.mnemonic);
    Ok(by_name(OPENING_BALANCES_NAME)
        .map(|a| (a.guid, FoundBy::Name))
        .or_else(|| by_name(&with_currency).map(|a| (a.guid, FoundBy::NameWithCurrency))))
}

// ---------------------------------------------------------------------------------------------------------------
// Cascade

/// What "Cascade Account Properties" sets. `None` leaves a property alone.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[allow(clippy::option_option)] // Not cascading, cascading no colour, and cascading a colour differ.
pub struct Cascade {
    /// The colour, as given (see [`parse_color`]); `Some("")` cascades no colour.
    pub color: Option<String>,
    /// Replace colours already set ("Replace any existing account colors").
    pub replace_colors: bool,
    /// Placeholder flag.
    pub placeholder: Option<bool>,
    /// Hidden flag.
    pub hidden: Option<bool>,
}

/// `gnc_account_cascade_properties_dialog` (`5.17:gnucash/gnome-utils/dialog-account.c:2422-2595`): apply the
/// properties to the account and each descendant. Returns the accounts that changed, the account first, then its
/// descendants depth first.
pub fn cascade(book: &mut Book, guid: Guid, c: &Cascade) -> DomainResult<Vec<Guid>> {
    if guid == book.root {
        return Err(DomainError::invalid("the root account's properties cannot be cascaded"));
    }
    let acc = account(book, guid)?;
    if book.children(guid).next().is_none() {
        return Err(DomainError::invalid(format!("{} has no sub-accounts to cascade to", acc.name)));
    }
    let color = c.color.as_deref().map(parse_color).transpose()?;
    let targets: Vec<Guid> = std::iter::once(guid).chain(book.descendants(guid).iter().map(|a| a.guid)).collect();
    targets.into_iter().try_fold(Vec::new(), |mut changed, g| {
        let acc = account_mut(book, g)?;
        let old = acc.clone();
        if let Some(new) = &color {
            update_account_color(acc, new.as_deref(), c.replace_colors);
        }
        if let Some(on) = c.placeholder.filter(|on| *on != acc.is_placeholder()) {
            acc.set_placeholder(on);
        }
        if let Some(on) = c.hidden.filter(|on| *on != acc.is_hidden()) {
            acc.set_hidden(on);
        }
        if *acc != old {
            changed.push(g);
        }
        Ok(changed)
    })
}

/// `update_account_color` (`dialog-account.c:2388-2407`): a new colour goes on an account without one, or on any
/// account when replacing; no colour removes an existing one only when replacing.
fn update_account_color(acc: &mut Account, new: Option<&str>, replace: bool) {
    let old = acc.color().map(ToOwned::to_owned);
    match new {
        Some(n) if (old.is_none() || replace) && old.as_deref() != Some(n) => acc.set_color(Some(n)),
        None if old.is_some() && replace => acc.set_color(None),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ops::accounts::{create_account, update_account};
    use crate::ops::commit::{NewSplit, NewTransaction, add_transaction};
    use crate::{Date, FixedEnv};

    struct Fx {
        book: Book,
        env: FixedEnv,
    }

    impl Fx {
        fn new() -> Self {
            let mut env = FixedEnv::new(Time64(1_719_835_200), 7);
            Self { book: Book::new_empty(&mut env), env }
        }

        fn add(&mut self, name: &str, t: AccountType, parent: Option<Guid>, commodity: &str) -> Guid {
            let edit = AccountEdit {
                name: Some(name.into()),
                account_type: Some(t),
                commodity: Some(CommodityRef::currency(commodity)),
                ..AccountEdit::default()
            };
            let parent = parent.unwrap_or(self.book.root);
            create_account(&mut self.book, &mut self.env, parent, &edit).unwrap()
        }

        #[allow(clippy::needless_pass_by_value)] // Tests build edits inline.
        fn edit(&mut self, guid: Guid, edit: AccountEdit) -> DomainResult<()> {
            update_account(&mut self.book, guid, &edit)
        }

        fn post(&mut self, debit: Guid, credit: Guid, amount: Numeric) {
            let mut t = NewTransaction::new(CommodityRef::currency("USD"), Date::from_ymd(2024, 6, 1).unwrap());
            let neg = Numeric::new(-amount.num(), amount.denom());
            t.splits = vec![NewSplit::new(debit, amount), NewSplit::new(credit, neg)];
            add_transaction(&mut self.book, &mut self.env, t).unwrap();
        }
    }

    fn n(num: i64, denom: i64) -> Numeric {
        Numeric::new(num, denom)
    }

    #[test]
    fn colours_are_normalised_as_the_dialog_writes_them() {
        assert_eq!(parse_color("#ff8000").unwrap().as_deref(), Some("rgb(255,128,0)"));
        assert_eq!(parse_color("#f80").unwrap().as_deref(), Some("rgb(255,136,0)"));
        assert_eq!(parse_color(" rgb( 1, 2 ,3 ) ").unwrap().as_deref(), Some("rgb(1,2,3)"));
        assert_eq!(parse_color("").unwrap(), None);
        assert_eq!(parse_color("#EDECEB").unwrap(), None, "the dialog's default colour is no colour");
        ["red", "rgb(256,0,0)", "#12345", "rgb(1,2)", "Not Set"].iter().for_each(|bad| assert!(parse_color(bad).is_err(), "{bad}"));
    }

    #[test]
    fn not_set_reads_as_no_colour_and_is_removed_by_a_change() {
        let mut fx = Fx::new();
        let a = fx.add("A", AccountType::Asset, None, "USD");
        Arc::make_mut(fx.book.accounts.get_mut(&a).unwrap()).set_color(Some("Not Set"));
        assert_eq!(fx.book.account(a).unwrap().color(), None);
        fx.edit(a, AccountEdit { color: Some(String::new()), ..AccountEdit::default() }).unwrap();
        assert_eq!(fx.book.account(a).unwrap().stored_color(), None);
    }

    #[test]
    fn limits_follow_the_dialog() {
        let mut fx = Fx::new();
        let a = fx.add("A", AccountType::Bank, None, "USD");
        let limits = |h, l| AccountEdit { higher_balance_limit: Some(h), lower_balance_limit: Some(l), ..AccountEdit::default() };
        assert!(fx.edit(a, limits(Some(n(100, 1)), Some(n(100, 1)))).is_err(), "equal and not zero");
        assert!(fx.edit(a, limits(Some(n(1, 1)), Some(n(2, 1)))).is_err(), "lower above higher");
        fx.edit(a, limits(Some(Numeric::ZERO), Some(Numeric::ZERO))).unwrap();
        fx.edit(a, AccountEdit { include_subaccount_balances: Some(true), ..limits(Some(n(50000, 100)), None) }).unwrap();
        let acc = fx.book.account(a).unwrap();
        assert_eq!((acc.higher_balance_limit(), acc.lower_balance_limit(), acc.includes_subaccount_balances()), (Some(n(50000, 100)), None, true));
        fx.edit(a, AccountEdit { higher_balance_limit: Some(None), ..AccountEdit::default() }).unwrap();
        assert!(!fx.book.account(a).unwrap().includes_subaccount_balances(), "no limits left clears the flag");
    }

    #[test]
    fn limit_states_follow_gnucash_precedence() {
        let (h, l) = (Some(n(100, 1)), Some(n(-50, 1)));
        assert_eq!(limit_state(n(101, 1), h, l), Some(LimitState::Above));
        assert_eq!(limit_state(n(100, 1), h, l), None, "reaching is crossing, not touching");
        assert_eq!(limit_state(n(-51, 1), h, l), Some(LimitState::Below));
        assert_eq!(limit_state(Numeric::ZERO, Some(n(-1, 1)), None), None, "a zero balance never crosses");
        let zero = Some(Numeric::ZERO);
        assert_eq!(limit_state(n(1, 1), zero, zero), Some(LimitState::ShouldBeZero));
        assert_eq!(limit_state(n(-1, 1), zero, zero), Some(LimitState::ShouldBeZero));
        assert_eq!(limit_state(n(1, 1), zero, None), Some(LimitState::Above));
        assert_eq!(limit_state(n(-1, 1), None, zero), Some(LimitState::Below));
    }

    #[test]
    fn the_limit_balance_adds_sub_accounts_in_the_same_commodity() {
        let mut fx = Fx::new();
        let a = fx.add("A", AccountType::Bank, None, "USD");
        let sub = fx.add("Sub", AccountType::Bank, Some(a), "USD");
        let eur = fx.add("Euro", AccountType::Bank, Some(a), "EUR");
        let inc = fx.add("Income", AccountType::Income, None, "USD");
        fx.post(a, inc, n(10000, 100));
        fx.post(sub, inc, n(5000, 100));
        Arc::make_mut(fx.book.accounts.get_mut(&eur).unwrap()).commodity = Some(CommodityRef::currency("EUR"));
        fx.edit(a, AccountEdit { higher_balance_limit: Some(Some(n(120, 1))), ..AccountEdit::default() }).unwrap();
        assert_eq!(check_balance_limit(&fx.book, a, None).unwrap().state, None);
        fx.edit(a, AccountEdit { include_subaccount_balances: Some(true), ..AccountEdit::default() }).unwrap();
        let check = check_balance_limit(&fx.book, a, None).unwrap();
        assert_eq!((check.balance, check.state), (n(15000, 100), Some(LimitState::Above)));
        assert_eq!(check_balance_limit(&fx.book, a, Some(Time64(0))).unwrap().balance, Numeric::ZERO);
    }

    #[test]
    fn one_opening_balance_account_per_currency_and_the_feature() {
        let mut fx = Fx::new();
        let equity = fx.add("Equity", AccountType::Equity, None, "USD");
        let ob = fx.add("Opening Balances", AccountType::Equity, Some(equity), "USD");
        let other = fx.add("Other", AccountType::Equity, Some(equity), "USD");
        let bank = fx.add("Bank", AccountType::Bank, None, "USD");
        let flag = |on| AccountEdit { opening_balance: Some(on), ..AccountEdit::default() };
        assert_eq!(opening_balance_account(&fx.book, &CommodityRef::currency("USD")).unwrap(), Some((ob, FoundBy::Name)));
        assert!(!uses_opening_balance_feature(&fx.book));
        fx.edit(other, flag(true)).unwrap();
        assert!(uses_opening_balance_feature(&fx.book));
        assert_eq!(opening_balance_account(&fx.book, &CommodityRef::currency("USD")).unwrap(), Some((other, FoundBy::EquityType)));
        assert!(matches!(fx.edit(ob, flag(true)), Err(DomainError::Conflict(_))), "a second one in USD");
        assert!(matches!(fx.edit(bank, flag(true)), Err(DomainError::Invalid(_))));
        fx.post(bank, other, n(100, 1));
        assert!(matches!(fx.edit(other, flag(false)), Err(DomainError::Conflict(_))), "it has splits");
        assert!(opening_balance_account(&fx.book, &CommodityRef::new("NASDAQ", "X")).is_err());
    }

    #[test]
    fn opening_balances_found_by_name_with_currency() {
        let mut fx = Fx::new();
        let equity = fx.add("Equity", AccountType::Equity, None, "USD");
        fx.add("Opening Balances", AccountType::Equity, Some(equity), "USD");
        let eur = fx.add("Opening Balances - EUR", AccountType::Equity, Some(equity), "EUR");
        assert_eq!(opening_balance_account(&fx.book, &CommodityRef::currency("EUR")).unwrap(), Some((eur, FoundBy::NameWithCurrency)));
        assert_eq!(opening_balance_account(&fx.book, &CommodityRef::currency("GBP")).unwrap(), None);
    }

    #[test]
    fn associated_accounts_are_set_and_cleared_by_tag() {
        let mut fx = Fx::new();
        let a = fx.add("A", AccountType::Asset, None, "USD");
        let b = fx.add("B", AccountType::Income, None, "USD");
        let set = |tag: &str, g| AccountEdit { associated_accounts: BTreeMap::from([(tag.to_owned(), g)]), ..AccountEdit::default() };
        fx.edit(a, set("stock-dividends", Some(b))).unwrap();
        fx.edit(a, set("stock-capgains", Some(b))).unwrap();
        assert_eq!(fx.book.account(a).unwrap().associated_accounts(), vec![("stock-capgains", b), ("stock-dividends", b)]);
        fx.edit(a, set("stock-capgains", None)).unwrap();
        assert_eq!(fx.book.account(a).unwrap().associated_accounts(), vec![("stock-dividends", b)]);
        assert!(fx.edit(a, set("a/b", Some(b))).is_err());
        assert!(fx.edit(a, set("x", Some(Guid::from_bytes([1; 16])))).is_err());
    }

    #[test]
    fn cascade_keeps_existing_colours_unless_replacing() {
        let mut fx = Fx::new();
        let top = fx.add("Top", AccountType::Asset, None, "USD");
        let red = fx.add("Red", AccountType::Asset, Some(top), "USD");
        let plain = fx.add("Plain", AccountType::Asset, Some(red), "USD");
        fx.edit(red, AccountEdit { color: Some("#ff0000".into()), ..AccountEdit::default() }).unwrap();
        let blue = Cascade { color: Some("#0000ff".into()), placeholder: Some(true), ..Cascade::default() };
        assert_eq!(cascade(&mut fx.book, top, &blue).unwrap(), vec![top, red, plain]);
        let colors = |b: &Book| [top, red, plain].map(|g| b.account(g).unwrap().color().map(ToOwned::to_owned));
        let some = |s: &str| Some(s.to_owned());
        assert_eq!(colors(&fx.book), [some("rgb(0,0,255)"), some("rgb(255,0,0)"), some("rgb(0,0,255)")]);
        assert!(fx.book.account(plain).unwrap().is_placeholder());
        let clear = Cascade { color: Some(String::new()), replace_colors: true, ..Cascade::default() };
        assert_eq!(cascade(&mut fx.book, red, &clear).unwrap(), vec![red, plain]);
        assert_eq!(colors(&fx.book), [some("rgb(0,0,255)"), None, None]);
        assert!(cascade(&mut fx.book, plain, &clear).is_err(), "no sub-accounts");
    }
}
