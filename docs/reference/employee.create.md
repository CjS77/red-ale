<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/employee.create
related: method/employee.update, method/invoice.create
---
Creates an employee: the owner of expense vouchers. It needs a `name` and a `currency`. GnuCash keeps an employee's
name as the name of its address, so `name` overrides `addr.name`.

`rate` is the hourly rate and `workday` the hours per day; `creditCardAccount`, if given, must be a `CREDIT` account,
which card-paid voucher lines are charged to when the voucher is posted. GnuCash's engine accepts any account here and
only its dialog offers credit cards; red-ale refuses the rest.

## Examples

```example fixture=synthetic/business-editable
{"method": "employee.create", "params": {"name": "John Roe", "currency": "EUR", "rate": "30.00"}}
{"result": {"guid": "…", "name": "John Roe", "rate": {"num": "3000", "denom": "100"}, "creditCardAccount": null, "active": true, "…": "…"}}
```

A card account that is not a credit card is refused:

```example fixture=synthetic/business-editable
{"method": "employee.create", "params": {"name": "John Roe", "currency": "EUR", "creditCardAccount": "1d8f8d4a358c4a169b3421d953b40369"}}
{"error": {"code": 2001, "message": "the credit card account must be of type CREDIT, not PAYABLE", "data": {"help": "error/invalid"}}}
```
