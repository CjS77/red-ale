<!-- SPDX-License-Identifier: GFDL-1.1-no-invariants-or-later -->
---
id: concept/commodities-and-currencies
title: Commodities and currencies
summary: A Commodity is anything an Account can hold - a currency or a security - named by namespace and mnemonic, with a smallest unit. The built-in ISO currencies, euro legacy currencies, display symbols and how to add a security.
related: concept/value-and-amount, concept/prices-and-exchange-rates, concept/investments, method/commodity.list, method/commodity.create, method/currency.list, method/currency.euroConvert
from: guide/ch_currency.docbook#currency_concepts1
---
A **Commodity** is anything an Account can hold. A **currency** is a Commodity in the `CURRENCY` namespace (US dollars,
euros). A **security** is any other Commodity: shares of a company, units of a mutual fund, a bond. **⚠** In GnuCash a
currency is not a separate kind of thing; it is a Commodity, and a *security* is what the rest of the world would call
a commodity.

## Naming

A Commodity is identified by a **namespace** and a **mnemonic**, written `NAMESPACE::MNEMONIC`: `CURRENCY::USD`,
`NASDAQ::AAPL`. The namespace is `CURRENCY` for currencies, an exchange for shares (`NASDAQ`, `NYSE`) or a name you
choose. The mnemonic is the symbol within it. **⚠** The XML calls it `id`, but it is not a GUID. Methods accept a
currency as `USD` or `CURRENCY::USD`.

[`commodity.list`](method/commodity.list) lists them, [`commodity.namespaces`](method/commodity.namespaces) groups
them, [`commodity.get`](method/commodity.get) reads one.

## Currencies

GnuCash supports over two hundred currencies, from the Andorran franc to the Zimbabwe dollar. Every book has all 227
ISO 4217 currencies built in: you can use any in an Account or Transaction without defining it, and the book file
stores one only when it carries quotes or other settings. [`currency.list`](method/currency.list) lists them.
You cannot create or delete a currency.

A currency replaced by the euro (DEM, FRF and so on) is a **euro legacy currency** with a fixed rate;
[`currency.euroConvert`](method/currency.euroConvert) converts between it and the euro.

Several Accounts can hold different currencies in one book: a euro account beside a dollar one. The cost of that is a
conversion every time money crosses between them, which is what [Value and amount](concept/value-and-amount) and
[Prices and exchange rates](concept/prices-and-exchange-rates) are about.

**Report currency.** A report totals in one currency, its *Report's currency* option, which defaults to the
Configuration's `reports.currency`. If your Accounts are in one currency and the reports in another, with no prices
between, the reports show zeros. Set the Configuration's `defaultCurrency` and `reports.currency` to your own
([The Configuration](concept/configuration)).

## Securities

[`commodity.create`](method/commodity.create) adds a security with its namespace, mnemonic, full name and `fraction`.
A security must exist before an Account can hold it. The currency namespace and a namespace containing `::` are
refused. A security has no price until you add one ([Prices and exchange rates](concept/prices-and-exchange-rates)).

## The smallest unit

A Commodity's **smallest commodity unit** (SCU, its `fraction`) is the denominator its amounts are kept in: 100 for
USD (cents), 1 for JPY, 10000 for a typical share. An Account can have its own, set with `scu` when it is created.
[Value and amount](concept/value-and-amount) explains the rounding.

## Symbols

A Commodity shows with its **display symbol**: the symbol you chose (`userSymbol`), else the built-in symbol of the ISO
table (`$`, `€`), else the mnemonic. **⚠** GnuCash calls this the "nice symbol".

## Quotes

A Commodity can carry a quote flag and a quote source naming where its online prices come from. See
[Prices and exchange rates](concept/prices-and-exchange-rates).

## Deleting

[`commodity.delete`](method/commodity.delete) refuses a Commodity something still refers to: an Account, a
Transaction, a business object, or Prices (unless you pass `deletePrices`). GnuCash lets you, and leaves dangling
references; red-ale does not.
