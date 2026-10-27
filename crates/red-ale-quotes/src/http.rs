//! The HTTP client Quote fetches use: HTTP/1.1 over plain TCP or TLS (rustls, with ring as its crypto provider and
//! the system's trust store). It follows no redirects, sends no cookies and keeps nothing between fetches.

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

/// A GET-only HTTP client.
#[derive(Clone)]
pub struct Client {
    inner: HyperClient<HttpsConnector<HttpConnector>, Empty<Bytes>>,
}

impl Client {
    /// A client trusting the system's certificate authorities.
    pub fn new() -> Result<Self, HttpError> {
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
        })
    }

    /// GET `url`: the status and up to 1 MiB of the body.
    pub async fn get(&self, url: &str) -> Result<(StatusCode, Bytes), HttpError> {
        let uri: Uri = url.parse().map_err(|e| HttpError(format!("not a URL: {e}")))?;
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

/// An error with its causes, `a: b: c`, since hyper's own message is only "client error (Connect)".
fn chain(e: &(dyn std::error::Error + 'static)) -> String {
    std::iter::successors(Some(e), |e| e.source())
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(": ")
}
