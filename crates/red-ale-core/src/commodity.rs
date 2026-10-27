//! Commodities: currencies and securities.

use std::fmt;

use crate::Frame;

/// The namespace of ISO 4217 currencies.
pub const CURRENCY_NAMESPACE: &str = "CURRENCY";
/// The legacy name of the currency namespace, read as [`CURRENCY_NAMESPACE`]. `5.17:libgnucash/engine/gnc-commodity.h`.
pub const LEGACY_CURRENCY_NAMESPACE: &str = "ISO4217";
/// The reserved namespace of the commodity used by scheduled-transaction templates.
pub const TEMPLATE_NAMESPACE: &str = "template";

/// A reference to a commodity by namespace and mnemonic: how accounts, transactions and prices name one.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CommodityRef {
    /// Namespace, such as `CURRENCY` or `NASDAQ`.
    pub namespace: String,
    /// Mnemonic within the namespace, such as `USD`.
    pub mnemonic: String,
}

impl CommodityRef {
    /// A reference from namespace and mnemonic. The legacy `ISO4217` namespace is read as `CURRENCY`.
    pub fn new(namespace: impl Into<String>, mnemonic: impl Into<String>) -> Self {
        let namespace = namespace.into();
        let namespace = if namespace == LEGACY_CURRENCY_NAMESPACE {
            CURRENCY_NAMESPACE.to_owned()
        } else {
            namespace
        };
        Self {
            namespace,
            mnemonic: mnemonic.into(),
        }
    }

    /// A currency by ISO code.
    pub fn currency(code: impl Into<String>) -> Self {
        Self::new(CURRENCY_NAMESPACE, code)
    }

    /// True when this names a currency.
    pub fn is_currency(&self) -> bool {
        self.namespace == CURRENCY_NAMESPACE
    }
}

impl fmt::Display for CommodityRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}::{}", self.namespace, self.mnemonic)
    }
}

/// A currency or security. `5.17:libgnucash/engine/gnc-commodity.cpp`.
#[derive(Debug, Clone, PartialEq)]
pub struct Commodity {
    /// Namespace and mnemonic.
    pub id: CommodityRef,
    /// Full name (`cmdty:name`); for currencies, from the ISO table.
    pub fullname: Option<String>,
    /// Exchange code such as a CUSIP or ISIN (`cmdty:xcode`).
    pub cusip: Option<String>,
    /// The SCU: quantities are kept in units of `1/fraction`.
    pub fraction: i64,
    /// Whether online quotes are wanted (`cmdty:get_quotes`).
    pub quote_flag: bool,
    /// Quote source internal name, kept even when unknown to red-ale.
    pub quote_source: Option<String>,
    /// Quote time zone (`cmdty:quote_tz`); `Some("")` is kept distinct from `None`.
    pub quote_tz: Option<String>,
    /// The commodity's slots, such as `user_symbol`.
    pub slots: Frame,
}

impl Commodity {
    /// A security with a fraction and no quote settings.
    pub fn new(id: CommodityRef, fullname: Option<String>, fraction: i64) -> Self {
        Self {
            id,
            fullname,
            cusip: None,
            fraction,
            quote_flag: false,
            quote_source: None,
            quote_tz: None,
            slots: Frame::new(),
        }
    }

    /// True for a commodity in the `CURRENCY` namespace.
    pub fn is_currency(&self) -> bool {
        self.id.is_currency()
    }
}
