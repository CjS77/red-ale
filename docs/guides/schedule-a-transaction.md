<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: guide/schedule-a-transaction
title: Schedule a transaction
summary: Set up a transaction that repeats (rent, a salary, a loan payment), see when it falls due, and create what is due with Since Last Run.
related: method/scheduledTransaction.create, method/scheduledTransaction.fromTransaction, method/scheduledTransaction.sinceLastRun, method/scheduledTransaction.apply, method/scheduledTransaction.discard, method/scheduledTransaction.loan, method/scheduledTransaction.occurrences, method/scheduledTransaction.list
---
Some transactions come round on a rhythm: rent on the first of the month, a salary every fortnight, a loan payment. A
**scheduled transaction** keeps a template of such a transaction and a schedule, and creates the real transactions when
they fall due. Nothing happens by itself in a headless program: *you* ask what is due, look at it, and apply it, usually
when you open the book. GnuCash calls this *Since Last Run*.

## Steps

1. **Start from an existing transaction.** If you have already posted the first rent payment, call
   [scheduledTransaction.fromTransaction](method/scheduledTransaction.fromTransaction) with its GUID and a `frequency`
   (`monthly`, `weekly`, and so on). It returns the scheduled transaction it would make from it, to check or adjust.
   Or build one from scratch with [scheduledTransaction.create](method/scheduledTransaction.create): a `name`, a
   `startDate`, a `schedule` (a list of recurrences: `{mult: 1, periodType: "month"}`), and template transactions whose
   splits take a `debitFormula` or `creditFormula`.
2. **Use a formula, not just a number,** when the amount varies. A formula can use variables, which you supply when you
   apply the proposal.
3. **Preview the dates** with [scheduledTransaction.occurrences](method/scheduledTransaction.occurrences), for an
   existing schedule or for one you describe.
4. **Ask what is due** with [scheduledTransaction.sinceLastRun](method/scheduledTransaction.sinceLastRun). It lists every
   occurrence between the last one created and today with a state (`toCreate`, `reminder` and so on) and a proposal `id`.
   It changes nothing.
5. **Apply it** with [scheduledTransaction.apply](method/scheduledTransaction.apply) and that `id`. In `choices` you can
   change one instance's state (to `postponed` or `ignored`) or give a variable its value. All instances are checked
   first, so if one fails none is created. Or drop the proposal with
   [scheduledTransaction.discard](method/scheduledTransaction.discard).
6. **For loans**, [scheduledTransaction.loan](method/scheduledTransaction.loan) is GnuCash's Loan assistant: give the
   loan account, principal, annual `rate`, `length` and the accounts to pay from and charge interest to, and it makes the
   scheduled transaction with the payment formulas.
7. **List them** with [scheduledTransaction.list](method/scheduledTransaction.list).

## Example

```example now=2024-07-01T12:00:00Z
{"method": "account.create", "params": {"name": "Checking", "type": "BANK", "commodity": "USD"}}
{"result": "…"}
{"method": "account.create", "params": {"name": "Rent", "type": "EXPENSE", "commodity": "USD"}}
{"result": "…"}
{"method": "scheduledTransaction.create", "params": {"name": "Monthly rent", "startDate": "2024-05-01", "schedule": [{"mult": 1, "periodType": "month"}], "templateTransactions": [{"description": "Rent", "splits": [{"account": "Rent", "debitFormula": "750"}, {"account": "Checking", "creditFormula": "750"}]}]}}
{"result": {"name": "Monthly rent", "enabled": true, "nextOccurrence": "2024-05-01", "…": "…"}}
{"method": "scheduledTransaction.sinceLastRun", "params": {}}
{"result": {"id": "…", "today": "2024-07-01", "summary": {"toCreate": 3, "reminders": 0, "postponed": 0, "errors": 0, "…": "…"}, "…": "…"}}
```

The proposal's `id` goes into the next call. It counts up in each server process, so it is not always `slr-1`:

```json
{"method": "scheduledTransaction.apply", "params": {"proposal": "slr-1"}}
```

It creates the three rent transactions, 750.00 each, and moves the schedule on. A second `scheduledTransaction.sinceLastRun`
then finds nothing to create until the next occurrence, 1 August.
