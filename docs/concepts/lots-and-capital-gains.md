<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: concept/lots-and-capital-gains
title: Lots and capital gains
summary: A Lot groups the Splits that buy and then sell one holding. Realised and unrealised gains, cost basis, the FIFO lot policy, the gains Transaction red-ale records, and what stock splits do to lots.
related: concept/investments, concept/prices-and-exchange-rates, method/lot.create, method/lot.assignSplit, method/lot.computeGains, method/lot.scrubAccount, method/lot.list, method/stock.split, guide/capital-gains-with-lots
from: guide/ch_capgain.docbook#capgain_concepts1
---
Many things you own change in value: stocks, bonds, a house, a painting. Track the change only for items you could sell
for cash or that matter for tax. Food and printer paper are expenses; you spent the money and it is gone.

## Gains

- When an asset's value rises above what you paid, the difference is an **unrealised gain**. It exists on paper only:
  red-ale never records it as a Split. A report such as the [Advanced Portfolio](report/advanced-portfolio) works it out
  as the value now, from the price database, less the cost basis.
- When you sell, the profit or loss is a **realised gain**, also called a **capital gain** or loss, and it is recorded.
- **Cost basis** is what a holding cost you, the figure gains are measured against. red-ale reads it from Lots: the
  purchases a sale draws on.

Whether and how a gain is taxed varies by country and by the kind of asset (a home you live in, a bond whose interest
must be recognised each year). Consult the tax rules that apply to you. Only revalue assets that sell readily at
a well-known price, such as listed shares; for the rest it is better to wait for the sale, when the gain is exact.

## Lots

A **Lot** groups the Splits of one Account that buy and then sell the same holding. A Lot is **closed** when it has
Splits and their amounts sum to zero: everything bought has been sold. You can see an Account's Lots with
[`lot.list`](method/lot.list) (`state`: `all`, `open`, `closed`) and one Lot with its Splits with
[`lot.get`](method/lot.get).

A Split joins only a Lot of its own Account. [`lot.assignSplit`](method/lot.assignSplit) puts a Split into a Lot, cutting
the Split in two if it would overfill the Lot; without a `lot` it chooses by the **lot policy**. GnuCash 5.17 has one
policy, **first in, first out (FIFO)**: a sale draws on the earliest purchases first. [`lot.removeSplit`](method/lot.removeSplit)
takes one out again. [`lot.create`](method/lot.create) makes an empty Lot, titled `Lot 1`, `Lot 2` and so on, and
[`lot.update`](method/lot.update) sets its title and notes.

[`lot.scrubAccount`](method/lot.scrubAccount) does it for a whole Account: it assigns every Split to a Lot by FIFO and
repairs each Lot, with `subaccounts: true` for the subtree. [`lot.scrub`](method/lot.scrub) repairs one Lot.
[`lot.freeSplits`](method/lot.freeSplits) lists the Splits in no Lot.

## Recording the gain

[`lot.computeGains`](method/lot.computeGains) (also run by the scrubs) records the realised gain of each selling Split
as a **gains Transaction**, described `Realized Gain/Loss`. It has two Splits:

- a *gains Split* in the Lot, with amount zero and the gain as its value;
- a Split in the **gains Account** with the opposite value.

The gains Account is the one the Lot's Account remembers for its currency; if none is set, an income Account
`Orphaned Gains-<CUR>` under the root, found by name or created. Pass `gainsAccount` to choose one. The result lists
what was created and changed.

## Stock splits

A stock split changes the number of shares without changing value: 100 shares become 200, each worth half.
[`stock.split`](method/stock.split) records it as a Split with value zero and the action `Split`. Such Splits never join
Lots and never produce gains, and Lot balances, closure and FIFO use each Split's *adjusted amount*, restated in the
units after any later split. The adjusted amount is worked out as needed and never stored. Do not post a stock split
by hand as an ordinary buy and sell: it would create a false gain.

## A worked case

Buy 100 shares at 10.00, then 100 at 12.00, then sell 150 at 15.00. FIFO takes the first 100 and 50 of the second:
the gain is `100 x (15-10) + 50 x (15-12) = 650`. The sale is cut into a Split of 100 shares, which closes the first
Lot, and one of 50, which sells half of the second; that Lot stays open with 50 shares.
