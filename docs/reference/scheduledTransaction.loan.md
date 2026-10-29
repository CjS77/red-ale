<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/scheduledTransaction.loan
related: method/scheduledTransaction.create, method/scheduledTransaction.occurrences
---
Makes the scheduled repayment of a loan, as GnuCash's mortgage and loan assistant does. Give the loan account (a
`LIABILITY`), the principal, the annual rate in percent, the length, and where the money comes from and where interest
goes. The result is a scheduled transaction whose formulas use `pmt`, `ipmt` and `ppmt`, and the fixed `payment` per
period. Like any scheduled transaction it enters nothing until Since Last Run.

Only the financial functions that can be computed exactly are supported. red-ale refuses `cpd_*` and `amort_*`, whose
results GnuCash rounds as floating point, so a rate or period count that would need them is refused.

## Examples

```example
{"method": "account.create", "params": {"name": "Checking", "type": "BANK", "commodity": "USD"}}
{"result": "…"}
{"method": "account.create", "params": {"name": "Mortgage", "type": "LIABILITY", "commodity": "USD"}}
{"result": "…"}
{"method": "account.create", "params": {"name": "Interest", "type": "EXPENSE", "commodity": "USD"}}
{"result": "…"}
{"method": "scheduledTransaction.loan", "params": {"loanAccount": "Mortgage", "principal": "12000", "rate": "6", "length": {"months": 12}, "startDate": "2024-07-01", "repayment": {"from": "Checking", "interestTo": "Interest"}}}
{"result": {"scheduledTransaction": {"name": "Loan", "schedule": [{"mult": 1, "periodType": "month", "…": "…"}], "templateTransactions": [{"description": "Loan - Payment", "splits": [{"accountName": "Checking", "creditFormula": "pmt( 0.06000 / 12.00 : 12.00 : 12000.00 : 0 : 0 )", "…": "…"}, {"accountName": "Mortgage", "debitFormula": "ppmt( 0.06000 / 12.00 : i : 12.00 : 12000.00 : 0 : 0 )", "…": "…"}, {"accountName": "Interest", "…": "…"}], "…": "…"}], "…": "…"}, "payment": {"num": "1613745557", "denom": "1562500"}}}
```
