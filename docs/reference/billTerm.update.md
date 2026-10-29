<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/billTerm.update
related: method/billTerm.create, method/billTerm.get
---
Changes a billing term. It merges: changing only `dueDays` keeps the type and the other fields. Any change drops the
cached frozen copy, so documents posted afterwards freeze the new rule; documents already posted keep the old one.

## Examples

```example fixture=synthetic/business-editable
{"method": "billTerm.update", "params": {"billTerm": "Net30", "dueDays": 45}}
{"result": {"name": "Net30", "type": "days", "dueDays": 45, "discountDays": 10, "…": "…"}}
```
