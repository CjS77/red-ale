<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/counter.set
related: method/counter.list
---
Sets a counter's last number issued (`last`) or its format. A format is a C-style `printf` pattern for a `long`, such as
`INV-%04li`; a null or empty format means the default. `formatValid` says whether the pattern is one GnuCash accepts,
and `next` is the ID the counter will give next, so check it before relying on the format.

## Examples

```example fixture=reports/test-data
{"method": "counter.set", "params": {"name": "gncInvoice", "last": 41}}
{"result": {"name": "gncInvoice", "last": 41, "format": null, "next": "000042", "…": "…"}}
{"method": "counter.set", "params": {"name": "gncInvoice", "format": "INV-%04li"}}
{"result": {"name": "gncInvoice", "last": 41, "format": "INV-%04li", "formatValid": true, "next": "INV-0042", "…": "…"}}
```
