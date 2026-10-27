//! The transport-free dispatcher: a registry of methods over one [`Context`], and JSON-RPC 2.0 framing.
//!
//! The HTTP server, the CLI and (from v0.2) the MCP server all call [`Dispatcher::call`], so every door runs the same
//! code.

use std::collections::BTreeMap;

use red_ale_core::{Book, DomainResult, Env, TimeZone};
use red_ale_quotes::fetch::Hosts;
use red_ale_service::session::{EnvDebug, SystemEnv};
use red_ale_service::{ServiceError, Session, Settings};
use schemars::{JsonSchema, Schema, SchemaGenerator};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Map, Value, json};

use crate::error::{RpcError, codes};
use crate::quote_fetches::{Notifier, QuoteFetches};

/// What a method does to the book: MCP's `readOnlyHint` and `destructiveHint` come from this.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Effect {
    /// Reads only.
    Read,
    /// Adds or changes data.
    Write,
    /// Deletes or overwrites data.
    Destructive,
}

/// What a method that is not `read` changes: its Effect target (`design/api.md`, "Effects"). MCP's hints come from
/// the Effect alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Target {
    /// The open Book: the change is journaled and makes the Book dirty.
    Book,
    /// red-ale's Configuration only: works with no Book open and never makes one dirty.
    Configuration,
}

/// The door a call came through. A few changes only a person may make are refused through MCP
/// (`design/api/configuration.md`); every other method behaves the same whatever the door.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Origin {
    /// The JSON-RPC API: the HTTP server, `red-ale call`, and `red-ale exec`, which runs methods in process.
    #[default]
    JsonRpc,
    /// The MCP server, driven by an agent.
    Mcp,
}

type Handler = Box<dyn Fn(&mut Context, Value) -> Result<Value, RpcError> + Send + Sync>;
type SchemaFn = fn(&mut SchemaGenerator) -> Schema;

/// One JSON-RPC method.
pub struct Method {
    /// Name, namespaced by context: `account.create`.
    pub name: &'static str,
    /// One line saying what it does.
    pub summary: &'static str,
    /// What it does to the book.
    pub effect: Effect,
    /// What it changes; `None` for a `read` method.
    pub target: Option<Target>,
    params: SchemaFn,
    result: SchemaFn,
    handler: Handler,
}

impl std::fmt::Debug for Method {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut d = f.debug_struct("Method");
        d.field("name", &self.name)
            .field("effect", &self.effect)
            .field("target", &self.target)
            .finish_non_exhaustive()
    }
}

impl Method {
    /// The same method, changing red-ale's Configuration rather than the Book. A `read` method keeps no target.
    #[must_use]
    pub fn on_configuration(mut self) -> Self {
        self.target = self.target.map(|_| Target::Configuration);
        self
    }
}

/// Build a method from a typed handler. Params are by name (a JSON object); absent params read as `{}`. A method that
/// is not `read` targets the Book unless [`Method::on_configuration`] says otherwise.
pub fn method<P, R>(name: &'static str, summary: &'static str, effect: Effect, f: fn(&mut Context, P) -> Result<R, RpcError>) -> Method
where
    P: DeserializeOwned + JsonSchema + 'static,
    R: Serialize + JsonSchema + 'static,
{
    Method {
        name,
        summary,
        effect,
        target: (effect != Effect::Read).then_some(Target::Book),
        params: |g| g.subschema_for::<P>(),
        result: |g| g.subschema_for::<R>(),
        handler: Box::new(move |ctx, params| {
            let params = match params {
                Value::Null => Value::Object(Map::new()),
                Value::Object(_) => params,
                _ => return Err(RpcError::invalid_params("params must be an object (by-name)")),
            };
            let typed: P = serde_json::from_value(params).map_err(|e| RpcError::invalid_params(e.to_string()))?;
            let out = f(ctx, typed)?;
            serde_json::to_value(out).map_err(|e| RpcError::internal(format!("cannot encode the result: {e}")))
        }),
    }
}

/// Makes the environment (clock and GUIDs) for each new session; tests pass a deterministic one.
pub type EnvFactory = Box<dyn Fn() -> Box<dyn EnvDebug> + Send + Sync>;

/// The state methods run against: at most one open session.
pub struct Context {
    session: Option<Session>,
    /// Settings for new sessions.
    pub settings: Settings,
    env_factory: Option<EnvFactory>,
    origin: Origin,
    /// The open book's Quote fetches; cleared, cancelling a running one, when the book closes.
    pub(crate) quote_fetches: QuoteFetches,
    quote_notifier: Option<Notifier>,
    /// The hosts Quote fetches may ask.
    pub(crate) quote_hosts: Hosts,
}

impl std::fmt::Debug for Context {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Context")
            .field("session", &self.session)
            .field("settings", &self.settings)
            .finish_non_exhaustive()
    }
}

impl Context {
    /// The door the call being run came through.
    pub fn origin(&self) -> Origin {
        self.origin
    }

    /// The open session.
    pub fn session(&mut self) -> Result<&mut Session, RpcError> {
        self.session.as_mut().ok_or_else(|| ServiceError::NoSession.into())
    }

    /// True when a book is open.
    pub fn has_session(&self) -> bool {
        self.session.is_some()
    }

    /// Install a newly opened session, giving it the configured environment. Fails if one is open.
    pub fn install(&mut self, mut session: Session) -> Result<&mut Session, RpcError> {
        if let Some(open) = &self.session {
            return Err(ServiceError::AlreadyOpen {
                path: open.path().to_owned(),
            }
            .into());
        }
        if let Some(factory) = &self.env_factory {
            session.set_env_boxed(factory());
        }
        self.quote_fetches.clear();
        Ok(self.session.insert(session))
    }

    /// Refuse if a book is already open.
    pub fn require_no_session(&self) -> Result<(), RpcError> {
        match &self.session {
            Some(open) => Err(ServiceError::AlreadyOpen {
                path: open.path().to_owned(),
            }
            .into()),
            None => Ok(()),
        }
    }

    /// Remove the open session. Its Quote fetches go with it: a running one is cancelled and stores nothing.
    pub fn take_session(&mut self) -> Result<Session, RpcError> {
        self.quote_fetches.clear();
        self.session.take().ok_or_else(|| ServiceError::NoSession.into())
    }

    /// What a Quote fetch calls when its requests are done, if a server installed something.
    pub(crate) fn quote_notifier(&self) -> Option<Notifier> {
        self.quote_notifier.clone()
    }

    /// The local time zone: the open session's, else the one new sessions get, else the server process's.
    pub fn zone(&self) -> TimeZone {
        match (&self.session, &self.env_factory) {
            (Some(s), _) => s.zone().clone(),
            (None, Some(factory)) => factory().zone().clone(),
            (None, None) => red_ale_service::zone::local_zone().clone(),
        }
    }

    /// The open book.
    pub fn book(&mut self) -> Result<&Book, RpcError> {
        self.session().map(|s| s.book())
    }

    /// Run a query that needs the environment (for "today").
    pub fn query<T>(&mut self, f: impl FnOnce(&Book, &mut dyn Env) -> Result<T, RpcError>) -> Result<T, RpcError> {
        self.session()?.query(f)
    }

    /// Run a read that works with or without a book: `f` gets the open Book and its session's environment, or no Book
    /// and the environment a new session would get when none is open.
    pub fn query_any<T>(&mut self, f: impl FnOnce(Option<&Book>, &mut dyn Env) -> Result<T, RpcError>) -> Result<T, RpcError> {
        match (&mut self.session, &self.env_factory) {
            (Some(s), _) => s.query(|book, env| f(Some(book), env)),
            (None, Some(factory)) => f(None, &mut *factory()),
            (None, None) => f(None, &mut SystemEnv),
        }
    }

    /// Run a Mutation atomically.
    pub fn mutate<T>(&mut self, f: impl FnOnce(&mut Book, &mut dyn Env) -> DomainResult<T>) -> Result<T, RpcError> {
        self.session()?.try_mutate(|book, env| f(book, env).map_err(RpcError::from))
    }

    /// Run a Mutation whose closure also resolves wire references, and so fails with an [`RpcError`]. Atomic like
    /// [`Context::mutate`].
    pub fn mutate_rpc<T>(&mut self, f: impl FnOnce(&mut Book, &mut dyn Env) -> Result<T, RpcError>) -> Result<T, RpcError> {
        self.session()?.try_mutate(f)
    }
}

/// The method registry and the context it runs against.
pub struct Dispatcher {
    ctx: Context,
    methods: BTreeMap<&'static str, Method>,
}

impl std::fmt::Debug for Dispatcher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Dispatcher")
            .field("ctx", &self.ctx)
            .field("methods", &self.methods.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl Dispatcher {
    /// A dispatcher with every red-ale method and no book open.
    pub fn new(settings: Settings) -> Self {
        let methods = crate::methods::all().into_iter().map(|m| (m.name, m)).collect();
        let ctx = Context {
            session: None,
            settings,
            env_factory: None,
            origin: Origin::default(),
            quote_fetches: QuoteFetches::default(),
            quote_notifier: None,
            quote_hosts: Hosts::Any,
        };
        Self { ctx, methods }
    }

    /// Use `factory` for the environment of every session opened from now on.
    pub fn set_env_factory(&mut self, factory: EnvFactory) {
        self.ctx.env_factory = Some(factory);
    }

    /// Call `notify` from a Quote fetch's thread when its requests are done (see
    /// [`crate::quote_fetches::settle_when_fetched`]).
    pub fn set_quote_notifier(&mut self, notify: Notifier) {
        self.ctx.quote_notifier = Some(notify);
    }

    /// Let Quote fetches ask loopback hosts only, refusing any other before connecting. Test dispatchers turn this on
    /// so a test can never reach a real Quote service; red-ale itself never does.
    pub fn restrict_quotes_to_loopback(&mut self) {
        self.ctx.quote_hosts = Hosts::Loopback;
    }

    /// Store a finished Quote fetch's prices, if its requests are done. Every call does this first.
    pub fn settle_quotes(&mut self) {
        self.ctx.settle_quotes();
    }

    /// Wait for the running Quote fetch's requests, if one is running, then store its prices: what
    /// `red-ale exec` does after `price.fetchQuotes`.
    pub fn wait_for_quotes(&mut self) {
        self.ctx.wait_for_quotes();
    }

    /// The context, for a caller that opens a session directly.
    pub fn context(&mut self) -> &mut Context {
        &mut self.ctx
    }

    /// Every method, by name.
    pub fn methods(&self) -> impl Iterator<Item = &Method> {
        self.methods.values()
    }

    /// Call a method with by-name params, through the JSON-RPC API.
    pub fn call(&mut self, name: &str, params: Value) -> Result<Value, RpcError> {
        self.call_from(Origin::JsonRpc, name, params)
    }

    /// Call a method with by-name params, saying which door the call came through.
    pub fn call_from(&mut self, origin: Origin, name: &str, params: Value) -> Result<Value, RpcError> {
        self.ctx.origin = origin;
        self.ctx.settle_quotes();
        if name == "api.schema" {
            return Ok(self.schema());
        }
        let method = self
            .methods
            .get(name)
            .ok_or_else(|| RpcError::new(codes::METHOD_NOT_FOUND, format!("no method {name}")))?;
        (method.handler)(&mut self.ctx, params)
    }

    /// Handle one JSON-RPC 2.0 message, single or batch. Returns `None` when nothing should be sent back (a
    /// notification, or a batch of notifications).
    pub fn handle_text(&mut self, text: &str) -> Option<String> {
        let response = match serde_json::from_str::<Value>(text) {
            Err(e) => Some(error_response(Value::Null, &RpcError::new(codes::PARSE_ERROR, e.to_string()))),
            Ok(Value::Array(batch)) if batch.is_empty() => {
                Some(error_response(Value::Null, &RpcError::new(codes::INVALID_REQUEST, "empty batch")))
            }
            Ok(Value::Array(batch)) => {
                let replies: Vec<Value> = batch.into_iter().filter_map(|r| self.handle_value(r)).collect();
                (!replies.is_empty()).then_some(Value::Array(replies))
            }
            Ok(single) => self.handle_value(single),
        };
        response.map(|v| v.to_string())
    }

    fn handle_value(&mut self, request: Value) -> Option<Value> {
        let Value::Object(mut obj) = request else {
            return Some(error_response(
                Value::Null,
                &RpcError::new(codes::INVALID_REQUEST, "a request must be an object"),
            ));
        };
        let id = obj.remove("id");
        let reply_id = id.clone().unwrap_or(Value::Null);
        let valid_id = id.as_ref().is_none_or(|v| v.is_string() || v.is_number() || v.is_null());
        let method = obj.remove("method");
        let (Some(Value::String(method)), true, Some("2.0")) = (method, valid_id, obj.get("jsonrpc").and_then(Value::as_str)) else {
            return Some(error_response(
                reply_id,
                &RpcError::new(codes::INVALID_REQUEST, "not a JSON-RPC 2.0 request"),
            ));
        };
        let result = self.call(&method, obj.remove("params").unwrap_or(Value::Null));
        id.map(|id| match result {
            Ok(value) => json!({ "jsonrpc": "2.0", "id": id, "result": value }),
            Err(e) => error_response(id, &e),
        })
    }

    /// The API's schema document: every method with its params and result schemas, and the error codes.
    pub fn schema(&self) -> Value {
        let mut generator = SchemaGenerator::default();
        let methods: Vec<Value> = self
            .methods
            .values()
            .map(|m| {
                json!({
                    "name": m.name,
                    "summary": m.summary,
                    "effect": m.effect,
                    "target": m.target,
                    "params": (m.params)(&mut generator),
                    "result": (m.result)(&mut generator),
                })
            })
            .collect();
        let errors: Vec<Value> = codes::ALL
            .iter()
            .map(|(code, name)| json!({ "code": code, "name": name }))
            .collect();
        json!({
            "redAleApi": crate::API_VERSION,
            "methods": methods,
            "errors": errors,
            "$defs": generator.take_definitions(true),
        })
    }
}

fn error_response(id: Value, e: &RpcError) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": e })
}
