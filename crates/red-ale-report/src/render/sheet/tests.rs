use serde_json::json;

use super::*;
use crate::options::ChoiceKey;

fn keys(sheet: &StyleSheet) -> Vec<(String, String, String)> {
    sheet.options().options().iter().map(|o| (o.section().to_owned(), o.name().to_owned(), o.classifier.key.clone())).collect()
}

fn value(sheet: &StyleSheet, section: &str, name: &str) -> OptionValue {
    sheet.options().get(section, name).unwrap().value().clone()
}

#[test]
fn templates_and_predefined_style_sheets_are_gnucash_s() {
    let names: Vec<&str> = StyleSheetTemplate::ALL.iter().map(|t| t.name()).collect();
    assert_eq!(names, ["CSS", "Easy", "Fancy", "Footer", "Head or Tail", "Plain"]);
    assert_eq!(StyleSheetTemplate::from_name("Head or Tail"), Some(StyleSheetTemplate::HeadOrTail));
    assert_eq!(StyleSheetTemplate::from_name("plain"), None);
    let sheets = StyleSheets::predefined();
    let listed: Vec<(&str, &str)> = sheets.iter().map(|s| (s.name(), s.template().name())).collect();
    assert_eq!(
        listed,
        [
            ("CSS-based stylesheet (experimental)", "CSS"),
            ("Default", "Plain"),
            ("Easy", "Easy"),
            ("Footer", "Footer"),
            ("Head or Tail", "Head or Tail"),
            ("Technicolor", "Fancy")
        ]
    );
    assert!(sheets.iter().all(StyleSheet::is_predefined));
}

#[test]
fn plain_declares_5_17_options_without_the_pixmap() {
    let plain = StyleSheet::new("Default", StyleSheetTemplate::Plain);
    let k = keys(&plain);
    assert_eq!(k[0], ("Colors".into(), "Alternate Table Cell Color".into(), "a".into()));
    assert_eq!(k[1..10].iter().map(|(_, n, _)| n.as_str()).collect::<Vec<_>>(), [
        "Title",
        "Account link",
        "Number cell",
        "Negative Values in Red",
        "Number header",
        "Text cell",
        "Total number cell",
        "Total label cell",
        "Centered label cell"
    ]);
    assert_eq!(k[10..12].iter().map(|(_, n, _)| n.as_str()).collect::<Vec<_>>(), ["Background Color", "Enable Links"]);
    assert_eq!(value(&plain, "Tables", "Table cell padding"), OptionValue::Number(crate::Rational::from_int(4)));
    assert_eq!(value(&plain, "Fonts", "Title"), OptionValue::Text("Noto Sans Bold 15".into()));
    assert!(plain.options().get("General", "Background Pixmap").is_none());
}

#[test]
fn easy_family_and_head_or_tail_keep_gnucash_s_quirks() {
    let easy = StyleSheet::new("Easy", StyleSheetTemplate::Easy);
    let colors: Vec<(String, String)> =
        keys(&easy).into_iter().filter(|(s, _, _)| s == "Colors").map(|(_, n, k)| (n, k)).collect();
    // "Link Color" and "Table Cell Color" share key c and keep declaration order.
    assert_eq!(colors[2], ("Link Color".into(), "c".into()));
    assert_eq!(colors[3], ("Table Cell Color".into(), "c".into()));
    assert_eq!(value(&easy, "Colors", "Subheading/Subtotal Cell Color"), OptionValue::Text("eee8aa".into()));
    assert_eq!(value(&easy, "General", "Show preparer info"), OptionValue::Bool(false));
    assert_eq!(StyleSheetTemplate::Fancy.options(), StyleSheetTemplate::Footer.options());
    let tail = StyleSheet::new("Head or Tail", StyleSheetTemplate::HeadOrTail);
    assert_eq!(value(&tail, "Colors", "Subheading/Subtotal Cell Color"), OptionValue::Text("eexe8xaa".into()));
    assert_eq!(value(&tail, "General", "Show preparer info"), OptionValue::Bool(true));
    assert_eq!(tail.options().options().len(), 34);
    assert!(tail.options().options().iter().all(|o| o.section() != "Images"));
}

#[test]
fn applying_options_checks_values_and_ignores_images() {
    let mut sheet = StyleSheet::new("Mine", StyleSheetTemplate::Footer);
    let changes = json!({
        "General": { "Preparer": "A. Bookkeeper", "Show preparer info": true },
        "Colors": { "Grand Total Cell Color": "#AABBCC" },
        "Images": { "Logo": "/home/me/logo.png", "Heading Alignment": "center" }
    });
    sheet.apply(&changes).unwrap();
    assert_eq!(value(&sheet, "Colors", "Grand Total Cell Color"), OptionValue::Text("aabbcc".into()));
    assert_eq!(sheet.changed(), json!({
        "Colors": { "Grand Total Cell Color": "aabbcc" },
        "General": { "Preparer": "A. Bookkeeper", "Show preparer info": true }
    }).as_object().unwrap().clone());
    let before = sheet.clone();
    let bad = json!({ "General": { "Preparer": "B" }, "Tables": { "Table border width": 21 } });
    let e = sheet.apply(&bad).unwrap_err();
    assert_eq!((e.section.as_str(), e.name.as_str()), ("Tables", "Table border width"));
    assert_eq!(sheet, before, "a refused change leaves the style sheet as it was");
    assert!(sheet.apply(&json!({ "General": { "Logo": "x" } })).is_err());
}

#[test]
fn css_that_could_escape_or_load_is_refused() {
    let mut css = StyleSheet::new("CSS-based stylesheet (experimental)", StyleSheetTemplate::Css);
    assert_eq!(value(&css, "General", "CSS"), OptionValue::Text(DEFAULT_CSS.into()));
    assert!(check_css(DEFAULT_CSS).is_ok());
    for bad in ["p{}</STYLE><script>x</script>", "@IMPORT 'x.css';", "body{background:URL(http://x)}", "a{b:image-set('x' 1x)}", "a{c:u\\72l(x)}"] {
        let e = css.apply(&json!({ "General": { "CSS": bad } })).unwrap_err();
        assert_eq!((e.section.as_str(), e.name.as_str()), ("General", "CSS"), "{bad}");
    }
    css.apply(&json!({ "General": { "CSS": "td.neg { color: purple; }" } })).unwrap();
    assert_eq!(value(&css, "General", "CSS"), OptionValue::Text("td.neg { color: purple; }".into()));
}

#[test]
fn the_configuration_adds_and_replaces_style_sheets() {
    let entries: Vec<StyleSheetEntry> = serde_json::from_value(json!([
        { "name": "Footer", "template": "Footer", "options": { "General": { "Preparer": "A. Bookkeeper" } } },
        { "name": "Board pack", "template": "Head or Tail" },
        { "name": "Board pack", "template": "Easy", "options": { "General": { "Footer": "Confidential" } } }
    ]))
    .unwrap();
    let sheets = StyleSheets::from_configuration(&entries).unwrap();
    assert_eq!(sheets.names(), [
        "Board pack",
        "CSS-based stylesheet (experimental)",
        "Default",
        "Easy",
        "Footer",
        "Head or Tail",
        "Technicolor"
    ]);
    let board = sheets.get("Board pack").unwrap();
    assert_eq!((board.template(), board.is_predefined()), (StyleSheetTemplate::Easy, false));
    assert_eq!(value(sheets.get("Footer").unwrap(), "General", "Preparer"), OptionValue::Text("A. Bookkeeper".into()));
    assert_eq!(sheets.get("Footer").unwrap().to_entry(), entries[0]);
    assert!(sheets.get("Deleted").is_none());
    let unknown = [StyleSheetEntry { name: "X".into(), template: "Gothic".into(), options: OptionValues::new() }];
    assert_eq!(StyleSheets::from_configuration(&unknown).unwrap_err(), StyleSheetError::UnknownTemplate {
        sheet: "X".into(),
        template: "Gothic".into()
    });
    let bad = [StyleSheetEntry {
        name: "X".into(),
        template: "Plain".into(),
        options: json!({ "General": { "Enable Links": "yes" } }).as_object().unwrap().clone(),
    }];
    assert!(matches!(StyleSheets::from_configuration(&bad), Err(StyleSheetError::Option { .. })));
}

#[test]
fn the_stylesheet_option_offers_every_name() {
    let sheets = StyleSheets::predefined();
    let mut db = OptionDb::new();
    db.add_general_options("Balance Sheet", &sheets.names()).unwrap();
    let option = db.get("General", "Stylesheet").unwrap();
    assert_eq!(option.value(), &OptionValue::Key(ChoiceKey::from(DEFAULT_STYLE_SHEET)));
    assert_eq!(option.kind.choices().unwrap().len(), 6);
}
