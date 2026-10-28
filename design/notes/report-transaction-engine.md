# Report note: the Transaction Report engine and the four reports on it (GnuCash 5.17)

Scope: milestone 5, "Decide before code", item 4 ([`../milestone5.md`](../milestone5.md)): the Transaction Report
engine (`report/trep-engine.scm`) as the library module `lib/trep-engine.rhai`, and the Transaction Report, General
Ledger, Reconciliation Report and General Journal as thin scripts. This note records what GnuCash does, what red-ale's
report library lacks for it, how GnuCash's own test becomes red-ale fixtures and tests, and a ticket plan. The Scheme
stays the authority; this note says where to look and what is easy to miss.

Citations are `5.17:<path>:<line>`; a bare `:<line>` refers to the file last named. Abbreviations, as in
[`../reports.md`](../reports.md): `report/` = `5.17:gnucash/report/`, `standard/` =
`5.17:gnucash/report/reports/standard/`. Also:

| Short | File |
|-------|------|
| TE | `report/trep-engine.scm` (2,645 lines) |
| TR | `standard/transaction.scm` (36) |
| GL | `standard/general-ledger.scm` (159) |
| RR | `standard/reconcile-report.scm` (115) |
| GJ | `standard/general-journal.scm` (125) |
| REG | `standard/register.scm` (674) |
| TT | `standard/test/test-transaction.scm` (1,123) |
| TEE | `5.17:bindings/guile/test/test-engine-extras.scm` |
| TRE | `report/test/test-report-extras.scm` |
| RU, CU, HU, DU | as in `reports.md` §12 |

Nothing here was run through Guile. Claims come from reading the source; where the test's expected strings confirm
a claim, the test line is cited.

---

## Facts that surprise

1. **The General Journal is not on the engine.** It takes the Register report's options and calls the Register
   report's renderer (GJ:43-45, :111-114), in journal mode with a fixed query (GJ:57-71). `milestone5.md`'s Reports
   table ("the Transaction Report with its own options") is wrong; the Register report it rests on is the one v0.5
   decided not to do (❌, decision 5). See §1.4 and open question 1.
2. **Some thin-report defaults name options that do not exist.** GL sets "Running Balance" #t and "Totals" #f
   (GL:82-83, :102-103) and RR sets and hides "Running Balance" (RR:53-57); the engine's options are "Account
   Balance" and "Grand Total". `GncOption-set-default-value` on a missing option is a no-op (`if (!$self) return;`,
   `5.17:bindings/guile/gnc-optiondb.i:1500-1503`), as is `make-internal` (`5.17:libgnucash/engine/gnc-optiondb.cpp:266-272`).
   So the General Ledger shows **no** balance column and **does** show a Grand Total.
3. **Primary Key "None" turns the query's sort off.** Its sort key is `'()` (TE:276-280); an empty first sort
   parameter compiles to "no sort" (`5.17:libgnucash/engine/qofquery.cpp:492-497`), and `qof_query_run` sorts only when
   the primary sort has a compare function (`:794-797`). Unless a custom key forces the Scheme sort, splits come in
   the order the split collection was walked: hash order.
4. **The "sort by date first" comparator does nothing.** `date-comparator?` (TE:2483-2484) asks `generic-less?` for
   key `'date` with date subtotal `'none`, whose `date-sortvalue` is `#f` (TE:307-311), so it is never true. The
   custom sort's starting order is the query's default sort, `xaccSplitOrder`.
5. **Two "account name" sorts.** The query sorts by `SPLIT-ACCT-FULLNAME`, which compares the account paths level by
   level with `g_utf8_collate` and puts an ancestor first (`5.17:libgnucash/engine/Split.cpp:1687-1709`). The custom
   sort (taken whenever a date key has a date subtotal, TE:2431-2437) compares whole full-name strings with
   `gnc:string-locale<?` (TE:179-184, :2467-2468). They differ when a name holds a character that sorts below the
   separator (`"Bank 2.X"` against `"Bank.Y"`).
6. **Transaction number sorts by its leading number; split action sorts as text.** `TRANS-NUM` is a
   `QOF_TYPE_STRING` parameter with its own compare function, `qof_string_number_compare_func`
   (`5.17:libgnucash/engine/Transaction.cpp:2879-2883`, `5.17:libgnucash/engine/qofquerycore.cpp:234-262`): the
   numbers `strtol` reads at the start (0 when there is none, negatives too), then the rest with `g_strcmp0`, so
   `"9"` sorts before `"10"` and `"-3"` before `""`. `SPLIT-ACTION`, the `number` key's parameter under the num field
   source, has none and compares with `g_strcmp0` (`qofquerycore.cpp:194-207`): `"10"` before `"9"`. Neither is
   `xaccTransOrder`'s `strtoull` rule. `crates/red-ale-testkit/tests/oracle_split_sort.rs` confirms both.
7. **The Transaction Report has no CSV export in 5.17.** TR registers no `export-types` or `export-thunk` (TR:31-36),
   so neither the GUI (`5.17:gnucash/gnome/gnc-plugin-page-report.cpp:1762-1777`) nor `gnucash-cli`
   (`report/report-core.scm:848-871`) reaches the engine's CSV path. Only the Income and GST Statement (v0.6, a tax
   report, `standard/income-gst-statement.scm:84-90`, `:415`) and the test (TT:1041-1050) call it.
8. **Weekly subtotals group and label differently at UTC.** A split's week is
   `floor((s/86400 - 1 - weekstart)/7)`, `s` the local start of its day (`5.17:bindings/guile/date-utilities.scm:123-125`,
   `:250-254`); the label is the interval from `7w+3` to `7w+9` days after the epoch, printed locally (`:169-173`).
   With Sunday weeks (`weekstart` 1) at offset 0, a group runs Saturday to Friday while its label reads Sunday to
   Saturday; east of UTC both run Sunday to Saturday, west both Saturday to Friday. All exact arithmetic.
9. **Voided splits show their former amounts.** When void transactions are included, the amount cells use
   `xaccSplitVoidFormerAmount` (TE:1262-1264; RR:71-74), but the running-balance cell and "Balance b/f" use the real
   balance, in which a voided split counts zero (TE:1290-1297, :1636-1640).
10. **"Transfer from/to" says "Split Transaction"**, not "-- Split Transaction --". The cell asks for the account of
    `xaccSplitGetOtherSplit`, which skips trading splits and `lot-split` splits and returns null for more than one
    other split (`Split.cpp:2161-2185`); `account-namestring` prints `"Split Transaction"` for a null account
    (TE:1798-1802). The *sort* value of "Other Account Name" is `xaccSplitGetCorrAccountFullName`, which counts every
    split and gives `"-- Split Transaction --"` (and `"Split"` for the code) past two (`Split.cpp:1620-1685`). A
    one-split transaction gets an empty cell (TE:1203-1204).
11. **"Filter By…" looks only at the other splits** of the transaction, never the split itself (TE:2491-2499).
12. **The reconciliation date filter keeps every split not reconciled.** Under Date Filter "Reconciled Date" only a
    split in state `y` is checked against the dates; `n`, `c` and `f` splits pass whatever their dates (TE:2568-2571).
    The posted-date term is then left out of the query (TE:2537-2538).
13. **Reconciled Status sorts voided first.** Its sort value is the length of the tail of `(n c y f v)` from the
    split's state (TE:204-212): ascending gives `v`, `f`, `y`, `c`, `n` (TT:734-738 confirms).
14. **The Subtotal Table's average divides by the number of columns**, not by the cells holding data (TE:2120,
    :2144-2146; TT:974-976, `$1.00 / 3 = $0.33`).
15. **The Shares column goes through a double**: an amount in a number cell, printed by the default number renderer
    with `double-to-gnc-numeric … 100` (TE:1212-1218; `report/html-style-info.scm:233-236`), so two decimals.
16. **Under the num field source, split order is not a total order.** `xaccSplitOrder` compares transactions by the
    two splits' actions, then a transaction's own splits by memo and then action (`Split.cpp:1548-1573`). A
    transaction whose splits' memos and actions sort opposite ways forms a cycle with another transaction on the same
    day entered between them, so the query's default sort, and every sort that falls back to it, follows hash order
    there. red-ale compares two splits of one transaction by their actions first, which makes the order total
    (`reports.md` §14).

---

## 1. Inventory

### 1.1 Files and templates

| File | Lines | Template | GUID | Menu | In menu |
|------|------:|----------|------|------|---------|
| TE | 2,645 | none: exports `gnc:trep-options-generator`, `gnc:trep-renderer`, `gnc:lists->csv` (TE:64-66) | | | |
| TR | 36 | Transaction Report (TR:33) | `2fe3b9833af044abb929a88d5a59620f` (TR:34) | none (top level) | yes |
| GL | 159 | General Ledger (GL:39, :151) | `2e22929e5c5b4b769f615a815ef0c20f` (GL:152) | `gnc:menuname-asset-liability` (GL:153) | yes |
| RR | 115 | Reconciliation Report (RR:108) | `e45218c6d76f11e7b5ef0800277ef320` (RR:109) | none (top level) | yes |
| GJ | 125 | General Journal (GJ:36, :118) | `25455562bd234dd0b048ecc5a8af9e43` (GJ:119) | `gnc:menuname-asset-liability` (GJ:120) | yes |
| REG | 674 | Register (REG:64) | `22104e02654c4adba844ee75a3f8d173` (REG:636) | none | **no** (`'in-menu? #f`, REG:651) |

All are version 1 and GPL-2.0-or-later (headers checked: TE:23-38, TR:6-21, GL:13-28, RR:6-21, GJ:9-24, REG:3-19).
RR's name uses `G_`, not `N_` (RR:108): GnuCash registers it translated; red-ale uses the English name.

The engine has one more user, outside v0.5: the Income and GST Statement (`standard/income-gst-statement.scm`, 416
lines, GUID `5bf27f249a0d11e7abc4cec278b6b50a`, `:412`). It uses `#:custom-calculated-cells` in the legacy vector
form (TE:1014-1026), `#:custom-source-accounts`, `#:custom-split-filter`, `#:export-type`, the hidden
`__trep/unique-transactions` option and `make-internal` (`:84-90`, `:180-195`). The library module should keep those
hooks so v0.6 needs no engine change.

### 1.2 Transaction Report

TR is the engine as is: `'options-generator gnc:trep-options-generator`, `'renderer gnc:trep-renderer` (TR:35-36).

### 1.3 General Ledger

Options: the engine's, with these defaults changed (`GncOption-set-default-value`, GL:51-53):

| Section / option | Default | Lines | Note |
|------------------|---------|-------|------|
| Accounts / Filter Type | `none` | GL:56 | already the default |
| Filter / Void Transactions | `non-void-only` | GL:57 | already the default |
| Display / Date | #t | GL:66, :87 | already |
| Display / Reconciled Date | #f | GL:67, :88 | already |
| Display / Num/Action (split-action books) or Num | #f | GL:68, :89 | |
| Display / Trans Number (split-action books only) | #f | GL:69 | already |
| Display / Description | #t | GL:70, :90 | already |
| Display / Memo | #f | GL:71, :91 | |
| Display / Account Name | #f | GL:72, :92 | already |
| Display / Use Full Account Name | #f | GL:73, :93 | |
| Display / Account Code | #f | GL:74, :94 | already |
| Display / Other Account Name | #f | GL:75, :95 | |
| Display / Use Full Other Account Name, Other Account Code, Shares, Price | #f | GL:76-79, :96-99 | already |
| Display / Amount | `double` | GL:81, :101 | already |
| Display / Running Balance | #t | GL:82, :102 | **no such option: no-op** |
| Display / Totals | #f | GL:83, :103 | **no such option: no-op** |
| Display / Sign Reverses | `credit-accounts` | GL:84, :104 | |
| Display / Detail Level | `single` | GL:110-111 | already |
| Sorting / Primary Key | `account-code` | GL:124 | |
| Sorting / Show Full Account Name | #f | GL:125 | already |
| Sorting / Show Account Code | #t | GL:126 | |
| Sorting / Primary Subtotal | #t | GL:127 | already |
| Sorting / Primary Subtotal for Date Key | `none` | GL:128 | |
| Sorting / Primary Sort Order | `ascend` | GL:129 | already |
| Sorting / Secondary Key | `register-order` | GL:130 | already |
| Sorting / Secondary Subtotal | #t | GL:131 | (register-order cannot be subtotalled, TE:214-218) |
| Sorting / Secondary Subtotal for Date Key | `none` | GL:132 | |
| Sorting / Secondary Sort Order | `ascend` | GL:133 | already |

Renderer: the Transaction Report's renderer (looked up by GUID, GL:142-145), then the document title is set to
"General Ledger" (GL:146), also on warning documents, which the engine leaves untitled (§2.10). The comment at
GL:114-116 notes the Report name cannot be set here; the engine's "Report name" defaults to the template name
anyway (`report/report-core.scm:298-335`).

### 1.4 General Journal: on the Register report

Options (GJ:43-107): the Register template's options (`gnc:report-template-new-options/report-guid`,
`report/report-core.scm:282-296`): REG's own options (REG:324-402) and the three General options. Defaults changed:

- `__reg/query`: a query for splits of the current book whose reconcile state is not `v` (GJ:57-59), sorted by
  `TRANS-DATE-POSTED` then the default sort, all ascending (GJ:60-64), whose account is **none** of the template
  root's descendants (GJ:66-71), i.e. not scheduled-transaction template splits. red-ale's Book model holds no
  template splits ([`../api/transactions.md`](../api/transactions.md), `transaction.find` deviations), so the last
  term matches everything.
- `__reg/journal` #t, `double` #t, `debit-string` "Debit", `credit-string` "Credit" (GJ:74-83).
- Display: Date #t, Num/Action or Num #f, Description #t, Account #t, Shares #f, Price #f, Amount `double`, Running
  Balance #f, Totals #f (GJ:87-103). Memo stays #t, Lot #f, Value #f (REG's defaults, REG:361-391).
- General: Report name and Title "General Journal" (GJ:105-106).

Renderer: REG's `reg-renderer` (GJ:111-114; REG:605-634): title from General/Title; the query's splits, **one per
transaction** (`xaccQueryGetSplitsUniqueTrans`, REG:621-622, `5.17:libgnucash/engine/Query.cpp:67-90`: the first
split of each transaction in query order); one table:

- Columns (`build-column-used`, REG:73-121; headings REG:123-164): Date; Num (heading "Num/Action" in a split-action
  book); in journal mode the Description column is used when Memo, Description or `double` is on, and holds the
  description on transaction rows and the memo on split rows; no Memo column in journal mode; Account (heading
  "Account" in journal mode); Shares; Lot; Price; Amount or Debit/Credit (the `debit-string`/`credit-string`);
  Value or Debit Value/Credit Value; Balance.
- Rows, per transaction (REG:528-603, `add-split-row` REG:166-322): a transaction row (`normal-row`) with the date,
  num (`gnc-get-num-action trans split`) and description, nothing in the split columns; with `double`, a notes row:
  `" "` under the date, the transaction num under Num when Num and Description are both shown in a split-action
  book (else `" "`), then the transaction notes spanning the rest (REG:288-322; the `else` branch at :316-320 builds
  a cell it never adds); then one `alternate-row` per split of the transaction, **all** splits including the one the
  query found: split action under Num (`gnc-get-action-num #f split`), memo under Description, the account's full
  name, the absolute amount under Debit (positive) or Credit (zero or negative) with a split link, and `" "` under
  Balance.
- A split with no account is skipped (REG:553-557). Totals accumulate every split of each transaction
  (REG:474-490, :570-578). With Totals off, the only possible total row is "Value Change", which appears only when a
  Value column is shown (REG:426-472, :549-550); with GJ's defaults there is none.

Template note: the template splits' exclusion is the only query difference between GJ and "all non-void splits by
date"; the Register's other uses (account registers, invoices) are GUI-only.

### 1.5 Reconciliation Report

Options: the engine's, with defaults changed (RR:34-58): Sorting/Primary Key `reconciled-status` (:36-37);
Secondary Key `date` (:38-39); Secondary Subtotal for Date Key `none` (:40-41); General/Start Date relative
`start-prev-quarter` (:42-44); End Date relative `today` (:45-47); Date Filter `reconciled` (:48-50); Display/Reconciled
Date #t (:51-52); Display/Memo #f (:55-56). "Running Balance" (:53-54, :57) is a no-op (fact 2).

Renderer (RR:100-104): the engine with

- `#:custom-calculated-cells` (RR:69-98): "Funds In" (the split's amount in its account's commodity when positive)
  and "Funds Out" (the negated amount when zero or negative), both subtotalled, never reversed, `start-dual-column?`
  #f, Funds In `merge-dual-column?` #t. Amounts use the void former amount for void transactions (RR:71-74) and are
  **never converted**: Common Currency has no effect on them and adds no `(USD)` to their headings. With
  `start-dual-column?` #f the subtotal rows do not merge the two columns (§2.8), but the Subtotal Table nets them
  (TE:1667-1670, :1767-1773), and CSV stays possible (TE:2052-2055).
- `#:empty-report-message` (RR:60-67): "The reconcile report is designed to be similar to the formal reconciliation
  tool. Please select the account from Report Options. Please note the dates specified in the options will apply to
  the Reconciliation Date." followed by two line breaks, shown after the no-account (or regex) warning only (TE:2526-2531).

The comment at RR:110-113 speaks of "a different split-date strategy", but the renderer passes no `#:split->date`;
the strategy is the Date Filter default.

---

## 2. The engine

### 2.1 Options (`gnc:trep-options-generator`, TE:521-1012)

The generator reads one Book value: whether the book uses split action for the num field
(`qof-book-use-split-action-for-num-field`, TE:522-524). It changes three option names and choices (marked *SA*).
Keys are GnuCash's sort tags; sections sort by name and options by key (`reports.md` §6). Help texts are at the cited
lines and are copied verbatim.

**General** (default section, TE:1011)

| Name | Key | Type | Default | Choices | Lines |
|------|-----|------|---------|---------|-------|
| Start Date | `aa` | start date, UI both | start of accounting period | the eight start dates | TE:539-540; `5.17:bindings/guile/options.scm:330-334`; `report/options-utilities.scm:54-59` |
| End Date | `ab` | end date, UI both | end of accounting period | the eight end dates | same |
| Date Filter | `a5` | multichoice | `posted` | `posted` "Date Posted", `reconciled` "Reconciled Date", `entered` "Date Entered" | TE:542-548 |
| Table for Exporting | `g` | boolean | #f | | TE:571-574 |

**Currency**

| Name | Key | Type | Default | Lines |
|------|-----|------|---------|-------|
| Common Currency | `a` | complex boolean (callback toggles the other three) | #f | TE:550-559 |
| Show original currency amount | `b` | boolean | #f | TE:561-563 |
| Report's currency | `c` | currency | the report currency | TE:565-566; OU:124-130 |
| Price Source | `d` | multichoice | `pricedb-nearest` | TE:568-569; OU:133-144 (the five sources) |

**Filter**

| Name | Key | Type | Default | Lines |
|------|-----|------|---------|-------|
| Account Name Filter | `a5` | string | `""` | TE:578-583 |
| Use regular expressions for account name filter | `a6` | boolean | #f | TE:585-592 |
| Account Name Filter excludes matched strings | `a7` | boolean | #f | TE:594-597 |
| Transaction Filter | `i1` | string | `""` | TE:599-604 |
| Use regular expressions for transaction filter | `i2` | boolean | #f | TE:606-612 |
| Transaction Filter excludes matched strings | `i3` | boolean | #f | TE:614-618 |
| Transaction Filter is case insensitive | `i4` | boolean | #f | TE:620-624 |
| Reconciled Status | `j1` | multichoice | `all` | `all` "Show All Transactions", `unreconciled` "Unreconciled only", `cleared` "Cleared only", `reconciled` "Reconciled only" (TE:390-410, :626-630) |
| Void Transactions | `k` | multichoice | `non-void-only` | `non-void-only` "Non-void only", `void-only` "Void only", `both` "Both (and include void transactions in totals)" (TE:362-374, :632-636) |
| Closing transactions | `l` | multichoice | `exclude-closing` | `exclude-closing` "Exclude closing transactions", `include-both` "Show both closing and regular transactions", `closing-only` "Show closing transactions only" (TE:376-388, :638-645) |

The Void Transactions help text is `N_`, not `G_` (TE:634): untranslated in GnuCash; no matter in English.

**Accounts**

| Name | Key | Type | Default | Lines |
|------|-----|------|---------|-------|
| Accounts | `a` | account list, any type | none (no allowed types, so no first-account default, `5.17:libgnucash/engine/gnc-option-impl.cpp:415-427`) | TE:650-658 |
| Filter Type | `c` | multichoice with callback | `none` | `none` "Do not do any filtering", `include` "Include Transactions to/from Filter Accounts", `exclude` "Exclude Transactions to/from Filter Accounts" (TE:351-360, :665-673) |
| Filter By… | `c1` | account list | none | TE:660-663. The name ends in U+2026 (TE:72) |

**Sorting** (TE:675-856)

| Name | Key | Type | Default | Lines |
|------|-----|------|---------|-------|
| Primary Key | `a` | multichoice with callback | `account-name` | TE:756-763 |
| Primary Sort Order | `e` | multichoice | `ascend` | `ascend` "Ascending", `descend` "Descending" (TE:413-418, :818-822) |
| Primary Subtotal for Date Key | `e2` | multichoice with callback | `monthly` | TE:809-816 |
| Primary Subtotal | `e5` | complex boolean | #t | TE:801-807 |
| Secondary Key | `f` | multichoice with callback | `register-order` | TE:825-833 |
| Secondary Sort Order | `i` | multichoice | `ascend` | TE:852-856 |
| Secondary Subtotal for Date Key | `i2` | multichoice with callback | `monthly` | TE:843-850 |
| Secondary Subtotal | `i5` | complex boolean | #f | TE:835-841 |
| Show Full Account Name | `j1` | boolean | #f | TE:765-769 |
| Show Account Code | `j2` | boolean | #f | TE:771-775 |
| Show Account Description | `j3` | boolean | #f | TE:777-781 |
| Show Informal Debit/Credit Headers | `j4` | boolean | #f | TE:783-787 |
| Add indenting columns | `j5` | boolean | #t | TE:789-793 |
| Show subtotals only (hide transactional data) | `j6` | boolean | #f | TE:795-799 |

Sort key choices, in order (TE:167-280): `account-name` "Account Name", `account-code` "Account Code", `date`
"Date", `reconciled-date` "Reconciled Date", `reconciled-status` "Reconciled Status", `register-order` "Register
Order", `corresponding-acc-name` "Other Account Name", `corresponding-acc-code` "Other Account Code", `amount`
"Amount", `description` "Description", `number` "Number" (*SA*: "Number/Action"), `t-number` "Transaction Number",
`memo` "Memo", `notes` "Notes", `none` "None". Date subtotal choices (TE:298-349): `none` "None", `daily`, `weekly`,
`monthly`, `quarterly`, `yearly` ("Daily" … "Yearly"). The callbacks (TE:687-753) only set selectability; red-ale
never calls them (`reports.md` §6).

**Display** (TE:858-1003)

| Name | Key | Type | Default | Lines |
|------|-----|------|---------|-------|
| Date | `a` | boolean | #t | TE:907 |
| Reconciled Date | `a2` | boolean | #f | TE:908 |
| Date Entered | `a3` | boolean | #f | TE:909 |
| Num (*SA*: Num/Action) | `b` | boolean | #t | TE:910-912 |
| Trans Number (*SA* only) | `b2` | boolean | #f | TE:930-933 |
| Description | `c` | boolean | #t | TE:913 |
| Memo | `d` | complex boolean | #t | TE:937-942 |
| Notes | `d2` | boolean | #t | TE:914 |
| Invoice | `d5` | boolean | #f | TE:944-946 |
| Account Name | `e` | complex boolean | #f | TE:949-954 |
| Use Full Account Name | `f` | boolean | #t | TE:916 |
| Account Code | `g` | boolean | #f | TE:917 |
| Detail Level | `h` | multichoice with callback | `single` | `multi-line` "One split per line", `single` "One transaction per line" (TE:964-972) |
| Other Account Name | `h5` | complex boolean | #t | TE:957-962 |
| Use Full Other Account Name | `i` | boolean | #f | TE:919 |
| Other Account Code | `j` | boolean | #f | TE:920 |
| Shares | `k` | boolean | #f | TE:922 |
| Price | `l` | boolean | #f | TE:924 |
| Link | `l5` | boolean | #f | TE:923 |
| Amount | `m` | multichoice with callback | `double` | `none` "Hide", `single` "Single Column", `double` "Two Columns" (TE:974-984) |
| Sign Reverses | `m1` | multichoice | `global` | `global` "Use Global Preference", `none` "Don't change any displayed amounts", `income-expense` "Income and Expense", `credit-accounts` "Credit Accounts" (TE:420-435, :990-994) |
| Enable Links | `m2` | boolean | #t | TE:986-988 |
| Subtotal Table | `m5` | boolean | #f | TE:926 |
| Account Balance | `n` | boolean | #f | TE:927 |
| Grand Total | `o` | boolean | #t | TE:928 |
| Running Totals | `o2` | multichoice | `none` | `none` "None", `all` "Grand Total and Subtotals", `grand` "Grand Total Only", `sub` "Subtotals Only" (TE:996-1003) |

**Internal**: `__trep` / `unique-transactions`, #f (TE:1005-1009).

There is no debug option. The "subtotal summary grid" is Display / Subtotal Table (§2.9).

### 2.2 Parameters (TE:2203-2429)

The renderer reads the options once into an alist of *parameters*; everything below uses them. The ones whose
derivation is not a plain read:

- `begindate` = local start of the Start Date's day; `enddate` = local end (23:59:59) of the End Date's day
  (TE:2233-2238).
- Accounts: `c_account_0` = `#:custom-source-accounts` or Accounts (TE:2219-2220). `c_account_1` = those whose full
  name contains (substring, case-sensitive, `string-contains`) or, with the regex option, matches the Account Name
  Filter, or the complement with "excludes"; unchanged when the filter is `""` (TE:2210-2230). Only the selected
  accounts are filtered; descendants are not added.
- `accounts-matched` = `c_account_1`, or `#t` when it is empty (TE:2301). Its commodities feed the exchange function.
- `common-currency` = Report's currency when Common Currency is on, else #f; `common-currency/original` and
  `common-currency/price-source` likewise (TE:2303-2311).
- `other-account-name`, `other-account-code`, `other-account-full-name` only with Detail Level `single`
  (TE:2321-2323, :2336-2341). `num` reads "Num/Action" or "Num" by the book (TE:2316-2318); `trans-number` only in a
  split-action book (TE:2342-2343).
- `reversed-signs` = Sign Reverses (TE:2345). `multiline` = Detail Level `multi-line` (TE:2346-2347).
- `running-grand-total` = Running Totals `grand` or `all` (TE:2349-2351); `running-prime` = a primary subtotal
  renderer exists and Running Totals is `sub` or `all`; `running-sec` likewise for the secondary (TE:2412-2419).
- `primary-date-subtotal` is set only when the primary key is a date key (`date`, `reconciled-date`, TE:150-151,
  :2358-2359); same for the secondary (TE:2361-2362).
- `primary-key/renderer-fn` and `primary-key/split-sortvalue` (TE:2393-2399, :2421-2422): for a date key, the date
  subtotal's `renderer-fn` and `split-sortvalue` (both #f for `none`); otherwise the key's, but only when the key
  can be subtotalled (it has a `renderer-fn`, TE:448-451) **and** Primary Subtotal is on. Secondary likewise
  (TE:2400-2406, :2423-2424). These decide whether subheadings and subtotal rows exist.
- `subtotals-only` = Show subtotals only and either renderer exists (TE:2425-2427).
- `bal-bf` (TE:2380-2389): #t when Detail Level is `single`, Date Filter is `posted`, the Transaction Filter is
  `""`, Reconciled Status is `all`, Void is `non-void-only`, the primary key is `account-name` or `account-code`, the
  secondary key is `register-order` or `date`, Primary Subtotal is on and the secondary order is ascending. It turns
  "Account Balance" into "Running Balance" with a "Balance b/f" on each account subheading.
- `cleared-filter` = Reconciled Status's flags AND the Void choice's flags (TE:2259-2262), with GnuCash's
  `CLEARED-NO` 1, `-CLEARED` 2, `-RECONCILED` 4, `-FROZEN` 8, `-VOIDED` 16, `-ALL` 31. As states: `all` = n c y f v,
  `unreconciled` = n, `cleared` = c, `reconciled` = y; `non-void-only` removes v, `void-only` keeps only v, `both`
  keeps all. `unreconciled` with `void-only` is the empty set: nothing matches (`Query.cpp:474-503`; `strchr` on an
  empty list).
- `closing-match` = #f, `both` or #t (TE:376-388, :2276-2279).
- `subtotal-table?` (TE:2281-2287): Subtotal Table on, **and** (primary key is a date key ? its date subtotal is
  not `none` : Primary Subtotal option on), **and** Amount is `single` or `double`. Note it reads the Primary
  Subtotal *option*, not whether the key can be subtotalled.
- `custom-sort?` (TE:2431-2437): a date key with a date subtotal other than `none`, primary or secondary; or a key
  with a `split-sortvalue` and no query sort key: `reconciled-status` and `notes` (TE:204-212, :270-274; `CUSTOM-SORTING?`
  TE:453-461).

### 2.3 Warnings before the query (TE:2501-2531)

In this order, the first that applies; the document gets no title and no table:

1. `c_account_1` empty: `gnc:html-make-no-account-warning` (HU:367-373): "No accounts selected" / "This report
   requires accounts to be selected in the report options." (TE:2509-2510).
2. The account regex did not compile (`make-regexp` raised `regular-expression-syntax`): a generic warning titled
   `"Error invalid-account-regex"` with empty text (TE:2212-2218, :2512-2516). (`no-guile-regex-support` cannot
   happen in red-ale.)
3. The transaction regex did not compile: `"Error invalid-transaction-regex"` (TE:2245-2254, :2518-2522).

Then the export error "No accounts, or regexp error" (TE:2524) and, when given, the empty-report message
(TE:2528-2531).

The account check uses `c_account_1`, so an account filter that removes every account gives the no-account warning,
not "no matching transactions" (TT:305-309 expects no table).

### 2.4 The query (TE:2533-2555)

On the current book, AND'ed:

1. the split's account is one of `c_account_1` (`QOF-GUID-MATCH-ANY`);
2. the split's reconcile state is in `cleared-filter` (§2.2);
3. only with Date Filter `posted`: date posted between `begindate` and `enddate`, both inclusive
   (`xaccQueryAddDateMatchTT … #t begindate #t enddate`, `Query.cpp:383-430`);
4. only when `closing-match` is a boolean: the transaction is (#t) or is not (#f) a closing transaction.

Sort, unless `custom-sort?`: primary, secondary and tertiary sort keys = the primary key's `sortkey`, the
secondary's, and `QUERY-DEFAULT-SORT`; increasing = primary ascending, secondary ascending, #t (TE:2541-2549). QOF
compares by the primary key; on a tie by the secondary; on a tie by the tertiary; only the deciding level's result is
negated for "descending" (`qofquery.cpp:345-374`). An empty key list compares equal (`:310-311`); an empty
*primary* list disables the whole sort (fact 3). The query's own default sort, kept when `custom-sort?`, is
`QUERY-DEFAULT-SORT` (`qofquery.cpp:169-171`).

| Key | QOF sort parameter | Compares |
|-----|--------------------|----------|
| `account-name` | `SPLIT-ACCT-FULLNAME` | `xaccSplitCompareAccountFullNames`: the same account is equal; else walk both paths from the top, the first differing account's names with `g_utf8_collate`; an account whose path ends first (an ancestor) is less (`Split.cpp:1687-1709`) |
| `account-code` | `SPLIT-ACCOUNT` → `ACCOUNT-CODE-` | the account codes, `g_strcmp0` (`qofquerycore.cpp:194-207`) |
| `date` | `SPLIT-TRANS` → `TRANS-DATE-POSTED` | posted times (`:403-413`, `QOF_DATE_MATCH_NORMAL`: to the second) |
| `reconciled-date` | `SPLIT-DATE-RECONCILED` | reconcile times; 0 when never reconciled |
| `reconciled-status` | none (custom) | |
| `register-order` | `QUERY-DEFAULT-SORT` | `xaccSplitOrder` (`Split.cpp:1536-1597`) |
| `corresponding-acc-name` | `SPLIT-CORR-ACCT-NAME` | `xaccSplitGetCorrAccountFullName` strings, `g_strcmp0` (`Split.cpp:1726-1745`) |
| `corresponding-acc-code` | `SPLIT-CORR-ACCT-CODE` | `xaccSplitGetCorrAccountCode` strings (`:1747-1760`) |
| `amount` | `SPLIT-VALUE` | split **value**, `gnc_numeric_compare` (`qofquerycore.cpp:543-553`) |
| `description` | `SPLIT-TRANS` → `TRANS-DESCRIPTION` | `g_strcmp0` |
| `number` | `SPLIT-ACTION` (*SA*) or `SPLIT-TRANS` → `TRANS-NUM` | `g_strcmp0` for the action; `qof_string_number_compare_func` for the num (fact 6) |
| `t-number` | `SPLIT-TRANS` → `TRANS-NUM` | `qof_string_number_compare_func` (fact 6) |
| `memo` | `SPLIT-MEMO` | `g_strcmp0` |
| `notes` | none (custom) | |
| `none` | `'()` | nothing |

Then `qof-query-run`, or with `__trep/unique-transactions` the first split of each transaction in query order
(TE:2551-2553; `Query.cpp:67-90`).

### 2.5 The split filter (TE:2557-2589)

Every split is kept when all four hold:

1. **Date source** (TE:2566-2578): `posted` always; `reconciled`: a split in state `y` must have
   `begindate <= reconcile date <= enddate`, others pass; `entered`: `begindate <= date entered <= enddate`;
   `custom` (`#:split->date` given): that date between the bounds, or `#:split->date-include-false?` when it returns
   #f.
2. **Filter Type** (TE:2579-2582, `is-filter-member` TE:2491-2499): `include` keeps the split when another split of
   its transaction is in "Filter By…"; `exclude` keeps it when none is. The split itself is skipped by identity.
3. **Transaction Filter** (TE:2583-2586): when not `""`, the split matches when the description, the notes or the
   split's memo matches (TE:2486-2489), in that order; "excludes" negates the whole. A match (TE:2439-2446) is the
   compiled regex (with `regexp/icase` when case-insensitive, TE:2250-2252) found anywhere, else
   `string-contains-ci` when case-insensitive, else `string-contains`.
4. **`#:custom-split-filter`** (TE:2587-2588).

### 2.6 The custom sort (TE:2448-2484, :2591-2594)

When `custom-sort?`, three `stable-sort!`s of the filtered list, in this order: `date-comparator?` (a no-op, fact 4),
the secondary comparator, the primary comparator. So the result is ordered by primary, ties by secondary, ties in
`xaccSplitOrder`.

`generic-less? X Y key date-subtotal ascend?`: the comparison value of a split is, for a date key, the date
subtotal's `date-sortvalue` applied to the key's date (`#f` when the subtotal is `none`); otherwise the key's
`split-sortvalue` (TE:179-280), or `#f` when it has none. X is less than Y when X's value is not #f and
`op(X, Y)` holds, `op` being `gnc:string-locale<?`/`>?` for strings and `<`/`>` for numbers by `ascend?`. A key whose
value is #f never orders anything.

`split-sortvalue`s: account-name: the account's full name; account-code: its code; date: posted time;
reconciled-date: `xaccSplitGetDateReconciled`; reconciled-status: §fact 13; register-order: none;
corresponding-acc-name/-code: the strings of fact 10; amount: `xaccSplitGetValue` (exact); description,
number (action or num), t-number, memo, notes: the strings; none: none.

Date subtotal `date-sortvalue`s (TE:282-296, :313-349) of a time `t`, all from `gnc-localtime t`: daily
`500 × year + yday` (yday from 1); weekly `gnc:date-get-week` (fact 8); monthly `100 × year + month`; quarterly
`10 × year + quarter`; yearly `year`. Each date subtotal's `split-sortvalue` is the same of the split's posted date
(`split->time64`, TE:295-296): **the date subtotals always group by date posted, even for the `reconciled-date`
key.** Their `renderer-fn`s give the group labels: daily `qof-print-date`; weekly
`gnc:date-get-week-year-string` (`gnc-date-interval-format` of fact 8's interval, DU:169-173); monthly `"%B %Y"`
(DU:166-167); quarterly `"Q<n> %Y"` (DU:154-161); yearly `"%Y"` (DU:151-152).

### 2.7 Columns

**Left columns** (TE:1103-1252), each present when its parameter is on, in this order:

| Heading | When | Cell | Only on transaction rows |
|---------|------|------|:---:|
| Date | `date` | `qof-print-date` of date posted, `date-cell` | yes |
| Date Entered | `entered` | `qof-print-date` of date entered | yes |
| Reconciled Date | `reconciled-date` | `qof-print-date` of the reconcile date when the state is `y`, else empty | no |
| Num, or Num/T-Num with Trans Number | `num` | `gnc-get-num-action trans split` (the action in a split-action book, else the num); with Trans Number and a non-empty transaction num, `"<num>/<t-num>"` (TE:1139-1155; `5.17:libgnucash/engine/engine-helpers.c:56-76`) | yes |
| Description | `description` | the description | yes |
| Memo, or Memo/Notes with Notes | `memo` | the memo; with Notes and an empty memo, the transaction notes (TE:1167-1177) | no |
| Invoice | `invoice` | links to the invoices of the transaction's A/P and A/R splits' lots (§2.7.1) | yes |
| Account | `account-name` or `account-code` | `account-namestring` of the split's account | no |
| Transfer from/to | `other-account-name` or `other-account-code` | with more than one split, `account-namestring` of `xaccSplitGetOtherSplit`'s account (fact 10); else empty | no |
| Shares | `shares` | the split's amount in a number cell (fact 15) | no |
| `""` (empty heading) | `doclink` | when the transaction has a document link: `"L"`, a link to it under Enable Links (TE:1220-1234) | no |
| Price | `price` | `xaccSplitGetSharePrice` printed with the transaction currency's price print info (`report/html-style-info.scm:221-222`) | no |

`account-namestring account code? name? full?` (TE:1798-1814): `"Split Transaction"` for no account; else the code
and a space when code? and the code is not empty, then the full name (full?) or the name, when name?.

When `subtotals-only`, or when no left column is on and the grand total or a subtotal renderer is, the left columns
are replaced by one column with heading `""` and empty cells (TE:1246-1252), which holds the subtotal labels.

#### 2.7.1 Invoice cell (TE:1036-1049)

For each split of the transaction in split-list order whose account is A/P or A/R, the invoice of its lot
(`gncInvoiceGetInvoiceFromLot`); each new invoice is put **in front** of the list, with a line break between, so the
cell lists the invoices in reverse order of their splits. Each is a link to the invoice with its ID as text.

**Calculated cells** (TE:1260-1555), each with `heading`, `calc-fn`, `reverse-column?`, `subtotal?`,
`start-dual-column?`, `friendly-heading-fn`, `merge-dual-column?` (TE:1350-1365). The heading gets `" (<mnemonic>)"`
of the report currency under Common Currency (`header-commodity`, TE:1270-1277). In order:

| Heading | When | Value of a split | Reversed | Subtotal | Dual start |
|---------|------|------------------|:---:|:---:|:---:|
| Amount | `amount-single` | converted amount | yes | yes | no |
| Debit, Credit | `amount-double` | converted amount when > 0; negated converted amount when ≤ 0 | no | yes | Debit |
| Running Balance (`bal-bf`) or Account Balance | `running-balance` | converted `xaccSplitGetBalance` | yes | no | no |
| Running Secondary Subtotal | `running-sec` | running total, transaction rows only | no | no | no |
| Running Primary Subtotal (with a secondary renderer) or Running Subtotal | `running-prime` | running total | no | no | no |
| Running Grand Total (with any subtotal renderer) or Running Total | `running-grand-total` | running total | no | no | no |
| then, with Show original currency amount and Common Currency, the same six in the account's commodity, without the mnemonic suffix | | | | | |

- *Amount of a split*: `xaccSplitVoidFormerAmount` when the transaction is void (`gnc:split-voided?` is
  `xaccTransGetVoidStatus`, RU:615-617), else `xaccSplitGetAmount` (TE:1262-1264). Always the **amount** in the
  account's commodity, never the value.
- *Converted*: `exchange-fn (amount in the account's commodity) row-currency (date posted)` (TE:1279-1284), where
  `row-currency` is the report currency under Common Currency, else the account's commodity (TE:1266-1267), and
  `exchange-fn` is `gnc:case-exchange-time-fn price-source report-currency (commodities of accounts-matched) enddate
  #f #f` under Common Currency, else `gnc:exchange-by-pricedb-nearest` (TE:1082-1089). Without Common Currency the
  conversion is to the same commodity: `exchange-if-same` returns it unchanged, but two euro-zone currencies go
  through the euro first (CU:635-644, :708-721), so an amount in a legacy euro currency is rounded by the euro rules.
- *Balance*: converted at `time64CanonicalDayTime` of date posted, local noon (TE:1290-1297;
  `5.17:libgnucash/engine/gnc-date.cpp:404-410`).
- *Running totals*: on a transaction row, add the converted (or original) amount to the column's running collector
  and show that collector's total in the split's commodity (TE:1313-1348).
- Debit/credit friendly headings: `gnc-account-get-debit-string`/`-credit-string` of the account type
  (`5.17:libgnucash/engine/Account.cpp:148-185`, `:4079-4101`): BANK Deposit/Withdrawal, CASH Receive/Spend, CREDIT
  Payment/Charge, ASSET Increase/Decrease, LIABILITY Decrease/Increase, STOCK, MUTUAL, CURRENCY Buy/Sell, INCOME
  Charge/Income, EXPENSE Expense/Rebate, PAYABLE Payment/Bill, RECEIVABLE Invoice/Payment, TRADING and EQUITY
  Decrease/Increase, otherwise Debit/Credit; with the "use formal accounting labels" preference always Debit/Credit.

`#:custom-calculated-cells` replaces the list (TE:1557-1570): a function of the options returning cells as alists,
or as the legacy vectors that `upgrade-vector-to-assoclist` converts; a malformed list logs and falls back to the
defaults. With `subtotals-only`, only cells with `subtotal?` are kept (TE:1571-1573).

**Sign reversal** (TE:1072-1080, :1883-1888): a cell with `reverse-column?` is negated when the split's account
reverses: under Sign Reverses `global`, `gnc-reverse-balance` (red-ale's `a.reverse_balance(ctx)`); under
`income-expense` and `credit-accounts`, the account type is in the choice's list (INCOME, EXPENSE; LIABILITY,
PAYABLE, EQUITY, CREDIT, INCOME; TE:420-435). **`none` reverses as `global` does**: the parameter turns it into #t
(TE:2345), `sign-reverse-list` has no #t entry, and Guile's `assq-ref` is sloppy (`scm_sloppy_assq` in
`libguile/alist.c`), giving #f for a missing key and for the #f alist alike, so `account-types-to-reverse` is #f and
`acc-reverse?` calls `gnc-reverse-balance` (TE:1072-1080). TT:620-634 cannot tell, as the test runs with no
preferences. red-ale follows GnuCash: "none" reads `reports.reverseBalancedAccounts`
(`lib_trep_engine.rs`, `sign_reverses_none_reads_the_global_preference`).

**Links** (TE:1892-1900): with Enable Links, an amount cell links to its split (`gnc:html-split-anchor`, only when the
split has an account, HU:195-200) when the cell has `subtotal?`, or when no cell has.

**Indenting** (TE:1584-1595): `primary-indent` is 1 when Add indenting columns is on and a primary renderer exists;
`secondary-indent` likewise; `indent-level` their sum. Each row starts with empty cells for the indent.

### 2.8 The table and the order of its rows (TE:1054-2063)

Header (TE:1929-1933): `indent-level` empty headings, the left headings, the calculated headings.

Row kinds:

- **Data row** (`add-split-row`, TE:1868-1917): unless `subtotals-only`: indent cells, the left cells, the calculated
  cells (each `number-cell`, reversed and linked as above). Then, on a transaction row only, each `subtotal?` cell's
  value is added to that column's primary, secondary and grand-total collectors (TE:1903-1917). Style: `normal-row`
  for odd splits or in multi-line mode, else `alternate-row` (TE:1967-1971); the parity runs over the whole table and
  is not reset by groups.
- **Multi-line split rows** (TE:1973-1979): after each split's row in multi-line mode, a row for every other split of
  its transaction (split-list order, `delete` by equality), `alternate-row`, not a transaction row: no date, num,
  description or invoice, no subtotals, no running totals.
- **Subheading** (`add-subheading`, TE:1597-1665), unless `subtotals-only`: style `primary-subheading` or
  `secondary-subheading`; left indent 0 (primary) or `primary-indent` (secondary); the label in one
  `total-label-cell` spanning `right-indent + left columns`, or with Table for Exporting the label alone
  (`total-label-cell` when `summary-style?`, else `text-cell`) then that many minus one empty cells; then for each
  calculated cell: under `bal-bf` with an account key, "Balance b/f": `xaccAccountGetBalanceAsOfDate` of the
  split's account at `begindate` (strictly before, `Account.cpp:3505-3524`), converted at local noon of the split's
  posted day, negated when the account reverses (TE:1635-1648); for the original-currency balance the same in the
  account's commodity, unconverted (TE:1649-1657); with Show Informal Debit/Credit Headers, Two Columns and an
  `account-name` or `account-code` key, the friendly heading in bold (TE:1658-1664); else empty. The label is the
  group label (§2.6, `render-summary` TE:1837-1850), with `": Balance b/f"` appended when a cell is `bal-bf` and the
  key is an account key (TE:1601-1607). Account labels: `account-namestring` with Show Account Code, always the
  name, Show Full Account Name; then `": <description>"` with Show Account Description when the description is not
  empty (TE:1821-1831).
- **Subtotal rows** (`add-subtotal-row`, TE:1680-1791): one row per commodity found in the columns' collectors, the
  label on the first only (`""` after); style `primary-subheading`, `secondary-subheading` or `grand-total`; left
  indent 0 (total), `primary-indent` (primary) or `indent-level` (secondary); the label cell as for subheadings
  (TE:1701-1711); then the data cells. Commodity order: `delete-duplicates` of the commodities of every column's
  `'format` list, columns in order, each in its collector's order (newest commodity first, RU:346-354). Data cells
  (TE:1713-1765): a column with `start-dual-column?` is held; the next column then shows the difference
  `held − this` (each `#f` read as 0; no cell when both are absent) as its absolute value in the Debit column when
  positive, else in the Credit column; other columns show the commodity's total, or an empty cell. Class
  `total-number-cell` when `summary-style?`, else `number-cell`. Label: `subtotal-label` (TE:1063-1071) puts
  `"Total For "` in front unless `subtotals-only` and the level is the shown detail level, judged by the Secondary
  Subtotal **option** (secondary when on, primary when off).
- **Grand total**: a `grand-total` row with one cell spanning every column holding a horizontal rule, then the
  subtotal rows labelled "Grand Total" from the grand-total collectors at level `total` (TE:1951-1961).

`summary-style? level` (TE:1672-1678) is true unless `subtotals-only` and `level` is the finest level shown
(secondary if a secondary renderer exists, else primary, else grand).

Order (TE:1935-2050):

1. a primary subheading for the first split, if a primary renderer exists; then a secondary subheading for it, if a
   secondary renderer exists;
2. for each split: its row (and multi-line rows); then, with `next` the following split:
   - if a primary comparator exists and there is no `next` or its primary value differs (`equal?`):
     if a secondary comparator exists, the secondary subtotal rows and a reset of the secondary collectors; the
     primary subtotal rows and a reset of the primary collectors; if there is a `next`, its primary subheading, its
     secondary subheading (if a secondary comparator exists), and a reset of the running primary and secondary
     collectors;
   - otherwise, if a secondary comparator exists and there is no `next` or its secondary value differs: the secondary
     subtotal rows, a reset; if there is a `next`, its secondary subheading and a reset of the running secondary
     collectors;
3. at the end, with Grand Total, the rule row and the grand-total rows.

The running grand-total collectors are never reset.

### 2.9 The Subtotal Table (`make-grid`, TE:2065-2179)

Each subtotal row also adds to a grid (TE:1767-1778): for each commodity of the row, the first calculated column's
total minus the second's when the first has `merge-dual-column?` (TE:1667-1670, :1767-1773), zeros included. The
cell is (row key, column key):

| Subtotal level | Row key | Column key |
|----------------|---------|------------|
| secondary | `(primary sort value . primary label)`, or `(#f . "")` with no primary comparator | `(secondary sort value . secondary label)`; also added to (`row-total`, same column) |
| primary | `(primary sort value . primary label)` | `col-total` |
| total | `row-total` | `col-total` |

When `subtotal-table?`, the grid becomes a table (TE:2112-2169) shown **before** the main table: caption "Subtotal
Table"; headings `""`, the column labels, "Total", and "Average" when there are two or more columns; rows sorted by
sort value (`gnc:string-locale<?` for strings, `<` for numbers, TE:2113-2118), then `row-total` last when present;
columns sorted the same way. Each row key gives one line per commodity it holds, sorted by `gnc-commodity-compare`
(unique name with `g_strcmp0`, then GUID, `5.17:libgnucash/engine/gnc-commodity.cpp:1502-1510`); the first line has
the label (`"Total"` for `row-total`), the others `""`. Cells: the column's amount, or empty; the `col-total`
amount; the average: `col-total / number of columns` rounded to the commodity's SCU with `GNC-HOW-RND-ROUND`
(banker's, TE:2122-2130).

The main table is left out when `subtotal-table?` and the Show subtotals only **option** is on (TE:2623-2625).

### 2.10 The document (TE:2596-2641)

- No split after filtering: a generic warning "No matching transactions found" / "No transactions were found that
  match the time interval and account selection specified in the Options panel." (TE:145-148, :2599-2603) and the
  export error "No splits found" (TE:2605). No title, no table, and no empty-report message.
- Otherwise: title = General / Report name (TE:2612); a text block `<h3>` with `gnc-date-interval-format begindate
  enddate` (TE:2614-2618); the Subtotal Table when `subtotal-table?`; the main table unless §2.9 leaves it out.

### 2.11 CSV (TE:463-512, :2052-2063, :2627-2641)

Only when the renderer is called with `#:export-type 'csv`. The rows are: `("from" <begindate>)`,
`("to" <enddate>)` (ISO `%Y-%m-%d`, local, `qof-date-format-get-string QOF-DATE-FORMAT-ISO`), then for each calculated
cell `(<heading> <monetary>…)` with the grand-total collector's amounts (empty for cells without `subtotal?`). When
any cell has `start-dual-column?`, there is no list: the export error "CSV disabled for double column amounts"
(TE:2053-2055). `lists->csv`: strings quoted with `"` doubled, numbers as `(number->string (exact->inexact n))`, a
monetary as its amount, `#f`/`'()` as nothing, other values with `object->string`; each row padded with commas to the
longest; rows joined by `\n` (TE:463-512). red-ale's `render::csv` and `number/flonum.rs` already do this
(`reports.md` §12).

---

## 3. What red-ale's report library lacks

Compared with `crates/red-ale-report` at `milestone-5` (`src/query/*.rs`, `src/options/*.rs`, `src/engine/*.rs`,
`src/result/*.rs`, `scripts/lib/*.rhai`). Already there and enough: collectors (`lib/collectors`, alist order kept),
`case_exchange_time_fn` and `exchange_by_pricedb_nearest` with the euro first (`lib/exchange`), `time64_start_day_time`
/ `end_day_time` and `date_to_week` (`lib/dates`), the warnings (`lib/html-utilities`), `book.split_query`'s accounts
and posted bounds, the Split's `balance`, `sharePrice`, `reconcile`, `reconcileDate`, `action`, `memo`; the
Transaction's `num`, `notes`, `isVoid`, `isClosing`, `dateEntered`; `a.balance_as_of`, `a.reverse_balance(ctx)`;
`ctx.print_date`, `print_date_interval`, `print_date_iso`, `local`, `make_time`, `export_type`; option types
including complex boolean and multichoice with callback; the result's `export` rows and `render::csv`.

### 3.1 Query API

| Need | Used by | GnuCash |
|------|---------|---------|
| `book.split_query` grows: `reconcile` (the states a split may be in; default `["n","c","y","f"]`, today's fixed behaviour), `isClosing` (`true`/`false`/absent, AND'ed, beside the existing OR'ed `closing`), `sort` (`[{ key, ascending }, …]`, at most two, the default sort third), `unique` (one split per transaction). Names follow `transaction.find`'s `reconcile.states` and `closing` | engine, GJ | TE:2533-2553; GJ:57-64; `Query.cpp:67-90`, `:474-503` |
| The query sort keys of §2.4, in core (`ops::transactions::sort`): `accountFullName` (path-wise), `accountCode`, `datePosted`, `dateEntered`, `dateReconciled`, `corrAccountFullName`, `corrAccountCode`, `value`, `amount`, `description`, `num`, `action`, `memo`, `default`; an empty primary means no sort (§fact 3, see §7 Q3) | engine | `qofquery.cpp:295-374`; `Split.cpp:1687-1760` |
| Split `otherSplit` (or `()`): `xaccSplitGetOtherSplit`, skipping trading and `lot-split` splits | Transfer from/to, corresponding-acc renderer | `Split.cpp:2161-2185`; core has `ops::transactions::other_split` |
| Split `corrAccountFullName`, `corrAccountCode`: `"-- Split Transaction --"` / `"Split"` past two splits | corresponding-acc sorts | `Split.cpp:1620-1685` |
| Split `voidFormerAmount` | amounts of void transactions | `Split.cpp:2190-2196`; `TE:1262-1264`; RR:71-74 |
| Split `lot` (a Lot or `()`), and the Lot's `invoice`: `#{ guid, id }` or `()` | Invoice column | `gncInvoiceGetInvoiceFromLot`; core `ops::business::payments::lot_invoice` |
| Transaction `docLink` (`""` when none) | Link column | `xaccTransGetDocLink`; core `ops::doc_links::doc_link` |
| Account `debitLabel`, `creditLabel` (friendly strings) | informal headers | `Account.cpp:148-185`, `:4079-4101` |
| `book.uses_split_action_for_num()` | the options generator (option names), Num cell, `number` key | `qof-book-use-split-action-for-num-field`; core `book_options::uses_split_action_for_num` |

`gnc-get-num-action` and `gnc-get-action-num` are two lines of Rhai on top of the last read and stay in the module.

### 3.2 Run context

| Need | Used by | GnuCash |
|------|---------|---------|
| `ctx.print_month_year(t)` (`"%B %Y"`, "January 2024" under C) and `ctx.print_year(t)` (`"%Y"`), month names from the Locale (red-ale-locale has them, `crates/red-ale-locale/src/icu.rs:72`) | monthly, quarterly, yearly labels | DU:151-167 |
| Local noon of a day (`time64CanonicalDayTime`): composable from `ctx.local` and `ctx.make_time`, so `lib/dates` gains `time64_canonical_day_time` rather than the host | balance cells, Balance b/f | `gnc-date.cpp:404-410` |
| `gnc:date-get-week` and `gnc:date-get-week-year-string` in `lib/dates` (exact; fact 8) | weekly subtotals | DU:123-125, :169-173 |

### 3.3 Options

| Need | Used by | GnuCash |
|------|---------|---------|
| `o.set_default(section, name, value)`: change the default of a declared option | GL, RR, GJ | `GncOption-set-default-value` (`gnc-optiondb.i:1500-1611`) |
| `o.make_internal(section, name)`: keep an option but leave it out of `report.options` and refuse it in changes | RR (no-op in 5.17), the GST Statement (v0.6) | `GncOptionDB::make_internal` (`gnc-optiondb.cpp:266-272`) |
| Options declared from another template's generator: GJ declares REG's options | GJ | `gnc:report-template-new-options/report-guid` (`report/report-core.scm:282-296`) |

The last is a plain function call in Rhai (`register::options(o, book)` from a library module) and needs no builder
change.

### 3.4 The report result

The result model (`reports.md` §7) carries most of it: one header row of column labels, `data` / `heading` /
`subtotal` / `grandTotal` / `rule` rows, `span`, monetary, number, price, date and text cells, `export` rows. Not
yet expressible:

| Need | Proposal | GnuCash |
|------|----------|---------|
| A link to a split (amount cells, GJ's amounts) | `Link::Split { split, transaction }`: `{ "split": "<guid>" }` | `gnc:html-split-anchor` (HU:195-200) |
| A link to an invoice | `Link::Invoice`: `{ "invoice": "<guid>" }` | `gnc:invoice-anchor-text` (HU:127-128) |
| A link to a transaction's linked document | `Link::TransactionDocLink`: `{ "transactionDocLink": "<txn guid>" }`; never the URI itself, so no export carries a `file:` path as a link | `gnc:html-transaction-doclink-anchor` (HU:207-210) |
| A cell holding several links (Invoice with more than one invoice) | a cell content `anchors: [{ text, link }]`, rendered one per line | TE:1036-1049, :1184-1186 |
| Row striping that is not by position (parity over splits only; multi-line rows) | an optional row flag `alternate: true` | `normal-row` / `alternate-row`, TE:1967-1977 |
| Bold text in an amount column (informal headers) | a text cell in a `heading` row; the renderer already styles heading rows. No change | TE:1662-1664 |
| A caption | the table's existing `title` | TE:2149 |
| Labels spanning indent and left columns | `span`, no change; indent columns are columns with `label: ""` | TE:1628-1630 |
| A table with no columns (Amount `Hide` and every column off) | allowed by the validator (a zero-width row); confirm the renderer writes an empty table rather than failing | TT:495-501 |
| Why there is no export: "No accounts, or regexp error", "No splits found", "CSV disabled for double column amounts" | an optional draft field `exportError` (string), set by the script when `ctx.export_type()` is `"csv"`; `report.exportCsv` answers `invalid` with `data.reason: "export-error"` and `data.message` | `gnc:html-document-set-export-error` (TE:2524, :2605, :2641) |
| A title for warning documents, which the engine leaves untitled | the Report name, as for the other untitled reports (`reports.md` §14) | TE:2501-2531, :2596-2605 |

All are additive to `ReportResult` and make `redAleApi` 0.5.0 (`milestone5.md`, "Interfaces").

### 3.5 Library modules

| Module | From | Holds |
|--------|------|-------|
| `lib/trep-engine` | TE | `options(o, book)` (the generator), `render(book, opts, ctx, hooks)` with `hooks` as a map: `custom_calculated_cells`, `empty_report_message`, `custom_split_filter`, `split_to_date`, `split_to_date_include_false`, `custom_source_accounts` (`#:export-type` is `ctx.export_type()`), `lists_to_csv` rows. Function values passed in `hooks` are function pointers, not closures, until the Rhai closure bugs are fixed (`milestone5.md`, "Carried forward") |
| `lib/register` | REG:32-634 | REG's options and renderer, journal and non-journal paths (§1.4, §7 Q1). No template |
| `lib/dates` | DU | gains `date_get_week`, `date_get_week_year_string`, `time64_canonical_day_time` |

---

## 4. GnuCash's tests

Only TT tests this group. There is no test of the General Ledger, the General Journal or the Register report's
journal mode (`standard/test/` has `test-register.scm` for the Register, which is ❌). TT has no licence header
(TT:1-12): cite it under GnuCash's licence, as `reports.md` §13 does for four other tests.

### 4.1 How TT reads the report

- `(setlocale LC_ALL "C")` (TT:34) and `qof-date-format-set QOF-DATE-FORMAT-ISO` before each render (TT:70):
  settings `locale: C`, `dateFormat: iso`, and the §13 defaults (`reverseBalancedAccounts: none`, USD).
- `options->sxml` renders through the template's renderer directly (TRE:28-63, :76-93), so the "Add options summary"
  `always` of `default-testing-options` has no effect (the summary is added only by `gnc:report-render-html`,
  `report/report-core.scm:774-780`).
- `get-row-col sxml row col` = `sxml->table-row-col sxml 1 row col` (TT:73-74; TRE:96-109): the texts of
  `// (table 1) // (tr row) // (td col) // *text*`; row 0 reads the `th` cells; `#f` means every row or cell;
  negative counts from the end. Header rows sit in `<thead>` and data rows in `<tbody>` (`report/html-table.scm:450`,
  `:488`), so row *n* is the *n*-th body row. Empty cells give no text and drop out of the lists. "Table 1" is the
  Subtotal Table when shown, else the main table.

red-ale's checks address the result instead: table *k*, body row *n* (all rows of `tbody` in order), cell *m* by
column position counting spans, the `display` strings, skipping cells with no text, as the ports of v0.2 do.

### 4.2 The books

**trep book** (TT:104-226), built in the current book:

- Accounts (`structure`, TT:85-97; `env-create-account-structure-alist`, TEE:320-358): a top-level account
  **"Root"** (ASSET) under the book's real root, with "Asset" (Bank, GBP Bank, USD Bank, Wallet), "Income" (INCOME),
  "Income-GBP" (INCOME), "Expenses" (EXPENSE), "Liabilities" (LIABILITY), "Equity" (EQUITY). Full names are
  `Root.Asset.Bank` and so on: the default separator is `.` (`Account.cpp:58`). Every account in USD
  (`gnc-default-report-currency` under C), set again explicitly (TT:150-153).
- Income's description "Salaries etc" (TT:146); Expenses' code "EXP-001" (TT:147); GBP's user symbol `"#"` (TT:157);
  GBP Bank and Income-GBP code "01-GBP", commodity GBP (TT:159-169); USD Bank code "02-USD", commodity USD — the
  same as before; "foreign1" is USD (TT:118-121, :171-175).
- Transactions (`env-transfer` TEE:199-223, `env-transfer-foreign` :170-197, `env-create-multisplit-transaction`
  :239-292; amounts are value = amount, the first split negated, every split gets the memo and the reconcile pair):

| Line | Date | From → to | Amount | Other |
|------|------|-----------|-------:|-------|
| TT:182 | 1970-01-01 | Bank → Expenses | 5 | desc-1, num trn1, memo memo-3 |
| TT:183 | 1969-12-31 | Income → Bank | 10 | desc-2, trn2, **voided** "void", then notes notes3 (set after `xaccTransVoid`, TEE:289-290) |
| TT:184-185 | 1969-12-31 | Income → Bank | 29 | desc-3, trn3, reconciled `c` at 1970-03-01 |
| TT:186 | 1970-02-01 | Bank → Expenses | 15 | desc-4, trn4, notes notes2, memo memo-1 |
| TT:187 | 1970-01-10 | Liabilities → Expenses | 10 | desc-5, trn5, **voided** "any" |
| TT:188 | 1970-01-10 | Liabilities → Expenses | 11 | desc-6, trn6, notes notes1 |
| TT:189-190 | 1970-02-10 | Bank → Liabilities | 8 | desc-7, trn7, notes notes1, memo memo-2, reconciled `y` at 1970-03-01 |
| TT:196-202 | 1971-02-14 | Bank −100, Expenses +80, Wallet +20 | | "$100bank -> $80expenses + $20wallet", notes multisplit |
| TT:205-206 | 1999-12-31 | Expenses → Equity | 111 | "Closing", marked closing |
| TT:210 | 2000-01-15 | GBP Bank −10 GBP → USD Bank +14 USD | | currency GBP; price USD in GBP 10/14 at that day's midnight |
| TT:211 | 2000-02-15 | USD Bank −9 USD → GBP Bank +6 GBP | | currency USD; price GBP in USD 9/6 |
| TT:221-226 | each month *m* of YEAR: the 8th, 3rd, 15th, 9th | Income-GBP → GBP Bank 51 "#51 income"; Income → Bank 103 "$103 income"; Bank → Expenses 22 "$22 expense"; Income → Bank 109 "$109 income" | | in that order per month |

YEAR is the current year (`gnc:get-today`, TT:117).

**invoice book** (TT:1007; `create-test-invoice-data`, TEE:487-849), built **into the same book** after the tests
above it: a second top-level "Root" (ASSET, USD) with Asset/Bank, VAT (VAT-on-Purchases, VAT-on-Sales LIABILITY),
Income, Expense, A/Receivable, A/Payable; a customer, a job, a vendor, an employee; invoices 0001-0008 (0005-0007
credit notes copied from 0001, 0003, 0004), each with one entry of 2 × $3 (0008 has eight entries over an order,
a tax table and a bill term); 0001-0007 posted on 1980-09-01 … 1980-09-07 to A/R or A/P, 0008 on 1980-09-08 and paid
on 1980-09-10. Entry dates are `(current-time)`.

The test's account lookups (TT:1012-1017) resolve by full name from the root, first match depth first
(`Account.cpp:3119-3173`): "Root.Asset.Bank" and "Root.Income" find the **trep** tree's accounts (created first),
"Root.A/Receivable" and "Root.A/Payable" the invoice tree's. The trep accounts have no 1980 splits.

**reconcile book** (TT:1077-1104): a third "Root" tree from the same structure, in the same book, with the seven
1969-1970 transfers again, except trn7 goes Bank → **Expenses** (TT:1103-1104).

### 4.3 The assertions

`default-testing-options` (TT:127-143): Accounts = Bank; Add indenting columns #f; Add options summary `always`;
Table for Exporting #t; Start `start-cal-year`, End `end-cal-year`; Account Name #t; Other Account Name #f; Amount
`single`. "Base" below means these.

| Lines | Options (beyond base or the previous row) | Reads | Expected |
|-------|-------------------------------------------|-------|----------|
| 99-102 | the template's defaults | renders | (red-ale: the no-account warning) |
| 244-246 | base | headers | Date, Num, Description, Memo/Notes, Account, Amount |
| 247-249 | | row −1 col 1 | "Grand Total" |
| 250-252 | | row −1 col −1 | "$2,280.00" |
| 255-268 | indenting #t, Table for Exporting #f, Common Currency #t, Show original #t, summary `never`, Primary account-name, Primary Subtotal #t, Secondary date, Secondary date subtotal monthly | headers | Date, Num, Description, Memo/Notes, Account, "Amount (USD)", Amount |
| 269-271 | | row −1 | "Grand Total", "$2,280.00", "$2,280.00" |
| 272-274 | | row −3 col −1 | "$2,280.00" |
| 275-277 | | row 6 col −1 | "$190.00" |
| 281-287 | + Running Totals `all` | headers | Date, Num, Description, Memo/Notes, Account, "Amount (USD)", "Running Secondary Subtotal (USD)", "Running Primary Subtotal (USD)", "Running Grand Total (USD)", Amount, "Running Secondary Subtotal", "Running Primary Subtotal", "Running Grand Total" |
| 293-303 | base; Accounts = every account of the trep tree; 1969-01-01 to 1970-12-31 absolute; Account Name Filter "Expenses" | row −1 col −1 | "$31.00" |
| 305-309 | filter "Expen.es" | all | none (no-account warning) |
| 311-315 | + regex | row −1 col −1 | "$31.00" |
| 318-323 | + excludes | row −1 col −1 | "-$31.00" |
| 325-330 | regex off | row −1 col −1 | "$0.00" |
| 333-340 | base, 1969-1970; Transaction Filter "desc-3" | row −1 col −1 | "$29.00" |
| 342-346 | "not.s?" | all | none (no matching transactions) |
| 348-352 | + regex | row −1 col −1 | "-$23.00" |
| 354-358 | + excludes | row −1 col −1 | "$24.00" |
| 360-367 | excludes off, regex off, case insensitive, "dEsC-3" | row −1 col −1 | "$29.00" |
| 369-374 | regex on, "NoT.S?" | row −1 col −1 | "-$23.00" |
| 377-384 | base, 1969-1970; Reconciled Status `unreconciled` | row −1 col −1 | "-$20.00" |
| 386-390 | `cleared` | row −1 col −1 | "$29.00" |
| 392-396 | `reconciled` | row −1 col −1 | "-$8.00" |
| 399-407 | base, 1969-1970; Filter By… Income, Filter Type `include` | row −1 col −1 | "$29.00" |
| 409-413 | `exclude` | row −1 col −1 | "-$28.00" |
| 416-423 | base, 1969-1970; Void `void-only` | row −1 col −1 | "$10.00" |
| 425-429 | `both` | row −1 col −1 | "$11.00" |
| 432-440 | base; Accounts Expenses; 1911-01-01 to 2012-12-31; Closing `exclude-closing` | row −1 col −1 | "$111.00" |
| 442-446 | `closing-only` | row −1 col −1 | "-$111.00" |
| 448-452 | `include-both` | row −1 col −1 | "$0.00" |
| 459-472 | base; Display Date, Reconciled Date, Num, Description, Memo, Notes, Account Name, Other Account Name, Shares, Price, Account Balance, Grand Total all #f | header cells, cells of body row 4, of the last row | 2 each |
| 474-483 | + Primary Subtotal #f, Primary date subtotal none, Secondary Subtotal #f, Secondary date subtotal none | same | 1 each |
| 485-488 | + Enable Links #f | links anywhere | none |
| 490-493 | Enable Links #t | links | some |
| 495-501 | + Amount `none` | same three counts | 0 |
| 503-511 | + Primary Subtotal #t, Primary date weekly, Secondary Subtotal #t, Secondary date weekly | header cells, last row's cells | 1 each |
| 514-530 | base, 1969-1970; Primary reconciled-status; every trep account; Display Date, Reconciled Date, Num, Description, Memo, Notes, Account Name, Other Account Name, Shares, Price, Account Balance, Grand Total, Use Full Other Account Name, Use Full Account Name #t; Running Totals `grand` | headers | Date, Reconciled Date, Num, Description, Memo/Notes, Account, Transfer from/to, Shares, Price, Amount, Account Balance, Running Grand Total |
| 531-537 | | col 2 of every row | "1970-03-01" or blank |
| 538-540 | | row −3 | "Total For Unreconciled", "$0.00" |
| 546-557 | base; 1971-02-13 to 1971-02-15; Detail Level `multi-line`; Grand Total #f; all four subtotal options off | col 4 of every row | all equal ("multisplit" ×3) |
| 561-563 | | link texts of each row's last cell | sum 0 |
| 566-571 | + Filter By… Expenses, `exclude` | all | none |
| 575-585 | base; Accounts USD Bank, GBP Bank; 2000-01-01 to 2000-12-31; Common Currency #t; Show original #t | headers | Date, Num, Description, Memo/Notes, Account, "Amount (USD)", Amount |
| 587-596 | + Amount `double`, Account Name #t, Account Code #t, Other Account Name #t, Other Account Code #t | headers | Date, Num, Description, Memo/Notes, Account, Transfer from/to, "Debit (USD)", "Credit (USD)", Debit, Credit |
| 597-599 | | row 2 col 5 | "01-GBP Root.Asset.GBP Bank" |
| 600-602 | | row 7 col 6 | "01-GBP GBP Bank" |
| 603-605 | | row 5 col 10 | "#4.00" (the test reads 4.0) |
| 606-611 | | rows 4 col 8, 9 col 7, 9 col 9 | "$5.00" each (5.0) |
| 612-614 | | row 11 | "Grand Total", "$0.00", "$5.00" |
| 620-634 | base, 1969-1970; every trep account; Sign Reverses `none`; Void `both`; Primary account-name, Primary Subtotal #f; Secondary description, Secondary Subtotal #f | for each row's last-cell link text, "has no `-`" | #f #t #t #f #f #t #t #t #t #f #f #f #f #t |
| 636-641 | `income-expense` | same | #f #t #t #f #f #f #f #f #f #t #t #f #f #t |
| 643-648 | `credit-accounts` | same | #f #t #t #f #f #t #t #t #t #t #t #t #t #f |
| 651-661 | base; Amount `double`; Common Currency #t; Show original #t; Primary date, Primary date subtotal none | headers | Date, Num, Description, Memo/Notes, Account, "Debit (USD)", "Credit (USD)", Debit, Credit |
| 662-664 | | row −1 | "Grand Total", "$2,280.00", "$2,280.00" |
| 665-667 | | row 1 without its first text | "$103 income", "Root.Asset.Bank", "$103.00", "$103.00" |
| 670-675 | base; Account Balance #t | headers | Date, Num, Description, Memo/Notes, Account, Amount, Running Balance |
| 676-678 | | row 1 | "Bank: Balance b/f", "-$99.00" |
| 680-688 | + Primary account-code, Secondary date | headers; row 1 | as above; "Bank: Balance b/f", "-$99.00" |
| 691-698 | base; Running Totals `grand`; Primary Subtotal #f | row −3 col −1 = row −1 col −2 | equal ("$2,280.00") |
| 699-702 | | row 9 col −1 = row 8 col −1 + row 9 col −2 (digits and `-` only) | equal |
| 709-726 | base, 1969-1970; Bank; Grand Total #f; Other Account Name #t; Void `both`; Primary Subtotal #f, Primary date none, Secondary none, Secondary Subtotal #f; Primary date | col 1 | 1969-12-31, 1969-12-31, 1970-01-01, 1970-02-01, 1970-02-10 |
| 728-732 | Primary number | col 2 | trn1, trn2, trn3, trn4, trn7 |
| 734-738 | Primary reconciled-status | col 3 | desc-2, desc-7, desc-3, desc-1, desc-4 |
| 740-744 | Primary memo | col 4 | notes3, memo-1, memo-2, memo-3 |
| 746-749 | Primary account-name | col 5 | sorted with `string<?` |
| 751-755 | Primary corresponding-acc-name | col 6 | Expenses, Expenses, Income, Income, Liabilities |
| 757-761 | Primary notes | col 4 | memo-3, memo-2, memo-1, notes3 |
| 763-767 | Primary amount | each row's last-cell link text | -$15.00, -$8.00, -$5.00, $10.00, $29.00 |
| 769-782 | base; indenting #t; Show original #t; Primary account-name, Subtotal #t; Secondary date, quarterly; Grand Total #t; Show subtotals only #t | col −1 | $570.00 ×4, $2,280.00, $2,280.00 |
| 784-804 | base; every trep account; Grand Total #t; Amount `double`; Show original #t; Table for Exporting #f; indenting #t; Primary account-name, Subtotal #t; Secondary date, quarterly; Informal headers #t; Account Description #f; Account Code #f | row 69; row 91 | "Expenses", "Expense", "Rebate"; "Income", "Charge", "Income" |
| 806-814 | + Account Description #t, Account Code #t | row 69; row 91 | "EXP-001 Expenses", "Expense", "Rebate"; "Income: Salaries etc", "Charge", "Income" |
| 816-822 | + Accounts Bank; Grand Total #f; Show subtotals only #t | col 4 | $570.00 ×4 |
| 824-828 | monthly | col 4 | $190.00 ×12 |
| 830-834 | yearly | col 4 | $2,280.00 |
| 836-844 | 1969-12-31 to 1972-12-31; Show subtotals only #f; Void `both`; daily | row 5 col 4 | "$39.00" |
| 846-851 | Show subtotals only #t; weekly | col 4 | "$34.00", "$89.00" |
| 852-854 | | col 5 | "$15.00", "$8.00", "$100.00" |
| 857-869 | base; Grand Total #f; Running Totals `sub`; Primary account-name, Subtotal #t; Secondary date, monthly | headers | Date, Num, Description, Memo/Notes, Account, Amount, Running Secondary Subtotal, Running Primary Subtotal |
| 870-872 | | row −3 col −1 = row −1 col −3 | equal |
| 873-876 | | row 10 col −1 = row 9 col −1 + row 10 col −3 | equal |
| 877-879 | | row 5 col −2 = row 6 col −3 | equal |
| 880-883 | | row 10 col −2 = row 9 col −2 + row 10 col −3 | equal |
| 885-891 | Running Totals `all` | headers | … Amount, Running Secondary Subtotal, Running Primary Subtotal, Running Grand Total |
| 893-898 | `none` | headers | Date, Num, Description, Memo/Notes, Account, Amount |
| 902-920 | base; the 14 Display booleans of TT:906-908 #t; Running `all`; Primary account-name, Subtotal #t; Secondary date, monthly; Show subtotals only #t; Common Currency #t; Show original #t | headers | "Amount (USD)", Amount |
| 922-926 | + Amount `double` | headers | "Debit (USD)", "Credit (USD)", Debit, Credit |
| 933-946 | base; Accounts Bank, GBP Bank, Income-GBP, Income, Expenses; start/end of calendar year; Subtotal Table #t; Primary account-name, Subtotal #t; Secondary date, monthly | grid row 1 | Bank, $190.00 ×12, $2,280.00, $190.00 |
| 947-950 | | row 2 | GBP Bank, #51.00 ×12, #612.00, #51.00 |
| 951-954 | | row 3 | Expenses, $22.00 ×12, $264.00, $22.00 |
| 955-959 | | row 4 | Income, -$212.00 ×12, -$2,544.00, -$212.00 |
| 960-963 | | row 5 | Income-GBP, -#51.00 ×12, -#612.00, -#51.00 |
| 964-966 | | row 6 | Total, #0.00 ×14 |
| 967-969 | | row 7 | $0.00 ×14 (the label is `""`) |
| 971-976 | 1969-01-01 to 1970-12-31 | row 1 | Bank, $29.00, -$5.00, -$23.00, $1.00, $0.33 |
| 977-979 | | row 2 | Expenses, $16.00, $15.00, $31.00, $10.33 |
| 980-982 | | row 3 | Income, -$29.00, -$29.00, -$9.67 |
| 983-985 | | row 4 | Total, $0.00, $11.00, -$8.00, $3.00, $1.00 |
| 986-1003 | | cols 1-6 | Bank, Expenses, Income, Total; $29.00, -$29.00, $0.00; -$5.00, $16.00, $11.00; -$23.00, $15.00, -$8.00; $1.00, $31.00, -$29.00, $3.00; $0.33, $10.33, -$9.67, $1.00 |
| 1007-1021 | base; invoice book; 1980-09-01 to 1980-09-07; Invoice #t; Accounts as §4.2 | col 5 | 0003, 0004, 0006, 0007, 0001, 0002, 0005 |
| 1025-1047 | base; Accounts Bank, USD Bank, GBP Bank, Income-GBP, Income, Expenses; 1969-1970; Subtotal Table #t; Common Currency #t, Report's currency GBP; Show original #t; Primary account-name, Subtotal #t; Secondary date, monthly; export `csv` | export error | none |
| 1048-1050 | | export string | `"from","1969-01-01"⏎"to","1970-12-31"⏎"Amount (GBP)",2.15⏎"Amount",3.0` (`⏎` = `\n`) |
| 1055-1057 | `lists->csv '(())` | | `""` |
| 1058-1061 | `(("from" "01/01/2010"))` | | `"from","01/01/2010"` |
| 1062-1069 | three rows, the last `("total" 23500 30000 25/7 sym)` | | padded to 5 cells; `23500.0`, `30000.0`, `3.5714285714285716`, `sym` |
| 1070-1074 | an improper list | | an error |
| 1107-1109 | RR; Accounts the reconcile tree's Bank, Liabilities; defaults | renders | |
| 1110-1115 | 1970-03-01 to 1970-03-31 | row 3 | "Total For Reconciled", "$8.00" |
| 1116-1118 | | row 6 | "Total For Cleared", "$29.00" |
| 1119-1121 | | row 11 | "Total For Unreconciled", "$31.00" |

The `lists->csv` cases are unit tests of `render::csv`; the last (a symbol, an improper list) has no counterpart, since
red-ale's export cells are strings, numbers or null.

Worked checks of three of these, against §2, as evidence the model above is right:

- TT:606-614: GBP −10 on 2000-01-15 converts at the nearest price, USD in GBP 10/14 inverted (1.4), to −$14; GBP +6
  on 2000-02-15 at 1.5 to +$9: GBP Bank's converted credit total is $5. The Grand Total's first line is USD (the
  converted columns' commodity comes first, §2.8): converted debits and credits both $23, so `$0.00` in Credit;
  originals USD +14 −9 = `$5.00` in Debit.
- TT:799-801: rows 1-46 are Bank (subheading, four quarters of subheading + 9 splits + subtotal, primary subtotal),
  47-68 GBP Bank (1 + 4 × 5 + 1), so Expenses' subheading is row 69 and Income's row 91.
- TT:1048-1050: every 1969-1970 USD split converts at the 2000-01-15 rate 10/14 and is rounded to GBP's SCU with
  banker's rounding (`gnc-pricedb-convert-balance-nearest-price-t64`): 20.71 − 3.57 − 10.71 − 5.71 + 3.57 + 7.86 +
  10.71 − 20.71 = 2.15.

### 4.4 Can the oracle rebuild the books?

Yes, through the 5.17 Python bindings and the ctypes layer `scripts/report-fixtures/engine_extras.py` already uses.
Every engine call TEE makes for the trep and reconcile books is in `gnucash_core_c` (`xaccTransVoid`,
`xaccTransSetIsClosingTxn`, `xaccSplitSetDateReconciledSecs`, `xaccAccountSetCode`, `xaccAccountSetDescription`,
`gnc_commodity_set_user_symbol`, the pricedb calls), and `engine_extras.py` already ports `env_transfer`,
`env_transfer_foreign`, `env_create_multisplit_transaction` and the account structure. For the invoice book the
bindings have `gncInvoiceCopy`, `gncInvoiceSetIsCreditNote`, `gncEntrySetDocQuantity`, `gncInvoicePostToAccount`,
`gncInvoiceApplyPayment`, `gncOwnerInitJob`, `gncOwnerInitEmployee`, `gncTaxTableEntrySetAmount`,
`gncEntrySetInvDiscountHow`, `gncBillTermSetDueDays`; **not** `gncOrderAddEntry` (checked in
`gnucash_core_c.py`), which inv-8's entries also go through (TEE:806). Call it through ctypes on
`libgnc-engine.so`; leaving the order out changes nothing the report reads, but the book would no longer be TEE's.

No Guile is loaded: the scripts make the engine calls themselves, as for v0.2 and v0.4.

### 4.5 Proposed fixtures

| Fixture | Built from | Used by |
|---------|-----------|---------|
| `reports/transaction.gnucash` | the trep book (TT:104-226), then `create-test-invoice-data` (TEE:487-849) in the same book, in that order | the Transaction Report (all of TT:239-1050); hand-worked GL and GJ cases |
| `reports/reconcile.gnucash` | the reconcile book (TT:1077-1104) alone | the Reconciliation Report (TT:1107-1121); hand-worked cases |

One script, `scripts/report-fixtures/transaction.py`, builds both and is added to `build-all.sh`. The reconcile tree
gets its own book: GnuCash builds it into the same book, but the report reads only its two accounts and sorts by
reconcile state, so no figure or order depends on the other trees, and the account lookups of the trep tests stay
unambiguous. The trep and invoice trees share one book because TT:1012-1017 resolve names across both.

Pinning, with `reports.md` §13's now **2024-03-15 12:00 UTC** and `TZ=UTC`:

- `YEAR` (TT:117) is 2024; the twelve months of transactions run from 2024-01-03 to 2024-12-15, past the pinned now,
  as GnuCash's run in any year does. `start-cal-year`/`end-cal-year` resolve to 2024-01-01 and 2024-12-31 23:59:59
  with the run's `now`.
- The invoice entries' dates (`(current-time)`, TEE:621-726, :782) are the pinned now.
- Transactions that `gncInvoicePostToAccount` and `gncInvoiceApplyPayment` make, and their splits and lots, get
  fixed GUIDs and entered dates after the call, as `engine_extras` does for others (`qof_instance_set_guid` moves the
  instance within its collection, `5.17:libgnucash/engine/qofinstance.cpp:463`; check the saved book reopens).
- The RR "basic run" (TT:1108-1109) uses relative defaults: from `start-prev-quarter` to `today`. Quarters count
  from the start of the accounting period, today (the 15th) with no preferences, so the run starts on 2023-12-15
  (`5.17:libgnucash/engine/gnc-option-date.cpp:519-596`).

`transaction.expected.json` names accounts by GUID: three accounts are called "Root" and two full names are
`Root.Asset.Bank`. "Every trep account" (TT:294, :518, :623, :785) is the trep tree's accounts **including** its
"Root", as `gnc-account-get-descendants` of the real root returned before the invoice tree existed.

### 4.6 Hand-worked cases the test does not reach

Each cites the TE lines it follows: Date Filter `entered` and `reconciled` on the Transaction Report (TE:2566-2578);
Date Entered, Invoice with two invoices (reverse order), Link with and without Enable Links, Price and Shares cells;
Trans Number and the `number` key in a split-action book (and the option names that change, TE:910-912, :930-933,
:245-256); Primary Key `none` (§7 Q3); descending orders through QOF and through the custom sort; `t-number` and
`description` keys; `corresponding-acc-code` with a split transaction (`"Split"`); `reconciled-date` key with a date
subtotal (groups by **posted** date, §2.6); regex errors (both warnings); `unique-transactions`; `void-only` with
`unreconciled` (nothing); multi-currency subtotal rows with original amounts; the Subtotal Table without a secondary
key (no Average) and with a single column; Balance b/f under Common Currency; the weekly label at UTC (fact 8);
`bal-bf` switched off by each of its conditions; the CSV export errors; General Ledger defaults (no balance column,
Grand Total, account-code subheadings with codes, `credit-accounts` signs) and its title on a warning; General
Journal rows (transaction row, notes row, one row per split, a two-transaction day, a void transaction excluded);
Reconciliation Report with Common Currency on (amounts unconverted) and with no accounts (warning plus
instructions).

---

## 5. Floating point, collation, regular expressions

The engine itself computes exactly; GnuCash's doubles are in printing.

Proposed rows for `reports.md` §12, "Floating point in GnuCash, exact in red-ale":

| Where | GnuCash | red-ale |
|-------|---------|---------|
| Transaction Report CSV numbers | `(number->string (exact->inexact n))` (TE:496) | as v0.3's chart CSV: the shortest digits of the nearest double, computed exactly (`number/flonum.rs`) |
| Shares column | the amount through `double-to-gnc-numeric … 100 GNC-RND-ROUND` (TE:1216-1218; `report/html-style-info.scm:233-236`) | the exact amount rounded to 1/100 with banker's rounding; differs only if the double lands across a rounding boundary |

Proposed rows for `reports.md` §14, "Deviations from GnuCash":

| What | GnuCash 5.17 | red-ale | Why |
|------|--------------|---------|-----|
| Primary Key "None" with no custom key | No query sort: hash order (fact 3) | Split order (`xaccSplitOrder`) | Determinism; GnuCash's order is undefined |
| Name comparisons in the engine's sorts and the Subtotal Table (`gnc:string-locale<?`, TE:2114, :2468; `g_utf8_collate` per path level, `Split.cpp:1687-1709`) | Locale collation | Bytewise, per path level where GnuCash compares per level | As v0.1; covered by "Name sorting" |
| Subtotal Table rows or columns with equal sort values and different labels (two accounts with the same code) | Hash order (TE:2086-2090, :2118-2119) | Ordered by label, then first appearance | Determinism |
| Account and transaction filters: POSIX extended expressions (`make-regexp`, `regexp/icase`), `string-contains-ci` | Guile | Rust `regex`; lowercase for case-insensitive substring | As the split query's row |
| "Show Account Description" with an Other Account key on a split transaction | A Scheme error (`xaccAccountGetDescription` of no account, TE:1828-1829) | No description | The report runs |
| Warning documents of the engine | Untitled (TE:2501-2531, :2596-2605) | Titled with the Report name | As the other untitled reports |
| Formal accounting labels preference | Debit/Credit on informal headers when set (`Account.cpp:4081-4082`) | Never set: the friendly labels, GnuCash's default (§7 Q5) | No preference store |
| General Journal query | A `__reg/query` option holding a query (REG:327-328) | The query is fixed in the script | GnuCash's GUI never changes it for the General Journal; red-ale has no query option type |

---

## 6. Translation plan

Tickets under one epic ("Transaction Report engine"), each under ~500 lines of Rust and Rhai excluding tests,
fixtures and docs. Order and dependencies:

| # | Ticket | Delivers | Depends on | Size |
|---|--------|----------|------------|------|
| 1 | **Split query and reads** (core + query API) | §3.1: `reconcile`, `isClosing`, `sort`, `unique` on `book.split_query` over a core `SortKey` list and QOF's sort rules; Split `otherSplit`, `corrAccountFullName`, `corrAccountCode`, `voidFormerAmount`, `lot`/invoice; Transaction `docLink`; Account `debitLabel`/`creditLabel`; `book.uses_split_action_for_num()`. `reports.md` §5 updated. An oracle test running GnuCash's QofQuery with each sort key on a test book (`qof_query_set_sort_order` is in the bindings) | — | ~400 |
| 2 | **Fixtures** | `scripts/report-fixtures/transaction.py`, `reports/transaction.gnucash`, `reports/reconcile.gnucash`, their `expected.json` from §4.3, the oracle check that the committed books are what the script builds | — | script only |
| 3 | **Options, context and result** | `o.set_default`, `o.make_internal`; `ctx.print_month_year`, `ctx.print_year`; `lib/dates` week and canonical-day functions; result: split, invoice and doc-link links, `anchors` cells, row `alternate`, `exportError`, zero-column tables; renderer and validator for each; `redAleApi` 0.5.0 and its committed 0.4.0 document | — | ~400 |
| 4 | **Engine: options, parameters, query, filter, sort** | `lib/trep-engine`'s `options` (§2.1) and the first half of `render`: parameters (§2.2), warnings (§2.3), the query (§2.4), the split filter (§2.5), the custom sort (§2.6); returns the splits. Unit tests on the fixture through a test template | 1, 3 | ~450 |
| 5 | **Engine: the table** | columns and calculated cells (§2.7), rows and their order, subheadings, subtotals, grand total (§2.8), the document (§2.10), CSV rows and export errors (§2.11) | 4 | ~500; split off running totals and Balance b/f if it passes |
| 6 | **Engine: Subtotal Table** | the grid (§2.9) | 5 | ~150 |
| 7 | **Transaction Report** | `standard/transaction.rhai`; every TT assertion of §4.3 on `transaction.gnucash`, XML and SQLite, `report.run` and `report.export`; the hand-worked Transaction Report cases of §4.6; golden bytes; `features.md` | 5, 6, 2 | script ~40; tests |
| 8 | **General Ledger and Reconciliation Report** | `standard/general-ledger.rhai`, `standard/reconcile-report.rhai`; RR's TT assertions on `reconcile.gnucash`; hand-worked GL and RR cases | 7 | ~150 |
| 9 | **General Journal** | `lib/register` (REG's options and renderer, §1.4) and `standard/general-journal.rhai`; hand-worked cases on `transaction.gnucash` | 1, 3; §7 Q1 | ~350 |

Tickets 1, 2 and 3 can run in parallel. The CSV path (ticket 5) is tested through the engine with
`ctx.export_type() == "csv"` whatever §7 Q2 decides, because TT:1024-1050 tests it and the GST Statement (v0.6) needs
it.

---

## 7. Open questions

1. **The General Journal rests on the Register report.** Options: (a) translate REG's options and renderer into
   `lib/register` and build the General Journal on it, keeping the Register *template* ❌; (b) approximate the
   General Journal with the engine, which differs in layout (notes rows, every split under each transaction, no
   subtotals). *Proposed:* (a). It gives GnuCash's General Journal exactly; the Register report stays out because
   its input is a register window's query, which red-ale has no way to supply, not because its renderer is hard.
   Correct `milestone5.md`'s row ("on `register.scm`, not the engine").
2. **CSV export of the Transaction Report.** GnuCash 5.17 offers none (fact 7). *Proposed:* the engine builds the
   rows and export errors (needed for the test and for the GST Statement), but no v0.5 template declares
   `exportTypes: ["csv"]`, matching GnuCash. Declaring it later would be an additive extension.
3. **Primary Key "None".** *Proposed:* split order, recorded as a deviation (§5). Alternative: walk transactions in
   Book order, which is no more GnuCash's than split order is.
4. **Result additions** (§3.4: three link kinds, `anchors`, `alternate`, `exportError`). *Proposed:* all, as one
   additive `redAleApi` 0.5.0 change in ticket 3. `alternate` could be dropped if row striping by position is judged
   close enough; it changes only shading.
5. **"Use formal accounting labels".** A GnuCash preference, not in the Book. *Proposed:* no setting in v0.5; the
   friendly labels, which GnuCash's tests also get. A `reports.formalAccountingLabels` setting can be added if asked
   for.
6. **`o.set_default` on an option that does not exist.** GnuCash ignores it, and GL and RR rely on that (fact 2).
   *Proposed:* red-ale refuses it (a `report-script` error catches typos in user scripts), and the translations
   leave out GL's and RR's no-op calls with a comment citing GL:82-83, :102-103 and RR:53-57. The figures are the
   same.
7. **Two account-name sorts** (fact 5). *Proposed:* keep both, as GnuCash does: the query's per-level comparison
   when the query sorts, whole full names when the custom sort runs. A user sees a different order only with a name
   containing a character below `.`.
8. **SQLite fixtures.** *Proposed:* as v0.4, the tests copy each XML fixture to SQLite through the oracle and run
   every assertion on both.
