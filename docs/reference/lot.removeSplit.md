<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/lot.removeSplit
related: method/lot.assignSplit, method/lot.delete
---
Takes a split out of its lot. A lot left empty is deleted (`lotDeleted: true`); GnuCash drops it from the account in
the same way. The split itself and its transaction are untouched.

## Examples

```example fixture=reports/lots-baseline
{"method": "lot.removeSplit", "params": {"split": "d0000000000000000000000000000000"}}
{"result": {"lotDeleted": true}}
```
