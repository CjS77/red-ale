<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/account.get
related: method/account.list, method/account.update, method/account.balance
---
Describes one account: its type, commodity, smallest fraction (`scu`), code, flags, colour, balance limits, whether it
has splits and how many sub-accounts it has. Name the account by its full name (`Assets:Checking`, levels joined with
`:`, the root left out) or by GUID. An unknown name is [not-found](error/not-found); names are case-sensitive.

## Examples

```example fixture=reports/test-data
{"method": "account.get", "params": {"account": "Root:Asset:Bank"}}
{"result": {"name": "Bank", "fullName": "Root:Asset:Bank", "type": "ASSET", "commodity": "CURRENCY::USD", "scu": 100, "placeholder": false, "hasSplits": true, "children": 0, "…": "…"}}
```
