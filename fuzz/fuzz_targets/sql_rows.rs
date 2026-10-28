//! SQLite rows: arbitrary cells (NULL, INTEGER, REAL, TEXT, BLOB) to every table's row decoder, the decoded slot rows
//! to the Frame builder, and `versions` rows to the version check (`design/sql-fidelity.md` §9, "Fuzzing"). None may
//! panic; every refusal is a finding.

#![no_main]

use std::collections::HashMap;

use libfuzzer_sys::fuzz_target;
use red_ale_core::{CommodityRef, Guid};
use red_ale_sql::cell::{Cell, Row, Table};
use red_ale_sql::rows::{self, Lookup, business};
use red_ale_sql::slots::{self, Owner};
use red_ale_sql::{schema, tables};

/// Reads cells from the fuzzer's bytes.
struct Input<'a>(&'a [u8]);

impl Input<'_> {
    fn byte(&mut self) -> u8 {
        let (first, rest) = self.0.split_first().map_or((0, &[][..]), |(f, r)| (*f, r));
        self.0 = rest;
        first
    }

    fn bytes(&mut self, len: usize) -> Vec<u8> {
        let (taken, rest) = self.0.split_at(len.min(self.0.len()));
        self.0 = rest;
        taken.to_vec()
    }

    fn eight(&mut self) -> [u8; 8] {
        let mut out = [0; 8];
        out.iter_mut().for_each(|b| *b = self.byte());
        out
    }

    /// A text often in a form the decoders accept: a GUID, a timestamp, a date or a name.
    fn text(&mut self) -> Vec<u8> {
        const SAMPLES: [&str; 8] = [
            "0123456789abcdef0123456789abcdef",
            "fedcba9876543210fedcba9876543210",
            "2024-06-03 10:59:00",
            "20240603105900",
            "20240229",
            "month",
            "GNC_TERM_TYPE_DAYS",
            "",
        ];
        let pick = self.byte();
        match SAMPLES.get(usize::from(pick % 16)) {
            Some(sample) => sample.as_bytes().to_vec(),
            None => {
                let len = usize::from(self.byte() % 40);
                self.bytes(len)
            }
        }
    }

    fn cell(&mut self) -> Cell {
        match self.byte() % 6 {
            0 => Cell::Null,
            1 => Cell::Integer(i64::from(self.byte() % 12) - 2),
            2 => Cell::Integer(i64::from_le_bytes(self.eight())),
            3 => Cell::Real(f64::from_bits(u64::from_le_bytes(self.eight()))),
            4 => Cell::Text(self.text()),
            _ => {
                let len = usize::from(self.byte() % 8);
                Cell::Blob(self.bytes(len))
            }
        }
    }

    fn row(&mut self, table: &Table, rowid: i64) -> Row {
        Row {
            rowid,
            cells: table.columns.iter().map(|_| self.cell()).collect(),
        }
    }
}

fn lookup() -> Lookup {
    let mut commodities = HashMap::new();
    if let Ok(g) = "fedcba9876543210fedcba9876543210".parse::<Guid>() {
        commodities.insert(g, CommodityRef::currency("USD"));
    }
    Lookup { commodities }
}

fuzz_target!(|data: &[u8]| {
    let mut input = Input(data);
    let lookup = lookup();
    let mut findings = Vec::new();
    let mut slot_rows = Vec::new();
    let mut versions = Vec::new();
    let mut rowid = 0;
    while !input.0.is_empty() {
        rowid += 1;
        let f = &mut findings;
        match input.byte() % 24 {
            0 => drop(rows::book(&input.row(&tables::BOOKS, rowid), f)),
            1 => drop(rows::commodity(&input.row(&tables::COMMODITIES, rowid), f)),
            2 => drop(rows::account(&input.row(&tables::ACCOUNTS, rowid), &lookup, f)),
            3 => drop(rows::lot(&input.row(&tables::LOTS, rowid), f)),
            4 => drop(rows::transaction(&input.row(&tables::TRANSACTIONS, rowid), &lookup, f)),
            5 => drop(rows::split(&input.row(&tables::SPLITS, rowid), f)),
            6 => drop(rows::price(&input.row(&tables::PRICES, rowid), &lookup, f)),
            7 => drop(rows::budget(&input.row(&tables::BUDGETS, rowid), f)),
            8 => drop(rows::budget_amount(&input.row(&tables::BUDGET_AMOUNTS, rowid), f)),
            9 => drop(rows::recurrence(&input.row(&tables::RECURRENCES, rowid), f)),
            10 => drop(rows::scheduled(&input.row(&tables::SCHEDXACTIONS, rowid), f)),
            11 | 12 => slot_rows.extend(rows::slot(&input.row(&tables::SLOTS, rowid), f)),
            13 => drop(business::bill_term(&input.row(&tables::BILLTERMS, rowid), f)),
            14 => drop(business::customer(&input.row(&tables::CUSTOMERS, rowid), &lookup, f)),
            15 => drop(business::employee(&input.row(&tables::EMPLOYEES, rowid), &lookup, f)),
            16 => drop(business::entry(&input.row(&tables::ENTRIES, rowid), f)),
            17 => drop(business::invoice(&input.row(&tables::INVOICES, rowid), &lookup, f)),
            18 => drop(business::job(&input.row(&tables::JOBS, rowid), f)),
            19 => drop(business::order(&input.row(&tables::ORDERS, rowid), f)),
            20 => drop(business::tax_table(&input.row(&tables::TAXTABLES, rowid), f)),
            21 => drop(business::tax_table_entry(&input.row(&tables::TAXTABLE_ENTRIES, rowid), f)),
            22 => drop(business::vendor(&input.row(&tables::VENDORS, rowid), &lookup, f)),
            _ => versions.push(input.row(&schema::VERSIONS_TABLE, rowid)),
        }
    }
    let entity = |g: Guid| match g.as_bytes()[0] {
        0x01 => Owner::Entity,
        0xfe => Owner::NoSlots,
        _ => Owner::Unknown,
    };
    let _ = slots::build(&slot_rows, &entity, &mut findings);
    let _ = schema::check_versions(&versions, &mut findings);
});
