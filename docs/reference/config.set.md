<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/config.set
related: method/config.get, method/config.schema, error/invalid-params
---
Changes one or more Configuration settings by path, all or none, validated against [config.schema](method/config.schema).
Give `changes` as an object of path to value; a null value restores the default. It changes red-ale's Configuration
only, never a book, and works with no book open.

Settings that open the network or hold keys (`quotes.network` and the quote services' keys) cannot be changed through
the MCP server, only through JSON-RPC or the CLI by a person, so an AI agent cannot switch the network on.

## Examples

```example book=none
{"method": "config.set", "params": {"changes": {"quotes.network": true}}}
{"result": {"values": {"quotes.network": true}, "clearedKeys": [], "configFile": null}}
{"method": "config.set", "params": {"changes": {"period.bogus": 1}}}
{"error": {"code": -32602, "message": "…", "data": {"problems": [{"path": "period.bogus", "message": "…"}], "help": "error/invalid-params"}}}
```
