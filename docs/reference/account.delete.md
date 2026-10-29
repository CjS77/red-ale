<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/account.delete
related: method/account.move, method/transaction.delete, error/invalid, error/conflict
---
Deletes an account. An account with transactions needs you to say what happens to them, `transactions`: `"delete"`
(whole transactions go, other accounts' splits included) or `{"moveTo": <account>}`; the same choices exist for a
sub-account (`subaccount`, `subaccountTransactions`), of which at most one is allowed. Targets must be of the deleted
account's type, and a move to an account in another commodity needs `allowCommodityMismatch: true`.

GnuCash's engine, given no choice, deletes the splits one by one and its imbalance scrub then adds `Imbalance`
splits; red-ale makes you choose and never leaves an imbalance. It refuses (`conflict`) while a document, budget,
scheduled transaction or read-only transaction uses the account.

## Examples

Without a choice the delete is refused and nothing changes:

```example fixture=reports/test-data
{"method": "account.delete", "params": {"account": "Root:Asset:Wallet"}}
{"error": {"code": 2001, "message": "…", "data": {"help": "error/invalid"}}}
{"method": "account.delete", "params": {"account": "Root:Asset:Wallet", "transactions": {"moveTo": "Root:Asset:Bank"}}}
{"result": {"deletedAccounts": ["…"], "reparented": null, "movedSplits": ["…"], "deletedTransactions": [], "deletedLots": []}}
```
