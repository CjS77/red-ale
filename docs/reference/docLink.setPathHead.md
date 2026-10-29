<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/docLink.setPathHead
related: method/docLink.pathHead, method/docLink.transactions
---
Sets the folder relative document links start from, or removes it with null. Give a directory path or `file://` URI;
the stored form is a `file://` URI ending in `/`. It changes red-ale's Configuration only, never the book.

## Examples

```example book=none
{"method": "docLink.setPathHead", "params": {"pathHead": "/home/me/receipts"}}
{"result": {"pathHead": "file:///home/me/receipts/", "configFile": null}}
```
