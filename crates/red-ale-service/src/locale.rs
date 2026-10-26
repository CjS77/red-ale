//! The Locale a report runs under (`design/milestone3.md`, "Locales"): a supported locale by name, or `environment`,
//! the locales the process environment selects, as GnuCash takes them from `setlocale (LC_ALL, "")`.

use red_ale_core::ops::commodities::lookup;
use red_ale_core::{Book, CommodityRef};
use red_ale_locale::{Locale, LocaleError};
use serde_json::Value;

use crate::ServiceError;
use crate::config::{Config, invalid};

/// The `locale` value that follows the process environment.
pub const ENVIRONMENT: &str = "environment";

/// The setting's key in the Configuration.
const SETTING: &str = "locale";

/// The Locale `name` names: a supported locale (`fr_FR`, `fr_FR.UTF-8`, `C`), or `environment`, read from this
/// process's `LC_ALL`, `LC_NUMERIC`, `LC_MONETARY`, `LC_TIME`, `LC_MESSAGES` and `LANG`.
pub fn resolve(name: &str) -> Result<Locale, LocaleError> {
    if name == ENVIRONMENT { Locale::from_environment(|v| std::env::var(v).ok()) } else { Locale::by_name(name) }
}

impl Config {
    /// The `locale` setting's value: `C` unless set.
    pub fn locale_name(&self) -> Result<String, ServiceError> {
        match self.checked(SETTING)? {
            Value::String(name) => Ok(name),
            other => Err(invalid(format!("the Configuration's {SETTING} must be a string, not {other}"))),
        }
    }

    /// The Locale the `locale` setting names.
    pub fn locale(&self) -> Result<Locale, ServiceError> {
        resolve(&self.locale_name()?).map_err(|e| invalid(format!("the Configuration's {SETTING}: {e}")))
    }

    /// GnuCash's default currency (`gnc_default_currency`, `5.17:libgnucash/app-utils/gnc-ui-util.cpp:747-780`): the
    /// `defaultCurrency` setting if it names a currency of `book`'s table, else the Locale's currency, else `USD`.
    pub fn default_currency(&self, book: &Book) -> Result<CommodityRef, ServiceError> {
        let setting = self.checked("defaultCurrency")?;
        let named = setting.as_str().map(|id| match id.split_once("::") {
            Some((namespace, mnemonic)) => CommodityRef::new(namespace, mnemonic),
            None => CommodityRef::currency(id),
        });
        let locale = CommodityRef::currency(self.locale()?.conventions().currency_code());
        let known = |id: &CommodityRef| id.is_currency() && lookup(book, id).is_some();
        Ok(named.filter(known).or_else(|| Some(locale).filter(known)).unwrap_or_else(|| CommodityRef::currency("USD")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_setting_names_the_locale() {
        let config = |text: &str| Config::parse(text).unwrap();
        assert_eq!(config("{}").locale().unwrap(), Locale::c());
        assert_eq!(config(r#"{"locale": "fr_FR"}"#).locale().unwrap(), Locale::by_name("fr_FR").unwrap());
        assert!(config(r#"{"locale": "xx_YY"}"#).locale().is_err());
        assert!(resolve("fr_FR.utf8").is_ok());
        assert!(resolve("xx_YY").is_err());
    }
}
