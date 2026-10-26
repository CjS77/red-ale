# Milestone 3: red-ale v0.3

**Status: in progress.** Items marked *Proposed* were accepted at the start of the work ("Decided" below); the rest is decided, here or in
[`open-questions.md`](open-questions.md) ("Decided for v0.3") and [`roadmap.md`](roadmap.md) ("v0.3").

**Goal:** red-ale holds books the size a small business reaches in a few gigabytes; imports QIF, CSV and business CSV
files as GnuCash 5.17 imports them, matching and learning as GnuCash does; exports CSV and account trees; fetches
online price quotes in its own Rust code from services whose URLs are all configurable; formats reports in a chosen
locale; and lets clients read and change its Configuration. All on XML books written by GnuCash 5.17.

This file tracks progress towards v0.3. Feature names and versions come from [`features.md`](features.md); when a
feature here is done, set its Ported column there to `0.3`.

## Scope

The main features for milestone 3 are

- **memory and scale**, first, so that import has room to work (see "Memory and scale");
- **import**: QIF and the three CSV importers of `features.md` §7 "File import", all of §7 "Matching", and §6
  "Business import";
- **export**: every row of §7 "Export";
- **online price quotes**, fetched by red-ale in the background from services set in the Configuration;
- **locales** for report formatting ([`roadmap.md`](roadmap.md), "Also in v0.3");
- **the Configuration API**: methods to read, list and change red-ale's Configuration.

Also in v0.3: CSV export of the net and category charts ([`reports.md`](reports.md) §12), importing GnuCash's saved
report configurations and style sheets ([`roadmap.md`](roadmap.md), "Also in v0.3"), and the gaps carried forward from
v0.2 (below).

**Out:**

- **OFX/QFX, MT940, MT942, DTAUS and CAMT import: won't do (❌).** GnuCash doesn't parse these files itself: LibOFX
  and AqBanking, separate C libraries, parse them and hand GnuCash records. red-ale links no C library, and there is
  no GnuCash code to translate for the parsing, so these formats are dropped. Their `features.md` rows become ❌.
  Banks that offer them nearly always offer CSV too, which red-ale imports. Books that already hold transactions
  GnuCash imported from them keep everything, `online_id` Slots included (see "Compatibility").
- Online banking: FinTS/HBCI, SEPA transfers and SEPA direct debits (§7 "Online banking"). See
  [`roadmap.md`](roadmap.md), "v1.0".
- **Attached documents** (scans of receipts and invoices stored with the book): v0.4. GnuCash has no attachments; it
  stores only document links, a path or URL per transaction or business document, which v0.2 already ports
  (`docLink.*`). Storing the files themselves goes beyond parity, so it needs a design doc of its own, and nothing in
  v0.3 depends on it: no format v0.3 imports carries attachments, and imported transactions take document links as
  any others do.
- SQL books, scheduled transactions and budgets (v0.4). Imports never create scheduled transactions.
- The log replay importer (`5.17:gnucash/import-export/log-replay/`): red-ale already replays its journal with
  `book.replayJournal`. Replaying a journal GnuCash wrote is a separate question, not taken up here.

## Compatibility in v0.3

Out of scope still never means lost, and every v0.1 and v0.2 rule holds, with these additions:

- **Import maps are GnuCash's.** Learned mappings are stored where GnuCash keeps them, in account Slots: the
  `import-map` frame for the account matcher, and `import-map-bayes` for Bayesian matching
  (`5.17:libgnucash/engine/Account.cpp:5152-5153`). red-ale writes them only in the flat, GUID-keyed form 5.17 writes
  (`:5598-5604`), so GnuCash keeps learning from what red-ale learned and the other way round.
- **Book features are set as GnuCash sets them, and no others.** Writing a Bayesian mapping sets
  `Account GUID based bayesian with flat KVP` (`5.17:libgnucash/engine/Account.cpp:5604`;
  `gnc-features.h:59`), as GnuCash does. red-ale never sets `Account GUID based Bayesian data` (`gnc-features.h:58`),
  which 5.17 no longer sets ([`notes/storage-and-safety.md`](notes/storage-and-safety.md) §10.2). Both are
  already in `SUPPORTED_FEATURES` (`crates/red-ale-service/src/session.rs:21`), so nothing opens read-only that did
  not before.
- **Old Bayesian data is converted as GnuCash converts it**, on the first Bayesian read or write
  (`5.17:libgnucash/engine/Account.cpp:5462-5507`), never on open or on save alone. A book that is only opened and
  saved keeps its old form byte for byte.
- **Online IDs are kept.** Transactions GnuCash imported from OFX or through AqBanking carry an `online_id` Slot on a
  split, which GnuCash checks to skip duplicates (`5.17:gnucash/import-export/import-backend.cpp:1035-1060`). red-ale
  imports none of those formats, so it never writes one, but keeps every one it reads, and copying a transaction
  leaves it out, as GnuCash does ([`api/transactions.md`](api/transactions.md)).
- **Settings GnuCash keeps outside the book stay outside it.** Import settings, CSV import layouts
  (`5.17:gnucash/import-export/csv-imp/gnc-imp-settings-csv-tx.cpp:147`), the QIF account map (a file in GnuCash's
  user data directory, `5.17:gnucash/import-export/qif-imp/qif-guess-map.scm:114`), quote services, and the locale go
  in red-ale's Configuration.
- **An import is one Mutation.** Every row an import adds, matches or updates is applied together or not at all.
- **Imbalanced imports are balanced as GnuCash balances them, and the user is told.** A transaction that doesn't
  balance gets an `Imbalance-<CUR>` split, as GnuCash's importer gives it. `import.preview` flags each such
  transaction with the amount it is out by, before anything changes, and `import.apply`'s result lists every
  Imbalance split it made, with its account and transaction, as a warning. v0.1 refuses unbalanced input everywhere
  else; an import has no caller to correct each row, so it follows GnuCash and says so.
- **Imports ignore the auto read-only threshold, as GnuCash's importers do, and the user is told.** GnuCash's
  importers check no threshold, so a statement can add transactions dated before it. `import.preview` flags each
  transaction that would be read-only by date once added, with the threshold date; the caller can skip it, and one
  left to be added is added. The importers join the list of what the threshold allows
  ([`api/book-options.md`](api/book-options.md), "Auto read-only threshold").
- **Exports only read.** CSV export changes nothing. Export accounts writes a new file and leaves the open book
  unchanged and not dirty.

## Memory and scale

First, as [`roadmap.md`](roadmap.md) ("First: memory and scale") argues: today a 4 GB machine stops at about 110,000
transactions, and bulk import makes it worse. The three changes, none of which changes a byte red-ale writes:

| Status | Change                                      | Notes                                                                                                       |
|:------:|---------------------------------------------|-------------------------------------------------------------------------------------------------------------|
|   ✅   | Keep raw text only where it is needed       | The Residue keeps an entity's original text only where its regenerated form differs                        |
|   ✅   | Parse as a stream                           | `read_book` translates from `quick-xml` events, as GnuCash's SAX parser does (`5.17:libgnucash/backend/xml/sixtp.cpp`) |
|   ✅   | Stream the save and its check               | Write through gzip to the temporary file; compare what is read back entity by entity, as it is parsed       |
|   ❌   | Mutations cost what they touch (*Proposed*) | Not needed: an import is one Mutation, which pays the fixed cost once. See "Decide before code", item 1     |

**Budgets** (fixed from the measurements below): on the 100,000-transaction synthetic book, red-ale at rest within
twice GnuCash 5.17's 188 MB (376 MB), and peaks at open and at save within three times (564 MB). The synthetic book's
plain file is 155.6 MB, so the budgets are 2.41 and 3.62 times the file's size, and
`crates/red-ale-testkit/tests/memory_budget.rs` holds them on a 5,000-transaction book of the same shape, counting
the heap with a counting allocator so each run gives the same figures. All five figures are held to the budgets: at
rest after open and after save to 2.41 times the file, and the peaks of open, post and save to 3.62. On that book
they are 1.09, 1.09, 1.47, 1.14 and 1.54 times the file; the 2,000 accounts weigh more there than on the large book.

**Measured.** `crates/red-ale-testkit/examples/memory.rs` generates the synthetic book (`synthetic.rs`: 2,000
accounts, transactions of two or three splits; GnuCash 5.17 saves it back byte for byte, `oracle_synthetic.rs`) and
measures one process through `book.open`, `transaction.post` and `book.save`. Heap as the allocator counts it, on the
100,000-transaction book (155.6 MB plain; the Book alone is 164.2 MB), in release mode:

| Step                                   | Open, at rest | Peak at open | Peak at post | Peak at save | After save | Peak RSS |
|----------------------------------------|--------------:|-------------:|-------------:|-------------:|-----------:|---------:|
| Before                                 |        674 MB |      2.70 GB |       679 MB |      3.26 GB |     833 MB |  3.87 GB |
| Original text only where needed        |        169 MB |      2.20 GB |       174 MB |      2.26 GB |     329 MB |  2.61 GB |
| One copy of the book after open, save  |        169 MB |      2.04 GB |       174 MB |      2.26 GB |     169 MB |  2.61 GB |
| Parse as a stream                      |        169 MB |       328 MB |       174 MB |       611 MB |     169 MB |   534 MB |
| Stream the save and its check          |        169 MB |       173 MB |       174 MB |       178 MB |     169 MB |   201 MB |
| Budget                                 |        376 MB |       564 MB |       564 MB |       564 MB |     376 MB |          |

GnuCash 5.17 holds 226 MB resident on the same book through its Python bindings, 186 MB above the interpreter alone.
The second row is the Residue change; the third drops the first read of the file before the locked one is parsed, and
records the session's own Book as the snapshot after a save instead of the re-read copy. The fourth reads the file as
a stream (`red_ale_xml::read_book_from`): gzip is undone, UTF-8 checked and line ends normalised as it is read, and
each part of the book (each price of the price database) is built as a small DOM, translated and dropped. What was
left at open was the file's bytes, read whole; at save, the file's text and a second Book read back from it. The fifth
streams both: `Session::open` reads the file through a buffer, and `Session::save` writes through gzip straight to
the temporary file (`write_book_to`), then reads that file back as a stream (`read_back`), comparing each entity with
the session's Book as it is parsed and keeping no second Book. Nothing replaces the old file unless it reads back as
the same Book, as before. Every peak is now within 10% of the open book, and red-ale's peak resident set is below
GnuCash's.

Neither change alters a byte red-ale writes. For every fixture, the 5,000-transaction synthetic book (also after a
post) and the 100,000-transaction one, the plain and gzipped files `book.save` writes and the text written with and
without originals are identical before and after. So are the results (errors, notices, read-only reasons, the files
left beside the book) of opening and saving 32 malformed or unusual files, with one exception: a second root element
after `gnc-v2` is now a syntax error ("content after the root element"), as libxml2 makes it, where the whole-file
parse read the last root and reported no `gnc:book`.

If v0.3 runs long, this part ships alone as v0.3, and the rest follows as v0.3.x ([`roadmap.md`](roadmap.md)).

## Import

### Translated from GnuCash, in Rust

Every importer is Rust, and red-ale links no C library for any of them. Importers follow reports rather than
`CLAUDE.md`'s default of designing from notes: GnuCash's own code is translated into Rust line by line, so red-ale
reads every file as GnuCash reads it.

- **Translated from GnuCash's C and C++:** the generic import backend and matcher
  (`5.17:gnucash/import-export/import-backend.cpp`, `import-main-matcher.cpp`, `import-account-matcher.cpp`,
  `import-parse.cpp`, `import-settings.cpp`), the CSV transaction, account and price importers (`csv-imp/`), and
  business import (`bi-import/`, `customer-import/`). GUI code (dialogs, assistants, GTK models) is not translated:
  what the user chose there arrives as method params, or as changes to the Import proposal.
- **Translated from GnuCash's Scheme:** the QIF importer (`5.17:gnucash/import-export/qif-imp/qif-file.scm`,
  `qif-parse.scm`, `qif-objects.scm`, `qif-to-gnc.scm`, `qif-merge-groups.scm`, `qif-guess-map.scm`,
  `qif-utils.scm`), as reports are translated: one Rust module per `.scm`, keeping its logic and its guesses (date and
  number formats, categories to accounts, investment actions), with Scheme idioms made plain. QIF finds duplicates with
  its own code (`qif-merge-groups.scm:44-57`), not the generic matcher, and the translation keeps that; its results
  still arrive as an Import proposal.
- **Licence.** Every GnuCash file named above is GPL-2.0-or-later (headers checked at 5.17), so its translation can
  be GPL-3.0-or-later. Each Rust module keeps its source's copyright lines and cites the file it comes from, as
  `5.17:gnucash/import-export/<path>`, and where it follows particular lines.
- **No crashes.** Each parser is a pure function from bytes to records, apart from the Book, and is property-tested
  and fuzzed: a malformed file is an error, never a panic.
- **Files arrive in the params, with their encoding.** An import method takes the file's content (text, or base64
  for binary), not a path, so a server never reads files a client names, as it never writes them (v0.2). CSV and QIF
  files are often not UTF-8, and GnuCash's CSV importer asks for the encoding, so the import methods take an
  `encoding` param, converted with `encoding_rs`. The JSON-RPC and MCP request size limits are set to allow a few
  years of statements (jsonrpsee's default is 10 MB), and are in the Configuration.

### One import pipeline, in two calls

GnuCash's importers share a backend: each format's parser produces transactions, and the generic matcher decides for
each one whether to add it, clear an existing transaction it matches, update one, or skip it. The GUI then lets the
user change those choices. red-ale has no GUI, so the same steps become two calls:

1. **`import.preview`** parses the file, runs the matcher, and returns an **Import proposal**: every transaction the
   file holds, with the action the matcher chose, its candidate matches with their scores, the account each split
   goes to, any imbalance it would leave, and whether it would be read-only by date. It changes nothing.
2. **`import.apply`** takes the proposal back, with any actions the caller changed, and applies it as one Mutation.
   It refuses a proposal the Book has moved on from since the preview (`conflict`).

The matcher follows GnuCash's rules and scores, with GnuCash's thresholds as Import settings in the Configuration
and GnuCash's defaults (`5.17:gnucash/gschemas/org.gnucash.GnuCash.dialogs.import.generic.gschema.xml.in:3-47`):

| Setting                    | GnuCash default | What it does                                                    |
|----------------------------|----------------:|-----------------------------------------------------------------|
| `enable-skip`              |            true | "Skip" may be chosen for a transaction                          |
| `enable-update`            |            true | "Update and clear" may be chosen                                |
| `use-bayes`                |            true | Bayesian matching instead of the plain account map              |
| `match-threshold`          |               1 | Lowest score a candidate needs to be offered as a match         |
| `match-date-threshold`     |          4 days | Dates this close add 2 to the score (the same day adds 3)       |
| `match-date-not-threshold` |         14 days | Dates further apart take 5 off the score                        |
| `auto-add-threshold`       |               3 | At or below this best score, or with no match, the transaction is added |
| `auto-clear-threshold`     |               6 | At or above this best score, the match is cleared, or updated and cleared |
| `atm-fee-threshold`        |            2.00 | Amount difference still matched, for ATM fees                   |

Between the two thresholds the transaction is skipped, or updated if skipping is off, or added if both are off
(`5.17:gnucash/import-export/import-backend.cpp:1176-1193`); the date scores are at `:643-657`. GnuCash reads the
numeric settings as doubles (`5.17:gnucash/import-export/import-settings.cpp:96-106`). red-ale keeps the day and
score settings as integers and the ATM fee, an amount, as a decimal string. `auto-create-commodity` (`:48-50`)
applies only to OFX securities, so red-ale has no such setting.

**Account matching** uses the plain account map (`gnc_account_imap_find_account`,
`5.17:libgnucash/engine/Account.cpp:5157`) or Bayesian matching (`gnc_account_imap_find_account_bayes`, `:5513`),
and learns from each applied import (`matchmap_store_destination`,
`5.17:gnucash/import-export/import-backend.cpp:531`). Learning is part of `import.apply`'s Mutation.

Bayesian matching follows GnuCash exactly, so red-ale and GnuCash pick the same account from the same map:

- **Tokens** are the description's words split on spaces, each split's memo split the same way, and the posted
  date's day of the week (`5.17:gnucash/import-export/import-backend.cpp:435-496`). The day name is formatted with
  `%A` in the locale (`:479`), so it follows red-ale's locale setting (see "Locales"); a map trained by a French
  GnuCash holds `mardi`, not `Tuesday`.
- **Scores are computed in `f64`**, as GnuCash computes them (`5.17:libgnucash/engine/Account.cpp:5236-5345`):
  each account's probability combines only the tokens that account has seen, is truncated to an integer out of
  100,000 (`:5280-5296`), and must reach 90% to be chosen (`:5509-5529`). Using the same doubles gives the same
  answer at the threshold. The float ban is for money; a match score is not money. The scoring function is the one
  place that allows `clippy::float_arithmetic`, with a comment saying why, and no amount passes through it.

### Formats

| Status | Feature                      | Introduced | Epic | Notes                                                                                      |
|:------:|------------------------------|:----------:|------|--------------------------------------------------------------------------------------------|
|   ✅   | Import matcher               |    1.8     |      | The pipeline above, for CSV transaction import and business import                         |
|   ✅   | Bayesian account matching    |   1.8.3    |      | Sets `Account GUID based bayesian with flat KVP` (`gnc-features.h:59`)                     |
|   ✅   | Import map editor            |    3.0     |      | List and delete learned mappings: `importMap.list`, `importMap.delete`                     |
|   ✅   | CSV transaction import       |    2.6     |      | The first format on the pipeline. Layouts are saved in the Configuration                   |
|   ✅   | QIF import                   |   ≤ 1.4    |      | Translated from Scheme; its own duplicate finder; the QIF account map in the Configuration |
|   ✅   | CSV account import           |    2.6     |      | Creates accounts from GnuCash's account CSV layout; no matcher                             |
|   ✅   | CSV price import             |    3.0     |      | GnuCash's same-day replace rule, then `price.add`'s; no matcher                            |
|        | Import invoices and bills    |   2.4.1    |      | `5.17:gnucash/import-export/bi-import/`; creates and optionally posts documents            |
|        | Import customers and vendors |    2.6     |      | `5.17:gnucash/import-export/customer-import/`                                              |
|   ❌   | OFX/QFX import               |    1.8     |      | Parsed by LibOFX, not GnuCash (see "Out")                                                  |
|   ❌   | AqBanking file import        |   1.8.9    |      | MT940, MT942 and DTAUS, parsed by AqBanking (see "Out")                                    |
|   ❌   | CAMT import                  |    5.0     |      | Parsed by AqBanking (see "Out")                                                            |

## Export

| Status | Feature         | Introduced | Epic | Notes                                                                                                              |
|:------:|-----------------|:----------:|------|--------------------------------------------------------------------------------------------------------------------|
|   ✅   | CSV export      |    2.6     |      | Account tree (`5.17:gnucash/import-export/csv-exp/csv-tree-export.cpp`) and transactions, simple and complex layouts (`csv-transactions-export.cpp`). Returns text |
|   ✅   | Export accounts |    1.8     |      | A new book with the account tree, commodities and no transactions, as `qof_session_export` makes it (`5.17:libgnucash/engine/qofsession.cpp:676`) |

CSV export returns its text and writes no file, as `report.export` returns HTML ([`open-questions.md`](open-questions.md),
"Decided for v0.2"); the CLI writes the file on its side. Export accounts creates a new book file, as `book.create`
does, and refuses a path that exists.

## Online quotes

Decided for v0.3 ([`open-questions.md`](open-questions.md)): red-ale fetches quotes itself, keeps Finance::Quote's
source names in the book, records fetched prices with the source string `Finance::Quote`, and builds prices as GnuCash
builds them (`5.17:libgnucash/app-utils/gnc-quotes.cpp:546-760`).

| Status | Feature                    | Introduced | Epic | Notes                                                                                                   |
|:------:|----------------------------|:----------:|------|---------------------------------------------------------------------------------------------------------|
|   ✅   | Online quotes (red-ale)    |   ≤ 1.4    |      | New `features.md` row beside the ❌ Finance::Quote one. Fetches in the background (below); [`api/quotes.md`](api/quotes.md) |
|   ✅   | Currency quotes            |   ≤ 1.4    |      | The `currency` source (`5.17:libgnucash/engine/gnc-commodity.cpp:182`): rates for the currencies Accounts use |
|   ✅   | Quote services             |     —      |      | Configurable services, below                                                                             |

### Quote services

A commodity's quote source names *what* to fetch, in Finance::Quote's vocabulary; a **Quote service** in the
Configuration says *where from*. **Every URL red-ale fetches from is configurable; none is hard-coded.** The defaults
below are default values of settings, nothing more. Each service has:

- **a URL template**, with placeholders red-ale fills in: `{symbol}`, `{base}` and `{quote}` (the two currencies of a
  rate), `{date}` and `{key}`;
- **a response format**, which picks the parser: one per service red-ale knows (`alphavantage`, `twelvedata`,
  `finnhub`, `frankfurter`, `yahoo-chart`), and a generic `json` format whose JSON Pointers say where the price, its
  currency and its date are, so a service red-ale doesn't know can be added from the Configuration alone;
- **an optional key**, and a **minimum interval** between requests, for free tiers with rate limits.

**Credentials through a proxy.** Because every URL is configurable, a service's URL can point at a local credential
proxy such as Iron Vault (`../iron-vault`), which adds the credential, forwards the request to the real service and
returns the response. red-ale then holds no key at all, and the service's URL template has no `{key}`. That is all
red-ale needs to do to support it.

The Configuration maps each Finance::Quote source name to the services that answer it, in order: `currency` to
`frankfurter`, `alphavantage` to `alphavantage`, and so on. Multi-market sources (`nasdaq`, `europe`, `usa`, …,
`gnc-commodity.cpp:238-244`) map to a list. A source with no service is reported unsupported, and the commodity's
source is never changed.

**Proposed defaults** (the state of the services in October 2026, from the providers' and reviewers' pages;
recheck before release):

| Source name(s)                 | Default service        | Key  | Why                                                                                               |
|--------------------------------|------------------------|:----:|---------------------------------------------------------------------------------------------------|
| `currency`                     | Frankfurter (ECB rates) |  no  | Keyless, no stated rate limit, ECB reference rates for about 30 currencies, daily, history to 1999 |
| `currency` (fallback)          | Alpha Vantage          | yes  | Finance::Quote's own default for currencies; wider coverage, 25 requests a day free               |
| `alphavantage`                 | Alpha Vantage          | yes  | The source GnuCash users already have keys for                                                    |
| `twelvedata`                   | Twelve Data            | yes  | The largest free tier among keyed services (about 800 requests a day)                            |
| `finnhub`                      | Finnhub                | yes  | Free personal tier, 60 requests a minute, mostly US                                               |
| `yahoo_json`                   | Yahoo chart endpoint   |  no  | Kept for GnuCash users who rely on it, but best-effort: Yahoo has no official API since 2017, and requests are often refused (HTTP 429) |
| `nasdaq`, `nyse`, `usa`        | Alpha Vantage, then Twelve Data | yes | As Finance::Quote's multi-market sources                                                       |

TradingView is not a candidate: it has no public market data API, only a charting library and an API for brokers,
and unofficial scrapers break its terms. Other sources in GnuCash's list (`gnc-commodity.cpp:190-233`) need a
service each and come when users ask for them.

### Fetching in the background

Fetching is asynchronous, so a slow or rate-limited service never holds up other clients:

- **`price.fetchQuotes`** starts a **Quote fetch** for every commodity with quotes on, or for a given list, and
  returns at once with its ID. The server goes on answering other methods while the requests run.
- The requests run outside the Book, each with a timeout and its service's minimum interval. When they are done,
  the fetched prices are added in one Mutation, in turn with every other method, through `price.add`'s same-day rules.
- **`price.fetchStatus`** reports a Quote fetch as running or finished and, for each commodity, the price stored, or
  why there is none (unsupported source, service error, timeout).
- Closing the book cancels a Quote fetch that is still running, and nothing from it is stored.
- **red-ale has no scheduler.** Users run quotes from cron, as GnuCash users run `gnucash-cli --quotes get`: the CLI
  starts a Quote fetch, waits for it to finish, prints its status and exits non-zero if any commodity failed.

### Keys and safety

- A key, where one is used rather than a proxy, lives in the Configuration, falling back to GnuCash's environment
  variables `ALPHAVANTAGE_API_KEY` and `FINANCEAPI_API_KEY` (`5.17:libgnucash/app-utils/gnc-quotes.cpp:82-85`,
  `:280-292`). It is never written to the Book, the journal, a log or a result, and `config.get` shows only whether it
  is set.
- **A key is bound to its service's host.** Changing a service's URL to another host clears its key, so a client
  that can change the Configuration can't send a key somewhere new without also knowing it.
- **Network access is off by default** (*Proposed*). The Configuration turns it on, and only Quote fetches use it.
  Everything else in red-ale stays offline, as v0.2's reports are.
- **Tests never reach the network.** Each response format is tested against recorded responses, and one test checks
  that fetching with network access off fails before any connection is made.

## The Configuration API

There are no general methods for the Configuration today. A few settings have their own (`period.get`/`period.set`,
`docLink.pathHead`/`docLink.setPathHead`, the saved report configuration and style sheet methods); everything else is
edited by hand in the file. v0.3 adds:

| Status | Method          | Effect                | What it does                                                                                     |
|:------:|-----------------|-----------------------|--------------------------------------------------------------------------------------------------|
|   ✅   | `config.schema` | read                  | The Configuration's JSON Schema: every setting, its type, default and description                |
|   ✅   | `config.get`    | read                  | The whole Configuration, or one setting by path; keys and other secrets shown only as set or unset |
|   ✅   | `config.set`    | write (configuration) | Change one or more settings by path, validated against the schema; `null` restores a default     |

- **Atomic.** `config.set` validates every change before writing any, and writes the file through a temporary file and
  a rename, so a failed or interrupted call leaves the Configuration as it was.
- **The existing methods stay** (the API is additive), now as shortcuts to the same settings.
- **No book needed.** Like the other Configuration methods, these work with no book open and never make a book dirty.
- **Keys and the network switch can't be set over MCP.** `config.set` called through the MCP server refuses them,
  so only a person, through the CLI, JSON-RPC or the file, can let red-ale reach the network or hand it a key. Service
  URLs can be changed over MCP; with the key bound to its host, that can't leak a key.

## Locales

As in [`roadmap.md`](roadmap.md), "Also in v0.3": the locale becomes a setting, the C locale stays the default, and
every test and output that exists today is unchanged under it. A locale covers the number format (decimal point and
grouping, `5.17:libgnucash/core-utils/gnc-locale-utils.c:75-108`), the `locale` date format (the locale's `D_FMT`,
`5.17:libgnucash/engine/gnc-date.cpp:61`, `:522`) and ICU's interval form for date ranges in titles
(`:618-680`), the locale's currency symbol and default report currency (`5.17:libgnucash/app-utils/gnc-ui-util.cpp:748-822`),
the first day of the week (`5.17:libgnucash/engine/gnc-date.cpp:195-215`), and day names.

*Proposed:*

- **Where it is set:** a `locale` setting in the Configuration, a POSIX locale name such as `en_US` or `fr_FR`,
  default `C`. `report.run` and `report.export` take an optional `locale` param for one run. The value `environment`
  follows `LC_ALL`, `LC_NUMERIC`, `LC_MONETARY`, `LC_TIME`, `LC_MESSAGES` and `LANG` as GnuCash does, one category
  at a time, for users who want red-ale to print what their GnuCash prints, as red-ale already takes the time zone
  from `TZ`.
- **Where the data comes from:** glibc's locale definitions, not CLDR. GnuCash on Linux reads its number formats from
  `localeconv`, its date format from `nl_langinfo` and its day names from `strftime`, which are all glibc's data. CLDR
  differs from glibc in places (en_US short dates, for one), so CLDR would not match GnuCash. red-ale ships a table
  of the locales it supports, generated by a script from glibc's locale sources and committed, so builds don't depend
  on what locales a machine has installed. glibc's locale files carry a notice that the FSF claims no copyright in
  the data; check that for each file the table uses. The exceptions are what GnuCash takes from ICU, the date
  interval in titles and the first day of the week (`gnc_start_of_week` calls ICU's `getFirstDayOfWeek`): those come
  from ICU4X (`Unicode-3.0`, already allowed in `deny.toml`).

*Done:* `red-ale-locale` holds 74 locales and C, generated by `scripts/gen-locales.py`, which follows `copy`
directives, resolves `<Uxxxx>` escapes and refuses any file without the FSF notice. The set is, for each language
GnuCash 5.17 ships a translation for, its main territories in glibc, plus common English, French, German, Spanish,
Italian and Dutch variants. The first day of the week and date intervals come from ICU4X 2.4 (`icu_datetime`'s range
formatter, behind its `unstable` feature and pinned exactly); its multi-day intervals matched ICU 74's, the oracle's,
for every locale sampled ([`reports.md`](reports.md) §14 lists the differences). Under C, every existing output is
unchanged, including `<from> to <to>` in titles.
- **What clients see:** `config.schema` lists the supported locales as the setting's allowed values, so `api.schema`
  needs nothing new.

| Status | Change                     | Notes                                                                                    |
|:------:|----------------------------|------------------------------------------------------------------------------------------|
|   ✅   | Locale setting             | `locale` in the Configuration, and an optional `locale` param on report runs             |
|   ✅   | Locale data                | A committed table from glibc's locale sources; ICU4X for date intervals and week start   |
|   ✅   | Number and date formats    | Every report renderer and `display` form follows the locale                              |
|   ✅   | Day names in import tokens | The Bayesian day-of-week token in the locale's language (`import-backend.cpp:479`)       |
|   ❌   | Locale collation           | Out of v0.3. Name sorting stays bytewise ([`reports.md`](reports.md) §14)               |

The trailing point after a whole amount (`5. ACME`) does not depend on the locale; see [`reports.md`](reports.md) §3.

## Also in v0.3

| Status | Feature                                     | Notes                                                                                                   |
|:------:|---------------------------------------------|---------------------------------------------------------------------------------------------------------|
|   ✅   | CSV export of the net and category charts   | `report.exportCsv`: the rows of GnuCash's CSV export, byte for byte ([`reports.md`](reports.md) §12, [`api/reports.md`](api/reports.md)) |
|   ✅   | Import saved report configurations          | `report.importConfigurations`: from `saved-reports-2.8`, else `saved-reports-2.4` (`5.17:gnucash/report/gnc-report.h:35-36`); literal option forms only, never evaluating Scheme ([`reports.md`](reports.md) §15) |
|   ✅   | Import style sheets                         | `report.importStyleSheets`: from `stylesheets-2.0` (`5.17:gnucash/report/html-style-sheet.scm:142-143`); image options kept, not rendered |

The details are in [`roadmap.md`](roadmap.md), "Also in v0.3": a saved configuration keeps only the options that
differ from its template's defaults, one whose template red-ale has not translated is reported and skipped, and both
spellings of the accounting-period dates are accepted.

## Carried forward from v0.2

Each is closed in v0.3 or recorded as a deviation ([`roadmap.md`](roadmap.md), "Carried forward from v0.2"):

| Status | Gap                                                                  | Proposed                                                                     |
|:------:|----------------------------------------------------------------------|------------------------------------------------------------------------------|
|   ✅   | Balance-limit check leaves out sub-accounts in other commodities    | Closed: converted at the latest price, as GnuCash does; checked by the oracle |
|   ✅   | GnuCash's business lot scrub is not an operation                     | Decided: the oracle check only, as business import never scrubs ([`api/business-posting.md`](api/business-posting.md)) |
|   ✅   | Day boundaries follow zone files exactly                             | Decided: a deviation, with its reason ([`api.md`](api.md), "Dates, times and the local time zone"); stored dates are unaffected |
|   ✅   | Nothing pins the API as additive                                     | Closed: `red-ale-rpc/tests/api_additive.rs` against the committed 0.2.0 schema ([`api.md`](api.md), "Versioning") |
|   ✅   | Two Rhai closure limits worked around by convention                  | Settled: both are Rhai bugs, reproduced without red-ale's resolver (`red-ale-report/tests/rhai_closure_limits.rs`); to report upstream, conventions kept until fixed |
|   ✅   | `fixtures/synthetic/rich-balance-report.html` is GnuCash 5.5 output | Labelled as 5.5's in [`../fixtures/README.md`](../fixtures/README.md); no 5.17 GUI build exists |

## Interfaces

These are red-ale's own work, not GnuCash features. Each feature above counts as done only once it is reachable
through JSON-RPC, MCP and the CLI.

| Status | Interface    | Notes                                                                                                                                  |
|:------:|--------------|----------------------------------------------------------------------------------------------------------------------------------------|
|        | JSON-RPC API | `import.*`, `importMap.*`, `export.*`, `config.*`, `price.fetchQuotes` and `price.fetchStatus`. `redAleApi` becomes `0.3.0`, additive over `0.2.0` |
|        | MCP server   | Every new method as a tool, as in v0.2. `import.apply` and `importMap.delete` are destructive; `import.preview`, `export.*`, `config.get`, `config.schema` and `price.fetchStatus` read only |
|        | CLI          | Every new method through `exec` and `call`. The CLI reads import files and writes export files on its own side, and waits for a Quote fetch to finish |

`book.open` and `book.create` keep their paths, since a book has to live beside its lock and backups.

## Testing

Every feature follows [`testing.md`](testing.md): round-trip and oracle tests, with GnuCash 5.17 opening what
red-ale wrote after an import.

The oracle can't run GnuCash's importers: they are under `gnucash/`, which builds only with the GUI
(`5.17:CMakeLists.txt:919-921`). As for reports in v0.2, expected results come from:

- **GnuCash's own import tests**, rebuilt as red-ale tests: `5.17:gnucash/import-export/test/`
  (`gtest-import-backend.cpp`, `gtest-import-account-matcher.cpp`, `gtest-import-parse.cpp`), `csv-imp/test/`
  (with `sample1.csv`) and `qif-imp/test/` (`test-qif-parse.scm`, `test-qif-imp.scm`, `test-qif-merge-groups.scm`).
  Each ported expectation cites its test, and fixtures taken from GnuCash keep their licence notice.
- **Hand-worked cases** for what those tests don't reach, each citing the 5.17 lines it follows.
- **The engine, through the oracle**, where the Python bindings reach it: GnuCash's own Bayesian lookup finds the
  account red-ale learned.
- **Property and fuzz tests** on every parser: no input crashes it, and a file red-ale exports as CSV imports back
  to the same transactions.
- **Quote fetches** against recorded responses only, including a slow service: other methods are answered while it
  runs.

## Order of work

1. **Memory and scale**, so import has room to work. The Mutation question (item 1 below) is settled before import
   code.
2. **The Configuration API**, since quote services, import settings, CSV layouts and the locale all live there.
3. **Locales** and **online quotes**: neither depends on import.
4. **The import pipeline** (proposal, matcher, account map, Bayesian matching) with **CSV transaction import** as its
   first format, since GnuCash's tests reach it best.
5. **QIF**, **CSV account and price import**, **business import**, **exports**, and the **import map editor**.
6. **Saved report configurations and style sheets**, and the **CSV export of charts**.

## Decide before code

1. **Mutation cost.** Each Mutation clones the entity maps and scans every transaction, 14 ms at 100,000
   transactions ([`roadmap.md`](roadmap.md)). An import is one Mutation, so it pays that once, which may be enough.
   Decide, from measurements after the memory work, whether Mutations also track what they change or use persistent
   maps. **Decided** (below): one Mutation per import is enough.
2. **Glossary.** New terms enter [`glossary.md`](glossary.md) before code: *Import proposal*, *Import action* (add,
   clear, update, skip), *Match score*, *Import map*, *Online ID*, *Import settings*, *Quote service*, *Quote fetch*,
   *Locale*. Flag the clashes: GnuCash's "update and clear" is one action; *clear* here is a split's reconcile state,
   not deleting; *Quote source* (in the book, Finance::Quote's name) is not *Quote service* (in the Configuration, a
   URL).
3. **The Import proposal**: its shape, whether the server holds it (by ID, until the book closes) or the client
   sends it back whole, and how `import.apply` tells that the Book moved on since the preview.
4. **The Quote fetch**: how long a finished one's status is kept, and whether more than one may run at a time.

### Decided

Settled at the start of v0.3, so every piece of work starts from the same answers:

- **Mutation cost (item 1): one Mutation per import is enough; Mutations neither track what they change nor use
  persistent maps in v0.3.** Measured after the memory work with `crates/red-ale-testkit/examples/mutation_cost.rs`,
  release mode, medians (the machine was shared, so single figures vary by a few milliseconds):

  | Synthetic book                              | 100,000 transactions | 200,000 transactions |
  |---------------------------------------------|---------------------:|---------------------:|
  | `transaction.post`                          |              9–24 ms |                48 ms |
  | A Mutation that changes nothing             |              8–28 ms |                48 ms |
  | One Mutation adding 5,000 transactions      |             52–69 ms |                91 ms |
  | of which the 5,000 edits themselves         |                13 ms |                13 ms |
  | 500 Mutations adding one each               |  10.3 s (21 ms each) |  25.0 s (50 ms each) |
  | Cloning the Book                            |               2.5 ms |               5.3 ms |
  | Finding changed transactions by GUID        |                11 ms |                22 ms |
  | Finding them by walking both maps in order  |              0.09 ms |              0.27 ms |

  An import that is one Mutation pays the fixed cost once: 5,000 transactions take under a tenth of a second at
  200,000, where 5,000 Mutations would take over four minutes. One call at a time, 10 to 50 ms is fine for a client.
  The fixed cost is not the clone: it is `Session::try_mutate` finding changed transactions by looking each one up by
  GUID. It makes two such passes (added or changed, then removed), each 22 ms at 200,000, so together they are most of
  an empty Mutation's 48 ms. Walking the old and
  new maps in order, falling back to lookups only where their keys stop lining up, finds the same changes a hundred
  times faster, and would bring a Mutation down to about the cost of the clone (a few milliseconds at 200,000).
  That is a small change in `session.rs`, proposed as its own ticket; it is not needed for import. Persistent maps
  or change tracking would only remove the clone, which stays small until books reach millions of transactions.
- **Glossary (item 2):** the terms enter [`glossary.md`](glossary.md) with the import design, before import code. **Done:**
  the terms are in [`glossary.md`](glossary.md), "Import and export", with the clashes flagged, and the designs are
  in [`api/import.md`](api/import.md), [`api/export.md`](api/export.md) and [`api/quotes.md`](api/quotes.md).
- **The Import proposal is held by the server (item 3).** Proposals can hold a few years of statements, too much for
  an agent to send back whole and easy to change by accident. `import.preview` returns the proposal with an ID, and
  `import.apply` takes that ID with only the caller's changes (an action, a match or an account per transaction,
  by index). The server keeps a proposal until it is applied, until the book closes, or until 16 newer ones push it
  out, and `import.discard` drops one. The Session counts its Mutations (the **Book revision**); a proposal records
  the revision it was made at, and `import.apply` refuses one made at another revision with `conflict`, so a
  proposal is never applied to a Book it was not computed from.
- **One Quote fetch at a time (item 4).** `price.fetchQuotes` refuses with `conflict` while one is running. The
  status of the last 16 finished fetches is kept until the book closes.
- ***Proposed* items accepted as written:** the memory budgets (fixed by the first memory ticket from its
  measurements), network access off by default, and the `locale` setting with its `environment` value and glibc
  data.

## Done when

- Every row above is ✅ or ❌, and each ✅ row's Ported column in `features.md` says `0.3`.
- On the 100,000-transaction synthetic book, open, `transaction.post` and save stay within the memory budgets, and
  every round-trip and oracle test passes unchanged.
- Every importer passes its tests under "Testing", and GnuCash 5.17 opens every book red-ale wrote after an import
  with no warnings and no lost data.
- An import applied, then the same file imported again, adds nothing: the matcher finds every transaction.
- `import.preview` flags every imbalanced transaction and every one dated before the auto read-only threshold, and
  `import.apply` reports every Imbalance split it made.
- Mappings red-ale learned are found by GnuCash 5.17's own lookup, and the book declares only the features GnuCash
  would have set.
- A file exported as CSV and imported back gives the same transactions.
- A Quote fetch stores prices GnuCash ranks as fetched quotes, from services set only in the Configuration, without
  holding up other methods, and no test reaches the network.
- `config.get`, `config.set` and `config.schema` cover every setting, a failed `config.set` changes nothing, no key
  appears in any result, log or journal, and `config.set` through MCP refuses keys and the network switch.
- Under the default locale, every v0.2 report gives the same bytes as before; under another locale, numbers, dates
  and day names follow it.
- Every JSON-RPC method is an MCP tool with the same name, params schema and hints, and `redAleApi` 0.3.0 is
  additive over 0.2.0, checked by a test.
- v0.1's and v0.2's done-when checks still pass.
