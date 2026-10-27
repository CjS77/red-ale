//! Formulas: the arithmetic-only mode, formulas with variables and the financial functions, and the free-variable
//! listing, in three number formats.

#![no_main]

use libfuzzer_sys::fuzz_target;
use red_ale_core::Numeric;
use red_ale_core::formula::{NumberFormat, Variables, evaluate, evaluate_arithmetic, free_variables};

const FORMATS: [NumberFormat; 3] = [
    NumberFormat { decimal_point: '.', group_separator: ',' },
    NumberFormat { decimal_point: ',', group_separator: '.' },
    NumberFormat { decimal_point: ',', group_separator: ' ' },
];

fuzz_target!(|data: &[u8]| {
    let Some((first, rest)) = data.split_first() else { return };
    let text = String::from_utf8_lossy(rest);
    let format = FORMATS[usize::from(first % 3)];
    let value = Numeric::new(i64::from(*first) - 128, 1 + i64::from(first >> 4));
    let vars: Variables = ["a", "b", "i", "rate", "x"].iter().map(|v| ((*v).to_owned(), value)).collect();
    let _ = evaluate_arithmetic(&text, format);
    let _ = evaluate(&text, format, &vars);
    let _ = free_variables(&text, format);
});
