//! What a style sheet's options mean for red-ale's look (`design/reports.md` §8, "What the options do"): colours,
//! table spacing, fonts, links, and the header and footer lines, read once from the style sheet.

use super::sheet::{StyleSheet, StyleSheetTemplate, check_css};
use super::text::font_css;
use crate::number::Rational;
use crate::options::{OptionDb, OptionValue, ReportOption};

/// A line of the header or footer a style sheet adds around the report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Line {
    /// "Prepared by: <name>".
    PreparedBy(String),
    /// "Prepared for: <name>".
    PreparedFor(String),
    /// The run's date under a label, and its time when asked for.
    Date { label: &'static str, time: bool },
    /// red-ale's version, where GnuCash shows its own.
    Version,
    /// Free text: a footer or additional comments.
    Note(String),
}

/// Row and page colours, each `#rrggbb`, or absent for none.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct Colors {
    pub background: Option<String>,
    pub text: Option<String>,
    pub link: Option<String>,
    pub cell: Option<String>,
    pub alternate: Option<String>,
    pub subheading: Option<String>,
    pub subsubheading: Option<String>,
    pub grand_total: Option<String>,
}

/// The font options, as CSS declarations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Fonts {
    pub title: String,
    pub account_link: String,
    pub number_cell: String,
    pub number_header: String,
    pub text_cell: String,
    pub total_number: String,
    pub total_label: String,
    pub centered_label: String,
}

/// A style sheet's options as the renderer uses them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Look {
    /// The template, for the body's class; `None` without a style sheet.
    pub template: Option<StyleSheetTemplate>,
    pub colors: Colors,
    /// Table cell spacing, padding and border width, as CSS lengths.
    pub spacing: String,
    pub padding: String,
    pub border: String,
    pub fonts: Fonts,
    pub negative_red: bool,
    /// Whether cells carry `data-account` and `data-link`.
    pub links: bool,
    /// Lines above the report, under the title.
    pub header: Vec<Line>,
    /// Lines below the report.
    pub footer: Vec<Line>,
    /// The CSS template's text, appended to the base stylesheet.
    pub css: Option<String>,
}

impl Look {
    /// The look of `sheet`; without one, the base look: the Plain template's defaults with no colours, as GnuCash
    /// renders a report whose style sheet is missing with none (`5.17:gnucash/report/html-document.scm:154-160`).
    pub fn of(sheet: Option<&StyleSheet>) -> Self {
        let Some(sheet) = sheet else {
            let mut look = Self::read(&StyleSheetTemplate::Plain.options(), None);
            look.colors = Colors::default();
            return look;
        };
        Self::read(sheet.options(), Some(sheet.template()))
    }

    fn read(db: &OptionDb, template: Option<StyleSheetTemplate>) -> Self {
        let o = Options(db);
        let defaults = StyleSheetTemplate::Plain.options();
        // The CSS template has no fonts or tables; the base look's stand in for them.
        let fonts_from = if db.get("Fonts", "Title").is_some() { db } else { &defaults };
        let tables = if db.get("Tables", "Table cell padding").is_some() { db } else { &defaults };
        Self {
            template,
            colors: colors(&o, template),
            spacing: Options(tables).length("Table cell spacing"),
            padding: Options(tables).length("Table cell padding"),
            border: Options(tables).length("Table border width"),
            fonts: fonts(&Options(fonts_from)),
            negative_red: Options(fonts_from).flag("Fonts", "Negative Values in Red"),
            links: o.flag_or("General", "Enable Links", true),
            header: lines(&o, template, false),
            footer: lines(&o, template, true),
            // A style sheet never holds refused CSS; checking again costs little and keeps the page closed.
            css: o.text("General", "CSS").filter(|css| check_css(css).is_ok()).map(str::to_owned),
        }
    }
}

/// Reads options by section and name; a missing option reads as unset.
struct Options<'a>(&'a OptionDb);

impl Options<'_> {
    fn value(&self, section: &str, name: &str) -> Option<&OptionValue> {
        self.0.get(section, name).map(ReportOption::value)
    }

    fn text(&self, section: &str, name: &str) -> Option<&str> {
        match self.value(section, name) {
            Some(OptionValue::Text(s)) => Some(s),
            _ => None,
        }
    }

    fn flag_or(&self, section: &str, name: &str, default: bool) -> bool {
        match self.value(section, name) {
            Some(OptionValue::Bool(b)) => *b,
            _ => default,
        }
    }

    fn flag(&self, section: &str, name: &str) -> bool {
        self.flag_or(section, name, false)
    }

    /// A colour as `gnc:color->html` writes it, `#` and the first six digits (`5.17:bindings/guile/options.scm:72-77`),
    /// or `None` when it is not a colour, which is what a browser makes of GnuCash's invalid ones.
    fn color(&self, section: &str, name: &str) -> Option<String> {
        let hex = self.text(section, name)?;
        let valid = matches!(hex.len(), 6 | 8) && hex.bytes().all(|b| b.is_ascii_hexdigit());
        valid.then(|| format!("#{}", hex[..6].to_ascii_lowercase()))
    }

    /// A Tables option in pixels.
    fn length(&self, name: &str) -> String {
        match self.value("Tables", name) {
            Some(OptionValue::Number(n)) => format!("{}px", pixels(n)),
            _ => "0px".to_owned(),
        }
    }

    fn font(&self, name: &str) -> String {
        font_css(self.text("Fonts", name).unwrap_or_default())
    }
}

/// A length, which the option's range keeps between 0 and 20, to hundredths of a pixel, half-even.
fn pixels(n: &Rational) -> String {
    let hundredths = n.abs().round_to(100, crate::number::Rounding::Bankers).unwrap_or_else(|_| Rational::zero());
    hundredths.to_exact_decimal().unwrap_or_else(|| "0".to_owned())
}

fn colors(o: &Options<'_>, template: Option<StyleSheetTemplate>) -> Colors {
    match template {
        // Plain colours the page and, with the same colour, subheading and grand-total rows
        // (`5.17:gnucash/report/stylesheets/plain.scm:104-167`).
        Some(StyleSheetTemplate::Plain) => {
            let background = o.color("General", "Background Color");
            Colors {
                background: background.clone(),
                alternate: o.color("Colors", "Alternate Table Cell Color"),
                subheading: background.clone(),
                grand_total: background,
                ..Colors::default()
            }
        }
        Some(StyleSheetTemplate::Css) | None => Colors::default(),
        Some(_) => Colors {
            background: o.color("Colors", "Background Color"),
            text: o.color("Colors", "Text Color"),
            link: o.color("Colors", "Link Color"),
            cell: o.color("Colors", "Table Cell Color"),
            alternate: o.color("Colors", "Alternate Table Cell Color"),
            subheading: o.color("Colors", "Subheading/Subtotal Cell Color"),
            subsubheading: o.color("Colors", "Sub-subheading/total Cell Color"),
            grand_total: o.color("Colors", "Grand Total Cell Color"),
        },
    }
}

fn fonts(o: &Options<'_>) -> Fonts {
    Fonts {
        title: o.font("Title"),
        account_link: o.font("Account link"),
        number_cell: o.font("Number cell"),
        number_header: o.font("Number header"),
        text_cell: o.font("Text cell"),
        total_number: o.font("Total number cell"),
        total_label: o.font("Total label cell"),
        centered_label: o.font("Centered label cell"),
    }
}

/// The header (`bottom` false) or footer lines. Easy, Fancy and Footer put the preparer lines and the date under the
/// title when "Show preparer info" is on, and the footer text below the report
/// (`5.17:gnucash/report/stylesheets/footer.scm:316-372`). Head or Tail places each item above or below by its "… at
/// bottom" option (`5.17:gnucash/report/stylesheets/head-or-tail.scm:380-515`). Empty names and texts are left out.
fn lines(o: &Options<'_>, template: Option<StyleSheetTemplate>, bottom: bool) -> Vec<Line> {
    let text = |name: &str| o.text("General", name).filter(|s| !s.trim().is_empty()).map(str::to_owned);
    match template {
        Some(StyleSheetTemplate::Easy | StyleSheetTemplate::Fancy | StyleSheetTemplate::Footer) => {
            if bottom {
                text("Footer").map(Line::Note).into_iter().collect()
            } else if o.flag("General", "Show preparer info") {
                let date = Some(Line::Date { label: "Date: ", time: false });
                [text("Preparer").map(Line::PreparedBy), text("Prepared for").map(Line::PreparedFor), date].into_iter().flatten().collect()
            } else {
                Vec::new()
            }
        }
        Some(StyleSheetTemplate::HeadOrTail) => {
            let here = |show: &str, at_bottom: &str| o.flag("General", show) && o.flag("General", at_bottom) == bottom;
            let comments_here = o.flag("General", "Show comments at bottom") == bottom;
            let time = o.flag("General", "Show time in addition to date");
            [
                here("Show preparer info", "Show preparer info at bottom").then(|| text("Preparer").map(Line::PreparedBy)).flatten(),
                here("Show receiver info", "Show receiver info at bottom").then(|| text("Prepared for").map(Line::PreparedFor)).flatten(),
                here("Show date", "Show date/time at bottom").then_some(Line::Date { label: "Report Creation Date: ", time }),
                here("Show GnuCash Version", "Show GnuCash version at bottom").then_some(Line::Version),
                comments_here.then(|| text("Additional Comments").map(Line::Note)).flatten(),
            ]
            .into_iter()
            .flatten()
            .collect()
        }
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn the_base_look_has_no_colours_and_default_fonts() {
        let base = Look::of(None);
        assert_eq!((base.template, base.colors.clone()), (None, Colors::default()));
        assert_eq!((base.padding.as_str(), base.spacing.as_str(), base.border.as_str()), ("4px", "0px", "0px"));
        assert!(base.links && base.negative_red && base.header.is_empty() && base.css.is_none());
        assert_eq!(base.fonts.title, "font-family:\"Noto Sans\",sans-serif;font-size:15pt;font-weight:bold;");
    }

    #[test]
    fn invalid_colours_set_no_colour() {
        let tail = Look::of(Some(&StyleSheet::new("Head or Tail", StyleSheetTemplate::HeadOrTail)));
        assert_eq!(tail.colors.subheading, None);
        assert_eq!(tail.colors.grand_total.as_deref(), Some("#ffff00"));
        assert_eq!(tail.header, [Line::Date { label: "Report Creation Date: ", time: true }, Line::Version]);
        assert!(tail.footer.is_empty());
    }

    #[test]
    fn head_or_tail_moves_items_to_the_bottom() {
        let mut sheet = StyleSheet::new("Head or Tail", StyleSheetTemplate::HeadOrTail);
        sheet
            .apply(&json!({ "General": {
                "Preparer": "Ann", "Prepared for": "Bob", "Additional Comments": "Draft",
                "Show date/time at bottom": true, "Show comments at bottom": true, "Show time in addition to date": false,
                "Show GnuCash Version": false
            }, "Colors": { "Background Color": "f0f0f0ff" } }))
            .unwrap();
        let look = Look::of(Some(&sheet));
        assert_eq!(look.header, [Line::PreparedBy("Ann".into()), Line::PreparedFor("Bob".into())]);
        assert_eq!(look.footer, [Line::Date { label: "Report Creation Date: ", time: false }, Line::Note("Draft".into())]);
        assert_eq!(look.colors.background.as_deref(), Some("#f0f0f0"));
    }

    #[test]
    fn the_css_template_borrows_the_base_fonts_and_tables() {
        let look = Look::of(Some(&StyleSheet::new("CSS-based stylesheet (experimental)", StyleSheetTemplate::Css)));
        assert_eq!(look.padding, "4px");
        assert!(look.css.is_some_and(|c| c.starts_with("/* default style */")));
        let mut easy = StyleSheet::new("Easy", StyleSheetTemplate::Easy);
        easy.apply(&json!({ "Tables": { "Table cell padding": "2.5" }, "General": { "Enable Links": false } })).unwrap();
        let look = Look::of(Some(&easy));
        assert_eq!((look.padding.as_str(), look.links), ("2.5px", false));
    }
}
