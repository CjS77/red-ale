//! Time Mutations on a synthetic book, for "Decide before code", item 1, in `design/milestone3.md`.
//!
//! ```text
//! cargo run --release -p red-ale-testkit --example memory -- generate 100000 /tmp/synthetic-100k.gnucash
//! cargo run --release -p red-ale-testkit --example mutation_cost -- /tmp/synthetic-100k.gnucash
//! ```
//!
//! It times `transaction.post` through the dispatcher, then, on the Session, a Mutation that changes nothing, and one
//! Mutation adding 5,000 transactions (an import) against Mutations adding one each, with the time cloning the Book
//! and making the edits take, and how long finding the changed transactions takes by GUID (as a Mutation does) and
//! by walking the two maps in order.

#![allow(clippy::expect_used, clippy::panic, clippy::cast_precision_loss)]

use std::path::Path;
use std::sync::Arc;
use std::hint::black_box;
use std::time::{Duration, Instant};

use red_ale_core::ops::commit::{NewSplit, NewTransaction};
use red_ale_core::ops::transactions::post;
use red_ale_core::{Book, CommodityRef, Date, DomainResult, Env, Guid, Numeric};
use red_ale_service::{Session, Settings};
use red_ale_testkit::{TempBook, call, dispatcher};
use serde_json::json;

/// Transactions an import adds in one Mutation.
const IMPORT: usize = 5_000;
/// Mutations adding one transaction each, timed for comparison with one Mutation adding [`IMPORT`].
const MANY: usize = 500;
/// `transaction.post` calls and empty Mutations timed.
const POSTS: usize = 25;

fn main() {
    let path = std::env::args().nth(1).expect("usage: mutation_cost <synthetic book>");
    let path = Path::new(&path);
    single_posts(path);
    import(path);
}

fn copy(path: &Path) -> TempBook {
    let book = TempBook::new_path();
    std::fs::copy(path, &book.path).unwrap_or_else(|e| panic!("copying {}: {e}", path.display()));
    book
}

fn single_posts(path: &Path) {
    let book = copy(path);
    let mut d = dispatcher();
    call(&mut d, "book.open", &json!({ "path": book.path_str() }));
    let posting = json!({
        "datePosted": "2024-06-20", "description": "Measured",
        "splits": [
            { "account": "Assets:Assets 0000", "value": "-12.34" },
            { "account": "Expenses:Expenses 0000", "value": "12.34" }
        ]
    });
    let mut times: Vec<Duration> = (0..POSTS).map(|_| timed(|| call(&mut d, "transaction.post", &posting)).1).collect();
    times.sort();
    println!("transaction.post        median {:>9.2?}  min {:>9.2?}", times[POSTS / 2], times[0]);
    call(&mut d, "book.close", &json!({ "discard": true }));
}

fn import(path: &Path) {
    let book = copy(path);
    let mut session = Session::open(&book.path_str(), false, Settings::default()).expect("the book opens");
    let (from, to) = (account(session.book(), "Assets:Assets 0000"), account(session.book(), "Expenses:Expenses 0000"));
    let (clone, clone_time) = timed(|| session.book().clone());
    println!("clone the Book          {clone_time:>9.2?}");
    let old = session.book();
    let (_, hashed) = timed(|| black_box(black_box(&clone).transactions.values().filter(|t| old.transactions.get(&t.guid).is_none_or(|o| !Arc::ptr_eq(o, t))).count()));
    println!("find changes by GUID    {hashed:>9.2?}");
    let (_, aligned) = timed(|| black_box(black_box(&clone).transactions.iter().zip(&old.transactions).filter(|((g, t), (h, o))| g != h || !Arc::ptr_eq(o, t)).count()));
    println!("find changes in order   {aligned:>9.2?}");
    drop(clone);
    let mut empty: Vec<Duration> = (0..POSTS).map(|_| timed(|| session.mutate(|_, _| Ok(())).expect("changes nothing")).1).collect();
    empty.sort();
    println!("a Mutation changing nothing {:>9.2?}", empty[POSTS / 2]);
    let mut edits = Duration::ZERO;
    let (_, one) = timed(|| {
        session
            .mutate(|b, env| {
                let (result, took) = timed(|| (0..IMPORT).try_for_each(|i| add(b, env, from, to, i)));
                edits = took;
                result
            })
            .expect("the import posts");
    });
    println!("{IMPORT} in one Mutation  {one:>9.2?}  (of which the edits {edits:.2?})");
    let (_, many) = timed(|| (0..MANY).for_each(|i| session.mutate(|b, env| add(b, env, from, to, i)).expect("posts")));
    println!("{MANY} Mutations of one   {many:>9.2?}  ({:.2?} each)", many / u32::try_from(MANY).unwrap_or(1));
    session.close(false).expect("closes");
}

fn account(book: &Book, name: &str) -> Guid {
    *book.accounts.keys().find(|g| book.full_name(**g).as_deref() == Some(name)).unwrap_or_else(|| panic!("no account {name}"))
}

fn add(book: &mut Book, env: &mut dyn Env, from: Guid, to: Guid, i: usize) -> DomainResult<()> {
    let cents = i64::try_from(i % 10_000).unwrap_or(0) + 1;
    let mut new = NewTransaction::new(CommodityRef::new("CURRENCY", "USD"), Date::from_ymd(2024, 6, 20).expect("a date"));
    new.description = Some(format!("Imported {i}"));
    new.splits = vec![NewSplit::new(from, Numeric::new(-cents, 100)), NewSplit::new(to, Numeric::new(cents, 100))];
    post(book, env, new).map(drop)
}

fn timed<T>(f: impl FnOnce() -> T) -> (T, Duration) {
    let start = Instant::now();
    let out = f();
    (out, start.elapsed())
}
