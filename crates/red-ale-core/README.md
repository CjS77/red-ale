# red-ale-core

The bookkeeping model, and the rules that keep it consistent. Everything else in red-ale is built around this crate.

## Role

`red-ale-core` describes what a GnuCash book *is*: commodities, accounts, transactions and their splits, prices, lots,
and business objects such as customers and invoices. It also holds every operation that changes a book, such as
posting a transaction, voiding it, recording a stock split or posting an invoice. Each operation follows GnuCash 5.17's
rules, and the code cites the GnuCash source it matches.

The crate is pure. It does no file or network I/O, has no async code and knows nothing about file formats. The clock
and new GUIDs come in through the `Env` trait, so every operation can be tested with a fixed time and predictable GUIDs.

Two rules hold everywhere:

- **Money is exact.** Every amount, value, price and rate is a `Numeric`: a fraction with 64-bit numerator and
  denominator, like GnuCash's `gnc_numeric`. Floating point is never used, and the crate denies float arithmetic.
- **Transactions balance.** A transaction's split values must sum to exactly zero. An operation that would leave one
  unbalanced is refused, not repaired.

## Where it fits

This is the domain at the centre of red-ale. It depends on no other red-ale crate, and every other crate depends on
it:

```text
red-ale-xml      reads and writes the core model as GnuCash XML
red-ale-service  opens a book, and runs core operations on it as all-or-nothing changes
red-ale-rpc      exposes core operations as JSON-RPC methods
```

The names in this crate are the ones in [`design/glossary.md`](../../design/glossary.md), which mostly follow GnuCash.
Watch for two in particular: a split's *value* is in the transaction's currency, and its *amount* is in the account's
commodity.

## What's inside

| Module | Contents |
|--------|----------|
| `book` | `Book`, which holds every entity of one GnuCash book; `Env` and `FixedEnv` |
| `numeric` | `Numeric`, with GnuCash's arithmetic and rounding modes |
| `account`, `transaction`, `commodity`, `price`, `lot`, `business` | The entities |
| `guid`, `time`, `slots` | GUIDs, timestamps and dates, and key-value slots |
| `ops` | The operations, one module per area: `accounts`, `transactions`, `commit`, `reconcile`, `scrub`, `commodities`, `prices`, `lots`, `period`, `business` |

## Usage

Build a book, add two accounts and post a transaction:

```rust
use red_ale_core::ops::accounts::{AccountEdit, balances, create_account};
use red_ale_core::ops::commit::{NewSplit, NewTransaction, add_transaction};
use red_ale_core::{AccountType, Book, CommodityRef, Date, FixedEnv, Numeric, Time64};

// Time and new GUIDs come from an Env; tests use a fixed one.
let mut env = FixedEnv::new(Time64(1_719_835_200), 0);
let mut book = Book::new_empty(&mut env);
let usd = CommodityRef::currency("USD");

let account = |name: &str, account_type| AccountEdit {
    name: Some(name.to_owned()),
    account_type: Some(account_type),
    commodity: Some(usd.clone()),
    ..AccountEdit::default()
};
let root = book.root;
let checking = create_account(&mut book, &mut env, root, &account("Checking", AccountType::Bank))?;
let salary = create_account(&mut book, &mut env, root, &account("Salary", AccountType::Income))?;

let mut pay = NewTransaction::new(usd, Date::from_ymd(2024, 7, 1).ok_or("bad date")?);
pay.description = Some("Pay day".to_owned());
pay.splits.push(NewSplit::new(checking, Numeric::parse_decimal("2500.00")?));
pay.splits.push(NewSplit::new(salary, Numeric::parse_decimal("-2500.00")?));
add_transaction(&mut book, &mut env, pay)?; // refused if the splits don't balance

assert_eq!(balances(&book, checking, None)?.balance.to_decimal_string(), "2500.00");
```

Exact arithmetic with GnuCash's rounding:

```rust
use red_ale_core::{How, Numeric, Round};

let price = Numeric::parse_decimal("19.99")?;                 // 1999/100, exactly
let cents = Numeric::new(1, 3).convert(100, How::round(Round::HalfUp))?;
assert_eq!(cents.to_decimal_string(), "0.33");
```

These crates are red-ale's internals, not a library for other programs. Other programs use red-ale through its
JSON-RPC API.
