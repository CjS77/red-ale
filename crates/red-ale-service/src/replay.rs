//! Replaying a journal onto a book, as GnuCash's *File → Import → Replay GnuCash .log file* does
//! (`5.17:gnucash/import-export/log-replay/gnc-log-replay.cpp`; `design/notes/storage-and-safety.md` §9).
//!
//! Replay is an upsert by GUID: `C` records create or update transactions and splits with the fields they carry
//! (an empty field leaves the value alone), `D` records delete, `B` and `R` records are ignored. Two deliberate
//! differences from GnuCash, both to protect the book:
//! - a record with a line that cannot be read (GnuCash writes tabs inside text fields raw, so such a line has more
//!   than 17 fields) is skipped whole, never applied in part;
//! - a replayed transaction that would not balance is refused instead of getting an `Imbalance-<CUR>` split, unless
//!   the caller asks to skip such records.

use std::sync::Arc;

use red_ale_core::ops::commit;
use red_ale_core::{Book, CommodityRef, DomainError, DomainResult, Env, Frame, Guid, Numeric, ReconcileState, Split, Time64, Transaction};

use crate::journal::{JournalLine, JournalRecord, RecordKind, SkippedLine};

/// What a replay did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReplayReport {
    /// Records read.
    pub records: usize,
    /// Transactions created.
    pub created: Vec<Guid>,
    /// Transactions updated.
    pub updated: Vec<Guid>,
    /// Transactions deleted.
    pub deleted: Vec<Guid>,
    /// Deletions of transactions the book does not have ("The transaction to delete was not found!").
    pub not_found: Vec<Guid>,
    /// Records skipped, with why.
    pub skipped_records: Vec<String>,
    /// Lines that could not be read.
    pub skipped_lines: Vec<SkippedLine>,
}

/// Apply `records` to `book`. With `skip_invalid`, a record that cannot be applied is reported and skipped;
/// otherwise the first one fails the whole replay.
pub fn replay(book: &mut Book, env: &mut dyn Env, records: &[JournalRecord], skip_invalid: bool) -> DomainResult<ReplayReport> {
    let mut report = ReplayReport { records: records.len(), ..ReplayReport::default() };
    records.iter().try_for_each(|record| match apply_record(book, env, record, &mut report) {
        Err(e) if skip_invalid => {
            report.skipped_records.push(format!("record at line {}: {e}", record.start_line));
            Ok(())
        }
        Err(e) => Err(DomainError::invalid(format!("journal record at line {}: {e}", record.start_line))),
        Ok(()) => Ok(()),
    })?;
    Ok(report)
}

fn apply_record(book: &mut Book, env: &mut dyn Env, record: &JournalRecord, report: &mut ReplayReport) -> DomainResult<()> {
    if !record.complete {
        report.skipped_records.push(format!("record at line {}: some lines could not be read", record.start_line));
        return Ok(());
    }
    let Some(first) = record.lines.first() else { return Ok(()) };
    match first.kind {
        RecordKind::Delete => {
            if book.transactions.shift_remove(&first.trans).is_some() {
                report.deleted.push(first.trans);
            } else {
                report.not_found.push(first.trans);
            }
            Ok(())
        }
        RecordKind::Commit => upsert(book, env, &record.lines.iter().filter(|l| l.kind == RecordKind::Commit).collect::<Vec<_>>(), report),
        RecordKind::Begin | RecordKind::Rollback => Ok(()),
    }
}

fn upsert(book: &mut Book, env: &mut dyn Env, lines: &[&JournalLine], report: &mut ReplayReport) -> DomainResult<()> {
    let Some(first) = lines.first() else { return Ok(()) };
    let guid = first.trans;
    let existing = book.transactions.get(&guid).map(|t| (**t).clone());
    let created = existing.is_none();
    let mut t = match existing {
        Some(t) => t,
        None => new_transaction(book, guid, lines)?,
    };
    apply_transaction_fields(&mut t, first);
    lines.iter().filter(|l| l.split.is_some()).try_for_each(|line| apply_split(book, &mut t, line))?;
    commit::commit(book, env, &mut t)?;
    book.transactions.insert(guid, Arc::new(t));
    if created { report.created.push(guid) } else { report.updated.push(guid) }
    Ok(())
}

/// A new transaction. Its currency is the first account's commodity, when that is a currency: the journal does not
/// record currencies (`gnc-log-replay.cpp:470-480` falls back to the default currency, which red-ale does not have).
fn new_transaction(book: &Book, guid: Guid, lines: &[&JournalLine]) -> DomainResult<Transaction> {
    let currency = lines
        .iter()
        .filter_map(|l| l.account.and_then(|a| book.account(a)).and_then(|a| a.commodity.clone()))
        .find(CommodityRef::is_currency)
        .ok_or_else(|| DomainError::invalid(format!("cannot tell the currency of new transaction {guid}")))?;
    Ok(Transaction {
        guid,
        currency,
        num: None,
        date_posted: Time64(0),
        date_entered: Time64(0),
        description: None,
        slots: Frame::new(),
        splits: Vec::new(),
    })
}

/// Fields present on the first line (`gnc-log-replay.cpp:415-462`). The posted date is set raw, as
/// `xaccTransSetDatePostedSecs` does, without touching the `date-posted` slot.
fn apply_transaction_fields(t: &mut Transaction, line: &JournalLine) {
    if let Some(d) = line.date_entered {
        t.date_entered = d;
    }
    if let Some(d) = line.date_posted {
        t.date_posted = d;
    }
    if let Some(n) = &line.num {
        t.num = Some(n.clone());
    }
    if let Some(d) = &line.description {
        t.description = Some(d.clone());
    }
    if let Some(n) = &line.notes {
        commit::set_notes(t, Some(n));
    }
}

/// Look up or create the split and set the fields present (`gnc-log-replay.cpp:463-517`). A split the book holds in
/// another transaction moves to this one, as `xaccTransAppendSplit` would move it.
fn apply_split(book: &mut Book, t: &mut Transaction, line: &JournalLine) -> DomainResult<()> {
    let Some(split_guid) = line.split else { return Ok(()) };
    if t.split(split_guid).is_none() {
        let moved = take_split_from_elsewhere(book, t.guid, split_guid)?;
        t.splits.push(moved.unwrap_or_else(|| Split { account: None, ..Split::new(split_guid, Guid::from_bytes([0; 16]), Numeric::ZERO, Numeric::ZERO) }));
    }
    let account = match line.account {
        Some(a) if book.account(a).is_some() => Some(Some(a)),
        Some(a) => return Err(DomainError::not_found("account", &a)),
        None => None,
    };
    let split = t.split_mut(split_guid).ok_or_else(|| DomainError::not_found("split", &split_guid))?;
    if let Some(a) = account {
        split.account = a;
    }
    if let Some(m) = &line.memo {
        split.memo = Some(m.clone());
    }
    if let Some(a) = &line.action {
        split.action = Some(a.clone());
    }
    if let Some(d) = line.date_reconciled {
        split.reconcile_date = (d.0 != 0).then_some(d);
    }
    if let Some(state) = line.reconciled.and_then(ReconcileState::from_char) {
        split.reconcile_state = state;
    }
    if let Some(a) = line.amount {
        split.amount = a;
    }
    if let Some(v) = line.value {
        split.value = v;
    }
    if split.account.is_none() {
        return Err(DomainError::invalid(format!("split {split_guid} has no account")));
    }
    Ok(())
}

fn take_split_from_elsewhere(book: &mut Book, keep: Guid, split: Guid) -> DomainResult<Option<Split>> {
    let Some(owner) = book.transactions.values().find(|t| t.guid != keep && t.split(split).is_some()).map(|t| t.guid) else {
        return Ok(None);
    };
    let other = book.transactions.get_mut(&owner).map(Arc::make_mut).ok_or_else(|| DomainError::not_found("transaction", &owner))?;
    let index = other.splits.iter().position(|s| s.guid == split).ok_or_else(|| DomainError::not_found("split", &split))?;
    Ok(Some(other.splits.remove(index)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::journal::parse;
    use red_ale_core::{Account, AccountType, FixedEnv};

    fn book() -> (Book, Guid, Guid) {
        let mut env = FixedEnv::new(Time64(0), 0);
        let mut book = Book::new_empty(&mut env);
        let usd = CommodityRef::currency("USD");
        let mut add = |guid: &str, name: &str, ty| {
            let g = Guid::parse(guid).unwrap();
            book.accounts.insert(g, Arc::new(Account::new(g, name, ty, Some(usd.clone()), Some(book.root))));
            g
        };
        let bank = add("d031a27171554d279107453116bb1dee", "Bank", AccountType::Bank);
        let food = add("48e84d8dc4e24c538e3db22e8d33a5c1", "Food", AccountType::Expense);
        let _ = env.new_guid();
        (book, bank, food)
    }

    const HEAD: &str = "mod\ttrans_guid\tsplit_guid\ttime_now\tdate_entered\tdate_posted\tacc_guid\tacc_name\tnum\tdescription\tnotes\tmemo\taction\treconciled\tamount\tvalue\tdate_reconciled\n-----------------\n";

    fn line(flag: char, desc: &str, split: &str, acc: &str, amount: &str) -> String {
        format!("{flag}\t65e21d8a290f4355b9555dfca9bfe888\t{split}\t2026-10-25 13:04:17\t2026-10-25 13:04:17\t2024-03-15 10:59:00\t{acc}\tX\t101\t{desc}\ta note\t\t\tn\t{amount}\t{amount}\t1970-01-01 00:00:00\n")
    }

    fn record(lines: &[String]) -> String {
        format!("===== START\n{}===== END\n", lines.concat())
    }

    #[test]
    fn creates_updates_and_deletes_by_guid() {
        let (mut b, bank, food) = book();
        let (s1, s2) = ("4b9a929657604caaa5a5f57b2a4b1917", "b7fa927032c94a4ea19da098987e9b42");
        let commit = |desc: &str| record(&[line('C', desc, s1, &bank.to_string(), "-1234/100"), line('C', desc, s2, &food.to_string(), "1234/100")]);
        let text = format!("{HEAD}===== START\n===== END\n{}{}", commit("Groceries"), commit("Groceries edited"));
        let (records, skipped) = parse(&text).unwrap();
        assert!(skipped.is_empty());
        let report = replay(&mut b, &mut FixedEnv::new(Time64(0), 1 << 64), &records, false).unwrap();
        let guid = Guid::parse("65e21d8a290f4355b9555dfca9bfe888").unwrap();
        assert_eq!((report.created, report.updated), (vec![guid], vec![guid]));
        let t = b.transaction(guid).unwrap();
        assert_eq!(t.description.as_deref(), Some("Groceries edited"));
        assert_eq!(t.notes(), Some("a note"));
        assert!(t.splits[0].value.same_repr(Numeric::new(1234, 100)), "debit first");
        let delete = record(&[line('D', "Groceries edited", s1, &bank.to_string(), "-1234/100")]);
        let (records, _) = parse(&format!("{HEAD}{delete}{delete}")).unwrap();
        let report = replay(&mut b, &mut FixedEnv::new(Time64(0), 1 << 64), &records, false).unwrap();
        assert_eq!((report.deleted, report.not_found), (vec![guid], vec![guid]));
        assert!(b.transactions.is_empty());
    }

    #[test]
    fn unbalanced_and_partial_records_are_refused_or_skipped() {
        let (mut b, bank, food) = book();
        let unbalanced = record(&[line('C', "x", "4b9a929657604caaa5a5f57b2a4b1917", &bank.to_string(), "-5/1"), line('C', "x", "b7fa927032c94a4ea19da098987e9b42", &food.to_string(), "4/1")]);
        let (records, _) = parse(&format!("{HEAD}{unbalanced}")).unwrap();
        assert!(replay(&mut b.clone(), &mut FixedEnv::new(Time64(0), 1 << 64), &records, false).is_err());
        let report = replay(&mut b, &mut FixedEnv::new(Time64(0), 1 << 64), &records, true).unwrap();
        assert_eq!(report.skipped_records.len(), 1);
        assert!(b.transactions.is_empty());
        let tabbed = record(&[line('C', "a\tb", "4b9a929657604caaa5a5f57b2a4b1917", &bank.to_string(), "-5/1")]);
        let (records, skipped) = parse(&format!("{HEAD}{tabbed}")).unwrap();
        assert_eq!(skipped.len(), 1);
        let report = replay(&mut b, &mut FixedEnv::new(Time64(0), 1 << 64), &records, false).unwrap();
        assert_eq!(report.skipped_records.len(), 1);
        assert!(b.transactions.is_empty());
    }
}
