<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/docLink.pathHead
related: method/docLink.setPathHead, method/docLink.transactions
---
Reads the folder that relative document links start from, as a `file://` URI, or null when none is set. A document
link on a transaction or business document that is a relative path is opened from this folder. It is kept in red-ale's
Configuration, not in the book, so the call works with no book open.

## Examples

```example book=none
{"method": "docLink.pathHead", "params": {}}
{"result": {"pathHead": null, "configFile": null}}
```
