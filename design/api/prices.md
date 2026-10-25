# API: prices

The price database, the price editor and "remove old prices". Domain code: `crates/red-ale-core/src/ops/prices.rs`;
methods: `crates/red-ale-rpc/src/methods/prices.rs`. Research:
[`notes/engine-commodities-prices-lots.md`](../notes/engine-commodities-prices-lots.md) §4.

A price is the value of one commodity in another (usually a currency) at a time: `value` units of `currency` per unit
of `commodity`, kept exactly as given (no reduction, no rounding to a fraction). Online quotes are not fetched in v0.1;
the quote source and time zone of a commodity are stored and edited only (see [`commodities.md`](commodities.md)).

`PriceInfo`: `{ guid, commodity, currency, time, date, source, type, value }`. `date` is the local day of `time`.

## Same day, source precedence

GnuCash ranks price sources; a lower rank wins (`PriceSource`, `5.17:libgnucash/engine/gnc-pricedb.h:169-182`):

| Rank | Source                     | Rank | Source                   |
|------|----------------------------|------|--------------------------|
| 0    | `user:price-editor`        | 6    | `user:stock-split`       |
| 1    | `Finance::Quote`           | 7    | `user:stock-transaction` |
| 2    | `user:price`               | 8    | `user:invoice-post`      |
| 3    | `user:xfer-dialog`         | 9    | `temporary`              |
| 4    | `user:split-register`      | 10   | `invalid`, any other string, or none |
| 5    | `user:split-import`        |      |                          |

A price read from a file keeps its source string even when GnuCash does not know it; it ranks 10. GnuCash itself would
write such a price back with source `invalid` (`gnc_price_set_source_string` ignores unknown strings,
`5.17:libgnucash/engine/gnc-pricedb.cpp:530-542`); keeping the string is lossless and GnuCash reads it the same.
New sources given to red-ale must be one of the eleven strings, so that what red-ale writes reads back the same in
GnuCash.

"The same day" is the same day in the server's local time zone, as in GnuCash: both times are moved to local noon of
their local day and compared (`time64CanonicalDayTime`, `5.17:libgnucash/engine/gnc-date.cpp:403-410`). A price
entered by date is stored at the day's neutral time (10:59 UTC from UTC−10 to UTC+13; see
[`../api.md`](../api.md), "Dates, times and the local time zone"). v0.1 decided the same day in UTC.

## Methods

### `price.add` (write)

| Param       | Type        | Meaning                                                     |
|-------------|-------------|-------------------------------------------------------------|
| `commodity` | CommodityId | The commodity priced; must exist                            |
| `currency`  | CommodityId | What the value is in; must exist and differ from `commodity` |
| `value`     | Amount      | Stored exactly                                              |
| `date`      | Date        | The day, stored at its neutral time as the price editor does (`5.17:gnucash/gnome-utils/gnc-date-edit.c:1069`) |
| `time`      | Timestamp   | Or an exact time; give exactly one of `date` and `time`     |
| `source`    | string?     | Default `user:price-editor`, the editor's (`5.17:gnucash/gnome/dialog-price-editor.c:175`) |
| `type`      | string?     | `last`, `bid`, `ask`, `nav`, `unknown`, …; any string is kept |

Returns `{ outcome, price, replaced, existing }`. The rules are `add_price`'s
(`5.17:libgnucash/engine/gnc-pricedb.cpp:1013-1110`):

1. Find the price of the pair nearest in time on the same day, **in either direction** (a price of USD in EUR counts
   for EUR in USD; `gnc_pricedb_lookup_day_t64`, `:2171`).
2. If its source ranks better than the new price's, nothing is added: `outcome: "rejected"`, `existing` is that price.
3. Otherwise it is removed (`replaced`).
4. If a price of the pair in the same direction on the same day has an equal value, nothing is added:
   `outcome: "duplicate"`, `existing` is that price (`price_is_duplicate`, `:696-703`).
5. Else the price is added: `outcome` is `added` or `replaced`.

Loading a file applies none of this (GnuCash loads prices in bulk mode, `:1057-1071`), so a book may hold several
prices of a pair on one day.

### `price.get` (read)

`{ price: Guid }` → `PriceInfo`.

### `price.list` (read)

| Param            | Type        | Meaning                                                     |
|------------------|-------------|-------------------------------------------------------------|
| `commodity`      | CommodityId? | Only prices of this commodity (`gnc_pricedb_get_prices`, forward only, `:2047`) |
| `currency`       | CommodityId? | Only in this currency (needs `commodity`)                  |
| `bothDirections` | bool        | Also prices of `currency` in `commodity` (needs both)       |
| `from`, `to`     | Date?       | Only prices from local 00:00:00 / to local 23:59:59 of these days |

Without `commodity`, every price, in the order GnuCash writes them: by commodity, then currency, then newest first
(`stable_price_traversal`, `:2688-2729`). With it, newest first, ties by GUID (`compare_prices_by_date`, `:674-692`).

### `price.latest` (read)

`{ commodity, currency }` → `FoundPrice | null`: the newest price of the pair in either direction
(`gnc_pricedb_lookup_latest`, `:1731`).

`FoundPrice`: `{ price: PriceInfo, reversed, rate }`. `reversed` says the price is of `currency` in `commodity`;
`rate` is the value in the requested direction (the exact inverse when reversed, null for a zero value). GnuCash
returns the stored price and leaves inverting to the caller.

### `price.nearest` (read)

`{ commodity, currency, date | time, mode }` → `FoundPrice | null`, looking in both directions:

| `mode`              | GnuCash function                                 | Picks                                                       |
|---------------------|--------------------------------------------------|-------------------------------------------------------------|
| `nearest` (default) | `gnc_pricedb_lookup_nearest_in_time64` (`:2180-2288`) | The closer of the newest price at or before the time and the oldest after it; a tie goes to the older |
| `before`            | `gnc_pricedb_lookup_nearest_before_t64` (`:2305`) | The newest price at or before the time                      |
| `sameDay`           | `gnc_pricedb_lookup_day_t64` (`:2171`)            | The nearest price on the same (local) day                   |

GnuCash has no "nearest after"; neither does red-ale.

### `price.update` (write)

`{ price, commodity?, currency?, date? | time?, source?, type?, value? }` → `{ price, replaced }`. `type: null` clears
the type. The price editor sets each field on the price (`gui_to_price`, `5.17:gnucash/gnome/dialog-price-editor.c:253-327`);
changing the commodity, currency or time removes the price and adds it again (`gnc_price_set_commodity` and friends,
`5.17:libgnucash/engine/gnc-pricedb.cpp:457-518`), so the adding rules apply against the other prices:

- a same-day price it outranks or equals is removed and returned as `replaced`;
- **deviation:** where GnuCash would then drop the edited price itself (a same-day price with a better source, or an
  identical one), red-ale refuses with `conflict` and changes nothing. GnuCash loses the price in that case.

Changing only the value, source or type re-runs nothing, as in GnuCash.

### `price.delete` (destructive)

`{ price }` → the deleted `PriceInfo` (`gnc_pricedb_remove_price`, `:1219`).

### `price.removeOld` (destructive)

| Param           | Type          | Default         | Meaning                                                    |
|-----------------|---------------|-----------------|------------------------------------------------------------|
| `commodities`   | CommodityId[] |                 | At least one; their prices in every currency, forward only |
| `before`        | Date          |                 | Only prices strictly before this day's neutral time        |
| `sources`       | string[]      | `["fetched"]`   | `fetched` (`Finance::Quote`), `user` (`user:price`), `app` (every other source, including the editor's) |
| `keep`          | string        | `lastOfWeek`    | `none`, `lastOfWeek`, `lastOfMonth`, `lastOfQuarter`, `lastOfPeriod`, `scaled` |
| `fiscalYearEnd` | Date?         | 31 Dec this year | For `lastOfQuarter` and `lastOfPeriod`                    |

Returns `{ removed: Guid[] }`. This is `gnc_pricedb_remove_old_prices` (`:1549-1610`) and the dialog's defaults
(`5.17:gnucash/gnome/dialog-price-edit-db.cpp:474-623`), exactly:

- The candidates are the commodities' forward prices with a matching source before the cutoff. The source mapping is
  GnuCash's: `user` matches only `user:price`; prices from the price editor, the transfer dialog and unknown sources
  are `app` (`check_one_price_date`, `:1261-1298`).
- Per (commodity, currency), newest first, the newest candidate is kept, and each later candidate is removed if it
  falls in the same period as the last price kept, else kept (`gnc_pricedb_process_removal_list`, `:1439-1546`).
- Periods compare by **number only**: ISO week number, month number, fiscal quarter number
  (`((roundUp(22 − start + month, 3) / 3) mod 4) + 1` with `start` the fiscal-year-end month + 1, `:1415-1431`). So
  January 2022 is "the same month" as January 2023: a year with no candidate in a period loses its price to the
  following year's. `lastOfPeriod` compares the year of the fiscal-year end containing the date
  (`gnc_gdate_set_fiscal_year_end`, `5.17:libgnucash/engine/gnc-date.cpp:1674`), which has no such aliasing.
- `scaled` is the dialog's: remove with `lastOfWeek` before the cutoff minus six months, then with `lastOfMonth`
  before the cutoff minus twelve months (`dialog-price-edit-db.cpp:572-596`).
- Periods compare local dates, as GnuCash does (`time64_to_gdate`, `:1482-1483`). A 29 February fiscal-year end is the 28th in other years (GLib would make
  an invalid date).

## Deviations from GnuCash

| What | GnuCash 5.17 | red-ale | Why |
|------|--------------|---------|-----|
| Unknown source strings read from a file | Written back as `invalid` | Kept | Never lose data; GnuCash reads both the same |
| New prices with an unknown source | Stored as `invalid` | `invalid` error | What red-ale writes must read back the same |
| A price of a commodity in itself | Accepted | `invalid` | It means nothing |
| Editing a price onto a day where it would be rejected or a duplicate | The edited price is silently dropped | `conflict`, nothing changes | Never lose data |
