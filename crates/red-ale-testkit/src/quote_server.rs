//! A Quote service for tests: an HTTP/1.1 server on 127.0.0.1 that answers each path with a recorded answer, after
//! an optional delay, and records every connection and request. No test that fetches quotes reaches the network.

use std::io::{BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

/// How the server answers one path.
#[derive(Debug, Clone)]
pub struct Answer {
    /// The HTTP status.
    pub status: u16,
    /// The body.
    pub body: Vec<u8>,
    /// How long to wait before answering.
    pub delay: Duration,
}

impl Answer {
    /// A 200 answer with `body`.
    pub fn ok(body: impl Into<Vec<u8>>) -> Self {
        Self {
            status: 200,
            body: body.into(),
            delay: Duration::ZERO,
        }
    }

    /// A 200 answer with a recorded answer from `crates/red-ale-quotes/tests/fixtures`.
    pub fn recorded(name: &str) -> Self {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../red-ale-quotes/tests/fixtures")
            .join(name);
        Self::ok(std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display())))
    }

    /// An answer with another status.
    pub fn status(status: u16, body: impl Into<Vec<u8>>) -> Self {
        Self {
            status,
            body: body.into(),
            delay: Duration::ZERO,
        }
    }

    /// The same answer, after `delay`.
    #[must_use]
    pub fn after(mut self, delay: Duration) -> Self {
        self.delay = delay;
        self
    }
}

/// A request the server received.
#[derive(Debug, Clone)]
pub struct Hit {
    /// The request target: path and query.
    pub target: String,
    /// When it arrived.
    pub at: Instant,
}

#[derive(Default)]
struct Log {
    hits: Mutex<Vec<Hit>>,
    connections: AtomicUsize,
}

/// A running test server; stopped when dropped.
pub struct QuoteServer {
    addr: SocketAddr,
    log: Arc<Log>,
    stop: Arc<AtomicBool>,
}

impl std::fmt::Debug for QuoteServer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("QuoteServer").field("addr", &self.addr).finish_non_exhaustive()
    }
}

impl QuoteServer {
    /// Serve `answers`: each request's path (without its query) is looked up; any other path is a 404.
    pub fn start(answers: Vec<(&str, Answer)>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind 127.0.0.1");
        let addr = listener.local_addr().expect("local address");
        let answers: Arc<Vec<(String, Answer)>> = Arc::new(answers.into_iter().map(|(p, a)| (p.to_owned(), a)).collect());
        let log = Arc::new(Log::default());
        let stop = Arc::new(AtomicBool::new(false));
        let (thread_log, thread_stop) = (Arc::clone(&log), Arc::clone(&stop));
        std::thread::spawn(move || {
            listener
                .incoming()
                .take_while(|_| !thread_stop.load(Ordering::SeqCst))
                .flatten()
                .for_each(|stream| {
                    thread_log.connections.fetch_add(1, Ordering::SeqCst);
                    let (answers, log) = (Arc::clone(&answers), Arc::clone(&thread_log));
                    std::thread::spawn(move || serve(&stream, &answers, &log));
                });
        });
        Self { addr, log, stop }
    }

    /// The server's base URL, `http://127.0.0.1:<port>`.
    pub fn url(&self) -> String {
        format!("http://{}", self.addr)
    }

    /// The requests received so far.
    pub fn hits(&self) -> Vec<Hit> {
        self.log.hits.lock().unwrap_or_else(PoisonError::into_inner).clone()
    }

    /// The connections accepted so far, whether or not a request followed.
    pub fn connections(&self) -> usize {
        self.log.connections.load(Ordering::SeqCst)
    }
}

impl Drop for QuoteServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        // Wake the accept loop so it sees the flag.
        drop(TcpStream::connect(self.addr));
    }
}

fn serve(stream: &TcpStream, answers: &[(String, Answer)], log: &Log) {
    let mut reader = BufReader::new(stream);
    let mut request_line = String::new();
    if reader.read_line(&mut request_line).is_err() {
        return;
    }
    // Read the headers up to the blank line; the server needs none of them.
    std::iter::from_fn(|| {
        let mut line = String::new();
        reader.read_line(&mut line).ok().filter(|n| *n > 2).map(|_| line)
    })
    .for_each(drop);
    let target = request_line.split_whitespace().nth(1).unwrap_or_default().to_owned();
    log.hits.lock().unwrap_or_else(PoisonError::into_inner).push(Hit {
        target: target.clone(),
        at: Instant::now(),
    });
    let path = target.split('?').next().unwrap_or_default();
    let answer = answers
        .iter()
        .find(|(p, _)| p == path)
        .map_or_else(|| Answer::status(404, "{\"message\":\"not found\"}"), |(_, a)| a.clone());
    std::thread::sleep(answer.delay);
    let head = format!(
        "HTTP/1.1 {} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        answer.status,
        answer.body.len()
    );
    let mut stream = stream;
    // The client may have gone, as a cancelled fetch does.
    stream.write_all(head.as_bytes()).and_then(|()| stream.write_all(&answer.body)).ok();
}
