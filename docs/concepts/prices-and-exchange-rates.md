<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: concept/prices-and-exchange-rates
title: Prices and exchange rates
summary: The price database holds the value of one Commodity in another at a time. Prices, direct and indirect lookup, same-day rules and source rank, and fetching quotes from Quote services when the Configuration allows the network.
related: concept/commodities-and-currencies, concept/value-and-amount, concept/investments, method/price.add, method/price.list, method/price.nearest, method/price.latest, method/price.fetchQuotes
from: guide/ch_invest.docbook#invest-stockprice1
---
An Account that holds shares tracks how many you own; what each share is worth lives in the **price database**. A
**Price** is the value of one Commodity in another, usually a currency, at a time: 40.50 USD per share of AMZN on a
given day. Exchange rates between currencies are Prices too, in the same table.

A Price has the Commodity it prices, the `currency` its value is in, a `value` (exact, as given), a time, a
`source` saying where it came from and a `type` (`last`, `bid`, `ask`, `nav` or `unknown`; informational).

## Adding and finding Prices

- [`price.add`](method/price.add) adds one by hand, for a day (`date`) or an exact time. The first price for a
  Commodity puts it in the database; you do it once.
- [`price.update`](method/price.update) and [`price.delete`](method/price.delete) edit and remove.
- [`price.list`](method/price.list) lists them by Commodity, currency and date range.
- [`price.removeOld`](method/price.removeOld) thins old prices, keeping the last of each week, month or quarter.

Transactions in other currencies, transfers and stock splits can record Prices too: see
[`transaction.transfer`](method/transaction.transfer) (`recordPrice`) and [`stock.split`](method/stock.split).

## How a rate is looked up

When something converts one Commodity to another (a report in one currency, a balance of foreign accounts), it asks
for a Price at a time:

- **`nearest`** takes the Price closest to the time (the older on a tie), **`before`** the newest at or before it, and
  **`latest`** the newest there is: [`price.nearest`](method/price.nearest) and [`price.latest`](method/price.latest).
- A **direct** price is one between the two Commodities, in either direction; a Price quoted the other way round is
  inverted. When there is none, an **indirect** price goes through a third Commodity both are priced in (shares to USD
  to EUR). **⚠** For the *latest* lookup GnuCash takes a direct price even if it lies in the future, but an indirect
  one only up to now.
- With no price at all, the conversion gives nothing: a report shows a zero or leaves the Commodity unconverted rather
  than guess.

## The same day

Two Prices of one Commodity pair on the same local day compete ([Dates and time zones](concept/dates-and-time-zones)).
The source decides which stays: a price you entered in the price editor ranks above a fetched quote, which ranks above
one recorded from a transaction, and so on down to `temporary`. A lower-ranked Price never replaces a higher one, and
a Price from an unknown source ranks last. A refused price is reported (`priceStatus`) rather than silently dropped.

## Online quotes

GnuCash fetches quotes through Finance::Quote, a Perl library. *red-ale has no Perl* and does not run
Finance::Quote. It asks web services itself.

- A Commodity is set up with a quote flag and a **quote source** (`alphavantage`, `yahoo_json`, `currency` and so on),
  which says *what* to fetch.
- A **Quote service** in the Configuration says *where from*: a URL template, a response format, an optional key and a
  minimum interval. The Configuration maps each quote source to the services that answer it.
- [`price.fetchQuotes`](method/price.fetchQuotes) starts a fetch in the background and [`price.fetchStatus`](method/price.fetchStatus)
  reports it; the Prices it got are added in one change, with the source GnuCash writes for fetched quotes.
- **The network is off by default.** `quotes.network` is `false` until a person sets it, and without it
  `price.fetchQuotes` fails with [`network-disabled`](error/network-disabled) and sends nothing. It cannot be changed
  through MCP, nor can a service's key.

See [The Configuration](concept/configuration) for these settings. A book whose prices red-ale fetched is a book GnuCash
could have fetched.
