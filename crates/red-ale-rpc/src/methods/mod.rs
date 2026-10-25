//! Every JSON-RPC method, grouped by the context it belongs to. Each module exposes `methods()`.

use crate::dispatch::Method;

pub mod accounts;
pub mod book;
pub mod business;
pub mod commodities;
pub mod lots;
pub mod period;
pub mod prices;
pub mod reconcile;
pub mod scrub;
pub mod transactions;

/// Every method red-ale serves.
pub fn all() -> Vec<Method> {
    [
        book::methods(),
        accounts::methods(),
        period::methods(),
        transactions::methods(),
        reconcile::methods(),
        scrub::methods(),
        commodities::methods(),
        prices::methods(),
        lots::methods(),
        business::methods(),
    ]
    .into_iter()
    .flatten()
    .collect()
}
