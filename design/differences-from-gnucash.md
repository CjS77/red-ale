# Differences from GnuCash

Every way red-ale behaves differently from GnuCash 5.17 on purpose, in one place. This is the tracking file: a new
difference gets a row here, in its area's table, when it is decided. Its area's design page keeps the detail (the
behaviour around it, tests, examples); the row here links to it.

What counts:

- GnuCash does one thing and red-ale another, given the same book and the same request.
- red-ale refuses what GnuCash accepts, or accepts what GnuCash refuses.
- The book written differs from the one GnuCash would write (beyond what [`file-fidelity.md`](file-fidelity.md) and
  [`sql-fidelity.md`](sql-fidelity.md) allow).

What doesn't: features red-ale doesn't have yet, or won't port (GUI windows, OFX import, Guile scripting and so on).
Those are rows of [`features.md`](features.md) with a blank or ❌ status. A design choice with no GnuCash counterpart
(the JSON-RPC API, the Configuration) is not a difference either.

Each row: **What** the case is, what **GnuCash 5.17** does (cited as `5.17:path:line`, or a path relative to the
area's source tree where the area's page says so), what **red-ale** does, and **Why**. A citation with no tag (for
example `Scrub.cpp:121`) is to `libgnucash/engine/` unless the row names another directory.

## Contents

1. [The principles behind most differences](#the-principles-behind-most-differences)
2. [Files, sessions and time](#files-sessions-and-time)
3. [Book options](#book-options)
4. [Accounts](#accounts)
5. [Commodities](#commodities)
6. [Prices](#prices)
7. [Online quotes](#online-quotes)
8. [Transactions](#transactions)
9. [Lots and capital gains](#lots-and-capital-gains)
10. [Trading accounts](#trading-accounts)
11. [Scrub (Check & Repair)](#scrub-check--repair)
12. [Reconciliation](#reconciliation)
13. [Closing the books](#closing-the-books)
14. [Budgets](#budgets)
15. [Scheduled transactions](#scheduled-transactions)
16. [Business: contacts](#business-contacts)
17. [Business: documents](#business-documents)
18. [Business: posting and payments](#business-posting-and-payments)
19. [Import](#import)
20. [Export](#export)
21. [Reports: running and saving](#reports-running-and-saving)
22. [Reports: what they show](#reports-what-they-show)

## The principles behind most differences

Most rows below follow from a handful of rules. A new difference that follows one of them needs no new argument;
one that follows none of them needs a good one.

| Principle | What it means against GnuCash | Rows that follow it |
|-----------|-------------------------------|---------------------|
| **The book is never changed unasked.** | GnuCash scrubs, upgrades and repairs on open, and runs Since Last Run and the budget sign scrub from its GUI. red-ale's open writes nothing; each repair is a method the caller runs. | Files (SQLite open), budgets, scheduled transactions, lots (gains), import (Bayesian data) |
| **Refuse rather than repair.** | GnuCash's commit balances an unbalanced transaction with an `Imbalance-<CUR>` split, and many dialogs save what a later step mis-reads. red-ale refuses with an error that names the cause. | Transactions, close book, reconciliation, business posting, scheduled transactions, import |
| **Never lose data.** | Where GnuCash drops what it can't hold or doesn't understand (unknown price sources, deferred instances in SQL, a replaced CSV price), red-ale keeps it or refuses the edit. | Prices, commodities, files, import |
| **Money is exact.** | GnuCash uses doubles in reports, the matcher and formulas, and rounds or fails past 64 bits. red-ale computes with exact rationals and refuses an overflow. | Reports, import, scheduled transactions, business posting |
| **Output is deterministic.** | GnuCash walks hash tables and sorts by locale collation. red-ale uses book order and compares text bytewise, so the same book gives the same answer everywhere. | Accounts, lots, trading accounts, scrub, budgets, quotes, reports, import, export |
| **Every change applies whole.** | GnuCash's windows apply choices row by row and can stop halfway. red-ale applies a Mutation completely or not at all, and a window becomes a proposal the caller applies. | Import, Since Last Run, business posting, budgets |
| **No GUI, no per-user state.** | Some GnuCash behaviour lives in the GUI or the user's preferences (default currency, formal labels, reverse-balance display, path head fallback). red-ale reads only the book and its Configuration. | Lots (stock split), reconciliation, budgets, transactions, reports |
| **English only.** | GnuCash translates account names it creates (`Imbalance`, `Orphan`, `Trading`), strings it writes and report text. red-ale writes the English msgid. | Scrub, trading accounts, business posting, import, export, reports |
| **No Guile, Perl or Scheme.** | Reports, formulas and saved configurations are Scheme in GnuCash; quotes are Perl. red-ale translates or reimplements them and imports GnuCash's files as data. | Reports, scheduled transactions (formulas), quotes |

## Files, sessions and time

Detail: [`file-fidelity.md`](file-fidelity.md), [`sql-fidelity.md`](sql-fidelity.md),
[`api.md`](api.md#book-methods) (journal replay), [`api.md`](api.md#dates-times-and-the-local-time-zone) (time zones),
[`open-questions.md`](open-questions.md).

| What | GnuCash 5.17 | red-ale | Why |
|------|--------------|---------|-----|
| Content the reader does not recognise (XML elements, slot types, SQL tables or columns) | XML: an unknown element fails the load in most places, and is dropped silently inside business objects ([`notes/xml-format-core.md`](notes/xml-format-core.md), [`notes/xml-format-business.md`](notes/xml-format-business.md)); SQL: unknown tables and columns are left alone while the book is edited | Kept and written back; the book opens read-only, naming what it found | Never lose data; an edit could leave the book inconsistent with data red-ale cannot see |
| A Book feature GnuCash implements and red-ale does not | Opened for editing | Opened read-only (`unsupported-features`) | A partly-right edit is worse than none |
| Compression on save | Gzipped by default | Written the way it was read (gzipped or plain); `book.save` can override | Keeps the user's choice; GnuCash reads either |
| Formatting of an XML book GnuCash 5.17 did not write; XML comments and processing instructions; `gnc:count-data` | Rewritten in 5.17's layout; dropped; recounted | The same (not kept) | GnuCash's own save does the same; listed so it is not mistaken for a loss |
| Deleting an account a scheduled transaction or Budget refers to | Allowed; the references dangle | Refused, naming the scheduled transactions and Budgets | A dangling reference breaks the book |
| A commodity renamed while template accounts or template transactions use it | Not followed into the templates | Renamed there too; a commodity they use is not deleted | The templates would point at nothing |
| SQLite read-only open | Writes: sets the ISO-dates feature, upgrades tables, recovers `_back` tables, scrubs imbalances, even in `SESSION_READ_ONLY` | Leaves the file byte for byte as it was | The book is never changed unasked |
| SQLite normal open and close with no save | The same writes as above | Every row as it was; only the `gnclock` row comes and goes | As above |
| SQLite book with an older table version, a `_back` or `_merge` table, or a missing table | Upgraded, recovered or created on open | Opened read-only (`schema-version`, `unrecognised-content`) | red-ale never upgrades or repairs a schema |
| SQLite book in write-ahead-log mode, or a read-only open with a hot journal | Opened | Refused (`file-format`) | Reading either can write the file |
| When an SQLite book is written | Write-through: each commit in its own SQL transaction, not atomic per GnuCash transaction | At `book.save`, in one `BEGIN IMMEDIATE` transaction, checked against `gnclock` and `data_version`, read back, then committed | A save applies whole; another writer is detected before anything is written |
| Releasing the SQLite lock | Deletes every `gnclock` row once it finds its own | Deletes only its own row | Another program's lock is not red-ale's to clear |
| A Mutation an SQLite book cannot hold (deferred instances, lists in lists in Slots, times outside `(MINTIME, MAXTIME)`, strings with NUL, NaN doubles) | Saved, then lost or changed on the next load | Refused, with the reason | Never lose data |
| A transaction and a template transaction with the same GUID | Both kept, but the Book's one transaction collection finds only the one read or made last (`5.17:libgnucash/engine/qofid.cpp:95-106`); an XML save writes both (`5.17:libgnucash/backend/xml/io-gncxml-v2.cpp:1166-1199`); inserting the second into an SQL book fails on the `transactions` primary key (`5.17:libgnucash/backend/sql/gnc-transaction-sql.cpp:595-606`) | A Mutation that would make the pair is refused; an XML book read with one opens, but `book.save` refuses it and writes nothing until one of the two is removed (`invalid`, `data.reason` `shared-guid`) | One would hide the other in GnuCash; only red-ale's testkit has made such a pair ([`file-fidelity.md`](file-fidelity.md), "Writing") |
| An SQLite book with an unbalanced transaction | Scrubbed on load | Read as it is; `scrub.imbalance` repairs it when asked | The book is never changed unasked |
| Journal replay: a record with an unreadable line | Applied with what parsed: a bad date becomes `INT64_MAX`, an extra field shifts the rest, a string is cut at 255 bytes (`5.17:gnucash/import-export/log-replay/gnc-log-replay.cpp:142-254`) | Skipped whole | A partial transaction is a wrong one |
| Journal replay: an unbalanced replayed transaction | Balanced with an `Imbalance-<CUR>` split | Refused, or skipped with `skipInvalid` | Refuse rather than repair |
| Local day boundaries near a daylight-saving change | Approximates the zone file with one rule per year (`TimeZoneProvider::parse_file`, `5.17:libgnucash/engine/gnc-timezone.cpp:635-720`): local midnight an hour late on the autumn change in zones whose rules are in UT, and no daylight saving from 2037 | Follows the zone file's transitions exactly | The zone file is the true local time; stored dates are unaffected (decided for v0.3) |
| `GncDate` "today" | Reads the C library's zone, whose POSIX `TZ` sign is the opposite of GnuCash's own | One zone | One meaning of "today" |
| A `TZ` naming a file that is not TZif | Aborts | Read as a POSIX string, then falls back as GnuCash does | A bad environment is not a crash |

## Book options

Detail: [`api/book-options.md`](api/book-options.md).

| What | GnuCash 5.17 | red-ale | Why |
|------|--------------|---------|-----|
| A negative auto read-only day count (the dialog cannot enter one) | Reports the option as used but has no threshold date; a `g_assert` aborts (`Transaction.cpp:2389-2390`) | Turns the threshold off | No crash on a value another program wrote |

## Accounts

Detail: [`api/accounts.md`](api/accounts.md#deviations-in-short).

| What | GnuCash 5.17 | red-ale | Why |
|------|--------------|---------|-----|
| A name containing the separator `:` | The name entry silently drops the separator as it is typed (`gnucash/gnome-utils/dialog-account.c:1420-1447`) | Refused | No silent change to what the caller asked for |
| Name ties in display order | Locale collation | Bytewise | Deterministic |
| Changing an account's type with sub-accounts retyped too | Retypes them without checking their splits | Refused when a retyped sub-account's splits forbid the change | A sub-account's splits must fit its new type too |
| Deleting an account with splits, given no choice | The engine deletes the splits one by one and the imbalance scrub adds `Imbalance-<CUR>` splits | A choice is required (and `allowCommodityMismatch` where commodities differ); never leaves an imbalance | Refuse rather than repair |
| Deleting an account: sub-accounts' business references; a re-parented sub-account whose name clashes with its new parent's child | Not checked | Refused | A dangling reference or two accounts of one full name |
| `account.balance` with sub-accounts in other commodities | Converted at the latest price | Reported unconverted; `account.checkBalanceLimit` converts as GnuCash does | A balance is figures, not a guess at a rate |
| Colours | Scrubbed by the GUI (`Not Set`) | Checked and written as the dialog writes them; `Not Set` reads as no colour | No GUI scrub |
| Opening balance account lookup | Finds or creates the account (`gnc_find_or_create_equity_account`, `5.17:libgnucash/app-utils/gnc-ui-util.cpp:563-622`) | Only finds; never creates or flags one | A read never writes |
| The accounting period | A user preference | In red-ale's Configuration | Per-user state, not the book's |

## Commodities

Detail: [`api/commodities.md`](api/commodities.md#deviations-from-gnucash). Citations are to
`libgnucash/engine/` and `gnucash/gnome/` as the detail page gives them.

| What | GnuCash 5.17 | red-ale | Why |
|------|--------------|---------|-----|
| Deleting a commodity in use as a transaction currency, by a business object, or with prices | Allowed; prices where it is the currency are left dangling (`dialog-commodities.cpp:183`, FIXME) | Refused; prices only with `deletePrices` | A dangling reference breaks the book |
| Namespace `GNC_LEGACY_CURRENCIES` or containing `::` for a new security | Allowed | Refused | The first is a currency namespace to GnuCash (`gnc-commodity.cpp:1953`); `::` would make `NAMESPACE::MNEMONIC` ambiguous |
| A price-less commodity with no account but used by template transactions | Deletable | Refused | The template transactions would point at nothing |
| Converting with a currency that has no euro rate | Returns 0 | `invalid` | A zero is a silent wrong answer |
| Quote source or time zone of a commodity without quotes | Kept in memory until the book is closed | Dropped | GnuCash does not write them (`gnc-commodity-xml-v2.cpp:97-108`), so they would not survive a save anyway |
| Accounts of a commodity whose fraction changes | Keep the old SCU stored; after a reload they become non-standard at the old SCU (`Account.cpp:2710-2722`) | Accounts that followed the commodity's fraction store the new one | The book reads back as GnuCash showed it before the reload |
| The editor's user-symbol field | Shows the Display symbol and stores it back | Changed only when given | The editor's display value is not the user's choice |
| Quote time zone outside the editor's six-entry menu | Written back as local time when the dialog is saved | Kept | Never lose data |

## Prices

Detail: [`api/prices.md`](api/prices.md#deviations-from-gnucash).

| What | GnuCash 5.17 | red-ale | Why |
|------|--------------|---------|-----|
| Unknown source strings read from a file | Written back as `invalid` | Kept | Never lose data; GnuCash reads both the same |
| New prices with an unknown source | Stored as `invalid` | `invalid` error | What red-ale writes must read back the same |
| A price of a commodity in itself | Accepted | `invalid` | It means nothing |
| Editing a price onto a day where it would be rejected or a duplicate | The edited price is silently dropped | `conflict`, nothing changes | Never lose data |

## Online quotes

Detail: [`api/quotes.md`](api/quotes.md#deviations-from-gnucash).

| What | GnuCash 5.17 | red-ale | Why |
|------|--------------|---------|-----|
| Who fetches | Finance::Quote, in Perl | red-ale, from Quote services in the Configuration | No Perl; every URL configurable |
| A commodity whose source can't be fetched | Left out silently | Reported `unsupported-source` | A missing price has a visible reason |
| Order of requests | Hash-table order | Commodity id order | Reproducible fetches and tests |
| Timeout | None | `quotes.timeoutSeconds` per request | A hung service can't hold the fetch |
| Total failure from `gnucash-cli --quotes get` | Exit status 0, book saved (`gnucash-commands.cpp:374-378`) | Status printed, exit status 2, nothing saved | Failures are visible |
| Keys | Two preferences, else the inherited environment | Per service, else `keyEnv`, bound to the service's host | A key goes only where it was meant to |
| Network | Always on | Off until the Configuration turns it on | Offline by default |
| A quote that fails at one service | Finance::Quote's own modules decide | The next service of the source is asked | Several services can stand in for each other |
| Twelve Data's date | `timestamp`'s local date (`TwelveData.pm:143-145`) | `datetime`'s date, the exchange's | Differs only near midnight; the exchange's day is the trading day |

## Transactions

Detail: [`api/transactions.md`](api/transactions.md).

| What | GnuCash 5.17 | red-ale | Why |
|------|--------------|---------|-----|
| Changing the currency of a transaction with a split in a third commodity | Divides the split's old price by the rate (`Transaction.cpp:1293-1299`), so the commit adds an `Imbalance-<CUR>` split | Multiplies, so the transaction stays balanced; a cent of rounding imbalance is refused as `unbalanced` | Refuse rather than repair; where GnuCash's result balances, the two agree exactly |
| Reversing a void transaction | Allowed; the all-zero clone copies the void slots without the read-only mark | Refused (`conflict`); unvoid first | The clone would read as void yet be editable |
| Transfer from a non-currency account | The dialog takes the currency from the account's parent and puts a zero amount on the from split | Refused | A zero-amount split is a wrong record |
| Transfer with a zero or negative rate, or a zero amount received | Saved | Refused | As above |
| Prices from transactions | Recorded by the register after each commit (`gnucash/register/ledger-core/split-register.c:1856`) | Only when the caller asks (`recordPrice`) | Commit itself does not record them in GnuCash either |
| `transaction.find`: case-insensitive matching | Casefolds and normalises to NFC (`qofutil.cpp:54-97`) | Lowercases both sides | Differs only for characters such as `ß` |
| `transaction.find`: regular expressions | POSIX extended | Rust `regex` | Close to POSIX extended; neither has backreferences |
| `transaction.find`: text ties in the sort | `g_utf8_collate`, by locale | Code point | Deterministic |
| `transaction.find`: sort order | Not a total order for nums and, with the num field source on, two splits of one transaction | A total order (see [Reports: what they show](#reports-what-they-show), split order and nums) | A sort never fails or depends on hash order |
| `transaction.find`: `dateEntered`, `void`, `notContains`, `notEquals` | Registered as query parameters; not offered by the dialog | Offered | The engine supports them |
| Document link resolution with no path head configured | Falls back to the user's Documents folder, else the data folder | A relative link stays unresolved (`uri` is `null`) | Per-user folders are not the book's |
| Changing the document link path head | Offers to rewrite links (`gnc_doclink_pref_path_head_changed`, `5.17:gnucash/gnome-utils/dialog-doclink-utils.c:390-444`) | Changes only the setting; links stay as stored | The book is never changed unasked |

## Lots and capital gains

Detail: [`api/lots.md`](api/lots.md#deviations).

| What | GnuCash 5.17 | red-ale | Why |
|------|--------------|---------|-----|
| A gains transaction met for the first time in a session | Rewritten, date entered set to now (`5.17:libgnucash/engine/cap-gains.cpp:837-846`; `Split.cpp:1110-1142`) | Written only when its values, amounts, currency, posted date or lot differ | A scrub that changes nothing leaves the book unchanged |
| Merging cut pieces | Deletes the merged piece's gains transaction only if it already looked at that piece's gains this session (`5.17:libgnucash/engine/Scrub2.cpp:308-315`); after a fresh open the gain is counted twice | Always deletes it | A gain counted twice is a wrong figure |
| A gains split met before the sale it records | Makes a second gains transaction (`5.17:libgnucash/engine/cap-gains.cpp:586-612`, `:767-769`) | Follows the `gains-split` slot and updates the one it has | As above |
| Stock split currency | The user's default currency preference | `currency`, defaulting to the nearest ancestor account held in a currency; refused when there is none | Per-user state, not the book's |
| Lot title and notes | The lot viewer writes both whenever the selection moves, storing empty strings (`5.17:gnucash/gnome/dialog-lot-viewer.c:236-258`) | `lot.update` changes only what it is given; an empty string removes the slot | The book is never changed unasked |
| Deleting a lot with splits | The viewer deletes any lot that is not an invoice's | Needs `releaseSplits` | The release should be explicit |
| Stale gains shown in the viewer | Recomputed, which can write (`xaccSplitGetCapGains`, `cap-gains.cpp:1037-1065`) | `lot.get` reports the recorded gains and never writes; `lot.scrub` or `lot.computeGains` recompute | A read never writes |
| The order of a lot's splits | The order they joined in, kept only in memory | Book order (transaction, then split), the order GnuCash builds on load; text ties bytewise | Deterministic |
| Assigning a split that would overfill a lot | The viewer's Add button adds it whole (`dialog-lot-viewer.c:724-740`) | Cut, as `xaccSplitAssignToLot` does; the rest returned in `rest` | The engine's rule, not the button's |

## Trading accounts

Detail: [`api/trading-accounts.md`](api/trading-accounts.md#deviations-from-gnucash).

| What | GnuCash 5.17 | red-ale | Why |
|------|--------------|---------|-----|
| Unbalanced values outside the scrubs | Balanced by the commit | Refused | Refuse rather than repair |
| The top account's name | Translated, `_("Trading")` (`Scrub.cpp:532`) | Always `Trading`; a translated one is still found | English only |
| Commit order of several transactions in one Mutation | The order the operation touched them | Book order | Decides only which creates a new trading account first, and so sibling order |
| The split scrub on commit (`xaccTransScrubSplits`) | Run | Not run; the scrubs run it | Commit never repairs |

## Scrub (Check & Repair)

Detail: [`api/scrub.md`](api/scrub.md#deviations-from-gnucash).

| What | GnuCash 5.17 | red-ale | Why |
|------|--------------|---------|-----|
| Imbalance and orphan account names | Translated, `_("Imbalance")-<CUR>`, `_("Orphan")-<CUR>` (`Scrub.cpp:121`, `:464`) | Always `Imbalance-` and `Orphan-`; a translated existing account is not found, so a new English one is made beside it | English only |
| Order of an account's transactions | An `unordered_set` (`Scrub.cpp:96-106`) | Book order | Deterministic; decides only which creates a new account first |
| "All" | Reaches transactions through accounts (its load-time scrub repairs the rest) | Every transaction, including one whose splits are all orphans | The scrub covers what the load-time scrub would |
| Giving an account with no commodity a commodity (`xaccAccountScrubCommodity`); the business scrub Check & Repair ends with (`5.17:libgnucash/engine/ScrubBusiness.c:736-762`) | Run | Not modelled; such splits are left alone | Not yet ported |

## Reconciliation

Detail: [`api/reconcile.md`](api/reconcile.md#deviations-from-gnucash-summarised), auto-clear
[`api/reconcile.md`](api/reconcile.md#deviations).

| What | GnuCash 5.17 | red-ale | Why |
|------|--------------|---------|-----|
| Setting a split to `y` | `xaccSplitSetReconcile` leaves the date alone; only the reconcile window sets it (`5.17:gnucash/gnome/reconcile-view.c:931-937`) | Stamps it with `date`, or now; other states keep the stored date | A reconciled split always says when |
| `include-children` and `last-interval` | Written when the window opens | Written at Finish or Postpone; cancelling records nothing | The book is never changed unasked |
| Unbalanced transactions | Balanced by the commit-time scrub | Refused; use `scrub.*` | Refuse rather than repair |
| The interest-transfer and credit-card payment prompts after Finish | Offered | Not modelled; use ordinary transfers | GUI conveniences |
| Auto-clear's limit | 5 seconds (`MAX_AUTOCLEAR_SECONDS`, `gnucash/gnome/window-autoclear.c:142`; checked every 100,000 steps, `gnc-autoclear.cpp:48-67`) | 2²⁵ search steps | The same book always gives the same answer; either may stop where the other finishes |
| Auto-clear stopping before any solution | "The selected amount cannot be cleared." (`:236-241`) | `limit` | Says why it stopped |
| Auto-clear with a split amount off the SCU | The conversion fails silently and searches with an error code in place of the amount | Refused (`invalid`) | A wrong search is a wrong answer |
| Auto-clear's sign | Credit accounts' sign flipped (`gnc_reverse_balance`, `:136-137`, `:250-251`) | The raw sign, as everywhere in reconciliation | A display preference |

## Closing the books

Detail: [`api/close-book.md`](api/close-book.md#deviations).

| What | GnuCash 5.17 | red-ale | Why |
|------|--------------|---------|-----|
| An income or expense account with a balance in a non-currency commodity | Closed in its nearest ancestor's currency (or the user's default), writing the share count as the value with a zero amount | Refused (`invalid`) | That split would book a quantity of a security as money |
| A chosen equity account's descendant named after the currency but in another commodity | Stops on an assertion | Refused (`invalid`) | No transaction can be made |
| Rounding the account splits leaves the transaction unbalanced | The commit adds an `Imbalance-<CUR>` split (`5.17:libgnucash/engine/Transaction.cpp:1549-1558`) | Fails with `unbalanced` | Refuse rather than repair |

## Budgets

Detail: [`api/budgets.md`](api/budgets.md#deviations-from-gnucash).

| What | GnuCash 5.17 | red-ale | Why |
|------|--------------|---------|-----|
| Natural-signs scrub | Guessed and applied by the GUI after every open, read-only ones too | Only by `budget.scrubSigns`; Budget edits refused until then | red-ale never changes a book on open |
| A book with the feature and no Budgets | Feature unset on the next GUI open | Kept until `budget.scrubSigns` or a new first Budget | As above |
| Number of periods | Any positive count in the engine; 1–60 in the dialog | 1–60 | The dialog clamps a larger count and writes it back on OK, hiding amounts |
| Weekend adjustment, `once` | Read and computed; never written by the GUI | Read and computed; refused on create and update | The next Budget Options OK in GnuCash would change them |
| Raising the count again after lowering it | Hidden amounts reappear only after a reload | They reappear at once | red-ale reads the Slots, not a cache |
| New Budget's description | `Created: <date in the locale>` | `""` unless given | No invented text |
| A zero denominator | Unsets the cell | Refused (`invalid-params`) | A malformed amount is an error, not an unset |
| Estimate's start date | Today the first time | Required | With no dialog, a default of today would overwrite every amount with zero |
| Estimate overflow | The period skipped, or an error stored | Refused whole | Every Mutation applies whole or not at all |
| Default budget fallback | First in hash order | First in book order | Deterministic |
| Deleting an account with budget amounts | Allowed; the amounts stay in every Budget | Refused, naming the Budgets | [`file-fidelity.md`](file-fidelity.md#references-from-schedules-and-budgets) |
| Display sign | The GUI negates input and display for reverse-balance accounts | Natural signs on the wire | The preference is a display choice; clients apply it |
| Unsetting a cell that was never set | `KvpFrame::set_path` creates the account's frame on the way (`5.17:libgnucash/engine/kvp-frame.cpp:84-98`, `:133-141`), so an empty `<account-guid>` frame is written | Nothing changes | GnuCash reads the two books alike; an empty frame carries nothing |

## Scheduled transactions

Detail: [`api/scheduled-transactions.md`](api/scheduled-transactions.md#deviations-from-gnucash), Since Last Run
[`api/scheduled-transactions.md`](api/scheduled-transactions.md#deviations-in-since-last-run) (with citations). GnuCash
still proposes the duplicates after a postponement on its own next run while a deferred instance remains; red-ale
can't prevent that.

| What | GnuCash 5.17 | red-ale | Why |
|------|--------------|---------|-----|
| Since Last Run on open | Run by the GUI, auto-create instances created at once | Only when asked | red-ale never changes a book on open |
| Choosing and applying | A dialog, auto-create instances first | A proposal, applied as one Mutation | No GUI; Since Last Run applies whole or not at all |
| Instances after a postponed one | Proposed again on every run | Proposed once | A duplicate is money recorded twice |
| A failing formula | Transaction created with 0, schedule not advanced | Apply refused | No wrong record, no repeat |
| A postponed instance left postponed | The schedule advanced again on every run: remaining occurrences drop by one and the instance count resets (`5.17:libgnucash/app-utils/gnc-sx-instance-model.c:1510-1517`) | Nothing changes | With no duplicates after a postponement, advancing again would end a counted schedule early and set `i` back |
| A deferred instance with no occurrence left (its schedule was edited since) | Proposed | Not proposed | Nothing is due |
| An exchange rate of 0 on a split with a value, a currency that is not a currency, a template split in the root or a commodity-less account | Created | Refused | A wrong or unbalanced record |
| Instances before the read-only threshold | Created, unchecked | Flagged; created only when named | The caller sees it first |
| The reminder rule | Other instances' states changed to keep it | Choices that break it refused | A call changes only what it names |
| Postponing in an SQLite book | Accepted, then lost on the next load | Refused | No silent loss |
| A template split in a placeholder account | Created | Refused | red-ale's commit rules hold for every door |
| Template numerics on open | Scrubbed by the GUI | Kept as read; set correctly when red-ale writes a formula | red-ale never changes a book on open |
| Global calculator variables | Searched, depending on parser start-up | Never | Per-user state, not the book's |
| Function calls | Any Guile `gnc:` procedure, in doubles | fin.scm's financial functions, exact; non-integer powers and other names refused | No Guile; money never passes through a float |
| Formula errors | "Stack underflow" for a failed call | The cause named | |
| Overflow and division by zero in a formula | Numeric error, used as 0 | Refused | A wrong amount is worse than a refused one |
| Formula text from Schedule from transaction and the loan | The locale's grouped form | The Locale's decimal form, no grouping | A group separator reads differently in another locale |
| Loan assistant defects (§6.3) and two-significant-figure other payments | As in GnuCash | Not copied | See "The loan assistant" |
| Loan template currency | The user's default currency | The loan account's | red-ale has no per-user default currency in the Book |
| Editor confirmations (duplicate name, never runs, unprovable balance) | Asked | Warnings; a provably unbalanced template refused | No dialog |

## Business: contacts

Detail: [`api/business-contacts.md`](api/business-contacts.md#deviations).

| What | GnuCash 5.17 | red-ale | Why |
|------|--------------|---------|-----|
| A job's new owner once it has invoices, bills or orders | Allowed | Refused | It would silently move those documents, posted ones included |
| Billing terms with negative days, a proximo due day outside 1–31, a discount day outside 0–31, a cutoff outside −31 to 31 or a discount outside 0–100 | Some accepted by the dialog | Refused | GnuCash computes an undefined due date from them |
| A tax table with no lines | The dialog can leave one empty | Refused | GnuCash deletes empty tables on its next load, orphaning references |
| An employee's `creditCardAccount` that is not a `CREDIT` account | Accepted by the engine | Refused | The GUI offers only credit card accounts; posting charges card payments to it |
| A counter format `printf` would refuse (two dots in the width and precision) | Passed to `printf`, undefined results | Falls back to the default format; `counter.set` refuses it; the `'` and `I` flags are accepted and ignored | red-ale writes ASCII digits |
| `counter.set` with a format GnuCash would refuse when numbering | The book options store any text, then number with the default | Refused | No silent fallback |

## Business: documents

Detail: [`api/business-documents.md`](api/business-documents.md#deviations).

| What | GnuCash 5.17 | red-ale | Why |
|------|--------------|---------|-----|
| Deleting a document | The GUI discards only a new one | `invoice.delete` deletes any unposted document, keeping GnuCash's entry and refcount rules | Agents need to undo a draft |
| New entry defaults | `gncEntryCreate`'s in the engine; the entry ledger's in the GUI, with tax-included from a global preference | The entry ledger's; a `useGlobal` tax-included setting gives yes | The GUI is how users make entries; the preference is per-user, so its shipped default (yes) is used |
| The default tax table's option name | Written as `Default Customer Tax Table`, read as `Default Customer TaxTable` (`5.17:libgnucash/engine/gncTaxTable.c:680-699`); likewise for vendors | Both read, the written spelling first | GnuCash writes one and reads the other |
| Entry fields with no ledger column on a kind of document, and accounts the ledger does not offer | Stored by the engine | Refused | GnuCash would ignore or mis-post them, and users could not see or fix them |
| A new bill or voucher entry on an order | Allowed by the engine | Refused | Order entries use the invoice side; a bill-side entry would show empty |
| Duplicating a document | Opens the document's dialog | Date and ID taken as params, following the Find results path | No dialog |

## Business: posting and payments

Detail: [`api/business-posting.md`](api/business-posting.md#deviations-from-gnucash).

| What | GnuCash 5.17 | red-ale | Why |
|------|--------------|---------|-----|
| Posting that meets a missing price or account | Posts as it goes and can stop halfway, or leaves an entry with no account out of the splits while counting it in the total (then an Imbalance split) | Checks everything first and refuses | Every change applies whole |
| Posting and card accounts not in the document currency | The dialog offers only such posting accounts; other card accounts get a zero amount from `xaccSplitSetBaseValue` | Refused | A zero amount is a wrong record |
| The document's currency at posting | Reset to the owner's by the dialog | Posts in the document's currency; a document with none is refused | No silent change |
| `last-posted-to-acct` slot on the owner | Written | Not written | A GUI convenience |
| Results past 64 bits | Approximated | Refused | Money is exact |
| Lot split order | The order splits joined the lot this session | Book order at each operation, the order after loading | Matters only for ties in the choice of split to move |
| Strings written (`Invoice`, `Bill`, `Expense`, `Credit Note`, `Payment`, `Lot Link`, the link memo, `Extra to Charge Card`, the read-only reason) | Translated | English | English only |
| Peer-split (`lot-split`) slots when lot-link splits are merged | Maintained | Not maintained | Business lots never have them |
| Payment from a void, read-only or lot-link transaction | Void and read-only: splits deleted all the same; lot-link: let through with no transfer split and a zero amount | Refused | A zero-amount payment is a wrong record |
| Payment from a transaction whose transfer and A/R or A/P commodities differ | The dialog proposes an amount converted at a price of its choosing (`xaccSplitConvertAmount`) | Needs `amount` | No guessed rate |
| A `date-posted` slot on a transaction turned into a payment | Left as it was (`5.17:libgnucash/engine/Transaction.cpp:1897-1903`) | Set to the new date | The slot would name another day than the posted date |

## Import

Detail: [`api/import.md`](api/import.md#deviations-from-gnucash). Citations are to `gnucash/import-export/` unless they
name another directory.

| What | GnuCash 5.17 | red-ale | Why |
|------|--------------|---------|-----|
| When choices are made | In a window, applied row by row | A proposal, applied as one Mutation | No GUI; an import applies whole or not at all |
| Learning on a hand-picked account | At once, kept if the import is cancelled | At apply only | A discarded proposal changes nothing |
| Converting old Bayesian data | On any lookup or listing | On the next write of Bayesian data or map deletion | Read methods never write |
| Comparing amounts in the matcher | Doubles, 1e-6 tolerance | Exact | Money never passes through a float |
| Folded text comparison | Locale collation | Bytewise | As every red-ale comparison |
| `skip` with `enable-skip` off | Unavailable | Allowed in `import.apply` | Skipping changes nothing in the Book; `enable-skip` still governs the matcher's choice |
| Encoding | Guessed (CSV), or the locale's charset | The client's `encoding`, else the layout's, else UTF-8; a byte-order mark wins | No guessing about what the user meant |
| Bytes invalid in the encoding | Dropped (CSV: `boost::locale::conv::to_utf`'s default `skip`, `5.17:gnucash/import-export/csv-imp/gnc-tokenizer.cpp:108`) | `import-file` error naming the line | A dropped byte can change a name or an amount |
| Preview with error rows | The assistant blocks until "Skip errors" is ticked | Proposed without them; `import.apply` needs `skipErrors` | The caller sees the whole proposal first |
| Unparseable numbers (QIF, business), and QIF numbers padded with spaces | 0 | Error row | A wrong amount in a book is worse than a refused row |
| CSV splits failing their essentials | Dropped, logged | Error row | The user sees every row that isn't imported |
| The last CSV transaction's void reason | Ignored | Applied | A GnuCash bug |
| QIF invalid days, unknown or missing investment actions, ambiguous price dates | Today; imbalance; failure | Error rows; decided by `dateFormat` | As above |
| QIF account tree merge | Built apart, then merged; siblings that differ in code, colour or notes stay with one name | Merged as each account is made; a new sibling with an existing name becomes `<name> <n>` | Two accounts of one full name can't be told apart by name |
| QIF maps' order | Hash order, sorted by locale collation | By QIF name, bytewise | Deterministic output |
| QIF duplicates ticked | None by default (`qif-merge-groups.scm:150-152`); only among the candidates; a voided one is kept (`xaccTransDestroy` refuses it) | A transaction with candidates defaults to `skip`, its first candidate selected, with a `possible-duplicate` warning; `add` stays a choice; `skip` on any transaction; a voided one is removed | Unticked is a window's default: with no window, a file imported twice must add nothing the second time (as the matcher auto-clears confident matches). The caller's skip is honoured whole |
| QIF dates ambiguous in several files | Asked file by file | One `dateFormat` for the preview; files with no format in common are refused | One preview, one choice |
| CSV account rows after one that doesn't fit | Ignored silently | Read; that row is an error | As above |
| CSV account rows with a missing parent, unknown commodity or type, or a name that isn't the full name's end | Top-level account; skipped; `INVALID` account; cut at the wrong place | Error rows | Nothing lands where the file didn't say |
| New CSV accounts breaking `account.create`'s rules | Created | Error rows | The Book's invariants hold for every door |
| An unparseable CSV account colour | Clears the colour | Leaves it, with a warning; colour names not recognised | A typo shouldn't erase data; no GDK colour table |
| A CSV account row that changes nothing | Counted updated | `skip`, with `not-updated` warnings for cells GnuCash never applies | The proposal says what will change |
| CSV price lines failing their essentials | Skipped, logged | Error rows | The user sees every line that isn't imported |
| A replacing CSV price the price database refuses | The old price is already removed | Error row; the old price stays | Never lose data |
| A CSV price layout with a default and its column; an unknown default commodity | The assistant clears one when the other is chosen; the preset loader throws | Refused; `not-found` | No GUI to clear one; a layout says one thing |
| `EnableEscape` in a CSV price preset | Not re-applied (`5.17:gnucash/import-export/csv-imp/gnc-import-price.cpp:313-314`) | Honoured | A GnuCash bug |
| The `locale` date format | ICU's lenient parse of the medium date pattern | The ICU4X pattern, read strictly; non-Gregorian calendars refused | A rolled-over or guessed date is a wrong date |
| Business lines over 999 bytes | Split in two | Read whole | A GnuCash limit |
| Business notes on a new document's first entry | Un-escaped twice | Once | A GnuCash bug |
| Business dates | Checked by `strptime`, read by `qof_scan_date` | One parser | The date checked is the date stored |
| Custom regexes (business, accounts) | Offered | Not offered | GLib regex syntax with duplicate names; a regex missing a field crashes GnuCash |
| Owner header row | Imported as an Owner | Warned and skipped by default | The file's header is not a customer |
| Business number syntax | Guile functions and variables; a group separator anywhere | Exact arithmetic only; a group separator after the decimal point is an error | No Guile; a misplaced separator is a misread amount |
| Business dates with a day the month lacks, or digits after them | Taken by `strptime`, rolled by `qof_scan_date` | Invalid, so fixed or an error as GnuCash fixes an invalid date | The date stored is the date written |
| Unknown business tax table | The entry gets none, silently | Error row | A silently untaxed entry is a wrong total |
| Entry date fixed to an invalid opening date | The entry keeps no date | The fixed opening date (today) | Every entry has a date |
| Entry accounts or postings red-ale's rules refuse | Created or posted | Error row; left unposted with `not-posted` | The Book's invariants hold for every door |
| A business ID a credit note already has | A second document with the ID | Created, with a `duplicate-id` warning | The caller should know |
| Business lines ending CRLF; characters XML can't hold | CR kept (a quoted last field then fails to match); stripped silently | CR removed; stripped with a `stripped-characters` warning | Windows files read as written; nothing changes unseen |
| Owner rows with neither company nor name | Dropped, counted | Error row | The user sees every row that isn't imported |
| An Owner row that changes nothing | Updated (no visible change) | `skip`, with an `unchanged` warning | The proposal says what will change |
| Vendor shipping columns | Ignored silently | Ignored, with an `ignored-columns` warning | The caller should know |
| The customer `semicolonQuoted` layout's last field | Read with `[^;]*`, keeping its quotes (`dialog-customer-import-gui.c:303`) | Read as the other fields | A GnuCash bug |
| Default names (QIF), headers | Translated | English | No translations |

## Export

Detail: [`api/export.md`](api/export.md#deviations-from-gnucash).

| What | GnuCash 5.17 | red-ale | Why |
|------|--------------|---------|-----|
| Header text and placeholders | Translated | English msgids | red-ale has no translations |
| Sibling order in the tree export | Names by locale collation | Bytewise | As every red-ale sort ([`reports.md`](reports.md) §14) |
| Export to an existing file | Overwrites after asking | `file-exists` | Never overwrite a file the client may need |
| How the exported book is written | In place, no temporary file; write errors after opening ignored (`gnc-xml-backend.cpp:343-355`) | Temporary file and rename; any error fails the call and leaves no file | A half-written book looks like a book |
| Exporting accounts to SQLite | Writes nothing, reports success | Not offered (SQL books come in v0.4) | |
| The open book's journal during Export accounts | Moved beside the exported file | Untouched | The journal belongs to the open book |
| The register export | From a register's query | No method | red-ale has no registers; `export.csvTransactions` covers it by account |

## Reports: running and saving

Detail: [`api/reports.md`](api/reports.md#deviations-from-gnucash), [`reports.md`](reports.md) §15 (importing
GnuCash's report files).

| What | GnuCash 5.17 | red-ale | Why |
|------|--------------|---------|-----|
| Where configurations and style sheets live | `saved-reports-2.8`, `stylesheets-2.0` (Scheme) | The Configuration (JSON); GnuCash's files are imported as data, never run | red-ale never runs Scheme |
| CSV export target | A file the GUI writes | CSV returned; only the CLI writes, on its side | The server never writes caller-named paths |
| Name clash on save | A number is appended | `conflict` | The caller gets the name it asked for |
| Invalid option value | Logged, option unchanged | `report-option` | A typo never runs a different report |
| A Multicolumn View's children | Report instances of the session; a saved configuration's options are copied into the new instance | Template or saved configuration GUIDs; a saved configuration child follows its later changes | Children stay put across restarts (`notes/report-statements-charts.md` Q4) |
| The Dashboard (v0.6) | A menu item (`5.17:gnucash/gnome/report-menus.scm:113-121`) that builds a new Multicolumn View and four child report instances at each click (`5.17:gnucash/report/reports/standard/dashboard.scm:40-65`); the view has no GUID of its own, and saving it makes a custom report under a new GUID | A read-only built-in saved configuration of the Multicolumn View, under the menu item's GUID: `report.list` lists it with `readOnly` and the menu fields, `report.run` runs it by name or GUID, and it changes only as a copy | A report is named by a GUID or name, and a saved configuration is what red-ale has for a configured report |
| `gnucash-cli --report run` failure | Exit status 0, error on standard error (`5.17:gnucash/gnucash-commands.cpp:234-250`) | An error response; the CLI exits with status 2 | Failures are visible |
| Export target | A file the GUI or CLI writes | HTML returned; only the CLI writes, on its side | The server never writes caller-named paths |

## Reports: what they show

Detail: [`reports.md`](reports.md#14-deviations-from-gnucash). Section numbers (§6, §15, "section 2") are
`reports.md`'s; report paths (`standard/…`, `report/…`, `example/…`) are under `gnucash/report/reports/` and
`gnucash/report/` at 5.17. "(v0.5)" and "(v0.6)" mark rows added with those versions' reports.

| What | GnuCash 5.17 | red-ale | Why |
|------|--------------|---------|-----|
| Amounts past 64 bits | Fail at `gnc_scm_to_numeric`: an empty cell or an error (`5.17:bindings/guile/gnc-engine-guile.cpp:1709-1727`) | Exact, printed | Figures within 64 bits are the same |
| Floating point (day fractions, pie percentages, chart values) | Doubles | Exact, rounded as GnuCash displays | No floats anywhere in red-ale |
| Bad option values | Logged and ignored | `report-option` error | A typo never runs a different report |
| A script's `set-default-value` or `make-internal` on an option that does not exist | Ignored (`5.17:bindings/guile/gnc-optiondb.i:1500-1503`; `5.17:libgnucash/engine/gnc-optiondb.cpp:266-272`); the General Ledger and Reconciliation Report rely on it | `report-script` error; the translations leave those calls out | A misspelt name in a user script is found; the figures are the same |
| A Multicolumn View's children (v0.5) | Report instances of the session, made by "Add" with a copy of a saved configuration's options (`report/report-core.scm:389-409`); an unknown id fails the whole view (`standard/view-column.scm:84-85`) | Template or saved configuration GUIDs (§6, report placements); a saved configuration child is a reference; an unknown GUID or invalid child options set by a caller are `report-option` | No sessions of report instances in red-ale; a reference stays put across restarts (`notes/report-statements-charts.md` Q4, Q7) |
| A Multicolumn View's child that cannot run (v0.5) | An unknown report id fails the whole view (`gnc:report-set-stylesheet!` outside the error handler, `standard/view-column.scm:84-85`); a failing child shows "Report error" in its cell (`:87-97`) | Every child that cannot run (gone, failing, too deep, past 100, a cycle) shows "Report error" in its cell; a child past an engine limit stops the run | One stale reference does not hide the other children |
| A Multicolumn View's links (v0.5) | "Edit Options" and "Single Report" under each child (`standard/view-column.scm:115-130`) | "Single Report" only, a report link | `report.options` edits options; red-ale has no instances |
| A saved Multicolumn View inside a saved view (v0.5) | Saved with its report-list of the session's instance ids and none of its children (`report/report-core.scm:567-598`); reloaded, it finds no report by those ids (the whole view fails) or another one | Imported with no children; its report-list is listed as `unsaved-children` | The children were never saved; an empty view is what can be kept (§15) |
| A split query's description pattern that is not a regular expression | The criterion is dropped (`qof_query_string_predicate` returns null, `5.17:libgnucash/engine/qofquerycore.cpp:319-334`) | `report-script` error | A typo never matches every transaction |
| A split query's regular expressions and case folding | POSIX extended expressions; `g_utf8_casefold` | Rust `regex`; lowercase, as `transaction.find` | Same results for patterns both dialects read alike |
| The Transaction Report engine's account and transaction filters (v0.5) | POSIX extended expressions (`make-regexp`, with `regexp/icase` when case-insensitive), `string-contains-ci` (`report/trep-engine.scm:2210-2254`, `:2439-2446`) | Rust `regex` through `make_regexp` (section 2); case-insensitive substrings compare lowercased | As the split query's row |
| Name comparisons in the Transaction Report engine's sorts (v0.5) | Locale collation: `gnc:string-locale<?` of whole full names in the custom sort (`report/trep-engine.scm:2467-2468`), `g_utf8_collate` per path level in the query's account-name sort (`5.17:libgnucash/engine/Split.cpp:1687-1709`) | Bytewise, whole full names in the custom sort and per path level in the query's (`notes/report-transaction-engine.md`, fact 5) | As "Name sorting" |
| A split query sorted with no first key (the Transaction Report's Primary Key "None") | Not sorted: hash order (`5.17:libgnucash/engine/qofquery.cpp:492-497`, `:792-797`) | Split order | Determinism; GnuCash's order is undefined |
| A split query's splits equal at every sort level | The order the query collected them in, hash order (a stable merge sort, `qofquery.cpp:792-797`) | Split order | Determinism; the Transaction Report always sorts by split order third, so its splits never tie |
| Subtotal Table rows or columns with equal sort values and different labels (two accounts with the same code) (v0.5) | Hash order (`report/trep-engine.scm:2086-2090`, `:2118-2119`) | Ordered by label, then as they came | Determinism |
| The General Journal's query (v0.5) | Stored as the default of the Register report's `__reg/query` option (`standard/general-journal.scm:57-79`; `standard/register.scm:327-328`) | Built by the script; there is no query option | No query option type; GnuCash's GUI never changes it for the General Journal |
| `__reg/journal` false in the General Journal (v0.5) | The Register report's non-journal layout (`standard/register.scm:91-102`, `:201-217`) | `report-script` error | Only journal mode is translated (`lib/register`); the option is internal and GnuCash's dialog never shows it |
| General Journal total rows (v0.5) | The label, then the total in one cell spanning from the second column to the amount column (`standard/register.scm:464-471`); `" "` cells under the date, num and balance | The total in its amount column, empty cells before and after; empty cells for `" "` | The result names columns; the figures are the same |
| "Show Account Description" with an Other Account key on a split transaction (v0.5) | A Scheme error: `xaccAccountGetDescription` of no account (`report/trep-engine.scm:1828-1829`) | No description | The report runs |
| The Transaction Report's informal Debit/Credit headers (v0.5) | Debit and Credit under the "use formal accounting labels" preference (`5.17:libgnucash/engine/Account.cpp:4081-4082`) | Always the friendly labels, GnuCash's default | No such preference in red-ale (`notes/report-transaction-engine.md` §7 Q5) |
| Split order with the num field source on | `xaccSplitOrder` is not a total order: two splits of one transaction compare by memo, then action (`5.17:libgnucash/engine/Split.cpp:1559-1597`), splits of different transactions by their actions as nums (`:1548-1556`), so splits A1 < A2 of one transaction by memo, with A2's action below A1's, form a cycle with a split B of another transaction on the same day whose action falls between (A2 < B by date entered, B < A1 by action); the sort's result follows hash order, and Rust's sorts may panic on such a comparator | Two splits of one transaction compare their actions as nums first, then as GnuCash does: a total order (`ops::transactions::order::split_order`). Splits of different transactions, and two of one transaction whose actions are equal as nums, are ordered as GnuCash orders them | A user's book never crashes a sort; the num field source is about ordering across transactions, which is kept |
| Nums (`order_by_int64_or_string`, `5.17:libgnucash/engine/Transaction.cpp:1773-1791`) | Two nums compare by their `strtoull` numbers when both have a non-zero one, else as whole strings, which cycles: `"-3" < "0" < "5" < "-3"` | Nums without a number that sort as text below `"1"` first, then nums with a number by number and rest, then the others as text (`order::num_order`). It differs only for a num with a number that starts below `"1"` (`" 10"`, `"+5"`, `"-3"`, `"05"`) against one without that also starts below `"1"` (`"0"`, `" "`) | A total order; usual nums (`""`, `"1"`, `"10"`, `"abc"`) are ordered as GnuCash orders them |
| The split query's `accountFullName` sort under two sibling accounts of the same name | Equal at that level (`xaccSplitCompareAccountFullNames`, `Split.cpp:1687-1709`), which is not an order once the siblings' sub-accounts are compared too | The names of the levels below decide | A total order |
| Saved configuration name clash | A number is appended on save | `conflict` | The caller gets the name it asked for |
| User text in style sheets | Inserted as raw HTML | Escaped | No injected markup in exported reports |
| CSS style sheet text | Default CSS on `</style` | Refused on `</style`, `@import`, `url(`, `image(`, `image-set(`, `src(`, `\` | Exported pages load nothing |
| Hover labels and chart look | Chart.js, `Intl.NumberFormat` | Static SVG, red-ale's formatting under the Locale | No script; deterministic bytes |
| Drill-down links | Report instances made at render time | Described in the result; no `href` in HTML | Clients run them with `report.run` |
| Document links (v0.5) | A `gnc-register:` link naming the transaction or invoice, opened by GnuCash (`report/html-utilities.scm:96-100`) | The document's URI in the result; text only in HTML | An exported page never points at a user's files |
| Title of the account pie charts | None: the document keeps its empty title (`report/html-document.scm:131`); the chart's title and the warnings' heading name the report | The report name, with its averaging suffix | A result has a title, and its warning blocks leave out the heading that names the report |
| Average Balance, Price Scatterplot and day-of-week pie titles, and the budget reports', the Transaction Report engine's and the Transaction Breakdown's warnings | Untitled documents (`example/average-balance.scm:291`, `standard/price-scatter.scm:146`, `standard/budget.scm:661-671`, `standard/budget-balance-sheet.scm:361-371`, `standard/budget-income-statement.scm:384-410`, `standard/budget-flow.scm:270-281`, `standard/budget-barchart.scm:289-298`; the Budget Chart's charts, `:313-316`; `report/trep-engine.scm:2501-2531`, `:2596-2605`; `standard/txn-columns.scm:143-156`, the title set only for the table at `:246`; `example/daily-reports.scm:144`, `:249-286`) | Titled with the "Report name" option (the budget reports' warnings: with the name the warning gives) | red-ale's warnings leave out the report-name heading GnuCash gives them; the title names the report |
| Unordered sets (`gnc-accounts-and-all-descendants`, which the Cash Flow and the day-of-week reports also use, `standard/cash-flow.scm:159`, `example/daily-reports.scm:175-177`, `:196-198`; the Securities pie) | Hash order | A fixed order | Determinism; no figure changes |
| Name sorting | Locale collation | Bytewise | As v0.1 |
| Date interval in titles under the `locale` format and the C locale | ICU's interval form for en_US_POSIX (`1/1/2026 – 12/31/2026`) | `<from> to <to>` | Output under C stays as it was before Locales |
| ICU data: the first day of the week, date intervals | The ICU the build links (ICU 74, CLDR 44, for the oracle) | ICU4X 2.4's compiled CLDR | Differences only where CLDR changed between them; the multi-day intervals of every supported locale sampled matched ICU 74's. A one-day interval prints ICU4X's `YMD` short pattern (`05.03.2026` under de_DE) where ICU 74 prints its `yMd` pattern (`5.3.2026`) |
| ICU dates outside 1400–9999 | Formatted | `<from> to <to>` | GnuCash's dates cannot hold them (`5.17:libgnucash/engine/gnc-datetime.hpp:240`); ICU4X 2.4's range formatter fails on years that need an era |
| `"%s to %s"` in titles | Translated through gettext | English | red-ale has no translations |
| Category chart series under "Percentage chart" | Fractions, shown ×100 by Chart.js's percent style (`standard/category-barchart.scm:611`, `:622-623`) | Percentages | A `percent` axis shows its values as they are |
| Accounts with no commodity (the trading accounts' parents in a book with no root currency, `5.17:libgnucash/engine/Scrub.cpp:493-516`) in the Balance Sheet, Income Statement, Equity Statement, Trial Balance and the Multicolumn statements | Their zero balance is added under `#f`; the exchange-rates table fails sorting the missing commodity (`report/report-utilities.scm:242-251`) | Left out of the balances and the rates table | The same figures and rates, and the report runs |
| Statement and account-table layout | Spacer and padding cells place amounts in columns (`standard/balance-sheet.scm:444-447`, `standard/equity-statement.scm:422-423`, `report/html-acct-table.scm:870-923`), and narrow indent cells and empty cells the Multicolumn statements' names and amounts (`standard/balsheet-pnl.scm:346-369`); the two-column forms nest tables in one row, as do the Equity Statement's exchange rates, under a header row of the two dates (`standard/equity-statement.scm:473-491`) | Rows with roles, depths and staircase levels; two columns are a `group` of tables, and so are the two rates tables, each titled with its date | The result carries data, not markup; the renderer draws the staircase |
| Trial Balance work sheet: Net Income/Loss figure | In the Adjustments column, by `add-labeled-amount-line`'s padding (HAT:899-901), not the Income Statement column `standard/trial-balance.scm:821-826` names; GnuCash's test reads it there (`standard/test/test-trial-balance.scm:136-138`) | In the Income Statement column | The result names columns; GnuCash's position is an HTML artifact |
| Trial Balance amounts with two or more account columns | Shifted left of their Debit/Credit headers by the same padding | In their columns | The same |
| Trial Balance with only TRADING accounts selected | Fails at `standard/trial-balance.scm:520-523`, reading the columns of a table with no row | `report-script` error saying only TRADING accounts are selected | Fails as GnuCash does, with a reason |
| Trial Balance layout | A spacer row of 60-pixel cells and `th` header rows in the table body (`standard/trial-balance.scm:523-573`) | Column labels "Account Name", "Debit" and "Credit"; the work sheet's five headings a first `heading` row of cells two columns wide | Layout, not data |
| Cash Flow's "Selected Accounts" | A `<ul>` of register links (`standard/cash-flow.scm:253-262`) | A `list` block whose items name their accounts | The result carries data and keeps the links |
| The cash-flow calculation's null-account warning | Printed to standard output (`5.17:gnucash/report/reports/cash-flow-calc.scm:65-67`) | The report log | Scripts have no standard output |
| Multicolumn statements' exchange rates | All rates of a column in one title-row cell, separated by line breaks (`standard/balsheet-pnl.scm:770-797`) | One row per commodity under the "Exchange Rates" heading: the one-unit monetary, then a price cell per column, or `"<symbol> missing"` | A cell holds one content; the figures and texts are the same |
| Multicolumn links' main currency on a tie | Hash order (`standard/balsheet-pnl.scm:804-812`) | The first in the accounts option's order | Determinism |
| A Multicolumn P&L header whose end is before its start (dates given swapped without a period; `pricedb-latest` with a period after now) | `<start> to <end>` (`standard/balsheet-pnl.scm:1140-1153`) | The same words in a text cell, not a date cell with `until` | A period's `until` may not be before its date |
| "Include Trading Accounts in report" of the Cash Flow and the Cash Flow Barchart | Works backwards since commit `2215dfc5e1`: off counts trading splits, on leaves them out (`cash-flow-calc.scm:68-70`); the help says the opposite (`standard/cash-flow.scm:99-101`) | The same figures; the help says it works backwards | Figures match 5.17; revisit if GnuCash fixes it |
| GnuCash's saved-reports and style-sheet files | Evaluated at start-up | Imported on request, as data (section 15) | They are Scheme programs; red-ale never runs Scheme |
| An entry of those files red-ale cannot import | An error while evaluating it | Skipped with its reason; the rest are imported | One entry does not cost the others |
| Advanced Portfolio's rates at a tie (v0.5) | Rounded from a double: 3.625 % prints `3.62%` (`standard/advanced-portfolio.scm:914-922`) | From the exact rate, half away from zero: `3.63%` | No floats ([`notes/report-investments.md`](notes/report-investments.md) Q1) |
| Advanced Portfolio's rates of 1,000 % or more, and under another Locale (v0.5) | Guile's `~,2f`: no grouping, `.` always | A number cell: grouped, with the Locale's separators | Rates are number cells, so clients keep the exact figure (the note's Q3) |
| Advanced Portfolio's investment arithmetic past 64 bits (v0.5) | `gnc_numeric` rounds to fit or fails to `#f` (`5.17:libgnucash/engine/gnc-numeric.cpp:71-85`; `5.17:bindings/guile/gnc-engine-guile.cpp:1729-1734`) | Exact | Same within 64 bits |
| Advanced Portfolio's link to an inverted price (v0.5) | A temporary price not in the book (`gnc-price-invert`, `standard/advanced-portfolio.scm:331-332`, `:900-903`) | The price stored | A link must name a real price |
| Advanced Portfolio's division-by-zero error and rule (v0.5) | Raw HTML before the table; a rule cell 17 columns wide whatever the table has (`standard/advanced-portfolio.scm:51-55`, `:1101-1106`) | A `warning` block titled "Error"; a rule over the table | Layout, not data |
| IFRS cost basis: a transaction with no stock amount and no units held after it (v0.5) | A Guile error, `(< 0 #f)` (`standard/ifrs-cost-basis.scm:417-418`) | `report-script` error naming the transaction | The same failure, with a message (`notes/report-investments.md`, Q11) |
| Transaction Breakdown's `[value] amount` (v0.5) | One cell of text, the value in the transaction's currency in brackets and the amount in the account's commodity linked to the split (`standard/txn-columns.scm:228-235`) | A `commodities` cell of one line: the amount, `converted` the value; the cell linked to the split | The pair is a monetary and its value; the figures are the same |
| IFRS cost basis: a transaction with no stock amount while units are held (v0.5) | A split link around `<boolean> #f`, the fallback rendering of #f (`report/html-document.scm:238-245`, `standard/ifrs-cost-basis.scm:485`) | An empty cell | A figure that is none is an empty cell everywhere else in the report |
| Investment Lots' percentages of 1,000 % or more, and under another Locale (v0.5) | Guile's `~,1f`: no grouping, `.` always (`standard/investment-lots.scm:1141-1162`) | A number cell: grouped, with the Locale's separators (`37,582.1%`) | As Advanced Portfolio's rates: clients keep the exact figure |
| Investment Lots' link to an inverted price (v0.5) | A temporary price not in the book (`gnc-price-invert`, `standard/investment-lots.scm:620-636`, `:944-962`) | The price stored | A link must name a real price |
| Investment Lots: a sale in the window from a lot with no purchase (v0.5) | A Guile error: `gnc:date-year-delta` of `'()` (`standard/investment-lots.scm:1859-1863`) | No CAGR: an empty cell, the rest of the row as computed | The report runs; the CAGR is undefined as when the basis is (`notes/report-investments.md`, Q6) |
| Investment Lots: an untitled lot with "Warn if a lot has a blank title" (v0.5) | A Guile error: `string-trim` of the `#f` that `gnc-lot-get-title` gives (`standard/investment-lots.scm:1577-1580`) | The blank-title warning | An untitled lot is blank; the report runs |
| Investment Lots: an account total's shares when the account has no lot (v0.5) | Printed with the default print info of a commodity of `'()`: the locale's default currency (`standard/investment-lots.scm:810-813`) | Printed in the report's currency | red-ale has no default-currency preference; the report's currency stands in |
| Investment Lots' chart with no account selected (v0.5) | A chart it never set up, with no data (`standard/investment-lots.scm:2266-2298`) | No chart | There is nothing to draw |
| The Printable Invoice's page (v0.6) | No title, and one `div.main-table` holding a two-column table (`standard/invoice.scm:797-869`) whose entries table has rows of different widths: *n* cells for an entry, 1 + max(3, *n*) for a total, max(3, *n*) for a payment (`:601-606`, `:670-678`) | An empty title; the main table's rows as blocks in order: a heading of class `invoice-title`, a group per layout row (the right-hand block's class starting `div-align-right`), the entries table and the notes. The entries table declares 1 + max(3, *n*) columns, the shown ones headed, and pads shorter rows with empty cells at the end | The report result has no layout tables and every row covers its table; the figures sit in the columns GnuCash's browser puts them in |
| An invoice option in the options summary (v0.6) | `#<swig-pointer …>`, different at every run: `disp` has no case for an invoice (`report/html-utilities.scm:302-323`) | The document's GUID | The same bytes for the same run |
| The invoice's picture (v0.6) | `<img src=…>` with the address quoted as a Scheme string (`report/html-text.scm:227-231`) | The address HTML-escaped | Every text on a page is HTML-escaped (`reports.md` §8) |
| An invoice whose owner is not a job, with Job Details on (v0.6) | `gncJobGetID` of the null job `gncOwnerGetJob` gives (`standard/invoice.scm:238-239`; `5.17:libgnucash/engine/gncOwner.c:376-381`); GnuCash's own test shows no job rows for such documents (`standard/test/test-invoice.scm:85-90`) | No job, so no job rows | As GnuCash's test runs show |
| `ctx.print_time` with a time directive (v0.6) | `gnc-print-time64` prints any `strftime` directive (`5.17:libgnucash/engine/gnc-date.cpp:370-385`) | Date directives only; any other is printed as written | Reports print dates: the Book's Fancy Date Format, which the invoice reads, is built from day, month and year (`5.17:gnucash/gnome-utils/gnc-date-format.c:458-461`) |
