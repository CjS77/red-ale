<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: concept/investments
title: Investments
summary: Stocks, mutual funds, bonds and fixed assets in the books - one STOCK or MUTUAL Account per holding, shares as amounts, prices in the price database, income Accounts for dividends and interest, and how fees, dividends and returns are recorded.
related: concept/lots-and-capital-gains, concept/prices-and-exchange-rates, concept/commodities-and-currencies, concept/value-and-amount, method/commodity.create, method/account.create, method/price.add
from: guide/ch_invest.docbook#invest_concepts1
---
An investment is something you buy hoping to earn from it or sell it for more. This page covers how the books hold the
common kinds. For gains see [Lots and capital gains](concept/lots-and-capital-gains).

## Words you will meet

| Term | Meaning |
|------|---------|
| Security | A Commodity that is not a currency: a share, a fund unit, a bond. **⚠** GnuCash's *stock* is a share of a company; `STOCK` is also the Account type |
| Capital gain or loss | The difference between purchase and selling prices. Also *realised* gain or loss |
| Commission | The fee a broker charges to buy or sell |
| Dividend | A cash payment a company makes to its shareholders |
| Interest | What a borrower pays a lender for the use of money, usually a percentage of the principal per year |
| Compounding | Reinvested interest that earns interest of its own |
| Principal | The original sum invested or borrowed |
| Yield | The income an investment earns, as a percentage of its price; it leaves out capital gains. A $100 share paying $2 a year yields 2% |
| Return | Income plus capital gains or losses. **⚠** Not a *return of capital* |
| Risk | The chance that the return differs from what was expected |
| Liquidity | How easily it turns into cash |
| Valuation | Working out what it would sell for now |
| Stock split | More shares for each existing one, at no cost, with the price adjusted so the value does not change |

## Kinds of investment

- **Interest-bearing accounts**, such as savings and certificates of deposit: ordinary `BANK` Accounts. Interest is
  income, recorded as a Transaction from an `INCOME` Account.
- **Stocks and mutual funds**: a mutual fund is treated exactly like a single stock, for tax and for the books.
- **Fixed assets**, a house or a painting: value is hard to know until you sell, so there is little to do beyond
  recording the purchase and the sale ([Other assets and depreciation](concept/other-assets-and-depreciation)).

## Setting up a holding

1. Make the security with [`commodity.create`](method/commodity.create) (namespace such as `NASDAQ`, mnemonic `AMZN`).
2. Make an Account of type `STOCK` (or `MUTUAL`) for **every security you own**, holding that Commodity, usually under an
   `Assets:Investments:Brokerage` branch, named by the ticker. The Account's balance is in shares, not money:
   100, not $2,000.
3. If you track income per security, make `Income:Dividends:AMZN`, `Income:Interest:AMZN` and capital-gain Accounts
   such as `Income:Cap Gain (Long):AMZN` and `Income:Cap Gain (Short):AMZN`, and an `Expenses:Commission` Account.
4. Put a Price for the security in the price database ([`price.add`](method/price.add)), once, and keep it current.

red-ale has no predefined investment Accounts. An Account can remember the Accounts for a security's roles (dividends,
capital gains, cash proceeds, broker fees) as its *associated accounts*.

## Buying

A purchase is a Transaction in the currency you paid, with a Split in the stock Account that has an *amount* in shares
and a *value* in money ([Value and amount](concept/value-and-amount)), and a Split in the bank or brokerage Account
paying for it. The share price is `value / amount`; give the amount and the value and let the price follow, so that no
rounding is hidden in it.

**Commissions** can be handled two ways. In *gross pricing* you record the price before fees and put the fee in an
expense Account in a third Split: this tracks fees but the gain scrub does not know to deduct them, so work gains out
yourself. In *net pricing* you record the price including the fee and no expense: it works with the lots, but the price
in the database is the effective price. Take advice if you are unsure which your tax rules want.

## Selling

A sale mirrors the purchase: negative value and amount in the stock Account, the proceeds in the bank Account, and
a gain or loss Split in an income Account. [Lots](concept/lots-and-capital-gains) can compute the gain by FIFO and
record it for you.

## Dividends

- **Cash dividends** are income into the Account that received the money, from `Income:Dividends`. To tie a dividend to
  one holding, add a Split in the stock Account with amount `0`, price `1` and value `0`.
- **Reinvested dividends** (a dividend reinvestment plan, DRIP) are recorded in the stock Account: the Income Account
  is credited and the stock Account receives the new shares.
- A **return of capital** has no income to record: it only lowers the cost basis, and the number of shares stays the
  same. Record a Split in the stock Account with amount `0` and a negative value equal to the capital returned, against
  a debit to the bank Account. [`stock.split`](method/stock.split) cannot do this.

## Stock splits and mergers

[`stock.split`](method/stock.split) records a split. A merger that swaps shares at a ratio (0.77942 new shares for each
old one) is a stock split in each affected holding, with `shares` the change in the number held (`"-22.058"` for 100 shares).
Give `price` only if you want a new Price recorded.

## Reports

The [Investment Portfolio](report/investment-portfolio), [Advanced Portfolio](report/advanced-portfolio) and
[Investment Lots](report/investment-lots) reports show holdings, cost, gains, income and returns.
