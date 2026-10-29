//! GnuCash's saved option values, as data, turned into red-ale's wire form for the option they set
//! (`design/reports.md` §6), so [`OptionDb::apply`](crate::options::OptionDb::apply) checks them as it checks any
//! client's.
//!
//! The forms are those `GncOption::save_scm_value` writes (`5.17:bindings/guile/gnc-optiondb.i:1216-1342`), read back
//! as `set_value_from_scm` reads them (`:1344-1500`), and those 2.x to 4.x wrote, which 5.17 still reads:
//!
//! | Option | Written as |
//! |--------|------------|
//! | boolean | `#t`, `#f` |
//! | string, text, font, pixmap | `"…"` |
//! | colour | `'(r g b a)`, components 0 to 255 written as `0.0`–`255.0` (`:1310-1328`), read by truncating each (`:330-346`) |
//! | multichoice | `'symbol`, `'"string"` or `'3`, matched against the choices by its text (`:996-1033`) |
//! | list | `'(a b)`, or `'#f` with none (`:1073-1081`) |
//! | number range | `'2.0` (5.17), `2` (4.x) |
//! | plot size | `'(percent . 50)`, `'(pixels . 500)`: the number alone decides, above 100 is pixels (`:1376-1384`) |
//! | date | `'(absolute . <seconds>)`, `'(relative . <name>)` (`5.17:libgnucash/engine/gnc-option-impl.cpp:967-984`) |
//! | account list | `'("<guid>" …)`, `'""` when empty (`:1226-1240`) |
//! | account | `"<guid>"` |
//! | currency | `"USD"` (5.17), `'(commodity-scm "CURRENCY" "USD")` (4.x) |
//! | commodity | `'(commodity-scm "<namespace>" "<mnemonic>")` (`:1249-1263`) |
//! | budget | `(gnc-budget-lookup "<guid>" (gnc-get-current-book))` (`5.17:bindings/guile/options.scm:150-160`) |
//!
//! 2.x wrote an absolute date as `'(absolute <seconds> . <nanoseconds>)` (`2.6.21:src/app-utils/options.scm:571-578`).
//! Relative dates keep their name: both spellings of the accounting period's (`start-prev-fin-year`, written by 5.17,
//! and `start-accounting-period`) are read by the option itself (`crate::options::parse_relative`).

use std::fmt::Write as _;

use red_ale_core::Time64;
use serde_json::{Map, Value};

use super::forms::{OptionForm, budget_lookup};
use super::sexp::Datum;
use crate::error::OptionError;
use crate::number::{Rational, Rounding};
use crate::options::{Choice, ChoiceKey, OptionDb, OptionKind, OptionValue};
use crate::wire::Timestamp;

type Converted<T> = Result<T, String>;

/// Why an option an entry sets was left out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ignored {
    /// The template has no such option. GnuCash ignores it too: its setter is `(if o …)`.
    UnknownOption,
    /// An option red-ale keeps in the Configuration but does not render, such as a style sheet's images.
    NotRendered,
}

impl Ignored {
    /// The reason's name in the API.
    pub const fn name(self) -> &'static str {
        match self {
            Self::UnknownOption => "unknown-option",
            Self::NotRendered => "not-rendered",
        }
    }
}

/// An entry's option values in wire form, by section and name, and the options left out.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct WireOptions {
    /// The values for the options the template has: `{ section: { name: value } }`.
    pub options: Map<String, Value>,
    /// Values for options red-ale keeps but does not render, in a generic wire form.
    pub kept: Map<String, Value>,
    /// Every option left out of `options`, in file order, and why.
    pub ignored: Vec<(String, String, Ignored)>,
}

/// The wire form of every option `forms` sets, by the option of `db` each names. `not_rendered` says which options
/// that `db` lacks are still kept. A value that does not fit its option is the error.
pub fn to_wire(db: &OptionDb, forms: &[OptionForm], not_rendered: impl Fn(&str, &str) -> bool) -> Result<WireOptions, OptionError> {
    forms.iter().try_fold(WireOptions::default(), |mut out, f| {
        let error = |reason: String| OptionError::new(&f.section, &f.name, reason);
        match db.get(&f.section, &f.name) {
            Some(option) => insert(
                &mut out.options,
                f,
                value(&option.kind, option.default_value(), &f.value).map_err(error)?,
            ),
            None if not_rendered(&f.section, &f.name) => {
                insert(&mut out.kept, f, generic(&f.value).map_err(error)?);
                out.ignored.push((f.section.clone(), f.name.clone(), Ignored::NotRendered));
            }
            None => out.ignored.push((f.section.clone(), f.name.clone(), Ignored::UnknownOption)),
        }
        Ok(out)
    })
}

fn insert(map: &mut Map<String, Value>, f: &OptionForm, value: Value) {
    let section = map.entry(f.section.clone()).or_insert_with(|| Value::Object(Map::new()));
    if let Value::Object(names) = section {
        names.insert(f.name.clone(), value);
    }
}

/// The wire value of `d` for an option of `kind`.
fn value(kind: &OptionKind, default: &OptionValue, d: &Datum) -> Converted<Value> {
    match kind {
        OptionKind::Boolean => boolean(d),
        OptionKind::Internal if matches!(default, OptionValue::Bool(_)) => boolean(d),
        OptionKind::String | OptionKind::Text | OptionKind::Font | OptionKind::Internal => string(d).map(Value::from),
        OptionKind::Color => color(d),
        OptionKind::Multichoice { choices } => choice(choices, d),
        OptionKind::List { choices } => list(choices, d),
        OptionKind::NumberRange { .. } => number(d).map(|n| number_json(&n)),
        OptionKind::PlotSize => plot_size(d),
        OptionKind::Date { .. } => date(d),
        OptionKind::AccountList { .. } => account_list(d),
        OptionKind::AccountSel { .. } => account(d),
        OptionKind::Currency | OptionKind::Commodity => commodity(d).map(Value::from),
        OptionKind::Budget => budget_lookup(d)
            .map(Value::from)
            .ok_or_else(|| format!("expected (gnc-budget-lookup \"<guid>\" (gnc-get-current-book)), got {d}")),
        // Its literal names the session's report instances, which only the embedded-report forms that follow it in
        // the file give meaning (`design/notes/report-statements-charts.md` §8.7).
        OptionKind::ReportPlacement => Err(format!("a report-list is read only with its embedded reports, got {d}")),
    }
}

fn boolean(d: &Datum) -> Converted<Value> {
    match d {
        Datum::Bool(b) => Ok(Value::Bool(*b)),
        _ => Err(format!("expected #t or #f, got {d}")),
    }
}

fn string(d: &Datum) -> Converted<&str> {
    d.as_str().ok_or_else(|| format!("expected a string, got {d}"))
}

fn number(d: &Datum) -> Converted<Rational> {
    match d {
        Datum::Number(n) => Ok(n.clone()),
        _ => Err(format!("expected a number, got {d}")),
    }
}

/// An integer when whole, else a decimal string, else `n/d`: what a number range option reads.
fn number_json(n: &Rational) -> Value {
    match whole(n) {
        Some(i) if n.is_integer() => Value::from(i),
        _ => Value::String(n.to_exact_decimal().unwrap_or_else(|| n.to_string())),
    }
}

/// `n` truncated toward zero, as GnuCash's casts from double truncate, if it fits.
fn whole(n: &Rational) -> Option<i64> {
    n.round_to(1, Rounding::Truncate).ok()?.to_numeric().map(red_ale_core::Numeric::num)
}

/// `'(r g b a)` as `rrggbbaa`, each component truncated (`scm_color_list_to_string`,
/// `5.17:bindings/guile/gnc-optiondb.i:330-346`); an opaque alpha is dropped, so a colour equal to a six-digit
/// default is not kept as changed. A string is taken as the hex text itself.
fn color(d: &Datum) -> Converted<Value> {
    if let Some(s) = d.as_str() {
        return Ok(Value::from(s));
    }
    let items = d
        .as_list()
        .filter(|l| matches!(l.len(), 3 | 4))
        .ok_or_else(|| format!("expected a colour (r g b a), got {d}"))?;
    let components = items
        .iter()
        .map(|c| {
            let value = number(c).ok().and_then(|n| whole(&n)).filter(|v| (0..=255).contains(v));
            value.ok_or_else(|| format!("not a colour component 0 to 255: {c}"))
        })
        .collect::<Converted<Vec<i64>>>()?;
    let opaque = components.get(3) == Some(&255);
    let shown = if opaque { &components[..3] } else { &components[..] };
    Ok(Value::String(shown.iter().fold(String::new(), |mut hex, c| {
        let _ = write!(hex, "{c:02x}");
        hex
    })))
}

/// A choice key's text as GnuCash compares it: a symbol's name, a string, a number's digits.
fn key_text(d: &Datum) -> Converted<String> {
    match d {
        Datum::Symbol(s) | Datum::String(s) => Ok(s.clone()),
        Datum::Number(n) if n.is_integer() => Ok(n.to_string()),
        _ => Err(format!("not a choice key: {d}")),
    }
}

/// The choice whose key reads as `d`'s text (`permissible_value_index` compares strings).
fn choice(choices: &[Choice], d: &Datum) -> Converted<Value> {
    let text = key_text(d)?;
    let found = choices
        .iter()
        .find(|c| c.key.to_string() == text)
        .ok_or_else(|| format!("{d} is not one of the choices"))?;
    Ok(match &found.key {
        ChoiceKey::Text(s) => Value::String(s.clone()),
        ChoiceKey::Number(n) => Value::from(*n),
    })
}

/// A list of keys; `#f` (no choice) reads as the default, and a lone key as a list of one.
fn list(choices: &[Choice], d: &Datum) -> Converted<Value> {
    match d {
        Datum::Bool(false) => Ok(Value::Array(Vec::new())),
        Datum::List(items, None) => items.iter().map(|i| choice(choices, i)).collect::<Converted<_>>().map(Value::Array),
        other => choice(choices, other).map(|k| Value::Array(vec![k])),
    }
}

/// A plot size: GnuCash reads only the number, above 100 meaning pixels, and truncates a fraction.
fn plot_size(d: &Datum) -> Converted<Value> {
    let n = match d.as_pair() {
        Some((unit, n)) if unit.is_symbol("percent") || unit.is_symbol("pixels") => n,
        _ => d,
    };
    number(n)
        .and_then(|n| whole(&n).ok_or_else(|| format!("not a plot size: {d}")))
        .map(Value::from)
}

/// `(absolute . seconds)`, 2.x's `(absolute seconds . nanoseconds)`, or `(relative . name)`.
fn date(d: &Datum) -> Converted<Value> {
    let shape = || format!("expected (absolute . seconds) or (relative . name), got {d}");
    let (kind, rest) = match d {
        Datum::List(items, Some(tail)) => match items.as_slice() {
            [kind] => (kind, &**tail),
            [kind, seconds] if kind.is_symbol("absolute") && matches!(**tail, Datum::Number(_)) => (kind, seconds),
            _ => return Err(shape()),
        },
        _ => return Err(shape()),
    };
    match kind.as_symbol() {
        Some("absolute") => {
            let seconds = number(rest)
                .ok()
                .filter(Rational::is_integer)
                .and_then(|n| whole(&n))
                .ok_or_else(shape)?;
            let text = Timestamp(Time64(seconds))
                .to_rfc3339()
                .ok_or_else(|| format!("{seconds} is outside the supported range"))?;
            Ok(serde_json::json!({ "absolute": text }))
        }
        Some("relative") => Ok(serde_json::json!({ "relative": rest.as_symbol().ok_or_else(shape)? })),
        _ => Err(shape()),
    }
}

/// GUID strings; `""`, `#f` and `()` are none, which reads as the default.
fn account_list(d: &Datum) -> Converted<Value> {
    match d {
        Datum::String(s) if s.is_empty() => Ok(Value::Array(Vec::new())),
        Datum::Bool(false) => Ok(Value::Array(Vec::new())),
        Datum::List(items, None) => items
            .iter()
            .map(|i| string(i).map(Value::from))
            .collect::<Converted<_>>()
            .map(Value::Array),
        _ => Err(format!("expected a list of account GUIDs, got {d}")),
    }
}

/// A GUID string; `""` or `#f` is none, which reads as the default.
fn account(d: &Datum) -> Converted<Value> {
    match d {
        Datum::Bool(false) => Ok(Value::Null),
        Datum::String(s) if s.is_empty() => Ok(Value::Null),
        _ => string(d).map(Value::from),
    }
}

/// `"USD"` (a currency), or `(commodity-scm "<namespace>" "<mnemonic>")`, as `NAMESPACE::MNEMONIC`. GnuCash's old
/// currency namespace `ISO4217` is `CURRENCY` (`5.17:libgnucash/engine/gnc-commodity.cpp:1556-1561`).
fn commodity(d: &Datum) -> Converted<String> {
    if let Some(code) = d.as_str() {
        return Ok(format!("CURRENCY::{code}"));
    }
    match d.as_list() {
        Some([tag, ns, mnemonic]) if tag.is_symbol("commodity-scm") => {
            let (ns, mnemonic) = (string(ns)?, string(mnemonic)?);
            let ns = if ns == "ISO4217" { "CURRENCY" } else { ns };
            Ok(format!("{ns}::{mnemonic}"))
        }
        _ => Err(format!("expected a currency code or (commodity-scm namespace mnemonic), got {d}")),
    }
}

/// A value for an option red-ale does not declare, kept as close to its datum as the wire allows.
fn generic(d: &Datum) -> Converted<Value> {
    match d {
        Datum::Bool(b) => Ok(Value::Bool(*b)),
        Datum::String(s) | Datum::Symbol(s) => Ok(Value::String(s.clone())),
        Datum::Number(n) => Ok(number_json(n)),
        Datum::List(..) => Err(format!("cannot keep the value {d}")),
    }
}

#[cfg(test)]
mod tests {
    use super::super::sexp::read_one;
    use super::*;
    use crate::options::{Classifier, DateUi, DateValue};
    use crate::result::PlotSize;
    use red_ale_core::time::RelativeDate;
    use red_ale_core::{CommodityRef, TimeZone};
    use serde_json::json;

    fn datum(text: &str) -> Datum {
        let d = read_one(text).unwrap();
        d.unquoted().cloned().unwrap_or(d)
    }

    fn c(name: &str) -> Classifier {
        Classifier::new("S", name, "a", "")
    }

    /// One option of each kind, at defaults.
    fn db() -> OptionDb {
        let mut db = OptionDb::new();
        db.boolean(c("bool"), false).unwrap();
        db.string(c("string"), "").unwrap();
        db.color(c("color"), "000000").unwrap();
        db.multichoice(
            c("multi"),
            "a".into(),
            vec![Choice::new("a", "A", ""), Choice::new("two plus three", "5", "")],
        )
        .unwrap();
        db.multichoice(c("levels"), 1.into(), vec![Choice::new(1, "1", ""), Choice::new(2, "2", "")])
            .unwrap();
        db.list(
            c("list"),
            vec!["good".into()],
            vec![Choice::new("good", "", ""), Choice::new("ugly", "", "")],
        )
        .unwrap();
        let int = Rational::from_int;
        db.number_range(c("range"), int(1), int(0), int(100), int(1)).unwrap();
        db.plot_size(c("plot"), PlotSize::Percent(100)).unwrap();
        db.date(c("date"), DateValue::Relative(RelativeDate::Today), DateUi::Both, Vec::new())
            .unwrap();
        db.account_list(c("accounts"), Vec::new(), Vec::new(), true).unwrap();
        db.account_sel(c("account"), None, Vec::new()).unwrap();
        db.currency(c("currency"), CommodityRef::currency("USD")).unwrap();
        db.commodity(c("commodity"), CommodityRef::currency("USD")).unwrap();
        db.internal(c("internal"), OptionValue::Bool(false)).unwrap();
        db.budget(c("budget"), None).unwrap();
        db
    }

    fn wire(name: &str, text: &str) -> Result<Value, OptionError> {
        let form = OptionForm {
            section: "S".into(),
            name: name.into(),
            value: datum(text),
        };
        let db = db();
        let out = to_wire(&db, &[form], |_, _| false)?;
        let v = out.options["S"][name].clone();
        // Every value converted is one the option itself accepts.
        let mut check = db.clone();
        check.apply(
            &json!({ "S": { name: v.clone() } }),
            &crate::options::ValueContext {
                book: None,
                zone: &TimeZone::utc(),
            },
        )?;
        Ok(v)
    }

    #[test]
    fn every_option_kind() {
        assert_eq!(wire("bool", "#t").unwrap(), json!(true));
        assert_eq!(wire("string", r#""pepper""#).unwrap(), json!("pepper"));
        assert_eq!(wire("color", "'(0.0 202.0 59.0 255.0)").unwrap(), json!("00ca3b"));
        assert_eq!(wire("color", "'(178.0 34.0 34.0 128.0)").unwrap(), json!("b2222280"));
        assert_eq!(wire("color", "'(255.9 0 0)").unwrap(), json!("ff0000"));
        assert_eq!(wire("multi", "'#{two plus three}#").unwrap(), json!("two plus three"));
        assert_eq!(wire("multi", r#"'"two plus three""#).unwrap(), json!("two plus three"));
        assert_eq!(wire("levels", "'2").unwrap(), json!(2));
        assert!(wire("levels", "'|2|").is_err());
        assert_eq!(wire("list", "'(ugly good)").unwrap(), json!(["ugly", "good"]));
        assert_eq!(wire("list", "'#f").unwrap(), json!([]));
        assert_eq!(wire("range", "'42.5").unwrap(), json!("42.5"));
        assert_eq!(wire("range", "2").unwrap(), json!(2));
        assert_eq!(wire("plot", "'(percent . 48)").unwrap(), json!(48));
        assert_eq!(wire("plot", "'(pixels . 500)").unwrap(), json!(500));
        assert_eq!(wire("plot", "'(pixels . 640.0)").unwrap(), json!(640));
        assert_eq!(
            wire("date", "'(absolute . 1608897600)").unwrap(),
            json!({ "absolute": "2020-12-25T12:00:00Z" })
        );
        assert_eq!(
            wire("date", "'(absolute 1608897600 . 0)").unwrap(),
            json!({ "absolute": "2020-12-25T12:00:00Z" })
        );
        assert_eq!(
            wire("date", "'(relative . end-prev-year)").unwrap(),
            json!({ "relative": "end-prev-year" })
        );
        assert_eq!(
            wire("date", "'(relative . start-prev-fin-year)").unwrap(),
            json!({ "relative": "start-prev-fin-year" })
        );
        assert_eq!(
            wire("date", "'(relative . start-accounting-period)").unwrap(),
            json!({ "relative": "start-accounting-period" })
        );
        let guid = "3a5a4bc736d84b879b776ea8caadd3b2";
        assert_eq!(wire("accounts", &format!("'(\"{guid}\")")).unwrap(), json!([guid]));
        assert_eq!(wire("accounts", "'\"\"").unwrap(), json!([]));
        assert_eq!(wire("account", &format!("\"{guid}\"")).unwrap(), json!(guid));
        assert_eq!(wire("currency", r#""EUR""#).unwrap(), json!("CURRENCY::EUR"));
        assert_eq!(
            wire("currency", r#"'(commodity-scm "ISO4217" "GBP")"#).unwrap(),
            json!("CURRENCY::GBP")
        );
        assert_eq!(
            wire("commodity", r#"'(commodity-scm "NASDAQ" "AAPL")"#).unwrap(),
            json!("NASDAQ::AAPL")
        );
        assert_eq!(wire("internal", "#t").unwrap(), json!(true));
        let lookup = format!("(gnc-budget-lookup \"{guid}\" (gnc-get-current-book))");
        assert_eq!(wire("budget", &lookup).unwrap(), json!(guid));
        assert!(wire("budget", &format!("\"{guid}\"")).is_err(), "GnuCash writes only the lookup");
        assert!(wire("account", &lookup).is_err(), "the lookup sets only a budget option");
    }

    #[test]
    fn values_that_do_not_fit_are_errors() {
        [
            "bool:\"yes\"",
            "color:'(1 2)",
            "color:'(0 0 300 255)",
            "multi:'b",
            "range:'x",
            "plot:'(percent . big)",
            "date:'(relative)",
            "date:'(sometime . 5)",
            "accounts:'(1 2)",
            "currency:'(commodity-scm \"X\")",
            "string:#t",
        ]
        .iter()
        .for_each(|case| {
            let (name, text) = case.split_once(':').unwrap();
            let error = wire(name, text).unwrap_err();
            assert_eq!(error.name, name, "{case}");
        });
        // A converted value the option refuses is also an error, from the option itself.
        assert!(wire("range", "'101").is_err());
        assert!(wire("date", "'(relative . yesterday)").is_err());
    }

    #[test]
    fn options_the_template_lacks() {
        let forms = [
            OptionForm {
                section: "Images".into(),
                name: "Logo".into(),
                value: datum("\"logo.png\""),
            },
            OptionForm {
                section: "S".into(),
                name: "gone".into(),
                value: datum("#t"),
            },
        ];
        let out = to_wire(&db(), &forms, |s, _| s == "Images").unwrap();
        assert_eq!(out.kept, json!({ "Images": { "Logo": "logo.png" } }).as_object().cloned().unwrap());
        assert_eq!(
            out.ignored.iter().map(|i| i.2).collect::<Vec<_>>(),
            [Ignored::NotRendered, Ignored::UnknownOption]
        );
        assert!(out.options.is_empty());
    }
}
