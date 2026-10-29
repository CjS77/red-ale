<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/invoice.updateEntry
related: method/invoice.addEntry, method/invoice.removeEntry
---
Changes a line of an unposted document or an open order; fields you leave out stay as they are. A line stays on the
documents and the order it was added to; this cannot move it. A line of a posted document or a closed order is refused
with `conflict`.

## Examples

```example fixture=synthetic/business-editable
{"method": "invoice.updateEntry", "params": {"entry": "5eff8018de334b2084ef00f90330cd57", "price": "12.50", "quantity": "2"}}
{"result": {"guid": "5eff8018de334b2084ef00f90330cd57", "quantity": {"num": "2", "denom": "1"}, "invoicePrice": {"num": "1250", "denom": "100"}, "…": "…"}}
```
