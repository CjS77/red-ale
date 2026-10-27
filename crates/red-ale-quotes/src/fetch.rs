//! Running a Quote fetch's requests outside the Book (`design/api/quotes.md`, "Fetching in the background").
//!
//! A fetch runs on a thread of its own, with its own single-threaded tokio runtime, so it needs nothing from the
//! process that starts it: the HTTP server, the MCP server and `red-ale exec` start fetches the same way, and none of
//! them is held up while a fetch runs. Different services are asked in parallel; each service's requests go one at
//! a time, at least its minimum interval apart, each with the per-request timeout. A commodity's requests are tried
//! in order until one gives a quote.
//!
//! Dropping the [`FetchHandle`] cancels the fetch: outstanding requests are dropped and `on_done` is never called.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use red_ale_core::TimeZone;
use red_ale_core::ops::quotes::QuoteRecord;
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinSet;
use tokio::time::Instant;

use crate::format::{self, Context};
use crate::http::Client;
pub use crate::http::Hosts;
use crate::plan::Attempt;

/// A Quote fetch's requests.
#[derive(Debug, Clone)]
pub struct Fetch {
    /// Per commodity, the requests to try in order; each list has at least one.
    pub items: Vec<Vec<Attempt>>,
    /// Each service's minimum interval between requests.
    pub intervals: BTreeMap<String, Duration>,
    /// How long one request may take.
    pub timeout: Duration,
    /// The local time zone, for answers that date a quote by Unix time.
    pub zone: TimeZone,
    /// The hosts its requests may go to.
    pub hosts: Hosts,
}

/// Why a commodity's requests gave no quote record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    /// An HTTP or format error, or a rate limit.
    ServiceError,
    /// No answer within the timeout.
    Timeout,
}

impl Reason {
    /// The stable name `price.fetchStatus` reports.
    pub fn code(self) -> &'static str {
        match self {
            Self::ServiceError => "service-error",
            Self::Timeout => "timeout",
        }
    }
}

/// How one commodity's requests ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// A service answered: with a quote, or, when no service had one, the last service's "no quote".
    Quote {
        /// The service that answered.
        service: String,
        /// Its answer.
        record: QuoteRecord,
    },
    /// The last request failed.
    Failed {
        /// The service last tried.
        service: Option<String>,
        /// Why.
        reason: Reason,
        /// The detail, with any key replaced by `***`.
        message: String,
    },
}

/// A running fetch. Dropping it cancels the fetch; it never waits for the fetch's thread.
#[derive(Debug)]
pub struct FetchHandle {
    _cancel: oneshot::Sender<()>,
}

/// Start `fetch` on its own thread. `on_item` is called as each commodity's requests end, with its index;
/// `on_done` once, with every outcome in order, unless the fetch is cancelled first.
pub fn start(
    fetch: Fetch,
    on_item: impl Fn(usize, &Outcome) + Send + Sync + 'static,
    on_done: impl FnOnce(Vec<Outcome>) + Send + 'static,
) -> std::io::Result<FetchHandle> {
    let (cancel, cancelled) = oneshot::channel();
    std::thread::Builder::new().name("red-ale-quote-fetch".into()).spawn(move || {
        let outcomes = match tokio::runtime::Builder::new_current_thread().enable_all().build() {
            Ok(runtime) => runtime.block_on(async {
                tokio::select! {
                    biased;
                    _ = cancelled => None,
                    outcomes = run(fetch, &on_item) => Some(outcomes),
                }
            }),
            Err(e) => Some(fail_all(fetch.items.len(), &format!("cannot start the fetch: {e}"))),
        };
        outcomes.map(on_done);
    })?;
    Ok(FetchHandle { _cancel: cancel })
}

fn fail_all(count: usize, message: &str) -> Vec<Outcome> {
    vec![
        Outcome::Failed {
            service: None,
            reason: Reason::ServiceError,
            message: message.to_owned()
        };
        count
    ]
}

/// A request waiting for its service's turn.
struct Job {
    attempt: Attempt,
    reply: oneshot::Sender<Outcome>,
}

type Queues = Arc<BTreeMap<String, mpsc::UnboundedSender<Job>>>;

async fn run(fetch: Fetch, on_item: &(dyn Fn(usize, &Outcome) + Send + Sync)) -> Vec<Outcome> {
    let client = match Client::new(fetch.hosts) {
        Ok(client) => client,
        Err(e) => return fail_all(fetch.items.len(), &e.to_string()),
    };
    let Fetch {
        items,
        intervals,
        timeout,
        zone,
        hosts: _,
    } = fetch;
    let mut workers = JoinSet::new();
    let queues: Queues = Arc::new(
        intervals
            .into_iter()
            .map(|(name, interval)| {
                let (queue, jobs) = mpsc::unbounded_channel();
                workers.spawn(worker(jobs, interval, timeout, client.clone(), zone.clone()));
                (name, queue)
            })
            .collect(),
    );
    let mut outcomes: Vec<Option<Outcome>> = vec![None; items.len()];
    let mut tasks = JoinSet::new();
    // Each commodity's first request is queued here, in commodity order, so each service hears them in that order.
    items.into_iter().enumerate().for_each(|(i, attempts)| {
        let first = attempts.first().map_or_else(|| Pending::Done(no_request()), |a| submit(&queues, a));
        tasks.spawn(item(i, attempts, Arc::clone(&queues), first));
    });
    while let Some(joined) = tasks.join_next().await {
        if let Ok((i, outcome)) = joined {
            on_item(i, &outcome);
            outcomes[i] = Some(outcome);
        }
    }
    outcomes.into_iter().map(|o| o.unwrap_or_else(no_request)).collect()
}

fn no_request() -> Outcome {
    Outcome::Failed {
        service: None,
        reason: Reason::ServiceError,
        message: "no request was made".to_owned(),
    }
}

/// A request queued, or one that ended before it was sent.
enum Pending {
    Waiting(String, oneshot::Receiver<Outcome>),
    Done(Outcome),
}

fn submit(queues: &Queues, attempt: &Attempt) -> Pending {
    let failed = |message: String| {
        Pending::Done(Outcome::Failed {
            service: Some(attempt.service.clone()),
            reason: Reason::ServiceError,
            message,
        })
    };
    if let Some(why) = &attempt.missing_key {
        return failed(why.clone());
    }
    let Some(queue) = queues.get(&attempt.service) else {
        return failed(format!("there is no Quote service {}", attempt.service));
    };
    let (reply, answer) = oneshot::channel();
    match queue.send(Job {
        attempt: attempt.clone(),
        reply,
    }) {
        Ok(()) => Pending::Waiting(attempt.service.clone(), answer),
        Err(_) => failed(format!("the Quote service {} stopped", attempt.service)),
    }
}

/// One commodity: its requests in turn until one gives a quote.
async fn item(index: usize, attempts: Vec<Attempt>, queues: Queues, first: Pending) -> (usize, Outcome) {
    let mut pending = first;
    let mut rest = attempts.iter().skip(1);
    loop {
        let outcome = match pending {
            Pending::Done(outcome) => outcome,
            Pending::Waiting(service, answer) => answer.await.unwrap_or_else(|_| Outcome::Failed {
                service: Some(service),
                reason: Reason::ServiceError,
                message: "the request was dropped".to_owned(),
            }),
        };
        let answered = matches!(&outcome, Outcome::Quote { record, .. } if record.success);
        match rest.next() {
            Some(next) if !answered => pending = submit(&queues, next),
            _ => return (index, outcome),
        }
    }
}

/// One service: its requests one at a time, starting at least `interval` apart.
async fn worker(mut jobs: mpsc::UnboundedReceiver<Job>, interval: Duration, timeout: Duration, client: Client, zone: TimeZone) {
    let mut last: Option<Instant> = None;
    while let Some(job) = jobs.recv().await {
        if let Some(previous) = last {
            tokio::time::sleep_until(previous + interval).await;
        }
        last = Some(Instant::now());
        let outcome = ask(&client, &job.attempt, timeout, &zone).await;
        // The commodity's task is gone only when the fetch is being dropped.
        job.reply.send(outcome).ok();
    }
}

/// Send one request and read its answer.
async fn ask(client: &Client, a: &Attempt, timeout: Duration, zone: &TimeZone) -> Outcome {
    let shown = a.url.shown();
    let failed = |reason, message: String| Outcome::Failed {
        service: Some(a.service.clone()),
        reason,
        message: a.url.redact(&message),
    };
    let (status, body) = match tokio::time::timeout(timeout, client.get(a.url.url())).await {
        Err(_) => return failed(Reason::Timeout, format!("{shown} did not answer within {} s", timeout.as_secs())),
        Ok(Err(e)) => return failed(Reason::ServiceError, format!("{shown}: {e}")),
        Ok(Ok(answer)) => answer,
    };
    if !status.is_success() {
        return failed(Reason::ServiceError, format!("{shown} answered HTTP {status}{}", excerpt(&body)));
    }
    let cx = Context {
        ask: &a.ask,
        currency: a.currency.as_deref(),
        pointers: &a.pointers,
        zone,
    };
    match format::parse(a.format, &body, &cx) {
        Ok(record) => Outcome::Quote {
            service: a.service.clone(),
            record: as_asked(record, a),
        },
        Err(e) => failed(Reason::ServiceError, format!("{shown}: {e}")),
    }
}

/// A rate's record as Finance::Quote's wrapper gives it: in the currency asked for, undated, and marked inverted when
/// the rate was asked the other way round (`5.17:libgnucash/quotes/finance-quote-wrapper.in:151-180`). Any key
/// echoed in a service's message is removed.
fn as_asked(mut record: QuoteRecord, a: &Attempt) -> QuoteRecord {
    if let Some(currency) = a.rate_currency() {
        record.currency = Some(currency.to_owned());
        record.date = None;
        record.inverted = a.inverted;
    }
    record.errormsg = record.errormsg.map(|m| a.url.redact(&m));
    record
}

/// The start of an error answer's text, for the message.
fn excerpt(body: &[u8]) -> String {
    let text = String::from_utf8_lossy(body);
    let words: Vec<&str> = text.split_whitespace().collect();
    let joined = words.join(" ");
    match joined.char_indices().nth(160) {
        _ if joined.is_empty() => String::new(),
        Some((cut, _)) => format!(": {}…", &joined[..cut]),
        None => format!(": {joined}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn excerpts_are_short_and_on_one_line() {
        assert_eq!(excerpt(b""), "");
        assert_eq!(excerpt(b"{\"message\":\n  \"not found\"}"), ": {\"message\": \"not found\"}");
        assert_eq!(excerpt("x".repeat(500).as_bytes()).chars().count(), 2 + 160 + 1);
    }

    #[test]
    fn reasons_have_stable_names() {
        assert_eq!((Reason::ServiceError.code(), Reason::Timeout.code()), ("service-error", "timeout"));
    }
}
