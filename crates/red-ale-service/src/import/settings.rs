//! What imports read from the Configuration: the Import settings and the CSV import layouts
//! (`design/api/configuration.md`, "v0.3").

use red_ale_core::Numeric;
use red_ale_import::csv::TxLayout;
use red_ale_import::matcher::ImportSettings;
use serde_json::Value;

use crate::ServiceError;
use crate::config::{Config, invalid};

/// A CSV import layout, by name or given whole.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LayoutChoice {
    /// A built-in layout, or one in the Configuration under `import.csvLayouts.<name>`.
    Named(String),
    /// A layout given in the params.
    Inline(Box<TxLayout>),
}

impl Config {
    /// The Import settings (`import.settings`), GnuCash's defaults filled in.
    pub fn import_settings(&self) -> Result<ImportSettings, ServiceError> {
        let v = self.checked("import.settings")?;
        let int = |key: &str| -> Result<i32, ServiceError> {
            v.get(key).and_then(Value::as_i64).and_then(|n| i32::try_from(n).ok()).ok_or_else(|| invalid(format!("import.settings.{key} must be an integer")))
        };
        let flag = |key: &str| v.get(key).and_then(Value::as_bool).ok_or_else(|| invalid(format!("import.settings.{key} must be a boolean")));
        let fee = v.get("atmFeeThreshold").and_then(Value::as_str).and_then(|s| Numeric::parse_any(s).ok());
        Ok(ImportSettings {
            enable_skip: flag("enableSkip")?,
            enable_update: flag("enableUpdate")?,
            use_bayes: flag("useBayes")?,
            match_threshold: int("matchThreshold")?,
            match_date_threshold: int("matchDateThreshold")?,
            match_date_not_threshold: int("matchDateNotThreshold")?,
            auto_add_threshold: int("autoAddThreshold")?,
            auto_clear_threshold: int("autoClearThreshold")?,
            atm_fee_threshold: fee.ok_or_else(|| invalid("import.settings.atmFeeThreshold must be a decimal string"))?,
            ..ImportSettings::default()
        })
    }

    /// The CSV import layout chosen: a built-in one by name (they can't be replaced), else the Configuration's,
    /// else given whole. An unknown name is `invalid`.
    pub fn csv_layout(&self, choice: &LayoutChoice) -> Result<TxLayout, ServiceError> {
        let name = match choice {
            LayoutChoice::Inline(layout) => return Ok(layout.as_ref().clone().sanitized()),
            LayoutChoice::Named(name) => name,
        };
        if let Some(preset) = TxLayout::preset(name) {
            return Ok(preset);
        }
        let layouts = self.checked("import.csvLayouts")?;
        let stored = layouts.get(name).ok_or_else(|| invalid(format!("no CSV import layout {name:?}: not built in, nor in import.csvLayouts")))?;
        serde_json::from_value::<TxLayout>(stored.clone())
            .map(TxLayout::sanitized)
            .map_err(|e| invalid(format!("the Configuration's import.csvLayouts.{name}: {e}")))
    }
}

#[cfg(test)]
mod tests {
    use red_ale_import::csv::ColumnType;

    use super::*;

    #[test]
    fn settings_and_layouts_come_from_the_configuration() {
        let config = Config::parse(
            r#"{"import": {"settings": {"matchThreshold": 2, "atmFeeThreshold": "3.50"},
                "csvLayouts": {"Bank": {"columns": ["Date", "Description", "Amount"], "skipStartLines": 1}}}}"#,
        )
        .unwrap();
        let s = config.import_settings().unwrap();
        assert_eq!((s.match_threshold, s.auto_clear_threshold, s.atm_fee_threshold), (2, 6, Numeric::new(350, 100)));
        let bank = config.csv_layout(&LayoutChoice::Named("Bank".to_owned())).unwrap();
        assert_eq!(bank.columns, [ColumnType::Date, ColumnType::Description, ColumnType::Amount]);
        assert_eq!(bank.csv.skip_start_lines, 1);
        assert!(config.csv_layout(&LayoutChoice::Named("GnuCash Export Format".to_owned())).unwrap().multi_split);
        assert!(config.csv_layout(&LayoutChoice::Named("Nope".to_owned())).is_err());
        assert_eq!(Config::default().import_settings().unwrap(), ImportSettings::default());
    }
}
