<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/counter.list
related: method/counter.set
---
Lists the book's counters, which number the business objects (customers, vendors, employees, invoices, bills,
expense vouchers, jobs and orders): the last number issued, the format (null for the default `%.6li`) and the next ID
each would give. GnuCash raises a counter each time it issues an ID.

## Examples

```example fixture=reports/test-data
{"method": "counter.list", "params": {}}
{"result": {"counters": [{"name": "gncCustomer", "last": 0, "format": null, "normalizedFormat": "%.6li", "next": "000001", "…": "…"}, "…"]}}
```
