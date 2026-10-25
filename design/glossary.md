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
| **Read-only trigger** | A property of a Book that makes red-ale open it read-only: an unknown Book feature, trading accounts, or an auto read-only threshold. |
| **Book feature** | A string in the Book's `features` slot naming a capability the Book relies on, so that programs lacking it refuse to edit the Book. **⚠** Not a red-ale "feature" in the `features.md` sense; say *Book feature* for this. |
| **Journal** | GnuCash's `.log` file beside the Book file, recording every transaction change since the last save. **⚠** GnuCash calls it the *transaction log* (`TransLog`); red-ale says *journal* so it is not confused with tracing logs. |
| **Replay** | Re-applying a journal to a Book to recover changes that were never saved. |
| **Backup** | A timestamped copy of the Book file made before it is overwritten (`<file>.<YYYYMMDDhhmmss>.gnucash`). |
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
| **Action** | A Split's free-text action field (`Buy`, `Sell`, `Split`, …). |
| **Memo** | A Split's free-text note. **⚠** Distinct from the Transaction's *notes*. |
| **Reconcile state** | A Split's state: `n` new, `c` cleared, `y` reconciled, `f` frozen, `v` voided. |
| **Void** | Marks a Transaction void: values and amounts become zero, the former figures and a reason are kept in Slots. *Unvoid* restores them. |
| **Reverse** | Creates a new Transaction with every Split's value and amount negated, linked to the original. |
| **Imbalance** | The amount by which a Transaction's values fail to sum to zero. A *scrub* moves it to an `Imbalance-<CUR>` Account. |
| **Orphan** | A Split with no Account. A scrub moves it to an `Orphan-<CUR>` Account. |
| **Scrub** | A repair that restores an invariant (imbalance, orphans, lots). *Check & Repair* runs the scrubs over an Account, a subtree or the Book. |
| **Statement** | A bank statement's ending date and balance, used to reconcile an Account. |

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
| **Euro legacy currency** | A currency replaced by the euro at a fixed rate (DEM, FRF, …). |

## Investments

| Term | Meaning |
|------|---------|
| **Lot** | A group of Splits in one Account that buy and then sell the same holding. *Closed* when its amount sums to zero. |
| **Capital gain** | The realised gain or loss of a Lot, recorded as a gains Split in a separate gains Transaction. |
| **Lot policy** | The rule picking which Lot a selling Split draws from. GnuCash 5.17 uses FIFO. |
| **Stock split** | A change in the number of shares held with no change in value, recorded as a Split with zero value and action `Split`. |

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
| **Counter** | The next number for a kind of document or Owner, kept in the Book's `counters` slot. |

## red-ale

| Term | Meaning |
|------|---------|
| **Use case** | One operation on a Session, such as posting a Transaction. Each JSON-RPC method calls exactly one use case. |
| **Mutation** | A use case that changes the Book. Applies completely or not at all, and leaves the Book balanced. |
| **Published language** | The JSON-RPC schema: versioned, documented, and changed only additively within a major version. |
