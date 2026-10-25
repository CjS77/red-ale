//! Domain operations. Each is a function over a `&mut Book` (or `&Book` for a query) that either applies completely
//! or returns a [`DomainError`](crate::DomainError); the service runs it on a copy of the Book, so a failure leaves
//! nothing behind.

pub mod account_properties;
pub mod accounts;
pub mod business;
pub mod commit;
pub mod commodities;
pub mod lots;
pub mod period;
pub mod prices;
pub mod reconcile;
pub mod scrub;
pub mod transactions;
