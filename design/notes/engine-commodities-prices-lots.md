# Engine rules: commodities, prices, lots and capital gains (GnuCash 5.17)

Scope: the milestone-1 features of §3 "Commodities, prices and investments" in
[`milestone1.md`](../milestone1.md): commodity table, ISO 4217 currencies, security editor, euro legacy currencies,
price database, price editor, remove old prices, quote source and time zone, lots, lot viewer, capital gains, stock
split assistant.

Citations are `5.17:path:line` in the GnuCash checkout at tag 5.17 (commit 0e302675ff). "Oracle" means an experiment
run against the GnuCash 5.17 Python bindings (scripts in the scratchpad, not committed); XML excerpts below are verbatim
output of those runs, with GUIDs shortened only where marked. *docs-only* marks anything not checked against code.

GUI dialogs are listed because the features are named after them. red-ale exposes their *engine* effects as API
operations; GUI-only side effects are called out so they are not copied by accident.

---

## 1. Commodity

### 1.1 Fields

| Field | Type / default | Persisted as (XML) | Notes |
|---|---|---|---|
| namespace | pointer to a namespace object, by name | `cmdty:space` | `ISO4217` is mapped to `CURRENCY` on every lookup/insert (§1.2). |
| mnemonic | string, default `""` | `cmdty:id` | The ticker / ISO code. Case-sensitive (hash is `g_str_hash`). |
| fullname | string, default `""` | `cmdty:name` | Not written for ISO currencies. |
| cusip ("exchange code", xcode) | string, default `""` | `cmdty:xcode`, only if non-empty | ISIN/CUSIP etc. For ISO currencies the built-in table sets it to the numeric ISO code (`"840"` for USD). |
| fraction (SCU) | int, default **10000** | `cmdty:fraction` | Not written for ISO currencies. GUI range 1..10^9. |
| quote_flag | bool, default false | presence of empty `<cmdty:get_quotes/>` | |
| quote_source | pointer into the quote-source lists, default null | `cmdty:quote_source` (internal name), only when quote_flag | ISO currencies report source `currency` when null. |
| quote_tz | string, default `""` | `cmdty:quote_tz`, only when quote_flag (written even when empty: `<cmdty:quote_tz/>`) | |
| `user_symbol` slot | KVP string at path `user_symbol` | in `cmdty:slots` | Display symbol override. |
| `auto_quote_control` slot | KVP string, only ever `"false"` | in `cmdty:slots` | Present = user overrode the automatic quote flag of a currency (§1.6). |
| default_symbol | string, **not persisted** | — | `local-symbol` from the ISO table; runtime only. |
| usage_count | int, **not persisted** | — | Number of accounts using the commodity. |
| printname, unique_name | derived | — | `"<mnemonic> (<fullname>)"` and `"<namespace>::<mnemonic>"`. |

Sources: struct `5.17:libgnucash/engine/gnc-commodity.cpp:75`; defaults `5.17:libgnucash/engine/gnc-commodity.cpp:563`
(fraction 10000) and lines 556-570; printname/unique name `5.17:libgnucash/engine/gnc-commodity.cpp:529`,
`5.17:libgnucash/engine/gnc-commodity.cpp:538`; user symbol slot `5.17:libgnucash/engine/gnc-commodity.cpp:1355`;
auto quote control slot `5.17:libgnucash/engine/gnc-commodity.cpp:1015`, `:1233`; XML writer
`5.17:libgnucash/backend/xml/gnc-commodity-xml-v2.cpp:57-112`.

XML child order written by 5.17 (oracle, also `gnc-commodity-xml-v2.cpp:74-111`): `cmdty:space`, `cmdty:id`,
`cmdty:name`, `cmdty:xcode`, `cmdty:fraction`, `cmdty:get_quotes`, `cmdty:quote_source`, `cmdty:quote_tz`,
`cmdty:slots`. On re-save GnuCash normalises any other child order to this one (oracle: a hand-edited file with
`get_quotes` before `fraction` came back with `fraction` first).

```xml
<gnc:commodity version="2.0.0">
  <cmdty:space>NASDAQ</cmdty:space>
  <cmdty:id>ACME</cmdty:id>
  <cmdty:name>Acme Corp</cmdty:name>
  <cmdty:xcode>US0000001</cmdty:xcode>
  <cmdty:fraction>1000</cmdty:fraction>
</gnc:commodity>
```

**ISO currencies are written only if** `quote_flag` is set or the commodity has slots
(`5.17:libgnucash/backend/xml/gnc-commodity-xml-v2.cpp:68`); then only space, id, quote elements and slots. On read, a
`CURRENCY`/`ISO4217` element starts from a copy of the built-in currency and overlays the file's children
(`5.17:libgnucash/backend/xml/gnc-commodity-xml-v2.cpp:213`, `:274-276`). A read commodity is rejected if namespace or
mnemonic is null or fraction is 0 (`5.17:libgnucash/backend/xml/gnc-commodity-xml-v2.cpp:192`).

Namespaces are written sorted, commodities sorted by id within each (`5.17:libgnucash/backend/xml/io-gncxml-v2.cpp:1037`).
The XML backend note owns the details.

### 1.2 Namespaces

| Constant | Value | Meaning |
|---|---|---|
| `GNC_COMMODITY_NS_CURRENCY` | `CURRENCY` | ISO 4217 currencies. The only namespace with the `iso4217` flag set. |
| `GNC_COMMODITY_NS_ISO` | `ISO4217` | Deprecated alias; mapped to `CURRENCY` by `gnc_commodity_table_map_namespace` on every lookup, insert and namespace creation. |
| `GNC_COMMODITY_NS_LEGACY` | `GNC_LEGACY_CURRENCIES` | File-I/O-only legacy name. `gnc_commodity_is_currency` is true for it and for `CURRENCY`. |
| `GNC_COMMODITY_NS_TEMPLATE` | `template` | Reserved for scheduled-transaction templates; holds exactly one commodity `template`. |
| `GNC_COMMODITY_NS_NONCURRENCY` | `NONCURRENCY` | Not a stored namespace. |
| GUI pseudo-names | `All non-currency`, `Currencies` | Filters only. `Currencies` is the GUI name of `CURRENCY`. |

Sources: `5.17:libgnucash/engine/gnc-commodity.h:103-111`; mapping `5.17:libgnucash/engine/gnc-commodity.cpp:1557`;
`is_iso` `5.17:libgnucash/engine/gnc-commodity.cpp:1550`, `:1941`; `is_currency` `:1953`; namespace creation
`:2106` (sets `iso4217 = is_iso(name)`, so only `CURRENCY` ever carries it).

Glossary flag: in GnuCash, *currency* (`is_iso`) means "in namespace `CURRENCY`", nothing else.

Rules:

- Template namespace: `gnc_commodity_new` and `gnc_commodity_table_insert` silently move any commodity other than
  `template` from namespace `template` to namespace **`User`** (`5.17:libgnucash/engine/gnc-commodity.cpp:790`,
  `:1797-1805`). The security editor refuses namespace `template` outright
  (`5.17:gnucash/gnome-utils/dialog-commodity.cpp:1286`).
- Every new book gets namespace `template` with commodity `template`/`template`, fullname `template`, cusip
  `template`, fraction 1, then all ISO currencies (`5.17:libgnucash/engine/gnc-commodity.cpp:2317-2327`). The
  `template` commodity is written to XML (oracle):

  ```xml
  <gnc:commodity version="2.0.0">
    <cmdty:space>template</cmdty:space>
    <cmdty:id>template</cmdty:id>
    <cmdty:name>template</cmdty:name>
    <cmdty:xcode>template</cmdty:xcode>
    <cmdty:fraction>1</cmdty:fraction>
  </gnc:commodity>
  ```

- Rename namespace (`5.17:libgnucash/engine/gnc-commodity.cpp:2135`): refused if old == new, new is `Currencies` (or its
  translation), or new already exists. The security editor disables rename for `CURRENCY` and `GNC_LEGACY_CURRENCIES`
  (`5.17:gnucash/gnome/dialog-commodities.cpp:357`).
- Namespaces exist only through their commodities in XML (there is no namespace element); an empty namespace does not
  survive a save. *(Inferred from the writer iterating commodities; not separately tested.)*

### 1.3 Uniqueness and identity

- Key: `(namespace, mnemonic)`, exact byte comparison after the `ISO4217`→`CURRENCY` mapping.
  `gnc_commodity_equiv` compares namespace pointer and mnemonic (`5.17:libgnucash/engine/gnc-commodity.cpp:1480`).
- `gnc_commodity_table_insert` of a commodity whose key already exists does **not** fail: it copies the new object's
  fields (fullname, mnemonic, namespace, fraction, cusip, quote flag/source/tz, all KVP) **onto the existing one**,
  destroys the new object and returns the existing one (`5.17:libgnucash/engine/gnc-commodity.cpp:1754-1795`,
  copy at `:876`). The loader relies on this to overlay file data on built-in currencies.
- Renamed ISO codes: lookup in `CURRENCY` maps `RUR→RUB`, `PLZ→PLN`, `UAG→UAH`, `NIS→ILS`, `MXP→MXN`, `TRL→TRY`
  (`5.17:libgnucash/engine/gnc-commodity.cpp:131-145`, applied at `:1650` and `:1782`). Oracle:
  `lookup("CURRENCY","RUR")` returns `CURRENCY::RUB`; `lookup("ISO4217","USD")` returns `CURRENCY::USD`.
- `gnc_commodity_compare` orders by unique name, then GUID (`5.17:libgnucash/engine/gnc-commodity.cpp:1502`).

### 1.4 Validation (security editor = `gnc_ui_commodity_dialog_to_object`, `5.17:gnucash/gnome-utils/dialog-commodity.cpp:1231`)

| Rule | Source |
|---|---|
| A new commodity in an ISO namespace is refused ("You may not create a new national currency"). | `:1275` |
| For an existing currency only quote flag, quote tz and user symbol can be edited. | `:1250-1272` |
| Namespace `template` refused. | `:1286` |
| fullname, namespace and mnemonic must all be non-empty. | `:1294`, `:1353` |
| `(namespace, mnemonic)` must not belong to another commodity. | `:1302` |
| fraction from a spin button, 1..1 000 000 000, default 10000; any integer in range (not restricted to powers of 10). | `5.17:gnucash/gtkbuilder/dialog-commodity.glade:5-9`; max `GNC_COMMODITY_MAX_FRACTION` `5.17:libgnucash/engine/gnc-commodity.h:116` |
| quote tz from a fixed menu: "Use local time" (stored as null) or one of `Asia/Tokyo`, `Australia/Sydney`, `America/New_York`, `America/Chicago`, `Europe/London`, `Europe/Paris`. | `5.17:gnucash/gnome-utils/dialog-commodity.cpp:805-844` |

The engine itself validates nothing (`gnc_commodity_set_fraction` accepts any int,
`5.17:libgnucash/engine/gnc-commodity.cpp:1219`). red-ale should apply the editor rules in its use cases.

GUI quirk not to copy: editing a commodity whose stored tz is not in the six-entry menu writes back null (menu
position 0); the API must keep any tz string.

User symbol setter (`5.17:libgnucash/engine/gnc-commodity.cpp:1355`): empty → slot removed; equal to the locale's
currency symbol for the locale's own currency → removed; equal to the built-in `default_symbol` → removed; otherwise
stored at `user_symbol`. The locale part is environment-dependent; red-ale should implement only "empty or equal to the
default symbol → remove".

### 1.5 Deleting a commodity

- Engine (`gnc_commodity_destroy`, `5.17:libgnucash/engine/gnc-commodity.cpp:868`) checks nothing.
- Security editor (`5.17:gnucash/gnome/dialog-commodities.cpp:160-246`):
  1. Remove is disabled for ISO currencies (`:345`).
  2. Refused if any account in the book has this commodity, listing the accounts (`:176-198`).
  3. "FIXME check for transaction references" (`:183`): transactions using it as currency are **not** checked.
  4. Prices with the commodity on the *commodity* side (`gnc_pricedb_get_prices(pdb, commodity, NULL)`, forward only)
     are listed in the confirmation and removed together with the commodity (`:203`, `:238`). Prices where it is the
     *currency* side are not found and are left dangling.
- Recommendation for red-ale: refuse while any account, transaction currency, price (either side), or
  scheduled-transaction template refers to it; delete the commodity's own prices only on explicit request. This is
  stricter than GnuCash, so it cannot break a book.

### 1.6 Currency quote flag automation (affects round-trip)

`gnc_commodity_increment_usage_count` (`5.17:libgnucash/engine/gnc-commodity.cpp:1402`) runs whenever an account's
commodity is set (`5.17:libgnucash/engine/Account.cpp:1319`, `:2682`), **including while a file is loaded**:

```
on account starts using commodity c:
    if c.usage_count == 0 and !c.quote_flag and auto_quote_control(c) and is_iso(c):
        c.quote_flag = true
        c.quote_source = "currency"
    c.usage_count += 1
on account stops using c (decrement, :1437):
    c.usage_count -= 1
    if c.usage_count == 0 and c.quote_flag and auto_quote_control(c) and is_iso(c):
        c.quote_flag = false
auto_quote_control(c) = slot "auto_quote_control" absent or != "false"
```

`gnc_commodity_user_set_quote_flag` (`:1256`), used by the editor, sets the flag and, for currencies, writes
`auto_quote_control = "false"` when the user's choice differs from the automatic default, removing the slot when it
matches.

Consequence (oracle): every currency used by an account is written as

```xml
<gnc:commodity version="2.0.0">
  <cmdty:space>CURRENCY</cmdty:space>
  <cmdty:id>USD</cmdty:id>
  <cmdty:get_quotes/>
  <cmdty:quote_source>currency</cmdty:quote_source>
  <cmdty:quote_tz/>
</gnc:commodity>
```

and deleting that element from a file makes GnuCash add it back on the next save. red-ale must reproduce this to
match GnuCash's output when it creates accounts.

### 1.7 Built-in ISO 4217 table

- Source data: **`libgnucash/engine/iso-4217-currencies.xml`** (59 170 bytes, sha256
  `fb183371393209b4281f39c09df4288216635c3c84dafe43464384e762315f54` at 5.17). Converted at build time by
  `iso-currencies-to-c.xsl` into C that is `#include`d into `gnc_commodity_table_add_default_data`
  (`5.17:libgnucash/engine/CMakeLists.txt:129-135`, `5.17:libgnucash/engine/gnc-commodity.cpp:2327`). Loaded into
  every new book at `book_begin` (`5.17:libgnucash/engine/gnc-commodity.cpp:2381`), then the commodity collection is
  marked clean.
- **227 `<currency>` entries** (oracle: `get_commodities("CURRENCY")` returns 227). The file header claims "sorted by
  ISO code".
- Format: root `<currencylist>`; each entry is an empty element with exactly these attributes in this order:

  | Attribute | Used? | Meaning |
  |---|---|---|
  | `isocode` | yes → mnemonic | 3-letter code |
  | `fullname` | yes → fullname, passed through `dgettext("iso_4217", …)` (**translated at runtime**) | |
  | `unitname`, `partname` | no | |
  | `namespace` | yes | always `ISO4217` in 5.17 (→ `CURRENCY`) |
  | `exchange-code` | yes → cusip | ISO numeric code, zero-padded string (`"008"`) |
  | `parts-per-unit` | no | differs from `smallest-fraction` for 21 codes, e.g. JPY 100 vs 1 |
  | `smallest-fraction` | yes → fraction | 1, 5, 100, 500, 1000, 10000 or 1000000 |
  | `local-symbol` | yes → default_symbol (runtime only) | may be `""` |

  Generated call per entry (`5.17:libgnucash/engine/iso-currencies-to-c.xsl:41-55`):
  `gnc_commodity_new(book, CUR_I18N(fullname), namespace, isocode, exchange-code, smallest-fraction)`, then
  `gnc_commodity_table_insert`, then `gnc_commodity_set_default_symbol(c, local-symbol)`.

  ```xml
  <!-- "KRW" - "Won"
  -->
  <currency
    isocode="KRW"
    fullname="Won"
    unitname="won"
    partname="chon"
    namespace="ISO4217"
    exchange-code="410"
    parts-per-unit="1"
    smallest-fraction="1"
    local-symbol="₩"
  />
  ```

- Fraction distribution (oracle): 100 → 197 codes; 1 → 14; 1000 → 7; 1000000 → 6 (XAG, XAU, XPD, XPT, XTS, XXX);
  10000 → CLF; 500 → MGF; 5 → MGA.
- **KRW**: `smallest-fraction` changed from 100 to **1** by commit 1fda575c5b "Bug 666536 - smallest fraction of KRW is
  invalid" (in 5.17). Oracle: KRW fraction 1. Existing books keep KRW amounts with whatever denominator they were
  written with; only the commodity default changes (KRW is not written to XML, so the table value is what GnuCash uses).
- **BGN**: still a normal entry (fraction 100, `exchange-code` 975). Its "retirement" in 5.17 (commit 6907d3648a) is
  only (a) a changed XML comment ("Pegged 1.95583 to 1 EUR 1999-2026 / Admitted to Euro-zone 1 Jan 2026") and (b) BGN
  added to the euro table (§3).
- **Retired ("historic") currencies are not marked in any machine-readable way.** There is no attribute; the only
  information is a free-text XML comment before the entry, in the informal form `<date> "<new code>" <ratio>`, e.g.

  ```xml
  <!-- "FIM" - "Finnish Markka"
    2002-01-01 "EUR" 5.94573
  -->
  ```

  56 comments mention a date or replacement; some are about *current* currencies (e.g. ZMW "replaced ZMK", CLF, VES).
  GnuCash treats all 227 entries identically. red-ale's embedded table should carry exactly the six used attributes
  plus, if wanted, a hand-curated `retired` flag kept outside the GnuCash-compatible data.

### 1.8 `gnc_commodity_get_nice_symbol` (display only)

user_symbol if set; else the locale's currency symbol if the mnemonic is the locale's `int_curr_symbol`; else
default_symbol if non-empty; else mnemonic (`5.17:libgnucash/engine/gnc-commodity.cpp:1095`).

---

## 2. Quote sources

A quote source has `{supported: bool, type, user_name, internal_name}`; the commodity stores a pointer and XML stores
`internal_name` (`5.17:libgnucash/engine/gnc-commodity.cpp:148-170`).

Types (`5.17:libgnucash/engine/gnc-commodity.h:128-143`): `SOURCE_SINGLE = 0`, `SOURCE_MULTI = 1`,
`SOURCE_UNKNOWN = 2`, `SOURCE_MAX = 3`, `SOURCE_CURRENCY = SOURCE_MAX = 3`.

Built-in lists, in this order (the order is the editor's menu order and the `index` used by
`gnc_quote_source_lookup_by_ti`):

| Type | Internal names (`5.17:libgnucash/engine/gnc-commodity.cpp`) |
|---|---|
| CURRENCY (`:180`) | `currency` (user name "Currency"; the only one with `supported = true` at start) |
| SINGLE (`:189-233`, 42) | `alphavantage`, `amfiindia`, `asegr`, `asx`, `borsa_italiana`, `bseindia`, `bvb`, `cse`, `comdirect`, `consorsbank`, `deka`, `ftfunds`, `finanzpartner`, `finnhub`, `fondsweb`, `goldmoney`, `googleweb`, `marketwatch`, `morningstarjp`, `fool`, `nzx`, `nseindia`, `onvista`, `bourso`, `sinvestor`, `za`, `six`, `stockdata`, `stooq`, `swissfunddata`, `tesouro_direto`, `tsx`, `tradegate`, `treasurydirect`, `twelvedata`, `unionfunds`, `usbonds`, `tsp`, `xetra`, `yahoo_json`, `yahooweb`, `financeapi` |
| MULTI (`:236-245`, 7) | `canada`, `europe`, `india`, `nasdaq`, `nyse`, `ukfunds`, `usa` |
| UNKNOWN (`:247`) | starts empty |

User names are translatable display strings (context "FQ Source"); only internal names matter for storage.

Unknown sources:

- XML read: `gnc_quote_source_lookup_by_internal(name)`; if not found, `gnc_quote_source_add_new(name, FALSE)` appends
  `{supported=false, SOURCE_UNKNOWN, name, name}` to the UNKNOWN list
  (`5.17:libgnucash/backend/xml/gnc-commodity-xml-v2.cpp:153-162`, `5.17:libgnucash/engine/gnc-commodity.cpp:317`).
  Lookup searches CURRENCY, SINGLE, MULTI, UNKNOWN in that order, exact string compare (`:350`).
- Oracle: `<cmdty:quote_source>my_private_src</cmdty:quote_source>` and `<cmdty:quote_tz>Mars/Olympus</cmdty:quote_tz>`
  are read as type 2 (UNKNOWN), unsupported, and written back unchanged. **Unknown sources and tzs round-trip.**
- Finance::Quote discovery marks known sources supported and adds new ones as UNKNOWN/supported
  (`gnc_quote_source_set_fq_installed`, `:456`). Out of scope for v0.1 (no fetching).
- Defaults: ISO currency with no source reports `currency` (`:1038`); `gnc_commodity_get_default_quote_source` gives
  `currency` for currencies, else `alphavantage` (`:1050`). Setting an ISO namespace also sets source `currency`
  (`:775-796`, `:1145-1163`).
- The quote source list is process-global in GnuCash (static lists); red-ale should keep the source as a plain string
  on the commodity and derive type by lookup in the built-in table, `UNKNOWN` otherwise.

---

## 3. Euro legacy currencies (`5.17:libgnucash/engine/gnc-euro.cpp`)

Rates are "units of the legacy currency per 1 EUR", stored as unreduced rationals (`:32-55`, oracle confirms
`gnc_euro_currency_get_rate`):

| Code | Rate | Code | Rate | Code | Rate |
|---|---|---|---|---|---|
| ATS | 137603/10000 | FIM | 594573/100000 | LVL | 702804/1000000 |
| BEF | 403399/10000 | FRF | 655957/100000 | MTL | 429300/1000000 |
| BGN | 195583/100000 | GRD | 340750/1000 | NLG | 220371/100000 |
| CYP | 585274/1000000 | HRK | 753450/100000 | PTE | 200482/1000 |
| DEM | 195583/100000 | IEP | 787564/1000000 | SIT | 239640/1000 |
| EEK | 156466/10000 | ITL | 193627/100 | SKK | 301260/10000 |
| ESP | 166386/1000 | LUF | 403399/10000 | **EUR** | **1/1** |

(Note: the ISO XML comment for HRK says 7.53400; the euro table's 7.53450 is the one used.)

Functions:

```
rate(c) = table[c.mnemonic] if is_iso(c) and present, else none      (:57)
gnc_is_euro_currency(c)   = rate(c) exists       // true for EUR itself   (:72)
gnc_convert_to_euro(c, v) = none → 0/1;  else round(v / rate, denom 100, ROUND_HALF_UP)        (:80-92)
gnc_convert_from_euro(c, v) = none → 0/1; else round(v * rate, denom fraction(c), ROUND_HALF_UP)  (:96-106)
gnc_euro_currency_get_rate(c) = rate or 0/1      (:110)
```

`ROUND_HALF_UP` in GnuCash means round half **away from zero** (`5.17:libgnucash/engine/gnc-rational-rounding.hpp:173-195`),
per EC Regulation 1103/97 (comment at `gnc-euro.cpp:88`). Division and multiplication are exact rationals (128-bit
intermediate) before the single rounding. Oracle: 100.00 DEM → `5113/100` EUR; 1.00 EUR → `1936/1` ITL (ITL fraction
1); -0.009779 DEM → `0/100`.

The rate table is used only by GUI helpers (transfer dialog `5.17:gnucash/gnome-utils/dialog-transfer.cpp`, register
`5.17:gnucash/gnome/gnc-split-reg.c`) to show or convert euro amounts; it never writes to the book.

---

## 4. Price database

### 4.1 Price fields

| Field | Type | XML (`gnc-pricedb-xml-v2.cpp`) | Notes |
|---|---|---|---|
| guid | GUID | `price:id` | |
| commodity | commodity ref | `price:commodity` | the thing priced |
| currency | commodity ref | `price:currency` | the unit of `value`; may be any commodity, not only ISO |
| time | time64 | `price:time` (`ts:date`) | |
| source | enum `PriceSource` | `price:source` (string), omitted only if the string is empty, which never happens | §4.2 |
| type | free string, null ≡ `""` | `price:type`, omitted if null/empty | §4.3 |
| value | gnc_numeric, default `0/1` | `price:value` | kept exactly as set; no reduction or SCU rounding |

Element order written: `id, commodity, currency, time, source, type, value`
(`5.17:libgnucash/backend/xml/gnc-pricedb-xml-v2.cpp:400-450`). Prices have no KVP slots.

### 4.2 Source: an enum, not a string — lossy

`PriceSource` (`5.17:libgnucash/engine/gnc-pricedb.h:169-182`) with strings (`5.17:libgnucash/engine/gnc-pricedb.cpp:105-120`).
**The enum order is the precedence order: lower value wins.**

| Value | Enum | String |
|---|---|---|
| 0 | `PRICE_SOURCE_EDIT_DLG` | `user:price-editor` |
| 1 | `PRICE_SOURCE_FQ` | `Finance::Quote` |
| 2 | `PRICE_SOURCE_USER_PRICE` | `user:price` |
| 3 | `PRICE_SOURCE_XFER_DLG_VAL` | `user:xfer-dialog` |
| 4 | `PRICE_SOURCE_SPLIT_REG` | `user:split-register` |
| 5 | `PRICE_SOURCE_SPLIT_IMPORT` | `user:split-import` |
| 6 | `PRICE_SOURCE_STOCK_SPLIT` | `user:stock-split` |
| 7 | `PRICE_SOURCE_STOCK_TRANSACTION` | `user:stock-transaction` |
| 8 | `PRICE_SOURCE_INVOICE` | `user:invoice-post` |
| 9 | `PRICE_SOURCE_TEMP` | `temporary` |
| 10 | `PRICE_SOURCE_INVALID` | `invalid` |

- A new price starts with source `INVALID` (`5.17:libgnucash/engine/gnc-pricedb.cpp:90-96`).
- `gnc_price_set_source_string` matches only the strings above (exact `strcmp`); **any other string leaves the source
  unchanged** (`:530-542`).
- Therefore, on load: an unknown source string (e.g. old `user:foo`, `old-file-import`) or a missing `price:source`
  element yields `INVALID`, and **GnuCash writes `<price:source>invalid</price:source>` on the next save** (oracle,
  diff after open+save of a hand-edited file):

  ```
  <     <price:source>user:foo</price:source>
  ---
  >     <price:source>invalid</price:source>
  91a92
  >     <price:source>invalid</price:source>          (element was absent)
  <     <price:source>old-file-import</price:source>
  ---
  >     <price:source>invalid</price:source>
  ```

  red-ale must keep the source as read (the "never lose data" rule), store it as a string, and compute precedence by
  mapping known strings to the enum and everything else to 10. Writing the original string back is safe: GnuCash reads
  it as `invalid` exactly as it would have anyway.

### 4.3 Type

Free string. Known values: `last` (`PRICE_TYPE_LAST`), `unknown` (`PRICE_TYPE_UNK`), `transaction` (`PRICE_TYPE_TRN`)
(`5.17:libgnucash/engine/gnc-pricedb.h:184-186`), plus `bid`, `ask`, `nav` offered by the price editor
(`5.17:gnucash/gnome/dialog-price-editor.c:121-136`; any other stored value shows as "unknown" there but is not
rewritten unless the user saves the dialog). Oracle: type `weird` round-trips unchanged.

### 4.4 Storage and ordering

- Two-level map `commodity → currency → list` (`5.17:libgnucash/engine/gnc-pricedb.cpp:771-781`); each list sorted by
  `compare_prices_by_date`: **time descending (newest first), ties by GUID ascending** (`:674-692`).
- XML write order (`gnc_pricedb_foreach_price(..., stable_order = TRUE)`,
  `5.17:libgnucash/backend/xml/gnc-pricedb-xml-v2.cpp:484`; `5.17:libgnucash/engine/gnc-pricedb.cpp:2688-2729`):
  commodities sorted by (namespace, mnemonic) with `strcmp`, then currencies the same way, then each list newest
  first. Oracle order: `CURRENCY::EUR` prices, `CURRENCY::USD` prices, `NASDAQ::ACME` prices (newest first).
- During XML load the db is in bulk-update mode: **no same-day replacement and no duplicate check**
  (`5.17:libgnucash/backend/xml/gnc-pricedb-xml-v2.cpp:260`, `:337`). Every price in the file is kept.

### 4.5 Adding a price (`add_price`, `5.17:libgnucash/engine/gnc-pricedb.cpp:1013-1110`)

```
add_price(db, p):
    require p.commodity and p.currency, same book
    if not db.bulk_update:
        old = lookup_day(db, p.commodity, p.currency, p.time)      // BOTH directions, see below
        if old:
            if p.source > old.source:  reject (return FALSE)        // existing has higher precedence
            remove_price(db, old)                                   // equal or better precedence replaces
    list = db[p.commodity][p.currency]
    if not bulk and exists q in list with same canonical day, value == (gnc_numeric_compare), same commodity,
           same currency:                                          // price_is_duplicate, :696
        return TRUE without inserting                              // silently dropped
    insert p sorted (time desc, guid asc)
```

- "Same day" = `time64CanonicalDayTime(t)`: local-time date at 12:00 (`5.17:libgnucash/engine/gnc-date.cpp:404`), so it
  depends on the process time zone.
- `lookup_day` uses `pricedb_get_prices_internal(..., bidi = TRUE)` (`:2180-2200`, `:1686`): it also sees prices in the
  reverse direction. **A same-day price of USD in EUR replaces (or is blocked by) a price of EUR in USD.** Oracle:
  adding `USD/EUR user:price-editor` on 2024-01-10 removed an existing `EUR/USD user:xfer-dialog` on that day.
- When both directions exist on one day only one of them is considered (the one `lookup_nearest_in_time` with
  `sameday` picks: the nearest in time, ties to the older).
- Oracle summary (all on one day unless noted): editor then FQ → FQ rejected; editor then editor → replaced;
  FQ then `user:price` → rejected; `user:stock-split` then FQ with equal value later same day → FQ replaces.
- `gnc_price_set_commodity/currency/time64` on a price that is in a db removes and re-adds it, so they re-run these
  rules (`:457-518`).

### 4.6 Lookups

All return newest-first semantics from the sorted lists.

| Function | Rule | Source |
|---|---|---|
| `gnc_pricedb_get_prices(c, cur or NULL)` | forward direction only; `NULL` currency merges all currencies of `c`, newest first | `:2047` |
| `gnc_pricedb_lookup_latest(c, cur)` | first of the merged forward+reverse list | `:1731` |
| `gnc_pricedb_lookup_day_t64(c, cur, t)` | bidirectional; nearest price whose canonical day equals t's | `:2171` |
| `gnc_pricedb_lookup_nearest_in_time64(c, cur, t)` | bidirectional; of the newest price `<= t` and the oldest price `> t`, the one with smaller `|Δt|`; **tie → the older** | `:2180-2288` |
| `gnc_pricedb_lookup_nearest_before_t64(c, cur, t)` | bidirectional; newest price with time `<= t` | `:2305` |
| `…_any_currency_t64` variants | per other commodity, the newest `<= t` (`latest_before`) or the nearest (`nearest_to`); list sorted newest first | `:1826`, `:1919`, `:1955-2000` |
| `get_nearest_price` / `convert_balance_*` | same commodity → 1/1; direct price (inverted if found in reverse); else one-hop via a shared third commodity; result `gnc_numeric_reduce`d; conversion multiplies amount by price rounding to the target's fraction with `GNC_HOW_RND_ROUND` (banker's) | `:2376-2595` |

Callers that get a reverse-direction price must invert the value themselves.

### 4.7 Removing a price

`gnc_pricedb_remove_price` (`:1219`) removes from the list (dropping empty maps) and destroys the price. No other
check.

### 4.8 Remove old prices

Engine: `gnc_pricedb_remove_old_prices(db, comm_list, fiscal_end_date, cutoff, source_flags, keep)`
(`5.17:libgnucash/engine/gnc-pricedb.cpp:1549-1610`). GUI: `gnc_prices_dialog_remove_old_clicked`
(`5.17:gnucash/gnome/dialog-price-edit-db.cpp:474-623`).

Options:

| Flag (`gnc-pricedb.h:381-397`) | Value | GUI label |
|---|---|---|
| `PRICE_REMOVE_SOURCE_FQ` | 1 | "Include _Fetched online prices" (default on) |
| `PRICE_REMOVE_SOURCE_USER` | 2 | "Include manually _Entered prices" |
| `PRICE_REMOVE_SOURCE_APP` | 4 | "Added _by the application" |
| `PRICE_REMOVE_SOURCE_COMM` | 8 | GUI-only: "some commodity selected"; OK needs flags > 8 (`dialog-price-edit-db.cpp:373`) |

| Keep (`PriceRemoveKeepOptions`) | Value | GUI |
|---|---|---|
| `PRICE_REMOVE_KEEP_NONE` | 0 | None |
| `PRICE_REMOVE_KEEP_LAST_WEEKLY` | 1 | Last of Week (GUI default) |
| `PRICE_REMOVE_KEEP_LAST_MONTHLY` | 2 | Last of Month |
| `PRICE_REMOVE_KEEP_LAST_QUARTERLY` | 3 | Last of Quarter |
| `PRICE_REMOVE_KEEP_LAST_PERIOD` | 4 | Last of Period (accounting year) |
| `PRICE_REMOVE_KEEP_SCALED` | 5 | Scaled (implemented in the dialog, not the engine) |

Source matching (`check_one_price_date`, `:1261-1298`) — note the surprising mapping:

```
candidate if (source == FQ        and flag FQ)
          or (source == USER_PRICE ("user:price") and flag USER)
          or (source not in {FQ, USER_PRICE} and flag APP)     // includes user:price-editor, xfer-dialog, invalid, …
and price.time < cutoff                                          // strict
```

Oracle: with flags = USER only and keep NONE, only `user:price` prices before the cutoff were deleted;
`user:price-editor` prices were kept.

Algorithm (`gnc_pricedb_process_removal_list`, `:1439-1546`):

```
remove_old_prices(db, comms, fiscal_end, cutoff, flags, keep):
    cands = [p for c in comms for each currency list of c (forward only) for p in list if matches(p)]
    if cands empty: return FALSE
    if fiscal_end invalid/null: fiscal_end = 31 Dec of the current year (GncDate today)       // :1595-1600
    fiscal_start_month = month(fiscal_end - 12 months) + 1      // 13 if fiscal_end is in December!   :1449-1454
    sort cands by (commodity cmp, currency cmp, time DESC, guid)                                     // :1318
    kept = none
    for p in cands:
        if keep == NONE: remove p; continue
        if kept is none or p.(commodity,currency) != kept.(commodity,currency):
            kept = p; continue                      // newest candidate of each pair is always kept
        a = key(kept), b = key(p)                   // dates via local time (time64_to_gdate)
        if a == b: remove p else kept = p
    return TRUE

key by keep:
    WEEKLY    : ISO-8601 week number only (g_date_get_iso8601_week_of_year)
    MONTHLY   : month number 1..12 only
    QUARTERLY : get_fiscal_quarter(d, fiscal_start_month) = ((roundUp(22 - start + month, 3) / 3) % 4) + 1   // :1428
    PERIOD    : year of the fiscal-year end containing d (gnc_gdate_set_fiscal_year_end)
```

Exact consequences (all confirmed by oracle):

- Only prices strictly older than `cutoff` are considered; everything at or after the cutoff stays.
- Per (commodity, currency) pair, within the candidates, the **newest** price of each period is kept.
- **Year aliasing:** WEEKLY, MONTHLY and QUARTERLY compare only the week/month/quarter *number*, against the last
  *kept* price. If a period has no candidates, an older price in the same-numbered period of an earlier year is deleted.
  Oracle (keep MONTHLY, prices 2021-01-15, 2022-01-10, 2022-01-20, 2023-01-05, 2023-06-01, 2023-06-02): kept only
  2023-06-02 and 2023-01-05; both 2022 prices and the 2021 price were removed. WEEKLY removed 2021-01-15 (week 2) after
  keeping 2022-01-10 (week 2). PERIOD keeps one per fiscal year correctly.
- With `fiscal_end` in December, `fiscal_start_month` is 13, giving quarter = `((roundUp(9 + m, 3)/3) % 4) + 1`, i.e.
  Jan-Mar → 1, Apr-Jun → 2, Jul-Sep → 3, Oct-Dec → 4. Other fiscal ends shift the quarters accordingly.
- Reverse-direction lists (`currency` side = selected commodity) are not touched.

Dialog behaviour (`dialog-price-edit-db.cpp`):

- Commodity list: namespace picker default "All non-currency"; `CURRENCY` shows only currencies; any other shows that
  namespace (`continue_namespace_check`, `:244`). Only commodities with ≥1 forward price are listed, with count and
  oldest date (`:263-327`). The user must select ≥1 commodity.
- Cutoff = selected date at "day neutral" time, 10:59:00 UTC (`gnc_date_edit_get_date` without time,
  `5.17:gnucash/gnome-utils/gnc-date-edit.c:1069`). Default date: today (`:487`).
- `fiscal_end_date` = end of the current accounting period (`gnc_accounting_period_fiscal_end`, `:420-431`; book
  setting, see the accounting-period note).
- SCALED (`:572-596`):

  ```
  t6  = cutoff_date - 6 months;  remove_old_prices(..., cutoff = t6,  keep = WEEKLY)
  t12 = t6 - 6 months;           remove_old_prices(..., cutoff = t12, keep = MONTHLY)
  ```

  i.e. everything kept for the 6 months before the chosen date, one per week for 6-12 months, one per month before
  that (tooltip `5.17:gnucash/gtkbuilder/dialog-price.glade:560`). `t6`/`t12` keep the day-neutral time.

### 4.9 Price editor and other producers

- Price editor (`5.17:gnucash/gnome/dialog-price-editor.c:253-327`): commodity found by namespace + printname; date at
  day-neutral time; source string from the (read-only) source entry, `user:price-editor` for new prices (`:647`);
  type from {bid, ask, last, nav, unknown}; value parsed with no rounding (`gnc_amount_edit_set_fraction(..., 0)`),
  so `12.345` is stored as `12345/1000`. If a price exists on the same day the user is asked to replace it.
- Stock split assistant: §7.

---

## 5. Lots

### 5.1 Fields

| Field | Persisted | Notes |
|---|---|---|
| guid | `lot:id` | |
| account | implicit: the lot is written inside its account's `<act:lots>` | All splits must be in this account; adding a split from another account is refused (`5.17:libgnucash/engine/gnc-lot.cpp:595-605`). |
| splits | implicit: each split's `split:lot` | List order = order added (`g_list_append`). |
| `title` slot | KVP string `title` | `gnc_lot_get_title/set_title` (`:445`, `:463`). |
| `notes` slot | KVP string `notes` | (`:454`, `:474`). |
| `gncInvoice/invoice-guid` slot | GUID | Business lots: the invoice posted to this lot (`gncInvoiceAttachToLot`, `5.17:libgnucash/engine/gncInvoice.c:1219-1231`; paths `5.17:libgnucash/engine/gnc-engine.h:257-261`). |
| `gncOwner/owner-type` slot | integer: 0 NONE, 1 UNDEFINED, 2 CUSTOMER, 3 JOB, 4 VENDOR, 5 EMPLOYEE | Pre-payment lots (`gncOwnerAttachToLot`, `5.17:libgnucash/engine/gncOwner.c:622`; enum `5.17:libgnucash/engine/gncOwner.h:45-53`). |
| `gncOwner/owner-guid` slot | GUID | |
| is_closed | **not persisted**; cached, recomputed from balance | |
| marker, cached_invoice | runtime | |

KVP frames are sorted maps, so `notes` is written before `title` (oracle):

```xml
<act:lots>
  <gnc:lot version="2.0.0">
    <lot:id type="guid">927b86b42c91484db17948547db0bffa</lot:id>
    <lot:slots>
      <slot>
        <slot:key>notes</slot:key>
        <slot:value type="string">some notes</slot:value>
      </slot>
      <slot>
        <slot:key>title</slot:key>
        <slot:value type="string">Empty lot</slot:value>
      </slot>
    </lot:slots>
  </gnc:lot>
</act:lots>
```

Empty lots (no splits) are written if they are in the account's lot list (oracle above).

Account-side slots used by lots: `lot-mgmt/next-id` (integer, next number for default titles) and
`lot-mgmt/gains-acct/<commodity unique name>` (GUID of the gains account per currency)
(`5.17:libgnucash/engine/Account.cpp:68`, `:478`, `:4805-4817`). Oracle:

```xml
<slot>
  <slot:key>lot-mgmt</slot:key>
  <slot:value type="frame">
    <slot>
      <slot:key>gains-acct</slot:key>
      <slot:value type="frame">
        <slot>
          <slot:key>CURRENCY::USD</slot:key>
          <slot:value type="guid">63a7fc991eea46e385018c6642f228a7</slot:value>
        </slot>
      </slot:value>
    </slot>
    <slot>
      <slot:key>next-id</slot:key>
      <slot:value type="integer">2</slot:value>
    </slot>
  </slot:value>
</slot>
```

`gnc_lot_make_default(acc)` (`5.17:libgnucash/engine/gnc-lot.cpp:765`): new lot, title `"<_("Lot")> <next-id>"`
(`Lot 0`, `Lot 1`, … in English; translated otherwise), then `next-id += 1`. The lot joins the account when its first
split is added (or explicitly via `xaccAccountInsertLot`).

### 5.2 Adjusted amounts (new in 5.17)

5.17 adds a runtime-only `adjusted_amount` to each split (commits 0d5c74b868, b1eb12f558, 1e654412ab). It restates
amounts before a stock split in post-split units. **All lot balances, lot closure, FIFO sign tests and gains use
adjusted amounts.**

Computed in `xaccAccountRecomputeBalance` (`5.17:libgnucash/engine/Account.cpp:2263-2330`) over the account's splits in
account order:

```
balance = starting_balance
for split in account.splits (sorted):
    amt = split.amount
    if is_stock_split(split):                      // slot split-type == "stock-split"
        if balance == 0: continue                  // skips the running-balance updates for this split too
        new_balance = balance + amt
        if new_balance == 0: continue
        ratio = reduce(new_balance / balance)
        for p in account.splits before split:
            p.adjusted_amount = round(p.adjusted_amount * ratio, denom = account SCU, ROUND_HALF_UP)
        balance = new_balance
    else:
        split.adjusted_amount = amt
        balance += amt
    (cleared/reconciled/noclosing balances as before)
```

`xaccSplitSetAmount` also sets `adjusted_amount = amount` (`5.17:libgnucash/engine/Split.cpp:1260`).
`is_stock_split` reads slot `split-type`; absent or `normal` → normal; `stock-split` → stock split; anything else is
logged and treated as normal (`5.17:libgnucash/engine/Split.cpp:2039-2081`).

### 5.3 Balance and closure

```
lot.balance = Σ adjusted_amount(s) for s in lot.splits         (gnc_numeric_add_fixed; :487-525)
lot.is_closed = lot.splits non-empty and balance == 0           (empty lot: open, balance 0)
gnc_lot_is_closed(NULL) = TRUE                                  (:367)
gnc_lot_get_balance_before(lot, split) -> (amount, value):      (:529-576)
    target = gains_source(split) or split; tb = target.txn
    sum adjusted_amount and value over s in lot where
        src = gains_source(s) or s; ta = src.txn
        (ta == tb and src != target) or xaccTransOrder(ta, tb) < 0
earliest/latest split: lot.splits sorted in place by posted date only, ties keep list order (:658-685,
    xaccSplitOrderDateOnly 5.17:libgnucash/engine/Split.cpp:1600)
```

Oracle: after a 2:1 split, lot "Lot 0" holding a buy of raw +10 (adjusted +20) and a sale of −20 is closed although raw
amounts sum to −10.

### 5.4 Lot viewer (`5.17:gnucash/gnome/dialog-lot-viewer.c`)

| Operation | Behaviour | Source |
|---|---|---|
| List lots | All lots of the account; optional "only show open lots" filter. Columns: type `I` if invoice lot, open date (earliest split's txn date), close date (latest split's txn date, only if closed), title, balance (adjusted), realized gains. | `:333-450` |
| Realized gains of a lot | Currency = txn currency of the first zero-amount split in the lot; gains = Σ value of zero-amount splits whose txn currency equals it. | `:147-187` |
| Split lists | "in lot" = lot's splits; "free" = account splits with no lot. Both hide zero-adjusted-amount splits (gains splits) except in A/R-A/P accounts, and hide stock-split splits. Amounts adjusted or raw by a checkbox shown only if the account has a stock split. | `:192-230`, `:490-581` |
| Per-split gain | in lot: `xaccSplitGetCapGains`; free: `xaccLotFreeSplitCapGain(split, selected lot)` (a what-if, nothing written). | `:549` |
| Edit title / notes | Saved on selection change: `gnc_lot_set_title(entry text)`, `gnc_lot_set_notes(text)`. | `:236-258` |
| Add split to lot | `gnc_lot_add_split(selected lot, free split)`, no other check (account must match). | `:724-740` |
| Remove split from lot | `gnc_lot_remove_split`, refused if the split's txn is the lot's own invoice txn. | `:472-487`, `:742-761` |
| Delete lot | Any selected lot except an invoice lot (**not only empty lots**): `xaccAccountRemoveLot` + `gnc_lot_destroy`; the lot's splits just lose their `split:lot`. | `:795-806`, `gnc_lot_free` `5.17:libgnucash/engine/gnc-lot.cpp:279-300` |
| Scrub lot | `xaccScrubLot(lot)`, or `gncScrubBusinessLot` for A/R-A/P accounts. | `:807-817` |
| Scrub account | `xaccAccountScrubLots(acc)` (business variant for A/R-A/P). | `:818-827` |
| New lot | `gnc_lot_make_default` + `xaccAccountInsertLot` (empty lot). | `:828-833` |
| View lot in register | Unimplemented (prints a message). | `:788-793` |

GUI quirk not to copy: selecting a lot and moving away writes `title` and `notes` from the widgets even if unchanged,
creating an empty `notes` slot (`gnc_lot_set_notes(lot, "")` stores `""`). The API should write slots only on an
explicit edit.

---

## 6. Capital gains

### 6.1 Policy

Only **FIFO** is implemented in 5.17. `policy.cpp` defines names `fifo`, `lifo`, `average`, `manual`
(`5.17:libgnucash/engine/policy.cpp:52-63`) but only `xaccGetFIFOPolicy` exists (`:154-168`); every account gets it at
creation and `gnc_account_set_policy(NULL)` falls back to it (`5.17:libgnucash/engine/Account.cpp:315`, `:2096-2104`).
Policy is not persisted.

FIFO callbacks (`5.17:libgnucash/engine/policy.cpp`):

```
PolicyGetLot(split) = xaccAccountFindEarliestOpenLot(split.acc, sign = adjusted_amount(split), split.txn.currency)
PolicyGetLotOpening(lot) = (adjusted_amount(e), e.value, e.txn.currency) where e = earliest split of lot
PolicyIsOpeningSplit(lot, s) = (s == earliest split of lot)
PolicyGetSplit(lot):                                    // used by xaccLotFill; DirectionPolicyGetSplit(…, 0), :67-114
    if lot closed or empty: none
    want_positive = lot.balance < 0
    open_time = posted date of the lot's latest split
    first split s in account order with: no lot, s.txn.posted >= open_time, s.txn.currency equiv lot's first
        split's txn currency, s.amount != 0, (s.amount > 0) == want_positive

xaccAccountFindEarliestOpenLot(acc, sign, currency):    // 5.17:libgnucash/engine/cap-gains.cpp:125-200
    best = none, best_time = +inf
    for lot in account.lots (list order):
        skip if closed; e = earliest split; skip if none
        skip unless e.amount has the OPPOSITE sign of `sign`      (raw amount of the opening split)
        skip unless sign(lot.balance) == sign(e.amount)           (not overfull)
        skip unless e.txn.currency equiv currency
        if best_time > e.txn.posted: best = lot, best_time = e.txn.posted      (strict: ties → first in list)
    return best
```

`xaccAccountHasTrades(acc)` (`5.17:libgnucash/engine/cap-gains.cpp:80`): true if the account type is STOCK, MUTUAL or
CURRENCY; otherwise true if any non-gains split's transaction currency differs from the account commodity.

### 6.2 Assigning splits to lots

```
xaccSplitAssign(split) -> bool split_up:                       // :436-485
    if split.lot: return false
    assert split is not a gains split
    if not HasTrades(split.acc) or split.amount == 0: return false
    while split:
        mark split value-dirty
        lot = PolicyGetLot(split) or gnc_lot_make_default(acc)
        split = xaccSplitAssignToLot(split, lot)               // returns the remainder split or null
        if split: split_up = true
    return split_up

xaccSplitAssignToLot(split, lot) -> remainder or null:        // :218-427
    if split.lot: return null
    if split.amount == 0: return null                          // voided or anomalous
    baln = lot.balance (adjusted)
    if lot.is_closed: return split                             // caller must pick another lot
    if baln == 0: add split; return null                       // empty lot
    if sign(split.amount) == sign(baln): add split; return null   // enlarges lot (warning only)
    if |adjusted_amount(split)| <= |baln|: add split; return null
    // split crosses the lot boundary: cut it in two
    if split.amount != adjusted_amount(split):
        baln = round(baln * reduce(split.amount / adjusted_amount(split)), denom = account SCU, ROUND_HALF_UP)
    amt_a = -baln                      ; amt_b = split.amount - amt_a
    val_a = round((amt_a / split.amount) * split.value, denom = denom(split.value), ROUND_HALF_UP)
    val_b = split.value - val_a
    split.amount = amt_a ; split.value = val_a ; lot.add(split)          // lot now closes
    new = new split in same txn and account: memo, action (split-action), reconcile state and
          reconcile date copied; amount = amt_b, value = val_b
    slot on split: lot-split = {peer_guid: new.guid, date: now}
    slot on new  : lot-split = {peer_guid: split.guid, date: now}
    return new
```

- `lot-split` is a **frame, not a list**, written with `set_path` (`qof_instance_kvp_add_guid`,
  `5.17:libgnucash/engine/qofinstance.cpp:1130-1141`; `xaccSplitAddPeerSplit` `5.17:libgnucash/engine/Split.cpp:2084`).
  When a split is cut a second time its previous `lot-split` frame is **overwritten**. Oracle (sell 25 across three
  lots of 10): first piece → peer = second piece; second piece → peer = third piece (link to the first lost); third
  piece → peer = second piece.
- Value rounding oracle (sale value −375.01 over 10/10/5 shares): `-15000/100`, `-15001/100`, `-7500/100`.
- The new split is appended to the transaction; on commit GnuCash re-sorts splits (debits first), so file order may
  differ from creation order (oracle: the cash debit came first). Transaction note owns split ordering.

`xaccAccountAssignLots(acc)` (`5.17:libgnucash/engine/Scrub2.cpp:58-83`):

```
restart:
for split in account.splits (account order):
    if split.lot: continue
    if is_stock_split(split): continue                 // 5.17 rule, :72-73
    if split.amount == 0 and txn is voided: continue
    if xaccSplitAssign(split): goto restart
```

### 6.3 Computing gains (`xaccSplitComputeCapGains`, `5.17:libgnucash/engine/cap-gains.cpp:521-891`)

```
ComputeCapGains(split, gain_acc):
    lot = split.lot; if none: return
    currency = split.txn.currency
    determine gain status (from slots gains-split / gains-source, Split.cpp:1110)
    if currency == account commodity: return                       // :549
    if IsOpeningSplit(lot, split): return                           // :556
    if is_stock_split(split): return                                // :580
    if split is a gains split: split = its source split (gains_split pointer, else slot lookup)   // :586-612
    if any opening split of lot is value-dirty: mark split value-dirty
    if split not dirty and split.gains_split exists and is not dirty: return
    if split.amount == 0: return
    (lot_amount, lot_value) = gnc_lot_get_balance_before(lot, split)
    (_, opening_value, opening_currency) = PolicyGetLotOpening(lot)
    if currency != opening_currency: return                          // different currencies: no gains, silently
    if |lot_amount| < |adj(split)|: error "too thin"; return
    if not opposite signs (lot_amount vs adj(split)): error "too fat"; return
    frac  = reduce(adj(split) / lot_amount)
    basis = round(frac * lot_value, denom = denom(opening_value), ROUND_HALF_UP)       // :718-724
    gain  = basis - split.value     (GNC_HOW_DENOM_FIXED: both must share a denominator, else error → return)
    if gain == 0: return            // an existing gains txn is NOT removed or zeroed
    lot_split = split.gains_split
    if none:
        gacc = gain_acc if gain_acc and its commodity equiv currency else xaccAccountGainsAccount(lot.acc, currency)
        new txn T (currency = currency, description = _("Realized Gain/Loss"))
        lot_split  : account = lot.acc,  memo = _("Realized Gain/Loss")      (first)
        gain_split : account = gacc,     memo = _("Realized Gain/Loss")      (second)
        slot split.gains-split     = lot_split.guid
        slot lot_split.gains-source = split.guid                // gain_split gets NO slot
        write = true
    else:
        T = lot_split.txn; gain_split = the other split of T
        if T has != 2 splits: write = false                     // user-edited: left alone
        elif already exactly {lot_split.value = gain, lot_split.amount = 0,
                              gain_split.value = gain_split.amount = -gain} and links consistent: write = false
        else: write = true; fix T.currency if it differs
    if write:
        T.date_posted  = split.txn.date_posted      (xaccTransSetDatePostedSecs: no normalisation, no date-posted slot)
        T.date_entered = now
        lot_split.amount = 0 ; lot_split.value = gain
        gain_split.amount = -gain ; gain_split.value = -gain
        lot.add(lot_split)
```

Sign convention: with a sale, `split.value < 0`, `basis < 0`; `gain = proceeds − cost`. A positive gain is a debit
(positive value) to the stock account's `lot_split` and a credit to the income account.

`xaccAccountGainsAccount(acc, cur)` (`5.17:libgnucash/engine/Account.cpp:4805`): slot
`lot-mgmt/gains-acct/<unique name of cur>` on the *stock* account; if absent, find or create (breadth-first search
by name from the root) an account named `_("Orphaned Gains") + "-" + mnemonic` of type INCOME, commodity `cur`,
description `_("Realized Gain/Loss")`, notes `_("Realized Gains or Losses from Commodity or Trading Accounts that
haven't been recorded elsewhere.")`, as a child of the root; store its GUID in the slot (`:4760-4800`).

`xaccLotComputeCapGains(lot, gain_acc)` (`:1070-1112`): if any opening split is value-dirty, clear that and mark every
split in the lot dirty; then `ComputeCapGains` for every split.

`xaccSplitGetCapGains(split)` (`:1038`): recompute if dirty; return the value of the gains split (`lot_split`).
`xaccLotFreeSplitCapGain(split, lot)` (`:896-1033`): same formula for a split not in the lot, read-only; 0 if the
split is not later than the lot's earliest split, is a stock split, or currencies differ.

Oracle, two buys (10 @ 10.00, 10 @ 12.00), sell 15 for 225.00, `xaccAccountScrubLots`:

```xml
<trn:description>Realized Gain/Loss</trn:description>
<trn:splits>
  <trn:split>
    <split:id type="guid">3624c50a1f2d492ca33c390330f7416d</split:id>
    <split:memo>Realized Gain/Loss</split:memo>
    <split:reconciled-state>n</split:reconciled-state>
    <split:value>5000/100</split:value>
    <split:quantity>0/1000</split:quantity>
    <split:account type="guid">4544c383c94648c19264f013f354a4cc</split:account>
    <split:lot type="guid">244fdb0fd3ff47f9ba858af052ec0072</split:lot>
    <split:slots>
      <slot>
        <slot:key>gains-source</slot:key>
        <slot:value type="guid">9ce9d9f79d0c42c287ee04a2d9674634</slot:value>
      </slot>
    </split:slots>
  </trn:split>
  <trn:split>
    <split:id type="guid">6617deea50ce4c8d8c6a45798d37825a</split:id>
    <split:memo>Realized Gain/Loss</split:memo>
    <split:reconciled-state>n</split:reconciled-state>
    <split:value>-5000/100</split:value>
    <split:quantity>-5000/100</split:quantity>
    <split:account type="guid">63a7fc991eea46e385018c6642f228a7</split:account>
  </trn:split>
</trn:splits>
```

The source sale split (cut into −10 → lot 0 and −5 → lot 1) carries:

```xml
<split:value>-15000/100</split:value>
<split:quantity>-10000/1000</split:quantity>
<split:account type="guid">4544c383c94648c19264f013f354a4cc</split:account>
<split:lot type="guid">244fdb0fd3ff47f9ba858af052ec0072</split:lot>
<split:slots>
  <slot>
    <slot:key>gains-split</slot:key>
    <slot:value type="guid">3624c50a1f2d492ca33c390330f7416d</slot:value>
  </slot>
  <slot>
    <slot:key>lot-split</slot:key>
    <slot:value type="frame">
      <slot>
        <slot:key>date</slot:key>
        <slot:value type="timespec">
          <ts:date>2026-10-25 13:05:32 +0000</ts:date>
        </slot:value>
      </slot>
      <slot>
        <slot:key>peer_guid</slot:key>
        <slot:value type="guid">0dd1c13e236a460bad1c254b9d1d36f9</slot:value>
      </slot>
    </slot:value>
  </slot>
</split:slots>
```

Gains: lot 0 = 150.00 − 100.00 = 50.00; lot 1 = 75.00 − 60.00 = 15.00. The gains transactions have no `trn:slots`
(no `date-posted` gdate slot), no num, and `date-entered` = now. The lot-split amount is written with the commodity
SCU denominator (`0/1000`).

### 6.4 Lot scrub (repair)

```
xaccScrubLot(lot) -> splits_deleted:                    // 5.17:libgnucash/engine/Scrub3.cpp:84-154
    xaccScrubMergeLotSubSplits(lot, strict = true)
    if lot.balance != 0:
        opening = PolicyGetLotOpening(lot).amount
        if sign(opening) != sign(balance):              // lot is "fat"
            remove every non-opening split from the lot
        xaccLotFill(lot)
        splits_deleted = xaccScrubMergeLotSubSplits(lot, true)
    if gains_possible(lot):                             // account commodity != first split's txn currency
        xaccLotComputeCapGains(lot, NULL)
        xaccLotScrubDoubleBalance(lot)                  // recompute gains; if closed, check Σ value == 0, log only

xaccLotFill(lot):                                       // 5.17:libgnucash/engine/Scrub2.cpp:94-166
    if lot closed: return
    split = PolicyGetSplit(lot); if none: return
    if is_stock_split(split): return                    // 5.17 rule, :122-128
    if split.amount == 0 and txn voided: return
    loop:
        r = xaccSplitAssignToLot(split, lot)
        if r == split: error; break
        if lot closed: break
        split = PolicyGetSplit(lot); if none: break     // NB: later stock splits are not re-checked here

xaccAccountScrubLots(acc):                              // 5.17:libgnucash/engine/Scrub3.cpp:158-178
    if not HasTrades(acc): return
    xaccAccountAssignLots(acc)
    for lot in account.lots: xaccScrubLot(lot)
xaccAccountTreeScrubLots(acc): ScrubLots on every descendant with trades, then on acc   (:190-196)
```

Sub-split merge (`5.17:libgnucash/engine/Scrub2.cpp:249-409`), skipped for invoice transactions: for each split `s` of
the lot, any other split of the same transaction in the same lot that is its peer (`lot-split/peer_guid` equals `s`)
is merged into `s`: amounts and values added, `s` set to not-reconciled, peer links removed and merged, the other
split's gains transaction destroyed, the other split destroyed.

**5.17 stock-split rule** (commit b1efe0763c "The lot scrubber should exclude stock-split splits", in tag 5.17): splits
whose `split-type` slot is `stock-split` are never put into lots by `xaccAccountAssignLots`
(`5.17:libgnucash/engine/Scrub2.cpp:72-73`) or `xaccLotFill` (`:122-128`), never produce gains
(`5.17:libgnucash/engine/cap-gains.cpp:580-584`, `:927-931`) and are hidden from the lot viewer
(`5.17:gnucash/gnome/dialog-lot-viewer.c:515`). Oracle: after buy 10, buy 10, 2:1 stock split (+20), sell 30, the
stock-split split has no `split:lot`; the sale is cut into −20 (lot 0, adjusted buy +20) and −10 (lot 1); gains are
50.00 and 15.00, the same as without the split.

### 6.5 When does GnuCash run any of this?

- **Not on transaction commit.** `xaccTransCommitEdit` calls `xaccTransScrubGains` only if the environment variable
  `GNC_AUTO_SCRUB_LOTS` is set ("Lot Scrubbing is temporarily disabled", `5.17:libgnucash/engine/Transaction.cpp:1559-1563`).
- Explicitly: lot viewer "Scrub" buttons; Actions → Check & Repair (the scrub note covers which entry points call
  `xaccAccountTreeScrubLots`).
- Always on destroy: destroying a transaction destroys the gains transactions of its splits
  (`destroy_gains`, `5.17:libgnucash/engine/Transaction.cpp:1377-1394`).
- `xaccTransScrubGains` (`:2663-2700`), when it runs: syncs a gains txn's posted date to its source; re-assigns or
  re-scrubs amount-dirty splits; recomputes value-dirty gains.

red-ale v0.1 should expose lot scrub and account lot scrub as explicit operations and keep "destroy gains with source"
as an invariant of transaction deletion.

---

## 7. Stock split assistant (`5.17:gnucash/gnome/assistant-stock-split.c`)

Inputs and validation:

| Input | Rule | Source |
|---|---|---|
| account | any account with `xaccAccountIsPriced` (STOCK, MUTUAL, CURRENCY), non-zero balance, not placeholder | `:111-186` |
| date | date only → day-neutral time; default today | `:616` |
| shares ("distribution") | expression, non-zero, rounded to the account SCU; **may be negative** (reverse split) | `:221-224`, `:273-288` |
| description | default `"Stock Split"` (translatable) | `5.17:gnucash/gtkbuilder/assistant-stock-split.glade:158` |
| price | optional, must be ≥ 0 if given; no rounding (fraction 0); currency picker defaults to the currency of the first latest price of the commodity (either direction), else the default currency | `:206-249`, `:290-304` |
| cash in lieu | optional, ≥ 0; if > 0 an income account (type INCOME) and an asset account (BANK, CASH or ASSET) must be chosen | `:309-335`, `:485-504` |
| memo | default `"Cash in lieu"` (translatable) | `5.17:gnucash/gtkbuilder/assistant-stock-split.glade:309` |

Result (`gnc_stock_split_assistant_finish`, `:339-473`):

```
T = new transaction
T.currency     = gnc_default_currency()        // the user's default-currency preference, NOT the account's
T.date_posted  = SetDatePostedSecsNormalized(date)     // day-neutral + date-posted gdate slot
T.description  = description
S1: account = stock account
    amount = shares (as entered)
    xaccSplitMakeStockSplit: value = 0/1 (set directly, denominator 1), slot split-type = "stock-split"
    action = C_("Action Column", "Split")  → "Split"  (split action; num/action book option respected)
    memo: none
if price > 0:
    new price: commodity = account commodity, currency = chosen currency, time = date (day-neutral),
               source = PRICE_SOURCE_STOCK_SPLIT ("user:stock-split"), type = "unknown", value = price
    gnc_pricedb_add_price (same-day precedence rules of §4.5 apply: an existing editor/FQ/user/xfer/register/import
    price that day blocks it — error dialog "Error adding price.")
if cash > 0:
    S2: account = asset account,  amount = value = +cash, memo = memo
    S3: account = income account, amount = value = −cash, memo = memo
commit T
```

Split order as appended: S1, S2, S3 (the commit-time sort may reorder; see the transaction note). Amount/value of S2
and S3 are set equal even if the asset or income account's commodity differs from `T.currency`; no exchange rate is
asked for.

Oracle (S1 only, created the same way through the bindings):

```xml
<trn:description>Stock Split</trn:description>
<trn:slots>
  <slot>
    <slot:key>date-posted</slot:key>
    <slot:value type="gdate">
      <gdate>2024-02-20</gdate>
    </slot:value>
  </slot>
</trn:slots>
<trn:splits>
  <trn:split>
    <split:id type="guid">0ff833d875bb4c09b33b65d9374afe10</split:id>
    <split:action>Split</split:action>
    <split:reconciled-state>n</split:reconciled-state>
    <split:value>0/1</split:value>
    <split:quantity>20000/1000</split:quantity>
    <split:account type="guid">4d4e14dca51540698a0ed734c4c42fe2</split:account>
    <split:slots>
      <slot>
        <slot:key>split-type</slot:key>
        <slot:value type="string">stock-split</slot:value>
      </slot>
    </split:slots>
  </trn:split>
</trn:splits>
```

Note `0/1`: `xaccSplitMakeStockSplit` assigns `gnc_numeric_zero()` without converting to the currency denominator
(`5.17:libgnucash/engine/Split.cpp:2063-2075`). red-ale must write `0/1` for the value of a stock-split split it
creates.

---

## 8. Things red-ale must reproduce exactly (checklist)

1. ISO table: 227 entries from `iso-4217-currencies.xml` (6 used attributes), `ISO4217`→`CURRENCY`, renamed-code
   aliases, `template` commodity in every book.
2. Currency quote flag automation on account use (§1.6), including on load.
3. Quote source and tz kept as arbitrary strings.
4. Price source kept as read (GnuCash would write `invalid`; writing the original is GnuCash-compatible), precedence
   by enum order, bidirectional same-day replacement, silent duplicate drop, no checks during load.
5. Price list order: (namespace, mnemonic) of commodity, then of currency, then time desc, GUID asc.
6. Remove-old-prices including source mapping, strict `<` cutoff and the period-number aliasing.
7. Adjusted amounts for lot balance/closure/FIFO/gains.
8. FIFO only; split cutting with value proration; `lot-split` frame overwrite; gains txn layout and slots;
   Orphaned Gains account and `lot-mgmt` slots.
9. Stock-split splits: value `0/1`, slot `split-type = stock-split`, action `Split`, excluded from lots and gains.

---

## Open questions

1. **Localised strings written to the book.** GnuCash writes translated text: lot titles (`_("Lot")`), gains
   transaction description and memos, Orphaned Gains account name/description/notes, stock split action ("Split") and
   assistant defaults, currency fullnames at runtime (not written). red-ale has no locale model yet; propose English
   (C locale) always, with the API letting callers pass description/memo/title. Needs a decision in
   `design/open-questions.md`.
2. **"Same day" for prices uses the process's local time zone** (`time64CanonicalDayTime`); so do remove-old-prices
   period keys (`time64_to_gdate`). red-ale needs a book-level or request-level zone; GnuCash's behaviour depends on
   the machine running it.
3. **Price source of unknown strings:** keeping the original string is lossless and safe, but a book saved by red-ale
   will then differ from one saved by GnuCash (`invalid`). Does the round-trip comparison treat these as equal? The
   oracle harness should normalise or the round-trip goal should allow it.
4. **Lot policy** is a runtime GObject property (`5.17:libgnucash/engine/Account.cpp:919`), never persisted, and only
   FIFO exists. If red-ale ever adds LIFO/average it is an extension and needs its own slot and design doc.
5. **When should red-ale run lot assignment / gains?** GnuCash does not do it on commit (env var gate). Proposal:
   only on explicit "scrub lots" API calls, matching 5.17. Confirm against the Check & Repair note.
6. **Gains to zero:** if a recomputed gain becomes 0 the existing gains transaction is left with its old value
   (`if gain == 0: return` before the update branch, `cap-gains.cpp:755`). Copy this bug or fix it? Fixing changes the
   book in a way GnuCash would not; leaning to copy, flagged.
7. **Commodity deletion** rule for red-ale (stricter than GnuCash, §1.5) needs sign-off.
8. **Price write order** depends on `strcmp` of namespace and mnemonic (byte order, not locale order); confirmed for
   ASCII, untested for non-ASCII mnemonics.
9. **Lot order inside `<act:lots>`** is the account's lot list order, which after scrubbing was `Lot 1, Lot 2, Lot 0,
   Empty lot` in one oracle run, i.e. not creation order. Keep as read; for newly created lots the insertion rule
   (`xaccAccountInsertLot`) belongs to the account/XML note.
10. **BGN as an euro legacy currency** makes `gnc_is_euro_currency(BGN)` true from 5.17 on; any euro display feature
    will treat BGN amounts as convertible at 1.95583 regardless of date. No date logic exists in GnuCash.
11. `iso-4217-currencies.xml` is GPL (GnuCash file, no separate header). Embedding it is fine under GPL-3.0-or-later, but
    its comment block should keep attribution when copied.
