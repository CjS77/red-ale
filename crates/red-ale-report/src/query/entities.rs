//! The `Split`, `Transaction`, `Commodity`, `Price` and `Lot` handles (`design/reports.md` §5, "Split, Transaction,
//! Commodity, Price").

use std::rc::Rc;

use red_ale_core::ops::balances::{Balances, Total};
use red_ale_core::book_options::uses_split_action_for_num;
use red_ale_core::lot::is_stock_split;
use red_ale_core::ops::business::payments::lot_invoice;
use red_ale_core::ops::lots::lot_split_list;
use red_ale_core::ops::commodities::{display_symbol, is_iso_namespace, lookup};
use red_ale_core::ops::doc_links::doc_link;
use red_ale_core::ops::transactions::find::share_price;
use red_ale_core::ops::transactions::void::{void_former_amount, void_former_value};
use red_ale_core::ops::transactions::order::trans_order;
use red_ale_core::ops::transactions::{corr_account_code, corr_account_full_name, other_split};
use red_ale_core::transaction::slot_paths;
use red_ale_core::{Commodity, Price, Split, Transaction};
use rhai::{Array, Dynamic, Engine, INT, Map};

use super::book::numeric;
use super::{AccountHandle, BookHandle, CommodityHandle, Fallible, LotHandle, PriceHandle, SplitHandle, TransactionHandle, script_error};
use crate::number::Rational;

pub(super) fn register(engine: &mut Engine) {
    split(engine);
    transaction(engine);
    commodity(engine);
    price(engine);
    lot(engine);
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
    engine.register_get("voidFormerAmount", field(|_, s| numeric(void_former_amount(s)).map(Dynamic::from)));
    engine.register_get("voidFormerValue", field(|_, s| numeric(void_former_value(s)).map(Dynamic::from)));
    // `xaccSplitGetType` is "stock-split" (`5.17:libgnucash/engine/Split.cpp:2039-2058`).
    engine.register_get("isStockSplit", field(|_, s| Ok(Dynamic::from(is_stock_split(s)))));
    // `gnc_get_action_num` (`5.17:libgnucash/engine/engine-helpers.c:85-102`): the split's action, or the
    // transaction's num when the book uses split actions for nums.
    engine.register_get("actionNum", |s: &mut SplitHandle| -> Fallible<String> {
        let (t, split) = s.get()?;
        Ok(if uses_split_action_for_num(s.snap.book()) {
            text(t.num.as_deref())
        } else {
            text(split.action.as_deref())
        })
    });
    split_relations(engine);
}

/// The reads of a split that look past it: at its transaction's other splits, its account's book or its lot.
fn split_relations(engine: &mut Engine) {
    engine.register_get("otherSplit", |s: &mut SplitHandle| -> Fallible<Dynamic> {
        let (t, split) = s.get()?;
        let other = other_split(s.snap.book(), t, split.guid);
        Ok(other.map_or(Dynamic::UNIT, |o| Dynamic::from(BookHandle(Rc::clone(&s.snap)).split(t, o))))
    });
    engine.register_get("corrAccountFullName", |s: &mut SplitHandle| -> Fallible<String> {
        let (t, split) = s.get()?;
        Ok(corr_account_full_name(s.snap.book(), t, split.guid))
    });
    engine.register_get("corrAccountCode", |s: &mut SplitHandle| -> Fallible<String> {
        let (t, split) = s.get()?;
        Ok(corr_account_code(s.snap.book(), t, split.guid))
    });
    engine.register_get("lot", |s: &mut SplitHandle| -> Fallible<Dynamic> {
        let lot = s.get()?.1.lot.filter(|g| s.snap.book().lot(*g).is_some());
        Ok(lot.map_or(Dynamic::UNIT, |guid| {
            Dynamic::from(LotHandle {
                snap: Rc::clone(&s.snap),
                guid,
            })
        }))
    });
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
    engine.register_get("docLink", field(|t| text(doc_link(&t.slots)).into()));
    engine.register_get(
        "splitCount",
        field(|t| Dynamic::from(INT::try_from(t.splits.len()).unwrap_or(INT::MAX))),
    );
    // `xaccTransOrder` (`5.17:libgnucash/engine/Transaction.cpp:1757-1761`, `:1794-1838`): -1, 0 or 1.
    engine.register_fn("order", |a: &mut TransactionHandle, b: TransactionHandle| -> Fallible<INT> {
        Ok(match trans_order(a.get()?, b.get()?) {
            std::cmp::Ordering::Less => -1,
            std::cmp::Ordering::Equal => 0,
            std::cmp::Ordering::Greater => 1,
        })
    });
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

fn lot(engine: &mut Engine) {
    engine.register_type_with_name::<LotHandle>("Lot");
    engine.register_fn("==", |a: &mut LotHandle, b: LotHandle| a.guid == b.guid);
    engine.register_fn("!=", |a: &mut LotHandle, b: LotHandle| a.guid != b.guid);
    engine.register_fn("to_string", |l: &mut LotHandle| format!("Lot({})", l.guid));
    engine.register_get("guid", |l: &mut LotHandle| l.guid.to_string());
    // `gncInvoiceGetInvoiceFromLot`: the invoice the lot's `gncInvoice/invoice-guid` slot names.
    engine.register_get("invoice", |l: &mut LotHandle| -> Dynamic {
        lot_invoice(l.snap.book(), l.guid).map_or(Dynamic::UNIT, |i| {
            let map: Map = [("guid".into(), i.guid.to_string().into()), ("id".into(), i.id.clone().into())]
                .into_iter()
                .collect();
            Dynamic::from(map)
        })
    });
    let optional = |s: Option<&str>| s.map_or(Dynamic::UNIT, Into::into);
    // `gnc-lot-get-title`, `gnc-lot-get-notes` (`5.17:libgnucash/engine/gnc-lot.cpp:444-460`): `()` when unset.
    engine.register_get("title", move |l: &mut LotHandle| -> Fallible<Dynamic> { Ok(optional(l.get()?.title())) });
    engine.register_get("notes", move |l: &mut LotHandle| -> Fallible<Dynamic> { Ok(optional(l.get()?.notes())) });
    engine.register_get("account", |l: &mut LotHandle| -> Fallible<AccountHandle> {
        Ok(BookHandle(Rc::clone(&l.snap)).account(l.get()?.account))
    });
    engine.register_fn("splits", |l: &mut LotHandle| {
        BookHandle(Rc::clone(&l.snap)).splits(lot_split_list(l.snap.book(), l.guid))
    });
}
