<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: guide/track-a-loan
title: Track a loan
summary: Record a loan you owe (a car loan or mortgage) with a liability account, the asset it bought and an interest expense; split each payment into principal and interest.
related: method/account.create, method/transaction.post, method/account.balance, method/scheduledTransaction.loan, method/reconcile.start, concept/loans-and-interest
from: guide/ch_loans.docbook#chapter_loans
---
A loan is money you owe, so the books carry it as a liability. When you borrow to buy something, the loan and the thing
you bought both appear: the liability grows and an asset grows by the same amount. Each repayment then has two parts.
The *principal* reduces what you owe. The *interest* is the lender's charge and is an expense; it does not reduce the
debt.

The same layout serves a mortgage, a car loan, and a loan you make to a friend, with the roles reversed for the last:
what the friend owes you is an asset, and the interest you receive is income.

## Steps

1. **Create the accounts** ([Set up accounts](guide/set-up-accounts)): the asset you bought (`ASSET`), the loan
   (`LIABILITY`), the interest expense (`EXPENSE`), and the bank account you repay from.
2. **Record the purchase.** The asset `+`, the loan `-`, both for the amount borrowed. If you also put down a deposit,
   a third split takes it from the bank account.
3. **Record each repayment** with [transaction.post](method/transaction.post) as one transaction with three splits: the
   loan `+` (principal), the interest expense `+`, and the bank account `-` (the full payment). The lender's statement
   gives the split between principal and interest.
4. **Check** the amount you still owe with [account.balance](method/account.balance) on the loan account. It is negative,
   because it is a credit balance.
5. **Reconcile** the loan account against each statement ([Reconcile an account](guide/reconcile-an-account)).
6. **Do not type every payment by hand?** [scheduledTransaction.loan](method/scheduledTransaction.loan) builds a scheduled
   transaction whose formulas compute the principal and interest of each payment; see
   [Schedule a transaction](guide/schedule-a-transaction).
7. **When you sell the asset**, record the sale as a deposit from the buyer and a withdrawal from the asset account, with
   any gain or loss in an income or expense account. Repay the loan with a payment of the remaining balance.

## Example

A 20,000.00 car loan, and the first repayment of 450.00 (380.00 principal, 70.00 interest).

```example now=2024-07-31T12:00:00Z
{"method": "account.create", "params": {"name": "Checking", "type": "BANK", "commodity": "USD"}}
{"result": "…"}
{"method": "account.create", "params": {"name": "Car", "type": "ASSET", "commodity": "USD"}}
{"result": "…"}
{"method": "account.create", "params": {"name": "Car loan", "type": "LIABILITY", "commodity": "USD"}}
{"result": "…"}
{"method": "account.create", "params": {"name": "Loan interest", "type": "EXPENSE", "commodity": "USD"}}
{"result": "…"}
{"method": "transaction.post", "params": {"datePosted": "2024-07-01", "description": "Buy car", "splits": [{"account": "Car", "value": "20000.00"}, {"account": "Car loan", "value": "-20000.00"}]}}
{"result": {"balanced": true, "…": "…"}}
{"method": "transaction.post", "params": {"datePosted": "2024-07-31", "description": "Loan payment 1", "splits": [{"account": "Car loan", "value": "380.00"}, {"account": "Loan interest", "value": "70.00"}, {"account": "Checking", "value": "-450.00"}]}}
{"result": {"balanced": true, "…": "…"}}
{"method": "account.balance", "params": {"account": "Car loan"}}
{"result": {"balances": {"commodity": "CURRENCY::USD", "balance": {"num": "-1962000", "denom": "100"}, "…": "…"}, "…": "…"}}
```
