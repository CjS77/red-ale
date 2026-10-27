# Glossary

The ubiquitous language of red-ale. Each term is defined once, here, and used with this meaning in conversation,
documents, code and the JSON-RPC schema. GnuCash's terms come first; where GnuCash uses a word differently from
everyday accounting or from its own code, the clash is flagged with **⚠**.

Code names are given in `code font` when they differ from the term.

## Books and files

| Term | Meaning |
|------|---------|
| **Book** | One set of accounts with everything that belongs to them: commodities, transactions, prices, lots, business objects and book-level slots. A GnuCash file holds exactly one Book. |
| **Book file** | The file on disk that stores a Book. v0.1 handles XML Book files, plain or gzip-compressed. |
| **Backend** | The anti-corruption layer that translates one file format to and from the core model (`red-ale-xml`; `red-ale-sql` from v0.4). |
| **Session** | An open Book file: the Book, its lock, its journal and whether it is read-only. Opened, edited, saved and closed through the API. |
| **Lock** | The `<file>.LCK` marker that stops two programs writing one Book file. A Session either holds the lock or is read-only. |
| **Read-only session** | A Session that never takes the lock and refuses every mutation. Carries the *reason* it is read-only. |
| **Read-only trigger** | A property of a Book that makes red-ale open it read-only: an unknown Book feature. (Trading accounts, the num field source and an auto read-only threshold were triggers in v0.1.) |
| **Book feature** | A string in the Book's `features` slot naming a capability the Book relies on, so that programs lacking it refuse to edit the Book. **⚠** Not a red-ale "feature" in the `features.md` sense; say *Book feature* for this. |
| **Book option** | A setting stored in the Book's `options` Frame, edited in GnuCash's File > Properties. Book data, unlike the Configuration. |
| **Num field source** (`splitActionForNum`) | The Book option "Use Split Action Field for Number". When on, a Split's Number is its Action, and the Transaction's Num is shown as *T-Num*. Turning it on sets the Book feature `Number Field Source`. |
| **Auto read-only threshold** (`autoReadOnlyDays`) | The Book option "Day Threshold for Read-Only Transactions (red line)": a number of days *N*. The *threshold date* is today minus *N* days; a Transaction posted strictly before it is *read-only by date*, and GnuCash's register refuses to change it. **⚠** GnuCash's GUI calls it the *red line*. |
| **Journal** | GnuCash's `.log` file beside the Book file (`<file>.<YYYYMMDDhhmmss>.log`), recording each transaction change as tab-separated records. Only transactions are journaled; a new journal starts after each save. **⚠** GnuCash calls it the *transaction log* (`TransLog`); red-ale says *journal* so it is not confused with tracing logs. |
| **Replay** | Re-applying a journal to a Book to recover changes that were never saved. |
| **Backup** | The previous Book file, kept under a timestamped name (`<file>.<YYYYMMDDhhmmss>.gnucash`, local time) when a save replaces it. GnuCash makes it as a hard link, falling back to a copy. Old backups and journals are deleted after a retention period (30 days by default). |
| **Residue** | Content of a Book file that the core model does not represent (unknown elements, out-of-scope objects). The Backend keeps it and writes it back unchanged. |
| **Round-trip** | Opening a Book file and saving it with no edits. A round-trip must give back a file GnuCash 5.17 considers identical; red-ale aims for byte-identical. |
| **Oracle** | GnuCash 5.17 itself (library and Python bindings), used by tests as the judge of compatibility. |
| **SQL book** | A Book kept in a database by GnuCash's SQL backend: an SQLite file (from v0.4), or a MySQL or PostgreSQL database (v0.7). Its lock is a row of the `gnclock` table. **⚠** An SQLite book is a file, opened by path or `sqlite3://`, but not an XML Book file: GnuCash writes each change at once and has no Save for it, while red-ale saves it at `book.save`. |
| **Schema version** | A row of an SQL book's `versions` table: a table name and the version of that table's layout, plus the book-wide `Gnucash` and `Gnucash-Resave` rows. A book whose table rows differ from GnuCash 5.17's, or whose `Gnucash-Resave` row is not 19920, opens read-only; red-ale never upgrades one ([`sql-fidelity.md`](sql-fidelity.md)). **⚠** Not a GnuCash release, nor the `version` attributes of the XML. |
| **SQL Residue** | What an SQL book holds that the core model does not, which the SQLite Backend leaves untouched: rows of entity types red-ale does not model, orphan slot rows, the `versions` table. The SQL counterpart of the XML *Residue*. |
| **Stored form** | What a value reads back as from an SQL column, compared after a save: each column reads as the XML reader would read the element GnuCash writes for the same value, so an empty and a missing memo are the same stored form. |

## Identity and values

| Term | Meaning |
|------|---------|
| **GUID** (`Guid`) | 128-bit identifier of an entity, written as 32 lowercase hex digits. Kept exactly as read. |
| **Numeric** | An exact rational `num/denom` with 64-bit parts: GnuCash's `gnc_numeric`. Every amount, value, price and rate is a Numeric. Never a float. |
| **SCU** | Smallest commodity unit: the denominator a Commodity's quantities are kept in (100 for USD, 1 for JPY). Also called the commodity's *fraction*. |
| **Time64** | An instant, in whole seconds since 1970-01-01 UTC. GnuCash's `time64`. |
| **Date** | A calendar date with no time (GnuCash's `GDate`). |
| **Local time zone** (`TimeZone`) | The time zone of the server process, read once at start-up as GnuCash reads it: `TZ` as a zone file, else `TZ` as a POSIX string with Boost's reversed sign, else `/etc/localtime`, else UTC. Day boundaries, "today", same-day prices and file-name timestamps use it. |
| **Local day** | The calendar date an instant falls on in the Local time zone; it runs from local 00:00:00 (*day start*) to 23:59:59 (*day end*). |
| **Day-neutral time** | The instant a Date chosen by the user is stored at: 10:59:00 UTC, moved to 11:59 UTC in zones west of UTC−10 and 09:59 UTC east of UTC+13, so it falls on that Date in the Local time zone. GnuCash's *neutral time* (`DayPart::neutral`). |
| **Relative date** | A date given relative to now, as GnuCash's report date options give them (`RelativeDatePeriod`): `today`, `one-month-ago`, `start-this-month`, `end-prev-quarter`, `start-prev-fin-year` and so on, under GnuCash's storage names. |
| **Slot** | One key-value entry attached to an entity. Values are integer, double, numeric, string, GUID, timespec, date, list or frame. |
| **Frame** | A Slot value holding nested Slots, so keys form paths such as `tax-US/code`. An entity's slots are its root Frame. **⚠** GnuCash code says *KVP*; red-ale says Slot and Frame. |

## Accounts

| Term | Meaning |
|------|---------|
| **Account** | A named place where Splits are recorded, with one Commodity and one Account type. |
| **Root account** | The single nameless top of the Account tree (type `ROOT`). Every other Account descends from it. |
| **Account tree** | The hierarchy of Accounts under the Root account. |
| **Account type** | One of `BANK`, `CASH`, `ASSET`, `CREDIT`, `LIABILITY`, `STOCK`, `MUTUAL`, `CURRENCY`, `INCOME`, `EXPENSE`, `EQUITY`, `RECEIVABLE`, `PAYABLE`, `ROOT`, `TRADING`. **⚠** `STOCK` is an account holding a security, not warehouse inventory. **⚠** `CREDIT` is a credit-card account. |
| **Placeholder** | An Account that groups others and accepts no Splits. |
| **Hidden** | An Account kept but left out of everyday listings. |
| **Account code** | A free-text code per Account used for sorting and numbering. |
| **Full name** | An Account's name prefixed by its ancestors' names, joined by the separator `:`. |
| **Tax-related** | A flag marking Accounts that feed tax reports. |
| **TXF code** | A US tax form-and-line code stored on an Account (`tax-US/code`). |
| **Display order** | GnuCash's order of sibling Accounts: code, then type rank, then name, then GUID. **⚠** Not the Book's storage order, which keeps parents before children as read. |
| **Name violation** | An Account name containing the separator `:`. red-ale refuses new ones and reports existing ones. |
| **Delete with move** | Deleting an Account while moving its Splits to another Account, or re-parenting its single sub-account, instead of deleting them. |
| **Accounting period** | Start and end bounds (a relative choice such as *start of this quarter*, or a date) and a fiscal year end. A user setting kept in red-ale's Configuration, not Book data. |
| **Fiscal year end** | The month and day a fiscal year ends, from the Configuration or the Book's `fy_end` slot. |
| **Account colour** | A display colour for an Account, in its `color` slot, written `rgb(r,g,b)` as GnuCash's account dialog writes it. Unset means GnuCash's default colour. **⚠** The slot key is spelt `color`. |
| **Balance limit** | A *higher* or *lower* limit on an Account's balance, in Raw sign, in the `balance-limit` Frame. GnuCash warns when the balance crosses one. **⚠** The include-sub-accounts slot is spelt `inlude-sub-accts` in GnuCash's files. |
| **Opening balance account** | The one EQUITY Account per currency that holds opening balances, marked by the slot `equity-type` = `opening-balance`. Its use is declared by a Book feature. |
| **Associated account** | An Account another Account remembers for a role, named by a *tag* (`stock-dividends`, `stock-capgains`, `stock-cash-proceeds`, `stock-broker-fees`), in the `associated-account` Frame. |
| **Cascade** | GnuCash's "Cascade Account Properties": setting colour, placeholder or hidden on an Account and all its descendants at once. |

## Transactions

| Term | Meaning |
|------|---------|
| **Transaction** | A dated, balanced exchange made of two or more Splits, in one currency. The aggregate that keeps itself balanced. |
| **Split** | One leg of a Transaction: an Account, a value and an amount. |
| **Transaction currency** | The currency every Split's value is expressed in. **⚠** A *currency* is a Commodity in the `CURRENCY` namespace, never a security. |
| **Value** | A Split's size in the Transaction currency. The values of a Transaction's Splits sum to exactly zero. **⚠** Always transaction-currency value, never "a number". |
| **Amount** | A Split's size in its Account's Commodity. **⚠** GnuCash *amount* means this quantity, not a generic number; the XML calls it `quantity`. Value and Amount are both stored; neither is derived from the other on save. |
| **Price of a split** | `value / amount`: the exchange rate a Split implies. |
| **Date posted** | The date a Transaction takes effect. Stored as a Time64 at the Day-neutral time of that date, with a `date-posted` slot holding the Date. Without the slot it reads as the UTC date of the Time64. |
| **Date entered** | When a Transaction was first recorded. |
| **Num** | A Transaction's free-text check or reference number. |
| **Number** (of a Split) | What GnuCash shows as a Split's "Num": its Action when the Book's Num field source is on, otherwise its Transaction's Num (`gnc_get_num_action`). **⚠** Not always the Transaction's Num. |
| **Action** | A Split's free-text action field (`Buy`, `Sell`, `Split`, …). |
| **Memo** | A Split's free-text note. **⚠** Distinct from the Transaction's *notes*. |
| **Reconcile state** | A Split's state: `n` new, `c` cleared, `y` reconciled, `f` frozen, `v` voided. |
| **Void** | Marks a Transaction void: values and amounts become zero, the former figures and a reason are kept in Slots. *Unvoid* restores them. A Transaction is void when its `void-time` slot holds a timestamp GnuCash can parse (the 5.17 rule); the reason does not decide it. |
| **Reverse** | Creates a new *reversing Transaction* with every Split's value and amount negated; the original's `reversed-by` slot points to it. |
| **Transfer** | GnuCash's Transfer dialog as one operation: a Transaction between two Accounts, possibly in different Commodities, with its rate (to-commodity per from-currency) optionally recorded as a Price. |
| **Find query** | Criteria over Splits (text, amounts, dates, states, Accounts) combined with all, any or not, as GnuCash's Find dialog builds them. |
| **Imbalance** | The amount by which a Transaction's values fail to sum to zero. A *scrub* moves it to an `Imbalance-<CUR>` Account. |
| **Use trading accounts** | The Book option (`options/Accounts/Use Trading Accounts` = `"t"`) under which every commit keeps a Transaction's amounts balanced per Commodity through Trading splits. |
| **Trading account** | An Account of type `TRADING`. GnuCash's own ones form the tree `Trading:<namespace>:<mnemonic>`, one leaf per Commodity. |
| **Trading split** | A Split in a Trading account. With Use trading accounts on, commit replaces a Transaction's Trading splits so that, per Commodity, amounts sum to zero. **⚠** GnuCash adds and removes them itself; they are not the user's entries. |
| **Balanced with trading accounts** | Non-trading values sum to zero, Trading split values sum to zero, and each Commodity's amounts sum to zero (`xaccTransIsBalanced`). Without the option, *balanced* means only that values sum to zero. |
| **Orphan** | A Split with no Account. A scrub moves it to an `Orphan-<CUR>` Account. |
| **Scrub** | A repair that restores an invariant (imbalance, orphans, lots). *Check & Repair* runs the scrubs over an Account, a subtree or the Book. |
| **Statement** | One reconciliation's Account, statement date (the last second of the local day), ending balance and include-subaccounts flag. |
| **Ticked split** | A Split marked to reconcile during a reconciliation. Held by the caller, never by the Book, until the reconciliation finishes. |
| **Postponed reconciliation** | A reconciliation saved for later: its date and balance in the Account's `reconcile-info/postpone` slots, its ticked Splits cleared. |
| **Repair** | One change a Scrub made, as the scrub methods report it. |
| **Raw sign** | Debits positive, credits negative: the sign of every reconcile amount. **⚠** GnuCash's flipped sign for credit Accounts is a display preference. |
| **Closing the books** (`book.closeBooks`) | GnuCash's *Close Book* (Tools ▸ Close Book…): at a closing date, one Closing transaction per Commodity moves the balances of the income Accounts, and another those of the expense Accounts, into an equity Account. **⚠** Not closing a Session (`book.close`): the books stay open and editable. |
| **Closing transaction** | A Transaction marked with the slot `book_closing` = 1, made when closing the books. Reports can leave such Transactions out. |
| **Split order** | The order of an Account's Splits (`xaccSplitOrder`): posted time, Closing transactions last, Num (the Split's Action when the Num field source is on), entered time, description, then the Split's memo, action, reconcile state, amount, value, reconcile date and GUID. Text compares bytewise where GnuCash collates by locale. |
| **Running balance** | An Account's total after a Split, in Split order. GnuCash keeps four per Split: the balance, the *cleared* (states other than `n`), the *reconciled* (`y` and `f`) and the *noclosing* balance. **⚠** A Stock split GnuCash skips (while the balance is zero, or when it would make it zero) never gets them, and they read as zero. |
| **Noclosing balance** | A balance that leaves Closing transactions out (`xaccSplitGetNoclosingBalance`), so income and expense reports see a period's activity before the books were closed. |
| **As-of balance** | An Account's Running balance at its last Split posted strictly before a time (`xaccAccountGetBalanceAsOfDate`). **⚠** Strictly before; the reports' balances at dates take Splits posted at or before each date. |
| **Period query** (`PeriodQuery`) | The Split query GnuCash's reports run: some Accounts, posted times from and to (inclusive, to the second), no void Splits, and optionally a description pattern OR Closing transactions (`gnc:account-get-trans-type-splits-interval`). |
| **Cross-commodity split** | A Split whose Transaction currency is not its Account's Commodity: the Splits reports derive exchange rates from (`gnc_get_match_commodity_splits`). |
| **Auto-clear** | Finding the one set of uncleared Splits in an Account that brings its cleared balance to a given figure, and marking them cleared. Fails when no set or more than one set does. |
| **Document link** (`docLink`) | A link from a Transaction or Business document to a file or URL, kept in its `assoc_uri` slot. A link with no `scheme://` is a path relative to the Path head. **⚠** GnuCash code also says *doclink*; before GnuCash 4 it was an *association*, hence the slot name. |
| **Path head** | The folder relative Document links start from: a GnuCash user preference, so kept in red-ale's Configuration, never in the Book. |

## Commodities and prices

| Term | Meaning |
|------|---------|
| **Commodity** | Anything an Account can hold: a currency or a security. Identified by namespace and mnemonic. |
| **Namespace** | A Commodity group: `CURRENCY`, an exchange such as `NASDAQ`, or a user-defined one. `template` is reserved. |
| **Mnemonic** | A Commodity's symbol within its namespace (`USD`, `AAPL`). **⚠** The XML calls it `cmdty:id`, but it is not a GUID. |
| **Security** | A Commodity outside the `CURRENCY` namespace. |
| **Quote source** | The name, in Finance::Quote's vocabulary, of where a Commodity's online quotes come from (`alphavantage`, `yahoo_json`, `currency`, …), stored on the Commodity in the Book. Says *what* to fetch; a Quote service says *where from*. **⚠** Not a Quote service, and not a Price's source string. |
| **Quote service** | A web service red-ale fetches quotes from, set in the Configuration: a URL template, a response format, an optional key and a minimum interval between requests. The Configuration maps each Quote source to the Quote services that answer it. Never stored in the Book. See [`api/quotes.md`](api/quotes.md). |
| **Quote record** | One commodity's answer as GnuCash reads it from Finance::Quote: `success`, `errormsg`, `last`/`nav`/`price`, `currency`, `date` (month/day/year), `inverted`, every value as text. Each Quote service's answer is read into one; prices are built only from quote records (`red_ale_core::ops::quotes::QuoteRecord`). |
| **Quote fetch** | One run of fetching quotes, started by `price.fetchQuotes`, with an ID. Its requests run in the background, outside the Book; the Prices it got are then added in one Mutation. One runs at a time; closing the book cancels it. |
| **Price** | The value of one Commodity in another (usually a currency) at a time, with a source and a type. |
| **Price database** (`PriceDb`) | All Prices of a Book. |
| **Price source rank** | The precedence of a Price's source (user entries before fetched quotes before prices recorded from Transactions). On the same day, a Price of a lower rank replaces one of a higher rank and is not replaced by it. Unknown sources rank lowest (`invalid`). |
| **Same day** (Prices) | The same Local day. GnuCash compares local noon of each price's local day (`time64CanonicalDayTime`). |
| **Price lookup** (`PriceAt`) | Which Price a conversion takes: the one *nearest* a time (a tie goes to the older), the newest at or *before* it, or the *latest*. |
| **Direct price** | A Price of the two Commodities being converted, in either direction; one quoted the other way round is inverted. |
| **Indirect price** | A rate between two Commodities through a third that both are priced in, used when there is no Direct price (`get_nearest_price`). **⚠** For the *latest* lookup GnuCash takes a Direct price even when it lies in the future, but an Indirect one only up to now. |
| **Built-in currency** | One of the 227 ISO 4217 currencies every Book has. Stored in the Book file only when it has quotes on or Slots. |
| **Usage** (of a Commodity) | What refers to it. **⚠** GnuCash's usage count is Accounts only; red-ale also reports Transactions, Prices and business objects. |
| **Auto quote control** | GnuCash's `auto_quote_control` slot on a currency: `"false"` means the user overrode the quote flag GnuCash sets for currencies Accounts use. |
| **Euro legacy currency** | A currency replaced by the euro at a fixed rate (DEM, FRF, …). |
| **User symbol** | A Commodity's display symbol chosen by the user, in its `user_symbol` slot. |
| **Default symbol** | A built-in currency's symbol from the ISO table (`$`, `€`); may be empty. Never stored. |
| **Display symbol** | The symbol GnuCash shows amounts with: the User symbol, else the Default symbol, else the Mnemonic. **⚠** GnuCash calls it the *nice symbol* (`gnc_commodity_get_nice_symbol`). |

## Investments

| Term | Meaning |
|------|---------|
| **Lot** | A group of Splits in one Account that buy and then sell the same holding. *Closed* when its amount sums to zero. |
| **Capital gain** | The realised gain or loss of a Lot, recorded as a gains Split in a separate gains Transaction. |
| **Lot policy** | The rule picking which Lot a selling Split draws from. GnuCash 5.17 uses FIFO. |
| **Stock split** | A change in the number of shares held with no change in value, recorded as a *stock-split Split*: value `0/1`, action `Split` and slot `split-type = stock-split`. Such Splits never join Lots and never produce gains. |
| **Adjusted amount** | A Split's Amount restated in the units after any later Stock split in its Account. Lot balances, closure, FIFO and gains use it. New in GnuCash 5.17; never stored in the file. |
| **Gains transaction** | The Transaction recording a Capital gain, described `Realized Gain/Loss`: a *gains Split* in the Lot (Amount zero, Value the gain, slot `gains-source` naming the selling Split) and a Split in the Gains account. The selling Split names the gains Split in its slot `gains-split`. |
| **Gains account** | Where a Lot Account's gains go: the Account named by its `lot-mgmt/gains-acct/<currency>` slot, or else an INCOME Account `Orphaned Gains-<CUR>` under the Root account, found by name or created. |
| **Cut split** | A Split cut in two because it would overfill a Lot. Each piece names the other in its `lot-split` Frame (its *peer*). |

## Business

| Term | Meaning |
|------|---------|
| **Owner** | Whoever a business document belongs to: a Customer, Vendor, Employee or Job. |
| **Customer**, **Vendor**, **Employee** | Owners with an ID, a name, an Address and terms. |
| **Job** | A piece of work grouping documents for one Customer or Vendor. |
| **Address** | A name, four address lines, phone, fax and email. |
| **Business document** (`Invoice` in code) | An invoice (to a Customer), a bill (from a Vendor) or an expense voucher (from an Employee). **⚠** GnuCash code calls all three *invoice*; red-ale says *document* for the general case. |
| **Entry** | One line of a document or order: quantity, price, discount, tax. |
| **Order** | A group of Entries for one Owner, later invoiced. |
| **Tax table** | Named tax rates, each a percentage or a fixed value posted to an Account. |
| **Billing term** | When a document is due and any early-payment discount: *days* or *proximo*. |
| **Post** | Turning a document into a Transaction in an A/Receivable or A/Payable Account, with a Lot linking the two. *Unpost* reverses it. |
| **Payment** | A Transaction that settles an Owner's open documents through their Lots. |
| **Due reminder** | The list of posted, unpaid documents falling due within a number of days. |
| **Counter** | The last number issued for a kind of document or Owner, kept in the Book's `counters/<name>` slot (`gncCustomer`, `gncInvoice`, …). The next ID is one more, formatted with the Counter format. **⚠** GnuCash's book options call it the *previous* number. |
| **Counter format** | A `printf` format with exactly one 64-bit integer conversion (`%li`, `%lli` or `%I64i`, with flags, width and precision), kept in `counter_formats/<name>`. A missing or invalid one means GnuCash's default `%.6li` (`000001`). |
| **End owner** | The Owner a document finally belongs to: a Job's Customer or Vendor, otherwise the Owner itself. Decides the document kind. |
| **Document kind** | Invoice (Customer), bill (Vendor) or expense voucher (Employee), from the End owner. |
| **Invoice type** (`InvoiceType`) | A document's End owner and whether it is a credit note: GnuCash's `GncInvoiceType`. Decides the signs of posted values and the word GnuCash writes in lot titles and split actions (`Invoice`, `Bill`, `Expense`, `Credit Note`). |
| **Entry side** | Which half of an Entry's fields applies: the *invoice side* (`i-` fields) for customer documents and every order Entry, the *bill side* (`b-` fields) for bills and expense vouchers. |
| **Refcount** | How many Customers, Vendors, documents and Entries use a Tax table or Billing term. Frozen copies are never counted. A table or term in use cannot be deleted. |
| **Frozen copy** | A hidden child copy of a Tax table or Billing term made when a document is posted, so later edits to the original do not change posted documents. **⚠** GnuCash calls it the *child*. |
| **Payment lot** | A Lot in the A/R or A/P Account holding a payment not yet matched to a document. A *pre-payment lot* is one made before the document existed. |
| **Lot link** | A Transaction of type `L` that moves value between Lots of one Owner to settle documents against payments or credits. |
| **Duplicate** | A new unposted document copied from another, with the next number, a new date for it and its Entries, and GnuCash's other resets. |
| **Credit note** | A document that reverses an invoice, bill or expense voucher of its kind: its `credit-note` slot is 1, its Entries store their quantity negated, and posting it moves the A/R or A/P balance the other way. Making one sets the Book feature `Credit Notes`. |
| **Document quantity** | An Entry's quantity as printed on its document: the stored quantity, negated on a credit note. |
| **Open item** | An Owner's open document Lot or Payment lot in an A/R or A/P Account: what a payment can be applied to, as GnuCash's payment dialog lists it. |
| **Transfer split** | A payment's Split in the Account the money moves through: any asset, liability or equity type but A/R and A/P. |

## Budgets

See [`notes/engine-budgets.md`](notes/engine-budgets.md).

| Term | Meaning |
|------|---------|
| **Recurrence** | A step repeated from a start Date: a multiplier, a *period type* (`once`, `day`, `week`, `month`, `end of month`, `nth weekday`, `last weekday`, `year`) and a *weekend adjustment* (`none`, `back` to the Friday, `forward` to the Monday; GnuCash sets one only on monthly and yearly types). Every 3 months from 2024-01-31, say. A Budget has one; a scheduled transaction's schedule is a list of them. |
| **Budget** | A named plan of amounts per Account and Budget period: a description, a number of periods, a Recurrence giving the periods, and Slots holding its Budget amounts and Budget notes. A Book may hold several; its `Default Budget` option names the one reports start from. |
| **Budget period** | One of a Budget's periods, numbered from 0. Period *p* runs from the Recurrence's *p*-th occurrence to the day before the next. **⚠** Not the *Accounting period*, which is a Configuration setting and has nothing to do with budgets. |
| **Budget amount** | The amount planned for one Account in one Budget period, kept as written (no rounding to the SCU) in the Budget's `<account-guid>/<period>` Slot. It is *set* or *unset*: an explicit zero is set, and only an unset amount lets a report sum the Account's children instead. **⚠** Unset is not zero, though GnuCash reads both as 0. |
| **Budget note** | Free text for one Account in one Budget period, in the Budget's `notes/<account-guid>/<period>` Slot. An empty note reads as no note. |
| **Actual** | What one Account and its sub-accounts did in one Budget period, to set against its Budget amount: the change in its no-closing balance between the period's bounds, sub-accounts converted to its commodity (`gnc_budget_get_account_period_actual_value`). Computed, never stored. |
| **Natural signs** | Budget amounts stored with the engine's sign, as actuals have it: expense budgets positive, income budgets negative, liability increases negative. GnuCash has stored them so since 3.8, and the Book feature `Use natural signs in budget amounts` says a book's amounts are. Before 3.8 the stored sign followed the user's "Reverse Balanced Accounts" preference, until GnuCash's sign scrub (`budget.scrubSigns`) converts them. ⚠ *Natural* means unreversed, not the sign a report shows. |

## Scheduled transactions

See [`api/scheduled-transactions.md`](api/scheduled-transactions.md).

| Term | Meaning |
|------|---------|
| **Scheduled transaction** (`ScheduledTransaction`) | A transaction that recurs: a name, a Schedule, a start date, an end (a date, a number of occurrences, or none), Template transactions, whether to create Instances automatically and how many days ahead, and its state (last occurrence, Instance count, remaining occurrences, Deferred instances). One that ends is kept, enabled, with no next Occurrence. **⚠** GnuCash's code says *SX* (`SchedXaction`); red-ale says *scheduled transaction*. |
| **Schedule** | A Scheduled transaction's list of Recurrences. Its next Occurrence is the earliest next date of any of them. **⚠** A list, not one Recurrence: GnuCash's "semi-monthly" is two monthly Recurrences, and "weekly on several days" one weekly Recurrence per day. |
| **Occurrence** | A date a Schedule gives a Scheduled transaction: on or after its start date, up to its end. **⚠** GnuCash's code calls the next Occurrence the *next instance* (`xaccSchedXactionGetNextInstance`). |
| **Instance** | One Occurrence as Since Last Run acts on it: its date, its Instance count, its Variable values and its state. The states are GnuCash's: *to create* (create its Transactions), *reminder* (nothing, shown again next time), *postponed* (saved as a Deferred instance), *ignored* (skipped for good) and *created*. **⚠** GnuCash calls every entity a *QofInstance*; a scheduled transaction's Instance is one occurrence of it, not an entity. |
| **Instance count** | The number the next Instance of a Scheduled transaction gets, which is the value of its Variable `i` (`sx:instanceCount`). Starts at 1. **⚠** Not how many Transactions it has created: postponing counts, and GnuCash overwrites it from the last Instance acted on. |
| **Deferred instance** | A postponed Instance as saved in its Scheduled transaction: the state before it (last occurrence, remaining occurrences, Instance count), so Since Last Run proposes it again. **⚠** GnuCash's GUI says *postponed*; its code and XML say *deferred* (`sx:deferredInstance`). An SQL book cannot hold them. |
| **Template root** | The Account of type `ROOT` named `Template Root` that holds every Template account, apart from the Root account. |
| **Template account** | The Account, one per Scheduled transaction, that holds its Template transactions: named by the scheduled transaction's GUID, commodity `template:template`, under the Template root. **⚠** Not in the Account tree: no balance, report, find query, import match, scrub or export sees it. |
| **Template transaction** | A Transaction in a Template account that Since Last Run copies into a real Transaction for each Instance: its description, num and notes, and per *template split* a memo, an action, the real Account the copy goes to and a debit and a credit Formula (the split's `sched-xaction` Frame). **⚠** Not a *Report template*. Not a Transaction of the Book: its values are zero and nothing that reads the Book sees it. |
| **Formula** | The text of a template split's debit or credit amount, in GnuCash's expression syntax: arithmetic, parentheses, Variables, assignments and calls of financial functions (`pmt(rate : nper : pv : fv : type)`, with `:` between arguments). Evaluated exactly. **⚠** GnuCash calls functions in Guile with doubles; red-ale computes fin.scm's financial functions exactly and refuses any other. |
| **Variable** | A name in a Formula, given an Amount per Instance before it is created. `i` is predefined as the Instance count. An *exchange-rate variable*, named `<split mnemonic> -> <transaction mnemonic>` (`EUR -> USD`), gives the rate for a split in another Commodity. **⚠** Not GnuCash's saved calculator variables, which red-ale never reads. |
| **Since Last Run** | Creating the Transactions of every enabled Scheduled transaction that have fallen due since it last ran, and reminding of those coming. In red-ale two calls, as an Import: `scheduledTransaction.sinceLastRun` returns a *Since Last Run proposal* of Instances, held by ID and tied to the Book revision it was made at, and `scheduledTransaction.apply` applies it with the caller's choices as one Mutation. **⚠** GnuCash's GUI runs it whenever a book opens; red-ale only when asked. |

## Import and export

See [`api/import.md`](api/import.md) and [`api/export.md`](api/export.md).

| Term | Meaning |
|------|---------|
| **Import** | Reading a file of transactions, accounts, prices, business documents or Owners into the Book, as one of GnuCash's importers does. Each import is one Mutation. |
| **Import proposal** | What `import.preview` returns and the server holds until it is applied, discarded, the book closes or 16 newer ones push it out: every transaction the file holds, each with its Import action, Candidate matches, the account of each Split, any Imbalance and whether it would be read-only by date. Identified by an ID and tied to the Book revision it was made at. It changes nothing until `import.apply`. **⚠** GnuCash has no such object: its matcher window holds the same state in the GUI. |
| **Imported transaction** | One transaction of an Import proposal, by its index in the file: the Transaction the importer built from the file, not yet in the Book. **⚠** GnuCash's `GNCImportTransInfo`. |
| **Import action** | What `import.apply` does with one record of an Import proposal. For an Imported transaction: *add* it as a new Transaction; *clear* the existing Transaction it matches; *update* that Transaction from it and clear it; or *skip* it. For accounts, prices, Business documents and Owners: *add*, *update* the existing one (replace a same-day Price) or *skip*. **⚠** GnuCash's GUI calls *update* "update and clear" (`GNCImport_UPDATE`); it is one action, not two. **⚠** *Clear* sets the matched Split's Reconcile state to `c` (cleared); it never deletes anything. |
| **Candidate match** | An existing Split in the imported split's Account that the matcher (or QIF's duplicate finder) offers as the same transaction, with its Match score. **⚠** GnuCash's `GNCImportMatchInfo`. |
| **Match score** | The integer the matcher gives a Candidate match: points for an equal or nearly equal amount, a close date, an equal number, memo and description; points off for a different amount, a different number or dates far apart. The thresholds of the Import settings decide the Import action from the best score. **⚠** An integer, unrelated to a Bayesian probability. |
| **Import settings** | The matcher's thresholds and switches (`match-threshold`, `auto-add-threshold`, `auto-clear-threshold`, `atm-fee-threshold`, the date thresholds, `enable-skip`, `enable-update`, `use-bayes`), with GnuCash's defaults. GnuCash keeps them as user preferences, so red-ale keeps them in the Configuration. |
| **Import map** | What GnuCash learns from imports, kept in account Slots. The *plain* Import map is the `import-map` Frame: an exact description or memo to an Account, on the source Account (categories `desc` and `memo`; only `desc` is looked up), and a name used in CSV files to the Account holding it (`csv-account-map`, on that Account itself). The *Bayesian* Import map is, on the source Account, a count per token and target Account, in the flat, GUID-keyed form GnuCash 5.17 writes (`import-map-bayes/<token>/<guid>` as one Slot key). **⚠** GnuCash code says *imap* or *match map*; its GUI says *import map*. |
| **Bayesian matching** | Choosing the other Account of an Imported transaction from the Bayesian Import map: the Account whose combined token probability reaches 90%. On when the Import setting `use-bayes` is (the default); otherwise the plain Import map is used. |
| **Online ID** | The `online_id` Slot GnuCash's OFX and AqBanking importers put on a Split (and an Account, for the bank account it came from), naming the bank's own ID for it. GnuCash's matcher skips an imported transaction whose Online ID is already in the account. red-ale imports no format that carries one, so it keeps them and writes none. |
| **CSV import layout** | A saved set of choices for importing one bank's CSV files: separators or fixed widths, lines to skip, column types, date and number formats, encoding, multi-split mode and base account. GnuCash's *preset* or *settings*, kept in its state file outside the Book (`5.17:gnucash/import-export/csv-imp/gnc-imp-settings-csv-tx.cpp:147`), so red-ale keeps them in the Configuration. |
| **QIF account map** | What the QIF importer remembers between imports: which GnuCash Account each QIF account, category and payee went to, and which Commodity each QIF security. GnuCash keeps it in a file in its user data directory (`5.17:gnucash/import-export/qif-imp/qif-guess-map.scm:114`); red-ale keeps it in the Configuration. **⚠** Not the Import map, which lives in the Book. |
| **Book revision** | The number of Mutations the Session has applied since the book was opened. An Import proposal records the Book revision it was made at, and `import.apply` refuses it (`conflict`) at any other. Not stored in the Book. |
| **CSV export** | The account tree, or transactions in a date range, written as CSV text in GnuCash's layouts (*simple* or *complex* for transactions). Returned as text; nothing is written by the server. |
| **Export accounts** | Writing a new Book file holding the Account tree and Commodities of the open Book and no Transactions, as GnuCash's File ▸ Export ▸ Export Accounts. |

## Reports

See [`reports.md`](reports.md).

| Term | Meaning |
|------|---------|
| **Report template** | A report definition: a GUID, a name, its options and the code that builds its result. Built-in templates keep GnuCash 5.17's names and GUIDs. **⚠** GnuCash also calls saved configurations "custom report templates"; red-ale says *saved report configuration* for those. |
| **Report script** | A `.rhai` file defining one or more Report templates. *Built-in* scripts are translations of GnuCash's `.scm` files compiled into red-ale; *user* scripts load from the Report directory. |
| **Report directory** | The directory, named in the Configuration, that user Report scripts load from. |
| **Report option** | One setting of a report, identified by section and name (GnuCash's English strings), with a type, a default and a value. |
| **Saved report configuration** | A Report template's options saved under a new name and GUID in the Configuration. Stores only what differs from the template's defaults. **⚠** GnuCash's *custom report*; its *saved report* is an open report window, which red-ale does not have. |
| **Report result** | What running a report gives: sections of tables, charts and text with exact figures. The published language for reports. |
| **Report log** | The lines a Report script prints with `print` and `debug`, returned with the Report result. |
| **Query API** | The read-only functions and handles through which a Report script reads the Book. |
| **Rational** | An exact rational with no size limit, the number of report arithmetic, as Scheme's exact numbers are in GnuCash. **⚠** Not a Numeric: it never enters the Book. |
| **Monetary** | A Rational with a Commodity: GnuCash's `gnc:monetary`. |
| **Commodity collector** | A report's running totals per Commodity: GnuCash's `gnc:make-commodity-collector`. |
| **Display rounding** | Rounding a figure to its Commodity's SCU with banker's rounding, only to show it, as GnuCash's report renderer does. |
| **Exchange method** | How a report converts other Commodities to the report currency: the *Price Source* option (average cost, weighted average, or the Price database before, nearest to or latest at the report date). **⚠** Not a Price's source string (see *Price source rank*). |
| **Style sheet** | A named set of presentation options from a *style sheet template* (Plain, CSS, Easy, Fancy, Footer, Head or Tail), applied when a Report result is rendered as HTML. |
| **Report settings** | The Configuration's `reports` values standing in for GnuCash preferences reports read: report currency, reversed-balance accounts, date format, week start, limits. |

## red-ale

| Term | Meaning |
|------|---------|
| **Use case** | One operation on a Session, such as posting a Transaction. Each JSON-RPC method calls exactly one use case. |
| **Mutation** | A use case that changes the Book. Applies completely or not at all, and leaves the Book balanced. |
| **Published language** | The JSON-RPC schema: versioned, documented, and changed only additively within a major version. |
| **Effect** | How much a method changes: *read*, *write* or *destructive*. MCP's read-only and destructive hints come from it. |
| **Effect target** | What a method that is not *read* changes: the *book* or the *configuration*. Saving a Saved report configuration changes the Configuration and never the Book. |
| **Tool** | MCP's name for an operation an agent can call. In red-ale each Tool is one JSON-RPC method, with the same name and params schema; it is not a separate operation. |
| **Configuration** | red-ale's own JSON settings file (`--config`), for what GnuCash keeps outside the book: user preferences (the Accounting period, the Report settings, the Import settings, the Locale), Saved report configurations, Style sheets, CSV import layouts, the QIF account map and Quote services. Never stored in the Book. Its settings are declared in a JSON Schema (`config.schema`). |
| **Setting** | One value of the Configuration, declared in its schema with a type, default and description, and named by its *setting path*: its keys joined with `.` (`import.settings.matchThreshold`). |
| **Secret** | A Setting whose value is never shown, only whether it is set: a Quote service's key. Not changeable through MCP. |
| **Origin** | The door a call came through: JSON-RPC (the HTTP server and the CLI) or MCP. Only `config.set` looks at it. |
| **Locale** | The language and region conventions red-ale formats with, as GnuCash takes them from the C library: decimal point and grouping, the `locale` date format, currency symbol and default report currency, and day names (which also appear in Bayesian tokens); and, as GnuCash takes them from ICU, the first day of the week and the interval form of date ranges. A POSIX name such as `en_US` or `fr_FR` in the Configuration, default `C`; `environment` takes each category as the C library does, from `LC_ALL`, else `LC_NUMERIC`, `LC_MONETARY`, `LC_TIME` or `LC_MESSAGES`, else `LANG`. **⚠** Not the Local time zone, which is set apart, from `TZ`. |
