<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: gnucash/stock-assistants
title: Stock assistants
summary: Actions > Stock Split is stock.split. The Stock Transaction assistant (Buy, Sell, Dividend, Return of Capital, Notional Distribution, Reverse Split) is not provided; each of its transactions is a transaction.post, and the recipes below say how.
related: concept/investments, concept/lots-and-capital-gains, concept/prices-and-exchange-rates, method/stock.split, method/transaction.post, method/lot.computeGains, method/price.add
from: manual/ch_Tools_Assistants.docbook#stock-assistant
---
GnuCash has two assistants for investments, both in **Actions** when the register is a Stock or Mutual Fund Account.

## Stock Split

| GnuCash | red-ale |
|---|---|
| Actions > Stock Split... | [`stock.split`](method/stock.split) |
| Date, Shares, Description | `date`, `shares` (the number of shares **added**, not the ratio; negative for a reverse split: a 2-for-1 split of 90 shares is `"90"`), `description` |
| New Price, Currency | `price` records the new price per share (source `user:stock-split`), in `priceCurrency`; `currency` is the Transaction's |
| Cash in lieu | `cash`, the amount, with `assetAccount` (the Bank, Cash or Asset Account that receives it) and `incomeAccount` (the Account it comes from) |

The result's `priceStatus` is `none`, `added`, `duplicate` or `rejected`. A same-day price from a higher-ranked source
blocks the new one (`blockingPrice`), and the split is still recorded. The account must be `STOCK`, `MUTUAL` or
`CURRENCY`. A stock split has value zero, so the holding changes and the cost does not; it never joins a Lot and
produces no gain ([Lots and capital gains](concept/lots-and-capital-gains)).

## Stock Transaction assistant

The Stock Transaction assistant walks through a Buy, Sell, Dividend, Return of Capital, Notional Distribution, Stock Split
or Reverse Split. **red-ale does not provide it.** Its pages ask for a date, a type, the shares traded, the value net of
fees, the cash Account, fees (capitalised or expensed) and gain; each becomes a Split of a
[`transaction.post`](method/transaction.post). It calculates nothing for you, so you lose no calculation.

| Transaction | Splits to post |
|---|---|
| Buy | Stock Account: `amount` shares, `value` the price paid (add the fees if you capitalise them). Cash Account: minus the total. Fees expensed: a Split in an expense Account |
| Sell | Stock Account: `amount` minus the shares, `value` minus the cost. Cash Account: the proceeds net of fees. The gain or loss in an income Account, or leave it to [`lot.computeGains`](method/lot.computeGains) |
| Dividend (cash) | Cash Account: the amount. Income Account (`Income:Dividends`): minus the amount. Fees if any |
| Dividend (reinvested) | The assistant cannot do this in one transaction. Post the dividend to the Income Account against the stock Account, with the new shares as `amount` |
| Return of Capital | A Split in the stock Account of amount `0` and negative `value` for the capital returned, against a debit to the cash Account. `stock.split` does not do this |
| Notional Distribution | A Split with amount `0` in the stock Account against the Income Account |
| Reverse Split | `stock.split` with negative `shares` |

For a sale, see [Investments](concept/investments) for gross and net pricing of commissions. If you use Lots to compute
gains, leave the gain Split out and run [`lot.scrubAccount`](method/lot.scrubAccount) or
[`lot.computeGains`](method/lot.computeGains). If a non-zero price slips into a record that should not have one, check
the price database for a Price it should not have made ([`price.list`](method/price.list), [`price.delete`](method/price.delete)).

Multi-currency stock trading is not supported by GnuCash's assistant either; post those Transactions with the amounts
and values in [`transaction.post`](method/transaction.post) ([Value and amount](concept/value-and-amount)).
