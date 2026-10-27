//! Option values on the wire (`design/reports.md` §6, the type table): reading a value a client sent, checking it
//! against its option, and writing a value back.

use red_ale_core::ops::commodities::lookup;
use red_ale_core::time::RelativeDate;
use red_ale_core::{AccountType, Book, CommodityRef, Date, Guid, Time64, TimeZone};
use serde_json::{Map, Value, json};

use super::model::{
    Choice, ChoiceKey, DateUi, DateValue, MAX_TIME, MIN_TIME, OptionKind, OptionValue, PLOT_SIZE_MAX, PLOT_SIZE_MIN, PLOT_SIZE_PERCENT_MAX,
    parse_relative, relative_name,
};
use crate::number::Rational;
use crate::result::PlotSize;
use crate::wire::{Timestamp, parse_commodity};

/// What reading an option value needs: the open Book, to resolve accounts and commodities, and the local time zone,
/// to place an absolute date.
#[derive(Debug, Clone, Copy)]
pub struct ValueContext<'a> {
    /// The open Book. Without one, accounts can be named only by GUID and are not checked, and commodities are not
    /// looked up.
    pub book: Option<&'a Book>,
    /// The local time zone.
    pub zone: &'a TimeZone,
}

type Read<T> = Result<T, String>;

/// Read `json` as a value of an option of `kind` whose default is `default`.
pub(super) fn read(kind: &OptionKind, default: &OptionValue, json: &Value, ctx: &ValueContext<'_>) -> Read<OptionValue> {
    match kind {
        OptionKind::Boolean => boolean(json).map(OptionValue::Bool),
        OptionKind::String | OptionKind::Text | OptionKind::Font => string(json).map(|s| OptionValue::Text(s.to_owned())),
        OptionKind::Color => color(json).map(OptionValue::Text),
        OptionKind::Internal => match default {
            OptionValue::Bool(_) => boolean(json).map(OptionValue::Bool),
            _ => string(json).map(|s| OptionValue::Text(s.to_owned())),
        },
        OptionKind::Multichoice { choices } => choice(choices, json).map(OptionValue::Key),
        OptionKind::List { choices } => list(choices, default, json),
        OptionKind::NumberRange { min, max, .. } => number(json, min, max).map(OptionValue::Number),
        OptionKind::PlotSize => plot_size(json).map(OptionValue::PlotSize),
        OptionKind::Date { ui, relative } => date(json, *ui, relative, ctx).map(OptionValue::Date),
        OptionKind::AccountList { types, multiple } => account_list(json, types, *multiple, default, ctx),
        OptionKind::AccountSel { types } => string(json)
            .and_then(|s| account(s, types, ctx))
            .map(|g| OptionValue::Account(Some(g))),
        OptionKind::Currency => commodity(json, true, ctx).map(OptionValue::Commodity),
        OptionKind::Commodity => commodity(json, false, ctx).map(OptionValue::Commodity),
        OptionKind::Budget => string(json).and_then(|s| budget(s, ctx)).map(|g| OptionValue::Budget(Some(g))),
    }
}

/// The wire form of a value.
pub(super) fn write(value: &OptionValue) -> Value {
    match value {
        OptionValue::Bool(b) => Value::Bool(*b),
        OptionValue::Text(s) => Value::String(s.clone()),
        OptionValue::Key(k) => key_json(k),
        OptionValue::Keys(keys) => Value::Array(keys.iter().map(key_json).collect()),
        OptionValue::Number(n) => number_json(n),
        OptionValue::PlotSize(p) => serde_json::to_value(p).unwrap_or(Value::Null),
        OptionValue::Date(DateValue::Absolute(t)) => json!({ "absolute": Timestamp(*t).to_rfc3339() }),
        OptionValue::Date(DateValue::Relative(r)) => json!({ "relative": relative_name(*r) }),
        OptionValue::Accounts(guids) => Value::Array(guids.iter().map(|g| Value::String(g.to_string())).collect()),
        OptionValue::Account(g) => g.map_or(Value::Null, |g| Value::String(g.to_string())),
        OptionValue::Commodity(c) => Value::String(c.to_string()),
        OptionValue::Budget(g) => g.map_or(Value::Null, |g| Value::String(g.to_string())),
    }
}

fn key_json(k: &ChoiceKey) -> Value {
    match k {
        ChoiceKey::Text(s) => Value::String(s.clone()),
        ChoiceKey::Number(n) => Value::from(*n),
    }
}

/// An integer when whole, else a decimal string, else `n/d`.
fn number_json(n: &Rational) -> Value {
    match n.to_numeric().filter(|_| n.is_integer()) {
        Some(whole) => Value::from(whole.num()),
        None => Value::String(n.to_exact_decimal().unwrap_or_else(|| n.to_string())),
    }
}

fn boolean(json: &Value) -> Read<bool> {
    json.as_bool().ok_or_else(|| format!("expected true or false, got {json}"))
}

fn string(json: &Value) -> Read<&str> {
    json.as_str().ok_or_else(|| format!("expected a string, got {json}"))
}

/// `rrggbb` or `rrggbbaa`, a leading `#` accepted, stored without it as GnuCash stores it.
pub(super) fn color(json: &Value) -> Read<String> {
    let text = string(json)?;
    let hex = text.strip_prefix('#').unwrap_or(text);
    let valid = matches!(hex.len(), 6 | 8) && hex.bytes().all(|b| b.is_ascii_hexdigit());
    if valid {
        Ok(hex.to_ascii_lowercase())
    } else {
        Err(format!("not a colour rrggbb or rrggbbaa: {text:?}"))
    }
}

fn choice_key(json: &Value) -> Read<ChoiceKey> {
    match json {
        Value::String(s) => Ok(ChoiceKey::Text(s.clone())),
        Value::Number(n) => n.as_i64().map(ChoiceKey::Number).ok_or_else(|| format!("not a choice key: {json}")),
        _ => Err(format!("expected a choice key (a string or an integer), got {json}")),
    }
}

fn choice(choices: &[Choice], json: &Value) -> Read<ChoiceKey> {
    let key = choice_key(json)?;
    if choices.iter().any(|c| c.key == key) {
        Ok(key)
    } else {
        let keys: Vec<String> = choices.iter().map(|c| key_json(&c.key).to_string()).collect();
        Err(format!("{json} is not one of the choices {}", keys.join(", ")))
    }
}

/// An empty list reads as the default (`5.17:bindings/guile/gnc-optiondb.i:1073-1081`).
fn list(choices: &[Choice], default: &OptionValue, json: &Value) -> Read<OptionValue> {
    let items = json
        .as_array()
        .ok_or_else(|| format!("expected an array of choice keys, got {json}"))?;
    if items.is_empty() {
        return Ok(default.clone());
    }
    let keys = items.iter().map(|item| choice(choices, item)).collect::<Read<Vec<_>>>()?;
    let repeated = keys.iter().enumerate().find(|(i, k)| keys[..*i].contains(k));
    match repeated {
        Some((_, k)) => Err(format!("{} is chosen twice", key_json(k))),
        None => Ok(OptionValue::Keys(keys)),
    }
}

/// A decimal string or an integer, never a float, within the range (`validate`,
/// `5.17:libgnucash/engine/gnc-option-impl.hpp:422-439`).
fn number(json: &Value, min: &Rational, max: &Rational) -> Read<Rational> {
    let n = match json {
        Value::String(s) => Rational::parse(s).map_err(|e| e.to_string())?,
        Value::Number(n) => n
            .as_i64()
            .map(Rational::from_int)
            .ok_or_else(|| format!("{json} is not a whole number; send decimals as a string"))?,
        _ => return Err(format!("expected a number as an integer or a decimal string, got {json}")),
    };
    if n < *min || n > *max {
        return Err(format!(
            "{} is outside {} to {}",
            display_rational(&n),
            display_rational(min),
            display_rational(max)
        ));
    }
    Ok(n)
}

fn display_rational(n: &Rational) -> String {
    n.to_exact_decimal().unwrap_or_else(|| n.to_string())
}

/// `{ "percent": n }` (10 to 100), `{ "pixels": n }` (101 to 65535), or a bare integer read GnuCash's way: above 100
/// means pixels (`5.17:bindings/guile/gnc-optiondb.i:1094-1108`, `:1376-1384`).
fn plot_size(json: &Value) -> Read<PlotSize> {
    let size = match json {
        Value::Number(n) => {
            let n = n
                .as_u64()
                .and_then(|n| u32::try_from(n).ok())
                .ok_or_else(|| format!("not a plot size: {json}"))?;
            if n > PLOT_SIZE_PERCENT_MAX {
                PlotSize::Pixels(n)
            } else {
                PlotSize::Percent(n)
            }
        }
        _ => serde_json::from_value(json.clone())
            .map_err(|_| format!("expected {{\"percent\": n}}, {{\"pixels\": n}} or an integer, got {json}"))?,
    };
    match size {
        PlotSize::Percent(n) if !(PLOT_SIZE_MIN..=PLOT_SIZE_PERCENT_MAX).contains(&n) => Err(format!("{n} % is outside 10 to 100")),
        PlotSize::Pixels(n) if !(PLOT_SIZE_PERCENT_MAX + 1..=PLOT_SIZE_MAX).contains(&n) => {
            Err(format!("{n} pixels is outside 101 to 65535"))
        }
        ok => Ok(ok),
    }
}

/// `{ "absolute": Date or Timestamp }` or `{ "relative": name }`. An absolute Date is local 10:59:00 of that day, the
/// time GnuCash's date widget gives (`gnc_tm_set_day_neutral`, `5.17:gnucash/gnome-utils/gnc-date-edit.c:1062-1066`).
fn date(json: &Value, ui: DateUi, allowed: &[RelativeDate], ctx: &ValueContext<'_>) -> Read<DateValue> {
    let shape = || format!("expected {{\"absolute\": …}} or {{\"relative\": …}}, got {json}");
    let fields = json.as_object().filter(|o| o.len() == 1).ok_or_else(shape)?;
    match fields.iter().next() {
        Some((k, v)) if k == "absolute" => {
            (ui != DateUi::Relative)
                .then_some(())
                .ok_or("this date option takes only relative dates")?;
            absolute_date(string(v)?, ctx.zone).map(DateValue::Absolute)
        }
        Some((k, v)) if k == "relative" => {
            (ui != DateUi::Absolute)
                .then_some(())
                .ok_or("this date option takes only absolute dates")?;
            let name = string(v)?;
            let r = parse_relative(name).ok_or_else(|| format!("not a relative date: {name:?}"))?;
            if allowed.is_empty() || allowed.contains(&r) {
                Ok(DateValue::Relative(r))
            } else {
                Err(format!("{name:?} is not offered by this option"))
            }
        }
        _ => Err(shape()),
    }
}

fn absolute_date(text: &str, zone: &TimeZone) -> Read<Time64> {
    let t = match Date::parse(text) {
        Ok(d) => zone
            .instant(d, 10, 59, 0)
            .ok_or_else(|| format!("{text} is outside the supported range"))?,
        Err(_) => {
            Timestamp::parse(text)
                .map_err(|_| format!("not a date YYYY-MM-DD or an RFC 3339 timestamp: {text:?}"))?
                .0
        }
    };
    check_time(t).map_err(|()| format!("{text} is outside the supported range"))
}

/// GnuCash's range for date options, `MINTIME < t < MAXTIME` (`5.17:libgnucash/engine/gnc-option-impl.hpp:1093-1098`).
pub(super) fn check_time(t: Time64) -> Result<Time64, ()> {
    if t.0 > MIN_TIME && t.0 < MAX_TIME { Ok(t) } else { Err(()) }
}

/// A non-multiselect option holds at most one account, and every account must be of an allowed type
/// (`5.17:libgnucash/engine/gnc-option-impl.cpp:384-407`). An empty list reads as the default.
fn account_list(json: &Value, types: &[AccountType], multiple: bool, default: &OptionValue, ctx: &ValueContext<'_>) -> Read<OptionValue> {
    let items = json
        .as_array()
        .ok_or_else(|| format!("expected an array of accounts, got {json}"))?;
    if items.is_empty() {
        return Ok(default.clone());
    }
    if !multiple && items.len() > 1 {
        return Err("this option takes one account".to_owned());
    }
    let guids = items
        .iter()
        .map(|item| string(item).and_then(|s| account(s, types, ctx)))
        .collect::<Read<Vec<_>>>()?;
    let repeated = guids.iter().enumerate().find(|(i, g)| guids[..*i].contains(g));
    match repeated {
        Some((_, g)) => Err(format!("account {g} is chosen twice")),
        None => Ok(OptionValue::Accounts(guids)),
    }
}

/// An account by GUID or full name. Without a book only a GUID can be read, and it is not checked.
fn account(text: &str, types: &[AccountType], ctx: &ValueContext<'_>) -> Read<Guid> {
    let text = text.trim();
    let Some(book) = ctx.book else {
        return Guid::parse(text).map_err(|_| format!("{text:?} is not a GUID; naming an account by full name needs an open book"));
    };
    let found = Guid::parse(text)
        .ok()
        .and_then(|g| book.account(g))
        .or_else(|| book.account_by_full_name(text));
    let a = found.ok_or_else(|| format!("unknown account {text:?}"))?;
    if types.is_empty() || types.contains(&a.account_type) {
        Ok(a.guid)
    } else {
        Err(format!(
            "account {text:?} is of type {}, not one this option allows",
            a.account_type.as_str()
        ))
    }
}

/// A budget by GUID, as GnuCash saves it (`5.17:bindings/guile/options.scm:150-160`). With a Book it must name one of
/// its budgets; without one it is kept unchecked, as an account's GUID is.
fn budget(text: &str, ctx: &ValueContext<'_>) -> Read<Guid> {
    let guid = Guid::parse(text.trim()).map_err(|_| format!("{text:?} is not a budget's GUID"))?;
    match ctx.book {
        Some(book) if book.budget(guid).is_none() => Err(format!("unknown budget {guid}")),
        _ => Ok(guid),
    }
}

/// A commodity id; a currency option takes only the `CURRENCY` namespace (`5.17:libgnucash/engine/gnc-option-impl.cpp:341-349`).
fn commodity(json: &Value, currency: bool, ctx: &ValueContext<'_>) -> Read<CommodityRef> {
    let id = parse_commodity(string(json)?)?;
    if currency && !id.is_currency() {
        return Err(format!("{id} is not a currency"));
    }
    match ctx.book {
        Some(book) => lookup(book, &id)
            .map(|c| c.id.clone())
            .ok_or_else(|| format!("unknown commodity {id}")),
        None => Ok(id),
    }
}

/// Put a wire value under its section and name.
pub(super) fn insert(map: &mut Map<String, Value>, section: &str, name: &str, value: Value) {
    let entry = map.entry(section.to_owned()).or_insert_with(|| Value::Object(Map::new()));
    if let Value::Object(names) = entry {
        names.insert(name.to_owned(), value);
    }
}
