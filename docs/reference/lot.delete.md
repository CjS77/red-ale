<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/lot.delete
related: method/lot.removeSplit, error/conflict
---
Deletes a lot; its splits stay in the account but leave the lot. A lot that still holds splits is refused with
`conflict` unless you pass `releaseSplits: true`, which makes the release explicit (GnuCash's viewer deletes without
asking). A lot that belongs to a business document cannot be deleted.

## Examples

```example fixture=reports/lots-baseline
{"method": "lot.delete", "params": {"lot": "10000000000000000000000000000000"}}
{"error": {"code": 2003, "message": "…", "data": {"help": "error/conflict"}}}
{"method": "lot.delete", "params": {"lot": "10000000000000000000000000000000", "releaseSplits": true}}
{"result": {"releasedSplits": ["d0000000000000000000000000000000"]}}
```
