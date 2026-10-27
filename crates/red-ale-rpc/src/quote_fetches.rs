//! The open book's Quote fetches: the one running and the last 16 finished (`design/milestone3.md`, "Decided"), and
//! the step that stores a finished fetch's prices (`design/api/quotes.md`, "Fetching in the background").
//!
//! A fetch's requests run on their own thread (`red_ale_quotes::fetch`). When they are all done the fetch thread
//! records their outcomes here and calls the dispatcher's notifier, if a server installed one
//! ([`settle_when_fetched`]); the prices are then added in one Mutation, in turn with every other method. Every
//! method call also settles a finished fetch first, so a Book is never seen without prices already fetched for it,
//! whichever door the call came through. Closing the book drops the registry, which cancels a running fetch:
//! nothing from it is stored.

use std::collections::VecDeque;
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError, Weak};

use red_ale_core::ops::prices::{Nearest, add_price, nearest};
use red_ale_core::ops::quotes::{QuoteRecord, build_price};
use red_ale_core::{Book, CommodityRef, Env, Price, Time64, TimeZone};
use red_ale_quotes::fetch::{FetchHandle, Outcome};

use crate::dispatch::{Context, Dispatcher};
use crate::error::RpcError;
use crate::methods::prices::{info, outcome_name};
use crate::methods::quotes::{CommodityFetch, FetchState, FetchStatus, ItemState};
use crate::wire::Timestamp;

/// How many finished fetches' statuses are kept.
const KEPT: usize = 16;

/// Called from a fetch's thread when its requests are done.
pub type Notifier = Arc<dyn Fn() + Send + Sync>;

/// The Quote fetches of the open book.
#[derive(Default)]
pub struct QuoteFetches {
    next_id: u64,
    running: Option<Running>,
    finished: VecDeque<FetchStatus>,
}

impl std::fmt::Debug for QuoteFetches {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("QuoteFetches")
            .field("next_id", &self.next_id)
            .field("running", &self.running.is_some())
            .field("finished", &self.finished.len())
            .finish()
    }
}

/// A fetch whose requests may still be running.
struct Running {
    shared: Arc<Shared>,
    /// For each of the fetch's items, its entry in the status.
    items: Vec<usize>,
    _handle: FetchHandle,
}

/// What the fetch thread and the dispatcher share.
#[derive(Default)]
pub struct Shared {
    state: Mutex<Progress>,
    done: Condvar,
}

/// A running fetch's status, and its outcomes once every request is done.
#[derive(Default)]
struct Progress {
    status: Option<FetchStatus>,
    outcomes: Option<Vec<Outcome>>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    // A panic while holding the lock leaves a status half updated at worst; the Book is not behind it.
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Shared {
    /// A shared state for a fetch starting with `status`.
    pub fn new(status: FetchStatus) -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(Progress {
                status: Some(status),
                outcomes: None,
            }),
            done: Condvar::new(),
        })
    }

    /// One item's requests ended: its entry becomes `fetched`, or `failed` with the reason.
    pub fn item_done(&self, entry: usize, outcome: &Outcome) {
        let mut progress = lock(&self.state);
        let Some(item) = progress.status.as_mut().and_then(|s| s.commodities.get_mut(entry)) else {
            return;
        };
        match outcome {
            Outcome::Quote { service, .. } => {
                item.state = ItemState::Fetched;
                item.service = Some(service.clone());
            }
            Outcome::Failed { service, reason, message } => {
                item.fail(reason.code(), Some(message.clone()));
                item.service.clone_from(service);
            }
        }
    }

    /// Every request is done.
    pub fn finish(&self, outcomes: Vec<Outcome>) {
        lock(&self.state).outcomes = Some(outcomes);
        self.done.notify_all();
    }

    fn status(&self) -> Option<FetchStatus> {
        lock(&self.state).status.clone()
    }
}

/// Makes sure a fetch that ends without delivering its outcomes, cancelled or failed, still wakes anyone waiting.
pub struct Delivery {
    shared: Arc<Shared>,
    notifier: Option<Notifier>,
    delivered: bool,
}

impl Delivery {
    /// A delivery into `shared`, calling `notifier` once delivered.
    pub fn new(shared: Arc<Shared>, notifier: Option<Notifier>) -> Self {
        Self {
            shared,
            notifier,
            delivered: false,
        }
    }

    /// Deliver the outcomes.
    pub fn deliver(mut self, outcomes: Vec<Outcome>) {
        self.delivered = true;
        self.shared.finish(outcomes);
        if let Some(notify) = &self.notifier {
            notify();
        }
    }
}

impl Drop for Delivery {
    fn drop(&mut self) {
        if !self.delivered {
            self.shared.finish(Vec::new());
        }
    }
}

impl QuoteFetches {
    /// The ID for a new fetch.
    pub fn next_id(&mut self) -> String {
        self.next_id += 1;
        format!("quote-fetch-{}", self.next_id)
    }

    /// True while a fetch's prices are not yet stored.
    pub fn is_running(&self) -> bool {
        self.running.is_some()
    }

    /// Record a started fetch.
    pub fn start(&mut self, shared: Arc<Shared>, items: Vec<usize>, handle: FetchHandle) {
        self.running = Some(Running {
            shared,
            items,
            _handle: handle,
        });
    }

    /// Forget every fetch, cancelling one still running, as closing the book does. IDs are not reused.
    pub fn clear(&mut self) {
        self.running = None;
        self.finished.clear();
    }

    /// A fetch's status by ID, or the latest one's.
    pub fn status(&self, id: Option<&str>) -> Option<FetchStatus> {
        let running = self.running.as_ref().and_then(|r| r.shared.status());
        let mut all = running.into_iter().chain(self.finished.iter().rev().cloned());
        match id {
            None => all.next(),
            Some(id) => all.find(|s| s.fetch == id),
        }
    }

    /// The running fetch's outcomes, if its requests are done.
    fn take_outcomes(&mut self) -> Option<(Running, FetchStatus, Vec<Outcome>)> {
        let running = self.running.as_ref()?;
        let mut progress = lock(&running.shared.state);
        let outcomes = progress.outcomes.take()?;
        let status = progress.status.take()?;
        drop(progress);
        self.running.take().map(|r| (r, status, outcomes))
    }

    fn keep(&mut self, status: FetchStatus) {
        if self.finished.len() == KEPT {
            self.finished.pop_front();
        }
        self.finished.push_back(status);
    }

    /// Block until the running fetch's requests are done, if one is running.
    fn wait(&self) {
        let Some(running) = &self.running else { return };
        let shared = &running.shared;
        let progress = lock(&shared.state);
        drop(
            shared
                .done
                .wait_while(progress, |p| p.outcomes.is_none())
                .unwrap_or_else(PoisonError::into_inner),
        );
    }
}

impl Context {
    /// Store a finished fetch's prices, if its requests are done: one Mutation, each price through `price.add`'s
    /// same-day rules, in commodity order.
    pub(crate) fn settle_quotes(&mut self) {
        let Some((running, mut status, outcomes)) = self.quote_fetches.take_outcomes() else {
            return;
        };
        // A fetch that stopped early delivers fewer outcomes than items; the rest are reported as stopped.
        let outcomes = outcomes.into_iter().map(Some).chain(std::iter::repeat(None));
        let fetched: Vec<Fetched> = running
            .items
            .iter()
            .zip(outcomes)
            .filter_map(|(&entry, outcome)| {
                let item = status.commodities.get_mut(entry)?;
                item.fetched(outcome).map(|record| Fetched {
                    entry,
                    commodity: item.commodity.0.clone(),
                    record,
                })
            })
            .collect();
        let finished = self.store(&mut status, &fetched);
        status.finish(finished);
        self.quote_fetches.keep(status);
    }

    /// Run the Mutation; returns when it ran.
    fn store(&mut self, status: &mut FetchStatus, fetched: &[Fetched]) -> Time64 {
        let Ok(session) = self.session() else { return Time64(0) };
        let zone = session.zone().clone();
        if fetched.is_empty() {
            return session.now();
        }
        let result = session.try_mutate(|book, env| {
            let now = env.now();
            let stored: Vec<_> = fetched
                .iter()
                .map(|f| {
                    (
                        f.entry,
                        f.record
                            .as_ref()
                            .map_or_else(|| Err(stopped()), |r| store_one(book, env, &f.commodity, r, now, &zone)),
                    )
                })
                .collect();
            Ok::<_, RpcError>((now, stored))
        });
        match result {
            Ok((now, stored)) => {
                stored.into_iter().for_each(|(entry, result)| {
                    if let Some(c) = status.commodities.get_mut(entry) {
                        c.settle(result, &zone);
                    }
                });
                now
            }
            Err(e) => {
                let entries: Vec<usize> = fetched.iter().map(|f| f.entry).collect();
                status
                    .commodities
                    .iter_mut()
                    .enumerate()
                    .filter(|(i, _)| entries.contains(i))
                    .for_each(|(_, c)| c.fail("quote-failed", Some(e.message.clone())));
                self.session().map_or(Time64(0), red_ale_service::Session::now)
            }
        }
    }

    /// Wait for the running fetch's requests, then store its prices.
    pub(crate) fn wait_for_quotes(&mut self) {
        self.quote_fetches.wait();
        self.settle_quotes();
    }
}

/// A commodity whose requests ended with a quote record, or that the fetch never finished.
struct Fetched {
    entry: usize,
    commodity: CommodityRef,
    record: Option<QuoteRecord>,
}

fn stopped() -> (&'static str, Option<String>) {
    (
        "service-error",
        Some("the fetch stopped before this commodity's requests ended".to_owned()),
    )
}

/// What storing one commodity's price did.
pub struct Stored {
    /// `price.add`'s outcome.
    pub outcome: &'static str,
    /// The price added.
    pub price: Option<Price>,
    /// The price that kept its place, or was replaced.
    pub existing: Option<Price>,
}

/// Build and add one commodity's price; a failure is its reason and message.
fn store_one(
    book: &mut Book,
    env: &mut dyn Env,
    id: &CommodityRef,
    record: &QuoteRecord,
    now: Time64,
    zone: &TimeZone,
) -> Result<Stored, (&'static str, Option<String>)> {
    if red_ale_core::ops::commodities::lookup(book, id).is_none() {
        return Err(("not-found", Some(format!("commodity {id} was deleted during the fetch"))));
    }
    let price = build_price(book, id, Some(record), now, zone, env.new_guid()).map_err(|f| (f.code(), f.message()))?;
    let same_day = nearest(book, &price.commodity, &price.currency, price.time, Nearest::SameDay, zone).cloned();
    let added = add_price(book, price, zone).map_err(|e| ("quote-failed", Some(e.to_string())))?;
    let kept = added
        .rejected_by
        .or(added.duplicate_of)
        .and_then(|g| book.prices.get(&g))
        .map(|p| (**p).clone());
    Ok(Stored {
        outcome: outcome_name(&added),
        price: added.added.and_then(|g| book.prices.get(&g)).map(|p| (**p).clone()),
        existing: added.replaced.and(same_day).or(kept),
    })
}

impl CommodityFetch {
    /// Take in this commodity's outcome: a failure is recorded at once; a quote record, or no outcome at all, goes
    /// on to the Mutation.
    #[allow(clippy::option_option)] // No outcome, and an outcome with no record, differ.
    fn fetched(&mut self, outcome: Option<Outcome>) -> Option<Option<QuoteRecord>> {
        match outcome {
            Some(Outcome::Quote { service, record }) => {
                self.service = Some(service);
                Some(Some(record))
            }
            Some(Outcome::Failed { service, reason, message }) => {
                self.fail(reason.code(), Some(message));
                self.service = service;
                None
            }
            None => Some(None),
        }
    }

    fn fail(&mut self, reason: &str, message: Option<String>) {
        self.state = ItemState::Failed;
        self.reason = Some(reason.to_owned());
        self.message = message;
    }

    fn settle(&mut self, result: Result<Stored, (&'static str, Option<String>)>, zone: &TimeZone) {
        match result {
            Ok(stored) => {
                self.state = ItemState::Stored;
                self.outcome = Some(stored.outcome.to_owned());
                self.price = stored.price.as_ref().map(|p| info(p, zone));
                self.existing = stored.existing.as_ref().map(|p| info(p, zone));
            }
            Err((reason, message)) => self.fail(reason, message),
        }
    }
}

impl FetchStatus {
    fn finish(&mut self, at: Time64) {
        self.state = FetchState::Finished;
        self.finished = Some(Timestamp(at));
        let count = |state| self.commodities.iter().filter(|c| c.state == state).count();
        self.stored = Some(count(ItemState::Stored));
        self.failed = Some(count(ItemState::Failed));
    }
}

/// Have `dispatcher` store a fetch's prices as soon as its requests are done, rather than at the next call. The
/// servers install this; the fetch thread holds only a weak reference, so it never keeps the dispatcher alive.
pub fn settle_when_fetched(dispatcher: &Arc<Mutex<Dispatcher>>) {
    let weak: Weak<Mutex<Dispatcher>> = Arc::downgrade(dispatcher);
    let notify: Notifier = Arc::new(move || {
        if let Some(d) = weak.upgrade() {
            lock(&d).settle_quotes();
        }
    });
    lock(dispatcher).set_quote_notifier(notify);
}
