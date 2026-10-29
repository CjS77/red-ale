<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: guide/close-the-books
title: Close the books
summary: At the end of a period, move the income and expense balances into equity with book.closeBooks, so those accounts start the next period at zero.
related: method/book.closeBooks, method/account.balance, method/book.save, method/book.close
from: manual/ch_Tools_Assistants.docbook#tool-close-book
---
At the end of an accounting year, accountants *close the books*: the year's income and expenses are moved into equity
(retained earnings) and the income and expense accounts return to zero. The reports for the new year then start
clean, and the balance sheet shows the profit you kept. Closing does not stop you editing; it adds transactions.

Not to be confused with [book.close](method/book.close), which closes the *file*.

## Steps

1. **Finish the period.** Enter every transaction up to the closing date, reconcile your accounts
   ([Reconcile an account](guide/reconcile-an-account)) and run [Check and repair](guide/check-and-repair).
2. **Save a copy** of the book first ([Back up and recover](guide/back-up-and-recover)). Closing adds transactions you
   cannot easily tell from your own.
3. **Choose the equity accounts** that take the income and the expenses; they may be one account. Create them if needed
   ([Set up accounts](guide/set-up-accounts)).
4. **Close** with [book.closeBooks](method/book.closeBooks): the closing `date`, `incomeTotalAccount`,
   `expenseTotalAccount` and a `description`. It posts, for each currency, one transaction for income and one for expenses,
   marked as closing transactions so reports can leave them out. A sub-account named after the currency is made under the
   equity account when more than one currency is closed.
5. **Check** with [account.balance](method/account.balance) and `asOf` the closing date: the income and expense accounts
   are zero at the end of that day.
6. **Save.**

Run it once per period: it does not remember the date. An income or expense balance held in a security rather than a
currency is refused.

## Example

```example fixture=reports/test-data
{"method": "book.closeBooks", "params": {"date": "2024-03-31", "incomeTotalAccount": "Root:Equity", "expenseTotalAccount": "Root:Equity", "description": "Close"}}
{"result": {"transactions": [{"guid": "…", "kind": "income", "currency": "CURRENCY::USD", "equityAccount": "…", "total": {"num": "-257300", "denom": "100"}}, "…"], "createdAccounts": ["…", "…"]}}
{"method": "account.balance", "params": {"account": "Root:Income", "asOf": "2024-03-31"}}
{"result": {"balances": {"balance": {"num": "0", "denom": "100"}, "…": "…"}, "…": "…"}}
```
