//! The numbers reports compute with, and how they are shown (`design/reports.md` §3).
//!
//! - [`Rational`]: exact, unbounded but for a [`SizeCap`]; what scripts add and multiply.
//! - [`Monetary`]: a Rational with a commodity, GnuCash's `gnc:monetary`.
//! - [`growth_rate`]: `ratio^(1/years) − 1` to 10⁻¹², the one figure that cannot be exact.
//! - [`format`]: rounding for display only, and GnuCash's amount text (`$1,234.56`, `30. FUNDS`, `$480 + 85/104`).
//!
//! A Rational never enters the Book.

mod flonum;
pub mod format;
mod growth;
mod monetary;
mod rational;

use thiserror::Error;

pub use flonum::guile_inexact;
pub use format::{
    CommodityFormat, MonetaryDisplay, PrintInfo, display_exact, display_monetary, display_number, display_number_rounded, display_price,
    display_share, fixed, format_amount,
};
pub use growth::{MAX_WORKING_BITS, cagr, growth_rate};
pub use monetary::Monetary;
pub use rational::{DEFAULT_MAX_BITS, Rational, Rounding, SizeCap};

/// An error from report arithmetic.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum NumberError {
    /// Division by zero, or a zero denominator.
    #[error("division by zero")]
    DivisionByZero,
    /// A result passed the [`SizeCap`]: the `number-size` limit.
    #[error("a number needs {bits} bits, more than the limit of {max}")]
    TooLarge {
        /// Bits of the result's larger part.
        bits: u64,
        /// The cap.
        max: u64,
    },
    /// A growth rate whose inputs need more working precision than [`MAX_WORKING_BITS`].
    #[error("growth_rate needs {bits} bits of working precision, more than the limit of {max}")]
    Precision {
        /// Bits the computation needs.
        bits: u64,
        /// The limit.
        max: u64,
    },
    /// Rounding to a denominator that is not positive.
    #[error("cannot round to denominator {0}; it must be positive")]
    InvalidDenominator(i64),
    /// A core Numeric holding GnuCash's error value (a zero denominator).
    #[error("not a valid amount: {0}")]
    InvalidNumeric(String),
    /// Adding or subtracting amounts of two commodities.
    #[error("cannot combine amounts of {left} and {right}")]
    MixedCommodities {
        /// The left operand's commodity.
        left: String,
        /// The right operand's commodity.
        right: String,
    },
    /// Text that is not a number or a rounding mode.
    #[error("{0}")]
    Parse(String),
}
