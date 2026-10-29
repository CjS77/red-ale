<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: method/invoice.totals
related: method/invoice.get, method/invoice.post
---
Gives a document's subtotal, tax and total, the tax per tax account and each line's value, discount and tax, worked out
as GnuCash does: exactly, with each line's value rounded to the currency's smallest unit, but each account's tax summed
unrounded over the lines and rounded once. So the tax total can differ by a cent from the sum of the lines' taxes.

Figures are signed as printed on the document. It works on posted and unposted documents alike, and a posted document's
total is what [invoice.post](method/invoice.post) put in the A/R or A/P split.

## Examples

```example fixture=synthetic/business-editable
{"method": "invoice.totals", "params": {"invoice": "aa210b7131de44c48b8513cd27c00e95"}}
{"result": {"subtotal": {"num": "125000", "denom": "100"}, "tax": {"num": "12300", "denom": "100"}, "total": {"num": "137300", "denom": "100"}, "taxes": [{"accountName": "Liabilities:Tax", "amount": {"num": "12050", "denom": "100"}, "…": "…"}, {"accountName": "Liabilities:Levy", "amount": {"num": "250", "denom": "100"}, "…": "…"}], "entries": ["…"]}}
```
