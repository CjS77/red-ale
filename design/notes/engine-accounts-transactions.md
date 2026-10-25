# GnuCash 5.17 engine rules: accounts and transactions

Domain rules of the GnuCash 5.17 engine behind the milestone-1 features of §1 "Books and accounts" and §2
"Transactions" (see [`../milestone1.md`](../milestone1.md)). It is written so that `red-ale-core` and
`red-ale-service` can be implemented without re-reading GnuCash source.

- **Citations** look like `5.17:path:line`. Paths are relative to the GnuCash repository root.
- **"Oracle"** marks a behaviour confirmed by running the GnuCash 5.17 Python bindings. The scripts are in the session
  scratchpad, under `engine/`.
- **XML file layout is out of scope.** This note mentions element and slot names only where an engine rule decides
  what gets written.

Contents:

1. gnc_numeric
2. Accounts
3. Renumber sub-accounts; delete with move
4. Accounting period
5. Transactions and splits
6. Void, unvoid, reverse
7. Find transactions
8. Reconcile against a statement
9. Scrubs and Check & Repair
10. Exchange rates on transfer
11. Open questions

---

## 1. gnc_numeric

### 1.1 Representation

- `struct { gint64 num; gint64 denom; }` (`5.17:libgnucash/engine/gnc-numeric.h:58-67`).
- **Error values** have `denom == 0` and `num == error code`: `GNC_ERROR_ARG = -1`, `OVERFLOW = -2`,
  `DENOM_DIFF = -3`, `REMAINDER = -4` (`gnc-numeric.h:221-233`).
  - `gnc_numeric_check` maps any other `num` with `denom == 0` to `OVERFLOW`, and `0/0` to `ARG`
    (`gnc-numeric.cpp:737-757`).
- **Negative denominators** are legal and mean "multiply": `n/-d` is `n*d`. For example, `5/-10 + 1` gives `51/1`
  (oracle). `GncRational(gnc_numeric)` converts it to `n*d / 1` (`gnc-rational.cpp:39-47`). `gnc_numeric_reduce`
  leaves such a value unchanged (`gnc-numeric.cpp:1216`). GnuCash never writes one for amounts, so red-ale can reject
  negative denominators on input from the API, but the XML reader must accept them.
- **Arithmetic is done in 128-bit** (`GncInt128`). A result whose numerator or denominator is "big" (does not fit 64
  bits) is first reduced, then `round_to_numeric()`. That scales the value down until it fits, rounding *half-down*
  (`gnc-numeric.cpp:71-85`, `gnc-rational.cpp:180-241`).
  - If the integer part alone does not fit, the operation fails with `OVERFLOW`. For example, `2^62 × 4` gives `-2/0`
    (oracle).

### 1.2 Arguments to add, sub, mul, div and convert

Every operation takes `(a, b, denom, how)` (`gnc-numeric.h:69-118`).

- `denom` is either `GNC_DENOM_AUTO = 0` or an explicit target denominator (`:245`).
- `how = round_mode | denom_mode | (sigfigs << 8)`. The masks are `RND 0x0f`, `DENOM 0xf0`, `SIGFIGS 0xff00`
  (`:126-128`, `:217-218`).

**Rounding modes** (`gnc-numeric.h:144-178`). The implementations are in `gnc-rational-rounding.hpp:84-221`; the
results below for −0.125 → hundredths are from the oracle.

| Name | Value | Rule | −0.125 → |
|---|---|---|---|
| `RND_FLOOR` | 0x01 | toward −∞ | −0.13 |
| `RND_CEIL` | 0x02 | toward +∞ | −0.12 |
| `RND_TRUNC` | 0x03 | toward 0 | −0.12 |
| `RND_PROMOTE` | 0x04 | away from 0 whenever there is a remainder | −0.13 |
| `RND_ROUND_HALF_DOWN` | 0x05 | nearest; a tie goes toward 0 (`|2·rem| > |den|`) | −0.12 |
| `RND_ROUND_HALF_UP` | 0x06 | nearest; a tie goes away from 0 (`|2·rem| >= |den|`) | −0.13 |
| `RND_ROUND` | 0x07 | banker's: nearest; a tie goes to the even quotient | −0.12 (0.135 → 0.14) |
| `RND_NEVER` | 0x08 | any remainder is an error: `GNC_ERROR_REMAINDER` (`-4/0`) | error |
| *(no mode, `how & 0xf == 0`)* | 0 | treated as TRUNC (`gnc-numeric.cpp:724-733`) | −0.12 |

**Denominator modes.** These only matter when `denom == GNC_DENOM_AUTO`.

| Name | Value | 5.17 behaviour |
|---|---|---|
| `DENOM_EXACT` | 0x10 | Exact `GncRational` result in 128-bit, with no reduction. If the target is AUTO and the rounding mode is not NEVER, the result goes through `round_to_numeric()`. Then it is converted, and it is an `OVERFLOW` error if it is still big (`gnc-numeric.cpp:944-952`). |
| `DENOM_REDUCE` | 0x20 | Result reduced to lowest terms before conversion (`:672-673`). `50/100 + 50/100` gives `1/1`. |
| `DENOM_LCD` | 0x30 | `denom = lcm(a.denom, b.denom)` (`:912-922`). `50/100 + 25/1000` gives `525/1000`. |
| `DENOM_FIXED` | 0x40 | **Same as AUTO in 5.17.** `DENOM_DIFF` is never raised: `1/2 + 1/3` with `FIXED|NEVER` gives `5/6` (oracle). The header comment (`gnc-numeric.h:202-206`) is obsolete. |
| `DENOM_SIGFIG` | 0x50, plus `GNC_HOW_DENOM_SIGFIGS(n)` | The target denominator is a power of ten chosen to keep `n` significant figures (`sigfigs_denom`, `gnc-numeric.cpp:469-486`). If `|x| > 1`, `10^(n - digits(int part))` with a minimum of 1; otherwise `10^(n + leading zeros)`. Examples: `2/3`, 6 figures → `666667/1000000`; `123456.78`, 6 figures → `123457/1`; `1/1000`, 3 figures → `1000/1000000` (not reduced) (oracle). |

**AUTO with no denominator flag** (the common case `how = RND_*` only) uses the plain result of the operator. That
result is not reduced:

- add and sub use the lcm of the denominators: `1/3 + 1/6` gives `3/6` (oracle);
- mul gives `a.num*b.num / a.den*b.den`: `150/100 × 2/1` gives `300/100`;
- div gives `a.num*b.den / a.den*b.num`, or just `a.num/b.num` when the denominators are equal (`gnc-rational.cpp:281-309`).

**Shortcuts and special cases:**

- add with either operand zero returns the other operand unchanged, including its denominator (`gnc-numeric.cpp:619-627`).
- mul returns `0/1` if either operand is zero (`:637-646`).
- div returns `0/1` for `0 / x`. Division by zero is `OVERFLOW` (`:650-662`, caught at `:1128-1131`).
- An invalid argument (`denom == 0`) to any operation gives `GNC_ERROR_ARG` (`:931-934`).

**`gnc_numeric_convert(n, denom, how)`** (`gnc-numeric.cpp:1173-1201`):

- The algorithm (`GncNumeric::convert`, `gnc-numeric.hpp:192-201`; `prepare_conversion`, `gnc-numeric.cpp:446-466`):
  1. Reduce `new_denom/old_denom` to `p/q`.
  2. Compute `t = num*p` in 128-bit.
  3. Set `rem = t % q` and `num' = t / q`.
  4. If `rem != 0`, apply the rounding rule.
  5. The result is `num'/new_denom`.
- `denom == AUTO` keeps the old denominator, except under REDUCE or SIGFIG.
- An error input is returned unchanged.
- `convert(1/3, 100, NEVER)` gives `-4/0` (REMAINDER).

### 1.3 Comparison and equality

- **`gnc_numeric_compare`** returns −1, 0 or 1. With equal denominators it compares numerators; otherwise it
  cross-multiplies in 128-bit. If either argument is an error it returns **0** (`gnc-numeric.cpp:837-855`).
- **`gnc_numeric_eq(a,b)`** is *representation* equality: `num == num && denom == denom` (`:862-866`). So `1/2` is not
  `eq` to `50/100`.
- **`gnc_numeric_equal(a,b)`** is *value* equality: `compare == 0`. Two error values are equal; an error and a valid
  value are not (`:873-894`).
- **`gnc_numeric_same(a,b,denom,how)`** converts both to `denom`, then calls `equal` (`:900-909`).
- `zero_p`, `negative_p` and `positive_p` return false for error values (`:763-830`).

### 1.4 String form

- **`gnc_numeric_to_string`** gives `"%PRId64/%PRId64"`, always `num/denom`, never reduced. For example, `0/100`,
  `-1235/100` (`gnc-numeric.cpp:1376-1385`). This is the form used for split value and quantity in XML, and for
  numeric slots (oracle: `<split:value>-1235/100</split:value>`, `<slot:value type="numeric">700/100</slot:value>`).
- **`gnc_numeric_from_string`** first tries the fast form `[-]digits[/digits]`, where the denominator must be greater
  than 0. A bare `n` becomes `n/1` (`:217-243`, `:1406-1429`).
  - Otherwise the `GncNumeric(string)` regex parser runs. It also accepts hex, "a b/c", decimals with `.` or `,`,
    grouped thousands and scientific notation (`:245-400`). red-ale's file reader only needs the fast form; anything
    else in a GnuCash file is suspect.
- `operator<<` prints decimals when the denominator is a power of ten (`gnc-numeric.hpp:356-375`). This is for
  display only and never goes into the file.

### 1.5 What split setters do to denominators (load-bearing for round-trip)

| Setter | Conversion | Citation |
|---|---|---|
| `xaccSplitSetValue(s, v)` | `convert(v, fraction(trans currency), ROUND_HALF_UP)`; AUTO (no change) if the split has no parent or the parent has no currency. **If the converted value is an error, or is 0 while `v != 0`, the new value is refused and the old value kept** (a `PERR` is logged). | `Split.cpp:1147-1158`, `:1279-1312` |
| `xaccSplitSetAmount(s, a)` | `convert(a, xaccAccountGetCommoditySCU(acc), ROUND_HALF_UP)` when the split has an account; otherwise stored as given. It also sets `adjusted_amount = amount`. | `Split.cpp:1160-1172`, `:1242-1267` |
| `xaccSplitSetSharePriceAndAmount(s, p, a)` | amount as above; `value = mul(amount, price, currency fraction, ROUND_HALF_UP)` | `Split.cpp:1176-1193` |
| `xaccSplitSetSharePrice(s, p)` | `value = mul(amount, p, currency fraction, ROUND_HALF_UP)`; a zero price is ignored | `Split.cpp:1204-1224` |
| `xaccSplitSetBaseValue(s, v, base)` | value and/or amount depending on which commodity `base` is; both use ROUND_HALF_UP (see §10) | `Split.cpp:1354-1407` |
| `xaccSplitSetParent(s, t)` | **re-applies `xaccSplitSetValue(s, value)`**, which converts the value to the new transaction's currency fraction | `Split.cpp:1904-1906` |
| `xaccSplitCommitEdit` when the split enters a new account | **re-applies `xaccSplitSetAmount(s, amount)`**, which converts to the new account's SCU | `Split.cpp:1020-1031` |
| `xaccAccountSetCommodity(acc, c)` | resets SCU to `fraction(c)` and `non_standard_scu = FALSE`, then re-applies `SetAmount` on every split | `Account.cpp:2666-2701` |

Oracle: `SetValue(-12345/1000)` on a USD transaction stored `-1235/100` (−12.345 rounds half-up to −12.35).
`SetValue(1/3)` stored `33/100`.

**Consequence: GnuCash normalises denominators when it loads a file.**

- The XML reader parses each split's value and quantity *before* the split has a parent or account, so nothing is
  converted at that point (`5.17:libgnucash/backend/xml/gnc-transaction-xml-v2.cpp:258-269`).
- Then `xaccTransAppendSplit` → `xaccSplitSetParent` converts the value to the currency fraction
  (`gnc-transaction-xml-v2.cpp:490-494`).
- At commit, `xaccSplitCommitEdit` converts the quantity to the account SCU.
- Oracle: a file holding `<split:value>12350/1000</split:value><split:quantity>12345/1000</split:quantity>` in a
  USD/SCU-100 account, and `-1/3` / `-1/3`, was opened and saved after an unrelated edit. It was written back as
  `1235/100` / `1235/100` and `-33/100` / `-33/100`.
- So for any split whose denominators differ from (currency fraction, account SCU), GnuCash 5.17 changes the stored
  numbers on its next save. red-ale must decide whether to do the same (see §11). GnuCash's own files always already
  match, so normal books are unaffected.

---

## 2. Accounts

### 2.1 Types

The XML `<act:type>` holds the enum name, for both writing and parsing (`Account.cpp:4232-4300`). The integer values
come from `Account.h:104-173`.

| Enum | Int | XML string | UI label | Fundamental type (`Account.cpp:4485-4510`) |
|---|---|---|---|---|
| BANK | 0 | `BANK` | Bank | ASSET |
| CASH | 1 | `CASH` | Cash | ASSET |
| ASSET | 2 | `ASSET` | Asset | ASSET |
| CREDIT | 3 | `CREDIT` | Credit Card | LIABILITY |
| LIABILITY | 4 | `LIABILITY` | Liability | LIABILITY |
| STOCK | 5 | `STOCK` | Stock | ASSET |
| MUTUAL | 6 | `MUTUAL` | Mutual Fund | ASSET |
| CURRENCY | 7 | `CURRENCY` | Currency (deprecated: excluded from `xaccAccountTypesValid`, `:4460-4467`) | ASSET |
| INCOME | 8 | `INCOME` | Income | INCOME |
| EXPENSE | 9 | `EXPENSE` | Expense | EXPENSE |
| EQUITY | 10 | `EQUITY` | Equity | EQUITY |
| RECEIVABLE | 11 | `RECEIVABLE` | A/Receivable | ASSET |
| PAYABLE | 12 | `PAYABLE` | A/Payable | LIABILITY |
| ROOT | 13 | `ROOT` | Root (hidden root of the tree; never offered) | — |
| TRADING | 14 | `TRADING` | Trading | NONE |
| (NONE) | −1 | `NONE` | — | — |
| CHECKING, SAVINGS, MONEYMRKT, CREDITLINE | 15–18 | parsed and written, but never created (`NUM_ACCOUNT_TYPES = 15`) | — | ASSET / LIABILITY |

- `xaccAccountSetType` refuses any value `>= NUM_ACCOUNT_TYPES` (`Account.cpp:2425-2432`).
- The UI labels are translated (`:4310-4335`).
- *Priced* types, i.e. those with a security commodity, are STOCK, MUTUAL and CURRENCY (`:4539-4547`).
- `xaccAccountIsAssetLiabType` is true for types compatible with an ASSET or LIABILITY parent, excluding A/R and A/P
  (`:4471-4480`).

### 2.2 Parent/child compatibility

- `xaccAccountTypesCompatible(parent, child)` is true when the child's allowed-parent mask contains the parent
  (`Account.cpp:4442-4457`).
  - NONE is compatible with nothing.
  - ROOT can never be a child.

| Child type | Allowed parent types (`xaccParentAccountTypesCompatibleWith`, `:4395-4440`) |
|---|---|
| BANK, CASH, ASSET, STOCK, MUTUAL, CURRENCY, CREDIT, LIABILITY, RECEIVABLE, PAYABLE | any of those ten, or ROOT |
| INCOME, EXPENSE | INCOME, EXPENSE or ROOT |
| EQUITY | EQUITY or ROOT |
| TRADING | TRADING or ROOT |

- **The engine never enforces this rule.** `gnc_account_append_child` accepts any parent (`:2824-2884`). The account
  dialog checks it on OK: "The selected account type is incompatible with the one of the selected parent"
  (`5.17:gnucash/gnome-utils/dialog-account.c:1008-1015`).
- When the type of an existing account changes to one incompatible with its children, the dialog **recursively sets
  every descendant to the new type** (`make_children_compatible` / `set_children_types`, `dialog-account.c:686-726`).
- **Type changes on an account that has splits** are limited to `xaccAccountTypesCompatibleWith(old)`
  (`dialog-account.c:1714-1728`, `Account.cpp:4355-4393`):
  - {BANK, CASH, ASSET, CREDIT, LIABILITY, INCOME, EXPENSE, EQUITY} can change among themselves;
  - {STOCK, MUTUAL, CURRENCY} can change among themselves;
  - RECEIVABLE, PAYABLE and TRADING cannot change at all.
- **Other dialog checks** on OK (`dialog-account.c:936-1080`):
  - the name is not empty;
  - no other account has the same full name, i.e. siblings must have distinct names;
  - a parent is selected;
  - a type is chosen;
  - a commodity is chosen;
  - balance limits are consistent.
- **Commodity rules** by type: STOCK and MUTUAL take a non-currency commodity, TRADING takes any, and all other types
  take a currency (`dialog-account.c:224-241`).

### 2.3 Commodity and SCU

- **`commodity_scu`** is the account's smallest unit, as a denominator.
- **`non_standard_scu`** says the SCU differs from the commodity's fraction. In XML these are `<act:commodity-scu>`
  and `<act:non-standard-scu/>`.
- `xaccAccountGetCommoditySCU` returns `commodity_scu` when `non_standard_scu` is set or the account has no commodity;
  otherwise it returns `gnc_commodity_get_fraction(commodity)` (`Account.cpp:2732-2743`).
  `xaccAccountGetCommoditySCUi` returns the raw field (`:2725-2730`).
- `xaccAccountSetCommoditySCU(scu)` stores `scu`, and sets `non_standard_scu = TRUE` if `scu != fraction(commodity)`.
  The reader calls it while parsing, to catch old mismatches (`:2703-2723`).
- `xaccAccountSetCommodity` resets both fields and re-rounds every split's amount (§1.5).
- In the dialog, the SCU choice is shown as "Smallest fraction": 1/10^k, or "Use commodity value"
  (`dialog-account.c:323-335`).

### 2.4 Fields and slots

**Booleans in account slots** are written as the *string* `"true"`. Setting one to false **deletes the slot**
(`set_kvp_boolean_path`, `Account.cpp:2557-2561`). The reader accepts the string `"true"`, or an int64 that is not 0
(`:2563-2577`). String setters delete the slot when the value is NULL or `""` (`set_kvp_string_path`, `:2518-2529`).

| Concept | Storage | Getter/setter | Notes |
|---|---|---|---|
| Name | `<act:name>` | `xaccAccountSetName` (`:2446-2463`) | Must not contain the separator (§2.5). |
| Code | `<act:code>`, written only if not empty | `xaccAccountSetCode` (`:2466-2480`) | NULL is stored as `""`. Free text, used as the first sort key (§2.6). |
| Description | `<act:description>` | `xaccAccountSetDescription` | |
| Notes | slot `notes` (string) | `:2643-2646`, `:3362-3365` | On XML load, notes that are empty after trimming are deleted (`Scrub.cpp:1392-1399`, called from `io-gncxml-v2.cpp:220`). |
| Placeholder | slot `placeholder` = `"true"` | `:4107-4116` | The load scrub deletes `placeholder = "false"` (`Scrub.cpp:1401-1405`). The engine still allows splits in a placeholder account. Only the register refuses them (see §3). |
| Hidden | slot `hidden` = `"true"` | `:4177-4187` | `xaccAccountIsHidden` is true if the account or any ancestor is hidden (`:4189-4205`). |
| Tax-related | slot `tax-related` = `"true"` | `:4022-4032` | |
| TXF code | slot `tax-US/code` (string, e.g. `"N123"`) | `:4034-4044` | |
| TXF payer name source | slot `tax-US/payer-name-source` (string; the UI uses `"current"` or `"parent"`) | `:4046-4056` | |
| TXF copy number | slot `tax-US/copy-number` (integer) | `:4058-4073` | Getter defaults to 1 when the slot is missing or 0. Setting 0 deletes the slot. |
| Last reconcile date | slot `reconcile-info/last-date` (integer, time64 seconds) | `:4555-4575` | |
| Last reconcile interval | slots `reconcile-info/last-interval/months` and `/days` (integers) | `:4581-4606` | The getter needs both. |
| Postponed reconcile | slots `reconcile-info/postpone/date` (integer) and `/balance` (numeric) | `:4612-4660` | `xaccAccountClearReconcilePostpone` deletes the whole `postpone` frame. |
| Auto interest transfer | slot `reconcile-info/auto-interest-transfer` = `"true"` | `:4162-4172` | |
| Last num | slot `last-num` | `:4666-4680` | |
| Opening balance | slot `equity-type = "opening-balance"`, EQUITY only | `:4131-4147` | post-2.0 |
| Colour, filter, sort | slots `color`, `filter`, `sort-order`, `sort-reversed` | `:2600-2622` | UI state; preserve |

Oracle XML for the account `Food` (EXPENSE, code 5100, hidden, notes, tax, TXF). Slots come out sorted by key, and the
`tax-US` sub-keys too:

```xml
  <act:commodity-scu>100</act:commodity-scu>
  <act:code>5100</act:code>
  <act:slots>
    <slot>
      <slot:key>hidden</slot:key>
      <slot:value type="string">true</slot:value>
    </slot>
    <slot>
      <slot:key>notes</slot:key>
      <slot:value type="string">some notes</slot:value>
    </slot>
    <slot>
      <slot:key>tax-US</slot:key>
      <slot:value type="frame">
        <slot>
          <slot:key>code</slot:key>
          <slot:value type="string">N123</slot:value>
        </slot>
        <slot>
          <slot:key>copy-number</slot:key>
          <slot:value type="integer">2</slot:value>
        </slot>
        <slot>
          <slot:key>payer-name-source</slot:key>
          <slot:value type="string">current</slot:value>
        </slot>
      </slot:value>
    </slot>
    <slot>
      <slot:key>tax-related</slot:key>
      <slot:value type="string">true</slot:value>
    </slot>
  </act:slots>
```

and `reconcile-info` (`last-date = 1700000000`, interval of 1 month and 0 days):

```xml
    <slot>
      <slot:key>reconcile-info</slot:key>
      <slot:value type="frame">
        <slot>
          <slot:key>last-date</slot:key>
          <slot:value type="integer">1700000000</slot:value>
        </slot>
        <slot>
          <slot:key>last-interval</slot:key>
          <slot:value type="frame">
            <slot>
              <slot:key>days</slot:key>
              <slot:value type="integer">0</slot:value>
            </slot>
            <slot>
              <slot:key>months</slot:key>
              <slot:value type="integer">1</slot:value>
            </slot>
```

### 2.5 Name separator and full name

- The separator is a **user preference, not book data**: GSettings `general/account-separator`, default `'colon'`
  (`5.17:gnucash/gschemas/org.gnucash.GnuCash.gschema.xml.in:18-22`). It is applied at start-up
  (`5.17:libgnucash/app-utils/gnc-ui-util.cpp:118-121`).
- `gnc_set_account_separator` falls back to `:` for an invalid or alphanumeric character (`Account.cpp:217-233`). The
  engine's static default before preferences load is `"."` (`:58`).
- **Full name** is the names from the top-level account down to the account, joined by the separator, with the root
  excluded (`gnc_account_get_all_parents` + `gnc_account_get_full_name`, `Account.cpp:3284-3320`).
- An account name containing the separator is not prevented by the engine. The GUI warns on file open
  (`gnc_account_list_name_violations`, `Account.cpp:273+`, used at `5.17:gnucash/gnome-utils/gnc-file.c:734`).
- red-ale should make the separator a per-connection or API setting, default `:`, and warn as GnuCash does.

### 2.6 Order of children

- **Storage order** is insertion order. `gnc_account_append_child` does `children.push_back`; removing a child erases
  it and keeps the rest in order (`Account.cpp:2824-2884`, `:2886-2920`). `gnc_account_get_children` returns this
  order (`:2948-2954`), and the XML writer walks it.
- **Display/report order** is `gnc_account_get_children_sorted` = `g_list_sort(children, xaccAccountOrder)`
  (`:2956-2962`). `xaccAccountOrder` (`:2363-2412`) compares, in order:
  1. `code` with `g_strcmp0` (bytewise; NULL sorts first);
  2. type, ranked by `typeorder` = BANK, STOCK, MUTUAL, CURRENCY, CASH, ASSET, RECEIVABLE, CREDIT, LIABILITY,
     PAYABLE, INCOME, EXPENSE, EQUITY, TRADING (`:2346-2354`);
  3. name, with `safe_utf8_collate` (locale collation);
  4. GUID.
- Newly created Imbalance and Orphan accounts are appended under root, so they come **last** among root's children
  (oracle: `Imbalance-USD` written after every other account).

### 2.7 Balances

`xaccAccountRecomputeBalance` (`Account.cpp:2263-2335`) walks the account's splits in `xaccSplitOrder` order (§5.6),
summing **amount** (quantity) with `gnc_numeric_add_fixed` (AUTO, NEVER), and stores running totals on each split.

| Balance | Splits included |
|---|---|
| balance (total, "future") | all |
| cleared | reconcile state ≠ `n`, i.e. `c`, `y`, `f`, `v` (`:2309-2312`) |
| reconciled | state `y` or `f` (`:2314-2319`) |
| noclosing | splits whose transaction is not a closing transaction (`book_closing` slot) (`:2321-2322`) |

- **Stock-split splits** (split type `stock-split`) are skipped in the running total when the balance is 0. Otherwise
  they adjust `adjusted_amount` on earlier splits (`:2286-2306`). This is a 5.17 addition and does not change the
  balance figure.
- **As-of balances** (`GetBalanceAsOfDate`, `:3506-3540`) take the running balance of the last split whose
  transaction has `date_posted < date` (strict).
  - "Present" = as of `gnc_time64_get_today_end()` (today 23:59:59 local) (`:3542-3547`).
  - "Reconciled as of date" uses the reconciled running total at that split. The cut-off is by **posted date**, not
    reconcile date (`:3533-3537`).
- The starting balances (`starting_*`) are only non-zero for partially loaded SQL books (`:3440-3452`). They are always
  0 for XML.

---

## 3. Renumber sub-accounts; delete with move

Both features are GUI logic. red-ale reimplements each as one atomic service operation.

### 3.1 Renumber sub-accounts (`5.17:gnucash/gnome-utils/dialog-account.c:2200-2380`)

- **Scope:** the **immediate children** of one account; grandchildren are not renumbered ("Renumber the immediate
  sub-accounts of '%s'?", `:2359`).
  - The action is enabled only when the book is writable and the account has at least one child
    (`5.17:gnucash/gnome/gnc-plugin-page-account-tree.cpp:249-255`, `:805-833`; `dialog-account.c:2341`).
- **Inputs and defaults:**
  - `prefix` (text): defaults to the **parent's own code**, possibly empty (`:2371`);
  - `interval` (1–10000): default 10 (`5.17:gnucash/gtkbuilder/dialog-account.glade:1989-1995`);
  - `digits` (1–100): default 3 (`dialog-account.glade:1059-1065`).
- **Digits floor:** `num_digits = (int)log10(n_children × interval) + 1`, i.e. the decimal digit count of the largest
  number. If `digits <= num_digits`, the spinner is reset to `num_digits` (`:2230-2240`).
  - So the effective width is `max(digits, digit_count(n × interval))`. red-ale should count the digits with integer
    arithmetic.
- **Order:** children in `gnc_account_get_children_sorted` order, i.e. `xaccAccountOrder` on the **current** codes,
  then type rank, then name (§2.6) (`:2300`).
- **Formula** for the i-th child, i = 1..n (`:2311-2320`):
  - `code_i = sprintf("%s-%0*d", prefix, width, interval*i)` if the prefix is non-empty;
  - `code_i = sprintf("%0*d", width, interval*i)` otherwise.
  - Example: prefix `1000`, interval 10, 3 children, digits 3 gives `1000-010`, `1000-020`, `1000-030`.
- **Effect:** `xaccAccountSetCode` on each child, and nothing else. It is a separate edit per child, skipped when the
  code is unchanged (`Account.cpp:2466-2482`). There is no uniqueness check against other codes.

### 3.2 Delete account (with move) (`5.17:gnucash/gnome/gnc-plugin-page-account-tree.cpp:1150-1745`)

**Refusals before the dialog:**

1. **Referring objects** (`qof_instance_get_referring_object_list` non-empty, `:1525-1534`). These are:
   - an invoice posted to the account (`5.17:libgnucash/engine/gncInvoice.c:253`);
   - an entry whose income or bill account is the account (`gncEntry.c:354-357`);
   - an employee whose credit-card account is the account (`gncEmployee.c:257`);
   - a tax-table entry (`gncTaxTable.c:319-329`).
   Scheduled-transaction templates and budgets are **not** checked.
2. **More than one sub-account:** "Move the subaccounts or delete them" (`:1541-1548`). With **exactly one** child, the
   dialog offers to move it.
3. **Fast path:** no splits and no children means the account is destroyed at once (`:1551-1555`).

**Choices.** "Move to" is the default for each (`dialog-account.glade:482-489`, `:576-583`, `:698-705`):

- this account's transactions: move to T, or delete;
- the sub-account: re-parent to P, or delete it;
- if the sub-account is deleted, its (and its descendants') transactions: move to S, or delete.

"Delete" is disabled when any affected transaction is read-only (has `trans-read-only`) (`:1427-1434`, `:1466-1472`).

**Allowed targets:**

- **Same account type** as the deleted account. This applies to all three selectors, a noted FIXME (`:1229`, `:1393`,
  `:1403`).
- Not the account itself and none of its descendants (`:1201-1210`, `:1232`, `:1292`). When the child is being
  re-parented, the child becomes a valid transaction target (`:1246-1250`).
- Placeholder and hidden accounts are hidden by default, except for the re-parent target (`:1286-1288`;
  `5.17:gnucash/gnome-utils/gnc-account-sel.c:213-214`, `:574-578`).
- **A commodity mismatch is only a warning**: "Do it anyway" (`:1300-1363`).
- red-ale: refuse placeholder targets for transaction moves (the register refuses postings to placeholders). Require
  an explicit `allow_commodity_mismatch` flag; see below for why this is dangerous.

**Order of operations** (`do_delete_account`, `:1674-1745`), all inside `xaccAccountBeginEdit(account)`:

1. Children:
   - re-parent: `gnc_account_append_child(P, child)`;
   - else move: `xaccAccountMoveAllSplits(desc, S)` for **every descendant**;
   - else: `xaccAccountDestroyAllTransactions(desc)` for every descendant.
2. Own transactions: `xaccAccountMoveAllSplits(account, T)`, or `xaccAccountDestroyAllTransactions(account)`.
3. Commit, then `xaccAccountDestroy(account)`, which recursively destroys the remaining children.

**Engine semantics:**

- **`xaccAccountMoveAllSplits(from, to)`** (`Account.cpp:2160-2230`): every affected transaction is opened. Then, per
  split, `xaccSplitSetAccount(to)` followed by `xaccSplitSetAmount(amount)`:
  - the amount is **numerically unchanged** but re-rounded to the target's SCU with ROUND_HALF_UP;
  - the value is untouched;
  - **there is no commodity conversion**, so a move across commodities silently reinterprets the quantity;
  - the split leaves its lot if the lot belongs to another account (`Split.cpp:1000-1002`);
  - each transaction commit runs the imbalance scrub.
- **`xaccAccountDestroyAllTransactions(acc)`** (`Account.cpp:1587-1603`): destroys **whole transactions**, other
  accounts' splits included. Read-only transactions are silently skipped (`Transaction.cpp:1363-1374`).
- **`xaccAccountDestroy`** → `xaccAccountCommitEdit` with the destroying flag set (`Account.cpp:1504-1585`):
  - destroys children recursively;
  - destroys the account's remaining splits one by one (`xaccSplitDestroy` is allowed even on read-only transactions
    while the account is being destroyed, `Split.cpp:1513-1518`), plus pending splits and lots.
  - Each affected transaction is committed. If it still has splits, the **imbalance scrub then adds an
    `Imbalance-<CUR>` split** for the lost side. If no splits remain, the transaction is destroyed. Oracle: deleting an
    account holding one side of a two-split transaction left `[Imbalance-USD +3.00, Checking −3.00]`.
  - The engine comment at `Account.cpp:1534-1540` notes the corner case of deleting Imbalance-XXX itself, where the
    splits are recreated in it.
- **red-ale `account.delete`:**
  - move-or-delete is mandatory input whenever the account has splits or children;
  - refuse when objects refer to the account;
  - make the whole operation one atomic mutation;
  - never leave the implicit Imbalance splits behind silently (report them, or refuse).

---

## 4. Accounting period

**In 5.17 the accounting period is a user preference, not book data.**

- **Storage:** GSettings schema `org.gnucash.GnuCash.window.pages.account-tree.summary`
  (`5.17:gnucash/gschemas/org.gnucash.GnuCash.window.pages.account.tree.gschema.xml.in:2-52`; keys at
  `5.17:libgnucash/core-utils/gnc-prefs.h:84-91`). It is edited on the Preferences "Accounting Period" tab.

  | Key | Type | Default |
  |---|---|---|
  | `start-choice-relative` / `start-choice-absolute` | bool | true / false |
  | `start-period` | int (enum below) | 5 |
  | `start-date` | time64 | 0 |
  | `end-choice-relative` / `end-choice-absolute` | bool | true / false |
  | `end-period` | int | 5 |
  | `end-date` | time64 | 0 |

- **`GncAccountingPeriod`** (`5.17:libgnucash/engine/gnc-accounting-period.h:57-73`; labels from
  `5.17:gnucash/gnome-utils/gnc-period-select.c:62-92`):

  | Value | Name | Start label / End label |
  |---|---|---|
  | 0 | TODAY | Today |
  | 1 | MONTH | Start / End of this month |
  | 2 | MONTH_PREV | Start / End of previous month |
  | 3 | QUARTER | Start / End of this quarter |
  | 4 | QUARTER_PREV | Start / End of previous quarter |
  | 5 | CYEAR | Start / End of this year *(default: calendar year to date)* |
  | 6 | CYEAR_PREV | Start / End of previous year |
  | 7 | FYEAR | Start / End of this accounting period |
  | 8 | FYEAR_PREV | Start / End of previous accounting period |

- **Computation** (`5.17:libgnucash/engine/gnc-accounting-period.c:60-321`; helpers at `gnc-date.cpp:1523-1716`):
  - **Absolute:** start = local start of day of `start-date`; end = local 23:59:59 of `end-date`.
  - **Relative:** take today's local date, adjust it with the rules below, then start = local 00:00:00 and end = local
    23:59:59 (`gnc-date.cpp:1488-1519`).
    - **Month:** 1st of the month to the last day. The previous month subtracts one month first.
    - **Quarter:** calendar quarters Jan, Apr, Jul and Oct. The previous quarter subtracts 3 months first.
    - **Year:** Jan 1 to Dec 31. The previous year subtracts 1.
    - **Fiscal year, given `fy_end` (day, month):** let T = fy_end in the current year.
      - If today > T: start = T + 1 day, end = T + 1 year.
      - Otherwise: start = T + 1 day − 1 year, end = T.
      - "Previous" is the same, minus 1 year.
  - **FYEAR / FYEAR_PREV without `fy_end`:** start becomes 0 (the epoch), and end becomes 0 and then −1
    (`gnc-accounting-period.c:95-96`, `:188-205`, `:282-300`).
- **Book data:** `fy_end` is read from the book slot **`fy_end`** (gdate), as the QofBook property `fy-end`
  (`qofbook.cpp:180-181`, `:219-220`, `:280-284`).
  - Nothing in 5.17 non-test code writes it, and the Preferences dialog never passes it to the period selector. So the
    two fiscal choices are **never offered** in 5.17 (`gnc-period-select.c:260-292`; `dialog-preferences.c:1412-1422`).
  - Preserve the slot if present.
- **red-ale:** the period choice is a red-ale setting or an explicit API parameter (`{relative: enum} | {absolute:
  date}` for each end). Fiscal-year end is a red-ale setting that may default from the `fy_end` slot.
- The consumers are the account tree "Period" columns, the summary bar, Remove old prices (§3 milestone item) and
  report dates. Only "Remove old prices" is in v0.1.

---

## 5. Transactions and splits

### 5.1 Aggregate invariants

- **Balanced** means the sum of split **values** is 0. It is computed with `gnc_numeric_add(..., AUTO, DENOM_EXACT)`
  (`xaccTransGetImbalanceValue`, `Transaction.cpp:990-1003`).
  - Without trading accounts, `xaccTransIsBalanced` is just that test (`:1078-1121`).
  - With trading accounts, non-trading and trading values must each sum to 0, and every per-commodity amount must too
    (`:1006-1075`; §9.6).
- **Value** is in the transaction currency (`common_currency`); **amount** (XML `quantity`) is in the account
  commodity. Neither is derived from the other on save.
- When the account commodity equals the transaction currency, `amount == value` is restored by the scrubs (§9.4).
- **The engine enforces balance at every commit** by adding an Imbalance split (§9.1), not by refusing the commit.
  red-ale's API should instead refuse unbalanced input (`transaction.post` is atomic and balanced). It needs the
  imbalance scrub only to match GnuCash when a caller asks for it, or when repairing a file.
- **Currency:** `xaccTransSetCurrency` re-values existing splits (`Transaction.cpp:1311-1338`):
  - if a split's account commodity is the new currency, value = amount;
  - otherwise the rate comes from the first split in the new currency (`find_new_rate`, amount/value with
    AUTO/NEVER);
  - the remaining splits are re-priced through `xaccSplitSetSharePrice`;
  - with no such split, each value is just re-converted to the new fraction.

### 5.2 Edit cycle (`xaccTransBeginEdit` / `xaccTransCommitEdit`)

**Begin** (`Transaction.cpp:1341-1360`):

- writes a `B` record to the `.log` journal (unless the book is read-only);
- clones the transaction for rollback.

**Commit** (`:1521-1588`), at edit level 0:

1. If no splits remain, the transaction is marked for destruction (`:1537-1538`).
2. If not destroying, data scrubbing is enabled, and the book is not shutting down: **`xaccTransScrubImbalance(trans,
   NULL, NULL)` runs in the engine** for every commit, not just from the GUI (`:1548-1560`). This adds an
   `Imbalance-<CUR>` split, or removes one (§9.1).
   - `xaccTransScrubGains` runs only when the environment variable `GNC_AUTO_SCRUB_LOTS` is set (`:1562-1563`).
   - The XML loader disables this scrubbing while parsing (`io-gncxml-v2.cpp:761`, `:814`).
3. **`date_entered` is set to `gnc_time(NULL)` (now, whole seconds) if it is still 0** (`:1570-1574`). This is the only
   place date-entered is set automatically.
4. `trans_cleanup_commit` (`:1460-1518`):
   - commits each dirty split, which moves it between account split lists and converts the amount to the account SCU
     (§1.5);
   - writes a `C` journal record;
   - **re-sorts the splits by sign (§5.5)**.

Destroying a transaction writes a `D` record (`:1397-1422`). It is refused while the transaction is read-only (has a
`trans-read-only` slot), unless the book is shutting down (`:1363-1374`).

### 5.3 Dates

**Date posted.**

- The UI and dialogs always use `xaccTransSetDatePostedSecsNormalized(t, time)`
  (`5.17:gnucash/register/ledger-core/split-register-model-save.c:75`; `5.17:gnucash/gnome-utils/dialog-transfer.cpp:1487`;
  reverse, stock split, and others). It:
  1. takes the **local** calendar date of `time` (`gnc_gdate_set_time64`, `gnc-date.cpp:1314-1321`);
  2. calls `xaccTransSetDatePostedGDate` (`Transaction.cpp:1906-1928`), which:
     - writes the transaction slot **`date-posted`** (type `gdate`), holding the calendar date;
     - sets `date_posted = gdate_to_time64(date)` = `gnc_dmy2time64_neutral` (`gnc-date.cpp:1323-1328`).
- **Neutral time is 10:59:00 UTC** on that date. It is shifted only for time zones with an offset below −10 h or above
  +13 h, to keep the same calendar date everywhere (`5.17:libgnucash/engine/gnc-datetime.cpp:248-268`).
- Oracle:

  ```xml
  <trn:date-posted>
    <ts:date>2024-01-15 10:59:00 +0000</ts:date>
  </trn:date-posted>
  ...
  <slot>
    <slot:key>date-posted</slot:key>
    <slot:value type="gdate">
      <gdate>2024-01-15</gdate>
    </slot:value>
  </slot>
  ```
- `xaccTransSetDatePostedSecs` (raw) does **not** write the gdate slot (`Transaction.cpp:1898-1904`). The XML reader uses
  it (`gnc-transaction-xml-v2.cpp:433`), so the slot appears only if it was in the file.
- `xaccTransGetDatePostedGDate` prefers the slot, else the UTC date of `date_posted` (`Transaction.cpp:2259-2289`).
- `xaccTransSetDate(day, mon, year)` replaces an invalid date with today (`:1937-1952`).

**Date entered.** Set at the first commit if 0 (above). The reverse command overwrites it with "now" (§6.3).

**Gains date.** Setting the posted date flags gains splits as date-dirty (`:1892-1904`). This is lot machinery, out of
scope here.

**Auto read-only threshold** (`xaccTransIsReadonlyByPostedDate`, `:2373-2408`): a transaction whose posted GDate is
before the threshold is read-only. Books that set it open read-only in v0.1.

### 5.4 Text fields and slots

| Field | Storage | Setter behaviour |
|---|---|---|
| Num | `<trn:num>` | `xaccTransSetNum`; NULL is ignored, `""` is allowed (`:1996-2005`). With the book option *num-field-source* (`split-action`), the UI shows the split action as "Num". v0.1 opens such books read-only. |
| Description | `<trn:description>` | NULL ignored (`:2016-2024`) |
| Notes | transaction slot `notes` (string) | `xaccTransSetNotes` → `set_kvp_string_path`; **`""` deletes the slot** (`:2042-2046`, `:1845-1853`) |
| Closing transaction | slot `book_closing` = int64 1; false deletes it (`:2049-2057`, `:178`) | |
| Read-only reason | slot `trans-read-only` (string) | `xaccTransSetReadOnly`/`ClearReadOnly` (`:1970-1984`) |
| Reversed by | slot `reversed-by` (guid) | §6.3 |
| Doc link | slot `assoc_uri` (post-2.0; preserve) | `:2027-2031` |
| Split memo | `<split:memo>` | `xaccSplitSetMemo`; NULL ignored (`Split.cpp:1766-1775`) |
| Split action | `<split:action>` | `xaccSplitSetAction`; NULL ignored (`Split.cpp:1785-1794`) |

A transaction's slot names are defined at `Transaction.cpp:174-188`: `notes`, `void-reason`, `void-time`,
`void-former-notes`, `book_closing`, `assoc_uri`, `trans-date-due`, `trans-txn-type`, `trans-read-only`,
`reversed-by`, `from-sched-xaction`.

### 5.5 Split order inside a transaction

- `xaccTransSortSplits` runs on every commit (`Transaction.cpp:1510`). It is a **stable** `g_list_sort` that puts
  splits with value ≥ 0 (debits, including zero) before splits with value < 0 (credits), keeping the relative order
  within each group (`:540-553`; reimplemented in 5.17 by commit 00cc11fb05, same result).
- New splits are appended in the order they are added (`Split.cpp:1908-1909`).
- Oracle: adding a credit split then a debit split gives the order [debit, credit] in the file.
- A voided transaction has all values 0, so its order is unchanged by the void.

### 5.6 Split order inside an account (decides the XML transaction order)

- `gnc_account_insert_split` appends, then `std::sort(split_cmp_less)` = `xaccSplitOrder < 0` (`Account.cpp:1925-1957`,
  `:1992-2004`).
- **The XML writer emits transactions** with `xaccAccountTreeForEachTransaction(root)` (`io-gncxml-v2.cpp:1166-1195`).
  This is a depth-first, post-order walk:
  - for each account, first recurse into its children in storage order (§2.6);
  - then go through the account's own sorted splits;
  - write each transaction the first time it is reached (`Account.cpp:5077-5134`).
- **`xaccSplitOrder(a, b)`** (`Split.cpp:1535-1597`):
  1. `xaccTransOrder(parent_a, parent_b)`, below. With split-action-as-num, the split actions are compared instead of
     the nums.
  2. memo, by `g_utf8_collate`;
  3. action, by `g_utf8_collate`;
  4. reconcile char, by byte value (`c` < `f` < `n` < `v` < `y`);
  5. amount (`gnc_numeric_compare`);
  6. value;
  7. date reconciled;
  8. split GUID (`qof_instance_guid_compare` = `memcmp` of the 16 bytes).
- **`xaccTransOrder_num_action`** (`Transaction.cpp:1793-1835`):
  1. `date_posted` (time64 seconds);
  2. non-closing before closing (`book_closing` slot);
  3. num by `order_by_int64_or_string` (`:1773-1791`): `strtoull` on both. If **both** parse to a non-zero number,
     compare numerically, then compare the rest of each string after the digits by `g_utf8_collate`. Otherwise compare
     the whole strings by `g_utf8_collate`. So `"10" > "9"`, `"0"` collates as a string, and `"-5"` collates as a string.
  4. `date_entered`;
  5. description, by `g_utf8_collate`;
  6. transaction GUID.
- **`g_utf8_collate` depends on the locale.** For identical byte-for-byte round-trips, red-ale must reproduce the
  writer's order. Ties after date, closing flag, num and date entered are rare, but where they happen the order of
  `<gnc:transaction>` elements depends on the collation of the GnuCash that wrote the file. For red-ale this matters
  only when it writes a transaction GnuCash has not already placed. See §11.

### 5.7 Reconcile states

| Char | Constant | Meaning |
|---|---|---|
| `n` | NREC | not reconciled or cleared |
| `c` | CREC | cleared |
| `y` | YREC | reconciled |
| `f` | FREC | frozen (never set by 5.17 code; preserve) |
| `v` | VREC | void |

(`Split.h:73-77`)

- `xaccSplitSetReconcile` rejects any other character ("Bad reconciled flag") (`Split.cpp:1818-1842`).
- It **does not touch** `date_reconciled`. `xaccSplitSetDateReconciledSecs` is separate (`:1844-1855`).
- Only the reconcile window sets the reconcile date (§8). Toggling the register's R cell changes the flag only
  (`split-register-model-save.c:204`).
- The register's R cell **cycles `n` ↔ `c`** only. A flag outside the order (`y`, `f`, `v`) goes back to the first
  flag, `n`, and the GUI asks for confirmation before un-reconciling (`5.17:gnucash/register/register-core/recncell.c:81-98`;
  `5.17:libgnucash/app-utils/gnc-ui-util.cpp:496-514`).
- Only the reconcile window produces `y`.
- A new split has `reconciled = 'n'`, `date_reconciled = 0`, memo `""`, action `""`, and amount and value 0
  (`Split.cpp:504-534`).

---

## 6. Void, unvoid, reverse

### 6.1 `xaccTransVoid(trans, reason)` (`Transaction.cpp:2502-2531`)

1. Return if `reason == NULL`. Refuse if the transaction is read-only (`trans-read-only` set): "Refusing to void a
   read-only transaction!".
2. In one edit:
   - if a `notes` slot exists, copy it to `void-former-notes`;
   - set `notes` to the translated "Voided transaction";
   - set `void-reason` = reason. **An empty reason writes no slot**, because `set_kvp_string_path` deletes on `""`;
   - set `void-time` to the current time as `"YYYY-MM-DD HH:MM:SS"`, **UTC, no zone suffix**
     (`gnc_time64_to_iso8601_buff`, `gnc-date.cpp:1212-1234`; `format_iso8601`, `gnc-datetime.cpp:631-636`);
   - for each split, `xaccSplitVoid` (`Split.cpp:2206-2218`):
     - slot `void-former-amount` = amount (numeric);
     - slot `void-former-value` = value;
     - `SetAmount(0)`, `SetValue(0)`, which are converted to the SCU or fraction, giving `0/100`;
     - `SetReconcile('v')`; `date_reconciled` is unchanged;
   - set `trans-read-only` = the translated "Transaction Voided".
3. **GUI preconditions** that the engine does not check (`5.17:gnucash/gnome/gnc-plugin-page-register.cpp:2453-2468`):
   - nothing happens if any split is already `v`;
   - refused if any split is `y` or `c` ("You cannot void a transaction with reconciled or cleared splits.");
   - refused if read-only.
   red-ale should apply these in `transaction.void`.

Oracle XML, reason "wrong payee", previous notes "orig notes" (slots sorted by key):

```xml
  <trn:slots>
    <slot><slot:key>date-posted</slot:key>...</slot>
    <slot>
      <slot:key>notes</slot:key>
      <slot:value type="string">Voided transaction</slot:value>
    </slot>
    <slot>
      <slot:key>trans-read-only</slot:key>
      <slot:value type="string">Transaction Voided</slot:value>
    </slot>
    <slot>
      <slot:key>void-former-notes</slot:key>
      <slot:value type="string">orig notes</slot:value>
    </slot>
    <slot>
      <slot:key>void-reason</slot:key>
      <slot:value type="string">wrong payee</slot:value>
    </slot>
    <slot>
      <slot:key>void-time</slot:key>
      <slot:value type="string">2026-10-25 13:05:27</slot:value>
    </slot>
  </trn:slots>
  <trn:splits>
    <trn:split>
      <split:id type="guid">…</split:id>
      <split:reconciled-state>v</split:reconciled-state>
      <split:value>0/100</split:value>
      <split:quantity>0/100</split:quantity>
      <split:account type="guid">…</split:account>
      <split:slots>
        <slot>
          <slot:key>void-former-amount</slot:key>
          <slot:value type="numeric">700/100</slot:value>
        </slot>
        <slot>
          <slot:key>void-former-value</slot:key>
          <slot:value type="numeric">700/100</slot:value>
        </slot>
      </split:slots>
    </trn:split>
```

With `Void("")`, the `void-reason` slot is absent and everything else is the same (oracle).

### 6.2 Void status follows the void timestamp (5.17 rule)

- `xaccTransGetVoidStatus(t)` is `xaccTransGetVoidTime(t) != INT64_MAX`. `GetVoidTime` parses the `void-time` slot
  with `gnc_iso8601_to_time64_gmt`, which returns `INT64_MAX` when the slot is missing or does not parse
  (`Transaction.cpp:2533-2551`; `gnc-date.cpp:1189-1207`).
- Before this change (commit 6e33c23815, "Bug 799791 - Unvoid is not available", first in 5.17), status was
  "`void-reason` exists and is not empty". A transaction voided with an empty reason was therefore never "void" and
  could not be unvoided.
- **red-ale rule:** a transaction is void if and only if its `void-time` slot holds a parseable timestamp. A
  `void-reason` without `void-time` is not void.
- The register's own Void/Unvoid menu state uses a different test: "any split has state `v`"
  (`xaccTransHasSplitsInState(trans, VREC)`, `gnc-plugin-page-register.cpp:860`, `:2453`, `:2511`). Find also uses
  Reconcile = Voided (§7). The two tests can disagree on hand-edited files. Expose the engine's definition, and treat
  the per-split `v` state as data.

### 6.3 `xaccTransUnvoid` (`Transaction.cpp:2553-2573`)

- Does nothing unless `GetVoidStatus` is true.
- `notes` ← `void-former-notes`. **If there were no former notes, the `notes` slot is deleted** (oracle).
- Deletes `void-former-notes`, `void-reason` and `void-time`.
- For each split, `xaccSplitUnvoid` (`Split.cpp:2220-2231`):
  - amount ← `void-former-amount`, or 0 if missing;
  - value ← `void-former-value`;
  - reconcile ← `n`;
  - deletes both slots.
- Clears `trans-read-only`.
- **Unvoid always resets the reconcile state to `n`**, whatever it was before the void (the GUI forbids voiding `c`/`y`
  anyway).

### 6.4 `xaccTransReverse(orig)` (`Transaction.cpp:2575-2610`)

1. Begin, mark dirty and commit `orig`, which runs the imbalance and trading scrub on it.
2. `trans = xaccTransClone(orig)` (`:606-660`; `Split.cpp:597-645`). The clone is a **new GUID** with the same
   `date_posted`, `date_entered`, num, description and currency.
   - It copies **all transaction slots**: notes, `date-posted` gdate, and so on.
   - Each split is copied with a new GUID, the same account, memo, action, reconcile state, `date_reconciled`, amount
     and value, and **all split slots except `online_id`**.
   - It is also **added to the same lot** as the original split (`Split.cpp:621-625`).
3. For each split: `SetAmount(-amount)`, `SetValue(-value)`, `SetReconcile('n')`. **`date_reconciled` is not reset**
   (oracle: the reversing split kept `<split:reconcile-date>` with state `n`).
4. `orig` gets the slot `reversed-by` = the new transaction's GUID (type `guid`).
5. Clears `trans-read-only` on the new transaction, then commits it. The commit re-sorts the splits by sign.

**GUI wrapper** (`gnc-plugin-page-register.cpp:2540-2600`; older path `5.17:gnucash/gnome/gnc-split-reg.c:1080-1105`):

- If `xaccTransGetReversedBy(trans)` already resolves, it does not reverse again ("A reversing entry has already been
  created for this transaction."; it offers to jump to it).
- Otherwise it asks for a date (default today) and calls `xaccTransReverse`. Then:
  - `xaccTransSetDatePostedSecsNormalized(new, date)`, which rewrites `date_posted` and the gdate slot;
  - `xaccTransSetDateEnteredSecs(new, now)`.
- **red-ale `transaction.reverse(guid, date)`** = engine reverse + those two date sets, and refuses when `reversed-by`
  already points at an existing transaction.
- Open point: should the reverse of a voided or read-only transaction be refused? The engine allows it and the GUI does
  not block it.

Oracle, reverse of t1 (nums and notes copied; memo and action kept; reconcile date kept on a split that was `y`):

```xml
<trn:num>100</trn:num> … <slot:key>notes</slot:key><slot:value type="string">hello</slot:value> …
    <trn:split>
      <split:id type="guid">fd2a…</split:id>
      <split:memo>memo</split:memo>
      <split:action>act</split:action>
      <split:reconciled-state>n</split:reconciled-state>
      <split:reconcile-date>
        <ts:date>2024-01-11 19:06:40 +0000</ts:date>
      </split:reconcile-date>
      <split:value>1235/100</split:value>
```

---

## 7. Find transactions

### 7.1 Criteria offered (`5.17:gnucash/gnome/dialog-find-transactions.c:105-158`)

The search object is the **split**. Rows are in UI order; parameter paths are relative to Split.

| UI label | Param path | Type → operators |
|---|---|---|
| Description | `trans.desc` | string |
| Notes | `trans.notes` | string |
| Memo | `memo` | string |
| Description, Notes, or Memo | any of the three (OR) (`:139-151`; `dialog-search.c:420-433`) | string |
| Number ("Transaction Number" if split-action-as-num) | `trans.num` | string |
| Action ("Number/Action" if split-action-as-num) | `action` | string |
| Reconciled Date | `date-reconciled` | date |
| Date Posted | `trans.date-posted` | date |
| Value | `value` | debit/credit numeric |
| Shares | `amount` | numeric |
| Share Price | `share-price` = value/amount, ROUND_HALF_UP, AUTO; 0 if amount is 0 (`Split.cpp:1994-2026`) | numeric |
| Reconcile | `reconcile-flag` | char set |
| Closing Entries | `trans.trans-is-closing?` (`book_closing` ≠ 0) | boolean |
| Balanced | `trans.trans-balanced?` (`xaccTransIsBalanced`) | boolean |
| Account | `account.guid` | account set: any or none |
| All Accounts | `trans.split-list.account-guid` | account set: all |

Not offered, though registered as query parameters: date entered, void status, reason and time, due date and doc link
(`Transaction.cpp:2891-2945`). Voided splits are found with Reconcile = `v`.

### 7.2 Operators (`5.17:gnucash/gnome-search/*.c`)

- **String** (`search-string.c:235-242`):
  - contains (default), equals, matches regex, does not match regex;
  - "Match case" is off by default, so matching is case-insensitive;
  - empty text is rejected;
  - a regex is compiled with `REG_EXTENDED | (REG_ICASE when case-insensitive)`.
  - The engine also has not-contains and not-equal (`search-string.h:32-37`).
- **Numeric** (`search-numeric.c:202-240`): `<`, `<=`, `==` (default), `!=`, `>`, `>=`; the default value is 0.
  - The debit/credit variant adds a sign selector: **any** (default), debits, credits.
- **Date** (`search-date.c:187-194`): before (default), on or before, on, not on, after, on or after. The default date
  is today.
- **Account:** matches any (default) or none of a non-empty set; "All Accounts" offers only "matches all".
- **Reconcile:** is (default) or is not, over a set of `{n, c, y, f, v}`. The default set is `{n}`.
- **Boolean:** equals a checkbox, default true.

### 7.3 Predicate semantics (`5.17:libgnucash/engine/qofquerycore.cpp`)

- **String** (`:126-191`):
  - NULL is treated as `""`;
  - case-insensitive *contains* applies UTF-8 casefold and NFC to both sides, then a substring test
    (`qofutil.cpp:54-76`);
  - case-insensitive *equals* casefolds, then `g_utf8_collate == 0` (`qofutil.cpp:82-97`);
  - case-sensitive matching is bytewise;
  - a regex matches anywhere in the value (not anchored).
- **Numeric** (`:484-541`):
  - The sign filter comes first: **credit keeps value ≤ 0** and **debit keeps value ≥ 0**, so 0 passes both. The header
    comment in `qofquerycore.h:94-97` says the opposite; follow the code.
  - Ordering operators compare **`|value|`** with the entered amount.
  - `==` and `!=` use a tolerance: `| |v| − |x| | < 1/10000`.
- **Date** (`:352-400`):
  - "on" and "not on" use DAY matching: both sides are moved to local noon with `time64CanonicalDayTime`
    (`gnc-date.cpp:404-410`).
  - The other operators compare seconds. The entered date becomes:
    - for `<=` and `>`: local 23:59:59;
    - for `<` and `>=`: neutral time, 10:59 UTC (`search-date.c:167-168`, `:261-264`;
      `5.17:gnucash/gnome-utils/gnc-date-edit.c:1067-1069`, `:1117-1127`).
- **GUID:**
  - ANY: the split's account is in the set;
  - NONE: it is not;
  - ALL: **every** chosen account appears among the transaction's splits (`:616-724`).
- **Char:** ANY means the flag is in the set; NONE means its negation (`:1157-1180`).
- **Balanced:** `xaccTransIsBalanced` (§5.1).

### 7.4 Combining criteria (`5.17:gnucash/gnome-search/dialog-search.c`)

- **"All criteria are met"** (default) ANDs the criteria; **"any criteria are met"** ORs them (`:449-485`).
- **Search modes** (`:494-517`):

  | Mode | Result |
  |---|---|
  | New search | `base AND crit` |
  | Refine current search | `current AND crit` |
  | Add results to current search | `current OR crit` |
  | Delete results from current search | `current AND NOT crit` |

- **Base query:**
  - from the menu: all splits of the book (`dialog-find-transactions.c:192-196`);
  - from a register: that register's query, which carries its account and filter terms (`:187-191`).
- The results register always excludes splits in **scheduled-transaction template accounts** (descendants of the
  template root) (`5.17:gnucash/register/ledger-core/gnc-ledger-display.c:176-200`, `:985-987`).
- **With no criteria**, everything matches.
- The "Search only active data" checkbox does nothing for splits.
- **Pitfall:** a negated operator on "Description, Notes, or Memo" gives `desc∌x OR notes∌x OR memo∌x`.

### 7.5 Results

- The result is a list of **splits**, shown journal-style with each transaction once.
- The default sort is `xaccSplitOrder` (§5.6) (`qofquery.cpp:168-170`).
- There is no limit by default. A search started from a register inherits that register's `max_results`; the
  `max-transactions` preference defaults to 0 (unlimited), and when a limit applies the **last** N are kept
  (`qofquery.cpp:799-822`).
- **red-ale `transaction.find`:** a tree of AND/OR/NOT terms over these fields and operators, plus the hidden extras
  (date entered, void, not-contains). It returns split GUIDs with their transaction GUIDs, in `xaccSplitOrder`, with an
  optional limit.

---

## 8. Reconcile against a statement

GUI: `5.17:gnucash/gnome/window-reconcile.cpp` and `5.17:gnucash/gnome/reconcile-view.c`. Signs are raw (debit
positive) internally. The "reverse balanced accounts" sign flip is a display **preference**
(`gnc-ui-util.cpp:126-172`; default: credit accounts), not book data.

### 8.1 Start: defaults

**Default statement date** (`gnc_get_reconcile_info`, `:1635-1698`):

1. Seed with today at 23:59:59 local.
2. If the slot `reconcile-info/last-date` exists, and the preference `always-reconcile-to-today` is false (the
   default):
   - start from last-date and add `last-interval`, defaulting to 1 month and 0 days when absent;
   - if months is non-zero, add the months, and if last-date was the last day of its month, snap to the new month's
     last day (1/31 → 2/28 → 3/31);
   - otherwise add the days;
   - take the end of that day, clamped to no later than today's end.
3. If the slot `reconcile-info/postpone/date` exists, it overrides the result.

**Default ending balance:**

- `reconcile-info/postpone/balance` if present;
- else the account's full balance **as of the statement date** (all splits posted before it, not only reconciled
  ones), including sub-accounts if chosen (`gnc-ui-balances.cpp:186-242`).

**Include sub-accounts:**

- defaults to the slot `reconcile-info/include-children` (int64 used as a bool, `Account.cpp:4868-4890`);
- forced off when any descendant has a different commodity;
- saved when the reconciliation starts.

**Starting balance** (display) = `xaccAccountGetReconciledBalance`: the all-time sum of `y` and `f` splits, whatever
their date (`Account.cpp:3469-3474`). This is the account alone, or summed over sub-accounts.

**On OK** (`:929-940`):

- the statement date becomes the chosen date at local 23:59:59;
- **`last-interval` is saved at this point**, if last-date exists (`gnc_save_reconcile_interval`, `:693-740`):
  - `days = (stmt − last_date) / 86400`, integer division;
  - if `days == 28` and the stored interval is 1 month or absent, store 1 month and 0 days;
  - else if `days > 28`, store months = the local calendar-month difference, and days = 0;
  - else store days;
  - nothing is stored if either value is negative.

The interest-transfer button (shown automatically when `reconcile-info/auto-interest-transfer` is set) and the
credit-card payment prompt after Finish are GUI conveniences. They are ordinary transfers in red-ale.

### 8.2 Splits listed and the difference

- **Listed:** splits of the account (and descendants if included) with state **`n` or `c`**, with **no date filter**
  (`reconcile-view.c:328-415`).
- **Pre-ticked:** `c` splits posted on or before the statement day's end, when the preference `check-cleared` is true
  (the default).
- **Difference:**
  - `reconciled = starting_reconciled + Σ(amount of ticked splits)`;
  - `difference = ending − reconciled` (`window-reconcile.cpp:328-391`).
- **Finish is enabled only when the difference is exactly 0** (`:376-378`).

### 8.3 Finish (`:2419-2475`)

1. Begin-edit the account (and descendants).
2. For each **ticked** split:
   - `xaccSplitSetReconcile('y')`;
   - `xaccSplitSetDateReconciledSecs(statement_date)`, i.e. the statement date at 23:59:59 local, **not now**
     (`reconcile-view.c:931-937`).
3. Unticked splits keep their state.
4. Commit.
5. `xaccAccountClearReconcilePostpone`: delete the whole `reconcile-info/postpone` frame.
6. `xaccAccountSetReconcileLastDate(statement_date)`: write `reconcile-info/last-date`.

### 8.4 Postpone (`:2482-2510`)

1. For each listed split that is posted on or before the statement date, **or** ticked:
   - ticked → `c`;
   - unticked → **`n`**, which un-clears cleared splits left unticked (`reconcile-view.c:961-998`).
2. Write `reconcile-info/postpone/date` (int64) and `reconcile-info/postpone/balance` (numeric, raw sign).
3. `last-date` is not touched.

**red-ale:**

- `account.reconcile(account, statement_date, ending_balance, split_guids, include_children)` is atomic. It refuses
  when the difference ≠ 0 unless `force`, matching the "finish anyway" path at `:2428-2434`.
- `account.reconcile_postpone(…)` is a separate operation.
- The "statement date at 23:59:59 local" rule needs a time zone parameter (§11).

---

## 9. Scrubs and Check & Repair

### 9.1 `xaccTransScrubImbalance(trans, root, account)` (`Scrub.cpp:845-899`)

1. `xaccTransScrubSplits(trans)` (§9.4). This also fixes orphans.
2. If `xaccTransIsBalanced`, stop.
3. **Without trading accounts:** if the imbalance value is not 0, call `add_balance_split(trans, imbalance, root,
   account)` (`:582-636`, `:638-651`):
   1. **Choose the balancing account** (`get_balance_split`, `:445-491`):
      - If `account` is given and its commodity equals the transaction currency, use it.
      - Otherwise use `xaccScrubUtilityGetOrMakeAccount(root, currency, "Imbalance-" + mnemonic, BANK,
        placeholder=FALSE, checkname=TRUE)` (`:1507-1540`). The name is `_("Imbalance")`, so it is translated.
      - **Lookup** (`gnc_account_lookup_by_type_and_commodity`, `Account.cpp:3176-3210`): search root's direct children
        for type BANK, commodity equivalent to the currency, and name equal. If none matches, recurse into each child.
        So an `Imbalance-USD` moved anywhere in the tree is still found, with the shallowest match first.
      - If several match: when no currency is given, prefer the one in the root currency; otherwise take the first
        with an exactly matching name.
      - **Creation** (`construct_account`, `Scrub.cpp:1457-1475`): a new account with that name and commodity
        (currency, else the root currency), type BANK, placeholder as given, **appended to root** (last child).
   2. Find an existing split of the transaction in that account (`xaccTransFindSplitByAccount`), or create one:
      `xaccMallocSplit`, `SetParent`, `SetAccount`.
   3. `new_value = old_value − imbalance`, rounded to `fraction(currency)` with ROUND_HALF_UP.
   4. **Zero-value removal:** if `new_value == 0` **and** the split's memo is empty **and** its action is empty,
      **destroy the split** and stop (`:607-619`).
      - This came from commit 048dfa1232, "Bug 343711", first in 5.13. It only fires when the transaction is
        unbalanced and the existing Imbalance split exactly absorbs the imbalance.
      - A balanced transaction that holds a zero Imbalance split is left alone.
      - A split with a memo or action is kept at value 0.
   5. Otherwise `SetValue(new_value)`. If the account commodity equals the currency, also `SetAmount(new_value)`.
      Then `xaccSplitScrub(split)` and commit.
4. **With trading accounts:** clear the trading splits, rebalance the value, build `Trading/<namespace>/<mnemonic>`
   splits, and add more splits for zero-amount gain splits (`:870-898`). See §9.6.

Oracle:

- A one-split transaction (Checking −5.00) committed to `[Imbalance-USD +500/100, Checking −500/100]`. The account was
  created as BANK, USD, under the root, not placeholder.
- Adding Food +5.00 and committing **removed** the Imbalance split.
- Deleting an account that held one side of a two-split transaction left `[Imbalance-USD +3.00, Checking −3.00]`, for
  the reason given under §3.2.

### 9.2 Orphans

- `TransScrubOrphansFast(trans, root)` (`Scrub.cpp:110-136`): every split with **no account** is assigned to
  `xaccScrubUtilityGetOrMakeAccount(root, trans currency, "Orphan-" + mnemonic, BANK, FALSE, TRUE)`.
- **`xaccTransScrubOrphans(trans)`** (`:178-210`): the root is taken from the first split that has an account, else
  from the book.
- **`xaccAccountScrubOrphans` / `xaccAccountTreeScrubOrphans`** (`:138-176`) apply it to every transaction touching the
  account (and its descendants), reporting progress every 10 transactions.
- An XML split always has an account. A missing account GUID makes the reader create a placeholder account object
  (`gnc-transaction-xml-v2.cpp:280-292`), so orphans are rare in XML books.

### 9.3 `xaccTransScrubCurrency(trans)` (`Scrub.cpp:1120-1235`)

1. Run `xaccTransScrubOrphans`.
2. If the transaction currency is set and is a currency (namespace `CURRENCY`), stop.
3. Otherwise find a common currency (`xaccTransFindCommonCurrency`, `:1055-1118`):
   - For each non-trading split, score the account commodity +3 if it is a currency. If not, score +1 for
     `gnc_account_get_currency_or_parent`.
   - Pick the highest score; ties go to the first in list order, because the sort is a stable ascending sort and the
     head is taken. **Uncertain:** see §11.
   - Fall back to the old `DxaccAccountGetCurrency` logic.
4. If a currency is found, `xaccTransSetCurrency`, and set `amount = value` on every split whose account commodity *is*
   that currency but where `amount != value`.
5. If none is found, use the first split's account commodity.

The XML reader runs it for every transaction during load, with `xaccTransScrubPostedDate`
(`io-gncxml-v2.cpp:282-283`).

### 9.4 `xaccSplitScrub` / `xaccTransScrubSplits` (`Scrub.cpp:236-430`)

For each split:

1. If it has no account, run the orphan scrub.
2. If value or amount is an error value, set it to 0.
3. If the account has no commodity, run `xaccAccountScrubCommodity`.
4. **Only if the account commodity is equivalent to the transaction currency:** `scu = min(account SCU, currency
   fraction)`. If `gnc_numeric_same(amount, value, scu, ROUND_HALF_UP)` is false, set `amount = value`.
   - Rounded to the same SCU, mismatches smaller than one unit are tolerated and left as stored.

`xaccTransScrubSplits` does a dry run first and opens an edit only if a split would change (`:394-420`).

### 9.5 Check & Repair (GUI) and automatic scrubs

The account tree page actions (`5.17:gnucash/gnome/gnc-plugin-page-account-tree.cpp:1940-2027`) run, in order, with a
progress callback and cancellable with Esc (`gnc_set_abort_scrub`):

| Action | Steps |
|---|---|
| Check & Repair Account | `xaccAccountScrubOrphans(acc)` → `xaccAccountScrubImbalance(acc)` → `xaccAccountScrubLots(acc)` **only if env `GNC_AUTO_SCRUB_LOTS`** → `gncScrubBusinessAccount(acc)` |
| … Subaccounts | the same with the `Tree` variants on the account |
| … All | the same `Tree` variants on the root |

- **`AccountScrubImbalance`** (`Scrub.cpp:353-391`): for each transaction touching the account(s), run
  `TransScrubOrphansFast` → `xaccTransScrubCurrency` → `xaccTransScrubImbalance(trans, root, NULL)`.
  - The set of transactions is an `std::unordered_set`, so **the processing order is unspecified** (`:96-106`). This
    only matters for which transaction creates the Imbalance account first.
- **`gncScrubBusinessAccount[Tree]`** (`5.17:libgnucash/engine/ScrubBusiness.c:736-762`): A/R and A/P accounts only.
  It repairs business lots and splits. This is covered by the business note.
- **Register "Check & Repair"** (`gnc-plugin-page-register.cpp:3542-3680`): per split, `xaccTransScrubOrphans` →
  `xaccTransScrubImbalance(trans, root, NULL)`. If the split is in a lot of an A/R or A/P account, it then runs
  `gncScrubBusinessLot` and `gncScrubBusinessSplit`.

**Automatic scrubs on XML load** (`io-gncxml-v2.cpp`), with commit scrubbing disabled during the parse:

- per account: `xaccAccountScrubCommodity` and `xaccAccountScrubKvp` (`:219-220`). The KVP scrub deletes blank
  `notes`, deletes `placeholder = "false"`, and deletes an empty `hbci` frame (`Scrub.cpp:1383-1410`);
- per transaction: `xaccTransScrubCurrency` and `xaccTransScrubPostedDate` (`:282-283`);
- after the parse and `qof_book_mark_session_saved`:
  - business scrub hooks;
  - `xaccAccountTreeScrubQuoteSources` (`:827`);
  - `xaccAccountTreeScrubCommodities` (`:830`): ScrubCurrency on every transaction, plus ScrubCommodity and removal of
    the `old-currency`, `old-security`, `old-currency-scu` and `old-security-scu` slots on every account
    (`Scrub.cpp:1273-1312`);
  - `xaccAccountTreeScrubSplits` (`:833`).
- The **GUI** additionally runs `gnc_maybe_scrub_all_budget_signs` and `xaccAccountScrubColorNotSet` after opening.
  The latter writes the book slot `remove-color-not-set-slots` (`5.17:gnucash/gnome-utils/gnc-file.c:754-790`).
  Headless sessions do not.
- red-ale's file adapter should apply the same load-time normalisations, or knowingly not apply them, so that its
  output matches what GnuCash would write (§11).

### 9.6 Trading accounts

**The option.** "Use Trading Accounts" is the Book slot `options/Accounts/Use Trading Accounts`, a string. It is on
when the value is exactly `"t"` (`qof_book_use_trading_accounts`, `5.17:libgnucash/engine/qofbook.cpp:919-928`), and
`xaccTransUseTradingAccounts` asks only that (`5.17:libgnucash/engine/Transaction.cpp:969-972`).

- The Book Options dialog registers it as a boolean, default off (`5.17:libgnucash/engine/gnc-optiondb.cpp:1216-1219`).
  Saving the dialog writes each changed boolean as `"t"` or `"f"` (`:404-410`, `:465-517`); turning it off writes
  `"f"`, it does not delete the slot. `qof_book_set_option` only sets the path and marks the Book dirty
  (`qofbook.cpp:1379-1389`).
- Changing the option touches no transaction. Existing transactions get trading splits only when they are next
  committed.

**What "balanced" means with trading accounts** (`xaccTransIsBalanced`, `Transaction.cpp:1078-1121`):

1. the values of the non-trading splits sum to 0, **and** the values of the trading splits (splits in TRADING
   accounts) sum to 0 — one cannot balance the other;
2. **and** `xaccTransGetImbalance` (`:1006-1076`) is empty. It walks the splits in order, adding values. As soon as
   a split's account commodity is not the transaction currency, or its amount ≠ its value, it starts a list with the
   transaction currency holding the values summed so far, and from then on adds every split's **amount** under its
   account's commodity. Entries are merged by `gnc_commodity_equiv` and new ones are **prepended**
   (`gnc_monetary_list_add_monetary`, `5.17:libgnucash/engine/gnc-commodity.cpp:2441-2464`), then zero entries are
   deleted (`:2469-2481`). For a valid transaction this is: the sum of amounts per account commodity, newest
   commodity first, the transaction currency last.

Per-commodity **values** are not checked: a rate change that leaves amounts and both value sums at zero leaves the
old trading splits alone.

**The tree.** `get_trading_split(trans, root, commodity)` (`5.17:libgnucash/engine/Scrub.cpp:518-579`) finds or
makes, with `xaccScrubUtilityGetOrMakeAccount` (`:1507-1540`):

| Level | Parent | Lookup (type TRADING) | Created as |
|---|---|---|---|
| `Trading` (`_("Trading")`) | root | any commodity, **name not checked** | placeholder, commodity = root currency |
| namespace, e.g. `CURRENCY`, `NASDAQ` | `Trading` | any commodity, name = namespace | placeholder, commodity = root currency |
| mnemonic, e.g. `EUR`, `ACME` | namespace | commodity equivalent, **name not checked** | not placeholder, commodity = the commodity |

- **Lookup** is `gnc_account_lookup_by_type_and_commodity` (`5.17:libgnucash/engine/Account.cpp:3176-3210`): matching
  direct children (prepended, so last first); only if none, each child's matches, last child first. With several
  matches: with no commodity asked for, the first whose commodity is the root currency; then the first whose name
  equals the name asked for; then the first.
- **Creation** (`construct_account`, `Scrub.cpp:1457-1475`): appended as the parent's last child, with the given
  commodity or else the **root currency**, type TRADING and the placeholder flag. `xaccAccountSetCommodity` sets the
  SCU to the commodity's fraction (`Account.cpp:2683-2684`).
- **Root currency** (`find_root_currency`, `Scrub.cpp:493-516`): the root account's commodity, else the commodity of
  the first top-level INCOME account that has one, else none (the account is created with no commodity). It reads
  the root of **the current session's** Book (`gnc_get_current_session`), which in the GUI is the open Book. A
  script that never sets the current session gets an empty Book and so trading parents with no commodity (oracle).
- The names are translated in GnuCash; namespace and mnemonic names are not.

**The commit-time scrub** (`xaccTransScrubImbalance`, `Scrub.cpp:844-895`), run by every `xaccTransCommitEdit`
(`Transaction.cpp:1521-1574`) — so by every setter, reconcile and lot change included, but never while the XML loader
parses (`io-gncxml-v2.cpp:761`, `:814`):

1. `xaccTransScrubSplits` (§9.4).
2. If `xaccTransIsBalanced`, stop. **Trading splits of a balanced transaction are never touched.**
3. `xaccTransClearTradingSplits` (`Scrub.cpp:684-712`): destroy every split in a TRADING account.
4. If the values now sum to non-zero, `add_balance_split` (§9.1) into `Imbalance-<CUR>`.
5. `gnc_transaction_balance_trading` (`:714-782`): for each entry `(commodity, imbalance)` of `xaccTransGetImbalance`,
   in list order:
   - take the transaction's split in the commodity's trading account, or append a new one (value and amount 0);
   - amount = `old amount − imbalance`, rounded to the **commodity's fraction** half up, then to the account SCU by
     `xaccSplitSetAmount`;
   - if the commodity is the transaction currency, value = that amount;
   - otherwise value = `old value − (sum of the values of all splits whose account holds the commodity)`, rounded to
     the currency fraction half up (`gnc_transaction_get_commodity_imbalance`, `:653-673`). The sum includes the
     trading split's own value, so the commodity's values come to 0;
   - `xaccSplitScrub` on it.
6. If the values still do not sum to 0, `gnc_transaction_balance_trading_more_splits` (`:784-836`): for each split
   present before this step with value ≠ 0 and amount = 0 (a realized-gain split), the trading split of its account's
   commodity gets value `old − split value` (currency fraction, half up), amount unchanged. Otherwise GnuCash logs
   "Balancing currencies unbalanced value".
7. The commit then removes destroyed splits and stably sorts the rest, debits first (§5.5), so new trading splits
   end up after the existing splits of the same sign.

Oracle (GnuCash 5.17, current session set, a USD book whose root has no commodity and whose top-level Income is USD):

| Posted | Committed as |
|---|---|
| Euro +110.00 USD value / +100.00 EUR, Checking −110.00 | Euro +110/+100, `Trading:CURRENCY:USD` +110/+110, Checking −110, `Trading:CURRENCY:EUR` −110/−100 |
| ACME +500.00 / +10.000 sh, Checking −500.00 | ACME, `Trading:CURRENCY:USD` +500/+500, Checking, `Trading:NASDAQ:ACME` −500/−10.000 |
| ACME +25.00 / 0 sh, Gains −25.00 | ACME, `Trading:CURRENCY:USD` +25/+25, Gains, `Trading:NASDAQ:ACME` −25/0 |

`Trading`, `Trading:CURRENCY` and `Trading:NASDAQ` came out USD, SCU 100, placeholder; the commodity accounts in
their commodity at its fraction, not placeholder.

**Consequences for every operation.** Every operation that creates or changes a transaction commits it, so with the
option on each one ends with this scrub: post, edit, transfer, reverse (which first commits the original), void and
unvoid (balanced either way, so no change), stock splits (the share split gets a trading split of −shares and value
0), capital gains transactions (step 6), business posting and payments in another currency, reconcile and lot
changes, delete with move. Deleting a transaction commits nothing.

**Elsewhere:** the transfer dialog treats the amount as the split's own amount when trading accounts are on
(`5.17:gnucash/gnome-utils/dialog-transfer.cpp:2375-2386`); the account dialog offers any commodity for a TRADING
account (`5.17:gnucash/gnome-utils/dialog-account.c:224-241`) and no opening balance (`:1340-1344`);
`xaccTransFindCommonCurrency` ignores trading splits (§9.3). TRADING is in `xaccAccountTypesValid`
(`Account.cpp:4459-4466`) and can only be a child of TRADING or ROOT (§2.2).

---

## 10. Exchange rates on transfer

### 10.1 Transfer dialog (`5.17:gnucash/gnome-utils/dialog-transfer.cpp`)

**When the rate section is active** (`:417`): the from and to commodities differ, where
`from = gnc_account_or_default_currency(from_acct)` and `to = commodity(to_acct)` (`:495-530`). OK is refused when the
from commodity is not a currency: "You can't transfer from a non-currency account" (`:1431`).

**Inputs.** There are three: amount (in the from commodity), rate (1 from = rate × to), and to amount. Only one of
rate and to amount is editable at a time.

- **Rate → to amount:**
  - `to_amount = gnc_numeric_mul(amount, rate, scu, GNC_HOW_RND_ROUND_HALF_UP)` (`:1006-1007`);
  - `scu = xaccAccountGetCommoditySCU(to_acct)`, falling back to the from account's SCU, then
    `fraction(to commodity)` (`:993-999`);
  - a zero or invalid rate gives a to amount of 0;
  - sets the price source to `"user:price"` and type `"transaction"` (`:984`, `:1026`).
- **To amount → rate:**
  - `rate = gnc_numeric_div(to_amount, amount, AUTO, DENOM_REDUCE)`, exact (`:184-192`, `:1047`);
  - sets the source to `"user:xfer-dialog"` and type `"transaction"` (`:1052-1053`).

**Pre-filled rate** (`:316-404`): look for a pricedb price on the same day, else the nearest in time (either side; ties
go to the older), searching both directions. A price stored as from-in-to is inverted.
- Two legacy euro currencies use the fixed euro conversion rate instead.
- The date is the dialog date at neutral time (`gdate_to_time64`).

**Transaction created** (`create_transaction`, `:1473-1530`):

- the currency is the from commodity;
- the date is set with `SetDatePostedSecsNormalized`;
- the from split is added first and the to split second;
- `xaccSplitSetBaseValue(from, -amount, from_com)` sets value = amount = −amount;
- `xaccSplitSetBaseValue(to, amount, from_com)` sets value = +amount;
- `xaccSplitSetBaseValue(to, to_amount, to_com)` sets quantity = +to_amount;
- all rounding is ROUND_HALF_UP, to the currency fraction or the account SCU (`Split.cpp:1373-1390`);
- the memo goes on both splits;
- num or action follows the book option.
- The commit then sorts the splits debit-first, so the file order is [to, from].

**Price recorded on OK** (`create_price`, `:1625-1640`). This runs whenever the commodities differ, including in
"exchange-rate only" mode. The two legacy-euro case is skipped.

- **Same-day price exists, in either direction** (`update_price`, `:1550-1588`):
  - Stop if the existing price's source ranks higher. Ranks are `PriceSource` enum order, lower is better:
    `user:price-editor` < `Finance::Quote` < `user:price` < `user:xfer-dialog` < `user:split-register` < …
    (`5.17:libgnucash/engine/gnc-pricedb.h:165-182`).
  - Stop if both values are equal after `round_price`:
    - currency↔currency: `convert(v, 10000, ROUND_HALF_UP)`;
    - otherwise: denominator `scu(currency side) × 10000` (`:201-217`).
  - Otherwise **update the existing price in place**: time, type and the *unrounded* value, inverted if the existing
    price is the other way round. **Its source is kept.**
- **No same-day price** (`new_price`, `:1591-1620`):
  - value = `|rate|`, **not rounded**;
  - orientation:
    - from is a currency and to is not: store commodity = to, currency = from, value = 1/rate;
    - else from == the user's default currency and to is not: also swapped;
    - otherwise commodity = from, currency = to;
  - source is `user:price` or `user:xfer-dialog`; type is `transaction`; time is neutral time on the transfer date;
  - `gnc_pricedb_add_price` replaces a same-day price of equal or lower rank, and refuses the new price if the existing
    one ranks higher (`gnc-pricedb.cpp:1053-1069`).
- The "default currency" is a user preference. red-ale must take it as a parameter (§11).

### 10.2 Register path and `xaccTransRecordPrice`

When a register split's account commodity differs from the transaction currency, the register opens the same dialog
in exchange mode (`5.17:gnucash/register/ledger-core/split-register-control.cpp:1350-1570`). It saves:

- `value = div(entered, rate, currency fraction, ROUND_HALF_UP)`;
- `amount = mul(value, rate, account SCU, ROUND_HALF_UP)` (`split-register-model-save.c:489`, `:506`).

After commit, the register calls `xaccTransRecordPrice(trans, PRICE_SOURCE_SPLIT_REG)` (`split-register.c:1856`,
`:1909`, `:2038`; `Transaction.cpp:2713-2810`). It is **not** called by `xaccTransCommitEdit`. For each split:

- Only for **priced account types (STOCK, MUTUAL, CURRENCY)**. A BANK account in a foreign currency records nothing
  (oracle).
- Skipped when amount is 0.
- price = value/amount (AUTO, EXACT); commodity = account commodity; currency = transaction currency; time = date
  posted.
- **Same-day price exists:**
  - if it is equal, do nothing;
  - if it ranks higher, do nothing, **except** that `user:split-register` may override `user:xfer-dialog`;
  - otherwise update it in place: value inverted if the existing price is reversed, then
    `convert(v, fraction(account commodity) × 10000, ROUND_HALF_UP)`, plus time, **source** and type `transaction`.
- **No same-day price:** create one the same way.
- Oracle: EUR (CURRENCY type) value 110.00 / quantity 100.00 gave price EUR/USD `1100000/1000000`, source
  `user:split-register`. The value is not reduced.

**red-ale:** `transaction.post` with an exchange rate should derive quantity = value × rate rounded half-up to the
account SCU (or accept an explicit quantity). It should optionally record a price with the transfer-dialog rules, but
only when asked: the price is a separate, visible side effect.

---

## 11. Open questions

1. **Denominator normalisation on load.** GnuCash 5.17 rewrites split values to the currency fraction and quantities to
   the account SCU on load (§1.5). Options:
   - (a) keep stored denominators (red-ale's "keep the original denominator" rule); the file differs from what GnuCash
     would save, though GnuCash reads it the same way;
   - (b) normalise as GnuCash does.
   A tie-breaker: GnuCash refuses a value conversion that rounds a non-zero value to 0, and keeps the raw value.
2. **Locale collation** in `xaccSplitOrder`, `xaccTransOrder` and `xaccAccountOrder` (`g_utf8_collate`). Which
   collation should red-ale use when a newly written transaction ties on date, closing flag, num and date entered?
   The suggestion is the C or Unicode root collation, documented as a known divergence.
3. Should `transaction.reverse` refuse voided or read-only transactions? GnuCash allows it.
4. **Load-time scrubs:** which of §9.5's load-time scrubs red-ale should run, and whether they mark the book dirty in
   GnuCash. Not checked: whether opening and saving an untouched book in GnuCash changes anything.
5. `xaccTransFindCommonCurrency` tie-breaking (`g_slist_sort` with `commodity_compare` ascending and taking the head
   looks like it picks the *lowest* score). This needs an oracle test with a mixed-currency transaction that has no
   currency.
6. The user's default currency and the account separator are GnuCash preferences, not book data. red-ale needs its own
   settings for them.
7. The `f` (frozen) reconcile state is never set by 5.17 code. Keep it as data and count it as reconciled.
8. **Local-time rules** need a time zone that red-ale must take from somewhere: the session, the API caller, or a
   setting. They are:
   - statement date at 23:59:59 local;
   - the local calendar date for `SetDatePostedSecsNormalized`;
   - Find's date boundaries;
   - accounting-period bounds.
   Neutral time (10:59 UTC) does not need a time zone, apart from the extreme-offset shift.
9. **Delete-with-move across commodities** silently reinterprets quantities (§3.2). Should red-ale forbid it, or
   require explicit per-split quantities?
10. The reconcile window lists future-dated `n`/`c` splits, with no date filter. Should red-ale's API do the same, or
    default to splits on or before the statement date?
