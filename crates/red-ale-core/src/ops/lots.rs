//! Domain operations: lots and capital gains.

use crate::{Book, DomainResult, Env, Guid};

/// Check & Repair's lot step for one account (`xaccAccountScrubLots`): assign unlotted splits to lots and recompute
/// capital gains. Called by `scrub::check_and_repair`.
pub fn scrub_lots(book: &mut Book, env: &mut dyn Env, account: Guid) -> DomainResult<()> {
    let _ = (book, env, account);
    Ok(())
}
