<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: guide/import-business-data
title: Import business data
summary: Load customers or vendors, and invoices or bills with their lines, from CSV files in GnuCash's layouts, with a preview before anything changes.
related: method/import.preview, method/import.apply, method/import.discard, method/customer.create, method/invoice.list
from: guide/ch_import_business_data.docbook#ch_import_bus_data
---
If your customers and invoices live in a spreadsheet or another program, you can bring them in with the business
imports. They work as other imports do: **preview** shows what would be created or updated, and **apply** does it. The
files use GnuCash's fixed column layouts, because red-ale reads the same files GnuCash does.

## Customers and vendors

1. Make a CSV with one owner on each line and 19 fields, in this order: `id`, `company`, `name`, `addr1` to `addr4`, `phone`,
   `fax`, `email`, `notes`, then a shipping address (`shipname`, `shipaddr1` to `shipaddr4`, `shipphone`, `shipfax`,
   `shipemail`). An empty `id` takes the next number from the counter. An empty `company` takes the `name`. There is no
   header row, and a header is imported as a row.
2. Call [import.preview](method/import.preview) with `format: "csvOwners"`, the file in `files`, `ownerKind` (`customer` or
   `vendor`) and the `separator` (`comma`, `semicolon`, ...). The proposal's `owners` say, per row, `add` or `update`
   (an owner with that `id` exists), and which fields would change.
3. Apply with [import.apply](method/import.apply). A row can be skipped with `changes`.

There is no employee import; GnuCash has none.

## Invoices and bills

1. The customer or vendor must exist already (its `id` is in the file).
2. Make a CSV with one line per entry and 22 fields: `id`, `date_opened`, `owner_id`, `billing_id`, `notes`, `date`,
   `desc`, `action`, `account`, `quantity`, `price`, `disc_type`, `disc_how`, `discount`, `taxable`, `taxincluded`,
   `tax_table`, `date_posted`, `due_date`, `account_posted`, `memo_posted`, `accu_splits`. Lines with the same `id` are the
   entries of one document. Fill `date_posted` and `account_posted` to post it as well.
3. Call [import.preview](method/import.preview) with `format: "csvDocuments"`, `documentKind` (`invoice` or `bill`, the default) and `dateFormat` (`iso` for `2024-03-01`).
   Each document in the proposal has its entries, its totals as they would be, and, if it would be posted, whether it
   `willPost` and why not if it will not. A row with an error drops its whole document, and the error is GnuCash's own
   message.
4. Apply with [import.apply](method/import.apply). `changes` can only set a document's `action` to `skip`.

The proposal `id` counts up in each server process, so, as in [Import transactions](guide/import-transactions), the apply
call is not part of the Example.

## Example

Preview one customer and then one invoice for them.

```example now=2024-03-10T12:00:00Z
{"method": "import.preview", "params": {"format": "csvOwners", "files": [{"name": "customers.csv", "text": "C1,Acme Ltd,Wile E,1 Main St,,,,555-1234,,billing@acme.example,,,,,,,,,\n"}], "separator": "comma"}}
{"result": {"id": "…", "format": "csvOwners", "owners": [{"index": 0, "kind": "customer", "id": "C1", "action": "add", "…": "…"}], "…": "…"}}
{"method": "customer.create", "params": {"name": "Acme Ltd", "currency": "USD", "id": "C1"}}
{"result": {"id": "C1", "…": "…"}}
{"method": "account.create", "params": {"name": "Income", "type": "INCOME", "commodity": "USD"}}
{"result": "…"}
{"method": "import.preview", "params": {"format": "csvDocuments", "files": [{"name": "invoices.csv", "text": "B1,2024-03-01,C1,PO-1,,2024-03-01,Design,,Income,2,50,,,,,,,,,,,\n"}], "documentKind": "invoice", "dateFormat": "iso", "separator": "comma"}}
{"result": {"id": "…", "format": "csvDocuments", "documents": [{"id": "B1", "type": "invoice", "action": "add", "…": "…"}], "…": "…"}}
```
