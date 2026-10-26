//! The fixed-width tokenizer.
//!
//! Translated from `5.17:gnucash/import-export/csv-imp/gnc-tokenizer-fw.cpp`.
//! Copyright (C) 2015 Geert Janssens <geert@kobaltwit.be>. GPL-2.0-or-later; this translation is GPL-3.0-or-later.
//!
//! Column widths count characters (GnuCash converts to wide characters first, `:182-197`), each field is trimmed of
//! C-locale whitespace, a line shorter than the columns gives fewer fields, and text past the last column is dropped
//! (boost's `offset_separator` without wrapping, `:194`).
//!
//! When no widths are set GnuCash starts from one column as wide as the longest line, counted in bytes
//! (`:140-158`). The column editing operations (`col_delete`, `col_narrow`, `col_widen`, `col_split`, `:53-137`) are
//! the assistant's mouse actions on the preview; red-ale takes the resulting widths in the layout instead.

use super::tokenizer::{Row, getline_split, trim_c_space};

/// `GncFwTokenizer::tokenize` (`gnc-tokenizer-fw.cpp:188-221`) over text whose line endings are already `\n`.
pub(crate) fn tokenize(text: &str, widths: &[u32]) -> Vec<Row> {
    let default = [longest_line(text)];
    let widths = if widths.is_empty() { &default[..] } else { widths };
    getline_split(text).map(|line| split_line(line, widths)).collect()
}

/// The longest line in bytes, as `GncFwTokenizer::load_file` measures it (`:144-153`).
fn longest_line(text: &str) -> u32 {
    getline_split(text).map(|line| u32::try_from(line.len()).unwrap_or(u32::MAX)).max().unwrap_or(0)
}

/// One line cut at the column offsets: a field per width while characters remain.
fn split_line(line: &str, widths: &[u32]) -> Row {
    let chars: Vec<char> = line.chars().collect();
    let mut rest = &chars[..];
    widths
        .iter()
        .map_while(|&width| {
            if rest.is_empty() {
                return None;
            }
            let take = usize::try_from(width).unwrap_or(usize::MAX).min(rest.len());
            let (field, tail) = rest.split_at(take);
            rest = tail;
            Some(trim_c_space(&field.iter().collect::<String>()).to_owned())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn widths_count_characters() {
        assert_eq!(tokenize("€12ab", &[1, 2, 5]), vec![vec!["€", "12", "ab"]]);
    }

    #[test]
    fn short_lines_give_fewer_fields_and_long_lines_are_cut() {
        assert_eq!(tokenize("abc\nabcdefgh", &[2, 2]), vec![vec!["ab", "c"], vec!["ab", "cd"]]);
    }

    #[test]
    fn no_widths_is_one_column() {
        assert_eq!(tokenize(" a b \nc", &[]), vec![vec!["a b"], vec!["c"]]);
    }

    #[test]
    fn empty_line_has_no_fields() {
        assert_eq!(tokenize("\nab", &[1, 1]), vec![vec![], vec!["a", "b"]]);
    }
}
