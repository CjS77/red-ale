//! Style sheets (`design/reports.md` §8, "Style sheets"; §9): GnuCash 5.17's style sheet templates, their predefined
//! style sheets, user style sheets from the Configuration, and lookup by name.
//!
//! A [`StyleSheet`] is a named set of presentation options from a [`StyleSheetTemplate`]. [`StyleSheets`] holds the
//! six predefined ones, with any changes and user style sheets the Configuration supplies as [`StyleSheetEntry`]
//! values. Nothing here reads a file.

mod declare;

use std::fmt;

use red_ale_core::TimeZone;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use thiserror::Error;

use crate::error::OptionError;
use crate::options::{OptionDb, OptionValue, ReportOption, ValueContext};
use crate::result::OptionValues;

#[cfg(test)]
use declare::DEFAULT_CSS;
pub(crate) use declare::FONT_FAMILY;

/// A style sheet template: GnuCash 5.17's `gnc:define-html-style-sheet` records.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum StyleSheetTemplate {
    /// "Plain" (`5.17:gnucash/report/stylesheets/plain.scm:211-215`).
    Plain,
    /// "CSS" (`5.17:gnucash/report/stylesheets/css.scm:222-226`).
    Css,
    /// "Easy" (`5.17:gnucash/report/stylesheets/footer.scm:378-382`).
    Easy,
    /// "Fancy" (`:384-388`), the same options and renderer as Easy.
    Fancy,
    /// "Footer" (`:390-394`), the same options and renderer as Easy.
    Footer,
    /// "Head or Tail" (`5.17:gnucash/report/stylesheets/head-or-tail.scm:518-522`).
    HeadOrTail,
}

impl StyleSheetTemplate {
    /// Every template, sorted by name bytewise.
    pub const ALL: [Self; 6] = [Self::Css, Self::Easy, Self::Fancy, Self::Footer, Self::HeadOrTail, Self::Plain];

    /// The template's name, GnuCash's English string.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Plain => "Plain",
            Self::Css => "CSS",
            Self::Easy => "Easy",
            Self::Fancy => "Fancy",
            Self::Footer => "Footer",
            Self::HeadOrTail => "Head or Tail",
        }
    }

    /// The template named `name`.
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|t| t.name() == name)
    }

    /// The options a style sheet of this template has, at their defaults.
    pub fn options(self) -> OptionDb {
        declare::options(match self {
            Self::Plain => declare::Set::Plain,
            Self::Css => declare::Set::Css,
            Self::Easy | Self::Fancy | Self::Footer => declare::Set::Easy,
            Self::HeadOrTail => declare::Set::HeadOrTail,
        })
    }
}

impl fmt::Display for StyleSheetTemplate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// The name of the style sheet reports use by default (`5.17:gnucash/report/stylesheets/plain.scm:217-218`).
pub const DEFAULT_STYLE_SHEET: &str = "Default";

/// The predefined style sheets and their templates (`5.17:gnucash/report/stylesheets/plain.scm:218`,
/// `css.scm:228`, `footer.scm:396-398`, `head-or-tail.scm:524`), sorted by name bytewise.
pub const PREDEFINED: [(&str, StyleSheetTemplate); 6] = [
    ("CSS-based stylesheet (experimental)", StyleSheetTemplate::Css),
    (DEFAULT_STYLE_SHEET, StyleSheetTemplate::Plain),
    ("Easy", StyleSheetTemplate::Easy),
    ("Footer", StyleSheetTemplate::Footer),
    ("Head or Tail", StyleSheetTemplate::HeadOrTail),
    ("Technicolor", StyleSheetTemplate::Fancy),
];

/// GnuCash options red-ale leaves out (`design/reports.md` §8, "Left out"): images and the banner's alignment. A value
/// for one is accepted and has no effect.
pub fn left_out(section: &str, name: &str) -> bool {
    section == "Images" || (section == "General" && name == "Background Pixmap")
}

/// Text in the CSS template's option that would end the `<style>` element or load something
/// (`design/reports.md` §8): GnuCash falls back to its default CSS on `</style`
/// (`5.17:gnucash/report/stylesheets/css.scm:204-209`); red-ale refuses it, and also refuses what could fetch a
/// resource: `@import`, `url(`, `image(`, `image-set(`, `src(`, and backslash escapes, which could spell any of them.
pub fn check_css(css: &str) -> Result<(), String> {
    const REFUSED: [&str; 7] = ["</style", "@import", "url(", "image(", "image-set(", "src(", "\\"];
    let lower = css.to_ascii_lowercase();
    REFUSED
        .iter()
        .find(|r| lower.contains(*r))
        .map_or(Ok(()), |r| Err(format!("CSS may not contain {r:?}")))
}

/// A style sheet: a name, a template and that template's options with their values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StyleSheet {
    name: String,
    template: StyleSheetTemplate,
    options: OptionDb,
}

impl StyleSheet {
    /// A style sheet of `template` with every option at its default.
    pub fn new(name: impl Into<String>, template: StyleSheetTemplate) -> Self {
        Self {
            name: name.into(),
            template,
            options: template.options(),
        }
    }

    /// The style sheet a Configuration entry describes.
    pub fn from_entry(entry: &StyleSheetEntry) -> Result<Self, StyleSheetError> {
        let template = StyleSheetTemplate::from_name(&entry.template).ok_or_else(|| StyleSheetError::UnknownTemplate {
            sheet: entry.name.clone(),
            template: entry.template.clone(),
        })?;
        let mut sheet = Self::new(entry.name.clone(), template);
        sheet
            .apply(&Value::Object(entry.options.clone()))
            .map_err(|error| StyleSheetError::Option {
                sheet: entry.name.clone(),
                error,
            })?;
        Ok(sheet)
    }

    /// The name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The template.
    pub fn template(&self) -> StyleSheetTemplate {
        self.template
    }

    /// Whether the name is one of the six predefined style sheets.
    pub fn is_predefined(&self) -> bool {
        PREDEFINED.iter().any(|(name, _)| *name == self.name)
    }

    /// The options, with their values.
    pub fn options(&self) -> &OptionDb {
        &self.options
    }

    /// Change options, by section and name in wire form, as [`OptionDb::apply`] does; `null` returns an option to its
    /// default. Options red-ale leaves out (images) are accepted and ignored. CSS text that [`check_css`] refuses is a
    /// `report-option` error. Nothing changes unless every value is valid.
    pub fn apply(&mut self, changes: &Value) -> Result<(), OptionError> {
        let mut options = self.options.clone();
        options.apply(
            &without_left_out(changes),
            &ValueContext {
                book: None,
                zone: &TimeZone::utc(),
            },
        )?;
        if let Some(OptionValue::Text(css)) = options.get("General", "CSS").map(ReportOption::value) {
            check_css(css).map_err(|reason| OptionError::new("General", "CSS", reason))?;
        }
        self.options = options;
        Ok(())
    }

    /// The options that differ from the template's defaults, in wire form: what the Configuration keeps.
    pub fn changed(&self) -> OptionValues {
        self.options.changed()
    }

    /// The Configuration entry for this style sheet.
    pub fn to_entry(&self) -> StyleSheetEntry {
        StyleSheetEntry {
            name: self.name.clone(),
            template: self.template.name().to_owned(),
            options: self.changed(),
        }
    }
}

/// `changes` without the options red-ale leaves out; anything that is not an object by section is passed on for
/// [`OptionDb::apply`] to refuse.
fn without_left_out(changes: &Value) -> Value {
    let Some(sections) = changes.as_object() else {
        return changes.clone();
    };
    let kept = sections
        .iter()
        .filter(|(section, _)| !left_out(section, ""))
        .map(|(section, names)| {
            let names = match names.as_object() {
                Some(names) => Value::Object(
                    names
                        .iter()
                        .filter(|(n, _)| !left_out(section, n))
                        .map(|(n, v)| (n.clone(), v.clone()))
                        .collect(),
                ),
                None => names.clone(),
            };
            (section.clone(), names)
        });
    Value::Object(kept.collect::<Map<String, Value>>())
}

/// A style sheet as the Configuration keeps it, under `reports.styleSheets` (`design/reports.md` §9): its name, its
/// template's name and the options that differ from the template's defaults.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct StyleSheetEntry {
    /// The style sheet's name.
    pub name: String,
    /// The template's name: `Plain`, `CSS`, `Easy`, `Fancy`, `Footer` or `Head or Tail`.
    pub template: String,
    /// Option values by section and name, in wire form.
    #[serde(default)]
    pub options: OptionValues,
}

/// A Configuration entry that does not make a style sheet.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum StyleSheetError {
    /// The entry names a template that does not exist.
    #[error("style sheet {sheet:?}: no template is called {template:?}")]
    UnknownTemplate {
        /// The style sheet.
        sheet: String,
        /// The template it names.
        template: String,
    },
    /// An option value is not valid.
    #[error("style sheet {sheet:?}: option {}/{}: {}", .error.section, .error.name, .error.reason)]
    Option {
        /// The style sheet.
        sheet: String,
        /// What is wrong.
        error: OptionError,
    },
}

/// The style sheets a report can name: the six predefined ones, then the Configuration's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StyleSheets {
    /// Sorted by name, bytewise; names are unique.
    sheets: Vec<StyleSheet>,
}

impl Default for StyleSheets {
    fn default() -> Self {
        Self::predefined()
    }
}

impl StyleSheets {
    /// The six predefined style sheets at their templates' defaults.
    pub fn predefined() -> Self {
        Self {
            sheets: PREDEFINED
                .iter()
                .map(|(name, template)| StyleSheet::new(*name, *template))
                .collect(),
        }
    }

    /// The predefined style sheets with the Configuration's entries applied in order. An entry with a predefined
    /// name replaces that style sheet, as GnuCash restores its saved sheets over the predefined ones
    /// (`5.17:gnucash/report/gnc-report.cpp:81-111`); an entry with a new name adds a user style sheet; a later
    /// entry with the same name replaces an earlier one.
    pub fn from_configuration(entries: &[StyleSheetEntry]) -> Result<Self, StyleSheetError> {
        entries.iter().try_fold(Self::predefined(), |mut sheets, entry| {
            sheets.insert(StyleSheet::from_entry(entry)?);
            Ok(sheets)
        })
    }

    /// Add `sheet`, replacing any style sheet of the same name, which is returned.
    pub fn insert(&mut self, sheet: StyleSheet) -> Option<StyleSheet> {
        match self.sheets.binary_search_by(|s| s.name.as_str().cmp(&sheet.name)) {
            Ok(i) => Some(std::mem::replace(&mut self.sheets[i], sheet)),
            Err(i) => {
                self.sheets.insert(i, sheet);
                None
            }
        }
    }

    /// The style sheet called `name`. `None` when there is none: GnuCash's lookup has no fallback, and the report
    /// renders with no style sheet (`5.17:gnucash/report/report-core.scm:457-462`), which [`super::html`] does when
    /// given `None`.
    pub fn get(&self, name: &str) -> Option<&StyleSheet> {
        self.sheets
            .binary_search_by(|s| s.name.as_str().cmp(name))
            .ok()
            .and_then(|i| self.sheets.get(i))
    }

    /// Every style sheet, sorted by name bytewise.
    pub fn iter(&self) -> impl Iterator<Item = &StyleSheet> {
        self.sheets.iter()
    }

    /// Every name, sorted bytewise: the choices of a report's General/"Stylesheet" option.
    pub fn names(&self) -> Vec<&str> {
        self.sheets.iter().map(StyleSheet::name).collect()
    }
}

#[cfg(test)]
mod tests;
