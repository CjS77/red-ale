//! Synthetic books in GnuCash 5.17's format, of any size, for measuring memory and speed.
//!
//! The shape follows a small business's books: [`ACCOUNTS`] accounts under five top-level accounts, and transactions
//! of two or three splits in one currency, each with a description and the `date-posted` slot GnuCash gives every
//! transaction. The book is built in the core model and written by red-ale's generators, which write what GnuCash
//! 5.17 writes; the oracle test `oracle_synthetic` checks that GnuCash saves it back byte for byte.
//!
//! Transactions are written in GnuCash's order (`5.17:libgnucash/backend/xml/io-gncxml-v2.cpp:1166-1176`):
//! accounts depth first, each account's splits by date, each transaction where it is first met.

use std::sync::Arc;

use red_ale_core::currencies::iso_currency;
use red_ale_core::{
    Account, AccountType, Book, CommodityRef, Env, FixedEnv, Frame, Guid, Numeric, ReconcileState, SlotValue, Split, Time64, Transaction,
};

/// Accounts in every synthetic book, the root included.
pub const ACCOUNTS: usize = 2_000;

/// 2015-01-01 10:59:00 UTC, the first transaction's date (GnuCash posts at 10:59 UTC, its neutral time).
const FIRST_POSTED: i64 = 1_420_109_940;
const DAY: i64 = 86_400;
const TRANSACTIONS_PER_DAY: usize = 40;

const TOP_LEVEL: [(&str, AccountType); 5] = [
    ("Assets", AccountType::Asset),
    ("Liabilities", AccountType::Liability),
    ("Equity", AccountType::Equity),
    ("Income", AccountType::Income),
    ("Expenses", AccountType::Expense),
];

/// A synthetic book of `transactions` transactions, as the plain (uncompressed) text of its file.
pub fn synthetic_book_text(transactions: usize) -> String {
    let book = synthetic_book(transactions);
    red_ale_xml::write_book_text(&book, &red_ale_xml::XmlResidue::new_book(&book, false))
}

/// A synthetic book of `transactions` transactions, with [`ACCOUNTS`] accounts. The same size always gives the same
/// book.
pub fn synthetic_book(transactions: usize) -> Book {
    let mut env = FixedEnv::new(Time64(FIRST_POSTED), 0x5e_u128 << 120);
    let mut book = Book::new_empty(&mut env);
    add_currency(&mut book);
    let leaves = add_accounts(&mut book, &mut env);
    let mut random = Lcg(0x2545_f491_4f6c_dd1d);
    let made: Vec<Transaction> = (0..transactions).map(|i| transaction(i, &leaves, &mut env, &mut random)).collect();
    gnucash_order(&book, made).into_iter().for_each(|t| {
        book.transactions.insert(t.guid, Arc::new(t));
    });
    book
}

fn usd() -> CommodityRef {
    CommodityRef::new("CURRENCY", "USD")
}

/// USD, with quotes on: GnuCash turns them on for a currency when an account first uses it
/// (`5.17:libgnucash/engine/gnc-commodity.cpp:1416-1423`), so its books carry it.
fn add_currency(book: &mut Book) {
    if let Some(iso) = iso_currency("USD") {
        let mut usd = iso.to_commodity();
        usd.quote_flag = true;
        usd.quote_source = Some("currency".into());
        usd.quote_tz = Some(String::new());
        book.commodities.insert(usd.id.clone(), Arc::new(usd));
    }
}

/// Five top-level accounts and the rest spread evenly under them, depth first as GnuCash writes them. Returns the
/// leaf accounts.
fn add_accounts(book: &mut Book, env: &mut FixedEnv) -> Vec<Guid> {
    let children = ACCOUNTS - 1 - TOP_LEVEL.len();
    let root = book.root;
    TOP_LEVEL
        .iter()
        .enumerate()
        .flat_map(|(i, (name, kind))| {
            let parent = add_account(book, env, (*name).to_owned(), *kind, root);
            let count = children / TOP_LEVEL.len() + usize::from(i < children % TOP_LEVEL.len());
            (0..count)
                .map(|n| add_account(book, env, format!("{name} {n:04}"), *kind, parent))
                .collect::<Vec<_>>()
        })
        .collect()
}

fn add_account(book: &mut Book, env: &mut FixedEnv, name: String, kind: AccountType, parent: Guid) -> Guid {
    let mut account = Account::new(env.new_guid(), name, kind, Some(usd()), Some(parent));
    account.commodity_scu = Some(100);
    let guid = account.guid;
    book.accounts.insert(guid, Arc::new(account));
    guid
}

/// Transaction `i`: posted on day `i / 40`, entered a minute after the one before, with two splits or (every other
/// one) three, in distinct accounts.
fn transaction(i: usize, leaves: &[Guid], env: &mut FixedEnv, random: &mut Lcg) -> Transaction {
    let n = i64::try_from(i).unwrap_or(i64::MAX);
    let date_posted = Time64(FIRST_POSTED + DAY * (n / i64::try_from(TRANSACTIONS_PER_DAY).unwrap_or(1)));
    let mut slots = Frame::new();
    if let Some(date) = date_posted.utc_date() {
        slots.insert_key("date-posted", SlotValue::Date(date));
    }
    let amounts = split_amounts(i, random);
    let accounts = distinct_accounts(amounts.len(), leaves, random);
    let splits = amounts
        .iter()
        .zip(accounts)
        .map(|(cents, account)| split(env.new_guid(), account, *cents, random))
        .collect();
    Transaction {
        guid: env.new_guid(),
        currency: usd(),
        num: None,
        date_posted,
        date_entered: Time64(FIRST_POSTED + 60 * n),
        description: Some(format!("Synthetic transaction {i}")),
        slots,
        splits,
    }
}

/// Values in cents that sum to zero.
fn split_amounts(i: usize, random: &mut Lcg) -> Vec<i64> {
    let total = random.below(1_000_000) + 1;
    if i.is_multiple_of(2) {
        vec![total, -total]
    } else {
        let part = random.below(total) + 1;
        vec![total, -part, part - total]
    }
}

fn distinct_accounts(n: usize, leaves: &[Guid], random: &mut Lcg) -> Vec<Guid> {
    let len = i64::try_from(leaves.len()).unwrap_or(1);
    let first = random.below(len);
    (0..n)
        .filter_map(|k| {
            leaves
                .get(usize::try_from((first + i64::try_from(k).unwrap_or(0) * 7) % len).unwrap_or(0))
                .copied()
        })
        .collect()
}

fn split(guid: Guid, account: Guid, cents: i64, random: &mut Lcg) -> Split {
    let mut s = Split::new(guid, account, Numeric::new(cents, 100), Numeric::new(cents, 100));
    if random.below(4) == 0 {
        s.memo = Some(format!("Memo {}", random.below(10_000)));
    }
    if random.below(3) == 0 {
        s.reconcile_state = ReconcileState::Cleared;
    }
    s
}

/// GnuCash's write order: accounts depth first, and in each account its transactions by date, each transaction
/// where it is first met. Transaction `i` is posted no earlier and entered later than transaction `i - 1`, so an
/// account's transactions sort by their index.
fn gnucash_order(book: &Book, made: Vec<Transaction>) -> Vec<Transaction> {
    let index_of = |account: Guid| book.accounts.get_index_of(&account);
    let mut by_account: Vec<Vec<usize>> = vec![Vec::new(); book.accounts.len()];
    made.iter().enumerate().for_each(|(i, t)| {
        t.splits
            .iter()
            .filter_map(|s| s.account.and_then(index_of))
            .for_each(|a| by_account[a].push(i));
    });
    let mut seen = vec![false; made.len()];
    let order: Vec<usize> = by_account
        .into_iter()
        .flatten()
        .filter(|i| !std::mem::replace(&mut seen[*i], true))
        .collect();
    let mut slots: Vec<Option<Transaction>> = made.into_iter().map(Some).collect();
    order.into_iter().filter_map(|i| slots[i].take()).collect()
}

/// A small deterministic generator (Knuth's MMIX constants), so the same size always gives the same book.
struct Lcg(u64);

impl Lcg {
    /// A number in `0..n`, for `n > 0`.
    fn below(&mut self, n: i64) -> i64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        let n = u64::try_from(n.max(1)).unwrap_or(1);
        i64::try_from((self.0 >> 33) % n).unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn has_the_requested_size_and_balances() {
        let book = synthetic_book(101);
        assert_eq!(book.accounts.len(), ACCOUNTS);
        assert_eq!(book.transactions.len(), 101);
        assert!(book.transactions.values().all(|t| t.imbalance().is_ok_and(Numeric::is_zero)));
        assert!(book.transactions.values().all(|t| (2..=3).contains(&t.splits.len())));
    }

    #[test]
    fn is_deterministic_and_reads_back_as_itself() {
        let text = synthetic_book_text(50);
        assert_eq!(text, synthetic_book_text(50));
        let (book, _) = red_ale_xml::read_book(text.as_bytes()).expect("reads");
        assert_eq!(book, synthetic_book(50));
    }
}
