<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: guide/reconcile-an-account
title: Reconcile an account
summary: Match an account to a bank or card statement: start with the statement date and ending balance, tick the splits that appear on it, and finish when the difference is zero, or postpone.
related: method/reconcile.start, method/reconcile.status, method/reconcile.setState, method/reconcile.autoClear, method/reconcile.finish, method/reconcile.postpone, method/reconcile.info, method/reconcile.setInfo, error/invalid
---
Reconciling checks your books against the statement the bank sends. You tick every split that appears on the statement;
when the ticked splits bring the account to the statement's ending balance, the difference is zero and you finish.
The ticked splits become *reconciled* (`y`), and the next reconciliation starts from there. Each split is in one of
three states: not cleared (`n`), cleared (`c`, ticked but not final) or reconciled (`y`).

red-ale keeps no reconcile window. Each call is complete in itself: you carry the statement date, the ending balance and
the list of ticked splits from call to call.

## Steps

1. **Start** with [reconcile.start](method/reconcile.start): the `account`, the `statementDate` and the
   `endingBalance` from the statement. It changes nothing. It returns the splits to consider, with the cleared ones
   already ticked, the `startingBalance` (what was reconciled last time), the `tickedBalance` and the `difference` still
   to explain. Amounts use the account's raw sign, so for a credit card the balance is negative. Leave out the date and
   balance to get GnuCash's defaults, or a postponed reconciliation's.
2. **Tick** splits. Try a selection with [reconcile.status](method/reconcile.status), passing `mark`: the splits you
   would tick, and it returns the new balances without keeping anything. To keep a split cleared between calls, set it
   with [reconcile.setState](method/reconcile.setState), state `c`.
3. **Let red-ale find a selection** with [reconcile.autoClear](method/reconcile.autoClear): give the `clearedBalance`
   the ticked splits must reach, and it finds the uncleared splits that make up the difference, as GnuCash's Auto-clear
   does. It refuses when no subset reaches that balance (reason `unreachable`).
4. **Finish** with [reconcile.finish](method/reconcile.finish), passing the same statement and the list of `ticked`
   splits. It is refused unless the difference is zero. The ticked splits become reconciled; the date and balance are
   remembered for next time ([reconcile.info](method/reconcile.info)).
5. **Cannot finish today?** [reconcile.postpone](method/reconcile.postpone) saves your ticks as cleared and remembers the
   statement; the next [reconcile.start](method/reconcile.start) picks it up (`resumedPostponed: true`).
6. **Find the cause of a difference** by checking amounts and dates against the statement, and look for a transaction
   entered twice or not at all. If a transaction is missing, post it
   ([Record deposits and withdrawals](guide/record-deposits-and-withdrawals)) and start again.

## Example

```example fixture=reports/reconcile
{"method": "reconcile.start", "params": {"account": "Root:Asset:Bank", "statementDate": "1970-03-01", "endingBalance": "21.00"}}
{"result": {"statementDate": "1970-03-01", "endingBalance": {"num": "2100", "denom": "100"}, "resumedPostponed": false, "difference": {"num": "0", "denom": "100"}, "ticked": ["d0000000000000000000000000000005"], "…": "…"}}
{"method": "reconcile.status", "params": {"account": "Root:Asset:Bank", "statementDate": "1970-03-01", "endingBalance": "21.00", "mark": ["d0000000000000000000000000000006"]}}
{"result": {"tickedBalance": {"num": "1400", "denom": "100"}, "difference": {"num": "1500", "denom": "100"}, "…": "…"}}
{"method": "reconcile.finish", "params": {"account": "Root:Asset:Bank", "statementDate": "1970-03-01", "endingBalance": "21.00", "ticked": ["d0000000000000000000000000000005"]}}
{"result": {"reconciled": [{"split": "d0000000000000000000000000000005", "from": "c", "to": "y", "…": "…"}], "difference": {"num": "0", "denom": "100"}, "…": "…"}}
{"method": "reconcile.info", "params": {"account": "Root:Asset:Bank"}}
{"result": {"lastDate": "…", "postponeDate": null, "…": "…"}}
```

A statement that does not match is refused, and nothing is reconciled:

```example fixture=reports/reconcile
{"method": "reconcile.finish", "params": {"account": "Root:Asset:Bank", "statementDate": "1970-03-01", "endingBalance": "9.00", "ticked": []}}
{"error": {"code": 2001, "message": "…", "data": {"help": "error/invalid"}}}
```
