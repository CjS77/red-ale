<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/account.renumber
related: method/account.update
---
GnuCash's "Renumber sub-accounts": gives an account's immediate sub-accounts codes in display order, `prefix-NNN` for
`interval`, twice `interval`, and so on. The prefix defaults to the account's own code, the interval to 10 and the
digits to 3. Grandchildren are not touched, and nothing checks that the codes are unique. An account with no
sub-accounts is refused with `invalid`.

## Examples

```example fixture=reports/test-data
{"method": "account.renumber", "params": {"account": "Root:Asset", "prefix": "1", "interval": 100}}
{"result": [{"account": "…", "fullName": "Root:Asset:Bank", "code": "1-100"}, {"account": "…", "fullName": "Root:Asset:GBP Bank", "code": "1-200"}, {"account": "…", "fullName": "Root:Asset:Wallet", "code": "1-300"}]}
```
