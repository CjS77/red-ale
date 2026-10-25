//! Methods: business objects.

use crate::dispatch::Method;

pub mod contacts;
pub mod counters;
pub mod documents;
pub mod duplicate;
pub mod payments;
pub mod posting;
pub mod terms;

/// The business methods.
pub fn methods() -> Vec<Method> {
    [contacts::methods(), counters::methods(), terms::methods(), documents::methods(), duplicate::methods(), posting::methods(), payments::methods()].into_iter().flatten().collect()
}
