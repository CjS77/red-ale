<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: guide/employees-and-payroll
title: Employees, vouchers and payroll
summary: Add an employee, record expenses they paid as an expense voucher and reimburse them, and record payroll with ordinary accounts and transactions.
related: method/employee.create, method/employee.list, method/invoice.create, method/invoice.addEntry, method/invoice.post, method/payment.process, method/transaction.post
---
red-ale has two things for people who work for you. The first is the **employee** record, with its **expense vouchers**:
what the employee spent for the business and you owe back. The second is payroll, which GnuCash does not calculate; you
record it yourself with accounts and ordinary transactions.

## Expense vouchers

An employee spends 40.00 on a taxi. The voucher records it as an expense and as money you owe the employee, until you pay
it back.

1. **Add the employee** with [employee.create](method/employee.create): `name`, `currency`, optionally an hourly `rate`,
   and a `creditCardAccount` (an account of type `CREDIT`) if the employee uses a company card.
2. **Create the voucher** with [invoice.create](method/invoice.create), giving the employee as `owner`. It is an
   expense voucher.
3. **Add the lines** with [invoice.addEntry](method/invoice.addEntry): the expense `account`, `quantity`, `price`, and for
   each line `payment`: `cash` if the employee paid, or `card` if the company card did. A line to a client can be marked
   `billable`.
4. **Post it** with [invoice.post](method/invoice.post) to your Payable account.
5. **Reimburse the employee** with [payment.process](method/payment.process), with the owner type `employee`.

## Payroll

Payroll is a set of accounts and a routine. A common layout:

- expense accounts for gross wages and employer taxes;
- liability accounts for tax withheld and for amounts owed to the tax office or a pension fund;
- the bank account that pays the net wage.

Each pay day, record one transaction with [transaction.post](method/transaction.post) (see
[Split transactions](guide/split-transactions)): wages expense `+` for the gross pay; the liabilities `-` for each
withholding; the bank `-` for the net pay. When you pay the tax office, post a second transaction: the liability `+`, the
bank `-`. For a routine pay day, make the first transaction a [scheduled transaction](guide/schedule-a-transaction).

## Example

Add an employee, and record and post a 40.00 taxi voucher for the existing employee Jane Doe.

```example fixture=synthetic/business-editable now=2024-03-10T12:00:00Z
{"method": "employee.create", "params": {"name": "John Roe", "currency": "EUR", "rate": "30.00"}}
{"result": {"name": "John Roe", "rate": {"num": "3000", "denom": "100"}, "active": true, "…": "…"}}
{"method": "employee.list", "params": {}}
{"result": {"employees": [{"name": "John Roe", "…": "…"}, {"name": "Jane Doe", "…": "…"}]}}
{"method": "invoice.create", "params": {"owner": "7f0d801fdccd48198abec7b197e55581"}}
{"result": {"type": "expenseVoucher", "posted": false, "…": "…"}}
{"method": "invoice.addEntry", "params": {"invoice": "00ed0000000000000000000000000002", "description": "Taxi", "quantity": "1", "price": "40.00", "account": "Expenses", "payment": "cash"}}
{"result": {"description": "Taxi", "…": "…"}}
{"method": "invoice.post", "params": {"invoice": "00ed0000000000000000000000000002", "account": "Liabilities:Payable", "postDate": "2024-03-10"}}
{"result": {"transaction": "…", "lot": "…", "due": "…"}}
```
