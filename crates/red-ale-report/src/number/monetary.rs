//! [`Monetary`]: an amount of a commodity, GnuCash's `gnc:monetary` (`5.17:bindings/guile/gnc-numeric.scm:81-91`).

use std::ops::Neg;

use red_ale_core::CommodityRef;
use red_ale_locale::Conventions;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::format::{CommodityFormat, PrintInfo, format_amount};
use super::{NumberError, Rational, Rounding, SizeCap};
use crate::wire::{CommodityId, commodity_ref};

/// An exact amount of one commodity. On the wire: `{ "amount": Rational, "commodity": "CURRENCY::USD" }`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Monetary {
    /// The amount.
    pub amount: Rational,
    /// The commodity.
    #[serde(with = "commodity_ref")]
    #[schemars(with = "CommodityId")]
    pub commodity: CommodityRef,
}

impl Monetary {
    /// An amount of `commodity`.
    pub fn new(commodity: CommodityRef, amount: Rational) -> Self {
        Self { amount, commodity }
    }

    /// `self + rhs`, within `cap`. Adding two commodities is [`NumberError::MixedCommodities`].
    pub fn checked_add(&self, rhs: &Self, cap: SizeCap) -> Result<Self, NumberError> {
        self.same_commodity(rhs)?;
        Ok(Self::new(self.commodity.clone(), self.amount.checked_add(&rhs.amount, cap)?))
    }

    /// `self - rhs`, within `cap`. Subtracting two commodities is [`NumberError::MixedCommodities`].
    pub fn checked_sub(&self, rhs: &Self, cap: SizeCap) -> Result<Self, NumberError> {
        self.same_commodity(rhs)?;
        Ok(Self::new(self.commodity.clone(), self.amount.checked_sub(&rhs.amount, cap)?))
    }

    fn same_commodity(&self, rhs: &Self) -> Result<(), NumberError> {
        if self.commodity == rhs.commodity {
            Ok(())
        } else {
            Err(NumberError::MixedCommodities {
                left: self.commodity.to_string(),
                right: rhs.commodity.to_string(),
            })
        }
    }

    /// Rounded to the commodity's SCU (`fraction`) with banker's rounding: the `round-scu` of
    /// `5.17:gnucash/report/reports/standard/account-piecharts.scm:497-500`.
    pub fn round_scu(&self, fraction: i64) -> Result<Self, NumberError> {
        Ok(Self::new(
            self.commodity.clone(),
            self.amount.round_to(fraction, Rounding::Bankers)?,
        ))
    }

    /// `gnc:monetary->string` (`5.17:gnucash/report/report-utilities.scm:143-146`): the exact amount with the
    /// commodity's print info and symbol, not rounded to the SCU.
    pub fn to_display_string(&self, format: &CommodityFormat, conv: &Conventions) -> String {
        format_amount(&self.amount, &PrintInfo::commodity(format, true), conv)
    }
}

impl Neg for Monetary {
    type Output = Monetary;

    fn neg(self) -> Monetary {
        Monetary::new(self.commodity, -self.amount)
    }
}

impl Neg for &Monetary {
    type Output = Monetary;

    fn neg(self) -> Monetary {
        Monetary::new(self.commodity.clone(), -&self.amount)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn usd(s: &str) -> Monetary {
        Monetary::new(CommodityRef::currency("USD"), Rational::parse(s).unwrap())
    }

    #[test]
    fn sums_need_one_commodity() {
        let cap = SizeCap::DEFAULT;
        assert_eq!(usd("1.25").checked_add(&usd("2.5"), cap).unwrap(), usd("3.75"));
        assert_eq!(usd("1.25").checked_sub(&usd("2.5"), cap).unwrap(), usd("-1.25"));
        let gbp = Monetary::new(CommodityRef::currency("GBP"), Rational::one());
        assert_eq!(
            usd("1").checked_add(&gbp, cap),
            Err(NumberError::MixedCommodities {
                left: "CURRENCY::USD".into(),
                right: "CURRENCY::GBP".into()
            })
        );
        assert_eq!(-usd("2"), usd("-2"));
    }

    #[test]
    fn rounding_and_text() {
        assert_eq!(usd("2.345").round_scu(100).unwrap(), usd("2.34"));
        let format = CommodityFormat::new(CommodityRef::currency("USD"), "$", 100);
        let c = red_ale_locale::Locale::c().conventions();
        assert_eq!(usd("2.345").to_display_string(&format, &c), "$2.345");
        assert_eq!(usd("-1234").to_display_string(&format, &c), "-$1,234.00");
    }

    #[test]
    fn json_form() {
        let m = usd("12.5");
        let v = serde_json::to_value(&m).unwrap();
        assert_eq!(v, json!({ "amount": { "num": "25", "denom": "2" }, "commodity": "CURRENCY::USD" }));
        assert_eq!(serde_json::from_value::<Monetary>(v).unwrap(), m);
    }
}
