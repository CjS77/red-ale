<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/docLink.transactions
related: method/docLink.documents, method/docLink.pathHead, method/transaction.update
---
Lists the transactions that have a document link, each with the link as stored, whether it is relative and the full
`uri` it opens (the link resolved against the path head). Set a link with the `docLink` param of
[transaction.post](method/transaction.post) or [transaction.update](method/transaction.update); an empty string removes it.

## Examples

```example fixture=reports/test-data
{"method": "docLink.setPathHead", "params": {"pathHead": "/home/me/receipts"}}
{"result": "…"}
{"method": "transaction.update", "params": {"guid": "c0000000000000000000000000000005", "docLink": "may/fuel.pdf"}}
{"result": "…"}
{"method": "docLink.transactions", "params": {}}
{"result": {"pathHead": "file:///home/me/receipts/", "links": [{"transaction": "c0000000000000000000000000000005", "docLink": "may/fuel.pdf", "relative": true, "uri": "file:///home/me/receipts/may/fuel.pdf", "…": "…"}]}}
```
