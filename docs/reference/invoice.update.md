<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/invoice.update
related: method/invoice.create, method/invoice.unpost
---
Changes an unposted document: its owner, number, dates, billing term, notes, document link and the like. A new owner must
give the same kind of document (another customer, or one of a customer's jobs, for an invoice), and the currency follows
the owner. Whether it is a credit note cannot change.

A posted document can only be activated or deactivated, or have its document link changed; any other change is refused
with `conflict`. Unpost it first with [invoice.unpost](method/invoice.unpost).

## Examples

```example fixture=synthetic/business-editable
{"method": "invoice.update", "params": {"invoice": "71803cac5d8e4dceabfd0d56ec5e7521", "notes": "Phase 1"}}
{"result": {"guid": "71803cac5d8e4dceabfd0d56ec5e7521", "notes": "Phase 1", "posted": false, "…": "…"}}
{"method": "invoice.update", "params": {"invoice": "aa210b7131de44c48b8513cd27c00e95", "notes": "Phase 1"}}
{"error": {"code": 2003, "message": "document 000001 is posted; only its active flag can change", "data": {"help": "error/conflict"}}}
{"method": "invoice.update", "params": {"invoice": "aa210b7131de44c48b8513cd27c00e95", "active": false}}
{"result": {"guid": "aa210b7131de44c48b8513cd27c00e95", "active": false, "posted": true, "…": "…"}}
```
