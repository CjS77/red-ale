<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/scheduledTransaction.apply
related: method/scheduledTransaction.sinceLastRun, method/scheduledTransaction.discard
---
Applies a held Since Last Run proposal as one change: it creates the transactions of the instances in state `toCreate`
and moves each scheduled transaction on, or does nothing. `choices` change an instance by its `index`: a new `state`
(`toCreate`, `reminder`, `postponed`, `ignored`) and values for its variables. Instances you do not name keep the
proposed state.

Everything is checked first and nothing is created if anything fails: a variable without a value, a formula that
cannot be worked out, a template account that is a placeholder, a reminder placed before another state in one schedule,
or an instance before the read-only date that you did not choose by name. GnuCash would create the transaction with
zero amounts and try again next run. Postponing is refused in an SQLite book. A proposal made at an earlier revision of the
book is refused with `conflict`: ask again.

If rounding leaves a created transaction unbalanced, an `Imbalance-<CUR>` split is added, as GnuCash's commit does,
and reported in `imbalanceSplits`.

## Examples

A proposal that is not held, because it was applied, discarded, pushed out or made before the book was opened, is
refused:

```example
{"method": "scheduledTransaction.apply", "params": {"proposal": "slr-999999"}}
{"error": {"code": 2000, "message": "…", "data": {"kind": "sinceLastRunProposal", "id": "slr-999999", "help": "error/not-found"}}}
```

A run with a monthly rent scheduled since May, ignoring May's instance (proposal IDs count up from 1 in each server
process, so this is shown, not run as an Example):

```json
{"method": "scheduledTransaction.apply", "params": {"proposal": "slr-1", "choices": [{"index": 0, "state": "ignored"}]}}
{"result": {"created": [{"index": 1, "date": "2024-06-01", "transactions": ["…"]}, {"index": 2, "date": "2024-07-01", "transactions": ["…"]}], "ignored": [0], "postponed": [], "reminders": [], "schedules": [{"lastOccurrence": "2024-07-01", "instanceCount": 4, "…": "…"}], "imbalanceSplits": [], "warnings": []}}
```
