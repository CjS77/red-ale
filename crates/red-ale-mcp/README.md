# red-ale-mcp

red-ale's MCP server: every JSON-RPC method as a [Model Context Protocol](https://modelcontextprotocol.io) tool, and
every Page of the Manual as a resource, so AI agents can drive red-ale and look up how.

## Role

The crate turns red-ale's API into MCP tools, served over standard input and output with
[`rmcp`](https://crates.io/crates/rmcp), the official Rust SDK:

- **One tool per method**, generated from the dispatcher's method registry at start-up, `api.schema` included. A tool
  has the method's name (`account.create`), its summary and the lead of its Manual page as the description (about 60
  words at most, ending with the Page ID; the summary alone with `--tool-descriptions short`), and its params schema as
  the input schema, made self-contained.
- **Hints from the Effect.** *read* methods are `readOnlyHint`; *destructive* ones are `destructiveHint`; *write* ones
  are neither.
- **Calls run through the `Dispatcher`.** The result comes back as JSON text, and as structured content when it is an
  object. A red-ale error comes back as a tool result with `isError: true` and the error object, with its stable
  code and name, so an agent can see what went wrong and try again.
- **The Manual as resources.** Every Page is `red-ale://manual/<page-id>`, Markdown with its Examples as tool calls,
  and `red-ale://manual/method/{name}` is the template for methods. The instructions are the Start Page's summary.

## Where it fits

In domain-driven design terms the crate is a *conformist*: it takes MCP's model as the specification defines it and
maps red-ale's published language onto it. It holds no logic of its own, so a tool call and the same JSON-RPC call
always do the same thing.

```text
red-ale mcp  ->  red-ale-mcp  ->  red-ale-rpc (Dispatcher)  ->  red-ale-service  ->  red-ale-core
```

The design is in [`design/mcp.md`](../../design/mcp.md).

## What's inside

| Module   | Contents                                                                                     |
|----------|----------------------------------------------------------------------------------------------|
| `tools`  | The tool list: input schemas from the params schemas, annotations from the Effect            |
| `manual` | The Manual's Pages as resources, the instructions, and tool descriptions from the Pages      |
| `server` | `McpServer`, the MCP handler over a shared `Dispatcher`, and `serve` over any reader/writer  |

## Usage

The `red-ale` binary runs it:

```sh
red-ale mcp --book home.gnucash --config ~/.config/red-ale.json
```

In process, over standard input and output or any pair of async streams:

```rust
use std::sync::{Arc, Mutex};

use red_ale_rpc::Dispatcher;
use red_ale_service::Settings;

let dispatcher = Arc::new(Mutex::new(Dispatcher::new(Settings::default())));
red_ale_mcp::serve_stdio(dispatcher, 64 << 20, red_ale_mcp::ToolDescriptions::Full).await?;
```
