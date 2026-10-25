//! Prices and the price database.

use crate::{CommodityRef, Guid, Numeric, Time64};

/// The value of one commodity in another at a time. `5.17:libgnucash/engine/gnc-pricedb.cpp`.
#[derive(Debug, Clone, PartialEq)]
pub struct Price {
    /// Identity.
    pub guid: Guid,
    /// The commodity priced.
    pub commodity: CommodityRef,
    /// The currency (or commodity) the price is in.
    pub currency: CommodityRef,
    /// When the price applies.
    pub time: Time64,
    /// Where the price came from, such as `user:price-editor`.
    pub source: Option<String>,
    /// Price type, such as `last`, `bid`, `ask`, `nav` or `unknown`.
    pub price_type: Option<String>,
    /// Units of `currency` per unit of `commodity`.
    pub value: Numeric,
}
