# API: commodities and currencies

The commodity table, the security editor, the built-in ISO 4217 currencies and euro conversion. Domain code:
`crates/red-ale-core/src/ops/commodities.rs`; methods: `crates/red-ale-rpc/src/methods/commodities.rs`. Research:
[`notes/engine-commodities-prices-lots.md`](../notes/engine-commodities-prices-lots.md) §1–§3.

## The commodity table

GnuCash loads the template commodity and its 227 ISO 4217 currencies into every book
(`5.17:libgnucash/engine/gnc-commodity.cpp:2317-2327`). red-ale does the same without storing them: the table is the
book's own commodities plus every built-in currency the book does not define. A currency is stored (and written) only
when GnuCash would write it: with quotes on or slots (`5.17:libgnucash/backend/xml/gnc-commodity-xml-v2.cpp:68`).
Accounts, transactions and prices can name any built-in currency without defining it first.

Lookups follow GnuCash's: `ISO4217` is `CURRENCY`, and the renamed codes `RUR PLZ UAG NIS MXP TRL` find
`RUB PLN UAH ILS MXN TRY` (`5.17:libgnucash/engine/gnc-commodity.cpp:131-145`). The table is 5.17's: KRW has fraction
1, and BGN is an ordinary currency that also has a fixed euro rate.

### `commodity.namespaces` (read)

No params. Returns `[{ name, commodities, isCurrency, reserved }]`, sorted by name. `CURRENCY` and `template` are
reserved: no security can be created in them.

### `commodity.list` (read)

| Param       | Type    | Meaning                                                      |
|-------------|---------|--------------------------------------------------------------|
| `namespace` | string? | Only this namespace (`CURRENCY` or `ISO4217` for currencies) |
| `usedOnly`  | bool    | Only commodities something in the book refers to             |

Returns `CommodityInfo[]` sorted by namespace, then mnemonic.

### `commodity.get` (read)

`{ commodity: CommodityId }` → `CommodityInfo`; `not-found` if neither the book nor the ISO table has it.

`CommodityInfo`:

| Field             | Meaning                                                                                     |
|-------------------|---------------------------------------------------------------------------------------------|
| `id`              | `NAMESPACE::MNEMONIC`                                                                       |
| `namespace`, `mnemonic`, `fullname`, `cusip`, `fraction` | As GnuCash holds them. A currency's `cusip` is its ISO numeric code. |
| `isCurrency`      | In the `CURRENCY` namespace (GnuCash's `gnc_commodity_is_iso`)                              |
| `builtIn`         | One of the 227 ISO currencies                                                               |
| `defined`         | Stored in the book file                                                                     |
| `quoteFlag`, `quoteSource` | As GnuCash reports them after loading the book (below)                             |
| `quoteSourceType` | `single`, `multi`, `currency`, or `unknown` for a source 5.17 does not know (kept as read)  |
| `quoteTz`         | Quote time zone; null is "use local time"                                                   |
| `userSymbol`, `defaultSymbol` | The `user_symbol` slot; a currency's built-in symbol                            |
| `displaySymbol`   | The symbol GnuCash shows amounts with (below)                                               |
| `usage`           | `{ accounts, transactions, prices, business }`: accounts holding it (GnuCash's usage count), transactions with it as currency, prices with it on either side, customers, vendors, employees and invoices with it as currency |

**Currency quote flags.** Loading a book runs `gnc_commodity_increment_usage_count` for each account, which turns
quotes on (source `currency`) for a currency an account uses unless its `auto_quote_control` slot is `"false"`
(`5.17:libgnucash/engine/gnc-commodity.cpp:1402-1425`). `quoteFlag` reports that result, so it matches GnuCash even for
a currency the file does not define. `ops::commodities::account_commodity_changed(book, old, new)` applies the same
rule (and the matching decrement, `:1428-1450`) when an account's commodity changes, so that red-ale writes the
`<gnc:commodity>` GnuCash would write for it.

## The security editor

### `commodity.create` (write)

| Param         | Type    | Default | Meaning                                         |
|---------------|---------|---------|-------------------------------------------------|
| `namespace`   | string  |         | Exchange or type, such as `NASDAQ`              |
| `mnemonic`    | string  |         | Ticker                                          |
| `fullname`    | string  |         | Required                                        |
| `cusip`       | string? |         | Exchange code (CUSIP, ISIN)                     |
| `fraction`    | integer | 10000   | 1 to 1000000000 (`GNC_COMMODITY_MAX_FRACTION`)  |
| `quoteFlag`   | bool    | false   |                                                 |
| `quoteSource` | string? |         | Internal name, such as `yahoo_json`; any name is kept |
| `quoteTz`     | string? |         | Any zone name; absent is local time             |
| `userSymbol`  | string? |         |                                                 |

Returns `CommodityInfo`. The checks are the editor's (`gnc_ui_commodity_dialog_to_object`,
`5.17:gnucash/gnome-utils/dialog-commodity.cpp:1231-1370`):

- `invalid`: namespace `CURRENCY`/`ISO4217` ("You may not create a new national currency", `:1275`); namespace
  `template` (`:1286`); empty full name, namespace or mnemonic (`:1294`); fraction out of range; quote source
  `currency`, which the editor offers only to currencies.
- `conflict`: the namespace and mnemonic already exist (`:1302`).

Namespace, mnemonic, full name, exchange code and time zone are trimmed, because GnuCash's XML reader strips them
(`5.17:libgnucash/backend/xml/gnc-commodity-xml-v2.cpp:174-188`) and an untrimmed value would not read back the same.

### `commodity.update` (write)

`{ commodity, namespace?, mnemonic?, fullname?, cusip?, fraction?, quoteFlag?, quoteSource?, quoteTz?, userSymbol? }`
→ `CommodityInfo`. Absent fields are kept; `null` clears `cusip`, `quoteSource`, `quoteTz` (local time) or
`userSymbol`.

- **A currency** accepts only `quoteFlag`, `quoteTz` and `userSymbol` (`dialog-commodity.cpp:1250-1272`); anything else
  is `invalid`. Setting the quote flag follows `gnc_commodity_user_set_quote_flag`
  (`5.17:libgnucash/engine/gnc-commodity.cpp:1256-1283`): a choice that differs from the automatic one (on exactly when
  an account uses the currency) is remembered as `auto_quote_control = "false"`, a matching one removes the slot.
  Turning quotes off clears the time zone, as the editor does (`dialog-commodity.cpp:1263-1264`).
- **A security** accepts every field, as the editor does. Changing `namespace` or `mnemonic` renames it: every
  account, transaction and price that names it is changed with it (GnuCash's entities hold the commodity object, so a
  rename reaches them all). A rename is `conflict` if the new name exists, if a customer, vendor, employee or invoice
  has it as currency, or if template transactions, scheduled transactions or budgets (kept as read, not modelled)
  refer to it.
- **Fraction.** GnuCash lets the fraction of a commodity in use change, with no check
  (`5.17:libgnucash/engine/gnc-commodity.cpp:1219`). Stored amounts keep their denominators.
- **User symbol**: empty, or equal to a currency's built-in symbol, removes the slot (`gnc_commodity_set_user_symbol`,
  `:1355-1380`). GnuCash also removes the locale's own currency symbol given for the locale's currency, but it tests
  the mnemonic against `int_curr_symbol`, which always ends in a space (`USD `; GnuCash's own fallback is `"USD "`,
  `5.17:libgnucash/core-utils/gnc-locale-utils.c:89`, and see its comment at `:124-127`), so that test never matches
  and red-ale's rule is GnuCash's in effect.
- The template commodity cannot be edited.

### `commodity.delete` (destructive)

`{ commodity, deletePrices? }` → `{ deletedPrices: Guid[] }`.

GnuCash's commodities dialog refuses only while an account uses the commodity, and deletes the commodity's own prices
with it (`5.17:gnucash/gnome/dialog-commodities.cpp:160-246`). red-ale refuses:

- a currency or the template commodity (`invalid`; the dialog disables removal, `:345`);
- a commodity any account, transaction (as currency) or business object uses (`conflict`);
- a commodity with prices on either side unless `deletePrices` is true (`conflict`); with it, those prices go too;
- a commodity template transactions, scheduled transactions or budgets refer to (`conflict`).

## Display symbol

The Display symbol is what GnuCash prints amounts with (`gnc_commodity_get_nice_symbol`,
`5.17:libgnucash/engine/gnc-commodity.cpp:1094-1115`, used by `xaccPrintAmount`,
`5.17:libgnucash/app-utils/gnc-ui-util.cpp:1341-1349`): the User symbol if not empty, else the Default symbol if not
empty, else the mnemonic. Between the first two GnuCash tries the locale's currency symbol when the mnemonic equals
the locale's `int_curr_symbol`; for the reason above that never matches, so the result does not depend on the locale.
`ops::commodities::display_symbol` gives it to the report renderer, and `CommodityInfo.displaySymbol` to clients.
GnuCash prints a security's symbol after the amount and a currency's before it (`gnc-ui-util.cpp:1344-1348`); where
it goes is the renderer's business.

## Currencies

### `currency.list` (read)

No params. Returns the 227 built-in currencies in ISO code order:
`[{ id, code, fullname, exchangeCode, fraction, symbol, euroRate }]`. `euroRate` (units per euro) is set for the euro
and the 21 currencies with a fixed rate in `5.17:libgnucash/engine/gnc-euro.cpp:32-55`, BGN included.

### `currency.euroConvert` (read)

`{ amount: Amount, from: CommodityId, to: CommodityId }` → `{ amount, rate }`. One side must be `EUR`.

- To euros: `amount / rate`, rounded half away from zero to cents (`gnc_convert_to_euro`, `gnc-euro.cpp:80-92`).
- From euros: `amount × rate`, rounded half away from zero to the currency's fraction (`gnc_convert_from_euro`,
  `:96-106`).

## Deviations from GnuCash

| What | GnuCash 5.17 | red-ale | Why |
|------|--------------|---------|-----|
| Deleting a commodity in use as a transaction currency, by a business object, or with prices | Allowed; prices where it is the currency are left dangling (`dialog-commodities.cpp:183`, FIXME) | Refused; prices only with `deletePrices` | A dangling reference breaks the book |
| Namespace `GNC_LEGACY_CURRENCIES` or containing `::` for a new security | Allowed | Refused | The first is a currency namespace to GnuCash (`gnc-commodity.cpp:1953`); `::` would make `NAMESPACE::MNEMONIC` ambiguous |
| A price-less commodity with no account but used by template transactions | Deletable | Refused | Template transactions are kept as read and would point at nothing |
| Converting with a currency that has no euro rate | Returns 0 | `invalid` | A zero is a silent wrong answer |
| Quote source or time zone of a commodity without quotes | Kept in memory until the book is closed | Dropped | GnuCash does not write them (`gnc-commodity-xml-v2.cpp:97-108`), so they would not survive a save anyway |
| Accounts of a commodity whose fraction changes | Keep the old SCU stored; after a reload they become non-standard at the old SCU (`Account.cpp:2710-2722`) | Accounts that followed the commodity's fraction store the new one | The book reads back as GnuCash showed it before the reload |
| The editor's user-symbol field | Shows the Display symbol and stores it back | Changed only when given | The editor's display value is not the user's choice |
| Quote time zone outside the editor's six-entry menu | Written back as local time when the dialog is saved | Kept | Never lose data |
