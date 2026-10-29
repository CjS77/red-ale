<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/price.fetchQuotes
related: method/price.fetchStatus, method/config.set, error/network-disabled
---
Starts fetching online prices for commodities that have a quote source, and returns a fetch ID at once; poll
[price.fetchStatus](method/price.fetchStatus) for the outcome. With no `commodities` it fetches every commodity
GnuCash would (its quote flag on and a supported source); `namespace` limits it to a namespace.

The network is off by default: until the Configuration setting `quotes.network` is true the call fails with
[network-disabled](error/network-disabled) and sends nothing. A person turns it on with
[config.set](method/config.set); over MCP the setting cannot be changed.

## Examples

```example fixture=reports/test-data
{"method": "price.fetchQuotes", "params": {}}
{"error": {"code": 5000, "message": "…", "data": {"help": "error/network-disabled"}}}
```
