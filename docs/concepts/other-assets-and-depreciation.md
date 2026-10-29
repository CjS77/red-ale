<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: concept/other-assets-and-depreciation
title: Other assets and depreciation
summary: Current and fixed assets, prepaid items and suspense accounts, and depreciation - why, book and tax depreciation, and the linear, geometric and sum-of-digits schemes - recorded with ordinary Accounts and a scheduled transaction.
related: concept/account-types, concept/scheduled-transactions, concept/lots-and-capital-gains, method/account.create, method/scheduledTransaction.create, method/transaction.post
from: guide/ch_dep.docbook#dep_concepts1
---
Assets in the balance sheet are grouped by how long they last. Neither group needs a special Account type: use `ASSET`
(or `BANK`, `CASH`) Accounts and build the tree as suits you.

## Current assets

**Current assets** are expected to last a year or less. Examples:

- **Short-term receivables**: money lent to someone you trust and expect back within a year. Lending 500 debits
  `Assets:Current Assets:LoanToJoe` and credits the bank; each repayment does the opposite. The word *loan* in the
  name is a tip-off that you really have a receivable.
- **Reimbursable expenses and travel advances**: money you spend for an employer and later recover. Keep one Account
  per trip if you want the details apart.
- **Prepaid items**, such as insurance paid for a year, which you spread over the year as it is used up.
- **Deposits**: if the deposit will be returned, record it as an asset (debit `Deposits Receivable`); if you write it
  off, debit an expense. Decide when you pay.
- **Suspense or wash Accounts**: a place for amounts you cannot yet place. Empty it as soon as you know what they were.

## Fixed assets

**Long-term or fixed assets** last longer than a year: land, buildings, vehicles, equipment and intangibles such as
patents. Land does not wear out, so it is recorded at cost and its rise in value is not recorded until you sell; the
cost of land bought with a building is split between them. Buildings, vehicles and equipment wear, which is what
depreciation accounts for.

To show a sale, compare the price with the cost less depreciation to find the gain or loss
([Lots and capital gains](concept/lots-and-capital-gains)).

## Depreciation

**Depreciation** is the accounting method for expensing a capital purchase over time. Two reasons to record it: to keep
an estimate of your net worth in personal books, or to match the cost of equipment against the income it earns in a
business.

| Term | Meaning |
|------|---------|
| Original cost | What the asset cost, including shipping, installation and training needed to use it |
| Book depreciation | The amount you record in your financial statements each period |
| Tax depreciation | The amount you take for income tax, by rules the tax authority sets |
| Accumulated depreciation | The total book depreciation taken so far |
| Net book value | Original cost less accumulated depreciation |
| Fair market value | What the asset could be sold for now |
| Salvage value | What you estimate it will sell for at the end of its useful life, usually taken as zero |

**Personal books.** Record the purchase as an asset, and each year a depreciation expense; the net book value then
tracks the market value. There are no rules: use your best judgement, and only for assets of notable worth such as a car
or boat.

**Business books.** Capitalise what you expect to help earn income for more than the current year, with the costs of
getting it into use; do not capitalise goods bought for resale. Choose a scheme for book depreciation; tax depreciation
is fixed by your tax code and often differs.

### Schemes

For an asset costing 1,500 and a salvage value of zero:

- **Linear**: the same amount each period over the useful life; 300 a year for 5 years takes 1,500 to 0.
- **Geometric** (declining balance): a fixed percentage of the previous period's value, so more early on and a
  remaining value at the end; 30% a year gives 450, 315, 220.50, and so on.
- **Sum of digits**: a falling fraction of the cost. For 5 years the fractions are 5/15, 4/15, 3/15, 2/15 and 1/15.

Stop when the net book value reaches the salvage value.

## Recording it

Each period's depreciation is one Transaction: debit an `EXPENSE` Account (`Expenses:Depreciation`) and credit a contra
asset Account (`Assets:Fixed Assets:Computer:Accumulated Depreciation`), or credit the asset directly. A fixed amount
every period is a [scheduled transaction](concept/scheduled-transactions), made with
[`scheduledTransaction.create`](method/scheduledTransaction.create); amounts that change each period are posted one
at a time with [`transaction.post`](method/transaction.post).
