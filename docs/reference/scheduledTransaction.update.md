<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/scheduledTransaction.update
related: method/scheduledTransaction.create, method/scheduledTransaction.get
---
Changes the fields you give; the rest stay. `end` sets one kind of end and clears the other. `templateTransactions`
replaces the whole list: an entry or split that carries the `guid` of an existing one keeps its identity, one without a
GUID is new, and any left out is deleted. Turning `autoCreate` on turns `notify` on unless you say otherwise.

The last occurrence, the instance count and the deferred instances are not changed here; only
[Since Last Run](method/scheduledTransaction.sinceLastRun) moves them, as in GnuCash. Disabling a scheduled transaction
leaves it out of Since Last Run; nothing disables one on its own, even when it has ended.

## Examples

```example fixture=synthetic/rich
{"method": "scheduledTransaction.update", "params": {"scheduledTransaction": "52f96837087e9670c933f9421b0ac626", "enabled": false}}
{"result": {"guid": "52f96837087e9670c933f9421b0ac626", "enabled": false, "warnings": [], "…": "…"}}
{"method": "scheduledTransaction.sinceLastRun", "params": {}}
{"result": {"summary": {"toCreate": 0, "…": "…"}, "schedules": [], "…": "…"}}
```
