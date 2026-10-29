<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/billTerm.create
related: method/billTerm.update, method/billTerm.list, method/invoice.dueDate
---
Creates a billing term, the rule that says when a document is due. A `days` term is due `dueDays` after posting; a
`proximo` term is due on day `dueDays` of the next month, and a document posted after day `cutoff` of the month is due a
month later (a cutoff of 0 or less counts back from the end of the month).

`discountDays` and `discount` (a percentage) describe an early-payment discount. They are stored and shown only:
red-ale does not take the discount off a payment. The discount days may not exceed the due days.

red-ale also refuses values GnuCash's dialog accepts but would compute an undefined due date from: negative days, a
proximo due day outside 1-31, a cutoff outside -31 to 31, a discount outside 0-100.

## Examples

```example fixture=synthetic/business-editable
{"method": "billTerm.create", "params": {"name": "Net14", "type": "days", "dueDays": 14}}
{"result": {"guid": "…", "name": "Net14", "type": "days", "dueDays": 14, "discountDays": 0, "refcount": 0, "…": "…"}}
{"method": "billTerm.create", "params": {"name": "Bad", "type": "days", "dueDays": 10, "discountDays": 20}}
{"error": {"code": 2001, "message": "discount days cannot be more than due days", "data": {"help": "error/invalid"}}}
```
