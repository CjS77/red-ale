//! The report result is a published language: its JSON Schema changes only on purpose. Regenerate the snapshot with
//! `UPDATE_SNAPSHOTS=1 cargo test -p red-ale-report --test result_schema` and review the diff.

use std::path::Path;

use red_ale_report::ReportResult;

#[test]
fn the_report_result_schema_matches_its_snapshot() {
    let schema = serde_json::to_string_pretty(&ReportResult::json_schema()).unwrap() + "\n";
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/snapshots/report-result.schema.json");
    if std::env::var_os("UPDATE_SNAPSHOTS").is_some() {
        std::fs::write(&path, &schema).unwrap();
    }
    let expected = std::fs::read_to_string(&path).unwrap();
    assert!(
        schema == expected,
        "the ReportResult schema changed; regenerate the snapshot with UPDATE_SNAPSHOTS=1 and review it"
    );
}
