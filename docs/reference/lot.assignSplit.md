<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/lot.assignSplit
related: method/lot.removeSplit, method/lot.freeSplits, method/lot.scrubAccount
---
Puts a split in a lot. Given `lot`, the split joins it, and is cut in two if it would overfill the lot; the cut-off
rest stays in no lot (`rest`). Without `lot`, GnuCash's FIFO policy chooses the earliest open lot the split reduces,
or opens a new one (`newLots`). A split joins only a lot of its own account, and a closed lot refuses with `conflict`.

## Examples

```example fixture=reports/investment-lots
{"method": "lot.assignSplit", "params": {"split": "d0000000000000000000000000000001"}}
{"result": {"pieces": [{"split": "d0000000000000000000000000000001", "lot": "…"}], "rest": null, "newLots": ["…"]}}
{"method": "lot.assignSplit", "params": {"split": "d0000000000000000000000000000007"}}
{"result": {"pieces": [{"split": "d0000000000000000000000000000007", "lot": "…"}], "rest": null, "newLots": []}}
```
