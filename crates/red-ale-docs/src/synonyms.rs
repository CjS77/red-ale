//! The synonym table of `help.search`: GnuCash's words, and other words users say, mapped to the Manual's terms
//! (`design/manual.md`, "Search"; `design/milestone6.md`, "Decided", item 7).
//!
//! Most of it is generated from the glossary's ⚠ entries, which are where a term's other names are recorded: in a
//! ⚠ note, every *italic* phrase and `code` span names the term another way ("GnuCash calls it the *transaction
//! log*"). A sentence starting "Not" says what the term is not, so it is skipped. The rest is a short hand-kept list,
//! `docs/synonyms.txt`, for words the glossary doesn't record.
//!
//! The glossary is compiled in and read when the Manual is built, rather than copied into `docs/` by a generator: so
//! the table cannot fall behind the glossary, there is no generated file to keep in step, and the parser is plain
//! code that unit tests reach.

use crate::search::words as tokens;

/// The glossary, `design/glossary.md`.
const GLOSSARY: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../design/glossary.md"));

/// The hand-kept supplement, `docs/synonyms.txt`.
const SUPPLEMENT: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/synonyms.txt"));

/// One synonym: the words of a phrase, and the words of the term it means.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Synonym {
    /// The phrase, as search tokens.
    pub phrase: Vec<String>,
    /// The term, as search tokens.
    pub term: Vec<String>,
}

/// The synonym table: the glossary's and the supplement's, sorted, without repeats.
pub fn table() -> Vec<Synonym> {
    let mut all: Vec<Synonym> = from_glossary(GLOSSARY).into_iter().chain(from_supplement(SUPPLEMENT)).collect();
    all.sort();
    all.dedup();
    all
}

/// The synonyms the ⚠ notes of a glossary give.
pub fn from_glossary(glossary: &str) -> Vec<Synonym> {
    glossary
        .lines()
        .filter_map(row)
        .flat_map(|(term, meaning)| row_synonyms(term, meaning))
        .collect()
}

/// A glossary table row's term (its first bold text) and the rest of the row.
fn row(line: &str) -> Option<(&str, &str)> {
    let cells = line.strip_prefix("| **")?;
    let (term, rest) = cells.split_once("**")?;
    Some((term, rest))
}

fn row_synonyms(term: &str, rest: &str) -> Vec<Synonym> {
    let term_tokens = tokens(term);
    let (name_cell, meaning) = rest.split_once(" | ").unwrap_or((rest, ""));
    let code_name = code_spans(name_cell);
    let notes = meaning
        .split('⚠')
        .skip(1)
        .flat_map(sentences)
        .filter(|s| !s.trim_start().starts_with("Not "));
    let phrases = notes.flat_map(|s| [italics(&s), code_spans(&s)].concat());
    let has_notes = meaning.contains('⚠');
    let names = if has_notes { code_name } else { Vec::new() };
    names
        .into_iter()
        .chain(phrases)
        .map(|p| tokens(&p))
        .filter(|p| !p.is_empty() && p.len() <= 4 && *p != term_tokens)
        .map(|phrase| Synonym {
            phrase,
            term: term_tokens.clone(),
        })
        .collect()
}

/// The sentences of a ⚠ note, split at `.`, `;` and the end of the cell.
fn sentences(note: &str) -> Vec<String> {
    let note = note.split(" | ").next().unwrap_or(note).replace("**", "");
    note.split(". ").flat_map(|s| s.split("; ")).map(str::to_owned).collect()
}

/// The `*italic*` phrases of a text with no bold markers left.
fn italics(text: &str) -> Vec<String> {
    delimited(text, '*')
}

/// The `` `code` `` spans of a text.
fn code_spans(text: &str) -> Vec<String> {
    delimited(text, '`')
}

/// The text between pairs of `mark`.
fn delimited(text: &str, mark: char) -> Vec<String> {
    text.split(mark)
        .enumerate()
        .filter(|(i, _)| i % 2 == 1)
        .map(|(_, s)| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .collect()
}

/// The supplement's lines, `phrase => term`; blank lines and lines starting with `#` are skipped.
pub fn from_supplement(text: &str) -> Vec<Synonym> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(|l| l.split_once("=>"))
        .map(|(phrase, term)| Synonym {
            phrase: tokens(phrase),
            term: tokens(term),
        })
        .filter(|s| !s.phrase.is_empty() && !s.term.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pairs(synonyms: &[Synonym]) -> Vec<(String, String)> {
        synonyms.iter().map(|s| (s.phrase.join(" "), s.term.join(" "))).collect()
    }

    #[test]
    fn warnings_give_synonyms() {
        let glossary = "| Term | Meaning |\n\
            | **Journal** | GnuCash's `.log` file. **⚠** GnuCash calls it the *transaction log* (`TransLog`); red-ale says *journal*. |\n\
            | **Closing the books** (`book.closeBooks`) | Close Book. **⚠** Not closing a Session (`book.close`): the books stay open. |\n\
            | **Amount** | A Split's size. **⚠** GnuCash *amount* means this quantity, not a generic number; the XML calls it `quantity`. |\n\
            | **Plain** (`plainCode`) | No warning, *no synonyms*. |\n";
        let found = pairs(&from_glossary(glossary));
        assert_eq!(
            found,
            [
                ("transaction log".to_owned(), "journal".to_owned()),
                ("translog".to_owned(), "journal".to_owned()),
                ("book closebook".to_owned(), "closing book".to_owned()),
                ("quantity".to_owned(), "amount".to_owned()),
            ]
        );
    }

    #[test]
    fn the_supplement_maps_phrases() {
        let found = pairs(&from_supplement("# comment\n\nbill => business document\nbad line\n"));
        assert_eq!(found, [("bill".to_owned(), "business document".to_owned())]);
    }

    #[test]
    fn the_real_glossary_maps_gnucash_words() {
        let found = pairs(&table());
        assert!(found.contains(&("quantity".to_owned(), "amount".to_owned())), "{found:?}");
        assert!(found.contains(&("transaction log".to_owned(), "journal".to_owned())), "{found:?}");
        assert!(found.contains(&("bill".to_owned(), "business document".to_owned())), "{found:?}");
    }
}
