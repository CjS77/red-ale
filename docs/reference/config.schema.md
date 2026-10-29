<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/config.schema
related: method/config.get, method/config.set
---
Returns the Configuration's JSON Schema: every setting with its type, default and description. Use it to find the path
and the allowed values before calling [config.set](method/config.set). It works with no book open.

## Examples

```example book=none
{"method": "config.schema", "params": {}}
{"result": {"title": "red-ale Configuration", "type": "object", "properties": {"accountingPeriod": {"type": "object", "…": "…"}, "…": "…"}, "…": "…"}}
```
