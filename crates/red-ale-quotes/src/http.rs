//! The HTTP client Quote fetches use: HTTP/1.1 over plain TCP or TLS (rustls, with ring as its crypto provider and
//! the system's trust store). It follows no redirects, sends no cookies and keeps nothing between fetches.

use std::net::IpAddr;
use std::sync::Arc;

use http_body_util::{BodyExt, Empty, Limited};
use hyper::body::Bytes;
use hyper::{Request, StatusCode, Uri};
use hyper_rustls::{HttpsConnector, HttpsConnectorBuilder};
use hyper_util::client::legacy::Client as HyperClient;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::rt::TokioExecutor;
use rustls::{ClientConfig, RootCertStore};

/// The largest answer read; a quote is a few kilobytes at most.
const MAX_BODY: usize = 1 << 20;

/// An HTTP failure. Its text holds no URL, so it can't hold a key.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct HttpError(String);

/// The hosts a Quote fetch may ask.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Hosts {
    /// Any host: what red-ale itself uses.
    #[default]
    Any,
    /// Loopback hosts only (127.0.0.0/8, `::1`, `localhost`), so a test's fetch can never reach a real service,
    /// even one it forgot to override. Any other host is refused before a name is looked up or a connection made.
    Loopback,
}

/// A GET-only HTTP client.
#[derive(Clone)]
pub struct Client {
    inner: HyperClient<HttpsConnector<HttpConnector>, Empty<Bytes>>,
    hosts: Hosts,
}

impl Client {
    /// A client trusting the system's certificate authorities, asking only `hosts`.
    pub fn new(hosts: Hosts) -> Result<Self, HttpError> {
        let mut roots = RootCertStore::empty();
        roots.add_parsable_certificates(rustls_native_certs::load_native_certs().certs);
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let tls = ClientConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .map_err(|e| HttpError(format!("cannot set up TLS: {e}")))?
            .with_root_certificates(roots)
            .with_no_client_auth();
        let connector = HttpsConnectorBuilder::new()
            .with_tls_config(tls)
            .https_or_http()
            .enable_http1()
            .build();
        Ok(Self {
            inner: HyperClient::builder(TokioExecutor::new()).build(connector),
            hosts,
        })
    }

    /// GET `url`: the status and up to 1 MiB of the body.
    pub async fn get(&self, url: &str) -> Result<(StatusCode, Bytes), HttpError> {
        let uri: Uri = url.parse().map_err(|e| HttpError(format!("not a URL: {e}")))?;
        self.allow(&uri)?;
        let request = Request::get(uri)
            .header(hyper::header::USER_AGENT, concat!("red-ale/", env!("CARGO_PKG_VERSION")))
            .header(hyper::header::ACCEPT, "application/json")
            .body(Empty::new())
            .map_err(|e| HttpError(e.to_string()))?;
        let response = self.inner.request(request).await.map_err(|e| HttpError(chain(&e)))?;
        let status = response.status();
        let body = Limited::new(response.into_body(), MAX_BODY)
            .collect()
            .await
            .map_err(|e| HttpError(format!("reading the answer: {e}")))?;
        Ok((status, body.to_bytes()))
    }
}

impl Client {
    /// Refuse `uri` unless its host is one this client may ask. The message names no host, as a host can't be
    /// told apart from the rest of a URL that may hold a key.
    fn allow(&self, uri: &Uri) -> Result<(), HttpError> {
        match self.hosts {
            Hosts::Loopback if !uri.host().is_some_and(is_loopback) => Err(HttpError(
                "refused: this client asks only loopback hosts (127.0.0.0/8, ::1, localhost)".to_owned(),
            )),
            _ => Ok(()),
        }
    }
}

/// True for `localhost` and loopback addresses, IPv6 ones in brackets as a URI writes them.
fn is_loopback(host: &str) -> bool {
    let bare = host.strip_prefix('[').and_then(|h| h.strip_suffix(']')).unwrap_or(host);
    host.eq_ignore_ascii_case("localhost") || bare.parse::<IpAddr>().is_ok_and(|ip| ip.to_canonical().is_loopback())
}

/// An error with its causes, `a: b: c`, since hyper's own message is only "client error (Connect)".
fn chain(e: &(dyn std::error::Error + 'static)) -> String {
    std::iter::successors(Some(e), |e| e.source())
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(": ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loopback_hosts_are_told_from_others() {
        let allowed = ["127.0.0.1", "127.5.5.5", "[::1]", "localhost", "LocalHost", "[::ffff:127.0.0.1]"];
        let refused = [
            "example.com",
            "10.0.0.1",
            "192.0.2.1",
            "[2001:db8::1]",
            "localhost.example",
            "127.0.0.1.example",
        ];
        assert!(allowed.iter().all(|h| is_loopback(h)), "{allowed:?}");
        assert!(!refused.iter().any(|h| is_loopback(h)), "{refused:?}");
    }

    #[test]
    fn a_loopback_client_refuses_other_hosts_before_connecting() {
        let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        let client = Client::new(Hosts::Loopback).unwrap();
        let err = runtime.block_on(client.get("http://192.0.2.1/quote?apikey=SECRET")).unwrap_err();
        assert!(err.to_string().starts_with("refused:"), "{err}");
        assert!(
            !err.to_string().contains("192.0.2.1") && !err.to_string().contains("SECRET"),
            "{err}"
        );
    }
}
