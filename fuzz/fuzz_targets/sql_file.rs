//! Whole SQLite files: the fuzzer's bytes, written to a file, opened read-only as `book.open` would
//! (`design/sql-fidelity.md` §9, "Fuzzing"). Seed it with copies of the SQLite fixtures, so mutations land in real
//! pages, rows and schema. Nothing may panic; every refusal is a finding or an error.

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // One file per fuzzing process: libFuzzer runs inputs one after another in each.
    let path = std::env::temp_dir().join(format!("red-ale-fuzz-sql-file-{}.gnucash", std::process::id()));
    if std::fs::write(&path, data).is_ok() {
        drop(red_ale_sql::open_read_only(&path));
    }
    drop(std::fs::remove_file(&path));
});
