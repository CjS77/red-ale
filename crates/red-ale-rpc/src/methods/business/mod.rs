//! Methods: business objects.

use crate::dispatch::Method;

pub mod contacts;
pub mod documents;
pub mod payments;
pub mod posting;
pub mod terms;

/// The business methods.
pub fn methods() -> Vec<Method> {
    [contacts::methods(), terms::methods(), documents::methods(), posting::methods(), payments::methods()].into_iter().flatten().collect()
}
