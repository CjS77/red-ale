//! The report fixture books are what their scripts build: rerunning every script in `scripts/report-fixtures/` with
//! the GnuCash 5.17 oracle gives the committed books byte for byte, and each script's balance checks pass in GnuCash.

#![cfg(feature = "oracle")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;

use red_ale_testkit::{fixture, oracle, report_fixtures};

const SCRIPTS: [&str; 13] = [
    "account_summary",
    "balsheet_pnl",
    "average_balance",
    "charts",
    "net_barchart",
    "category_report",
    "budget",
    "adjusting_entries",
    "cash_flow",
    "balsheet_pnl_extras",
    "stock_test_data",
    "ifrs_cost_basis",
    "investment_lots",
];

#[test]
fn the_scripts_rebuild_the_committed_books() {
    oracle::check_version().unwrap();
    let out = tempfile::tempdir().unwrap();
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/report-fixtures");
    SCRIPTS.iter().for_each(|script| {
        oracle::run_script(&dir.join(format!("{script}.py")), &[&out.path().display().to_string()]).unwrap_or_else(|e| panic!("{e}"));
    });
    let mut built: Vec<String> = std::fs::read_dir(out.path())
        .unwrap()
        .map(|e| format!("reports/{}", e.unwrap().file_name().to_string_lossy()))
        .collect();
    built.sort();
    assert_eq!(built, report_fixtures());
    let stale: Vec<&String> = built
        .iter()
        .filter(|name| std::fs::read(out.path().join(&name["reports/".len()..])).unwrap() != std::fs::read(fixture(name)).unwrap())
        .collect();
    assert!(stale.is_empty(), "rebuilt books differ from the committed ones: {stale:?}");
}
