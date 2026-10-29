//! The built-in saved report configurations: saved configurations compiled into red-ale beside the built-in
//! scripts, read-only, which the Configuration never stores (`design/reports.md` §9, §11). Each is the record a
//! Configuration entry deserializes into (`guid`, `name`, `template`, `options`) with GnuCash's menu fields, since
//! each stands for a GnuCash menu item that builds a configured report. Adding one is one entry in [`BUILTIN`].

use serde::Deserialize;
use serde_json::{Map, Value};

use crate::error::ReportError;
use crate::wire::Id;

/// A built-in saved report configuration.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BuiltinConfiguration {
    /// Its GUID: the GnuCash menu item's.
    pub guid: Id,
    /// Its name: the menu item's English name.
    pub name: String,
    /// The GUID of the template it configures.
    pub template: Id,
    /// The options that differ from the template's defaults, by section and name, in wire form.
    pub options: Map<String, Value>,
    /// GnuCash's menu path below Reports (`"_Multicolumn"`).
    pub menu_path: Vec<String>,
    /// The menu entry when it differs from the name.
    pub menu_name: Option<String>,
    /// The menu tip.
    pub menu_tip: Option<String>,
    /// Whether it shows in menus.
    pub in_menu: bool,
}

/// The Dashboard, GnuCash's Reports › Multicolumn › Dashboard (`5.17:gnucash/gnome/report-menus.scm:113-121`), which
/// builds a two-column Multicolumn View of four reports at their defaults (`gnc:make-dashboard`,
/// `5.17:gnucash/report/reports/standard/dashboard.scm:40-65`; Copyright 2001 Bill Gribble <grib@gnumatic.com>,
/// Copyright 2019 Geert Janssens <geert@kobaltwit.be>; GPL-2.0-or-later). The children, in its order and spans
/// (`(report wide high #f)`, `:60-64`): the expense pie, the income pie, the income/expense chart and the Account
/// Summary (`:34-38`). The menu item's GUID (`report-menus.scm:117`) names it, since GnuCash gives the view none.
const DASHBOARD: &str = r#"{
    "guid": "ad80271c890b11dfa79f2dcedfd72085",
    "name": "Dashboard",
    "template": "d8ba4a2e89e8479ca9f6eccdeb164588",
    "options": {
        "General": { "Report name": "Dashboard", "Number of columns": 2 },
        "__general": { "report-list": [
            { "report": "9bf1892805cb4336be6320fe48ce5446", "colspan": 1, "rowspan": 1 },
            { "report": "e1bd09b8a1dd49dd85760db9d82b045c", "colspan": 1, "rowspan": 1 },
            { "report": "80769921e87943adade887b9835a7685", "colspan": 2, "rowspan": 1 },
            { "report": "3298541c236b494998b236dfad6ad752", "colspan": 2, "rowspan": 1 }
        ] }
    },
    "menuPath": ["_Multicolumn"],
    "menuName": null,
    "menuTip": "A basic dashboard for your accounting data",
    "inMenu": true
}"#;

/// Every built-in saved configuration, as JSON. Add an entry here for each new one.
const BUILTIN: &[&str] = &[DASHBOARD];

/// Every built-in saved configuration, in the order of [`BUILTIN`]. One that does not read is a bug in red-ale, an
/// `internal-error`; the library's tests read them all.
pub fn builtin_configurations() -> Result<Vec<BuiltinConfiguration>, ReportError> {
    BUILTIN
        .iter()
        .map(|text| {
            serde_json::from_str(text).map_err(|e| ReportError::Internal(format!("a built-in saved configuration does not read: {e}")))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::GNUCASH_TEMPLATES;

    #[test]
    fn every_built_in_configuration_reads_and_configures_a_gnucash_template() {
        let all = builtin_configurations().unwrap();
        assert_eq!(all.len(), BUILTIN.len());
        all.iter().for_each(|c| {
            let template = c.template.0.to_string();
            assert!(GNUCASH_TEMPLATES.iter().any(|(g, _)| *g == template), "{}", c.name);
            assert!(
                GNUCASH_TEMPLATES.iter().all(|(g, n)| *g != c.guid.0.to_string() && *n != c.name),
                "{}",
                c.name
            );
        });
    }

    #[test]
    fn the_dashboard_is_gnucashs() {
        let dashboard = &builtin_configurations().unwrap()[0];
        assert_eq!(dashboard.guid.0.to_string(), "ad80271c890b11dfa79f2dcedfd72085");
        assert_eq!(dashboard.name, "Dashboard");
        let spans: Vec<(i64, i64)> = dashboard.options["__general"]["report-list"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| (c["colspan"].as_i64().unwrap(), c["rowspan"].as_i64().unwrap()))
            .collect();
        assert_eq!(spans, [(1, 1), (1, 1), (2, 1), (2, 1)]);
    }
}
