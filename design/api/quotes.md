# API: online quotes

Fetching prices from the web: `price.fetchQuotes` and `price.fetchStatus`, the Quote services they fetch from, and how
a fetched quote becomes a Price. Scope and decisions: [`../milestone3.md`](../milestone3.md) ("Online quotes",
"Decided") and [`../open-questions.md`](../open-questions.md) ("Decided for v0.3"). Prices themselves:
[`prices.md`](prices.md); quote sources on commodities: [`commodities.md`](commodities.md).

GnuCash fetches quotes by running Finance::Quote, a Perl library, as a child process. red-ale has no Perl: it asks
web services itself, in Rust. What it keeps from GnuCash is everything the Book sees: which commodities are fetched,
the quote source names, and how a quote becomes a Price. So a Book whose prices red-ale fetched is a Book GnuCash
could have fetched. This page says what GnuCash does, cited to the 5.17 source, then what red-ale does.

## What GnuCash does

### Which commodities

`gnc_quotes_get_quotable_commodities` (`5.17:libgnucash/app-utils/gnc-quotes.cpp:1009-1057`) collects the
commodities a fetch covers. A commodity is fetched when (`:978-1007`):

- its quote flag is on;
- it has a quote source;
- the source is *supported*: the `currency` source always is (`5.17:libgnucash/engine/gnc-commodity.cpp:182`); the
  others only once Finance::Quote has said it has them (`gnc_quote_source_set_fq_installed`, `:455-484`). A source
  name GnuCash doesn't know, read from a file, is added unsupported
  (`5.17:libgnucash/backend/xml/gnc-commodity-xml-v2.cpp:159-161`).

A commodity that fails these checks is left out **silently**: no failure is recorded (`gnc-quotes.hpp:120`).
`gnucash-cli --quotes get --namespace <regex>` scans only namespaces matching the regex, POSIX extended,
case-insensitive, unanchored (`gnc-quotes.cpp:1023-1035`). The template commodity has no quote flag, so it never
qualifies (`gnc-commodity.cpp:2323-2324`).

**Currencies.** A currency's quote source is `currency` whenever it has none (`gnc-commodity.cpp:1044-1045`), and its
quote flag is turned on when an Account first uses it, unless its `auto_quote_control` slot is `"false"`
(`:1401-1430`). Loading a book sets every Account's commodity, so every currency an Account uses has quotes on after
loading ([`commodities.md`](commodities.md), "Currency quote flags").

**The base currency.** Currencies are quoted in the *default currency*, `gnc_default_currency()`
(`gnc-quotes.cpp:301-315`): a user preference, not Book data. It is the `general/currency-other` preference when
`general/currency-choice-other` is set, else the locale's currency, else `USD`
(`5.17:libgnucash/app-utils/gnc-ui-util.cpp:737-786`). The default currency itself, and any currency with mnemonic
`XXX`, are not fetched (`gnc-quotes.cpp:459-463`, `:688-690`).

### The request and its answer

GnuCash sends Finance::Quote one JSON request for the whole fetch (`comm_vec_to_json_string`,
`gnc-quotes.cpp:448-475`): `defaultcurrency`, then each currency under `currency` and each other commodity under its
quote source's internal name, by **mnemonic** only; the namespace and exchange code are never sent. The wrapper
script turns off Finance::Quote's own currency conversion, so prices come back in each service's own currency
(`5.17:libgnucash/quotes/finance-quote-wrapper.in:253`). For a currency it asks for the rate from the currency to the
default currency, and if there is none, the rate the other way, marked `inverted` (`:151-180`). It sends no date
with a currency rate.

GnuCash reads, per commodity (`parse_quote_json`, `gnc-quotes.cpp:575-597`): `success`, `errormsg`, the first of
`last`, `nav` and `price` present, `inverted`, `date` and `currency`. It reads `time` and ignores it, because
Finance::Quote's times are made up (`:602-607`).

### How a quote becomes a Price

`parse_one_quote` (`gnc-quotes.cpp:680-754`), for each commodity fetched:

1. **No answer** for the commodity: failure `NO_RESULT` (`:701-708`). **`success` false**: failure `QUOTE_FAILED`,
   with Finance::Quote's `errormsg` (`:711-719`).
2. **The value and type** (`get_price_and_type`, `:553-573`): `last` gives type `last`; else `nav` gives `nav`; else
   `price` gives `unknown`, as the old Guile code did (`:569-571`). None of them: failure `NO_PRICE`.
3. **The value is exact**, parsed from the answer's text by `GncNumeric(std::string)` (`:638`;
   `5.17:libgnucash/engine/gnc-numeric.cpp:247-401`). A decimal keeps its written places as an unreduced
   denominator: `"1.0004"` is 10004/10000 (`numeric_from_decimal_match`, `:155-174`; the test at
   `5.17:libgnucash/app-utils/test/gtest-gnc-quotes.cpp:312`). It also takes `n/d`, scientific notation and
   thousands separators; a value that doesn't fit 64 bits fails rather than rounds (`gnc-numeric.cpp:133-138`), as
   `PRICE_PARSE_FAILURE`. Nothing is rounded to an SCU (`5.17:libgnucash/engine/gnc-pricedb.cpp:556-567`).
4. **Inverted** answers take the exact reciprocal, numerator and denominator swapped, unreduced
   (`gnc-quotes.cpp:646-647`; `gnc-numeric.cpp:422-430`): 10832/10000 becomes 10000/10832.
5. **The currency** is the answer's `currency`, upper-cased and looked up among the ISO currencies
   (`gnc-quotes.cpp:652-678`). None: failure `NO_CURRENCY`. One the Book's table lacks (`BTC`): failure
   `UNKNOWN_CURRENCY`. There is no fallback to the default currency.
6. **The time** (`calc_price_time`, `:599-630`): the answer's `date`, read as month-day-year
   (`GncDate(date, "m-d-y")`), at the day-neutral time (10:59 UTC, shifted in zones beyond UTC−10 and UTC+13;
   `5.17:libgnucash/engine/gnc-datetime.cpp:244-269`; the test at `gtest-gnc-quotes.cpp:263-279`). No date, or one
   that doesn't parse: **now**, to the second. Currency rates have no date, so they are always stamped now.
7. **The Price**: the commodity fetched, in the answer's currency, at that time, source `Finance::Quote`
   (`PRICE_SOURCE_FQ`, `gnc-pricedb.cpp:105-109`), with the type and value above (`gnc-quotes.cpp:744-752`).

The prices are then added one by one through `gnc_pricedb_add_price` (`create_quotes`, `gnc-quotes.cpp:805-817`),
so `add_price`'s same-day rules apply ([`prices.md`](prices.md), `price.add`): a fetched price (rank 1) replaces a
same-day price of the pair in **either direction** of rank 1 or worse, and never one from the price editor (rank 0).

The commodity's quote time zone is not used: nothing in the quote code reads it.

### Keys

GnuCash has two key preferences, passed to Finance::Quote as environment variables (`gnc-quotes.cpp:82-85`,
`:279-295`): `alphavantage-api-key` as `ALPHAVANTAGE_API_KEY`, and `yhfinance-api-key` as `FINANCEAPI_API_KEY`. A
preference that is set overrides the variable; an empty one leaves the inherited variable as it is. A missing Alpha
Vantage key only logs a warning (`:290-292`). The child inherits GnuCash's whole environment (`:177`), so the keys of
Finance::Quote's other modules work when they are in the environment.

### Failures and the CLI

The failure kinds are `NO_RESULT`, `QUOTE_FAILED`, `NO_CURRENCY`, `UNKNOWN_CURRENCY`, `NO_PRICE` and
`PRICE_PARSE_FAILURE` (`5.17:libgnucash/app-utils/gnc-quotes.hpp:39-49`; `UNKNOWN_PRICE_TYPE` is never raised). A
fetch with failures still stores the prices it got. `gnucash-cli --quotes get` prints the failures, exits with status
2 when there were any, and saves the book (`5.17:gnucash/gnucash-commands.cpp:342-385`). GnuCash sets **no
timeout** on Finance::Quote (`gnc-quotes.cpp:243-244`).

## What red-ale does

### Quote sources and Quote services

The Book keeps quote sources exactly as GnuCash does: Finance::Quote's internal names, stored on the commodity
([`commodities.md`](commodities.md)). 5.17 knows the `currency` source, 42 single sources and 7 multi-market ones
(`5.17:libgnucash/engine/gnc-commodity.cpp:180-256`), and red-ale keeps any other name as read.

Where a source's quotes come from is the Configuration's business, not the Book's: a **Quote service** is a URL
template, a response format, an optional key and a minimum interval between requests. The Configuration maps each
source name to the services that answer it, tried in order until one gives a quote. A source is *supported* when it
maps to at least one service.

```json
{
  "defaultCurrency": "CURRENCY::USD",
  "quotes": {
    "network": false,
    "timeoutSeconds": 30,
    "services": {
      "frankfurter": {
        "format": "frankfurter",
        "currencyUrl": "https://api.frankfurter.dev/v1/latest?base={base}&symbols={quote}"
      },
      "alphavantage": {
        "format": "alphavantage",
        "securityUrl": "https://www.alphavantage.co/query?function=GLOBAL_QUOTE&symbol={symbol}&apikey={key}",
        "currencyUrl": "https://www.alphavantage.co/query?function=CURRENCY_EXCHANGE_RATE&from_currency={base}&to_currency={quote}&apikey={key}",
        "keyEnv": "ALPHAVANTAGE_API_KEY",
        "minIntervalSeconds": 15
      },
      "my-proxy": {
        "format": "json",
        "securityUrl": "http://127.0.0.1:8200/quote/{symbol}",
        "pointers": { "price": "/price", "currency": "/currency", "date": "/date" }
      }
    },
    "sources": {
      "currency": ["frankfurter", "alphavantage"],
      "alphavantage": ["alphavantage"],
      "nasdaq": ["alphavantage", "twelvedata"]
    }
  }
}
```

| Setting | Meaning |
|---------|---------|
| `defaultCurrency` | GnuCash's default currency preference (`general/currency-other`): what currencies are quoted in, and the currency of new Owners and QIF imports ([`import.md`](import.md)). Unset, the Locale's currency, else `USD`, as `gnc_default_currency` (`gnc-ui-util.cpp:737-786`) |
| `quotes.network` | Network access, off by default. Off, `price.fetchQuotes` fails before any connection is made |
| `quotes.timeoutSeconds` | Per request; default 30. GnuCash has none, but a hung service must not hold a Quote fetch for ever |
| `quotes.services.<name>.format` | Which parser reads the answer (below) |
| `…securityUrl`, `…currencyUrl` | URL templates for securities and for currency rates. A service without one can't answer that kind |
| `…key` | The key, if one is used. Never shown by `config.get`, only whether it is set |
| `…keyEnv` | An environment variable to take the key from when `key` is unset |
| `…minIntervalSeconds` | Least time between two requests to the service; default 0 |
| `…currency` | The currency of a price when the format's answer has none (`alphavantage` and `finnhub` securities) |
| `…pointers` | For the `json` format: JSON Pointers to `price` (required), `currency`, `date` |
| `quotes.sources.<source>` | The services answering a quote source, in order |

**URL templates** take these placeholders, each percent-encoded as it is filled in; any other `{…}` is refused by
`config.set`:

| Placeholder | Filled with |
|-------------|-------------|
| `{symbol}` | The commodity's mnemonic, as GnuCash sends it (`gnc-quotes.cpp:457`, `:468`) |
| `{base}`, `{quote}` | A rate's two currencies: the price of one `{base}` in `{quote}` |
| `{date}` | Today's local date, `YYYY-MM-DD` |
| `{key}` | The service's key |

Every URL red-ale fetches from comes from these settings; the values in this page are defaults, nothing more. The
default services and the source mapping are those of [`../milestone3.md`](../milestone3.md), "Quote services"
(*Proposed*; to be rechecked before release):

| Source(s) | Services | Default `keyEnv` |
|-----------|----------|------------------|
| `currency` | `frankfurter`, then `alphavantage` | |
| `alphavantage` | `alphavantage` | `ALPHAVANTAGE_API_KEY`, GnuCash's (`gnc-quotes.cpp:82-85`) |
| `twelvedata` | `twelvedata` | `TWELVEDATA_API_KEY` (Finance::Quote's; docs-only) |
| `finnhub` | `finnhub` | `FINNHUB_API_KEY` (Finance::Quote's; docs-only) |
| `yahoo_json` | `yahoo-chart` | |
| `nasdaq`, `nyse`, `usa` | `alphavantage`, then `twelvedata` | |

`FINANCEAPI_API_KEY`, GnuCash's other variable, belongs to the `financeapi` source (YH Finance), which has no default
service; a service added for it can name it in `keyEnv`.

### Keys and the network

- **A key never leaves the Configuration** except in the request to its own service: it is never written to the
  Book, the journal, a log, a status or an error. Errors quote URLs with the key replaced by `***`.
- **A key is bound to its service's host.** `config.set` clears `key`, and a `keyEnv` the file sets, when a
  service's URL changes to another host, and a default service's environment key (`keyEnv`) is used only while every
  URL of the service is on its default host. So a client that can change URLs can't send a key elsewhere
  ([`configuration.md`](configuration.md)).
- **Keys, `keyEnv` and `quotes.network` can't be set over MCP** (`config.set` refuses them there; see
  [`../milestone3.md`](../milestone3.md), "The Configuration API"). `keyEnv` is refused because naming a variable on
  a service of the agent's own would hand it that key.
- **Credentials through a proxy.** A service's URL can point at a local credential proxy that adds the key itself;
  its template then has no `{key}` and red-ale holds none.
- **Only Quote fetches use the network.** Tests never do: each format is tested against recorded answers, and one
  test checks that a fetch with `quotes.network` off fails before any connection is attempted.

### Response formats

Each format is a parser from one service's answer to a **quote record**: the fields Finance::Quote hands GnuCash
(`success`, `errormsg`, `last` or `nav` or `price`, `currency`, `date`, `inverted`). The parsers are an
anti-corruption layer: no provider's field names reach the code that builds prices, which is translated from
`gnc-quotes.cpp:546-760` and sees only quote records. Numbers are read from their text, never through a float: a
JSON number is taken as written (`serde_json`'s `arbitrary_precision`), so `1.0945` reaches the price builder as the
text `1.0945`, exactly as GnuCash's `property_tree` passes it on. A date is handed on as month/day/year, the form
`calc_price_time` reads.

What follows is from the providers' public documentation, **docs-only**, as of October 2026. Each parser is tested
against recorded answers, and the table is rechecked before release.

| Format | Default URL templates | Fields used | Notes |
|--------|-----------------------|-------------|-------|
| `alphavantage` | Securities: `https://www.alphavantage.co/query?function=GLOBAL_QUOTE&symbol={symbol}&apikey={key}`. Currencies: `https://www.alphavantage.co/query?function=CURRENCY_EXCHANGE_RATE&from_currency={base}&to_currency={quote}&apikey={key}` | `Global Quote`: `05. price` (as `last`), `07. latest trading day` (date). `Realtime Currency Exchange Rate`: `5. Exchange Rate` (as `last`) | A security's answer has no currency: the service's `currency` setting gives it, default `USD`. Finance::Quote's AlphaVantage module guesses the currency from the symbol's exchange suffix (docs-only); check that module and translate its table before code. An answer holding only `Note`, `Information` or `Error Message` is a failure; the first two mean the rate limit |
| `twelvedata` | Securities: `https://api.twelvedata.com/quote?symbol={symbol}&apikey={key}`. Currencies: `https://api.twelvedata.com/exchange_rate?symbol={base}/{quote}&apikey={key}` | `/quote`: `close` (as `last`), `currency`, `datetime` (date). `/exchange_rate`: `rate` (as `last`) | `{"status": "error", "code", "message"}` is a failure; code 429 is the rate limit |
| `finnhub` | Securities: `https://finnhub.io/api/v1/quote?symbol={symbol}&token={key}` | `c` (as `last`), `t` (Unix time, for the date) | No currency in the answer: the service's `currency` setting, default `USD` (the free tier is mostly US markets). `c` and `t` both 0 means an unknown symbol. No currency rates |
| `frankfurter` | Currencies: `https://api.frankfurter.dev/v1/latest?base={base}&symbols={quote}` | `rates.<quote>` (as `last`) | ECB reference rates, once a working day, keyless. Currencies only |
| `yahoo-chart` | Securities: `https://query1.finance.yahoo.com/v8/finance/chart/{symbol}?interval=1d&range=1d`. Currencies: the same with the symbol `{base}{quote}=X` | `chart.result[0].meta`: `regularMarketPrice` (as `last`), `currency`, `regularMarketTime` (Unix time, for the date, in `exchangeTimezoneName`) | Unofficial and best-effort; often refused with HTTP 429. Prices in `GBp` (pence) are turned into `GBP` by dividing by 100 exactly, as Finance::Quote's YahooJSON module does (docs-only). A non-null `chart.error` is a failure |
| `json` | Set by the user | The `pointers`: `price` (as `last`), `currency`, `date` (`YYYY-MM-DD`, RFC 3339 or Unix seconds) | For services red-ale doesn't know. A `price` pointer that finds nothing is a failure; no `currency` pointer means the service's `currency` setting |

An HTTP status other than 2xx, a timeout or an answer that isn't the format's JSON fails that service for that
commodity, and the next service of the source is tried.

### Building prices

red-ale builds each Price with `parse_one_quote`'s steps, translated, from the quote record the first answering
service gave:

- **which commodities**: those `gnc_quotes_get_quotable_commodities` takes (quote flag on, a source), with
  *supported* meaning "mapped to a service". Unlike GnuCash, a commodity whose source has no service is **reported**
  (`unsupported-source`), not left out silently, so a user can see why a price is missing. The default currency and
  `XXX` are left out, as in GnuCash. Commodities are fetched in id order (namespace, then mnemonic), not GnuCash's
  hash-table order, so a fetch is reproducible;
- **currencies** are asked for as the rate from the currency to `defaultCurrency`, and if no service answers,
  the rate the other way, inverted, as the wrapper asks (`finance-quote-wrapper.in:151-180`). The quote record of a
  currency carries no date, as the wrapper's carries none, so a currency rate is stamped now, as in GnuCash, even when
  the service dates it;
- **the value**: GnuCash's decimal parsing, exact and unreduced, and the exact reciprocal when inverted;
- **the type**: `last`, `nav` or `unknown`;
- **the currency**: the answer's, looked up among ISO currencies; no fallback;
- **the time**: the date at its day-neutral time, else now;
- **the source**: `Finance::Quote`. **⚠** Here it means "an online quote", not that Finance::Quote fetched it; it is
  the only string GnuCash ranks as a fetched quote ([`../open-questions.md`](../open-questions.md), "Decided for v0.3").

### Fetching in the background

A **Quote fetch** runs in two parts. Its requests run outside the Book, so a slow or rate-limited service holds up
no other method: different services are asked in parallel, each service's requests one at a time with its
`minIntervalSeconds` between them. When every request has finished, the Prices are added in **one Mutation**, taking
its turn with every other method, with `price.add`'s same-day rules applied to each, in commodity order, against the
Book as it then is. A commodity deleted meanwhile is reported (`not-found`) and gets no Price.

- **One at a time.** `price.fetchQuotes` refuses with `conflict` while a Quote fetch is running.
- **Status is kept** for the running fetch and the last 16 finished ones, until the book closes.
- **Closing the book cancels** a running Quote fetch: outstanding requests are dropped and nothing from it is stored.
- A read-only book can't fetch (`read-only`): a fetch adds Prices.
- **No scheduler.** Users run quotes from cron through the CLI, as GnuCash users run `gnucash-cli --quotes get`.

## Methods

| Method | Effect | Does |
|--------|--------|------|
| `price.fetchQuotes` | write | Start a Quote fetch; returns its ID at once |
| `price.fetchStatus` | read | A Quote fetch's state and, per commodity, the Price stored or why there is none |

### `price.fetchQuotes` (write)

| Param | Type | Meaning |
|-------|------|---------|
| `commodities` | CommodityId[]? | Fetch these, whatever their quote flag; each needs a quote source (a currency has `currency`). Default: every commodity with quotes on and a source, as above |
| `namespace` | string? | Only commodities in namespaces matching this pattern, case-insensitive and unanchored, as `gnucash-cli --namespace` (`5.17:gnucash/gnucash-cli.cpp:86-87`). Rust `regex` syntax, which reads ordinary POSIX extended patterns the same. Not with `commodities` |

Returns `{ fetch, commodities: CommodityId[] }`: the Quote fetch's ID and the commodities it covers, in order.
Errors: `no-book`; `read-only`; `network-disabled` (new, below) when `quotes.network` is off; `conflict` while
another fetch runs; `not-found` for an unknown commodity; `invalid` for a commodity with no quote source, a pattern
that doesn't compile, or a fetch that covers nothing (GnuCash: "GncQuotes::Fetch called with no commodities",
`gnc-quotes.cpp:341-342`).

### `price.fetchStatus` (read)

`{ fetch?: string }` → the status of that Quote fetch, or of the latest one:

| Field | Meaning |
|-------|---------|
| `fetch` | Its ID |
| `state` | `running`, `finished` or `cancelled` |
| `started`, `finished` | Timestamps; `finished` is null while running |
| `commodities` | One entry per commodity, in the order fetched (below) |
| `stored`, `failed` | Counts, once finished |

Each commodity: `{ commodity, source, state, service, outcome, price, existing, reason, message }`:

- `state`: `pending`, `fetched` (waiting for the Mutation), `stored` or `failed`;
- `service`: the service that answered, or the last one tried;
- once stored, `outcome` is `price.add`'s: `added`, `replaced`, `duplicate` or `rejected`, with `price` the new
  `PriceInfo` and `existing` the price that kept its place or was replaced;
- once failed, `reason` is one of GnuCash's failures, `no-result`, `quote-failed`, `no-currency`, `unknown-currency`,
  `no-price`, `price-parse-failure`, or one of red-ale's: `unsupported-source` (no service), `service-error` (HTTP or
  format error, rate limit), `timeout`, `not-found` (deleted during the fetch). `message` gives the detail, such as the
  service's own error text, with any key removed.

Errors: `no-book`; `not-found` (`data.kind: "quoteFetch"`) for an unknown ID, or when there has been no fetch.

### Errors

One new code, for [`../api.md`](../api.md):

| Code | Name | When |
|------|------|------|
| 5000 | `network-disabled` | A method needs the network and `quotes.network` is off. Nothing was sent |

## CLI

```
red-ale exec --book books/home.gnucash price.fetchQuotes
red-ale exec --book books/home.gnucash price.fetchQuotes namespace=NASDAQ
```

With `exec`, `price.fetchQuotes` waits for its Quote fetch to finish, prints its final status, saves the book when a
Price was stored, and exits with status 2 when any commodity failed, as `gnucash-cli --quotes get` does
(`5.17:gnucash/gnucash-commands.cpp:368-372`). `call` returns at once with the ID, and `price.fetchStatus` follows it.

## Deviations from GnuCash

| What | GnuCash 5.17 | red-ale | Why |
|------|--------------|---------|-----|
| Who fetches | Finance::Quote, in Perl | red-ale, from Quote services in the Configuration | No Perl; every URL configurable |
| A commodity whose source can't be fetched | Left out silently | Reported `unsupported-source` | A missing price has a visible reason |
| Order of requests | Hash-table order | Commodity id order | Reproducible fetches and tests |
| Timeout | None | `quotes.timeoutSeconds` per request | A hung service can't hold the fetch |
| Total failure from `gnucash-cli --quotes get` | Exit status 0, book saved (`gnucash-commands.cpp:374-378`) | An error, exit status 2, nothing saved | Failures are visible |
| Keys | Two preferences, else the inherited environment | Per service, else `keyEnv`, bound to the service's host | A key goes only where it was meant to |
| Network | Always on | Off until the Configuration turns it on | Offline by default |

## Tests

- **Price building:** GnuCash's own expectations from `5.17:libgnucash/app-utils/test/gtest-gnc-quotes.cpp`,
  rebuilt over quote records: exact unreduced values (`:312`), inverted currencies, the 10:59 UTC time from a date
  (`:263-279`), now without one (`:307-310`), each failure (`BTC` as an unknown currency, `:345-355`).
- **Formats:** each parser against recorded answers, including errors and rate-limit answers; numbers keep their
  written digits.
- **Background:** with a recorded service that answers slowly, other methods are answered meanwhile; closing the book
  cancels the fetch and stores nothing; a second `price.fetchQuotes` is `conflict`.
- **Safety:** with `quotes.network` off nothing connects; no key appears in any result, status, log or journal; a
  URL changed to another host loses its key.
- **Oracle:** GnuCash 5.17 reads the fetched Prices with source `Finance::Quote` and ranks them as fetched quotes.
