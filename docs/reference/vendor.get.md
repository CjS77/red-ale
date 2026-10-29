<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/vendor.get
related: method/vendor.list, method/vendor.update
---
Returns one vendor by GUID.

## Examples

```example fixture=synthetic/business-editable
{"method": "vendor.get", "params": {"vendor": "9697710198334d558a9bfde219bb1e41"}}
{"result": {"guid": "9697710198334d558a9bfde219bb1e41", "id": "000001", "name": "Paper Supplies", "taxIncluded": "yes", "active": true, "…": "…"}}
```
