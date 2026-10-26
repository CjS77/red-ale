//! No input makes the QIF import panic: arbitrary and mutated files through the whole preview, then applied with
//! their error rows left out, then previewed again over what was applied (`design/milestone3.md`, "Testing").

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use proptest::prelude::*;
use red_ale_core::{Book, CommodityRef, FixedEnv, ReconcileState, Time64, TimeZone};
use red_ale_import::pipeline::{self, ApplyChoices, Change};
use red_ale_import::qif::parse::DATE_FORMATS;
use red_ale_import::qif::guess_map::SavedMap;
use red_ale_import::qif::import::{QifFileInput, QifOptions, propose};
use red_ale_import::matcher::ImportAction;

/// 2024-07-01 12:00 UTC.
const NOW: Time64 = Time64(1_719_835_200);

/// Two valid files touching every kind of record, with a transfer between them, to mutate.
const CHECKING: &str = "!Account\nNChecking\nTBank\n^\n!Type:Bank\nD03/14/2024\nT-500.00\nPTo broker\nL[Broker]\n^\n\
D03/15/2024\nT-100.00\nPDinner\nLDining\nSDining\n$-60.00\nSGroceries\n$-40.00\n^\nD03/16/2024\nT1,000.00\nPOpening Balance\nL[Checking]\n^\n\
D03/17/2024\nT-1.00\nP**VOID**Cheque\nLFees\n^\n";
const BROKER: &str = "!Account\nNBroker\nTInvst\n^\n!Type:Security\nNAcme\nSACM\nTStock\n^\n!Type:Invst\nD03/14/2024\nNXIn\nT500.00\nL[Checking]\n^\n\
D03/15/2024\nNBuy\nYAcme\nI10.50\nQ40\nT429.95\nO9.95\n^\nD03/16/2024\nNStkSplit\nYAcme\nQ20\n^\nD03/17/2024\nNDivX\nYAcme\nT5.00\nL[Checking]\n^\n\
D03/18/2024\nNShrsOut\nYAcme\nQ10\n^\nD03/19/2024\nNMiscExpX\nYAcme\nT1.00\nL[Checking]|Fees\n^\n\
!Type:Prices\n\"ACM\",11 1/2,\"03/20/2024\"\n^\n!Type:Prices\n\"ACM\",12,\"03/20/2024\"\n^\n";

fn options() -> QifOptions {
    QifOptions {
        date_format: None,
        currency: CommodityRef::currency("USD"),
        status_pref: ReconcileState::New,
        accounts: Default::default(),
        categories: Default::default(),
        payees: Default::default(),
        securities: Default::default(),
        saved: SavedMap::default(),
    }
}

/// Preview, apply skipping the first transaction, preview again; with every date format the dates allow.
fn exercise(texts: &[String]) {
    let mut env = FixedEnv::new(NOW, 1 << 100);
    let mut book = Book::new_empty(&mut env);
    let inputs: Vec<QifFileInput> =
        texts.iter().enumerate().map(|(i, t)| QifFileInput { text: t.clone(), path: Some(format!("file-{i}.qif")), default_account: None }).collect();
    let zone = TimeZone::utc();
    let Ok(first) = propose(&book, NOW, &zone, &inputs, &options(), None) else { return };
    let formats = first.qif.as_ref().map(|q| q.date_formats.clone()).unwrap_or_default();
    let mut proposals = vec![first];
    proposals.extend(formats.iter().filter_map(|f| propose(&book, NOW, &zone, &inputs, &QifOptions { date_format: Some(*f), ..options() }, None).ok()));
    proposals.iter().for_each(|p| {
        let _ = pipeline::review(&book, NOW, &zone, p);
        let skip = [Change { index: 0, action: Some(ImportAction::Skip), ..Change::default() }];
        let changes = if p.transactions.is_empty() { &[][..] } else { &skip[..] };
        let mut copy = book.clone();
        let _ = pipeline::apply(&mut copy, &mut env, p, changes, ApplyChoices { skip_errors: true, append_text: None });
    });
    if let Some(p) = proposals.last()
        && pipeline::apply(&mut book, &mut env, p, &[], ApplyChoices { skip_errors: true, append_text: None }).is_ok()
    {
        let _ = propose(&book, NOW, &zone, &inputs, &QifOptions { date_format: DATE_FORMATS.first().copied(), ..options() }, None);
        let _ = propose(&book, NOW, &zone, &inputs, &options(), None);
    }
}

/// One edit to a list of lines.
#[derive(Debug, Clone)]
enum Mutation {
    Delete(usize),
    Duplicate(usize),
    Swap(usize, usize),
    Replace(usize, String),
    InsertChar(usize, usize, char),
}

fn mutation() -> impl Strategy<Value = Mutation> {
    let line = 0..60_usize;
    prop_oneof![
        line.clone().prop_map(Mutation::Delete),
        line.clone().prop_map(Mutation::Duplicate),
        (line.clone(), line.clone()).prop_map(|(a, b)| Mutation::Swap(a, b)),
        (line.clone(), "[!A-Z$^\\[\\]/|0-9.,' *-]{0,12}").prop_map(|(i, s)| Mutation::Replace(i, s)),
        (line, 0..20_usize, any::<char>()).prop_map(|(i, at, c)| Mutation::InsertChar(i, at, c)),
    ]
}

fn mutate(text: &str, mutations: &[Mutation]) -> String {
    let mut lines: Vec<String> = text.lines().map(str::to_owned).collect();
    mutations.iter().for_each(|m| {
        let len = lines.len().max(1);
        match m {
            _ if lines.is_empty() => {}
            Mutation::Delete(i) => {
                lines.remove(i % len);
            }
            Mutation::Duplicate(i) => lines.insert(i % len, lines[i % len].clone()),
            Mutation::Swap(a, b) => lines.swap(a % len, b % len),
            Mutation::Replace(i, s) => lines[i % len].clone_from(s),
            Mutation::InsertChar(i, at, c) => {
                let line = &mut lines[i % len];
                let at = line.char_indices().nth(*at).map_or(line.len(), |(b, _)| b);
                line.insert(at, *c);
            }
        }
    });
    lines.join("\n")
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn arbitrary_text(text in any::<String>()) {
        exercise(&[text]);
    }

    #[test]
    fn qif_shaped_text(text in "(![A-Za-z: ]{0,14}\n|[A-Z$^!][-0-9/.,' $\\[\\]|A-Za-z*]{0,14}\n){0,40}") {
        exercise(&[text]);
    }

    #[test]
    fn mutated_valid_files(a in prop::collection::vec(mutation(), 0..8), b in prop::collection::vec(mutation(), 0..8)) {
        exercise(&[mutate(CHECKING, &a), mutate(BROKER, &b)]);
    }
}

#[test]
fn the_seeds_import_whole() {
    let mut env = FixedEnv::new(NOW, 1 << 100);
    let mut book = Book::new_empty(&mut env);
    let inputs: Vec<QifFileInput> =
        [CHECKING, BROKER].iter().map(|t| QifFileInput { text: (*t).to_owned(), path: None, default_account: None }).collect();
    let p = propose(&book, NOW, &TimeZone::utc(), &inputs, &options(), None).unwrap();
    assert!(p.errors.is_empty(), "{:?}", p.errors);
    assert_eq!(p.transactions.len(), 9, "the transfer and the dividend's transfer are each imported once");
    let applied = pipeline::apply(&mut book, &mut env, &p, &[], ApplyChoices::default()).unwrap();
    assert_eq!(applied.added.len(), 9);
    assert!(applied.qif_account_map.is_some());
}
