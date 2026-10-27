//! Column types and the properties one line of a price import holds.
//!
//! Translated from `5.17:gnucash/import-export/csv-imp/gnc-imp-props-price.cpp` and `gnc-imp-props-price.hpp`
//! (GPL-2.0-or-later, as their headers say; `gnc-import-price.hpp` is Copyright (c) 2015 Geert Janssens and (c) 2017
//! Robert Fewell). This translation is GPL-3.0-or-later.
//!
//! GnuCash parses each cell as soon as its column gets a type, against the Book's commodity table, recording one
//! error per property as `"<column name>: <reason>"`. The order of the columns matters: the From commodity is
//! looked up once both its symbol and its namespace are known, and the check that it differs from the To currency
//! sees only what earlier columns set.

use std::collections::BTreeMap;

use red_ale_core::ops::commodities::{lookup, namespaces};
use red_ale_core::{Book, CommodityRef, Date, Numeric};
use serde::{Deserialize, Serialize};

use crate::csv::date::{DateContext, DateFormat, parse_date};
use crate::csv::monetary::{CurrencyFormat, MonetaryLocale, parse_amount_price};

/// What a price import column holds: GnuCash's `GncPricePropType` (`gnc-imp-props-price.hpp:47-55`), in its order,
/// which orders a line's errors.
///
/// In JSON a column type is GnuCash's saved name ([`PriceColumn::gnucash_name`]). GnuCash loads an unknown name as
/// `None`; red-ale refuses it, so a typo in a layout is reported.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize)]
#[serde(try_from = "String", into = "&'static str")]
pub enum PriceColumn {
    /// Ignored.
    #[default]
    None,
    /// The price's date.
    Date,
    /// The price: units of the To currency per unit of the From commodity.
    Amount,
    /// The From commodity's mnemonic, or its unique name `NAMESPACE::MNEMONIC`.
    FromSymbol,
    /// The From commodity's namespace.
    FromNamespace,
    /// The To currency's code, or its unique name.
    ToCurrency,
}

impl PriceColumn {
    /// Every column type, in GnuCash's order.
    pub const ALL: [Self; 6] = [
        Self::None,
        Self::Date,
        Self::Amount,
        Self::FromSymbol,
        Self::FromNamespace,
        Self::ToCurrency,
    ];

    /// The English name GnuCash shows and saves (`gnc_price_col_type_strs`, `gnc-imp-props-price.cpp:51-58`).
    pub fn gnucash_name(self) -> &'static str {
        match self {
            Self::None => "None",
            Self::Date => "Date",
            Self::Amount => "Amount",
            Self::FromSymbol => "From Symbol",
            Self::FromNamespace => "From Namespace",
            Self::ToCurrency => "Currency To",
        }
    }
}

impl From<PriceColumn> for &'static str {
    fn from(t: PriceColumn) -> Self {
        t.gnucash_name()
    }
}

impl TryFrom<String> for PriceColumn {
    type Error = String;

    fn try_from(name: String) -> Result<Self, Self::Error> {
        Self::ALL
            .into_iter()
            .find(|t| t.gnucash_name() == name)
            .ok_or_else(|| format!("unknown CSV price column type {name:?}"))
    }
}

/// What parsing a cell needs besides the Book.
pub struct PriceContext<'a> {
    /// How dates are written.
    pub date_format: DateFormat,
    /// How amounts are written.
    pub currency_format: CurrencyFormat,
    /// Today and the locale's date parser.
    pub dates: DateContext<'a>,
    /// The locale's monetary separators.
    pub monetary: MonetaryLocale,
}

/// One line's properties: GnuCash's `GncImportPrice` (`gnc-imp-props-price.hpp:83-115`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PriceLine {
    /// The date.
    pub date: Option<Date>,
    /// The amount, exactly as written.
    pub amount: Option<Numeric>,
    /// The From Symbol cell.
    pub from_symbol: Option<String>,
    /// The From Namespace cell, once it names a namespace.
    pub from_namespace: Option<String>,
    /// The From commodity, once symbol and namespace resolve, or the layout's.
    pub from_commodity: Option<CommodityRef>,
    /// The To currency, or the layout's.
    pub to_currency: Option<CommodityRef>,
    errors: BTreeMap<PriceColumn, String>,
}

impl PriceLine {
    /// The errors, in column type order: `"<column name>: <reason>"` each.
    pub fn errors(&self) -> &BTreeMap<PriceColumn, String> {
        &self.errors
    }

    /// `GncImportPrice::set` (`gnc-imp-props-price.cpp:152-254`) with `enable_test_empty` on, as every column has it
    /// when no layout default stands in for it (`gnc-import-price.cpp:698-714`).
    pub fn set(&mut self, book: &Book, column: PriceColumn, value: &str, ctx: &PriceContext) {
        self.errors.remove(&column);
        if let Err(reason) = self.set_value(book, column, value, ctx) {
            self.errors.insert(column, format!("{}: {reason}", column.gnucash_name()));
        }
    }

    fn set_value(&mut self, book: &Book, column: PriceColumn, value: &str, ctx: &PriceContext) -> Result<(), String> {
        if value.is_empty() && column != PriceColumn::None {
            return Err("Column value can not be empty.".to_owned());
        }
        match column {
            PriceColumn::None => Ok(()),
            PriceColumn::Date => {
                self.date = None;
                self.date = Some(parse_date(value, ctx.date_format, &ctx.dates).map_err(|e| e.to_string())?);
                Ok(())
            }
            PriceColumn::Amount => {
                self.amount = None;
                self.amount = Some(parse_amount_price(value, ctx.currency_format, &ctx.monetary).map_err(|e| e.to_string())?);
                Ok(())
            }
            PriceColumn::FromSymbol => {
                self.from_symbol = Some(value.to_owned());
                self.resolve_from(book)
            }
            PriceColumn::FromNamespace => {
                self.from_namespace = None;
                if !has_namespace(book, value) {
                    return Err("Value can't be parsed into a valid namespace.".to_owned());
                }
                self.from_namespace = Some(value.to_owned());
                self.resolve_from(book)
            }
            PriceColumn::ToCurrency => self.set_to_currency(book, value),
        }
    }

    /// Once both the symbol and the namespace are known, the From commodity, which must differ from the To currency.
    fn resolve_from(&mut self, book: &Book) -> Result<(), String> {
        let (Some(symbol), Some(namespace)) = (&self.from_symbol, &self.from_namespace) else {
            return Ok(());
        };
        let commodity = parse_commodity(book, symbol, namespace)?;
        if self.to_currency.as_ref() == Some(&commodity) {
            return Err("'Commodity From' can not be the same as 'Currency To'.".to_owned());
        }
        self.from_commodity = Some(commodity);
        Ok(())
    }

    fn set_to_currency(&mut self, book: &Book, value: &str) -> Result<(), String> {
        self.to_currency = None;
        let commodity = parse_commodity(book, value, red_ale_core::commodity::CURRENCY_NAMESPACE)?;
        if self.from_commodity.as_ref() == Some(&commodity) {
            return Err("'Currency To' can not be the same as 'Commodity From'.".to_owned());
        }
        if !commodity.is_currency() {
            return Err("Value parsed into an invalid currency for a currency column type.".to_owned());
        }
        self.to_currency = Some(commodity);
        Ok(())
    }

    /// `GncImportPrice::verify_essentials` (`gnc-imp-props-price.cpp:278-293`): why the line can't make a price.
    pub fn missing_essentials(&self) -> Option<&'static str> {
        match (&self.date, &self.amount, &self.to_currency, &self.from_commodity) {
            (None, ..) => Some("No date column."),
            (_, None, ..) => Some("No amount column."),
            (_, _, None, _) => Some("No 'Currency to'."),
            (.., None) => Some("No 'Commodity from'."),
            (_, _, Some(to), Some(from)) if to == from => Some("'Commodity From' can not be the same as 'Currency To'."),
            _ => None,
        }
    }
}

/// `parse_commodity_price_comm` (`gnc-imp-props-price.cpp:108-130`): by unique name `NAMESPACE::MNEMONIC`, then by
/// `namespace` and `symbol`. GnuCash's table holds every ISO currency; red-ale's built-in table stands in for those
/// the Book has not defined.
pub fn parse_commodity(book: &Book, symbol: &str, namespace: &str) -> Result<CommodityRef, String> {
    let found = |id: CommodityRef| lookup(book, &id).map(|c| c.id.clone());
    symbol
        .split_once("::")
        .and_then(|(ns, m)| found(CommodityRef::new(ns, m)))
        .or_else(|| found(CommodityRef::new(namespace, symbol)))
        .ok_or_else(|| "Value can't be parsed into a valid commodity.".to_owned())
}

/// `gnc_commodity_table_has_namespace`: a namespace of the Book's commodities, the currencies (also as `ISO4217`),
/// or `template`, which GnuCash's table always has (`gnc_commodity_table_add_default_data`,
/// `5.17:libgnucash/engine/gnc-commodity.cpp:2317-2339`).
pub fn has_namespace(book: &Book, namespace: &str) -> bool {
    matches!(namespace, "template" | "ISO4217") || namespaces(book).iter().any(|(ns, _)| ns == namespace)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn column_names() {
        let json = serde_json::to_string(&PriceColumn::ToCurrency).unwrap();
        assert_eq!(json, "\"Currency To\"");
        assert_eq!(
            serde_json::from_str::<PriceColumn>("\"From Namespace\"").unwrap(),
            PriceColumn::FromNamespace
        );
        assert!(serde_json::from_str::<PriceColumn>("\"Price\"").is_err());
    }
}
