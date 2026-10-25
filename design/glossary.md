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
| **Backend** | The anti-corruption layer that translates one file format to and from the core model (`red-ale-xml`; `red-ale-sql` from v0.3). |
| **Session** | An open Book file: the Book, its lock, its journal and whether it is read-only. Opened, edited, saved and closed through the API. |
| **Lock** | The `<file>.LCK` marker that stops two programs writing one Book file. A Session either holds the lock or is read-only. |
| **Read-only session** | A Session that never takes the lock and refuses every mutation. Carries the *reason* it is read-only. |
| **Read-only trigger** | A property of a Book that makes red-ale open it read-only: an unknown Book feature, or trading accounts. |
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

## Identity and values

| Term | Meaning |
|------|---------|
| **GUID** (`Guid`) | 128-bit identifier of an entity, written as 32 lowercase hex digits. Kept exactly as read. |
| **Numeric** | An exact rational `num/denom` with 64-bit parts: GnuCash's `gnc_numeric`. Every amount, value, price and rate is a Numeric. Never a float. |
| **SCU** | Smallest commodity unit: the denominator a Commodity's quantities are kept in (100 for USD, 1 for JPY). Also called the commodity's *fraction*. |
| **Time64** | An instant, in whole seconds since 1970-01-01 UTC. GnuCash's `time64`. |
| **Date** | A calendar date with no time (GnuCash's `GDate`). |
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
| **Date posted** | The date a Transaction takes effect. Stored as a Time64 at 10:59:00 UTC of that date (GnuCash's *neutral time*). |
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
| **Orphan** | A Split with no Account. A scrub moves it to an `Orphan-<CUR>` Account. |
| **Scrub** | A repair that restores an invariant (imbalance, orphans, lots). *Check & Repair* runs the scrubs over an Account, a subtree or the Book. |
| **Statement** | One reconciliation's Account, statement date (the last second of the local day), ending balance and include-subaccounts flag. |
| **Ticked split** | A Split marked to reconcile during a reconciliation. Held by the caller, never by the Book, until the reconciliation finishes. |
| **Postponed reconciliation** | A reconciliation saved for later: its date and balance in the Account's `reconcile-info/postpone` slots, its ticked Splits cleared. |
| **Repair** | One change a Scrub made, as the scrub methods report it. |
| **Raw sign** | Debits positive, credits negative: the sign of every reconcile amount. **⚠** GnuCash's flipped sign for credit Accounts is a display preference. |
| **Closing the books** (`book.closeBooks`) | GnuCash's *Close Book* (Tools ▸ Close Book…): at a closing date, one Closing transaction per Commodity moves the balances of the income Accounts, and another those of the expense Accounts, into an equity Account. **⚠** Not closing a Session (`book.close`): the books stay open and editable. |
| **Closing transaction** | A Transaction marked with the slot `book_closing` = 1, made when closing the books. Reports can leave such Transactions out. |

## Commodities and prices

| Term | Meaning |
|------|---------|
| **Commodity** | Anything an Account can hold: a currency or a security. Identified by namespace and mnemonic. |
| **Namespace** | A Commodity group: `CURRENCY`, an exchange such as `NASDAQ`, or a user-defined one. `template` is reserved. |
| **Mnemonic** | A Commodity's symbol within its namespace (`USD`, `AAPL`). **⚠** The XML calls it `cmdty:id`, but it is not a GUID. |
| **Security** | A Commodity outside the `CURRENCY` namespace. |
| **Quote source** | The named source online quotes would come from. Stored and edited in v0.1, never fetched. |
| **Price** | The value of one Commodity in another (usually a currency) at a time, with a source and a type. |
| **Price database** (`PriceDb`) | All Prices of a Book. |
| **Price source rank** | The precedence of a Price's source (user entries before fetched quotes before prices recorded from Transactions). On the same day, a Price of a lower rank replaces one of a higher rank and is not replaced by it. Unknown sources rank lowest (`invalid`). |
| **Same day** (Prices) | The UTC calendar day in red-ale. **⚠** GnuCash uses its local time zone. |
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

## red-ale

| Term | Meaning |
|------|---------|
| **Use case** | One operation on a Session, such as posting a Transaction. Each JSON-RPC method calls exactly one use case. |
| **Mutation** | A use case that changes the Book. Applies completely or not at all, and leaves the Book balanced. |
| **Published language** | The JSON-RPC schema: versioned, documented, and changed only additively within a major version. |
| **Effect** | What a method does to the Book: *read*, *write* or *destructive*. MCP's read-only and destructive hints come from it. |
| **Tool** | MCP's name for an operation an agent can call. In red-ale each Tool is one JSON-RPC method, with the same name and params schema; it is not a separate operation. |
| **Configuration** | red-ale's own JSON settings file (`--config`), for what GnuCash keeps as user preferences (the Accounting period). Never stored in the Book. |
