<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: guide/find-transactions
title: Find transactions
summary: Search for transactions by description, date, amount or account with transaction.find; list an account's register with transaction.list; find business documents with invoice.list.
related: method/transaction.find, method/transaction.list, method/transaction.get, method/invoice.list
---
There are two ways to look at transactions. [transaction.list](method/transaction.list) is a register: all transactions,
or those of one account and a date range, in GnuCash's order. [transaction.find](method/transaction.find) is GnuCash's
Find Transaction dialog: it matches splits against criteria you combine.

## Steps

1. **To read an account**, call [transaction.list](method/transaction.list) with `account` and, if you like, `from` and
   `to`. With no parameters it lists the whole book, which is GnuCash's General Journal.
2. **To search**, call [transaction.find](method/transaction.find) with a `query`. A query is a criterion or a group of
   criteria: `{"all": [...]}` means every one must match, `{"any": [...]}` means at least one, `{"not": ...}` negates. Criteria include
   `description`, `notes`, `memo`, `num`, `datePosted`, `value`, `amount`, `reconcile`, `void` and `accounts`; the
   reference page lists them with their operators. Add `limit` to keep only the last matches.
3. **Read the matches.** Each is a split (`split`) with its `transaction` and `account`. Several matches may belong to one
   transaction; pass its GUID to [transaction.get](method/transaction.get) for the whole thing.
4. **Business documents** are found with [invoice.list](method/invoice.list), filtered by `type` (`invoice`, `bill`,
   `expenseVoucher`, `creditNote`), `posted` and owner. See [Invoices and bills](guide/invoices-and-bills).

## Example

```example fixture=reports/test-data
{"method": "transaction.list", "params": {"account": "Root:Asset:Wallet", "from": "1971-01-01", "to": "1971-12-31"}}
{"result": {"transactions": [{"guid": "c0000000000000000000000000000007", "description": "$100bank -> $80expenses + $20wallet", "…": "…"}]}}
{"method": "transaction.find", "params": {"query": {"all": [{"description": {"text": "$22 expense"}}, {"datePosted": {"op": "before", "date": "1978-03-01"}}]}}}
{"result": {"matches": [{"transaction": "c000000000000000000000000000000d", "split": "…", "account": "…"}, "…"]}}
{"method": "invoice.list", "params": {"posted": false}}
{"result": {"invoices": []}}
```
