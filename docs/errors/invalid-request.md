<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: error/invalid-request
summary: The request was JSON but not a JSON-RPC 2.0 request.
---
**Cause.** The document parsed, but it is not a JSON-RPC 2.0 request: `jsonrpc` is not `"2.0"`, `method` is missing
or not a string, or `params` is neither an object nor absent. This is a protocol error (code -32600).

**Fix.** Send `{"jsonrpc": "2.0", "id": 1, "method": "book.info", "params": {}}`. Params are always an object, by
name; an array of positional params is refused with [invalid-params](error/invalid-params).
