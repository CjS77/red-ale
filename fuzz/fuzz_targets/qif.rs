//! The QIF reader and every step after it that needs no Book: reading, the field parsers, the account check and
//! re-parsing dates in each format.

#![no_main]

use libfuzzer_sys::fuzz_target;
use red_ale_import::qif::parse::DATE_FORMATS;
use red_ale_import::qif::{ReadOptions, check_from_acct, parse_fields, read_file, reparse_dates, reparse_price_dates};

fuzz_target!(|data: &[u8]| {
    let text = String::from_utf8_lossy(data);
    let Ok(result) = read_file(&text, &ReadOptions::default()) else { return };
    let mut file = result.file;
    let _ = check_from_acct(&file);
    let _ = file.path_to_accountname();
    if let Ok(report) = parse_fields(&mut file) {
        let _ = report.date_formats();
    }
    DATE_FORMATS.iter().for_each(|format| {
        let mut copy = file.clone();
        let _ = reparse_dates(&mut copy, *format);
        let _ = reparse_price_dates(&mut copy, *format);
    });
});
