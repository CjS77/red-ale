<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/job.list
related: method/job.get, method/customer.list
---
Lists jobs, optionally only those of one customer or vendor (`owner`), the active ones, those whose name contains
some text or one ID.

## Examples

```example fixture=synthetic/business-editable
{"method": "job.list", "params": {"owner": "5b22073fa4e34411b07c14a77147ce6b"}}
{"result": {"jobs": [{"guid": "3048f991c36143769571bcc833015d53", "name": "Website rebuild", "…": "…"}]}}
```
