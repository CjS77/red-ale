//! Methods: online quotes, `price.fetchQuotes` and `price.fetchStatus`. See `design/api/quotes.md`.

use std::sync::Arc;

use red_ale_core::ops::quotes::{Quotable, named_quotable, quotable};
use red_ale_core::{Book, CommodityRef};
use red_ale_quotes::fetch::{self, Fetch};
use red_ale_quotes::plan::{Plan, Planner, QuoteSettings};
use red_ale_service::ServiceError;
use red_ale_service::config::Config;
use regex::{Regex, RegexBuilder};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::dispatch::{Context, Effect, Method, method};
use crate::error::{RpcError, codes};
use crate::methods::prices::PriceInfo;
use crate::quote_fetches::{Delivery, Shared};
use crate::wire::{CommodityId, Timestamp};

/// The online quotes methods.
pub fn methods() -> Vec<Method> {
    vec![
        method(
            "price.fetchQuotes",
            "Start a Quote fetch of online prices; returns its ID at once.",
            Effect::Write,
            fetch_quotes,
        ),
        method(
            "price.fetchStatus",
            "A Quote fetch's state and, per commodity, the price stored or why there is none.",
            Effect::Read,
            fetch_status,
        ),
    ]
}

/// Params of `price.fetchQuotes`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct FetchParams {
    /// Fetch these, whatever their quote flag; each needs a quote source (a currency has `currency`). Default: every
    /// commodity with quotes on and a quote source.
    #[serde(default)]
    pub commodities: Option<Vec<CommodityId>>,
    /// Only commodities in namespaces matching this pattern, case-insensitive and unanchored, as
    /// `gnucash-cli --namespace`. Not with `commodities`.
    #[serde(default)]
    pub namespace: Option<String>,
}

/// What `price.fetchQuotes` started.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FetchStarted {
    /// The Quote fetch's ID, for `price.fetchStatus`.
    pub fetch: String,
    /// The commodities it covers, in the order fetched.
    pub commodities: Vec<CommodityId>,
}

/// Params of `price.fetchStatus`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct StatusParams {
    /// The Quote fetch; default the latest.
    #[serde(default)]
    pub fetch: Option<String>,
}

/// Where a Quote fetch is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum FetchState {
    /// Its requests are running, or its prices are not yet stored.
    Running,
    /// Its prices are stored.
    Finished,
}

/// Where one commodity of a Quote fetch is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum ItemState {
    /// Its requests are running.
    Pending,
    /// A service answered; the price waits for the Mutation.
    Fetched,
    /// `price.add`'s rules ran on its price: see `outcome`.
    Stored,
    /// No price: see `reason`.
    Failed,
}

/// A Quote fetch.
#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FetchStatus {
    /// Its ID.
    pub fetch: String,
    /// `running` or `finished`.
    pub state: FetchState,
    /// When it started.
    pub started: Timestamp,
    /// When its prices were stored; null while running.
    pub finished: Option<Timestamp>,
    /// One entry per commodity, in the order fetched.
    pub commodities: Vec<CommodityFetch>,
    /// Commodities whose price went through `price.add`'s rules, once finished.
    pub stored: Option<usize>,
    /// Commodities with no price, once finished.
    pub failed: Option<usize>,
}

/// One commodity of a Quote fetch.
#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CommodityFetch {
    /// The commodity.
    pub commodity: CommodityId,
    /// Its quote source (`currency` for a currency).
    pub source: String,
    /// `pending`, `fetched`, `stored` or `failed`.
    pub state: ItemState,
    /// The Quote service that answered, or the last one tried.
    pub service: Option<String>,
    /// Once stored, `price.add`'s outcome: `added`, `replaced`, `duplicate` or `rejected`.
    pub outcome: Option<String>,
    /// The price added.
    pub price: Option<PriceInfo>,
    /// The same-day price that kept its place, or was replaced.
    pub existing: Option<PriceInfo>,
    /// Once failed: `no-result`, `quote-failed`, `no-currency`, `unknown-currency`, `no-price`,
    /// `price-parse-failure` (GnuCash's), or `unsupported-source`, `service-error`, `timeout`, `not-found`.
    pub reason: Option<String>,
    /// The detail, such as the service's own message; never holds a key.
    pub message: Option<String>,
}

impl CommodityFetch {
    fn new(q: &Quotable) -> Self {
        Self {
            commodity: q.commodity.clone().into(),
            source: q.source.clone(),
            state: ItemState::Pending,
            service: None,
            outcome: None,
            price: None,
            existing: None,
            reason: None,
            message: None,
        }
    }
}

fn fetch_quotes(ctx: &mut Context, p: FetchParams) -> Result<FetchStarted, RpcError> {
    if p.commodities.is_some() && p.namespace.is_some() {
        return Err(RpcError::invalid_params("give commodities or namespace, not both"));
    }
    if let Some(reason) = ctx.session()?.read_only_reasons().first() {
        return Err(ServiceError::ReadOnly {
            reason: reason.to_string(),
        }
        .into());
    }
    let config = ctx.settings.config.load()?;
    let settings =
        QuoteSettings::from_value(config.checked("quotes")?).map_err(|e| RpcError::invalid(format!("the Configuration's quotes: {e}")))?;
    if !settings.network {
        return Err(RpcError::new(
            codes::NETWORK_DISABLED,
            "network access is off: quotes.network is false in the Configuration; nothing was sent",
        ));
    }
    if ctx.quote_fetches.is_running() {
        return Err(RpcError::new(codes::CONFLICT, "a Quote fetch is already running").with_data(json!({ "reason": "fetch-running" })));
    }
    let namespace = p.namespace.as_deref().map(namespace_pattern).transpose()?;
    let book = ctx.book()?;
    let default_currency = config.default_currency(book)?;
    let quotables = select(book, p.commodities.as_deref(), namespace.as_ref(), &default_currency)?;
    if quotables.is_empty() {
        return Err(RpcError::invalid(
            "the Quote fetch covers no commodity (GnuCash: \"GncQuotes::Fetch called with no commodities\")",
        ));
    }
    start(ctx, &settings, &default_currency, quotables)
}

/// `gnucash-cli --namespace`'s pattern: case-insensitive and unanchored (`5.17:libgnucash/app-utils/gnc-quotes.cpp:1023-1035`).
fn namespace_pattern(pattern: &str) -> Result<Regex, RpcError> {
    RegexBuilder::new(pattern)
        .case_insensitive(true)
        .build()
        .map_err(|e| RpcError::invalid(format!("namespace pattern {pattern:?}: {e}")))
}

fn select(
    book: &Book,
    named: Option<&[CommodityId]>,
    namespace: Option<&Regex>,
    default: &CommodityRef,
) -> Result<Vec<Quotable>, RpcError> {
    let Some(named) = named else {
        return Ok(quotable(book, default, namespace));
    };
    let found: Vec<Quotable> = named
        .iter()
        .map(|c| named_quotable(book, &c.0, default))
        .filter_map(Result::transpose)
        .collect::<Result<_, _>>()?;
    let mut seen = std::collections::BTreeSet::new();
    Ok(found.into_iter().filter(|q| seen.insert(q.commodity.clone())).collect())
}

/// Plan the requests, record the status and start the fetch's thread.
fn start(
    ctx: &mut Context,
    settings: &QuoteSettings,
    default_currency: &CommodityRef,
    quotables: Vec<Quotable>,
) -> Result<FetchStarted, RpcError> {
    let defaults = QuoteSettings::from_value(Config::default().checked("quotes")?).map_err(|e| RpcError::internal(e.to_string()))?;
    let (today, now, zone) = ctx.query(|_, env| Ok((env.today(), env.now(), env.zone().clone())))?;
    let today = today.ok_or_else(|| RpcError::internal("today is outside the calendar"))?;
    let env = |var: &str| std::env::var(var).ok();
    let planner = Planner {
        settings,
        defaults: &defaults.services,
        default_currency: &default_currency.mnemonic,
        today,
        env: &env,
    };
    let id = ctx.quote_fetches.next_id();
    let mut status = FetchStatus {
        fetch: id.clone(),
        state: FetchState::Running,
        started: Timestamp(now),
        finished: None,
        commodities: quotables.iter().map(CommodityFetch::new).collect(),
        stored: None,
        failed: None,
    };
    let mut items = Vec::new();
    let mut requests = Vec::new();
    quotables
        .iter()
        .map(|q| planner.plan(q))
        .zip(status.commodities.iter_mut())
        .enumerate()
        .for_each(|(entry, (plan, item))| match plan {
            Plan::Requests(attempts) => {
                items.push(entry);
                requests.push(attempts);
            }
            Plan::Unsupported(why) => {
                item.state = ItemState::Failed;
                item.reason = Some("unsupported-source".to_owned());
                item.message = Some(why);
            }
        });
    let commodities = quotables.into_iter().map(|q| q.commodity.into()).collect();
    let shared = Shared::new(status);
    let progress = Arc::clone(&shared);
    let item_entries = items.clone();
    let delivery = Delivery::new(Arc::clone(&shared), ctx.quote_notifier());
    let fetch = Fetch {
        items: requests,
        intervals: settings.intervals(),
        timeout: settings.timeout(),
        zone,
        hosts: ctx.quote_hosts,
    };
    let on_item = move |i: usize, outcome: &_| {
        item_entries
            .get(i)
            .into_iter()
            .for_each(|&entry| progress.item_done(entry, outcome));
    };
    let handle = fetch::start(fetch, on_item, move |outcomes| delivery.deliver(outcomes))
        .map_err(|e| RpcError::internal(format!("cannot start the Quote fetch: {e}")))?;
    ctx.quote_fetches.start(shared, items, handle);
    Ok(FetchStarted { fetch: id, commodities })
}

fn fetch_status(ctx: &mut Context, p: StatusParams) -> Result<FetchStatus, RpcError> {
    ctx.session()?;
    ctx.quote_fetches
        .status(p.fetch.as_deref())
        .ok_or_else(|| RpcError::not_found("quoteFetch", p.fetch.as_deref().unwrap_or("latest")))
}
