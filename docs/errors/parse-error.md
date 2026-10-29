<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: error/parse-error
summary: The request was not valid JSON.
---
**Cause.** The server received text it could not parse as JSON, so it could not even tell which method was meant.
This is a JSON-RPC protocol error (code -32700) and comes from the HTTP door and stdin scripts, not from a method.

**Fix.** Send a well-formed JSON document: balanced braces, double-quoted names and strings, no trailing commas.
With `red-ale exec`, give params as one JSON object or as `key=value` and `key:=json` pairs, and quote the object
for your shell.
