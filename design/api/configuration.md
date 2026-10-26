# API: the Configuration

red-ale's **Configuration** is its own JSON settings file (`--config <file>`; without it, settings last as long as
the process): what GnuCash keeps outside the book, in its preferences and its user data directory. v0.3 declares
every setting in a JSON Schema and adds three methods to read and change them
([`../milestone3.md`](../milestone3.md), "The Configuration API"). Domain code: `red-ale-service::config`
(`schema` declares the settings, `settings` reads and changes them); methods: `red-ale-rpc::methods::config`.

## Methods

| Method          | Effect | Target          | Does |
|-----------------|--------|-----------------|------|
| `config.schema` | read   |                 | The Configuration's JSON Schema (2020-12): every setting, its type, default and description. |
| `config.get`    | read   |                 | The whole Configuration, or one setting by path, with defaults filled in; secrets shown only as set or unset. |
| `config.set`    | write  | `configuration` | Change one or more settings by path, validated against the schema, all or none; `null` restores a default. |

Like the other Configuration methods (`period.set`, `docLink.setPathHead`, the saved report configuration and style
sheet methods), they work with no book open, never make a book dirty and never touch the journal. Those older
methods stay, unchanged, as shortcuts to the same settings: `period.set` writes `accountingPeriod.*`,
`docLink.setPathHead` writes `docLinks.pathHead`, and the report methods write `reports.saved` and
`reports.styleSheets`.

### Setting paths

A setting is named by its **setting path**: its keys from the top of the Configuration joined with `.`, such as
`import.settings.matchThreshold` or `quotes.services.alphavantage.key`. A map entry (a Quote service, a quote
source, a CSV import layout) is a key like any other. A path may name a single setting or a group
(`import.settings`, `quotes.services.mine`). Keys are never empty and never contain `.`; Quote service and source
names are letters, digits, `_` and `-`. JSON Pointer was not chosen: the paths are typed on command lines and by
agents, and no declared key needs escaping. A path cannot reach inside a free-form object (a CSV import layout, the
QIF account map, a saved configuration's options): set the object whole.

### `config.schema`

No params. Returns the schema document. Beyond standard JSON Schema it uses:

- `format` as an assertion, with red-ale's formats: `period-bound`, `fiscal-year-end`, `guid`, `commodity-id`,
  `decimal` (a non-negative decimal string), `url-template` (an `http` or `https` URL with a host, holding no
  placeholder but `{symbol}`, `{base}`, `{quote}`, `{date}` and `{key}`), `json-pointer`, `env-name` and `name`;
- `readOnly: true` for settings other methods own (`reports.saved`, `reports.styleSheets`);
- `writeOnly: true` for secrets (a Quote service's `key`);
- `x-refusedOverMcp: true` for settings only a person may change: `quotes.network`, and each service's `key` and
  `keyEnv`.

`locale`'s `enum` lists the supported locales, so clients learn them from the schema and `api.schema` needs nothing
new.

### `config.get`

Params: `path` (optional; absent for the whole Configuration).

Result: `{ path, value, unknown, problems, configFile }`.

- `value`: the setting, or the whole Configuration, with every default filled in. A map's defaults (the default
  Quote services and sources) are listed beside the entries the file adds, and a file entry overrides a default one
  field by field. A map entry that does not exist is `null`.
- **Secrets are never shown.** Each reads `{"set": true}` or `{"set": false}`, at any depth; asking for the secret's
  own path gives the same. No key appears in any result, error message, log or journal.
- `unknown`: keys in the file, at or below `path`, that declare no setting (`other`, `import.settings.typo`). They
  are kept on every write and do nothing; listing them shows typos in a file edited by hand.
- `problems`: `[{ path, message }]` for settings, at or below `path`, whose values in the file do not match the
  schema. A problem's path gives list items as `[i]`.
- `configFile`: the file, or `null` for a Configuration held in memory.

An unknown or malformed path is `invalid-params`.

### `config.set`

Params: `changes`, an object from setting paths to new values; `null` restores a setting's default (removes it from
the file, along with any group the removal leaves empty).

Result: `{ values, clearedKeys, configFile }`: each path changed with its value now, as `config.get` shows it; the
`key` and `keyEnv` settings cleared because their service moved host (below).

- **Atomic.** Every change is checked before any is made; then the file is written to a temporary file beside it,
  synced and renamed into place. A refused or interrupted call leaves the file as it was, byte for byte. A new file
  is created readable by its owner only (`0600`), since it can hold keys; an existing file keeps its mode.
- **Validated against the schema.** Refused with `invalid-params`, and `data.problems` listing every problem found:
  an unknown or malformed path; a path inside another path of the same call; a setting another method owns
  (`reports.saved`, `reports.styleSheets`, or a group holding one, such as `reports`), since replacing those lists
  would delete saved configurations, which a `write` method must not do; a value that does not match the schema; a
  Quote service with no `format`, with neither `securityUrl` nor `currencyUrl`, or of the `json` format with no
  `pointers.price`; a quote source naming a service that does not exist. A change may not introduce a problem, nor
  leave one in a setting it touches; problems elsewhere in a hand-edited file are left alone and do not block it.
- **Groups merge over defaults.** Setting a group (`quotes.sources`, `import.settings`) replaces what the file holds
  for it; defaults still fill in what it leaves out.
- **Unknown keys are kept**, at every level.
- **A key is bound to its service's host** ([`quotes.md`](quotes.md), "Keys and the network"). When a change gives
  a Quote service a URL (`securityUrl` or `currencyUrl`) on an origin none of its URLs had before (scheme, host and
  port: `https` to `http` counts), its stored `key` and `keyEnv` are cleared and listed in `clearedKeys`, unless the
  same call sets them. Restoring a default URL with `null` is a move too. A default service's `keyEnv` then reads its
  default again; the Quote fetch uses an environment key only while every URL of the service is on its default host.
  So a client that can change the Configuration cannot send a key somewhere new without also knowing it.
- **Keys and the network switch can't be set over MCP.** Called through the MCP server, `config.set` refuses any
  call that would change a setting marked `x-refusedOverMcp` (a `key` set, replaced or cleared; a `keyEnv`;
  `quotes.network`) with `conflict` and `data: { "reason": "refused-over-mcp", "paths": [...] }`, and writes
  nothing. Setting one to the value it has already is no change and passes. A key cleared by a move to another host
  is not refused, so an agent can change a service's URL. Through the CLI, JSON-RPC or the file, a person can change
  them all. `keyEnv` is refused too, though not secret: naming `ALPHAVANTAGE_API_KEY` on a service of the agent's
  own would hand it that key.

The MCP server tells the dispatcher which door a call came through (`Dispatcher::call_from(Origin::Mcp, …)`); the
rule itself lives in `red-ale-rpc`, so `red-ale-mcp` keeps no logic of its own ([`../mcp.md`](../mcp.md)).

## Settings

Every setting, with its default. Settings declared in v0.3 ahead of the work that reads them are marked *declared*.

### Accounting period and document links (v0.1, v0.2)

| Setting | Type | Default | Meaning |
|---|---|---|---|
| `accountingPeriod.start` | `period-bound` | `year` | The period's start: a relative choice or `YYYY-MM-DD` ([`accounts.md`](accounts.md)). |
| `accountingPeriod.end` | `period-bound` | `year` | Its end. |
| `accountingPeriod.fiscalYearEnd` | `fiscal-year-end` or null | null | `MM-DD`; unset uses the open book's `fy_end` slot. |
| `docLinks.pathHead` | string or null | null | The path head relative document links start from ([`transactions.md`](transactions.md)). |

### Reports (v0.2)

[`../reports.md`](../reports.md) §4 and §9.

| Setting | Type | Default | Meaning |
|---|---|---|---|
| `reports.directory` | string or null | null | The report directory; relative to the Configuration file. |
| `reports.currency` | `commodity-id` or null | null | The default of every "Report's currency" option. Unset, the Locale's currency if the book has it, else `USD` (`gnc_default_report_currency`). |
| `reports.reverseBalancedAccounts` | `none`, `credit`, `incomeExpense` | `credit` | Which accounts show reversed balances. |
| `reports.dateFormat` | `us`, `uk`, `ce`, `iso`, `locale` | `locale` | How report dates print; `locale` is the Locale's date format. |
| `reports.weekStart` | `sunday` … `saturday`, or null | null | The first day of the week. Unset, the Locale's (`sunday` under C). |
| `reports.limits.<limit>` | integer ≥ 1 | §2's defaults | `operations`, `seconds`, `call-depth`, `expression-depth`, `string-size`, `array-size`, `map-size`, `modules`, `variables`, `functions`, `number-size`. |
| `reports.saved` | list (owned) | `[]` | Saved report configurations; changed only by the `report.*Configuration` methods. |
| `reports.styleSheets` | list (owned) | `[]` | Style sheet entries; changed only by the `report.*StyleSheet` methods. |

### v0.3

| Setting | Type | Default | Meaning |
|---|---|---|---|
| `locale` | `C`, the supported locales (`fr_FR` …), `environment` | `C` | The Locale reports format in, unless `report.run` or `report.export` names one, and the default of an unset `reports.currency` and `reports.weekStart` ([`../milestone3.md`](../milestone3.md), "Locales"; [`../reports.md`](../reports.md) §4). `environment` takes each category from `LC_ALL`, else `LC_NUMERIC`, `LC_MONETARY`, `LC_TIME` or `LC_MESSAGES`, else `LANG`, as the C library does; an unsupported value there fails the run. |
| `defaultCurrency` | `commodity-id` or null | null | GnuCash's default currency: what currencies are quoted in, and the currency of new Owners and QIF imports. Unset, the Locale's currency, else `USD` ([`quotes.md`](quotes.md)). Read by Quote fetches; *declared* for new Owners and QIF imports. |
| `jsonRpc.maxRequestBytes` | integer, 1 MiB to 4 GiB − 1 | 64 MiB | The largest request `red-ale serve` accepts (jsonrpsee's default is 10 MB). Read when the server starts. |
| `mcp.maxRequestBytes` | integer, 1 MiB to 4 GiB − 1 | 64 MiB | The largest message `red-ale mcp` accepts (rmcp's default is 16 MiB). Read when the server starts. |
| `import.settings.enableSkip` | boolean | `true` | "Skip" may be chosen (`enable-skip`). *Declared.* |
| `import.settings.enableUpdate` | boolean | `true` | "Update and clear" may be chosen (`enable-update`). *Declared.* |
| `import.settings.useBayes` | boolean | `true` | Bayesian matching instead of the plain account map (`use-bayes`). *Declared.* |
| `import.settings.matchThreshold` | integer 0–1000 | `1` | Lowest score offered as a match. *Declared.* |
| `import.settings.matchDateThreshold` | integer 0–3650 (days) | `4` | Dates this close add 2 to the score. *Declared.* |
| `import.settings.matchDateNotThreshold` | integer 0–3650 (days) | `14` | Dates further apart take 5 off. *Declared.* |
| `import.settings.autoAddThreshold` | integer 0–1000 | `3` | At or below this best score the transaction is added. *Declared.* |
| `import.settings.autoClearThreshold` | integer 0–1000 | `6` | At or above this best score the match is cleared. *Declared.* |
| `import.settings.atmFeeThreshold` | `decimal` string | `"2.00"` | Amount difference still matched, for ATM fees. *Declared.* |
| `import.csvLayouts.<name>` | object | none | A CSV transaction import layout ([`import.md`](import.md)). Its fields are declared with CSV import. *Declared.* |
| `import.csvPriceLayouts.<name>` | object | none | A CSV price import layout. *Declared.* |
| `import.qifAccountMap` | object | `{}` | The QIF account map, saved by `import.apply`. *Declared.* |
| `quotes.network` | boolean | `false` | Let Quote fetches reach the network; nothing else does. Refused over MCP. |
| `quotes.timeoutSeconds` | integer 1–600 | `30` | How long one request may take. |
| `quotes.services.<name>` | Quote service | below | Where quotes are fetched from. |
| `quotes.sources.<source>` | list of service names, at least one | below | The services that answer a Finance::Quote source, in order. |

The Import settings are GnuCash's, with its defaults
(`5.17:gnucash/gschemas/org.gnucash.GnuCash.dialogs.import.generic.gschema.xml.in:3-47`). GnuCash reads the numbers
as doubles (`5.17:gnucash/import-export/import-settings.cpp:96-106`); red-ale keeps scores and days as integers
and the ATM fee, an amount, as a decimal string. The upper bounds (1000 for scores, 3650 days) are red-ale's own,
not checked against GnuCash's dialog.

A **Quote service** ([`quotes.md`](quotes.md), "Quote services") has:

| Field | Type | Default | Meaning |
|---|---|---|---|
| `format` | `alphavantage`, `twelvedata`, `finnhub`, `frankfurter`, `yahoo-chart`, `json` | (required) | The response format, which picks the parser. |
| `securityUrl`, `currencyUrl` | `url-template` or null | null | The URL templates for securities and currency rates; at least one. |
| `key` | string or null, **secret** | null | The service's key. Never shown; refused over MCP; bound to the URLs' hosts. |
| `keyEnv` | `env-name` or null | null | An environment variable to take the key from when `key` is unset. Refused over MCP; bound like `key`. |
| `minIntervalSeconds` | integer 0–86,400 | `0` | The least time between two requests to the service. |
| `currency` | `commodity-id` or null | null | The currency of a price when the answer has none (the format's default applies when unset). |
| `pointers.price`, `pointers.currency`, `pointers.date` | `json-pointer` or null | null | For the `json` format, which needs `price`: where the price, its currency and its date are. |

The default services and sources are those of [`quotes.md`](quotes.md), with these intervals:

| Service | URLs | `keyEnv` | `minIntervalSeconds` |
|---|---|---|---:|
| `frankfurter` | currency | | 0 |
| `alphavantage` | security, currency | `ALPHAVANTAGE_API_KEY` | 15 |
| `twelvedata` | security, currency | `TWELVEDATA_API_KEY` | 8 |
| `finnhub` | security | `FINNHUB_API_KEY` | 1 |
| `yahoo-chart` | security, currency | | 2 |

| Source | Services |
|---|---|
| `currency` | `frankfurter`, `alphavantage` |
| `alphavantage` | `alphavantage` |
| `twelvedata` | `twelvedata` |
| `finnhub` | `finnhub` |
| `yahoo_json` | `yahoo-chart` |
| `nasdaq`, `nyse`, `usa` | `alphavantage`, `twelvedata` |

The intervals other than Alpha Vantage's 15 are red-ale's reading of the free tiers (Twelve Data 8 a minute, Finnhub
60 a minute; Yahoo has none published); recheck them with the URLs before release.

## Adding a setting

A setting is one property in `red-ale-service/src/config/schema.rs`, with its type, default and description; the
validation, defaults, masking and MCP rule all follow from it. A secret gets `writeOnly` and `x-refusedOverMcp`; a
list another method owns gets `readOnly`. Code reads a setting with `Config::checked("import.settings.matchThreshold")`
(defaults filled in, `invalid` if the file holds a bad value for it), or `Config::effective()` for all of them. A
check the schema cannot express goes in `schema::cross_checks`. The setting then gets a row above.
