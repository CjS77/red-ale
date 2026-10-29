<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/config.get
related: method/config.set, method/config.schema
---
Reads red-ale's Configuration: the settings GnuCash keeps outside the book (accounting period, document link folder,
report options, import settings, quote services, limits). Give a dotted `path` for one setting (`quotes.network`);
without it you get the whole Configuration, with defaults filled in and secrets shown only as set or unset. It works
with no book open. An unknown path is `invalid-params`.

## Examples

```example book=none
{"method": "config.get", "params": {"path": "quotes.network"}}
{"result": {"path": "quotes.network", "value": false, "unknown": [], "problems": [], "configFile": null}}
```
