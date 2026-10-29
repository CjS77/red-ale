<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/help.get
related: method/help.topics, method/help.search
---
Returns one Page of the Manual by Page ID (`error/unbalanced`) or by method name (`transaction.post`), as Markdown or
plain text (`format`), with its Examples written for one door (`door`: `jsonrpc`, `cli` or `mcp`). The same Pages are
available as MCP resources and through `red-ale help`. An unknown Page is `not-found`.

## Examples

```example book=none
{"method": "help.get", "params": {"id": "error/unbalanced"}}
{"result": {"id": "error/unbalanced", "kind": "error", "title": "unbalanced", "format": "markdown", "door": "jsonrpc", "content": "…", "…": "…"}}
{"method": "help.get", "params": {"id": "transaction.post", "door": "cli", "format": "text"}}
{"result": {"id": "method/transaction.post", "kind": "method", "format": "text", "door": "cli", "…": "…"}}
```
