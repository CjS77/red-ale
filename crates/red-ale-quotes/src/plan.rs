//! From a commodity to the requests that may answer it, as the Configuration's Quote services and sources say
//! (`design/api/quotes.md`, "Quote sources and Quote services", "Keys and the network").
//!
//! A security is asked of each service of its quote source in turn. A currency is asked of the `currency` source's
//! services for its rate to the default currency, then, if none answers, for the rate the other way, to be
//! inverted, as Finance::Quote's wrapper asks (`5.17:libgnucash/quotes/finance-quote-wrapper.in:151-180`).

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::time::Duration;

use red_ale_core::Date;
use red_ale_core::ops::commodities::CURRENCY_QUOTE_SOURCE;
use red_ale_core::ops::quotes::Quotable;
use red_ale_service::config::schema::origin;
use serde::Deserialize;
use serde_json::Value;

use crate::format::{Ask, Format, Pointers, currency_code, request_symbol};
use crate::secret::{KeyedUrl, Secret, fill, wants_key};

/// The `quotes` settings, with their defaults filled in.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuoteSettings {
    /// Quote fetches may reach the network.
    pub network: bool,
    /// How long one request may take, in seconds.
    pub timeout_seconds: u64,
    /// Quote services by name.
    pub services: BTreeMap<String, Service>,
    /// Quote source names to the services that answer them, in order.
    pub sources: BTreeMap<String, Vec<String>>,
}

impl QuoteSettings {
    /// Read the `quotes` group of the Configuration (`Config::checked("quotes")`).
    pub fn from_value(quotes: Value) -> Result<Self, serde_json::Error> {
        serde_json::from_value(quotes)
    }

    /// The per-request timeout.
    pub fn timeout(&self) -> Duration {
        Duration::from_secs(self.timeout_seconds)
    }

    /// Each service's minimum interval between requests.
    pub fn intervals(&self) -> BTreeMap<String, Duration> {
        self.services
            .iter()
            .map(|(name, s)| (name.clone(), Duration::from_secs(s.min_interval_seconds)))
            .collect()
    }
}

/// A Quote service.
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Service {
    /// The response format's name.
    pub format: String,
    /// The URL template for securities.
    #[serde(default)]
    pub security_url: Option<String>,
    /// The URL template for currency rates.
    #[serde(default)]
    pub currency_url: Option<String>,
    #[serde(default)]
    key: Option<String>,
    /// The environment variable to take the key from when `key` is unset.
    #[serde(default)]
    pub key_env: Option<String>,
    /// The least time between two requests, in seconds.
    #[serde(default)]
    pub min_interval_seconds: u64,
    /// The currency of a price when the answer has none.
    #[serde(default)]
    pub currency: Option<String>,
    /// The `json` format's pointers.
    #[serde(default)]
    pub pointers: Pointers,
}

impl fmt::Debug for Service {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Service")
            .field("format", &self.format)
            .field("security_url", &self.security_url)
            .field("currency_url", &self.currency_url)
            .field("key", &self.key.as_ref().map(|_| crate::secret::MASK))
            .field("key_env", &self.key_env)
            .finish_non_exhaustive()
    }
}

impl Service {
    fn urls(&self) -> impl Iterator<Item = &str> {
        [&self.security_url, &self.currency_url].into_iter().flatten().map(String::as_str)
    }
}

/// One request that may answer a commodity.
#[derive(Debug, Clone)]
pub struct Attempt {
    /// The service asked.
    pub service: String,
    /// How its answer is read.
    pub format: Format,
    /// What is asked.
    pub ask: Ask,
    /// The answer is the rate the other way round.
    pub inverted: bool,
    /// Where to ask.
    pub url: KeyedUrl,
    /// The service's `currency` setting, an ISO code.
    pub currency: Option<String>,
    /// The `json` format's pointers.
    pub pointers: Pointers,
    /// Why the request can't be sent: its URL wants a key and the service has none.
    pub missing_key: Option<String>,
}

impl Attempt {
    /// The currency a rate's answer is in: the one Finance::Quote's wrapper names (`finance-quote-wrapper.in:172`),
    /// the default currency, whichever way the rate was asked.
    pub fn rate_currency(&self) -> Option<&str> {
        match &self.ask {
            Ask::Rate { base, .. } if self.inverted => Some(base),
            Ask::Rate { quote, .. } => Some(quote),
            Ask::Security { .. } => None,
        }
    }
}

/// How a commodity will be fetched.
#[derive(Debug, Clone)]
pub enum Plan {
    /// These requests, in order, until one gives a quote.
    Requests(Vec<Attempt>),
    /// No service answers its source: why.
    Unsupported(String),
}

/// Plans the requests of a Quote fetch.
pub struct Planner<'a> {
    /// The Configuration's `quotes` settings.
    pub settings: &'a QuoteSettings,
    /// The default Quote services: a default service's environment key is used only while its URLs are on its
    /// default hosts.
    pub defaults: &'a BTreeMap<String, Service>,
    /// The default currency's ISO code.
    pub default_currency: &'a str,
    /// Today's local date, for `{date}`.
    pub today: Date,
    /// Reads an environment variable.
    pub env: &'a dyn Fn(&str) -> Option<String>,
}

impl fmt::Debug for Planner<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Planner")
            .field("default_currency", &self.default_currency)
            .field("today", &self.today)
            .finish_non_exhaustive()
    }
}

impl Planner<'_> {
    /// The requests for one commodity.
    pub fn plan(&self, q: &Quotable) -> Plan {
        let source = if q.commodity.is_currency() {
            CURRENCY_QUOTE_SOURCE
        } else {
            q.source.as_str()
        };
        let Some(names) = self.settings.sources.get(source) else {
            return Plan::Unsupported(format!(
                "no Quote service answers the quote source {source:?} (quotes.sources.{source})"
            ));
        };
        let mnemonic = q.commodity.mnemonic.as_str();
        let asks: Vec<(Ask, bool)> = if q.commodity.is_currency() {
            let rate = |base: &str, quote: &str| Ask::Rate {
                base: base.to_owned(),
                quote: quote.to_owned(),
            };
            vec![
                (rate(mnemonic, self.default_currency), false),
                (rate(self.default_currency, mnemonic), true),
            ]
        } else {
            vec![(
                Ask::Security {
                    symbol: mnemonic.to_owned(),
                },
                false,
            )]
        };
        let attempts: Vec<Attempt> = asks
            .iter()
            .flat_map(|(ask, inverted)| names.iter().filter_map(move |name| self.attempt(name, ask, *inverted)))
            .collect();
        if attempts.is_empty() {
            let kind = if q.commodity.is_currency() { "currencyUrl" } else { "securityUrl" };
            return Plan::Unsupported(format!("no service of the quote source {source:?} has a {kind}"));
        }
        Plan::Requests(attempts)
    }

    fn attempt(&self, name: &str, ask: &Ask, inverted: bool) -> Option<Attempt> {
        let service = self.settings.services.get(name)?;
        let format: Format = service.format.parse().ok()?;
        let (template, values) = match ask {
            Ask::Security { symbol } => (service.security_url.as_deref()?, vec![("symbol", request_symbol(format, symbol))]),
            Ask::Rate { base, quote } => (
                service.currency_url.as_deref()?,
                vec![("base", base.as_str()), ("quote", quote.as_str())],
            ),
        };
        let today = self.today.to_string();
        let values: Vec<(&str, &str)> = values.into_iter().chain([("date", today.as_str())]).collect();
        let key = self.key(name, service);
        let missing_key = (wants_key(template) && key.is_none()).then(|| self.missing_key(name, service));
        Some(Attempt {
            service: name.to_owned(),
            format,
            ask: ask.clone(),
            inverted,
            url: fill(template, &values, key.as_ref()),
            currency: service.currency.as_deref().map(|c| currency_code(c).to_owned()),
            pointers: service.pointers.clone(),
            missing_key,
        })
    }

    /// The service's key: its `key`, else its `keyEnv` variable, which a default service may use only while every
    /// URL it has is on one of its default hosts.
    fn key(&self, name: &str, service: &Service) -> Option<Secret> {
        if let Some(key) = service.key.clone().and_then(Secret::new) {
            return Some(key);
        }
        let var = service.key_env.as_deref()?;
        self.on_default_hosts(name, service)
            .then(|| (self.env)(var))
            .flatten()
            .and_then(Secret::new)
    }

    fn on_default_hosts(&self, name: &str, service: &Service) -> bool {
        let Some(default) = self.defaults.get(name) else { return true };
        let homes: BTreeSet<String> = default.urls().filter_map(origin).collect();
        service.urls().all(|url| origin(url).is_some_and(|o| homes.contains(&o)))
    }

    fn missing_key(&self, name: &str, service: &Service) -> String {
        let env = service.key_env.as_deref().map_or_else(String::new, |var| {
            if self.on_default_hosts(name, service) {
                format!(" or the environment variable {var}")
            } else {
                format!(" ({var} is not used while the service's URLs are off its default hosts)")
            }
        });
        format!("the Quote service {name} needs a key: set quotes.services.{name}.key{env}")
    }
}

#[cfg(test)]
mod tests {
    use red_ale_core::CommodityRef;
    use serde_json::json;

    use super::*;

    fn settings(quotes: Value) -> QuoteSettings {
        QuoteSettings::from_value(quotes).unwrap()
    }

    fn defaults() -> QuoteSettings {
        settings(red_ale_service::config::Config::default().checked("quotes").unwrap())
    }

    fn quotable(id: CommodityRef, source: &str) -> Quotable {
        Quotable {
            commodity: id,
            source: source.into(),
        }
    }

    fn plan_with(s: &QuoteSettings, q: &Quotable, env: &dyn Fn(&str) -> Option<String>) -> Plan {
        let d = defaults();
        let planner = Planner {
            settings: s,
            defaults: &d.services,
            default_currency: "USD",
            today: Date::from_ymd(2024, 5, 3).unwrap(),
            env,
        };
        planner.plan(q)
    }

    fn shown(plan: &Plan) -> Vec<String> {
        match plan {
            Plan::Requests(a) => a
                .iter()
                .map(|a| format!("{} {} {}", a.service, a.inverted, a.url.shown()))
                .collect(),
            Plan::Unsupported(why) => vec![why.clone()],
        }
    }

    #[test]
    fn currencies_ask_both_ways_round() {
        let env = |v: &str| (v == "ALPHAVANTAGE_API_KEY").then(|| "k".to_owned());
        let plan = plan_with(&defaults(), &quotable(CommodityRef::currency("EUR"), "currency"), &env);
        assert_eq!(
            shown(&plan),
            [
                "frankfurter false https://api.frankfurter.dev/v1/latest?base=EUR&symbols=USD",
                "alphavantage false https://www.alphavantage.co/query?function=CURRENCY_EXCHANGE_RATE&from_currency=EUR&to_currency=USD&apikey=***",
                "frankfurter true https://api.frankfurter.dev/v1/latest?base=USD&symbols=EUR",
                "alphavantage true https://www.alphavantage.co/query?function=CURRENCY_EXCHANGE_RATE&from_currency=USD&to_currency=EUR&apikey=***",
            ]
        );
        let Plan::Requests(a) = plan else { panic!() };
        assert_eq!((a[0].rate_currency(), a[2].rate_currency()), (Some("USD"), Some("USD")));
        assert!(a.iter().all(|a| a.missing_key.is_none()));
    }

    #[test]
    fn securities_ask_their_sources_services() {
        let plan = plan_with(&defaults(), &quotable(CommodityRef::new("NASDAQ", "AAPL"), "nasdaq"), &|_| None);
        let Plan::Requests(a) = &plan else { panic!() };
        assert_eq!(
            a.iter().map(|a| a.service.as_str()).collect::<Vec<_>>(),
            ["alphavantage", "twelvedata"]
        );
        assert!(a[0].missing_key.as_deref().unwrap().contains("ALPHAVANTAGE_API_KEY"));
        let none = plan_with(&defaults(), &quotable(CommodityRef::new("FUND", "X"), "tiaacref"), &|_| None);
        assert!(matches!(none, Plan::Unsupported(why) if why.contains("tiaacref")));
    }

    #[test]
    fn an_environment_key_stays_on_its_default_host() {
        let mut s = defaults();
        s.services.get_mut("alphavantage").unwrap().security_url = Some("https://evil.example/q?s={symbol}&k={key}".into());
        let env = |_: &str| Some("from-env".to_owned());
        let plan = plan_with(&s, &quotable(CommodityRef::new("NASDAQ", "AAPL"), "alphavantage"), &env);
        let Plan::Requests(a) = &plan else { panic!() };
        assert_eq!(a[0].url.url(), "https://evil.example/q?s=AAPL&k=");
        assert!(a[0].missing_key.as_deref().unwrap().contains("off its default hosts"));
        let own = settings(json!({ "network": true, "timeoutSeconds": 5, "sources": { "mine": ["mine"] },
            "services": { "mine": { "format": "json", "securityUrl": "http://127.0.0.1:9/{symbol}?k={key}", "keyEnv": "MY_KEY" } } }));
        let plan = plan_with(&own, &quotable(CommodityRef::new("X", "ACME"), "mine"), &env);
        let Plan::Requests(a) = &plan else { panic!() };
        assert_eq!(a[0].url.url(), "http://127.0.0.1:9/ACME?k=from-env");
    }

    #[test]
    fn a_configured_key_wins_and_never_shows() {
        let s = settings(json!({ "network": true, "timeoutSeconds": 5, "sources": { "mine": ["mine"] },
            "services": { "mine": { "format": "json", "securityUrl": "http://127.0.0.1:9/{symbol}?k={key}", "key": "SECRET", "currency": "CURRENCY::EUR" } } }));
        assert!(!format!("{s:?}").contains("SECRET"));
        let plan = plan_with(&s, &quotable(CommodityRef::new("X", "ACME"), "mine"), &|_| Some("env".into()));
        assert!(!format!("{plan:?}").contains("SECRET"));
        let Plan::Requests(a) = &plan else { panic!() };
        assert_eq!(
            (a[0].url.url(), a[0].currency.as_deref()),
            ("http://127.0.0.1:9/ACME?k=SECRET", Some("EUR"))
        );
    }
}
