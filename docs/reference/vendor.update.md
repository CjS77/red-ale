<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/vendor.update
related: method/vendor.create, method/vendor.get
---
Changes the fields of a vendor you name; fields you leave out stay as they are, and `null` clears `terms` or
`taxTable`. Vendors cannot be deleted, as in GnuCash; set `active: false`.

## Examples

```example fixture=synthetic/business-editable
{"method": "vendor.update", "params": {"vendor": "9697710198334d558a9bfde219bb1e41", "terms": null, "notes": "Pays on receipt"}}
{"result": {"guid": "9697710198334d558a9bfde219bb1e41", "notes": "Pays on receipt", "terms": null, "…": "…"}}
```
