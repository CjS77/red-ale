//! Lots: groups of splits that open and close a holding.

use crate::{Frame, Guid};

/// Slot paths GnuCash uses on lots. `5.17:libgnucash/engine/gnc-lot.cpp`.
pub mod slot_paths {
    /// The lot's title.
    pub const TITLE: &str = "title";
    /// The lot's notes.
    pub const NOTES: &str = "notes";
    /// The invoice a business lot belongs to.
    pub const INVOICE: &str = "gncInvoice/invoice-guid";
    /// The owner type of a business lot.
    pub const OWNER_TYPE: &str = "gncOwner/owner-type";
    /// The owner of a business lot.
    pub const OWNER_GUID: &str = "gncOwner/owner-guid";
}

/// A lot. Its splits name it (`Split::lot`); its account holds it.
#[derive(Debug, Clone, PartialEq)]
pub struct Lot {
    /// Identity.
    pub guid: Guid,
    /// The account holding the lot.
    pub account: Guid,
    /// Slots, including title and notes.
    pub slots: Frame,
}

impl Lot {
    /// The lot's title.
    pub fn title(&self) -> Option<&str> {
        self.slots.get_str(slot_paths::TITLE)
    }

    /// The lot's notes.
    pub fn notes(&self) -> Option<&str> {
        self.slots.get_str(slot_paths::NOTES)
    }
}
