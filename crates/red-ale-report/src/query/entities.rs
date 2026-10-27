//! The `Split`, `Transaction`, `Commodity` and `Price` handles (`design/reports.md` §5, "Split, Transaction,
//! Commodity, Price").

use std::rc::Rc;

use red_ale_core::ops::balances::{Balances, Total};
use red_ale_core::ops::commodities::{display_symbol, is_iso_namespace, lookup};
use red_ale_core::ops::transactions::find::share_price;
use red_ale_core::transaction::slot_paths;
use red_ale_core::{Commodity, Price, Split, Transaction};
use rhai::{Array, Dynamic, Engine, INT};

use super::book::numeric;
use super::{BookHandle, CommodityHandle, Fallible, PriceHandle, SplitHandle, TransactionHandle, script_error};
use crate::number::Rational;

pub(super) fn register(engine: &mut Engine) {
    split(engine);
    transaction(engine);
    commodity(engine);
    price(engine);
}

fn text(s: Option<&str>) -> String {
    s.unwrap_or("").to_owned()
}

fn split(engine: &mut Engine) {
    engine.register_type_with_name::<SplitHandle>("Split");
    engine.register_fn("==", |a: &mut SplitHandle, b: SplitHandle| a.guid == b.guid);
    engine.register_fn("!=", |a: &mut SplitHandle, b: SplitHandle| a.guid != b.guid);
    engine.register_fn("to_string", |s: &mut SplitHandle| format!("Split({})", s.guid));
    let field = |f: fn(&Transaction, &Split) -> Fallible<Dynamic>| {
        move |s: &mut SplitHandle| -> Fallible<Dynamic> {
            let (t, split) = s.get()?;
            f(t, split)
        }
    };
    engine.register_get("guid", |s: &mut SplitHandle| s.guid.to_string());
    engine.register_get("account", |s: &mut SplitHandle| -> Fallible<Dynamic> {
        let book = BookHandle(Rc::clone(&s.snap));
        Ok(s.get()?.1.account.map_or(Dynamic::UNIT, |a| Dynamic::from(book.account(a))))
    });
    engine.register_get("transaction", |s: &mut SplitHandle| TransactionHandle {
        snap: Rc::clone(&s.snap),
        guid: s.transaction,
    });
    engine.register_get("amount", field(|_, s| numeric(s.amount).map(Dynamic::from)));
    engine.register_get("value", field(|_, s| numeric(s.value).map(Dynamic::from)));
    engine.register_get("memo", field(|_, s| Ok(text(s.memo.as_deref()).into())));
    engine.register_get("action", field(|_, s| Ok(text(s.action.as_deref()).into())));
    engine.register_get("reconcile", field(|_, s| Ok(s.reconcile_state.as_char().to_string().into())));
    engine.register_get(
        "reconcileDate",
        field(|_, s| Ok(s.reconcile_date.map_or(Dynamic::UNIT, |t| Dynamic::from(t.0)))),
    );
    engine.register_get("sharePrice", field(|_, s| numeric(share_price(s)).map(Dynamic::from)));
    engine.register_get("balance", |s: &mut SplitHandle| running(s, Total::Balance));
    engine.register_get("noclosingBalance", |s: &mut SplitHandle| running(s, Total::Noclosing));
}

/// A running total GnuCash keeps on the split (`xaccSplitGetBalance`, `xaccSplitGetNoclosingBalance`,
/// `5.17:libgnucash/engine/Split.cpp:1316-1325`); zero for a split in no account, and for a stock split the engine skips.
fn running(s: &SplitHandle, total: Total) -> Fallible<Rational> {
    let Some(account) = s.get()?.1.account else {
        return Ok(Rational::zero());
    };
    let totals = s.snap.running(account)?;
    numeric(totals.get(&s.guid).copied().unwrap_or(Balances::ZERO).get(total))
}

fn transaction(engine: &mut Engine) {
    engine.register_type_with_name::<TransactionHandle>("Transaction");
    engine.register_fn("==", |a: &mut TransactionHandle, b: TransactionHandle| a.guid == b.guid);
    engine.register_fn("!=", |a: &mut TransactionHandle, b: TransactionHandle| a.guid != b.guid);
    engine.register_fn("to_string", |t: &mut TransactionHandle| format!("Transaction({})", t.guid));
    let field = |f: fn(&Transaction) -> Dynamic| move |t: &mut TransactionHandle| -> Fallible<Dynamic> { Ok(f(t.get()?)) };
    engine.register_get("guid", |t: &mut TransactionHandle| t.guid.to_string());
    engine.register_get("currency", |t: &mut TransactionHandle| -> Fallible<CommodityHandle> {
        Ok(BookHandle(Rc::clone(&t.snap)).commodity(&t.get()?.currency))
    });
    engine.register_get("datePosted", field(|t| Dynamic::from(t.date_posted.0)));
    engine.register_get("dateEntered", field(|t| Dynamic::from(t.date_entered.0)));
    engine.register_get("num", field(|t| text(t.num.as_deref()).into()));
    engine.register_get("description", field(|t| text(t.description.as_deref()).into()));
    engine.register_get("notes", field(|t| text(t.slots.get_str(slot_paths::NOTES)).into()));
    engine.register_get("isClosing", field(|t| Dynamic::from(t.is_closing())));
    engine.register_get("isVoid", field(|t| Dynamic::from(t.is_void())));
    engine.register_get(
        "splitCount",
        field(|t| Dynamic::from(INT::try_from(t.splits.len()).unwrap_or(INT::MAX))),
    );
    engine.register_fn("splits", |t: &mut TransactionHandle| -> Fallible<Array> {
        let book = BookHandle(Rc::clone(&t.snap));
        let txn = t.get()?;
        Ok(book.splits(txn.splits.iter().map(|s| (txn, s))))
    });
}

fn commodity(engine: &mut Engine) {
    engine.register_type_with_name::<CommodityHandle>("Commodity");
    engine.register_fn("==", |a: &mut CommodityHandle, b: CommodityHandle| a.id == b.id);
    engine.register_fn("!=", |a: &mut CommodityHandle, b: CommodityHandle| a.id != b.id);
    engine.register_fn("to_string", |c: &mut CommodityHandle| c.id.to_string());
    let field = |f: fn(&Commodity) -> Dynamic| move |c: &mut CommodityHandle| -> Fallible<Dynamic> { Ok(f(&get_commodity(c)?)) };
    engine.register_get("id", |c: &mut CommodityHandle| c.id.to_string());
    engine.register_get("uniqueName", |c: &mut CommodityHandle| c.id.to_string());
    engine.register_get("namespace", |c: &mut CommodityHandle| c.id.namespace.clone());
    engine.register_get("mnemonic", |c: &mut CommodityHandle| c.id.mnemonic.clone());
    engine.register_get("fullname", field(|c| text(c.fullname.as_deref()).into()));
    engine.register_get("fraction", field(|c| Dynamic::from(c.fraction)));
    engine.register_get("isCurrency", |c: &mut CommodityHandle| is_iso_namespace(&c.id.namespace));
    engine.register_get("symbol", |c: &mut CommodityHandle| -> Fallible<String> {
        display_symbol(c.snap.book(), &c.id).ok_or_else(|| script_error(format!("unknown commodity {}", c.id)))
    });
}

fn get_commodity(c: &CommodityHandle) -> Fallible<Commodity> {
    lookup(c.snap.book(), &c.id)
        .map(std::borrow::Cow::into_owned)
        .ok_or_else(|| script_error(format!("unknown commodity {}", c.id)))
}

fn price(engine: &mut Engine) {
    engine.register_type_with_name::<PriceHandle>("Price");
    engine.register_fn("==", |a: &mut PriceHandle, b: PriceHandle| a.guid == b.guid);
    engine.register_fn("!=", |a: &mut PriceHandle, b: PriceHandle| a.guid != b.guid);
    engine.register_fn("to_string", |p: &mut PriceHandle| format!("Price({})", p.guid));
    let field = |f: fn(&Price) -> Fallible<Dynamic>| move |p: &mut PriceHandle| -> Fallible<Dynamic> { f(p.get()?) };
    let handle = |p: &PriceHandle, id: &red_ale_core::CommodityRef| BookHandle(Rc::clone(&p.snap)).commodity(id);
    engine.register_get("guid", |p: &mut PriceHandle| p.guid.to_string());
    engine.register_get("commodity", move |p: &mut PriceHandle| -> Fallible<CommodityHandle> {
        Ok(handle(p, &p.get()?.commodity))
    });
    engine.register_get("currency", move |p: &mut PriceHandle| -> Fallible<CommodityHandle> {
        Ok(handle(p, &p.get()?.currency))
    });
    engine.register_get("time", field(|p| Ok(Dynamic::from(p.time.0))));
    engine.register_get("source", field(|p| Ok(text(p.source.as_deref()).into())));
    engine.register_get("type", field(|p| Ok(text(p.price_type.as_deref()).into())));
    engine.register_get("value", field(|p| numeric(p.value).map(Dynamic::from)));
}
