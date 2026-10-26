//! Every JSON-RPC method, grouped by the context it belongs to. Each module exposes `methods()`.

use crate::dispatch::Method;

pub mod account_properties;
pub mod accounts;
pub mod autoclear;
pub mod book;
pub mod book_options;
pub mod business;
pub mod close_book;
pub mod commodities;
pub mod config;
pub mod doc_links;
pub mod export;
pub mod lots;
pub mod period;
pub mod prices;
pub mod reconcile;
pub mod reports;
pub mod scrub;
pub mod transactions;

/// Every method red-ale serves.
pub fn all() -> Vec<Method> {
    [
        book::methods(),
        book_options::methods(),
        accounts::methods(),
        account_properties::methods(),
        period::methods(),
        config::methods(),
        transactions::methods(),
        doc_links::methods(),
        reconcile::methods(),
        autoclear::methods(),
        scrub::methods(),
        commodities::methods(),
        prices::methods(),
        lots::methods(),
        business::methods(),
        close_book::methods(),
        reports::methods(),
        export::methods(),
    ]
    .into_iter()
    .flatten()
    .collect()
}
