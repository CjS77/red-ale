<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/job.get
related: method/job.list, method/job.update
---
Returns one job by GUID, with its owner as a `kind` (`customer` or `vendor`) and GUID.

## Examples

```example fixture=synthetic/business-editable
{"method": "job.get", "params": {"job": "3048f991c36143769571bcc833015d53"}}
{"result": {"guid": "3048f991c36143769571bcc833015d53", "id": "JOB-1", "name": "Website rebuild", "owner": {"kind": "customer", "guid": "5b22073fa4e34411b07c14a77147ce6b"}, "reference": "PO-77", "…": "…"}}
```
