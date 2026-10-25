# API: closing the books

GnuCash's *Close Book* (Tools ▸ Close Book…, `5.17:gnucash/ui/gnc-plugin-basic-commands.ui:119`) zeroes the income
and expense accounts into equity at a closing date. Its logic lives in the dialog's callbacks
(`5.17:gnucash/gnome-utils/dialog-book-close.c`); red-ale runs the same steps as one Mutation.

The name is `book.closeBooks`, because `book.close` already closes the Session. Closing the books leaves the book
open and editable.

Code: `red-ale-core/src/ops/close_book.rs`, `red-ale-rpc/src/methods/close_book.rs`.
Tests: unit tests beside the core code, `crates/red-ale-rpc/tests/close_book.rs` (API) and
`crates/red-ale-testkit/tests/oracle_close_book.rs` (GnuCash 5.17 closes a copy of the same book with the dialog's
steps through its bindings, and both books must match).

## Method

| Method           | Effect | Params                                                          | Result        |
|------------------|--------|-----------------------------------------------------------------|---------------|
| `book.closeBooks`| write  | `{date, incomeTotalAccount, expenseTotalAccount, description?}` | `CloseResult` |

- `date`: the closing date (`YYYY-MM-DD`).
- `incomeTotalAccount`, `expenseTotalAccount` (`AccountRef`): the equity accounts that take the period's income and
  expense, the dialog's *Income Total* and *Expense Total*. Both must be `EQUITY` accounts, the only type the dialog
  offers (`5.17:gnucash/gnome-utils/dialog-book-close.c:350-365`); they may be the same account.
- `description`: the closing transactions' description. The dialog's field starts empty, and so does this default.

`CloseResult`: `transactions`, one `{guid, kind: income|expense, currency, equityAccount, total}` per closing
transaction made, income first; `createdAccounts`, the GUIDs of equity accounts made for a currency. `total` is the
equity split's value: the sum of the closed balances. Closing a book with no income or expense balances makes
nothing and succeeds.

## What GnuCash does

The dialog closes income, then expense (`gnc_book_close_response_cb`, `dialog-book-close.c:284-308`). For each:

1. **Balances.** Every account of exactly that type under the root, depth first in tree order, placeholder and
   hidden ones included, gives its balance as of the end of the closing day: the running balance of its last split
   posted strictly before local 23:59:59 of that day (`close_accounts_cb`, `:123-157`; `xaccAccountGetBalanceAsOfDate`,
   `5.17:libgnucash/engine/Account.cpp:3506-3524`; `gnc_time64_get_day_end`, `5.17:libgnucash/engine/gnc-date.cpp:1386-1394`).
   An account whose balance is zero is skipped. Sub-accounts are closed on their own, never through their parent.
2. **One transaction per currency.** The account's currency is its commodity when that is a currency, else the
   nearest ancestor's (`gnc_account_or_default_currency`, `5.17:libgnucash/app-utils/gnc-ui-util.cpp:788-812`;
   `gnc_account_get_currency_or_parent`, `5.17:libgnucash/engine/Account.cpp:3403-3411`). The first account in a
   currency starts that currency's transaction (`find_or_create_txn`, `dialog-book-close.c:89-115`): currency, date
   posted the closing date at neutral time with its `date-posted` slot, date entered now, the description, and the
   slot `book_closing` = 1 (`xaccTransSetIsClosingTxn`, `5.17:libgnucash/engine/Transaction.cpp:2049-2055`).
3. **The account's split** has value and amount minus its balance, rounded half up to the currency's fraction and
   the account's SCU (`xaccSplitSetBaseValue`, `5.17:libgnucash/engine/Split.cpp:1355-1400`). Memo, action and
   reconcile state are GnuCash's defaults (empty, empty, `n`).
4. **The equity split** (`finish_txn_cb`, `dialog-book-close.c:160-212`) takes the exact sum of the balances as value
   and amount, in:
   - the chosen account itself, when this pass found only one currency and the account is in it;
   - otherwise the account's descendant named after the currency's mnemonic, searched children first and then each
     child's subtree (`gnc_account_lookup_by_name`, `5.17:libgnucash/engine/Account.cpp:3080-3084`);
   - otherwise a new `EQUITY` account under the chosen one, named and described with the mnemonic, in that currency,
     placed last among its children.
5. **Commit** sorts the splits with debits first (`xaccTransSortSplits`, `5.17:libgnucash/engine/Transaction.cpp:540-553`).

After closing, every income and expense account's balance at the end of the closing day is zero. Nothing else marks
the book: GnuCash records no closing date.

Day boundaries are local time (milestone 2, "Time zones"). The server's local zone sets the end of the closing day.

## Deviations

| Deviation | Reason |
|-----------|--------|
| An income or expense account with a balance whose commodity is not a currency is refused with `invalid`. GnuCash closes it in its nearest ancestor's currency (or the user's default currency), writing the share count as the value with a zero amount. | That split would book a quantity of a security as money. |
| A chosen equity account's descendant named after the currency but in another commodity is refused with `invalid`. GnuCash stops on an assertion. | No transaction can be made. |
| If rounding the account splits to the currency's fraction leaves the transaction unbalanced (balances finer than the currency allows), the call fails with `unbalanced`. GnuCash's commit adds an `Imbalance-<CUR>` split (`5.17:libgnucash/engine/Transaction.cpp:1549-1558`). | red-ale refuses unbalanced transactions rather than repairing them (`red-ale-core/src/ops/commit.rs`). |
