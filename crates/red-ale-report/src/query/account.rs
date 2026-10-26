//! The `Account` handle (`design/reports.md` §5, "Account").

use red_ale_core::ops::accounts::{depth, descendants_sorted, sorted_children, tree_depth, type_label};
use red_ale_core::ops::balances::{Total, account_splits, account_splits_between, balance_as_of, running_totals_at};
use red_ale_core::{Account, Guid};
use rhai::{Array, Dynamic, Engine, INT};

use super::args::{optional_time_arg, time_arg};
use super::book::numeric;
use super::{AccountHandle, Fallible, script_error};
use crate::engine::ContextHandle;
use crate::number::Rational;

pub(super) fn register(engine: &mut Engine) {
    engine.register_type_with_name::<AccountHandle>("Account");
    properties(engine);
    engine.register_fn("==", |a: &mut AccountHandle, b: AccountHandle| a.guid == b.guid);
    engine.register_fn("!=", |a: &mut AccountHandle, b: AccountHandle| a.guid != b.guid);
    engine.register_fn("to_string", |a: &mut AccountHandle| a.snap.book().full_name(a.guid).unwrap_or_default());
    engine.register_fn("to_debug", |a: &mut AccountHandle| format!("Account({})", a.snap.book().full_name(a.guid).unwrap_or_default()));
    engine.register_fn("children", |a: &mut AccountHandle| list(a, a.snap.book().children(a.guid).map(|c| c.guid)));
    engine.register_fn("descendants", |a: &mut AccountHandle| list(a, a.snap.book().descendants(a.guid).iter().map(|c| c.guid)));
    engine.register_fn("children_sorted", |a: &mut AccountHandle| list(a, sorted_children(a.snap.book(), a.guid).iter().map(|c| c.guid)));
    engine.register_fn("descendants_sorted", |a: &mut AccountHandle| {
        list(a, descendants_sorted(a.snap.book(), a.guid).iter().map(|c| c.guid))
    });
    engine.register_fn("tree_depth", |a: &mut AccountHandle| count(tree_depth(a.snap.book(), a.guid)));
    engine.register_fn("splits", |a: &mut AccountHandle| a.book().splits(account_splits(a.snap.book(), a.guid)));
    engine.register_fn("splits_between", |a: &mut AccountHandle, from: Dynamic, to: Dynamic| splits_between(a, &from, &to, false));
    engine.register_fn("splits_between", |a: &mut AccountHandle, from: Dynamic, to: Dynamic, descendants: bool| {
        splits_between(a, &from, &to, descendants)
    });
    engine.register_fn("balance_as_of", |a: &mut AccountHandle, t: Dynamic| -> Fallible<Rational> {
        let total = balance_as_of(a.snap.book(), a.guid, time_arg(&t)?, Total::Balance).map_err(|e| script_error(e.to_string()))?;
        numeric(total)
    });
    engine.register_fn("accumulate_at", accumulate_at);
    engine.register_fn("reverse_balance", |a: &mut AccountHandle, ctx: ContextHandle| -> Fallible<bool> {
        Ok(ctx.settings().reverse_balanced_accounts.reverses(a.get()?.account_type))
    });
}

fn properties(engine: &mut Engine) {
    let text = |f: fn(&Account) -> Option<&str>| {
        move |a: &mut AccountHandle| -> Fallible<String> { Ok(f(a.get()?).unwrap_or("").to_owned()) }
    };
    engine.register_get("guid", |a: &mut AccountHandle| a.guid.to_string());
    engine.register_get("name", |a: &mut AccountHandle| -> Fallible<String> { Ok(a.get()?.name.clone()) });
    engine.register_get("fullName", |a: &mut AccountHandle| a.snap.book().full_name(a.guid).unwrap_or_default());
    engine.register_get("type", |a: &mut AccountHandle| -> Fallible<String> { Ok(a.get()?.account_type.as_str().to_owned()) });
    // `xaccAccountGetTypeStr`, untranslated: the label GnuCash shows for the type ("Credit Card").
    engine.register_get("typeString", |a: &mut AccountHandle| -> Fallible<String> { Ok(type_label(a.get()?.account_type).to_owned()) });
    engine.register_get("code", text(|a| a.code.as_deref()));
    engine.register_get("description", text(|a| a.description.as_deref()));
    engine.register_get("notes", text(Account::notes));
    engine.register_get("placeholder", |a: &mut AccountHandle| -> Fallible<bool> { Ok(a.get()?.is_placeholder()) });
    engine.register_get("hidden", |a: &mut AccountHandle| -> Fallible<bool> { Ok(a.get()?.is_hidden()) });
    engine.register_get("commodity", |a: &mut AccountHandle| -> Fallible<Dynamic> {
        Ok(a.get()?.commodity.as_ref().map_or(Dynamic::UNIT, |c| Dynamic::from(a.book().commodity(c))))
    });
    engine.register_get("parent", |a: &mut AccountHandle| -> Fallible<Dynamic> {
        Ok(a.get()?.parent.map_or(Dynamic::UNIT, |p| Dynamic::from(a.book().account(p))))
    });
    engine.register_get("depth", |a: &mut AccountHandle| count(depth(a.snap.book(), a.guid)));
}

fn list(a: &AccountHandle, guids: impl Iterator<Item = Guid>) -> Array {
    let book = a.book();
    guids.map(|g| Dynamic::from(book.account(g))).collect()
}

fn count(n: usize) -> INT {
    INT::try_from(n).unwrap_or(INT::MAX)
}

/// `gnc-account-foreach-split-between-dates`, both ends inclusive; `()` leaves an end open.
fn splits_between(a: &AccountHandle, from: &Dynamic, to: &Dynamic, descendants: bool) -> Fallible<Array> {
    let found = account_splits_between(a.snap.book(), a.guid, optional_time_arg(from)?, optional_time_arg(to)?, descendants);
    Ok(a.book().splits(found))
}

/// `gnc-account-accumulate-to-dates` (`5.17:bindings/engine.i:196-236`): for each time, the running total `what`
/// (`"balance"`, `"noclosingBalance"`, `"clearedBalance"` or `"reconciledBalance"`) of the last split posted at or
/// before it, `()` before the first split.
fn accumulate_at(a: &mut AccountHandle, times: Array, what: &str) -> Fallible<Array> {
    let total = match what {
        "balance" => Total::Balance,
        "noclosingBalance" => Total::Noclosing,
        "clearedBalance" => Total::Cleared,
        "reconciledBalance" => Total::Reconciled,
        other => return Err(script_error(format!("unknown running total {other:?}"))),
    };
    let times = times.iter().map(time_arg).collect::<Fallible<Vec<_>>>()?;
    let totals = running_totals_at(a.snap.book(), a.guid, &times, total).map_err(|e| script_error(e.to_string()))?;
    totals.into_iter().map(|t| t.map_or(Ok(Dynamic::UNIT), |n| numeric(n).map(Dynamic::from))).collect()
}
