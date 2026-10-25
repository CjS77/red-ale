//! Lots: groups of splits that open and close a holding.

use crate::{Frame, Guid, SlotValue, Split};

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

/// Slot keys GnuCash uses on splits for lots, capital gains and stock splits.
pub mod split_slots {
    /// On a split cut in two by lot assignment: a frame naming the other piece (`xaccSplitAddPeerSplit`,
    /// `5.17:libgnucash/engine/Split.cpp:2084-2099`).
    pub const LOT_SPLIT: &str = "lot-split";
    /// Key of the other piece's GUID inside the `lot-split` frame.
    pub const PEER_GUID: &str = "peer_guid";
    /// Key of the time of the cut inside the `lot-split` frame.
    pub const DATE: &str = "date";
    /// On a split whose sale realised a gain: the GUID of the gains transaction's split in the lot
    /// (`5.17:libgnucash/engine/cap-gains.cpp:813-815`).
    pub const GAINS_SPLIT: &str = "gains-split";
    /// On the gains transaction's split in the lot: the GUID of the split whose gain it records (`:817-819`).
    pub const GAINS_SOURCE: &str = "gains-source";
    /// The split type: absent or `normal`, or `stock-split` (`5.17:libgnucash/engine/Split.cpp:2039-2081`).
    pub const SPLIT_TYPE: &str = "split-type";
    /// The `split-type` of a stock split's split.
    pub const STOCK_SPLIT: &str = "stock-split";
}

/// Slot paths GnuCash uses on accounts for lot management (`5.17:libgnucash/engine/Account.cpp:68`, `:478`, `:4807`).
pub mod account_slots {
    /// The frame of lot settings.
    pub const LOT_MGMT: &str = "lot-mgmt";
    /// Key of the number the next default lot title uses, inside `lot-mgmt`.
    pub const NEXT_ID: &str = "next-id";
    /// Key of the frame of gains accounts by currency unique name, inside `lot-mgmt`.
    pub const GAINS_ACCT: &str = "gains-acct";
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
    /// A lot with no slots.
    pub fn new(guid: Guid, account: Guid) -> Self {
        Self { guid, account, slots: Frame::new() }
    }

    /// The lot's title.
    pub fn title(&self) -> Option<&str> {
        self.slots.get_str(slot_paths::TITLE)
    }

    /// The lot's notes.
    pub fn notes(&self) -> Option<&str> {
        self.slots.get_str(slot_paths::NOTES)
    }

    /// The invoice the lot belongs to, by the slot GnuCash writes.
    pub fn invoice(&self) -> Option<Guid> {
        self.slots.get(slot_paths::INVOICE).and_then(SlotValue::as_guid)
    }
}

/// True when the split records a stock split: its `split-type` slot is `stock-split` (`xaccSplitIsStockSplit`,
/// `5.17:libgnucash/engine/Split.cpp:2078`). Any other value reads as normal.
pub fn is_stock_split(split: &Split) -> bool {
    split.slots.get_str(split_slots::SPLIT_TYPE) == Some(split_slots::STOCK_SPLIT)
}

/// The split whose gain this split records, when it is a gains split (`xaccSplitGetGainsSourceSplit`).
pub fn gains_source(split: &Split) -> Option<Guid> {
    split.slots.get_key(split_slots::GAINS_SOURCE).and_then(SlotValue::as_guid)
}

/// The gains split recording this split's gain (`xaccSplitGetCapGainsSplit`).
pub fn gains_split(split: &Split) -> Option<Guid> {
    split.slots.get_key(split_slots::GAINS_SPLIT).and_then(SlotValue::as_guid)
}

/// True when the split's `lot-split` slot names `peer`: a frame or a list of frames (`qof_instance_kvp_has_guid`,
/// `5.17:libgnucash/engine/qofinstance.cpp:1158-1188`).
pub fn is_peer_split(split: &Split, peer: Guid) -> bool {
    let names = |v: &SlotValue| v.as_frame().and_then(|f| f.get_key(split_slots::PEER_GUID)).and_then(SlotValue::as_guid) == Some(peer);
    match split.slots.get_key(split_slots::LOT_SPLIT) {
        Some(v @ SlotValue::Frame(_)) => names(v),
        Some(SlotValue::List(items)) => items.iter().any(names),
        _ => false,
    }
}
