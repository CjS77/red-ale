<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/invoice.delete
related: method/invoice.unpost, method/invoice.removeEntry
---
Deletes an unposted document and its lines. Lines that are also on an order stay on the order. A posted document is
refused with `conflict`: [invoice.unpost](method/invoice.unpost) it first, which deletes its transaction.

GnuCash's own screens can only discard a document that has just been created; red-ale lets you delete any unposted
one, so a draft can be undone. Its number is not given back to the counter.

## Examples

```example fixture=synthetic/business-editable
{"method": "invoice.delete", "params": {"invoice": "aa210b7131de44c48b8513cd27c00e95"}}
{"error": {"code": 2003, "message": "document 000001 is posted; unpost it first", "data": {"help": "error/conflict"}}}
{"method": "invoice.delete", "params": {"invoice": "71803cac5d8e4dceabfd0d56ec5e7521"}}
{"result": {}}
```
