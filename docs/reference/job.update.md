<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/job.update
related: method/job.create, method/job.get
---
Changes a job's name, ID, reference, rate or active state. Fields you leave out stay as they are.

Once a job has invoices, bills or orders its `owner` cannot change, and the call is refused with `conflict`. GnuCash
allows it, and silently moves the job's documents, posted ones included, to the new owner.

## Examples

```example fixture=synthetic/business-editable
{"method": "job.update", "params": {"job": "3048f991c36143769571bcc833015d53", "owner": "9697710198334d558a9bfde219bb1e41"}}
{"error": {"code": 2003, "message": "the job has invoices, bills or orders, so its owner cannot change", "data": {"help": "error/conflict"}}}
{"method": "job.update", "params": {"job": "3048f991c36143769571bcc833015d53", "active": false}}
{"result": {"guid": "3048f991c36143769571bcc833015d53", "active": false, "…": "…"}}
```
