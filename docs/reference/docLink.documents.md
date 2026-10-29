<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/docLink.documents
related: method/docLink.transactions, method/docLink.pathHead
---
Lists the business documents (invoices, bills and expense vouchers) that have a document link, with the URI each one
opens. It is the counterpart of [docLink.transactions](method/docLink.transactions) for the business side.

## Examples

```example fixture=reports/test-data
{"method": "docLink.documents", "params": {}}
{"result": {"pathHead": null, "links": []}}
```
