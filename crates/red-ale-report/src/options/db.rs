//! [`OptionDb`]: a template's options, declared by its script, set by a client and read by its renderer. GnuCash's
//! `GncOptionDB` (`5.17:libgnucash/engine/gnc-optiondb.cpp`).

use red_ale_core::ops::accounts::sorted_children;
use red_ale_core::time::RelativeDate;
use red_ale_core::{AccountType, Book, CommodityRef, Guid};
use serde_json::{Map, Value};

use super::model::{Choice, ChoiceKey, Classifier, DateUi, DateValue, OptionKind, OptionValue, ReportOption};
use super::value::{self, ValueContext, check_time, insert};
use crate::error::OptionError;
use crate::number::Rational;
use crate::result::{OptionValues, PlotSize};

/// The section of the options every template gets (`gnc:pagename-general`, `5.17:gnucash/report/report-core.scm:165`).
pub const GENERAL: &str = "General";
/// The report's name (`gnc:optname-reportname`, `:168`).
pub const REPORT_NAME: &str = "Report name";
/// Whether to append the options summary (`gnc:optname-options-summary`, `:169`).
pub const OPTIONS_SUMMARY: &str = "Add options summary";
/// The style sheet to render with (`gnc:optname-stylesheet`, `:170`).
pub const STYLESHEET: &str = "Stylesheet";

/// The start-date choices of `gnc-register-start-date-option` (`5.17:libgnucash/engine/gnc-optiondb.cpp:1052-1062`).
pub const START_DATES: [RelativeDate; 8] = [
    RelativeDate::Today,
    RelativeDate::StartThisMonth,
    RelativeDate::StartPrevMonth,
    RelativeDate::StartCurrentQuarter,
    RelativeDate::StartPrevQuarter,
    RelativeDate::StartCalYear,
    RelativeDate::StartPrevYear,
    RelativeDate::StartAccountingPeriod,
];

/// The end-date choices of `gnc-register-end-date-option` (`:1076-1086`).
pub const END_DATES: [RelativeDate; 8] = [
    RelativeDate::Today,
    RelativeDate::EndThisMonth,
    RelativeDate::EndPrevMonth,
    RelativeDate::EndCurrentQuarter,
    RelativeDate::EndPrevQuarter,
    RelativeDate::EndCalYear,
    RelativeDate::EndPrevYear,
    RelativeDate::EndAccountingPeriod,
];

/// A template's options.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OptionDb {
    options: Vec<ReportOption>,
    default_section: Option<String>,
}

type Declared = Result<(), OptionError>;

impl OptionDb {
    /// No options.
    pub fn new() -> Self {
        Self::default()
    }

    /// Declare an option. Its default is checked against its type, as GnuCash's constructors check it: a multichoice
    /// default that is not a choice becomes the first choice (`5.17:libgnucash/engine/gnc-optiondb.cpp:812-828`) and a
    /// number outside its range becomes the minimum (`5.17:libgnucash/engine/gnc-option-impl.hpp:410-413`). Declaring
    /// the same section and name twice is an error.
    pub fn register(&mut self, classifier: Classifier, kind: OptionKind, default: OptionValue) -> Declared {
        let fail = |reason: String| OptionError::new(&classifier.section, &classifier.name, reason);
        if self.get(&classifier.section, &classifier.name).is_some() {
            return Err(fail("declared twice".to_owned()));
        }
        let default = checked_default(&kind, default).map_err(fail)?;
        self.options.push(ReportOption::declared(classifier, kind, default));
        Ok(())
    }

    /// `gnc-register-simple-boolean-option`.
    pub fn boolean(&mut self, c: Classifier, default: bool) -> Declared {
        self.register(c, OptionKind::Boolean, OptionValue::Bool(default))
    }

    /// `gnc-register-string-option`.
    pub fn string(&mut self, c: Classifier, default: &str) -> Declared {
        self.register(c, OptionKind::String, OptionValue::Text(default.to_owned()))
    }

    /// `gnc-register-text-option`.
    pub fn text(&mut self, c: Classifier, default: &str) -> Declared {
        self.register(c, OptionKind::Text, OptionValue::Text(default.to_owned()))
    }

    /// `gnc-register-font-option`.
    pub fn font(&mut self, c: Classifier, default: &str) -> Declared {
        self.register(c, OptionKind::Font, OptionValue::Text(default.to_owned()))
    }

    /// `gnc-register-color-option`. The default is kept as given, even when it is not a valid colour, as GnuCash keeps
    /// Head or Tail's `"eexe8xaa"` (`5.17:gnucash/report/stylesheets/head-or-tail.scm:168-172`).
    pub fn color(&mut self, c: Classifier, default: &str) -> Declared {
        self.register(c, OptionKind::Color, OptionValue::Text(default.trim_start_matches('#').to_owned()))
    }

    /// `gnc-register-multichoice-option`.
    pub fn multichoice(&mut self, c: Classifier, default: ChoiceKey, choices: Vec<Choice>) -> Declared {
        self.register(c, OptionKind::Multichoice { choices }, OptionValue::Key(default))
    }

    /// `gnc-register-list-option`.
    pub fn list(&mut self, c: Classifier, defaults: Vec<ChoiceKey>, choices: Vec<Choice>) -> Declared {
        self.register(c, OptionKind::List { choices }, OptionValue::Keys(defaults))
    }

    /// `gnc-register-number-range-option`.
    pub fn number_range(&mut self, c: Classifier, default: Rational, min: Rational, max: Rational, step: Rational) -> Declared {
        self.register(c, OptionKind::NumberRange { min, max, step }, OptionValue::Number(default))
    }

    /// `gnc-register-number-plot-size-option`.
    pub fn plot_size(&mut self, c: Classifier, default: PlotSize) -> Declared {
        self.register(c, OptionKind::PlotSize, OptionValue::PlotSize(default))
    }

    /// `gnc-register-date-option-set`: a date with the given UI and relative choices (empty for any).
    pub fn date(&mut self, c: Classifier, default: DateValue, ui: DateUi, relative: Vec<RelativeDate>) -> Declared {
        self.register(c, OptionKind::Date { ui, relative }, OptionValue::Date(default))
    }

    /// `gnc-register-start-date-option`: the eight start choices, defaulting to the start of the accounting period;
    /// UI `both`, or `relative` when `both` is false (`5.17:libgnucash/engine/gnc-optiondb.cpp:1064-1074`).
    pub fn start_date(&mut self, c: Classifier, both: bool) -> Declared {
        let ui = if both { DateUi::Both } else { DateUi::Relative };
        self.date(
            c,
            DateValue::Relative(RelativeDate::StartAccountingPeriod),
            ui,
            START_DATES.to_vec(),
        )
    }

    /// `gnc-register-end-date-option`: the eight end choices, defaulting to the end of the accounting period.
    pub fn end_date(&mut self, c: Classifier, both: bool) -> Declared {
        let ui = if both { DateUi::Both } else { DateUi::Relative };
        self.date(c, DateValue::Relative(RelativeDate::EndAccountingPeriod), ui, END_DATES.to_vec())
    }

    /// `gnc-register-account-list-option` and `-limited-option`.
    pub fn account_list(&mut self, c: Classifier, default: Vec<Guid>, types: Vec<AccountType>, multiple: bool) -> Declared {
        self.register(c, OptionKind::AccountList { types, multiple }, OptionValue::Accounts(default))
    }

    /// `gnc-register-account-sel-limited-option`.
    pub fn account_sel(&mut self, c: Classifier, default: Option<Guid>, types: Vec<AccountType>) -> Declared {
        self.register(c, OptionKind::AccountSel { types }, OptionValue::Account(default))
    }

    /// `gnc-register-currency-option`.
    pub fn currency(&mut self, c: Classifier, default: CommodityRef) -> Declared {
        self.register(c, OptionKind::Currency, OptionValue::Commodity(default))
    }

    /// `gnc-register-commodity-option`.
    pub fn commodity(&mut self, c: Classifier, default: CommodityRef) -> Declared {
        self.register(c, OptionKind::Commodity, OptionValue::Commodity(default))
    }

    /// `gnc_register_internal_option`: a string or boolean default.
    pub fn internal(&mut self, c: Classifier, default: OptionValue) -> Declared {
        self.register(c, OptionKind::Internal, default)
    }

    /// `GncOptionDBPtr-set-default-section`: the section a dialog opens on.
    pub fn set_default_section(&mut self, section: &str) {
        self.default_section = Some(section.to_owned());
    }

    /// The section a dialog opens on.
    pub fn default_section(&self) -> Option<&str> {
        self.default_section.as_deref()
    }

    /// GnuCash's three General options, each unless the script declared it (`gnc:report-template-new-options`,
    /// `5.17:gnucash/report/report-core.scm:298-335`): "Report name" (`0a`, the template's name), "Stylesheet" (`0b`, a
    /// choice of `style_sheets`, default `Default`) and "Add options summary" (`0c`, `always` or `never`, default
    /// `never`).
    pub fn add_general_options(&mut self, template_name: &str, style_sheets: &[&str]) -> Declared {
        let general = |name: &str, key: &str, help: &str| Classifier::new(GENERAL, name, key, help);
        if self.get(GENERAL, REPORT_NAME).is_none() {
            self.string(
                general(REPORT_NAME, "0a", "Enter a descriptive name for this report."),
                template_name,
            )?;
        }
        if self.get(GENERAL, STYLESHEET).is_none() {
            let choices = style_sheets.iter().map(|s| Choice::new(*s, *s, "")).collect();
            self.multichoice(
                general(STYLESHEET, "0b", "Select a stylesheet for the report."),
                ChoiceKey::from("Default"),
                choices,
            )?;
        }
        if self.get(GENERAL, OPTIONS_SUMMARY).is_none() {
            let choices = vec![Choice::new("always", "Always", ""), Choice::new("never", "Never", "")];
            self.multichoice(
                general(OPTIONS_SUMMARY, "0c", "Add summary of options."),
                ChoiceKey::from("never"),
                choices,
            )?;
        }
        Ok(())
    }

    /// Every option in GnuCash's order: sections by name, then options by key (`5.17:libgnucash/engine/gnc-optiondb.cpp:143-148`,
    /// `:191-194`), both compared bytewise. Equal keys keep declaration order, where GnuCash's sort leaves them in no
    /// defined order.
    pub fn options(&self) -> Vec<&ReportOption> {
        let mut ordered: Vec<&ReportOption> = self.options.iter().collect();
        ordered.sort_by(|a, b| a.section().cmp(b.section()).then_with(|| a.classifier.key.cmp(&b.classifier.key)));
        ordered
    }

    /// The option `section`/`name`.
    pub fn get(&self, section: &str, name: &str) -> Option<&ReportOption> {
        self.options.iter().find(|o| o.section() == section && o.name() == name)
    }

    /// Set options from the `options` param of `report.run` and the configuration methods: an object by section
    /// and name with the wire form of each value; `null` returns an option to its default. Every value is checked
    /// before any is set, and the first one that fails is the error, naming its option; nothing changes then.
    pub fn apply(&mut self, changes: &Value, ctx: &ValueContext<'_>) -> Result<(), OptionError> {
        let sections = changes
            .as_object()
            .ok_or_else(|| OptionError::new("", "", "options must be an object by section and name"))?;
        let parsed = sections
            .iter()
            .map(|(section, names)| {
                let names = names
                    .as_object()
                    .ok_or_else(|| OptionError::new(section, "", "a section must be an object by option name"))?;
                names
                    .iter()
                    .map(|(name, json)| self.read(section, name, json, ctx))
                    .collect::<Result<Vec<_>, _>>()
            })
            .collect::<Result<Vec<_>, _>>()?;
        parsed.into_iter().flatten().for_each(|(index, value)| match value {
            Some(v) => self.options[index].set(v),
            None => self.options[index].reset(),
        });
        Ok(())
    }

    /// The index of `section`/`name` and the value `json` sets it to; `None` resets it.
    fn read(&self, section: &str, name: &str, json: &Value, ctx: &ValueContext<'_>) -> Result<(usize, Option<OptionValue>), OptionError> {
        let index = self
            .options
            .iter()
            .position(|o| o.section() == section && o.name() == name)
            .ok_or_else(|| OptionError::new(section, name, "unknown option"))?;
        if json.is_null() {
            return Ok((index, None));
        }
        let option = &self.options[index];
        let value =
            value::read(&option.kind, option.default_value(), json, ctx).map_err(|reason| OptionError::new(section, name, reason))?;
        Ok((index, Some(value)))
    }

    /// Every option's value in wire form, in order: the `options` of a report result.
    pub fn values(&self) -> OptionValues {
        self.collect(|_| true)
    }

    /// The options whose value differs from the default, in wire form: what a saved report configuration keeps
    /// (`5.17:bindings/guile/options.scm:141`).
    pub fn changed(&self) -> OptionValues {
        self.collect(ReportOption::is_changed)
    }

    fn collect(&self, keep: impl Fn(&ReportOption) -> bool) -> OptionValues {
        self.options().into_iter().filter(|o| keep(o)).fold(Map::new(), |mut map, o| {
            insert(&mut map, o.section(), o.name(), value::write(o.value()));
            map
        })
    }
}

impl ReportOption {
    /// The accounts an account option names when the report runs: its value; else its default; else, with allowed
    /// types, the first account of an allowed type in display order (`5.17:libgnucash/engine/gnc-option-impl.cpp:410-444`,
    /// `:483-522`). Empty for any other option.
    pub fn accounts(&self, book: &Book) -> Vec<Guid> {
        let (OptionKind::AccountList { types, .. } | OptionKind::AccountSel { types }) = &self.kind else {
            return Vec::new();
        };
        let chosen = |v: &OptionValue| match v {
            OptionValue::Accounts(list) => list.clone(),
            OptionValue::Account(one) => one.iter().copied().collect(),
            _ => Vec::new(),
        };
        [chosen(self.value()), chosen(self.default_value())]
            .into_iter()
            .find(|list| !list.is_empty())
            .or_else(|| first_of_types(book, types).map(|g| vec![g]))
            .unwrap_or_default()
    }
}

/// The first account below the root, depth first in display order, whose type is one of `types`.
fn first_of_types(book: &Book, types: &[AccountType]) -> Option<Guid> {
    if types.is_empty() {
        None
    } else {
        first_below(book, book.root, types)
    }
}

fn first_below(book: &Book, parent: Guid, types: &[AccountType]) -> Option<Guid> {
    sorted_children(book, parent).into_iter().find_map(|a| {
        if types.contains(&a.account_type) {
            Some(a.guid)
        } else {
            first_below(book, a.guid, types)
        }
    })
}

/// The default a declaration ends up with, or why it is malformed.
fn checked_default(kind: &OptionKind, default: OptionValue) -> Result<OptionValue, String> {
    let mismatch = || format!("a {} option cannot default to {default:?}", kind.type_name());
    match (kind, &default) {
        (OptionKind::Boolean, OptionValue::Bool(_))
        | (OptionKind::String | OptionKind::Text | OptionKind::Font | OptionKind::Color, OptionValue::Text(_))
        | (OptionKind::Internal, OptionValue::Bool(_) | OptionValue::Text(_))
        | (OptionKind::PlotSize, OptionValue::PlotSize(_))
        | (OptionKind::Commodity, OptionValue::Commodity(_))
        | (OptionKind::AccountSel { .. }, OptionValue::Account(_)) => Ok(default),
        (OptionKind::Currency, OptionValue::Commodity(c)) if c.is_currency() => Ok(default),
        (OptionKind::Currency, OptionValue::Commodity(c)) => Err(format!("{c} is not a currency")),
        (OptionKind::Multichoice { choices }, OptionValue::Key(key)) => multichoice_default(choices, key),
        (OptionKind::List { choices }, OptionValue::Keys(keys)) => list_default(choices, keys).map(|()| default),
        (OptionKind::NumberRange { min, max, .. }, OptionValue::Number(n)) => number_default(min, max, n),
        (OptionKind::Date { ui, relative }, OptionValue::Date(d)) => date_default(*ui, relative, *d).map(|()| default),
        (OptionKind::AccountList { multiple, .. }, OptionValue::Accounts(list)) if *multiple || list.len() <= 1 => Ok(default),
        (OptionKind::AccountList { .. }, OptionValue::Accounts(_)) => Err("a single-account option defaults to one account".to_owned()),
        _ => Err(mismatch()),
    }
}

fn distinct_keys(choices: &[Choice]) -> Result<(), String> {
    let repeated = choices
        .iter()
        .enumerate()
        .find(|(i, c)| choices[..*i].iter().any(|d| d.key == c.key));
    repeated.map_or(Ok(()), |(_, c)| Err(format!("choice {} is declared twice", c.key)))
}

fn multichoice_default(choices: &[Choice], key: &ChoiceKey) -> Result<OptionValue, String> {
    distinct_keys(choices)?;
    let first = choices.first().ok_or("a multichoice option needs choices")?;
    let key = if choices.iter().any(|c| c.key == *key) {
        key.clone()
    } else {
        first.key.clone()
    };
    Ok(OptionValue::Key(key))
}

fn list_default(choices: &[Choice], keys: &[ChoiceKey]) -> Result<(), String> {
    distinct_keys(choices)?;
    keys.iter().try_for_each(|k| {
        if choices.iter().any(|c| c.key == *k) {
            Ok(())
        } else {
            Err(format!("default {k} is not a choice"))
        }
    })
}

fn number_default(min: &Rational, max: &Rational, n: &Rational) -> Result<OptionValue, String> {
    if min > max {
        return Err(format!("the range is empty: {min} to {max}"));
    }
    Ok(OptionValue::Number(if n < min || n > max { min.clone() } else { n.clone() }))
}

fn date_default(ui: DateUi, allowed: &[RelativeDate], d: DateValue) -> Result<(), String> {
    match d {
        DateValue::Absolute(_) if ui == DateUi::Relative => Err("a relative-only date cannot default to an absolute date".to_owned()),
        DateValue::Absolute(t) => check_time(t)
            .map(drop)
            .map_err(|()| format!("{} is outside the supported range", t.0)),
        DateValue::Relative(_) if ui == DateUi::Absolute => Err("an absolute-only date cannot default to a relative date".to_owned()),
        DateValue::Relative(r) if allowed.is_empty() || allowed.contains(&r) => Ok(()),
        DateValue::Relative(r) => Err(format!(
            "default {} is not among the relative dates offered",
            super::model::relative_name(r)
        )),
    }
}
