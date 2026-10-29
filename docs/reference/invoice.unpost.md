<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/invoice.unpost
related: method/invoice.post, method/invoice.delete
---
Unposts a document: deletes its transaction and the lot links to other documents, so it can be edited, and puts it
back as unposted. Payments made against it are not deleted: they stay in its lot as pre-payments of the owner, ready to
be applied again. A lot left empty is deleted. The document keeps its frozen billing term; with `resetTaxTables` its
lines go back to the tax tables the frozen copies came from.

## Examples

```example fixture=synthetic/business-editable
{"method": "invoice.unpost", "params": {"invoice": "aa210b7131de44c48b8513cd27c00e95"}}
{"result": {}}
{"method": "invoice.get", "params": {"invoice": "aa210b7131de44c48b8513cd27c00e95"}}
{"result": {"posted": false, "datePosted": null, "postedTransaction": null, "postedLot": null, "…": "…"}}
```
