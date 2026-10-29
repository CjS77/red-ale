<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/lot.update
related: method/lot.get
---
Sets a lot's `title` or `notes`; a field you leave out is kept and an empty string removes it. GnuCash's lot viewer
writes empty strings whenever you move the selection; this writes only what you give.

## Examples

```example fixture=reports/lots-baseline
{"method": "lot.update", "params": {"lot": "10000000000000000000000000000000", "title": "Core holding", "notes": "bought at open"}}
{"result": {"guid": "10000000000000000000000000000000", "title": "Core holding", "notes": "bought at open", "…": "…"}}
```
