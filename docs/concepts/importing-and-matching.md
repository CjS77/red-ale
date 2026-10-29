<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: concept/importing-and-matching
title: Importing and matching
summary: Importing QIF, CSV and business files in two calls - import.preview builds a proposal and changes nothing, import.apply applies your choices - and how the matcher finds duplicates, scores candidates and learns accounts from import maps.
related: concept/transactions-and-splits, concept/reconciliation, concept/configuration, method/import.preview, method/import.apply, method/import.discard, method/importMap.list, method/importMap.delete
from: guide/ch_importing.docbook#importing-matcher
---
GnuCash imports transactions from files in the formats QIF, OFX/QFX, CSV, MT940, MT942 and DTAUS. red-ale imports **QIF
and CSV**; OFX, QFX, MT940, MT942, DTAUS and online banking are not supported
([From GnuCash: Not in red-ale](gnucash/not-in-red-ale)).

| `format` | Imports | Duplicates found by |
|----------|---------|---------------------|
| `csvTransactions` | Transactions, with a saved or inline column layout | The matcher |
| `qif` | Accounts, commodities, Transactions and Prices | QIF's own duplicate finder |
| `csvAccounts` | An Account tree | Full name |
| `csvPrices` | Prices | Same day, either direction |
| `csvDocuments` | Invoices or bills, posted when the file says | Document ID |
| `csvOwners` | Customers or Vendors | Owner ID |

## Two calls

An import is never a single step. GnuCash shows its matcher in a window; red-ale's equivalent is a proposal you can read.

1. [`import.preview`](method/import.preview) reads the file and works out what it holds, and returns an **Import
   proposal**: each **Imported transaction** (or account, price, document, owner) with the **Import action** chosen for
   it, its **Candidate matches**, the account of each Split, any imbalance, and whether it would be read-only by date. It
   changes nothing. The server holds the proposal under an ID.
2. Read it. Change an action, a chosen match or an account.
3. [`import.apply`](method/import.apply) applies the proposal with your changes as **one change**: everything or nothing.
   [`import.discard`](method/import.discard) throws a proposal away.

A proposal is tied to the **book revision** it was made at: if the book changed since, `import.apply` refuses it
(`conflict`) and you preview again. The server keeps 16 proposals and drops the oldest. Paths are read on the server, so
they are relative to where it runs.

### Import actions

For an Imported transaction, `import.apply` can:

- **add** it as a new Transaction;
- **clear** the existing Transaction it matches: it sets that Split's reconcile state to `c` and deletes nothing;
- **update** that Transaction from the imported one and clear it (GnuCash's "update and clear" is one action, not two);
- **skip** it.

For accounts, prices, documents and owners the actions are add, update (replace a same-day Price) and skip.

## The matcher

For `csvTransactions` the matcher compares each imported transaction with the book's existing ones, so that importing
the same bank file twice does not double your entries. It looks for **Candidate matches**: Splits in the same Account
posted within 42 days. Each gets a **Match score**, an integer: points for an equal or nearly equal amount, a close
date, an equal number, memo and description; points off for a different amount or number, or dates far apart. The
best score decides the action by the thresholds of the **Import settings**: at or below `autoAddThreshold` the
transaction is added, at or above `autoClearThreshold` it is cleared against the match, and the others are left for
you. **⚠** The score is not a probability. The settings (`matchThreshold`, the date thresholds, `enableSkip`,
`enableUpdate`, `useBayes`, `atmFeeThreshold`) are in the Configuration under `import.settings`, with GnuCash's defaults
([The Configuration](concept/configuration)).

## Learning accounts

A file with only one side of each Transaction needs a second Account. The matcher fills it in from what earlier imports
taught it, kept in Slots of the Accounts as the **Import map**:

- **Bayesian matching**, on by default (`useBayes`): the Account whose combined probability over the transaction's
  words reaches 90%, learned per source Account.
- the **plain Import map**: an exact description to an Account, when Bayesian matching is off.

A CSV layout can also name the Account itself. `import.apply` learns from your final choices, so the second import of a
file from the same bank needs fewer corrections. [`importMap.list`](method/importMap.list) shows what has been learned and
[`importMap.delete`](method/importMap.delete) forgets an entry.

## Layouts and QIF

GnuCash keeps CSV column layouts ("presets") and the QIF account map (which Account each QIF account, category and payee
went to) in its own user files. red-ale keeps them in the Configuration (`import.csvLayouts.<name>`, `import.qifAccountMap`)
and `import.apply` saves the QIF map.

## Imbalance

GnuCash's importers do not refuse an imported transaction that does not balance: committing it adds an
`Imbalance-<CUR>` Split. red-ale flags each such transaction in the proposal as `imbalance`, before anything changes, and
`import.apply` then does as GnuCash does. See [Imbalance, orphans and scrubs](concept/imbalance-and-scrubs).

## Exporting

The other way: [`export.csvTransactions`](method/export.csvTransactions) and [`export.csvTree`](method/export.csvTree)
return CSV text (nothing is written by the server), and [`export.accounts`](method/export.accounts) writes a new book
holding the Account tree and Commodities and no Transactions.
