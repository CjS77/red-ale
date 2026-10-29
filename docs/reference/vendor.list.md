<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/vendor.list
related: method/vendor.get, method/customer.list
---
Lists vendors, in GUID order, optionally only the active ones, those whose name contains some text (any case) or those
with an exact ID. IDs are not unique in GnuCash, so an `id` may match more than one vendor.

## Examples

```example fixture=synthetic/business-editable
{"method": "vendor.list", "params": {"id": "000001"}}
{"result": {"vendors": [{"guid": "9697710198334d558a9bfde219bb1e41", "name": "Paper Supplies", "…": "…"}]}}
```
