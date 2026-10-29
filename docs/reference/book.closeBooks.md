<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/book.closeBooks
related: method/book.close, method/account.balance, method/period.dates
---
GnuCash's Tools > Close Book: at a closing date it moves every income and expense balance into equity, one closing
transaction per currency and kind, so those accounts start the next period at zero. Name the two equity accounts
that take the income and the expenses (they may be the same one); a sub-account named after a currency is made under
the equity account when more than one currency is closed.

The book stays open and editable. It does not mark the date anywhere, so run it once per period. An income or
expense balance held in a security rather than a currency is refused with `invalid`, where GnuCash would book it
as money.

## Examples

```example fixture=reports/test-data
{"method": "book.closeBooks", "params": {"date": "2024-03-31", "incomeTotalAccount": "Root:Equity", "expenseTotalAccount": "Root:Equity", "description": "Close"}}
{"result": {"transactions": [{"guid": "…", "kind": "income", "currency": "CURRENCY::USD", "equityAccount": "…", "total": {"num": "-257300", "denom": "100"}}, "…"], "createdAccounts": ["…", "…"]}}
{"method": "account.balance", "params": {"account": "Root:Income", "asOf": "2024-03-31"}}
{"result": {"balances": {"balance": {"num": "0", "denom": "100"}, "…": "…"}, "…": "…"}}
```
