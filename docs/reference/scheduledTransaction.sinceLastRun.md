<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/scheduledTransaction.sinceLastRun
related: method/scheduledTransaction.apply, method/scheduledTransaction.discard, method/scheduledTransaction.occurrences
---
Proposes the transactions of every enabled scheduled transaction that have fallen due up to today (and its advance
days), as GnuCash's Since Last Run window does, and changes nothing. The proposal is held by `id` (`slr-1`, …) until you
[apply](method/scheduledTransaction.apply) or [discard](method/scheduledTransaction.discard) it, 16 newer proposals push
it out, or the book closes.

Each instance has a `state` (`toCreate`, `reminder`, `postponed`), a date and the values of its variables, and, when
all variables are set, the transactions that creating it would make. Instances that need a value for a variable
other than `i` have `value: null`; give them in `apply`. `readOnlyByDate` marks instances before the book's
auto read-only date, which `apply` refuses unless you choose them by name.

red-ale never runs Since Last Run when a book opens; GnuCash does. Where GnuCash re-proposes instances that were already
created after a postponed one, red-ale proposes each postponed instance and then only the new ones.

## Examples

```example
{"method": "account.create", "params": {"name": "Checking", "type": "BANK", "commodity": "USD"}}
{"result": "…"}
{"method": "account.create", "params": {"name": "Rent", "type": "EXPENSE", "commodity": "USD"}}
{"result": "…"}
{"method": "scheduledTransaction.create", "params": {"name": "Monthly rent", "startDate": "2024-05-01", "schedule": [{"mult": 1, "periodType": "month"}], "templateTransactions": [{"description": "Rent", "splits": [{"account": "Rent", "debitFormula": "750"}, {"account": "Checking", "creditFormula": "750"}]}]}}
{"result": "…"}
{"method": "scheduledTransaction.sinceLastRun", "params": {}}
{"result": {"id": "…", "today": "2024-07-01", "summary": {"toCreate": 3, "reminders": 0, "postponed": 0, "errors": 0, "…": "…"}, "schedules": [{"name": "Monthly rent", "instances": [{"index": 0, "date": "2024-05-01", "state": "toCreate", "instanceCount": 1, "…": "…"}, {"index": 1, "date": "2024-06-01", "…": "…"}, {"index": 2, "date": "2024-07-01", "…": "…"}], "…": "…"}], "…": "…"}}
```
