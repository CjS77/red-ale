//! The options each style sheet template declares: GnuCash 5.17's sections, names, keys, help texts and defaults,
//! minus the image options milestone 2 leaves out (`design/reports.md` §8, "Style sheets").

use crate::error::OptionError;
use crate::number::Rational;
use crate::options::{Classifier, OptionDb};

/// The default font family. GnuCash takes the GTK font, or Arial without a GUI
/// (`5.17:gnucash/report/gnc-report.cpp:302-331`); red-ale has no GUI and uses its own.
pub(crate) const FONT_FAMILY: &str = "Noto Sans";

/// The CSS template's default text (`default-css`, `5.17:gnucash/report/stylesheets/css.scm:31-112`).
pub(crate) const DEFAULT_CSS: &str = r#"/* default style */
@media (prefers-color-scheme: dark) {
    body {
        color: #000; background-color: #fff;
    }
}

html, body {
    height: 100vh;
    margin-top: 0px;
    margin-bottom: 0px;
    margin-left: 8px;
    margin-right: 8px;
    font-family: "Noto Sans", Sans-Serif;
    font-size: 10pt;
}

body, p, table, tr, td, a, th {
    vertical-align: top;
}

h3 {
    font-size: 15pt;
    font-weight: bold;
}

a {
    font-style: italic;
}

/* table elements as follows */
td, th {
    padding:4px;
}

tr.alternate-row {
    background: #ffffff
}

tr {
    page-break-inside: avoid !important;
}

td, th {
    border-color: grey
}

td.total-number-cell, td.total-label-cell, td.centered-label-cell {
    font-size: 12pt;
    font-weight: bold;
}

th.column-heading-left {
    text-align: left;
}

td.centered-label-cell, th.column-heading-center {
    text-align: center;
}

td.number-header, th.column-heading-right, td.number-cell, td.total-number-cell {
    text-align: right;
}

td.neg {
    color: red;
}

td.number-cell, td.total-number-cell, td.anchor-cell, td.date-cell {
    white-space: nowrap;
}

td.highlight {
    background-color: #e1e1e1
}

@media print {
    html, body {
        height: unset;
    }
}
"#;

/// An option a template declares.
enum Kind {
    Color(&'static str),
    Boolean(bool),
    String(&'static str),
    Text(&'static str),
    /// A font: the default family followed by this style and size.
    Font(&'static str),
    /// A number from 0 to 20 in steps of 1, as every table option is.
    Range(i64),
}

struct Decl(&'static str, &'static str, &'static str, &'static str, Kind);

impl Decl {
    fn register(&self, db: &mut OptionDb) -> Result<(), OptionError> {
        let Self(section, name, key, help, kind) = self;
        let c = Classifier::new(*section, *name, *key, *help);
        match kind {
            Kind::Color(d) => db.color(c, d),
            Kind::Boolean(d) => db.boolean(c, *d),
            Kind::String(d) => db.string(c, d),
            Kind::Text(d) => db.text(c, d),
            Kind::Font(style) => db.font(c, &format!("{FONT_FAMILY} {style}")),
            Kind::Range(d) => db.number_range(c, Rational::from_int(*d), Rational::zero(), Rational::from_int(20), Rational::one()),
        }
    }
}

/// Plain (`5.17:gnucash/report/stylesheets/plain.scm:38-72`), without "Background Pixmap".
const PLAIN: &[Decl] = &[
    Decl("General", "Background Color", "a", "Background color for reports.", Kind::Color("ffffff")),
    Decl("General", "Enable Links", "c", "Enable hyperlinks in reports.", Kind::Boolean(true)),
    Decl("Colors", "Alternate Table Cell Color", "a", "Background color for alternate lines.", Kind::Color("ffffff")),
    Decl("Tables", "Table cell spacing", "a", "Space between table cells.", Kind::Range(0)),
    Decl("Tables", "Table cell padding", "b", "Space between table cell edge and content.", Kind::Range(4)),
    Decl("Tables", "Table border width", "c", "Bevel depth on tables.", Kind::Range(0)),
];

/// CSS (`5.17:gnucash/report/stylesheets/css.scm:114-122`).
const CSS: &[Decl] =
    &[Decl("General", "CSS", "a", "CSS code. This field specifies the CSS code for styling reports.", Kind::Text(DEFAULT_CSS))];

/// Easy, Fancy and Footer share one generator (`5.17:gnucash/report/stylesheets/footer.scm:45-160`); the Images section
/// is left out.
const EASY_GENERAL: &[Decl] = &[
    Decl("General", "Preparer", "a", "Name of person preparing the report.", Kind::String("")),
    Decl("General", "Prepared for", "b", "Name of organization or company prepared for.", Kind::String("")),
    Decl("General", "Show preparer info", "c", "Name of organization or company.", Kind::Boolean(false)),
    Decl("General", "Enable Links", "d", "Enable hyperlinks in reports.", Kind::Boolean(true)),
    Decl("General", "Footer", "e", "String to be placed as a footer.", Kind::Text("")),
];

/// Head or Tail's General section (`5.17:gnucash/report/stylesheets/head-or-tail.scm:48-118`), including its two
/// options with key `m`.
const HEAD_OR_TAIL_GENERAL: &[Decl] = &[
    Decl("General", "Preparer", "a", "Name of person preparing the report.", Kind::String("")),
    Decl("General", "Prepared for", "b", "Name of organization or company prepared for.", Kind::String("")),
    Decl("General", "Show preparer info", "c", "Name of organization or company.", Kind::Boolean(true)),
    Decl("General", "Show receiver info", "d", "Name of organization or company the report is prepared for.", Kind::Boolean(true)),
    Decl("General", "Show date", "e", "The creation date for this report.", Kind::Boolean(true)),
    Decl(
        "General",
        "Show time in addition to date",
        "f",
        "The creation time for this report can only be shown if the date is shown.",
        Kind::Boolean(true),
    ),
    Decl("General", "Show GnuCash Version", "g", "Show the currently used GnuCash version.", Kind::Boolean(true)),
    Decl("General", "Enable Links", "h", "Enable hyperlinks in reports.", Kind::Boolean(true)),
    Decl("General", "Additional Comments", "i", "String for additional report information.", Kind::Text("")),
    Decl(
        "General",
        "Show preparer info at bottom",
        "j",
        "Per default the preparer info will be shown before the report data.",
        Kind::Boolean(false),
    ),
    Decl(
        "General",
        "Show receiver info at bottom",
        "k",
        "Per default the receiver info will be shown before the report data.",
        Kind::Boolean(false),
    ),
    Decl(
        "General",
        "Show date/time at bottom",
        "l",
        "Per default the date/time info will be shown before the report data.",
        Kind::Boolean(false),
    ),
    Decl(
        "General",
        "Show comments at bottom",
        "m",
        "Per default the additional comments text will be shown before the report data.",
        Kind::Boolean(false),
    ),
    Decl(
        "General",
        "Show GnuCash version at bottom",
        "m",
        "Per default the GnuCash version will be shown before the report data.",
        Kind::Boolean(false),
    ),
];

/// The Colors section of Easy, Fancy, Footer (`5.17:gnucash/report/stylesheets/footer.scm:102-140`) and Head or Tail
/// (`5.17:gnucash/report/stylesheets/head-or-tail.scm:146-182`), which differ only in the subheading default:
/// Head or Tail's is the misspelt `"eexe8xaa"`, kept as GnuCash has it.
fn colors(subheading: &'static str) -> [Decl; 8] {
    [
        Decl("Colors", "Background Color", "a", "General background color for report.", Kind::Color("ffffff")),
        Decl("Colors", "Text Color", "b", "Normal body text color.", Kind::Color("000000")),
        Decl("Colors", "Link Color", "c", "Link text color.", Kind::Color("b22222")),
        Decl("Colors", "Table Cell Color", "c", "Default background for table cells.", Kind::Color("ffffff")),
        Decl("Colors", "Alternate Table Cell Color", "d", "Default alternate background for table cells.", Kind::Color("ffffff")),
        Decl("Colors", "Subheading/Subtotal Cell Color", "e", "Default color for subtotal rows.", Kind::Color(subheading)),
        Decl("Colors", "Sub-subheading/total Cell Color", "f", "Color for subsubtotals.", Kind::Color("fafad2")),
        Decl("Colors", "Grand Total Cell Color", "g", "Color for grand totals.", Kind::Color("ffff00")),
    ]
}

/// The Tables section of Easy, Fancy, Footer and Head or Tail: every width 1.
const TABLES_ONE: &[Decl] = &[
    Decl("Tables", "Table cell spacing", "a", "Space between table cells.", Kind::Range(1)),
    Decl("Tables", "Table cell padding", "b", "Space between table cell edge and content.", Kind::Range(1)),
    Decl("Tables", "Table border width", "c", "Bevel depth on tables.", Kind::Range(1)),
];

/// `register-font-options` (`5.17:gnucash/report/html-fonts.scm:100-142`).
const FONTS: &[Decl] = &[
    Decl("Fonts", "Title", "a", "Font info for the report title.", Kind::Font("Bold 15")),
    Decl("Fonts", "Account link", "b", "Font info for account name.", Kind::Font("Italic 10")),
    Decl("Fonts", "Number cell", "c", "Font info for regular number cells.", Kind::Font("10")),
    Decl("Fonts", "Negative Values in Red", "d", "Display negative values in red.", Kind::Boolean(true)),
    Decl("Fonts", "Number header", "e", "Font info for number headers.", Kind::Font("10")),
    Decl("Fonts", "Text cell", "f", "Font info for regular text cells.", Kind::Font("10")),
    Decl("Fonts", "Total number cell", "g", "Font info for number cells containing a total.", Kind::Font("Bold 12")),
    Decl("Fonts", "Total label cell", "h", "Font info for cells containing total labels.", Kind::Font("Bold 12")),
    Decl("Fonts", "Centered label cell", "i", "Font info for centered label cells.", Kind::Font("Bold 12")),
];

/// Which declarations a template makes.
#[derive(Debug, Clone, Copy)]
pub(super) enum Set {
    Plain,
    Css,
    Easy,
    HeadOrTail,
}

/// The declared options of a set, in declaration order. The declarations are constants that a test registers, so a
/// failure here cannot happen; it would leave the template without options rather than panic.
pub(super) fn options(set: Set) -> OptionDb {
    let (easy_colors, tail_colors) = (colors("eee8aa"), colors("eexe8xaa"));
    let groups: Vec<&[Decl]> = match set {
        Set::Plain => vec![PLAIN, FONTS],
        Set::Css => vec![CSS],
        Set::Easy => vec![EASY_GENERAL, &easy_colors, TABLES_ONE, FONTS],
        Set::HeadOrTail => vec![HEAD_OR_TAIL_GENERAL, &tail_colors, TABLES_ONE, FONTS],
    };
    let mut db = OptionDb::new();
    let declared = groups.into_iter().flatten().try_for_each(|d| d.register(&mut db));
    declared.map(|()| db).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_set_declares_without_error() {
        let declare = |groups: &[&[Decl]]| {
            let mut db = OptionDb::new();
            groups.iter().copied().flatten().try_for_each(|d| d.register(&mut db)).map(|()| db.options().len())
        };
        assert_eq!(declare(&[PLAIN, FONTS]), Ok(15));
        assert_eq!(declare(&[CSS]), Ok(1));
        assert_eq!(declare(&[EASY_GENERAL, &colors("eee8aa"), TABLES_ONE, FONTS]), Ok(25));
        assert_eq!(declare(&[HEAD_OR_TAIL_GENERAL, &colors("eexe8xaa"), TABLES_ONE, FONTS]), Ok(34));
        assert_eq!(options(Set::HeadOrTail).options().len(), 34);
    }
}
