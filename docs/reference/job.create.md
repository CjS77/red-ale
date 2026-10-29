<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/job.create
related: method/job.update, method/job.list, method/customer.create, method/invoice.create
---
Creates a job: a project of a customer or a vendor that its invoices or bills can be filed under. It needs a `name` and
an `owner`, the GUID of a customer or vendor. A job has no currency of its own; it uses its owner's.

`rate` is the default price of new lines on the job's documents (GnuCash keeps it in the job's `job-rate` slot); zero
removes it. Leave `id` out and the job is numbered from the `gncJob` counter.

## Examples

```example fixture=synthetic/business-editable
{"method": "job.create", "params": {"name": "Audit", "owner": "5b22073fa4e34411b07c14a77147ce6b", "rate": "120"}}
{"result": {"guid": "…", "name": "Audit", "owner": {"kind": "customer", "guid": "5b22073fa4e34411b07c14a77147ce6b"}, "rate": {"num": "120", "denom": "1"}, "active": true, "…": "…"}}
```
