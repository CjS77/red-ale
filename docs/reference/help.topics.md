<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/help.topics
related: method/help.get, method/help.search
---
Lists the Pages of red-ale's Manual: Page ID, kind (`method`, `error`, `report`, `concept`, `guide`, `gnucash`, …),
title and summary. Give `kind` to list one kind. Start here to see what the Manual covers, then read a Page with
[help.get](method/help.get). It works with no book open.

## Examples

```example book=none
{"method": "help.topics", "params": {"kind": "error"}}
{"result": {"topics": [{"id": "error/arithmetic", "kind": "error", "title": "arithmetic", "summary": "…"}, "…"]}}
```
