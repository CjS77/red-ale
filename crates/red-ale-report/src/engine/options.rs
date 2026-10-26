//! Options as scripts see them (`design/reports.md` §6): the builder `o` a template's `options()` declares them through,
//! mirroring GnuCash's `gnc-register-*-option`, and the values `opts` its `render()` reads.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use red_ale_core::time::{PeriodBounds, RelativeDate};
use red_ale_core::{AccountType, Guid, Time64, TimeZone};
use rhai::{Array, Dynamic, Engine, INT, Map};
use serde_json::Value;

use crate::error::{OptionError, ReportError};
use crate::options::{Choice, ChoiceKey, Classifier, DateUi, DateValue, OptionDb, OptionKind, OptionValue, ReportOption, parse_relative};
use crate::query::{BookHandle, Fallible, account_arg, commodity_arg, rational_arg, script_error};
use crate::result::PlotSize;

/// The option builder handed to a template's `options()` as `o` (Rhai type `OptionBuilder`).
#[derive(Debug, Clone)]
pub struct OptionBuilder {
    db: Rc<RefCell<OptionDb>>,
    book: BookHandle,
}

impl OptionBuilder {
    pub(crate) fn new(book: BookHandle) -> Self {
        Self { db: Rc::new(RefCell::new(OptionDb::new())), book }
    }

    /// The options declared so far.
    pub(crate) fn into_db(self) -> OptionDb {
        self.db.borrow().clone()
    }
}

/// A declaration's error as a script error.
fn declare(declared: Result<(), OptionError>) -> Fallible<()> {
    declared.map_err(|e| script_error(format!("option {}/{}: {}", e.section, e.name, e.reason)))
}

type Text = fn(&mut OptionDb, Classifier, &str) -> Result<(), OptionError>;

/// The resolved option values handed to a template's `render()` as `opts` (Rhai type `Options`).
#[derive(Debug, Clone)]
pub struct OptionsHandle(Rc<BTreeMap<(String, String), (Dynamic, Value)>>);

fn classifier(s: &str, n: &str, k: &str, help: &str) -> Classifier {
    Classifier::new(s, n, k, help)
}

/// `[key, label, help?]` for each choice.
fn choices(items: &Array) -> Fallible<Vec<Choice>> {
    items
        .iter()
        .map(|item| {
            let parts = item.read_lock::<Array>().ok_or_else(|| script_error("a choice is [key, label, help]"))?;
            let text = |i: usize| parts.get(i).map(|p| p.clone().into_string().map_err(|t| script_error(format!("expected a string, got {t}"))));
            let key = choice_key(parts.first().ok_or_else(|| script_error("a choice needs its key"))?)?;
            let label = text(1).ok_or_else(|| script_error("a choice needs its label"))??;
            Ok(Choice::new(key, label, text(2).transpose()?.unwrap_or_default()))
        })
        .collect()
}

fn choice_key(v: &Dynamic) -> Fallible<ChoiceKey> {
    if let Ok(n) = v.as_int() {
        return Ok(ChoiceKey::Number(n));
    }
    v.clone().into_string().map(ChoiceKey::Text).map_err(|t| script_error(format!("a choice key is a string or an integer, got {t}")))
}

fn account_types(v: Option<&Dynamic>) -> Fallible<Vec<AccountType>> {
    let Some(v) = v.filter(|v| !v.is_unit()) else { return Ok(Vec::new()) };
    let items = v.read_lock::<Array>().ok_or_else(|| script_error("types is an array of account type names"))?;
    items
        .iter()
        .map(|t| t.clone().into_string().map_err(|t| script_error(format!("not an account type: {t}")))?.parse().map_err(|e| script_error(format!("{e}"))))
        .collect()
}

fn plot_size(v: &Dynamic) -> Fallible<PlotSize> {
    if let Ok(n) = v.as_int() {
        let n = u32::try_from(n).map_err(|_| script_error(format!("not a plot size: {n}")))?;
        return Ok(if n > 100 { PlotSize::Pixels(n) } else { PlotSize::Percent(n) });
    }
    let map = v.read_lock::<Map>().ok_or_else(|| script_error("a plot size is an integer or #{ percent: n } or #{ pixels: n }"))?;
    let size = |n: &Dynamic| n.as_int().ok().and_then(|n| u32::try_from(n).ok()).ok_or_else(|| script_error("not a plot size"));
    match (map.get("percent"), map.get("pixels"), map.len()) {
        (Some(n), None, 1) => size(n).map(PlotSize::Percent),
        (None, Some(n), 1) => size(n).map(PlotSize::Pixels),
        _ => Err(script_error("a plot size is #{ percent: n } or #{ pixels: n }")),
    }
}

/// A date default: a relative date's name, or a Time64.
fn date_value(v: &Dynamic) -> Fallible<DateValue> {
    if let Ok(t) = v.as_int() {
        return Ok(DateValue::Absolute(Time64(t)));
    }
    let name = v.clone().into_string().map_err(|t| script_error(format!("a date is a relative date's name or a time, got {t}")))?;
    parse_relative(&name).map(DateValue::Relative).ok_or_else(|| script_error(format!("not a relative date: {name:?}")))
}

fn relative_dates(v: Option<&Dynamic>) -> Fallible<Vec<RelativeDate>> {
    let Some(v) = v.filter(|v| !v.is_unit()) else { return Ok(Vec::new()) };
    let items = v.read_lock::<Array>().ok_or_else(|| script_error("relative is an array of relative date names"))?;
    items
        .iter()
        .map(|d| match date_value(d)? {
            DateValue::Relative(r) => Ok(r),
            DateValue::Absolute(_) => Err(script_error("relative lists relative date names")),
        })
        .collect()
}

fn date_ui(v: Option<&Dynamic>) -> Fallible<DateUi> {
    let name = v.map_or(Ok("both".into()), |v| v.clone().into_string().map_err(|t| script_error(format!("ui must be a string, got {t}"))))?;
    match name.as_str() {
        "absolute" => Ok(DateUi::Absolute),
        "relative" => Ok(DateUi::Relative),
        "both" => Ok(DateUi::Both),
        other => Err(script_error(format!("ui must be \"absolute\", \"relative\" or \"both\", got {other:?}"))),
    }
}

fn spec_keys(spec: &Map, known: &[&str]) -> Fallible<()> {
    spec.keys().find(|k| !known.contains(&k.as_str())).map_or(Ok(()), |k| Err(script_error(format!("unknown field {k:?}"))))
}

fn bool_arg(v: Option<&Dynamic>, default: bool) -> Fallible<bool> {
    v.map_or(Ok(default), |v| v.as_bool().map_err(|_| script_error("expected true or false")))
}

pub(super) fn register_builder(engine: &mut Engine) {
    engine.register_type_with_name::<OptionBuilder>("OptionBuilder");
    engine.register_fn("to_string", |_: &mut OptionBuilder| "OptionBuilder".to_owned());
    engine.register_fn("boolean", |o: &mut OptionBuilder, s: &str, n: &str, k: &str, h: &str, d: bool| {
        declare(o.db.borrow_mut().boolean(classifier(s, n, k, h), d))
    });
    let texts: [(&str, Text); 4] = [("string", OptionDb::string), ("text", OptionDb::text), ("font", OptionDb::font), ("color", OptionDb::color)];
    texts.into_iter().for_each(|(name, f)| {
        engine.register_fn(name, move |o: &mut OptionBuilder, s: &str, n: &str, k: &str, h: &str, d: &str| {
            declare(f(&mut o.db.borrow_mut(), classifier(s, n, k, h), d))
        });
    });
    engine.register_fn("multichoice", |o: &mut OptionBuilder, s: &str, n: &str, k: &str, h: &str, d: Dynamic, c: Array| -> Fallible<()> {
        let (d, c) = (choice_key(&d)?, choices(&c)?);
        declare(o.db.borrow_mut().multichoice(classifier(s, n, k, h), d, c))
    });
    engine.register_fn("list", |o: &mut OptionBuilder, s: &str, n: &str, k: &str, h: &str, d: Array, c: Array| -> Fallible<()> {
        let (d, c) = (d.iter().map(choice_key).collect::<Fallible<_>>()?, choices(&c)?);
        declare(o.db.borrow_mut().list(classifier(s, n, k, h), d, c))
    });
    engine.register_fn("number_range", number_range);
    engine.register_fn("plot_size", |o: &mut OptionBuilder, s: &str, n: &str, k: &str, h: &str, d: Dynamic| -> Fallible<()> {
        let d = plot_size(&d)?;
        declare(o.db.borrow_mut().plot_size(classifier(s, n, k, h), d))
    });
    engine.register_fn("date", date);
    engine.register_fn("start_date", |o: &mut OptionBuilder, s: &str, n: &str, k: &str, h: &str, both: bool| {
        declare(o.db.borrow_mut().start_date(classifier(s, n, k, h), both))
    });
    engine.register_fn("end_date", |o: &mut OptionBuilder, s: &str, n: &str, k: &str, h: &str, both: bool| {
        declare(o.db.borrow_mut().end_date(classifier(s, n, k, h), both))
    });
    engine.register_fn("account_list", account_list);
    engine.register_fn("account_sel", account_sel);
    engine.register_fn("currency", |o: &mut OptionBuilder, s: &str, n: &str, k: &str, h: &str, d: Dynamic| -> Fallible<()> {
        let d = commodity_arg(o.book.0.book(), &d)?;
        declare(o.db.borrow_mut().currency(classifier(s, n, k, h), d))
    });
    engine.register_fn("commodity", |o: &mut OptionBuilder, s: &str, n: &str, k: &str, h: &str, d: Dynamic| -> Fallible<()> {
        let d = commodity_arg(o.book.0.book(), &d)?;
        declare(o.db.borrow_mut().commodity(classifier(s, n, k, h), d))
    });
    engine.register_fn("internal", |o: &mut OptionBuilder, s: &str, n: &str, k: &str, h: &str, d: Dynamic| -> Fallible<()> {
        let d = match d.as_bool() {
            Ok(b) => OptionValue::Bool(b),
            Err(_) => OptionValue::Text(d.into_string().map_err(|t| script_error(format!("an internal option is a string or a bool, got {t}")))?),
        };
        declare(o.db.borrow_mut().internal(classifier(s, n, k, h), d))
    });
    engine.register_fn("set_default_section", |o: &mut OptionBuilder, s: &str| o.db.borrow_mut().set_default_section(s));
}

/// `gnc-register-number-range-option`: numbers as Rationals, integers or decimal text.
#[allow(clippy::too_many_arguments)]
fn number_range(o: &mut OptionBuilder, s: &str, n: &str, k: &str, h: &str, d: Dynamic, min: Dynamic, max: Dynamic, step: Dynamic) -> Fallible<()> {
    let [d, min, max, step] = [d, min, max, step].map(|v| rational_arg(&v));
    declare(o.db.borrow_mut().number_range(classifier(s, n, k, h), d?, min?, max?, step?))
}

/// `gnc-register-date-option-set`: the default (a relative date's name or a Time64), then `#{ ui, relative }`. The
/// default comes first, as for the account options, since `default` is a reserved word in Rhai.
fn date(o: &mut OptionBuilder, s: &str, n: &str, k: &str, h: &str, d: Dynamic, spec: Map) -> Fallible<()> {
    spec_keys(&spec, &["ui", "relative"])?;
    let (default, ui, relative) = (date_value(&d)?, date_ui(spec.get("ui"))?, relative_dates(spec.get("relative"))?);
    declare(o.db.borrow_mut().date(classifier(s, n, k, h), default, ui, relative))
}

/// `gnc-register-account-list-option`: the default accounts, then `#{ types, multiple }`.
fn account_list(o: &mut OptionBuilder, s: &str, n: &str, k: &str, h: &str, d: Array, spec: Map) -> Fallible<()> {
    spec_keys(&spec, &["types", "multiple"])?;
    let default = d.iter().map(|a| account_arg(o.book.0.book(), a)).collect::<Fallible<Vec<Guid>>>()?;
    let (types, multiple) = (account_types(spec.get("types"))?, bool_arg(spec.get("multiple"), true)?);
    declare(o.db.borrow_mut().account_list(classifier(s, n, k, h), default, types, multiple))
}

/// `gnc-register-account-sel-limited-option`: the default account or `()`, then `#{ types }`.
fn account_sel(o: &mut OptionBuilder, s: &str, n: &str, k: &str, h: &str, d: Dynamic, spec: Map) -> Fallible<()> {
    spec_keys(&spec, &["types"])?;
    let default = if d.is_unit() { None } else { Some(account_arg(o.book.0.book(), &d)?) };
    let types = account_types(spec.get("types"))?;
    declare(o.db.borrow_mut().account_sel(classifier(s, n, k, h), default, types))
}

pub(super) fn register_reader(engine: &mut Engine) {
    engine.register_type_with_name::<OptionsHandle>("Options");
    engine.register_fn("to_string", |_: &mut OptionsHandle| "Options".to_owned());
    engine.register_fn("get", |o: &mut OptionsHandle, s: &str, n: &str| -> Fallible<Dynamic> { o.lookup(s, n).map(|(v, _)| v.clone()) });
    engine.register_fn("raw", |o: &mut OptionsHandle, s: &str, n: &str| -> Fallible<Dynamic> {
        rhai::serde::to_dynamic(o.lookup(s, n)?.1.clone())
    });
}

impl OptionsHandle {
    fn lookup(&self, section: &str, name: &str) -> Fallible<&(Dynamic, Value)> {
        self.0.get(&(section.to_owned(), name.to_owned())).ok_or_else(|| script_error(format!("unknown option {section}/{name}")))
    }
}

/// What resolving option values needs.
pub(crate) struct Resolve<'a> {
    pub book: &'a BookHandle,
    pub now: Time64,
    pub zone: &'a TimeZone,
    pub period: &'a Result<PeriodBounds, String>,
}

/// Every option's value as `render()` reads it: dates turned into Time64 as `gnc:date-option-absolute-time` does
/// (`5.17:bindings/guile/options.scm:335-340`), accounts and commodities into handles, numbers into Rationals.
pub(crate) fn resolve(db: &OptionDb, r: &Resolve<'_>) -> Result<OptionsHandle, ReportError> {
    let wire = db.values();
    let values = db
        .options()
        .into_iter()
        .map(|o| {
            let raw = wire.get(o.section()).and_then(|s| s.get(o.name())).cloned().unwrap_or(Value::Null);
            Ok(((o.section().to_owned(), o.name().to_owned()), (value(o, r)?, raw)))
        })
        .collect::<Result<BTreeMap<_, _>, ReportError>>()?;
    Ok(OptionsHandle(Rc::new(values)))
}

fn value(o: &ReportOption, r: &Resolve<'_>) -> Result<Dynamic, ReportError> {
    let fail = |reason: String| ReportError::from(OptionError::new(o.section(), o.name(), reason));
    let key = |k: &ChoiceKey| match k {
        ChoiceKey::Text(s) => Dynamic::from(s.clone()),
        ChoiceKey::Number(n) => Dynamic::from(*n),
    };
    let accounts = || o.accounts(r.book.0.book()).into_iter().map(|g| Dynamic::from(r.book.account(g)));
    Ok(match (o.value(), &o.kind) {
        (_, OptionKind::AccountList { .. }) => Dynamic::from(accounts().collect::<Array>()),
        (_, OptionKind::AccountSel { .. }) => accounts().next().unwrap_or(Dynamic::UNIT),
        (OptionValue::Bool(b), _) => Dynamic::from(*b),
        (OptionValue::Text(s), _) => Dynamic::from(s.clone()),
        (OptionValue::Key(k), _) => key(k),
        (OptionValue::Keys(keys), _) => Dynamic::from(keys.iter().map(key).collect::<Array>()),
        (OptionValue::Number(n), _) => Dynamic::from(n.clone()),
        (OptionValue::PlotSize(p), _) => plot_size_value(*p),
        (OptionValue::Date(d), _) => Dynamic::from(date_time(*d, r).map_err(fail)?.0),
        (OptionValue::Commodity(c), _) => Dynamic::from(r.book.commodity(c)),
        (OptionValue::Accounts(_) | OptionValue::Account(_), _) => Dynamic::UNIT,
    })
}

fn plot_size_value(p: PlotSize) -> Dynamic {
    let (unit, value) = match p {
        PlotSize::Percent(n) => ("percent", n),
        PlotSize::Pixels(n) => ("pixels", n),
    };
    let map: Map = [("unit".into(), Dynamic::from(unit.to_owned())), ("value".into(), Dynamic::from(INT::from(value)))].into_iter().collect();
    Dynamic::from(map)
}

/// The instant a date value names at the run's now. Only the accounting-period dates need the accounting period.
fn date_time(d: DateValue, r: &Resolve<'_>) -> Result<Time64, String> {
    let needs_period = matches!(d, DateValue::Relative(RelativeDate::StartAccountingPeriod | RelativeDate::EndAccountingPeriod));
    let period = match r.period {
        Ok(p) => *p,
        Err(e) if needs_period => return Err(format!("the accounting period: {e}")),
        Err(_) => PeriodBounds { start: r.now, end: r.now },
    };
    d.resolve(r.now, r.zone, period).ok_or_else(|| "the date is outside the supported range".to_owned())
}
