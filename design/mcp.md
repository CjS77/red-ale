# The MCP server

`red-ale mcp` serves red-ale's API to AI agents over the [Model Context Protocol](https://modelcontextprotocol.io).
It is the third door onto the one `Dispatcher` (see [`api.md`](api.md)): every MCP tool is a JSON-RPC method, and a
tool call runs exactly the code the HTTP server and the CLI run. It also serves the Manual
([`manual.md`](manual.md)): every Page is a resource.

## Bounded context

`red-ale-mcp` is a **conformist** adapter: it takes the MCP specification's model (tools, annotations, tool results,
resources) as it is, and maps red-ale's published language and the Manual onto it. It has no logic of its own. It never interprets params,
never changes a result and never decides what a method may do; anything that would need that belongs in
`red-ale-rpc` or below, so all doors get it. The one fact it passes on is that a call came through MCP
(`Origin::Mcp`), since `config.set` refuses keys and the network switch to agents; the rule itself is in
`red-ale-rpc` ([`api/configuration.md`](api/configuration.md)).

It uses [`rmcp`](https://crates.io/crates/rmcp), the official Rust SDK (Apache-2.0), with the stdio transport only.

## Protocol

- **Revision.** red-ale is designed and tested against the
  [2025-11-25 specification](https://modelcontextprotocol.io/specification/2025-11-25). It negotiates that revision or
  an earlier one rmcp knows; a later revision is added with a test for it.
- **Transport: stdio.** The client starts `red-ale mcp` and speaks newline-delimited JSON-RPC on its standard input
  and output. Standard output is the protocol channel: nothing else is ever written to it. Diagnostics go to
  standard error.
- **Capabilities: tools and resources.** No prompts, completions or logging. The tool list and the resource list
  are fixed for the life of the process, so neither `listChanged` nor `subscribe` is offered, and `tools/list` and
  `resources/list` each return everything in one page.
- **Instructions** are generated from the Manual: the Start Page's summary, then one sentence saying every Page is a
  resource at `red-ale://manual/<page-id>`, starting at `red-ale://manual/start`
  (`crates/red-ale-mcp/src/manual.rs`, `instructions`).
- **Lifecycle.** The server stops when the client closes its standard input, or on SIGINT or SIGTERM. It then closes
  any open Book **without saving**, exactly as `red-ale serve` does: the lock is released and unsaved changes stay in
  the `.log` journal, where `book.replayJournal` can recover them. Agents save with `book.save` or
  `book.close {"save": true}`.

## Options

`red-ale mcp` takes the options `red-ale serve` takes, except `--listen`:

| Option                   | Meaning                                                                    |
|--------------------------|----------------------------------------------------------------------------|
| `--book <path>`          | Open this Book at start (the agent can also call `book.open`).             |
| `--read-only`            | Open that Book read-only.                                                  |
| `--retention-days <n>`   | Keep backups and old journals this many days (default 30).                 |
| `--keep-backups-forever` | Never delete backups or old journals.                                      |
| `--config <file>`        | red-ale's Configuration file; without it, the default Configuration file.  |
| `--no-config`            | Keep the Configuration in memory: no file is read or written.              |
| `--tool-descriptions <d>`| `full` (the default) or `short`: how much each tool's description says (below). |

A client configuration, for example:

```json
{ "mcpServers": { "red-ale": { "command": "/home/me/.local/bin/red-ale", "args": ["mcp"] } } }
```

## Tools

The tools are **generated** from the dispatcher's method registry when the server starts, through the schema
document (`Dispatcher::schema`). Nothing is listed by hand, so a method added to `red-ale-rpc` is a tool at the next
build. The one addition is `api.schema`, which the dispatcher answers itself and so is not in the document's method
list.

| Tool field    | Comes from                                                                                       |
|---------------|--------------------------------------------------------------------------------------------------|
| `name`        | The method name, unchanged (`account.create`). The 2025-11-25 tool name rules allow `.`.          |
| `description` | The method's summary, the lead of its Reference page and its Page ID (below).                    |
| `inputSchema` | The method's params schema, made self-contained (below).                                          |
| `annotations` | The method's Effect (below).                                                                     |

**`inputSchema`.** In the schema document each method's params are a `$ref` into one shared `$defs`. MCP needs each
tool's schema to stand alone as an object schema, so the tool's `inputSchema` is the referenced params definition
itself, plus a `$defs` holding every definition it reaches, directly or through other definitions. `$ref`s keep their
`#/$defs/<Name>` form, which now resolves inside the tool's own schema. Inlining every `$ref` is not possible:
`FindQuery` (in `transaction.find`) refers to itself. `type: "object"` is always present, as MCP requires. JSON
Schema 2020-12 is the dialect of both, which is MCP's default.

**`description`.** With `--tool-descriptions full`, the default: the method's summary; then the lead of its
Reference page (the first paragraph of the hand-written part, what the method is for), as many whole sentences of it
as fit; then `Manual: <page-id>`. The whole stays within 60 words; a lead whose first sentence does not fit is cut at
a word and ends with `…`. A method nobody has written up yet has the summary and the Page ID. `api.schema`, which has
no Page, has its summary. With `short`, every tool has its one-line summary only, for clients short of context.

**Annotations.** The Effect is the only source:

| Effect        | `readOnlyHint` | `destructiveHint` |
|---------------|:--------------:|:-----------------:|
| `read`        | `true`         | (absent)          |
| `write`       | `false`        | `false`           |
| `destructive` | `false`        | `true`            |

`destructiveHint` is meaningful only when `readOnlyHint` is false, so read tools leave it out. `idempotentHint` and
`openWorldHint` are left out: the method registry records neither, and setting them per tool here would be logic of
the adapter's own. Their defaults (not idempotent; open world) are the cautious reading.

`outputSchema` is not given. Several methods return arrays, and MCP's structured content is an object.

## Resources

The Manual's Pages, from the same `red_ale_rpc::manual::manual()` the `help.*` methods read:

| Request                    | Answer                                                                                            |
|----------------------------|---------------------------------------------------------------------------------------------------|
| `resources/list`           | Every Page, by Page ID: `uri` `red-ale://manual/<page-id>`, `name` the Page ID, `title`, `description` the summary, `mimeType` `text/markdown`. |
| `resources/templates/list` | One template, `red-ale://manual/method/{name}`: a method's Reference page by the method's name.     |
| `resources/read`           | One text content: the Page as Markdown, as `help.get` gives it with `door` `mcp` (Examples as tool calls). |

A URI that names no Page is the protocol error `-32002`, the 2025-11-25 specification's "Resource not found". The
Page ID is the whole path after `red-ale://manual/`, with no decoding, so a resource's URI is exactly what
`resources/list` gives.

## Tool calls

A `tools/call` runs `Dispatcher::call_from(Origin::Mcp, name, arguments)` on a blocking thread, one call at a time.
A message larger than the Configuration's `mcp.maxRequestBytes` (64 MiB by default, read at start) is dropped, as
rmcp drops an over-long line. Absent `arguments`
read as `{}`, as absent JSON-RPC params do.

| Outcome                    | Tool result                                                                                       |
|----------------------------|---------------------------------------------------------------------------------------------------|
| Success                    | One text block holding the result as JSON. When the result is an object it is also the `structuredContent`. |
| JSON-RPC error             | `isError: true`. Text and `structuredContent` are `{"error": {"code", "name", "message", "data"}}`: the error object, with the code's stable name from [`api.md`](api.md) added. |
| Unknown tool name          | A protocol error, `-32602`, as the specification's "Error Handling" asks.                         |

So red-ale's error codes reach the agent unchanged, `invalid-params` included: the specification counts input
validation errors as tool execution errors, which the model is shown and can correct.

## Testing

- `crates/red-ale-mcp/tests/tools.rs` drives the server over an in-memory pipe with raw JSON-RPC: the done-when check
  (every method in the schema document, plus `api.schema`, is a tool with the same name, the same params schema and
  the hints its Effect gives, and there are no other tools), and a session that creates a Book, adds an account,
  saves, and gets an error result.
- `crates/red-ale-mcp/tests/manual.rs`: a resource for every Page and nothing else, a read gives the Page as
  `help.get` renders it for MCP, the method template is listed, the instructions come from the Start Page, every tool
  description is within its budget and ends with its Page ID, and `short` descriptions are the summaries.
- `crates/red-ale/tests/mcp.rs` starts `red-ale mcp` and checks that every line it writes to standard output is a
  JSON-RPC message, and that it releases the Book's lock when its input closes.
