<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/invoice.dueDate
related: method/billTerm.create, method/invoice.post, method/invoice.due
---
Computes when a document posted on `postDate` falls due under a billing term: the document's own (give `invoice`) or one
you name by GUID (`term`). A `days` term adds its days; a `proximo` term falls due on its day of the next month
(clamped to the month's end), or of the month after when the posting day is past its cutoff. With no term, the due date
is the posting date. [invoice.post](method/invoice.post) uses the same rule when you give no `dueDate`.

## Examples

```example fixture=synthetic/business-editable
{"method": "invoice.dueDate", "params": {"invoice": "aa210b7131de44c48b8513cd27c00e95", "postDate": "2024-03-28"}}
{"result": {"dueDate": "2024-04-27", "due": "2024-04-27T10:59:00Z"}}
{"method": "invoice.dueDate", "params": {"term": "ce15231d730a43c6b7c737ffc9af7b8c", "postDate": "2024-03-28"}}
{"result": {"dueDate": "…", "due": "…"}}
```
