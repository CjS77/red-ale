//! The red-ale bookkeeping domain.
//!
//! The Book model and its invariants, and the domain operations on it. Pure: no I/O, no async and no file formats.
//! Time, the local time zone and fresh GUIDs come in through [`Env`].

#![forbid(unsafe_code)]
#![deny(clippy::float_arithmetic)]

pub mod account;
pub mod book;
pub mod book_options;
pub mod business;
pub mod commodity;
pub mod currencies;
pub mod diff;
pub mod error;
pub mod formula;
pub mod guid;
#[allow(clippy::unreadable_literal)] // Generated; fractions are copied verbatim from GnuCash's table.
mod iso4217_table;
pub mod lot;
pub mod numeric;
pub mod ops;
pub mod price;
pub mod slots;
pub mod time;
pub mod transaction;

pub use account::{Account, AccountType};
pub use book::{Book, Env, FixedEnv};
pub use commodity::{Commodity, CommodityRef};
pub use error::{DomainError, DomainResult};
pub use guid::Guid;
pub use lot::Lot;
pub use numeric::{DENOM_AUTO, DenomMode, How, Numeric, NumericError, Round};
pub use price::Price;
pub use slots::{Frame, SlotValue};
pub use time::{Date, Time64, TimeZone};
pub use transaction::{ReconcileState, Split, Transaction};
