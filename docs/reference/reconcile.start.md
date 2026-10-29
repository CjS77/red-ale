<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/reconcile.start
related: method/reconcile.status, method/reconcile.finish, method/reconcile.postpone
---
Begins reconciling an account against a bank statement, as GnuCash's Reconcile dialog opens. It fills in GnuCash's
defaults (the statement date, the ending balance, a postponed reconciliation if there is one), lists the account's
uncleared and cleared splits with the cleared ones already ticked, and shows the balances: `startingBalance`,
`tickedBalance`, `reconciledBalance` and the `difference` from the `endingBalance`.

It changes nothing. red-ale keeps no reconciliation window: you carry the statement and the ticked splits from call to
call and hand them to [reconcile.status](method/reconcile.status), then to [reconcile.finish](method/reconcile.finish)
or [reconcile.postpone](method/reconcile.postpone). Amounts use the account's raw sign (debits positive), whatever
sign GnuCash's window would show for a credit account.

## Examples

```example fixture=reports/reconcile
{"method": "reconcile.start", "params": {"account": "Root:Asset:Bank", "statementDate": "1970-03-01", "endingBalance": "21.00"}}
{"result": {"statementDate": "1970-03-01", "endingBalance": {"num": "2100", "denom": "100"}, "resumedPostponed": false, "startingBalance": {"num": "-800", "denom": "100"}, "tickedBalance": {"num": "2900", "denom": "100"}, "reconciledBalance": {"num": "2100", "denom": "100"}, "difference": {"num": "0", "denom": "100"}, "ticked": ["d0000000000000000000000000000005"], "splits": [{"split": "d0000000000000000000000000000005", "state": "c", "ticked": true, "…": "…"}, {"state": "n", "ticked": false, "…": "…"}, {"state": "n", "ticked": false, "…": "…"}], "…": "…"}}
```
