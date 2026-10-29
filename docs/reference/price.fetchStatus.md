<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/price.fetchStatus
related: method/price.fetchQuotes
---
Reports a quote fetch: whether it is running or finished and, for each commodity, the price stored or why none was.
`fetch` is the ID [price.fetchQuotes](method/price.fetchQuotes) returned; leave it out for the latest fetch. With no
fetch started it fails with `not-found`.

## Examples

```example fixture=reports/test-data
{"method": "price.fetchStatus", "params": {}}
{"error": {"code": 2000, "message": "…", "data": {"kind": "quoteFetch", "id": "latest", "help": "error/not-found"}}}
```
