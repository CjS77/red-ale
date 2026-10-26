//! CSV from a report result's export rows, as GnuCash's `gnc:lists->csv` writes it
//! (`5.17:gnucash/report/trep-engine.scm:463-512`): each row padded with empty cells to the longest, text quoted
//! with its quotes doubled, numbers as Guile prints them once inexact, empty cells as nothing, rows joined by `\n`
//! with none after the last.

use crate::number::guile_inexact;
use crate::result::{ExportCell, ExportRows};

/// The CSV text of `rows`.
pub fn csv(rows: &ExportRows) -> String {
    let width = rows.iter().map(Vec::len).max().unwrap_or(0);
    rows.iter().map(|row| line(row, width)).collect::<Vec<_>>().join("\n")
}

/// One row: its cells joined by commas, then a comma for each cell it lacks (`strify`'s padding, `:494-502`).
fn line(row: &[Option<ExportCell>], width: usize) -> String {
    let cells: Vec<String> = row.iter().map(|c| cell(c.as_ref())).collect();
    format!("{}{}", cells.join(","), ",".repeat(width.saturating_sub(row.len())))
}

/// `strify` (`:490-507`): an empty cell is nothing, text is quoted, a number is `(number->string (exact->inexact n))`.
fn cell(c: Option<&ExportCell>) -> String {
    match c {
        None => String::new(),
        Some(ExportCell::Text(s)) => format!("\"{}\"", s.replace('"', "\"\"")),
        Some(ExportCell::Number(n)) => guile_inexact(n),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::number::Rational;

    #[allow(clippy::unnecessary_wraps)]
    fn text(s: &str) -> Option<ExportCell> {
        Some(ExportCell::Text(s.to_owned()))
    }

    #[allow(clippy::unnecessary_wraps)]
    fn num(s: &str) -> Option<ExportCell> {
        Some(ExportCell::Number(Rational::parse(s).unwrap()))
    }

    /// GnuCash's own cases (`5.17:gnucash/report/reports/standard/test/test-transaction.scm:1053-1066`); its symbol
    /// cell, written bare, has no counterpart in red-ale's export cells.
    #[test]
    fn gnucash_lists_to_csv_cases() {
        assert_eq!(csv(&vec![vec![]]), "");
        assert_eq!(csv(&vec![vec![text("from"), text("01/01/2010")]]), "\"from\",\"01/01/2010\"");
        let rows = vec![
            vec![text("from"), text("01/01/2010")],
            vec![text("to"), text("31/12/2010")],
            vec![text("total"), num("23500"), num("30000"), num("25/7"), None],
        ];
        assert_eq!(csv(&rows), "\"from\",\"01/01/2010\",,,\n\"to\",\"31/12/2010\",,,\n\"total\",23500.0,30000.0,3.5714285714285716,");
    }

    #[test]
    fn quotes_are_doubled() {
        assert_eq!(csv(&vec![vec![text("say \"hi\", then"), num("-12.34")]]), "\"say \"\"hi\"\", then\",-12.34");
    }
}
