# API: import

Importing transactions, accounts, prices, business documents and Owners from files, as GnuCash 5.17's importers do;
the matcher that decides what each imported transaction does; and the Import maps it learns. Scope and decisions:
[`../milestone3.md`](../milestone3.md) ("Import", "Compatibility in v0.3", "Decided"). Terms:
[`../glossary.md`](../glossary.md), "Import and export".

Every importer is translated from GnuCash's own code (C and C++, and Scheme for QIF), so red-ale reads every file as
GnuCash reads it. Each part of this page says first what GnuCash does, cited to the 5.17 source, then what red-ale
does. GUI code is not translated: what the user chooses in GnuCash's assistants and matcher window arrives as params
of `import.preview` or as changes in `import.apply`. Where GnuCash's behaviour can damage a book silently, red-ale
reports it instead, and the deviation is listed at the end.

## Contents

1. [Formats](#formats)
2. [The pipeline](#the-pipeline): `import.preview`, the Import proposal, `import.apply`, `import.discard`
3. [The matcher](#the-matcher): candidates, Match scores, Import actions, what each action does
4. [Import maps](#import-maps): plain and Bayesian, learning, `importMap.list`, `importMap.delete`
5. [CSV transactions](#csv-transactions)
6. [QIF](#qif)
7. [CSV accounts](#csv-accounts)
8. [CSV prices](#csv-prices)
9. [Business documents](#business-documents)
10. [Customers and vendors](#customers-and-vendors)
11. [Errors](#errors), [deviations](#deviations-from-gnucash), [tests](#tests)

## Formats

| `format` | GnuCash importer | Produces | Duplicates found by |
|----------|------------------|----------|---------------------|
| `csvTransactions` | Import Transactions from CSV (`5.17:gnucash/import-export/csv-imp/gnc-import-tx.cpp`) | Transactions | The matcher |
| `qif` | Import QIF (`5.17:gnucash/import-export/qif-imp/`, Scheme) | Accounts, commodities, transactions, prices | QIF's own finder (`qif-merge-groups.scm`) |
| `csvAccounts` | Import Accounts from CSV (`csv-imp/csv-account-import.c`) | Accounts | Full name |
| `csvPrices` | Import Prices from CSV (`csv-imp/gnc-import-price.cpp`) | Prices | Same day, either direction |
| `csvDocuments` | Import Bills & Invoices (`5.17:gnucash/import-export/bi-import/`) | Invoices or bills, posted when the file says | Document ID |
| `csvOwners` | Import Customers & Vendors (`5.17:gnucash/import-export/customer-import/`) | Customers or vendors | Owner ID |

Only `csvTransactions` goes through GnuCash's generic matcher: QIF has its own duplicate finder and does not call the
matcher (no `gnc_gen_trans*` in `qif-imp/`), and the business importers match by ID. All six still go through the same
two calls, so nothing reaches the Book without a preview.

There is no employee import: GnuCash has none (`5.17:gnucash/import-export/customer-import/dialog-customer-import-gui.c:345-346`).

## The pipeline

1. **`import.preview`** reads the file, builds what it holds, runs the matcher or the format's own duplicate finder,
   and returns an **Import proposal**. It changes nothing.
2. The caller reads the proposal, and may change an Import action, a chosen match or an account.
3. **`import.apply`** applies the proposal with those changes as **one Mutation**: everything or nothing.

The server holds each proposal by ID until it is applied, discarded, pushed out by 16 newer ones, or the book closes
([`../milestone3.md`](../milestone3.md), "Decided"). A proposal records the **Book revision** it was made at: the
number of Mutations the Session has applied. `import.apply` refuses it with `conflict` if the revision has moved, so a
proposal is never applied to a Book it was not computed from. Applying one is itself a Mutation, so the other
proposals held become stale.

### `import.preview` (read)

Common params:

| Param | Type | Meaning |
|-------|------|---------|
| `format` | string | One of the six above. The other params depend on it (the params schema is one object per format, chosen by `format`) |
| `files` | ImportFile[] | The file's content. Exactly one, except for `qif`, which takes several, as GnuCash's QIF assistant loads several before converting (`5.17:gnucash/import-export/qif-imp/assistant-qif-import.c:2362-2372`) |

`ImportFile`: `{ name?, text? , base64?, encoding? }`:

- exactly one of `text` (the content as a JSON string) and `base64` (the bytes);
- `encoding` names the encoding of `base64`'s bytes, as a WHATWG label decoded with `encoding_rs`; it is refused with
  `text`, which is already Unicode. Without it, the CSV import layout's `encoding` applies, else `utf-8`. Decoding
  follows WHATWG: a UTF-8 or UTF-16 byte-order mark wins over the label and is removed, as GnuCash's readers remove a
  UTF-8 one (`qif-file.scm:119-137`; the business regexes allow it, `dialog-bi-import-gui.c:312`);
- `name` is shown in messages, and gives QIF's default account name (below).

Bytes that are not valid in the encoding are an `import-file` error (`reason: "encoding"`) naming the line: nothing is
replaced or dropped. GnuCash guesses a CSV file's
encoding (`5.17:gnucash/import-export/csv-imp/gnc-tokenizer.cpp:64-114`) and falls back to the locale's charset for
other files; red-ale takes the encoding the client gives, which the CSV import layout can name.

The format params are under each format's section below. The result is the Import proposal.

Works on a read-only book: a preview reads only. Errors: `no-book`, `invalid-params`, `invalid` (an unknown layout, a
params combination the format refuses), `not-found` (an account or commodity a param names), `import-file` (the file
can't be read as the format at all).

### The Import proposal

| Field | Meaning |
|-------|---------|
| `id` | The proposal's ID, a string |
| `format` | As given |
| `bookRevision` | The Book revision it was made at |
| `thresholdDate` | The auto read-only threshold date at the time of the preview, or null with no threshold ([`book-options.md`](book-options.md)) |
| `summary` | Counts: records by Import action, `errors`, `imbalanced`, `readOnlyByDate` |
| `ignoredLines` | `[{ file, line, reason }]`: lines the reader skipped (lines a business regex did not match, lines skipped by the layout). Here and wherever a record is located, `file` is the file's 0-based position in `files` and `line` the 1-based record (for CSV, the row, which a quoted line break makes longer than one text line) |
| `errors` | `[{ file, line, column, message }]`: rows that can't be imported, with GnuCash's message for each; `column` is the 1-based position of the first column of the type at fault, or null when the layout has no column of that type. None of their records appear below |
| `warnings` | `[Warning]`: things the caller should know that don't stop the import and belong to no one record (each record's own are with it) |
| `newAccounts` | `[{ name, type, commodity }]`: accounts the import will create, by full name, parents first |
| `newCommodities` | `[{ id, fullname, fraction }]`: commodities it will create (QIF securities) |
| `transactions` | `ImportedTransaction[]`, for `csvTransactions` and `qif` |
| `accounts`, `prices`, `documents`, `owners` | The records of the other formats, each with an `index` and an `action` (sections below) |
| `csv`, `qif` | Format-specific state: the account names a CSV file used and where each resolved; QIF's date format choices, account, category and payee maps, and securities |

`ImportedTransaction`, one per transaction the file holds, in the order the importer builds them:

| Field | Meaning |
|-------|---------|
| `index` | Its position in `transactions`: the key `import.apply` changes it by |
| `source` | `{ file, lines: [first, last] }`: where it came from |
| `currency`, `datePosted`, `num`, `description`, `notes`, `voidReason` | As built |
| `splits` | `[{ account, accountName, value, amount, memo, action, reconcileState, reconcileDate }]`. `account` is a GUID, or null for an account to be created (`accountName` then names it). Split 0 is the *imported split*: the one in the account the file is a statement of, which the matcher matches on |
| `other` | The balancing split GnuCash would add on *add*: `{ account, accountName, from, value, amount }`. `from` says where the account came from: `file`, `bayes`, `map`, `caller`, or null when none was found. Shown whatever the action; null only when the imported split's value is zero, so no split would be added |
| `action` | The Import action the matcher (or the format) chose: `add`, `clear`, `update` or `skip` |
| `matches` | Candidate matches, best first: `[{ split, transaction, score, updateProposed, datePosted, description, num, memo, amount, value, reconcileState }]`. `score` is null for QIF, whose finder doesn't score |
| `selectedMatch` | The split GUID of the match `clear` or `update` would use, or null |
| `imbalance` | With the current action and accounts: `{ value, currency, account, accountName }`, the `Imbalance-<CUR>` split applying would make (`account` null when that account is to be created), or null. Worked out by applying the whole proposal to a copy of the Book |
| `readOnlyByDate` | True when `datePosted` is before `thresholdDate`: the transaction would be read-only by date once added |
| `warnings` | `[Warning]` for this transaction |

`Warning`: `{ code, message, index?, … }`, with codes such as `imbalance`, `read-only-by-date`, `missing-rate` (an
amount taken at a rate of 1, as GnuCash does), `no-match` (*clear* or *update* with no match selected),
`other-split-not-updated` (*update* of a transaction whose other split can't follow the new amount), `cannot-apply`
(the proposal as it stands would fail to apply, with the reason), `header-row`, `duplicate-id`, `not-posted`,
`no-split`.

**Imbalance.** GnuCash's importers do not refuse an unbalanced transaction: committing it adds an `Imbalance-<CUR>`
split (`xaccTransScrubImbalance` from `xaccTransCommitEdit`, `5.17:libgnucash/engine/Transaction.cpp:1549-1558`;
`5.17:libgnucash/engine/Scrub.cpp:445-464`). red-ale does the same ([`../milestone3.md`](../milestone3.md),
"Compatibility in v0.3"), so every such transaction is flagged with `imbalance` before anything changes, and its
`Imbalance-<CUR>` account is in `newAccounts` if it doesn't exist yet.

**Read-only by date.** GnuCash's importers check no auto read-only threshold. A transaction that would be read-only by
date is flagged, with the threshold date; the caller can skip it, and one left to be added is added. Clearing or
updating an existing transaction that is read-only by date is allowed too, as in GnuCash, and flagged the same way.
The importers join the list of what the threshold allows ([`book-options.md`](book-options.md), "Auto read-only
threshold").

### `import.apply` (destructive)

| Param | Type | Meaning |
|-------|------|---------|
| `proposal` | string | The proposal's ID |
| `changes` | ImportChange[] | The caller's changes, by index; default none |
| `skipErrors` | bool | Apply although `errors` is not empty, leaving those rows out: GnuCash's "Skip errors" (`5.17:gnucash/import-export/csv-imp/gnc-import-tx.cpp:298`). Default false |
| `appendText` | bool? | For *update*: append the imported description and notes to the matched ones instead of replacing them (below). Default: the import account's `import-append-text` slot, as GnuCash's checkbox defaults (`5.17:gnucash/import-export/import-main-matcher.cpp:504-516`) |

`ImportChange`: `{ index, action?, match?, account?, amount? }`:

- `action`: the new Import action. `clear` and `update` need a match. Which actions a format allows is in its section;
  `update` is refused when the Import setting `enable-update` is off, as GnuCash's matcher then hides it
  (`import-main-matcher.cpp:1736-1737`).
- `match`: the split GUID of one of the transaction's `matches`, as GnuCash's match picker chooses one.
- `account`: the account of the `other` split (AccountRef): GnuCash's "select transfer account" for an *add*.
- `amount`: that split's amount, when its account's commodity is not the transaction currency (GnuCash asks for an
  exchange rate there, `import-main-matcher.cpp:1145-1152`). Without one, the amount is the value, with a
  `missing-rate` warning, as GnuCash does (`5.17:gnucash/import-export/import-backend.cpp:910-921`).

Records not named keep the proposal's choice. Returns:

| Field | Meaning |
|-------|---------|
| `added`, `cleared`, `updated` | `[{ index, transaction }]` |
| `skipped` | Indexes |
| `accounts` | `{ created: [AccountRef], updated: [AccountRef] }` |
| `commodities` | Created commodities |
| `prices` | `{ added, replaced, duplicate }`, Price GUIDs |
| `documents`, `owners` | `{ created, updated, posted: [{ index, guid }], skipped: [index] }` (Owners are never posted) |
| `imbalanceSplits` | `[{ index, transaction, split, account, value }]`: every `Imbalance-<CUR>` split made |
| `learned` | `{ bayes, plain, csvAccountMap }`: Import map entries written |
| `warnings` | Every Imbalance split again, as a warning, every transaction added read-only by date, and the format's own warnings |

`import.apply` changes the Book (Effect target `book`). QIF also saves its QIF account map in the Configuration, as
GnuCash saves its map file at the end of an import (`5.17:gnucash/import-export/qif-imp/qif-guess-map.scm:296-299`).

Errors: `no-book`, `read-only`, `not-found` (`data.kind: "importProposal"`: unknown, applied, discarded, pushed out,
or from before the book was opened), `conflict` (`data.reason: "book-revision"`, with `data.proposalRevision` and
`data.bookRevision`), `invalid` (an index out of range, an action the format or settings refuse, `clear` or `update`
without a match, a match not in `matches`, a placeholder account, errors without `skipErrors`, a QIF proposal still
waiting for a date format). A proposal stays held after a failed apply.

### `import.discard` (read)

`{ proposal }` → `{ discarded: true }`. Drops a held proposal. Its effect is `read`: it changes neither the Book nor
the Configuration, only drops a cached result that a new preview recreates. `not-found` for an unknown ID.

## The matcher

GnuCash's generic import backend (`5.17:gnucash/import-export/import-backend.cpp`) and the non-GUI parts of its
matcher window (`import-main-matcher.cpp`), translated. `csvTransactions` uses it; nothing else does in v0.3.

### What GnuCash does

**Settings.** The Import settings are preferences (`5.17:gnucash/gschemas/org.gnucash.GnuCash.dialogs.import.generic.gschema.xml.in:3-47`),
read once per matcher (`5.17:gnucash/import-export/import-settings.cpp:81-110`); the thresholds are read as doubles
and truncated to integers, the ATM fee stays a double. Adding and clearing are always allowed (`:40-41`, `:93-94`).
The candidate window, `match_date_hardlimit`, is not a preference: 42 days, which the CSV importer passes
(`5.17:gnucash/import-export/csv-imp/assistant-csv-trans-import.cpp:671-672`; `import-settings.cpp:108`).

**Online IDs first.** An imported transaction whose split 0 has an `online_id` already on a split in the same account
is dropped before matching (`gnc_import_exists_online_id`, `import-backend.cpp:1050-1083`;
`import-main-matcher.cpp:2267-2275`). The account's IDs are gathered once per matcher, so duplicates within one file
are not caught. The CSV importer never sets an `online_id`.

**Order.** Transactions are prepended as they arrive (`import-main-matcher.cpp:2280-2282`) and matched and shown in
that list's order (`:2405-2444`), so the matcher works in **reverse order of arrival**. The CSV importer hands them
over sorted by posted date, ties in file order (`5.17:gnucash/import-export/csv-imp/gnc-import-tx.hpp:169`).

**Destination account.** When an imported transaction arrives, the account of its balancing split is looked up
(`gnc_import_TransInfo_new`, `import-backend.cpp:1127-1147`; `matchmap_find_destination`, `:501-524`): with
`use-bayes` on, from the Bayesian Import map of split 0's account, with the transaction's tokens; else from the plain
map's `desc` category, by the whole description, exact. An account the file names (CSV's Transfer Account) overrides
it (`:306-310`). The balancing split's value is minus the transaction's imbalance; its amount is the value when the
account is in the transaction currency, else value ÷ price when a price is known (`trans_info_calculate_dest_amount`,
`:1092-1124`).

**Candidates** (`filter_existing_splits_on_account_and_date`, `import-main-matcher.cpp:2309-2350`;
`create_hash_of_potential_matches`, `:2355-2379`): one query for the whole batch, for splits in any account of the
imported splits, posted from the batch's earliest date minus 42 days to its latest plus 42 days. Splits with an
`online_id` are never candidates, nor are splits of transactions still open. Reconciled and void splits are. Each
imported transaction is scored against the candidates in **its own split 0's account**.

**Scores** (`split_find_match`, `import-backend.cpp:579-784`), starting from 0:

| Test | Points | Lines |
|------|-------:|-------|
| Amounts (split 0's amount and the candidate's, as doubles) differ by less than 1e-6 | +3 | `:594-608` |
| Else they differ by at most the ATM fee threshold | +2 | `:610-617` |
| Else | −5 | `:619-625` |
| Posted dates on the same day (whole days, `|Δ seconds| / 86400`, truncated) | +3 | `:629-646` |
| Else at most `match-date-threshold` days apart | +2 | `:648-651` |
| Else more than `match-date-not-threshold` days apart | −5 | `:653-658` |
| Both numbers (`gnc_get_num_action`) set and equal, as text or as `strtol` integers | +4 | `:668-693` |
| Both set and different | −2 | `:695-700` |
| Both memos set and equal, case-folded | +2 | `:703-713` |
| Else equal in the first half of the longer one's bytes, ASCII case-insensitive, both longer than 1 | +1 | `:715-729` |
| Descriptions: the same two tests | +2 / +1 | `:733-761` |

`updateProposed` is `score < 6` taken after the amount and date tests: false only when both were exact (`:666`). A
candidate scoring below `match-threshold` is dropped (`:764-767`). The integer test keeps a GnuCash bug: only the
second `strtol` decides whether the conversion worked (`:682-686`), so an imported `abc` (which converts to 0) matches
an existing `0`.

**Action** (`gnc_import_TransInfo_init_matches`, `import-backend.cpp:1162-1200`): candidates are sorted by score, best
first, with a stable sort. The candidates were prepended twice, into their account's bucket and then into the match
list (`import-main-matcher.cpp:2375`, `import-backend.cpp:782`), so equal scores keep the query's order
(`xaccSplitOrder`). Then:

1. best ≥ `auto-clear-threshold`: *update* if `enable-update` and the best match's `updateProposed`, else *clear*;
2. else no match, or best ≤ `auto-add-threshold`: *add*;
3. else *skip* if `enable-skip`; else *update* if `enable-update` (whatever `updateProposed` says); else *add*.

**Conflicts** (`resolve_conflicts`, `import-main-matcher.cpp:409-458`): when several imported transactions have the
same existing transaction as their best match, the one with the highest score keeps it (the earlier row on a tie);
each other one drops its best match and takes its next (`gnc_import_TransInfo_remove_top_match`,
`import-backend.cpp:133-145`), and the scan restarts. The action is **not** recomputed for the new best match; a
transaction left with no match becomes *add*.

**Applying** (`on_matcher_ok_clicked`, `import-main-matcher.cpp:534-604`; `gnc_import_process_trans_item`,
`import-backend.cpp:885-1032`), row by row:

- *skip*: nothing; the imported transaction is destroyed.
- *add*: if the transaction is unbalanced and has a destination account, the balancing split is made (value, amount
  as above; no memo, action or reconcile state, though the CSV importer passed them, `:299-312`). Split 0 is set to
  reconcile state `c`, reconciled now (`:924-927`). The transaction is committed, so any imbalance left becomes an
  `Imbalance-<CUR>` split, and `xaccTransRecordPrice` records a price for each split in a priced account with source
  `user:split-import` (`:929-930`; `Transaction.cpp:2712-2807`).
- *clear* (`process_reconcile`, `:847-881`): the matched split goes to `c` if it was `n`; its reconcile date becomes
  now **whatever its state**; it takes split 0's `online_id` if there is one; learning (below); the imported
  transaction is destroyed.
- *update* (`:932-1003`): on the matched transaction, the posted date becomes the imported one; the matched split takes
  split 0's amount and value; if there is a balancing value and the matched split has exactly one other split
  (`xaccSplitGetOtherSplit`, `5.17:libgnucash/engine/Split.cpp:2161-2185`) in state `n`, that split's value becomes it
  and **so does its amount** (GnuCash reads the value where it meant the amount, `:968`, so the price branch at
  `:969-979` never runs); an other split not in `n` is left, and commit balances the transaction with an Imbalance
  split; split 0's memo replaces the matched one if not empty; the description is replaced by the imported one, **even
  when that is empty**, and the notes by the imported notes when there are any (an imported transaction with none
  passes a null to `xaccTransSetNotes`, which changes nothing, `5.17:libgnucash/engine/Transaction.cpp:2042-2046`);
  or, with "append", each becomes `old|new` unless the new text is already there after NFC normalisation (`update_desc_and_notes`, `maybe_append_string`, `:789-845`); then as *clear*.

The append choice is saved on the first row's account as the slot `import-append-text` (`"true"`, or removed when
off; `import-main-matcher.cpp:577-581`; `5.17:libgnucash/engine/Account.cpp:4119-4128`, `:2557-2561`).

### What red-ale does

The matcher is translated as above, with these choices:

- **The Import settings** are in the Configuration, under `import.settings`, with GnuCash's names and defaults:
  `enableSkip`, `enableUpdate`, `useBayes`, `matchThreshold` (1), `matchDateThreshold` (4), `matchDateNotThreshold`
  (14), `autoAddThreshold` (3), `autoClearThreshold` (6) as integers, and `atmFeeThreshold` (`"2.00"`) as a decimal
  string ([`../milestone3.md`](../milestone3.md), "One import pipeline"). The candidate window stays 42 days, as the
  CSV importer fixes it.
- **Amounts are compared exactly**: equal amounts score +3, amounts at most `atmFeeThreshold` apart +2. GnuCash
  compares doubles with a 1e-6 tolerance; the two differ only for amounts less than a millionth apart, and the float
  ban for money stands.
- **Text comparisons** follow GnuCash's: case-folding by Unicode, and the half-prefix test on bytes, ASCII
  case-insensitive. GnuCash's `safe_strcasecmp` collates by locale after folding
  (`5.17:libgnucash/engine/qofutil.cpp:82-97`); red-ale compares folded text bytewise, as it sorts.
- **Order, ties and conflicts** are GnuCash's: `index` is the order the format builds transactions in (CSV: by posted
  date, ties in file order), and the matcher and conflict resolution walk them in reverse, as GnuCash's prepended
  list does, so the same file against the same book gives the same proposal as GnuCash's matcher window.
- **Destroying the imported transaction** has no counterpart: an imported transaction is never in the Book until
  *add* puts it there.
- **Online IDs**: red-ale never writes one on import (no format it reads has them) but checks them as GnuCash does, so
  a CSV file can't re-add a transaction a bank file added; and *clear* and *update* copy none, there being none to
  copy.
- **Changing the destination account** in `import.apply` (`account`) is GnuCash's manual choice, and learns (below).

## Import maps

### What GnuCash does

Import maps are Slots on accounts (`5.17:libgnucash/engine/Account.cpp:5146-5777`).

**The plain map** is the frame `import-map/<category>/<key>`, each value a GUID: the target account
(`gnc_account_imap_find_account`, `:5156-5167`; `gnc_account_imap_add_account`, `:5195-5206`;
`gnc_account_imap_delete_account`, `:5209-5225`, which also removes the category and the frame when they empty).
Categories:

- `desc` and `memo` (`5.17:gnucash/import-export/import-backend.cpp:51-52`), on the **source** account (the account of
  the imported split). Learning writes both; lookup reads only `desc` (`:518-521`).
- `csv-account-map` (`5.17:gnucash/import-export/csv-imp/gnc-imp-props-tx.hpp:80`), on the **target** account itself,
  pointing at itself: a name in a CSV file mapped to that account. Found by scanning every account for the key
  (`gnc_account_imap_find_any`, `Account.cpp:5169-5192`).

**The Bayesian map** is kept in the *flat* form: each entry is one Slot directly on the source account whose key is
the whole path `import-map-bayes/<token>/<guid>` (slashes and all, one key) and whose value is an int64 count
(`:5598`; `change_imap_entry`, `:5540-5555`). The XML backend writes that key as it is
(`5.17:libgnucash/backend/xml/sixtp-dom-generators.cpp:315-321`).

**Tokens** (`TransactionGetTokens`, `5.17:gnucash/import-export/import-backend.cpp:457-496`), built once per imported
transaction, in this order, each new one put in front:

1. the description split on single spaces, empty pieces dropped, each token once (exact comparison);
2. the posted date's weekday name, `%A` in the locale, from the posted time **in UTC** (`gnc_gmtime`, `:476-485`),
   added without the duplicate check, so a description holding the weekday's name counts twice;
3. each split's memo, split the same way, each token once.

**Probability** (`gnc_account_imap_find_account_bayes`, `Account.cpp:5509-5538`, with `:5232-5346`): for each token in
list order, every flat key `import-map-bayes/<token>/` followed by exactly 32 characters is read, giving the token's
total count and each account's count. For each account, over **only the tokens it has a count for**, with
p = count ÷ token total in doubles: `product` is the product of the p's and `product_difference` of the (1 − p)'s. Its
probability is `int32_t(product / (product + product_difference) × 100000)`, truncated (`:5280-5298`). The highest
wins, the first on a tie (accounts in order of first appearance: tokens in list order, GUIDs in byte order), and only
at 90,000 or more (`:5509`). A GUID with no account gives none.

**Learning** (`matchmap_store_destination`, `import-backend.cpp:530-573`) runs in two places only:

- when the user **picks the destination account by hand** (`gnc_import_TransInfo_set_destacc` with
  `selected_manually`, `:226-227`), at once, before OK, and kept even if the import is cancelled;
- on *clear* and *update* (`process_reconcile`, `:873`), with the matched transaction's other account, when it has
  exactly one other split.

Never on *add*, and an account found by the maps is never learned again. With `use-bayes` on, every token's count for
the target goes up by one (`gnc_account_imap_add_account_bayes`, `Account.cpp:5558-5608`), and the Book feature
`Account GUID based bayesian with flat KVP` is set **every time** (`:5604`; `5.17:libgnucash/engine/gnc-features.h:59`).
With it off, `desc` → target and `memo` → target are written to the plain map, replacing what was there.

**Old Bayesian data** (`:5397-5507`): before 2.7 the map was nested frames, `import-map-bayes/<token…>/<account>`,
with the account by full name (by GUID in 2.7 betas), and a token with `/` split into deeper frames.
`check_import_map_data` converts the whole book to the flat form at the start of every Bayesian lookup, learning or
listing, unless the book has the flat feature or the conversion already ran in this process. Names are resolved by
full name, then with an old separator guessed and replaced (`:5348-5381`); a name that still doesn't resolve becomes
the all-zero GUID, not dropped; colliding keys overwrite. The feature is set when any account was converted.
GnuCash's tests: `5.17:libgnucash/engine/test/gtest-import-map.cpp:330-376`.

**The import map editor** (`5.17:gnucash/gnome/dialog-imap-editor.c`) lists, for every account, the Bayesian entries
with their counts, the plain entries by category (`desc`, `memo`, `csv-account-map`), and the *online* entries: the
account's `online_id`, its `hbci/account-id` and `hbci/bank-code`, and `ofx/associated-income-account`. It deletes one
entry (`gnc_account_delete_map_entry`, `Account.cpp:5735-5760`), all Bayesian entries of an account
(`gnc_account_delete_all_bayes_maps`, `:5762-5777`), a whole plain category, or an online entry's head slot
(`dialog-imap-editor.c:131-190`). "Delete invalid mappings" removes every entry whose account doesn't resolve, or whose
online ID is empty (`:216-238`, `:292-392`).

### What red-ale does

- **Same Slots, same form.** red-ale reads and writes exactly these Slots, and writes Bayesian data only in the flat
  form. The flat key is one Slot key holding `/`; the Backend must never split it (the Residue keeps it as read).
- **Bayesian scores in `f64`**, as GnuCash computes them, in GnuCash's token order and account order, so both pick
  the same account at the threshold. This one function allows `clippy::float_arithmetic`, with a comment saying why;
  no amount passes through it.
- **The weekday token** is the weekday of the posted time in UTC, named in red-ale's Locale (`C`: `Monday`…); a map
  trained by a French GnuCash holds `mardi` ([`../milestone3.md`](../milestone3.md), "Locales").
- **Learning is part of `import.apply`'s Mutation**, for the same cases as GnuCash: a transaction whose destination
  account the caller set (`account` in a change), and every *clear* and *update* whose matched transaction has one
  other split. Nothing is learned at preview, so a discarded proposal learns nothing; GnuCash learns as soon as the
  account is picked.
- **The Book feature** `Account GUID based bayesian with flat KVP` is set when Bayesian data is written, as GnuCash
  sets it, and no other. `Account GUID based Bayesian data` is never set.
- **Old Bayesian data** is read through the conversion in memory by `import.preview` and `importMap.list`, which may
  not change the Book. It is **written** converted, with the feature, by the first Mutation that writes Bayesian data
  or deletes an import map entry of that book; never on open or on save alone. (GnuCash also converts on a lookup or a
  listing; red-ale's read methods can't write, so the conversion waits for the next write, with the same result.)
- **`import-append-text`** is written as GnuCash writes it: every `import.apply` of a `csvTransactions` proposal
  sets it on the account of the first transaction in GnuCash's row order to the `appendText` used (`"true"`, or the
  Slot removed), so the next import defaults to the same choice.

### `importMap.list` (read)

| Param | Type | Meaning |
|-------|------|---------|
| `account` | AccountRef? | Only this source account (for `csvAccountMap`, the target, which holds it) |
| `kind` | string? | `bayes`, `desc`, `memo`, `csvAccountMap`, `onlineId`, `hbci` or `ofxIncomeAccount` |
| `invalidOnly` | bool | Only entries the editor's "Delete invalid mappings" would remove |

→ `{ entries: ImportMapEntry[] }`, by source account full name, then kind, then key, bytewise:

| Field | Meaning |
|-------|---------|
| `account` | `{ guid, fullName }`: the account holding the entry |
| `kind` | As above |
| `key` | The token (`bayes`), the text (`desc`, `memo`, `csvAccountMap`), or the online ID |
| `target` | `{ guid, fullName }`, or null when the GUID resolves to no account. For `onlineId` and `hbci`, null |
| `count` | For `bayes`: the count |
| `valid` | False when the editor counts it invalid |

### `importMap.delete` (destructive)

| Param | Type | Meaning |
|-------|------|---------|
| `entries` | `[{ account, kind, key?, target? }]` | Each: one entry, or with no `key`, every entry of that kind on that account (the editor's parent row). A `bayes` token is held once per target account: `target` (AccountRef) picks one, and without it the token goes for every target |
| `invalid` | bool | Instead of `entries`: every invalid entry in the book |

→ `{ deleted: ImportMapEntry[] }`. Deleting follows the editor: an empty plain category and an empty `import-map`
frame go too; deleting an online entry removes its head Slot (`online_id`, the `hbci` frame, or
`ofx/associated-income-account`). One Mutation. `not-found` for an entry that isn't there; `invalid` for both or
neither of `entries` and `invalid`.

## CSV transactions

### What GnuCash does

`GncTxImport` (`5.17:gnucash/import-export/csv-imp/gnc-import-tx.hpp:104-207`) tokenizes the file, maps columns to
properties, builds a draft transaction per line or per group of lines, and hands the drafts to the matcher, sorted by
date (`:169`). Citations in this section are to `5.17:gnucash/import-export/csv-imp/` unless they say otherwise.

**Column types** (`gnc-imp-props-tx.hpp:49-78`; saved names, English msgids, `gnc-imp-props-tx.cpp:57-83`):

| Level | Saved names |
|-------|-------------|
| Transaction | `Transaction ID`, `Date`, `Number`, `Description`, `Notes`, `Transaction Commodity`, `Void Reason` |
| Split | `Action`, `Account`, `Amount`, `Amount (Negated)`, `Value`, `Value (Negated)`, `Price`, `Memo`, `Reconciled`, `Reconcile Date` |
| Transfer split | `Transfer Action`, `Transfer Account`, `Transfer Amount`, `Transfer Amount (Negated)`, `Transfer Memo`, `Transfer Reconciled`, `Transfer Reconcile Date` |

`None` skips a column. The amount, value and transfer amount types may repeat and are summed (Σ positive − Σ negated,
`gnc-imp-props-tx.cpp:101-108`, `:572-638`, `:739-768`); every other type is used once. Old names are read as new
(`Deposit` → `Amount`, `Withdrawal` → `Amount (Negated)`, `Num` → `Number`; `gnc-imp-settings-csv-tx.cpp:255-262`).
Multi-split mode refuses the transfer types; single-line mode refuses `Transaction ID` (`gnc-imp-props-tx.cpp:89-123`).

**Required** (`verify_column_selections`, `gnc-import-tx.cpp:503-557`): a date, a description, an amount (or negated
amount), and an account column, or in single-line mode a base account instead. When a line's accounts involve more
than one commodity, also a price, value or (single-line) transfer amount (`gnc-imp-props-tx.cpp:391-424`).

**The CSV import layout** (GnuCash's *preset*), saved in the per-book state file under the group
`Import csv,transaction - <name>` (`gnc-imp-settings-csv-tx.cpp:47`; `gnc-imp-settings-csv.cpp:50-66`, `:122-252`;
file `<userdata>/books/<book>.gcm`, `5.17:libgnucash/app-utils/gnc-state.c:69-200`):

| Key | Meaning | Default |
|-----|---------|---------|
| `CsvFormat` | CSV (true) or fixed width | true |
| `Separators` | The separator characters, concatenated (any of space, tab, `,`, `:`, `;`, `-`, and custom) | `,` |
| `EnableEscape` | Backslash escapes | true |
| `ColumnWidths` | Fixed width only | |
| `SkipStartLines`, `SkipEndLines`, `SkipAltLines` | Lines to skip | 0, 0, false |
| `DateFormat` | 0 `y-m-d`, 1 `d-m-y`, 2 `m-d-y`, 3 `d-m`, 4 `m-d`, 5 Locale (`5.17:libgnucash/engine/gnc-datetime.cpp:95-162`) | 0 |
| `CurrencyFormat` | 0 Locale, 1 Period (`123,456.78`), 2 Comma (`123.456,78`) | 0 |
| `Encoding` | | UTF-8 |
| `MultiSplit` | Multi-split mode | false |
| `BaseAccount`, `BaseAccountGuid` | Single-line mode's account when there is no Account column | |
| `ColumnTypes` | The saved names, one per column | |

"Skip errors" is not saved. Built-in layouts (`gnc-imp-settings-csv-tx.cpp:59-135`): `No Settings`, and `GnuCash
Export Format` (one header line, multi-split, the columns of the complex CSV export: Date, Transaction ID, Number,
Description, Notes, Transaction Commodity, Void Reason, Action, Memo, Account, None, None, Amount, None, Value,
Reconciled, Reconcile Date, Price), and `GnuCash Export Format (4.x and older)` (the same without the two value
columns). Their date and number formats are not set (`:74-79`).

**Tokenizing** (`gnc-tokenizer-csv.cpp:53-152`): the file is converted to UTF-8 and every CR LF or lone CR becomes LF;
each line is trimmed; a line with an open quote is joined to the next with a space; `""` inside a field is a quote;
with escapes on, only `\\`, `\"` and `\n` escape; boost's `escaped_list_separator` splits fields, which are not
trimmed. Fixed width cuts each line into the widths, by characters, and trims each field (`gnc-tokenizer-fw.cpp`).
Line *i* is skipped when before `SkipStartLines`, within the last `SkipEndLines`, every second line after the start
with `SkipAltLines`, or when it has errors and "Skip errors" is on (`gnc-import-tx.cpp:279-300`).

**Dates** are parsed with `GncDate(value, format)` (`5.17:libgnucash/engine/gnc-datetime.cpp:707-755`): boost's
parser, then a regex search that tolerates text around the date; two-digit years below 69 are 20xx; the yearless
formats take this year. A posted date is stored at the day-neutral time, with the `date-posted` slot
(`gnc-imp-props-tx.cpp:348-349`).

**Numbers** (`parse_monetary`, `gnc-imp-props-tx.cpp:132-170`): an empty cell is 0; a cell with no digit is an error;
currency symbols, blanks and every `--` are removed; then `xaccParseAmountImport` with the locale's monetary
separators, or the period or comma format (`5.17:libgnucash/app-utils/gnc-ui-util.cpp:1721-2054`). That parser is
exact: integer digits and a power-of-ten denominator, kept (`1,000.00` is 100000/100). It stops at the first
unexpected character and succeeds (`12.34abc` is 12.34, `12.34-` is +12.34), and keeps at most 12 decimal places.

**Fields** (`GncPreTrans::set`, `gnc-imp-props-tx.cpp:228-300`; `GncPreSplit::set`, `:426-562`):

- an empty date or description is an error in single-line mode only;
- `Transaction Commodity` is looked up by unique name, then as a currency, then in each namespace; a non-currency is
  silently ignored when building;
- an `Account` or `Transfer Account` is resolved by the `csv-account-map` Import map, then by full name; an empty or
  unknown one is an error;
- `Reconciled` must be one of the letters `n c y f v` (`v` reads as `n`); an empty cell is an error;
- the transaction's num is set with `xaccTransSetNum`, whatever the Book's num field source.

**Grouping.** In single-line mode each line is a transaction. In multi-split mode a line joins the transaction above
when each of its transaction fields is empty or equal to that transaction's, and that transaction has no errors;
otherwise it starts a new one (`is_part_of`, `gnc-imp-props-tx.cpp:364-377`; `gnc-import-tx.cpp:831-885`). The
`Transaction ID` column only groups: it never becomes the GUID.

**Building** (`trans_properties_to_trans`, `gnc-import-tx.cpp:629-678`; `GncPreSplit::create_split`,
`gnc-imp-props-tx.cpp:712-867`): the currency is the commodity column's if it is a currency, else the first split
account's currency (or its parent's). A split's amount is Σ amount − Σ negated amount; its value, by the method that
rounds least: the value columns; the amount, in the transaction currency; minus the transfer amount; amount × price;
or the nearest price in the price database (none: value 0, logged). A transfer split (single-line mode) gets minus
the value, and an amount found the same way; with none, it is left to the matcher. Splits are rounded by the engine to
their SCUs. A split failing its essentials (no amount, state `y` without a date, no price for another commodity) is
**dropped with only a log line** (`:720-728`). Void reasons are applied when the *next* transaction is built, so the
file's last transaction is never voided (`gnc-import-tx.cpp:649-660`). The importer balances nothing itself.

Every transaction goes to the matcher (`assistant-csv-trans-import.cpp:2134-2218`), with the transfer account, price
and amount meant for the balancing split.

**Account names** (`assistant-csv-trans-import.cpp:1803-1921`): the match page lists every account name the file uses,
resolved by the map or full name; mapping one by hand writes `csv-account-map` on the chosen account.

### What red-ale does

**Params** of `import.preview` with `format: "csvTransactions"`:

| Param | Type | Meaning |
|-------|------|---------|
| `layout` | string or CsvImportLayout | A CSV import layout by name (a built-in one or one in the Configuration), or given whole |
| `baseAccount` | AccountRef? | Overrides the layout's |
| `accountNames` | `{ "<name in the file>": AccountRef }` | GnuCash's account match page: what each name the file uses means. Learned as `csv-account-map` at apply |

**The CSV import layout** is kept in the Configuration under `import.csvLayouts.<name>`. GnuCash's built-in layouts,
`GnuCash Export Format` and `GnuCash Export Format (4.x and older)`, are always there and read only, and win over a
saved layout of the same name, as GnuCash's preset list comes first (`config.schema` gives every field):

```json
{ "format": "csv", "separators": ",", "escape": true, "columnWidths": [],
  "skipStartLines": 1, "skipEndLines": 0, "skipAltLines": false,
  "dateFormat": "d-m-y", "currencyFormat": "comma", "encoding": "windows-1252",
  "multiSplit": false, "baseAccount": "Assets:Current Assets:Checking",
  "columns": ["Date", "Description", "None", "Amount", "Amount (Negated)"] }
```

Every field is a key of GnuCash's layout, with `dateFormat` and `currencyFormat` by name instead of index and the
column types by their saved names, so a GnuCash `.gcm` group can be carried over field for field. The Configuration
API (`config.set`) adds, changes and deletes layouts; `import.preview` never saves one. The base account is kept by
GUID and full name, as GnuCash keeps both.

**The proposal** has `csv.accountNames`: each account name the file uses, with the account it resolved to and how
(`map`, `fullName`, `param`), or null. A row whose account resolves to nothing is an error row, as in GnuCash, until
`accountNames` names it.

**Actions**: all four. Defaults are the matcher's.

**Translated as GnuCash does it**: the tokenizer, skipping, every date and number parser, grouping, the value
methods, the transfer split, and what reaches the matcher. Under the default `C` locale, `currencyFormat: "locale"`
reads as the period format.

**The `locale` date format** (CSV transactions and prices) is ICU's default date pattern for the `locale` setting's
LC_TIME, as GnuCash's (`DateFormat::createDateInstance (kDefault, …)`, `5.17:libgnucash/engine/gnc-datetime.cpp:646-703`;
no fallback when ICU refuses the text): `MMM d, y` under C and en_US (`Mar 1, 2024`), `d MMM y` under fr_FR
(`1 mars 2024`), `dd.MM.y` under de_DE. red-ale takes the pattern and CLDR's abbreviated and wide month names from
ICU4X (`Locale::icu_date_pattern`) and reads it strictly (`red_ale_locale::LocaleDateParser`): whitespace is loose
(around the text, and a space in the pattern matches any run or none), Arabic bidi marks are ignored, month names
match in any case, and digits may be ASCII or the locale's own; a two-digit year in a `y` field is in ICU's default
century (80 years back to 20 ahead). ICU's lenient parser also accepts what red-ale refuses: a day the month doesn't
have (rolled over), a number for a month name, missing or other literal text. ICU4X's CLDR is newer than the
oracle's ICU 74, so a pattern can differ where CLDR changed. A locale whose calendar is not Gregorian (fa_IR) is
refused: GnuCash reads its Persian year, month and day as Gregorian ones.

**Multi-currency checks** are GnuCash's: only the Account column (and the base account) count towards the currencies
a file uses (`UpdateCrossSplitCounters`, `is_multi_currency`, `5.17:gnucash/import-export/csv-imp/gnc-imp-props-tx.cpp:391-424`;
`verify_column_selections`, `gnc-import-tx.cpp:542-556`), so a transfer into an account in another
currency is not refused but gets a `missing-rate` warning unless a Price or Transfer Amount column gives the amount.
When the Account column does make a transaction multi-currency, the layout needs a price or (negated) value column
(single-line layouts may use a (negated) transfer amount column instead), else the preview fails with `import-file`
`reason: "columns"`; each such line without one of those values is an error row.

**Errors** of `import-file` from CSV: `reason: "tokenize"` (the tokenizer failed: an unclosed quote, a fixed-width
layout with no widths) and `reason: "columns"` (the layout misses a required column, or the file has no line to
import).

**Deviations, for the book's sake** (also in the table at the end):

- A split GnuCash would drop silently is an **error row** naming why.
- The last transaction's void reason is applied, as every other's is.

## QIF

### What GnuCash does

The QIF importer is Scheme in `5.17:gnucash/import-export/qif-imp/` (every file GPL-2.0-or-later; most by Bill
Gribble), driven by a C assistant, `assistant-qif-import.c`. Citations here are to that directory.

**Reading** (`qif-file.scm:79-566`): UTF-8 with a byte-order mark removed; lines end at CR or LF; each value that
isn't valid UTF-8 is converted from the locale's charset, or stripped, with a warning (`:176-192`). A `!` line switches
section (`:194-250`): `Type:Bank`, `Cash`, `CCard`, `Invst`, `Port`, `Oth A`, `Oth L`, `Oth S` (transactions),
`Type:Class`, `Type:Cat`, `Account`, `Type:Security`, `Type:Prices`, `Option:AutoSwitch` and `Clear:AutoSwitch`;
`Type:Memorized` and any other type are ignored. Transaction fields (`:261-392`): `D` date, `T` and `U` amount, `P`
payee, `A` address lines, `N` number (the action in investment sections), `C` cleared, `M` memo, `I` share price, `Q`
shares, `Y` security, `O` commission, `L` category; `S`, `E` and `$` start a split, give its memo and amount. `%` and
other letters are ignored. A transaction with no date is dropped with a warning. The first transaction of a section
that is an opening balance (payee `Opening Balance` and an `[Account]` category) goes to `Equity:Retained Earnings`
(`:606-629`; `qif-dialog-utils.scm:112-114`).

**Formats** (`qif-file.scm:668-1136`; `qif-parse.scm`): each field's format is decided once per file over every value
it has. Dates may be `m-d-y`, `d-m-y`, `y-m-d` or `y-d-m`, with `-`, `/`, `.` or `'` between parts, or 8 digits
(`qif-parse.scm:269-390`); when more than one fits, the assistant asks, with no default (`assistant-qif-import.c:2042-2083`),
and reparses the **transactions** only (`qif-file.scm:643-651`). Two-digit years: below 50 are 20xx, a leading `'`
drops two characters, `19134` is 2034 (`qif-parse.scm:118-152`). Numbers are `decimal` (`1,234.56`), `comma`
(`1.234,56`) or rational; an ambiguous one is taken as `decimal` silently; parsing is exact (`#e`), a `-` anywhere
negates, and an unreadable number is 0 (`qif-parse.scm:398-471`).

**The QIF account map** (`qif-guess-map.scm`): a file `qif-accounts-map` in the user data directory (`:114`) holding,
as Scheme data, the account, category and payee/memo maps, the securities (name, namespace, mnemonic) and the account
separator. QIF names with no mapping are guessed (`:362-480`): an existing account of an allowed type whose short name,
full name, or full name containing the QIF name matches, ignoring case; else a new account named as the QIF name (or
`Unspecified`, or the investment defaults: `Income:Dividends:<brokerage>:<security>`, `Expenses:Commissions:<brokerage>`
and so on, `qif-dialog-utils.scm:66-127`). QIF types map to allowed GnuCash types (`qif-parse.scm:159-177`: `Bank`,
`Port`, `Invst` → BANK; `CCard` → CREDIT; `Oth L` → LIABILITY or CREDIT; …). The map file is written only when the
import completes.

**Converting** (`qif-to-gnc.scm:257-556`): everything is built under a new root and merged in at the end. Accounts are
made shallowest first; an existing account of a compatible type and commodity is reused, an incompatible one gets a
new account `<name> 2` (`:62-240`); placeholders are never set. Files are converted in descending order of transaction
count, each in file order.

- *Bank transactions* (`:565-720`): description from the payee; num by the num field source; memo to the notes (or the
  description when there is no payee); the near split's state from `C` (`*`/`c` → `c`, `x`/`r` → `y`), else the
  default status preference; one far split per QIF split, value = amount, account from `[Account]`, the category, the
  payee or the memo, else `Unspecified`.
- *Investment transactions* (`:725-912`): by action (`qif-parse.scm:194-239`, German names included). Buy and the
  reinvest actions: shares in the stock account `<brokerage>:<security>`, cash out of the brokerage or a gains or
  income account; Sell: the reverse; Div, IntInc, CGLong, CGMid, CGShort, MiscInc, RtrnCap, XIn: cash in from an
  income account; XOut, MiscExp, MargInt: cash out; ShrsIn, ShrsOut: shares against `Equity:Retained Earnings`;
  StkSplit: two splits in the stock account, the balance × Q/10 in and the balance out, with values that cancel,
  using the account's balance at that moment (in the tree being built, so it depends on conversion order); a commission gets its own
  split in `Expenses:Commissions:<brokerage>`. The `x` forms move cash to the `L` account. A payee starting `**VOID**`
  voids the transaction.
- Securities: a new commodity in the namespace the user chose (default the saved one, else `NONCURRENCY`) with
  fraction 100000 (`qif-dialog-utils.scm:725-837`).
- Each transaction is committed, so an imbalance becomes an Imbalance split, and `xaccTransRecordPrice` records
  `user:split-import` prices (`qif-to-gnc.scm:436-458`).
- `!Type:Prices` lines (`:462-542`, `:1371-1455`): source `user:price`, type `unknown`, in the default currency, at
  10:59 local; a price with one already on that local day is skipped.

**Transfers between QIF files** (`qif-to-gnc.scm:929-1326`): a split to `[Account]` in one file and the matching
transaction in that account's file are merged into one: same date exactly, opposite amounts (one to one, one to many,
or many to one); the survivor is the transaction with more splits, else the investment one.

**Duplicates of existing transactions** (`gnc:account-tree-find-duplicates`, `qif-merge-groups.scm:44-162`): for each
new transaction, existing ones in the account of the same full name with a split of exactly equal value, posted within
7 days (`:73`, `:133-142`). The user ticks at most one per new transaction; nothing is ticked by default. Ticked ones
are not imported (`gnc:prune-matching-transactions`, `:175-184`); existing transactions are never changed.

**Choices** (the assistant's pages, `assistant-qif-import.c`): the date format when ambiguous; a default account name
for a file with no `!Account` (from the file name, `qif-objects.scm:657-675`); the account, category and payee maps;
the default currency (`gnc_default_currency`); each new security's namespace, mnemonic and name; the duplicates.

GnuCash's tests: `test/test-qif-parse.scm` (dates, years, numbers, categories, actions), `test/test-qif-imp.scm`
(price lines), `test/test-qif-merge-groups.scm` (the duplicate finder).

### What red-ale does

One Rust module per `.scm` file, keeping its logic and guesses ([`../milestone3.md`](../milestone3.md), "Translated
from GnuCash").

**Params** of `import.preview` with `format: "qif"`:

| Param | Type | Meaning |
|-------|------|---------|
| `files` | ImportFile[] | One or more, each optionally with `defaultAccount`: the account for its transactions with no `!Account`. Default from the file name, as GnuCash's |
| `dateFormat` | string? | `m-d-y`, `d-m-y`, `y-m-d` or `y-d-m`, when the dates are ambiguous |
| `currency` | CommodityId? | The default currency; default `defaultCurrency` ([`quotes.md`](quotes.md)) |
| `defaultReconcileState` | string | `n`, `c` or `y` for transactions with no `C`; default `n`, as GnuCash's preference |
| `accounts`, `categories`, `payees` | `{ "<QIF name>": "<full name>" }` | Override the guessed maps: an existing account, or a new one to create |
| `securities` | `{ "<QIF name>": { namespace, mnemonic, fullname } }` | Override a new security |

**The proposal** has `qif`: `dateFormats` (the formats that fit, when there is more than one and `dateFormat` is
missing: the proposal then holds no transactions and can't be applied until a new preview gives one), `accounts`,
`categories` and `payees` (`[{ qifName, account, type, new, guessed }]`), and `securities`. `transactions` holds the
converted transactions; merged transfer pairs appear once. Each has the actions `add` and `skip` only; its `matches`
are QIF's duplicate candidates, `score` null, and the default action is `add`, as nothing is ticked in GnuCash.
Choosing `skip` is ticking a duplicate. `clear` and `update` are refused for QIF.

**The QIF account map** lives in the Configuration under `import.qifAccountMap`, as JSON: `accounts`, `categories`,
`payees` (QIF name → full name and allowed types) and `securities`. `import.apply` saves the maps used, merged over
what was there, as GnuCash's `save-map-prefs` does. Reading GnuCash's own `qif-accounts-map` file (Scheme data read
with `read`, never evaluated) is left for later.

**Translated as GnuCash does it**, including the guesses, the account names (in English: red-ale has no translations;
GnuCash's are translated), the reversed split order, the conversion order by file size, the stock split arithmetic,
exact numbers, transfer merging and the duplicate finder.

**Deviations, for the book's sake:**

- A number that doesn't parse is an **error row**, not 0.
- A day that doesn't exist (31/02) is an error row; GnuCash posts the transaction today (`Transaction.cpp:1942-1948`).
- An unknown investment action is an error row; GnuCash leaves the transaction to the imbalance scrub.
- An ambiguous price date is decided by `dateFormat` too; GnuCash reparses only transaction dates and then fails on
  the prices.

## CSV accounts

### What GnuCash does

The account importer (`5.17:gnucash/import-export/csv-imp/csv-account-import.c`, `assistant-csv-account-import.c`)
reads the 12 columns of GnuCash's own tree export ([`export.md`](export.md)) with a regex built for the separator:
`,` (default), `;` or `:`, or a regex the user edits (`assistant-csv-account-import.c:112-133`, `:276-310`). Fields
may be quoted, with `""` for a quote (`csv-account-import.c:44-74`). Matching stops at the first line that doesn't
fit, silently. If the first row's Type is `Type`, one header row is assumed (`:316-319`, `:405-408`). Text that isn't
UTF-8 is converted from the locale's charset.

For each row (`csv_account_import`, `csv-account-import.c:201-374`), by **full name**:

- *An existing account* is updated: code, description, notes and colour, each only when the cell isn't empty (an
  unparseable colour clears it). Type, name, commodity, hidden, placeholder and tax are never changed.
- *A new account* needs its name at the end of its full name; its parent is the full name less the name and one
  separator character; a missing parent puts it at the top level with an error. Its commodity is looked up by
  namespace and symbol (missing: the row is skipped). Type from the type's name (an unknown one becomes `INVALID`),
  hidden and placeholder from `T`. Tax Info is read and never used.

The summary counts added and updated accounts and lists errors.

### What red-ale does

**Params**: `separator` (`comma`, `semicolon` or `colon`; default `comma`) and `headerRows` (records to leave out at
the start; default: one when the first record's Type is `Type`, as GnuCash detects it). One file, UTF-8 unless it
names another `encoding`. Characters XML can't hold are removed, as GnuCash's `gnc_utf8_strip_invalid` does, with an
`invalid-characters` warning. A file with no account rows is an `import-file` error (`reason: "columns"`).

The proposal's `accounts`, one record per row in file order:

| Field | Meaning |
|-------|---------|
| `index`, `line` | Its position (the key `import.apply` changes it by) and its record in the file |
| `fullName` | The account's full name, by which it is found |
| `action` | `add` (create it), `update` (change the fields in `changes`) or `skip` (it exists and the row changes nothing) |
| `account` | The existing account an `update` changes; null for an `add`, or for an account an earlier row adds |
| `fields` | The row's cells as read: `type`, `name`, `code`, `description`, `color`, `notes`, `symbol`, `namespace`, `hidden`, `taxInfo`, `placeholder` |
| `changes` | `[{ field, from, to }]`: the fields an update changes (`code`, `description`, `notes`, `color`) |
| `warnings` | `color` (a colour GnuCash can't parse, left as it was) and `not-updated` (cells that differ from the account but that GnuCash's import never changes) |

Rows run in file order on a copy of the Book, as applying runs them, so a parent an earlier row creates is there for
the rows after it, and `newAccounts` lists what the import creates. The caller may only change an `add` or `update`
to `skip`; a row that then fails (its parent skipped, say) fails the whole apply. `import.apply` reports the accounts
in `accounts.created` and `accounts.updated`. Translated in `crates/red-ale-import/src/csv/accounts/`.

**Deviations**, so that nothing lands where the file didn't say:

- A record that doesn't have the twelve fields, text after a closing quote and a quote never closed are error rows,
  and the records after them are still read; GnuCash's regex stops at the first record that doesn't fit, silently.
  A record ends at a line break outside quotes; GnuCash lets an unquoted field run over a line break.
- A missing parent, an unknown commodity, a type GnuCash doesn't know and a name that isn't the last part of the full
  name are error rows. GnuCash creates a top-level account, skips the row, creates an `INVALID` account, or cuts the
  full name by length at the wrong place (`Assets:Bank` named `Ass` looks for a parent `Asse`).
- A new account goes through `account.create`'s rules (a type allowed under its parent's, a commodity fit for the
  type, no `:` in the name); a row breaking one is an error row.
- A colour GnuCash can't parse leaves the account's colour as it was, with a warning; GnuCash clears it. Colours by
  name (`red`) are not recognised, since red-ale has no GDK colour table.
- An existing account whose cells change nothing is `skip`; GnuCash counts it updated.
- The custom regex is not offered.

## CSV prices

### What GnuCash does

The price importer (`5.17:gnucash/import-export/csv-imp/gnc-import-price.cpp`, `gnc-imp-props-price.cpp`) shares the
transaction importer's tokenizer, skipping, date and currency formats, and layouts (group `Import csv,price - <name>`,
with `PriceFromCommodity` and `PriceToCurrency`; `gnc-imp-settings-csv-price.cpp:47-228`). Columns: `Date`, `Amount`,
`From Symbol`, `From Namespace`, `Currency To` (`gnc-imp-props-price.cpp:51-58`); a default "Commodity From" and
"Currency To" can stand in for the columns. An empty amount is an error; currency symbols are removed; zero and
negative prices are accepted. The commodity is found by unique name, then by namespace and symbol.

Each price (`create_price`, `gnc-imp-props-price.cpp:295-366`): time at the day-neutral time; source `user:price`;
type `last`; value converted to the currency's SCU × 10000 (`COMMODITY_DENOM_MULT`,
`5.17:libgnucash/engine/gnc-pricedb.h:286`), half up. A price of the pair already on that day, **in either
direction** (`gnc_pricedb_lookup_day_t64`), is replaced when "Allow existing prices to be over written" is ticked
(default off, not saved), else the line counts as a duplicate. The summary counts added, duplicate and replaced
prices; failed lines aren't counted.

### What red-ale does

**Params**: `layout` (a CSV price layout by name, in `import.csvPriceLayouts.<name>`, or whole), `overwrite` (bool,
default false: GnuCash's "Allow existing prices to be over written", not saved with the layout). A price layout has
the CSV transaction layout's reading fields (`format`, `separators`, `escape`, `columnWidths`, `skipStartLines`,
`skipEndLines`, `skipAltLines`, `dateFormat`, `currencyFormat`, `encoding`), plus `fromCommodity` (a commodity ID such
as `NASDAQ::ACME`, the default "Commodity From"), `toCurrency` (a currency, the default "Currency To"; `USD` is
read as `CURRENCY::USD`) and `columns`, each one of GnuCash's names: `None`, `Date`, `Amount`, `From Symbol`, `From
Namespace`, `Currency To`. The preset `No Settings` is built in. One file.

Each cell is parsed as GnuCash parses it, the errors in its words (`"<column>: <reason>"`); amounts by
`parse_amount_price`, which removes only currency symbols. A file whose columns can't give a price (no `Date`, no
`Amount`, no commodity column or default, no currency column or default) is an `import-file` error (`reason:
"columns"`) with GnuCash's messages.

The proposal's `prices`, one record per line in file order:

| Field | Meaning |
|-------|---------|
| `index`, `line` | Its position and its record in the file |
| `commodity`, `currency` | The pair, as commodity IDs |
| `date`, `time` | The line's date and the price's time: the date's day-neutral time (10:59 UTC) |
| `amount` | The amount as the file writes it |
| `value` | The value stored: the amount at the currency's fraction × 10000, rounded half up |
| `action` | `add`; with a same-day price, `update` (replace it; proposed with `overwrite`) or `skip` (a duplicate; proposed without it) |
| `existing` | The same-day price of the pair, in either direction: `{ price }` in the Book, or `{ index }` for one an earlier line adds |

The caller may choose `add`, `update` or `skip` for any price. Lines run in file order on a copy of the Book, so a
second line for the same day meets the first's price. Each price is added through `price.add`'s code
([`prices.md`](prices.md)) with source `user:price` and type `last`, after the old price is removed for an `update`;
`import.apply` reports `prices.added`, `prices.replaced` (the new prices that replaced one) and `prices.duplicate`
(the same-day prices kept). Translated in `crates/red-ale-import/src/csv/prices/`.

**Deviations:**

- A line failing its essentials (a short line, a cell that doesn't parse) is an error row; GnuCash skips it with
  only a log line.
- A price `gnc_pricedb_add_price` would refuse once the old one is removed (another same-day price from a
  better-ranked source) is an error row and the old price stays; GnuCash has already removed it.
- A layout with both a default and the column it stands for is refused (GnuCash's assistant clears one when the
  other is chosen), and an unknown default commodity is a `not-found` error (GnuCash's preset loader throws).
- `escape` is honoured; GnuCash doesn't re-apply a price preset's `EnableEscape`
  (`5.17:gnucash/import-export/csv-imp/gnc-import-price.cpp:313-314`).
- A column name GnuCash doesn't know is refused; GnuCash loads it as `None`.

## Business documents

### What GnuCash does

Import Bills & Invoices (`5.17:gnucash/import-export/bi-import/`; GPL-2.0-or-later, © 2009 Sebastian Held, Rob Laan,
Mike Evans). Citations here are to that directory unless they say otherwise.

**Layout**: 22 fields per line (`dialog-bi-import.h:41-47`): `id`, `date_opened`, `owner_id`, `billing_id`, `notes`,
`date`, `desc`, `action`, `account`, `quantity`, `price`, `disc_type`, `disc_how`, `discount`, `taxable`,
`taxincluded`, `tax_table`, `date_posted`, `due_date`, `account_posted`, `memo_posted`, `accu_splits`. Read with a
regex per line (`dialog-bi-import.c:131-224`; `dialog-bi-import-gui.c:312-360`): semicolon (default), comma,
semicolon or comma with optional quotes (`""` inside quotes allowed only in `notes` and `desc`), or a custom regex.
Lines that don't match are counted as ignored. There is no header handling. Lines are read 999 bytes at a time
(`dialog-bi-import.c:159-172`). Values are trimmed.

**Choices**: bill (default) or invoice (`dialog-bi-import-gui.c:106`); no credit notes. Whether to update existing
documents is asked once, when the first one is met (`dialog-bi-import.c:730-740`).

**Checks** (`gnc_bi_import_fix_bis`, `dialog-bi-import.c:275-581`): consecutive rows with the same `id` are one
document (an empty `id` continues the one above); its first row holds the document's fields. An error in any row
drops **every row of the document**: no `id` in the first row, no owner or an unknown one (vendor for bills, customer
for invoices, by exact ID, `5.17:libgnucash/engine/gncIDSearch.c:104-159`), with `date_posted` set: an invalid
`date_posted`, an unknown `account_posted` or one not A/P (bills) or A/R (invoices); an empty `price`; an unknown
`account`. Fixes: an invalid `due_date` becomes `date_posted`; an invalid `date_opened` becomes today; an empty
`quantity` becomes 1; an invalid `date` becomes `date_opened`. Dates are checked with `strptime` in the date format
preference (`dialog-bi-import-helper.c:54-66`) and read with `qof_scan_date` (`5.17:libgnucash/engine/gnc-date.cpp:742-988`).
Accounts are looked up by full name, or by leaf name when the "show leaf account names" preference is on
(`gnc_account_lookup_for_register`, `5.17:libgnucash/app-utils/gnc-ui-util.cpp:448-459`).

**Creating** (`gnc_bi_import_create_bis`, `dialog-bi-import.c:605-951`): an existing document is found by ID among
invoices or bills (not credit notes). A new one gets the ID, the owner, **the owner's currency**, `date_opened` (start
of the local day), billing ID, notes and active; not the owner's billing terms or tax table. An existing posted one is
skipped; an unposted one, if the user said yes, gets the rows' entries appended, its own fields unchanged. Each row
becomes an Entry: date, date entered now, description, action, quantity, account, price, taxable, tax included and
tax table (by exact name; an unknown name means none, silently); for invoices also notes, discount, discount type
(`%` or empty → percent, else value) and how (`=` same time, `>` post-tax, else pre-tax). Text flags are true when they
start with `y` or `t`, or are `1` or `x` (`dialog-bi-import-helper.c:34-102`). Quotes are removed from `desc` and
`notes` by `un_escape` (`dialog-bi-import.c:957-974`).

Numbers go through GnuCash's expression parser (`5.17:libgnucash/app-utils/gnc-exp-parser.c`): arithmetic on exact
numerics, the locale's monetary separators, the result reduced (`10.50` is 21/2). Its return value is ignored, so
**a number that doesn't parse is 0**.

**Posting**: when `date_posted` is set, the document is posted to `account_posted` with that date, the due date,
`memo_posted` and `accu_splits` (`gncInvoicePostToAccount`, `5.17:libgnucash/engine/gncInvoice.c:1442-1445`), with the
auto-pay preference (default off). Not when an entry's account or tax account is in another commodity, or the
document's currency is not `account_posted`'s; the document is then left unposted with a message.

GnuCash has no tests for this importer.

### What red-ale does

Code: `crates/red-ale-import/src/business/` (`documents.rs`, with `reader.rs`, `date.rs` and `expr.rs` shared with
Owners).

**Params** of `format: "csvDocuments"`:

| Param | Type | Default | Meaning |
|-------|------|---------|---------|
| `documentKind` | string | `bill` | `bill` or `invoice`, as GnuCash's choice |
| `separator` | string | `semicolon` | `semicolon`, `comma`, `semicolonQuoted`, `commaQuoted` |
| `dateFormat` | string | `reports.dateFormat` | `us`, `uk`, `ce`, `iso` or `locale`: GnuCash's date format preference |
| `accountNames` | string | `full` | `full` or `leaf`: GnuCash's "show leaf account names" preference. `leaf` finds the first account of that name breadth-first, as `gnc_account_lookup_by_name` does |
| `updateExisting` | bool | false | GnuCash's question, answered up front |
| `autoPay` | bool | false | GnuCash's auto-pay preference for the kind (`5.17:gnucash/gschemas/org.gnucash.GnuCash.dialogs.business.gschema.xml.in:110-111`) |

Numbers use the locale's monetary decimal point and thousands separator (`mon_decimal_point`, `mon_thousands_sep`),
as GnuCash's expression parser does.

**The proposal's `documents`**, one per group of rows, in file order:

| Field | Meaning |
|-------|---------|
| `index`, `source`, `lines` | Its position, `{ file, lines: [first, last] }`, and every line it was built from |
| `id`, `type`, `owner`, `ownerId` | The document ID, `invoice` or `bill`, the Owner (`{ kind, guid }`) and its ID as written |
| `existing` | The GUID of the document in the Book it updates or skips, or null |
| `sameAs` | The index of an earlier record in this file with the same ID: this one updates (or skips) what that one creates |
| `action` | `add`; `update` (an unposted document, with `updateExisting`: entries appended, its own fields unchanged); or `skip` (an existing one without `updateExisting`, or a posted one) |
| `currency`, `dateOpened`, `billingId`, `notes` | What a new document takes: the Owner's currency, the opening day, the billing ID and notes |
| `entries` | `[{ line, date, description, action, notes, account, accountName, quantity, price, discount, discountType, discountHow, taxable, taxIncluded, taxTable }]`: the entries the rows add. `discount*` and `notes` are set for invoices only, as in GnuCash |
| `post` | Null without a posting date; else `{ willPost, reason, account, accountName, date, dueDate, memo, accumulateSplits, autoPay }`. `reason` says why `willPost` is false: GnuCash's two messages, or red-ale's posting rules (below) |
| `totals` | `{ subtotal, tax, total }` of the document as applying would leave it, from a trial on a copy of the Book |
| `readOnlyByDate` | True when it would be posted before the auto read-only threshold |
| `warnings` | `fixed` (each of GnuCash's fixes), `exists`, `posted`, `not-posted`, `duplicate-id`, `duplicate-entry` (an appended entry the document already has: same date, description, account, quantity and price), `read-only-by-date` |

Rows the reader didn't match are in `ignoredLines` with `reason: "noMatch"`. A line with characters XML can't hold
gets a `stripped-characters` warning; GnuCash strips them silently (`gnc_utf8_strip_invalid`,
`dialog-bi-import.c:182`). Error rows carry GnuCash's messages (`invoice B1/2: price not set.`), and the
first row of a dropped document also gets GnuCash's `Error(s) in invoice B1, all rows of this invoice ignored.`

Each document is worked out on a copy of the Book through the same code `import.apply` runs, so what red-ale's rules
refuse shows in the proposal, as an error row (an entry account) or a `not-posted` reason (the posting), before
anything changes.

**Apply.** `changes` may only set a record's `action` to `skip`. Skipping a record another one has as `sameAs` is
refused unless that one is skipped too. In file order, each document is created (`add_invoice`) or found, its entries
added (`add_entry`: date at noon of the local day, date entered now, as GnuCash), and, when `willPost`, posted with
`invoice.post`'s operation ([`business-posting.md`](business-posting.md)), the translation of the
`gncInvoicePostToAccount` GnuCash calls: the posting date, the due date at local midnight (`gnc_dmy2time64`,
`dialog-bi-import.c:879-886`), `memo_posted`, `accu_splits` and `autoPay`. The result's `documents`:
`{ created, updated, posted: [{ index, guid }], skipped: [index] }`.

**Deviations, for the book's sake:**

- A number that doesn't parse is an **error** that drops its document, as other errors do; GnuCash makes it 0.
  Arithmetic (`2*3.5`, `(1+2)/3`) is exact, as GnuCash's; variables and function calls are not accepted (GnuCash
  evaluates functions in Guile); a group separator after the decimal point is an error.
- Dates are checked and read by the same parser (`qof_scan_date`'s translation), so the date checked is the date
  stored. A day the month doesn't have, or a digit straight after the date, is invalid; GnuCash's `strptime`
  check passes both, and `qof_scan_date` rolls such a day over (`5.17:libgnucash/engine/gnc-date.cpp:977-982`).
- An unknown tax table name is an error; GnuCash leaves the entry without a tax table, silently.
- An entry date fixed to an invalid opening date takes the fixed opening date (today); GnuCash's entry is left
  without a date.
- An entry account red-ale's ledger rules refuse (a placeholder, or a type an entry can't use) is an error row; a
  posting they refuse leaves the document unposted with a `not-posted` warning.
- Lines are read whole: GnuCash splits a line longer than 999 bytes into two. A trailing `\r` is removed.
- `notes` is un-escaped once; GnuCash un-escapes the first entry's notes twice. A lone `"` is kept.
- A document ID that an existing credit note has gets a `duplicate-id` warning; GnuCash silently creates a second
  document with that ID.
- The custom regex is not offered.

## Customers and vendors

### What GnuCash does

Import Customers & Vendors (`5.17:gnucash/import-export/customer-import/`; GPL-2.0-or-later, © 2009 Sebastian Held).
Citations here are to that directory.

**Layout**: 19 fields (`dialog-customer-import.h:39-46`): `id`, `company`, `name`, `addr1`–`addr4`, `phone`, `fax`,
`email`, `notes`, `shipname`, `shipaddr1`–`shipaddr4`, `shipphone`, `shipfax`, `shipemail`, with the same four
separators and custom regex as business documents (`dialog-customer-import-gui.c:285-328`), no header handling, and
customer (default) or vendor.

**Fix-up** (`dialog-customer-import.c:206-266`): an empty `company` takes `name`; a row with neither is dropped.

**Creating** (`:268-421`): an empty `id` takes the next number from the Owner's Counter (which advances it). An Owner
with the ID is updated, else one is created with currency `gnc_default_currency()`. Either way, **every** field is set
from the row, empty cells included: name from `company`, notes, the address (its name from `name`), and for customers
the shipping address (vendors have none; those columns are ignored); and the Owner is made active. Terms, tax tables
and discounts are not touched. A header row is imported as an Owner like any other.

### What red-ale does

Code: `crates/red-ale-import/src/business/owners.rs`.

**Params** of `format: "csvOwners"`: `ownerKind` (`customer`, default, or `vendor`) and `separator` as for documents.
New Owners take `defaultCurrency` ([`quotes.md`](quotes.md)), GnuCash's default currency preference; when it is unset
or not a currency the book knows, the locale's currency, else USD.

**The proposal's `owners`**, one per row, in file order:

| Field | Meaning |
|-------|---------|
| `index`, `line`, `kind` | Its position, its line, `customer` or `vendor` |
| `id`, `idFromCounter` | The ID: the file's, or, for an empty one, the number the Owner's Counter would give (the Counter advances at apply, as in GnuCash, and each such row takes the next) |
| `existing`, `sameAs` | The Owner in the Book it updates, or the earlier row of this file that creates it |
| `action` | `add`, `update` or `skip` |
| `alternative` | For a header row proposed as `skip`: the action GnuCash would take, which `import.apply` accepts |
| `fields` | `{ company, address: { name, addr1–addr4, phone, fax, email }, notes, shipping }`: every field the row sets, empty ones included; `shipping` is null for vendors |
| `currency` | A new Owner's currency |
| `changes` | For `update`: each field that would change (`company`, `notes`, `address.addr1`, `shipping.email`, `active`, …) |
| `reactivate` | True when an inactive Owner would become active |
| `warnings` | `fixed` (no company; the name is used), `header-row`, `unchanged`, `ignored-columns`, `id-in-use` (the Counter's next number is already an Owner's ID, so GnuCash updates that Owner) |

**Apply.** `changes` may set `skip`, or a header row's `alternative`. Owners are created or updated with the
`customer.*` and `vendor.*` operations ([`business-contacts.md`](business-contacts.md)): every field from the row and
active; terms, tax tables, discounts and credit limits untouched. The Counter is set past the highest number used.
The result's `owners`: `{ created, updated: [{ index, guid }], posted: [], skipped: [index] }`.

**Deviations:**

- A first row whose `id` and `company` cells are those columns' names gets a `header-row` warning and the action
  `skip`; GnuCash imports it as an Owner named `company`.
- A row with neither company nor name is an error row; GnuCash drops it, counting it only.
- A row that would change nothing is proposed as `skip`, with an `unchanged` warning (not for an empty ID, which
  always takes a new number).
- A vendor row's shipping cells get an `ignored-columns` warning; GnuCash ignores them silently.
- The quoted semicolon layout reads its last field as it reads the others; GnuCash's expression reads it with
  `[^;]*`, so keeps its quotes (`dialog-customer-import-gui.c:303`).
- A custom regex is not offered (GnuCash crashes on one lacking `company` or `name`).

## Errors

Existing codes, as above, and one new one, for [`../api.md`](../api.md):

| Code | Name | When |
|------|------|------|
| 4000 | `import-file` | The file can't be read as the format at all: bytes invalid in its encoding, not QIF ("File does not appear to be in QIF format", `qif-file.scm:537-541`), a tokenizer error, required columns missing. `data`: `{ file, line, reason, message }` |

Rows that can't be imported are not errors: they are listed in the proposal, as GnuCash lists them.

## Methods

| Method | Effect | MCP | Does |
|--------|--------|-----|------|
| `import.preview` | read | read only | Read a file and propose what importing it would do |
| `import.apply` | destructive (book) | destructive | Apply a proposal with the caller's changes, as one Mutation |
| `import.discard` | read | read only | Drop a held proposal |
| `importMap.list` | read | read only | List Import map and online entries |
| `importMap.delete` | destructive (book) | destructive | Delete Import map or online entries |

`import.apply` is destructive because *update* overwrites an existing transaction's date, amounts and text, and the
business and Owner importers overwrite existing records.

## CLI

```
red-ale exec --book books/home.gnucash import.preview format=csvTransactions layout=MyBank \
    'files:=[{"name": "statement.csv", "base64": "…"}]'
red-ale call import.apply proposal=imp-3 'changes:=[{"index": 4, "action": "skip"}]'
```

The CLI reads import files on its own side: `--file <path>` on `import.preview` puts the file's bytes in `files` as
`base64`, with its name; repeat it for QIF. `--encoding <label>` adds `encoding` to every file given with `--file`.
Both work on `call` and on `exec` with one method, and are refused with `--script`. With `exec`, a script can preview
and apply in one run.

## Deviations from GnuCash

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
| Unparseable numbers (QIF, business) | 0 | Error row | A wrong amount in a book is worse than a refused row |
| CSV splits failing their essentials | Dropped, logged | Error row | The user sees every row that isn't imported |
| The last CSV transaction's void reason | Ignored | Applied | A GnuCash bug |
| QIF invalid days, unknown actions, ambiguous price dates | Today; imbalance; failure | Error rows; decided by `dateFormat` | As above |
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

## Tests

From [`../milestone3.md`](../milestone3.md), "Testing":

- **GnuCash's own tests, rebuilt**: `5.17:gnucash/import-export/test/gtest-import-backend.cpp` (scores, actions),
  `5.17:libgnucash/engine/test/gtest-import-map.cpp` (both maps, Bayesian lookup and learning, conversion of old data,
  tokens with `/`, listing), `csv-imp/test/` (`sample1.csv`, the tokenizer cases, `parse_monetary`), `qif-imp/test/`
  (parsing, price lines, the duplicate finder). The business importers have none: hand-worked cases citing the lines
  they follow.
- **The matcher**: each row of the score table, each branch of the action choice, conflicts (including the action not
  recomputed), *update* with and without append and with empty imported notes.
- **The pipeline**: a stale proposal is `conflict`; a proposal pushed out by 16 newer ones is `not-found`; a failed
  apply changes nothing; every Imbalance split made is reported; every transaction before the threshold is flagged.
- **Done when** ([`../milestone3.md`](../milestone3.md)): a file applied, then previewed again, proposes nothing to
  add; a CSV export imports back to the same transactions.
- **Oracle**: GnuCash 5.17 opens every book red-ale wrote after an import with no warnings; its own
  `gnc_account_imap_find_account_bayes` finds the account red-ale learned; the book declares only `Account GUID based
  bayesian with flat KVP`, and only after Bayesian learning. After a CSV account or price import, `oracle_check` finds
  no differences, and GnuCash ranks every imported price as `PRICE_SOURCE_USER_PRICE` with the value's stored
  denominator the currency's fraction × 10000 (`crates/red-ale-testkit/tests/oracle_import_records.rs`).
- **Business oracle** (`crates/red-ale-testkit/tests/oracle_import_business.rs`): after importing Owners and
  posted and unposted documents, GnuCash opens the book with no warnings, `oracle_check` finds no differences
  (posted documents' lots and transactions included), its business scrub changes nothing, importing the same files
  again changes nothing, and each document the import posts matches what `gncInvoicePostToAccount` posts from the
  same documents imported unposted.
- **Property and fuzz tests** on every parser: no input panics. The business importers also apply whatever their preview proposes.
