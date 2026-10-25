# red-ale-rpc

red-ale's JSON-RPC 2.0 API: every method, its params and results, its error codes, and the HTTP server.

## Role

Everything red-ale can do is a JSON-RPC method in this crate, from `book.open` to `invoice.post`. The crate defines:

- **The methods**, named by area (`account.create`, `transaction.void`, `price.add`, `lot.list`, …). Each method is
  marked as *read*, *write* or *destructive*, and each one is atomic: it applies completely or not at all.
- **The wire types.** Amounts travel as exact fractions (`{"num": "1234", "denom": "100"}`) or decimal strings, and
  JSON numbers are refused, so no amount ever passes through floating point. Accounts can be named by GUID or full
  name.
- **Stable error codes:** the JSON-RPC protocol codes, plus red-ale's own (`1000`–`1012` for books and sessions,
  `2000`–`2004` for bookkeeping errors such as `unbalanced`).
- **The schema.** Every method's params and results are described as JSON Schema, generated from the Rust types. The
  `api.schema` method returns it, and `red-ale schema` prints it.
- **The `Dispatcher`.** It routes a method name and params to the right handler, with no transport attached.
- **The server.** It puts the dispatcher behind HTTP, on localhost.

## Where it fits

In domain-driven design terms, the API is red-ale's *published language*: the contract other programs build on. It
is versioned (`redAleApi` is `0.1.0`), and within a major version it only grows: new methods, new optional params and
new result fields. Nothing released is renamed or removed.

```text
HTTP, CLI, MCP (v0.2)  ->  red-ale-rpc  ->  red-ale-service  ->  red-ale-core
```

The methods translate between wire types and core types, and call core operations through a `red-ale-service`
session. They hold no bookkeeping rules themselves. The CLI, the HTTP server and, from v0.2, the MCP server all go
through the same `Dispatcher`, so every route into red-ale runs the same code.

Each area's methods are documented in [`design/api/`](../../design/api), with the conventions in
[`design/api.md`](../../design/api.md).

## What's inside

| Module | Contents |
|--------|----------|
| `dispatch` | `Dispatcher`, `Context`, `Method`, and `method()` to declare one |
| `methods` | One module per area: `book`, `accounts`, `transactions`, `reconcile`, `scrub`, `commodities`, `prices`, `lots`, `period`, `business` |
| `wire` | Amounts, GUIDs, dates, timestamps, commodity IDs and account references |
| `error` | `RpcError` and the error `codes` |
| `server` | `serve`: the HTTP server |

## Usage

Call methods in-process, with no server:

```rust
use red_ale_rpc::Dispatcher;
use red_ale_service::Settings;
use serde_json::json;

let mut d = Dispatcher::new(Settings::default());
d.call("book.open", json!({ "path": "home.gnucash" }))?;
let balance = d.call("account.balance", json!({ "account": "Assets:Checking" }))?;
println!("{}", balance["balances"]["balance"]);

// Raw JSON-RPC text in, raw JSON-RPC text out, as the HTTP server does for each request.
let reply = d.handle_text(r#"{"jsonrpc": "2.0", "id": 1, "method": "book.info"}"#);

d.call("book.close", json!({ "save": true }))?;
```

Add a method by writing a handler that takes typed params and returns a typed result, then registering it in its
area's `methods()` list:

```rust
pub fn methods() -> Vec<Method> {
    vec![
        method("period.get", "The configured accounting period and fiscal year end.", Effect::Read, get),
        // ...
    ]
}
```

The params and result types derive `Deserialize`/`Serialize` and `JsonSchema`, which is how the schema stays in step
with the code.
