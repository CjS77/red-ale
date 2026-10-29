//! `help.search`: Pages ranked for a query with BM25F (BM25 with a weight and a length norm per field) over their titles,
//! method names, summaries and bodies, built once from the compiled-in corpus (`design/manual.md`, "Search"). Deterministic
//! and offline: the same corpus and query always give the same ranking, ties broken by Page ID.
//!
//! Scores are `f64`: they rank Pages and never leave this module, so the float ban on money does not reach them.

use std::collections::BTreeMap;

use crate::page::{Kind, Page};
use crate::synonyms::Synonym;

/// BM25's term-frequency saturation.
const K1: f64 = 1.2;
/// BM25's length normalisation.
const B: f64 = 0.75;
/// How much a word counts in each field.
const TITLE_WEIGHT: f64 = 3.0;
/// A method's name says most exactly what its Page is for, so it outweighs a title that only shares its words (an
/// error such as `book-already-open` against `book.open`).
const METHOD_WEIGHT: f64 = 6.0;
const SUMMARY_WEIGHT: f64 = 2.0;
const BODY_WEIGHT: f64 = 1.0;
/// How much a word a synonym brings in counts against one the query has.
const SYNONYM_WEIGHT: f64 = 0.5;

/// Words that tell Pages apart too little to index.
const STOP_WORDS: [&str; 24] = [
    "a", "an", "and", "are", "as", "at", "be", "by", "do", "for", "from", "how", "i", "in", "is", "it", "of", "on", "or", "the", "this",
    "to", "what", "with",
];

/// The words of a text, for synonyms: lower case, split at anything but letters and digits, plurals made singular,
/// stop words and single characters left out.
pub fn words(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.chars().nth(1).is_some())
        .map(|w| stem(&w.to_lowercase()))
        .filter(|w| !STOP_WORDS.contains(&w.as_str()))
        .collect()
}

/// The search terms of a text: its [`words`], each dotted name whole (`transaction.post`), and the parts of each
/// camelCase name (`closeBooks` gives `close` and `book`).
pub fn terms(text: &str) -> Vec<String> {
    let dotted = text
        .split(|c: char| !(c.is_alphanumeric() || c == '.'))
        .map(|w| w.trim_matches('.'))
        .filter(|w| w.contains('.'))
        .map(str::to_lowercase);
    let camel = text
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.chars().skip(1).any(char::is_uppercase))
        .flat_map(camel_parts)
        .map(|p| stem(&p.to_lowercase()))
        .filter(|w| !STOP_WORDS.contains(&w.as_str()));
    words(text).into_iter().chain(dotted).chain(camel).collect()
}

/// `closeBooks` as `close`, `Books`.
fn camel_parts(word: &str) -> Vec<String> {
    let mut parts: Vec<String> = Vec::new();
    word.chars().for_each(|c| match parts.last_mut() {
        Some(last) if !c.is_uppercase() => last.push(c),
        _ => parts.push(c.to_string()),
    });
    parts
}

/// A light English stemmer: plurals to singular (`entries` to `entry`, `books` to `book`).
fn stem(word: &str) -> String {
    let n = word.chars().count();
    if n > 4
        && let Some(base) = word.strip_suffix("ies")
    {
        return format!("{base}y");
    }
    let keeps_s = ["ss", "us", "is"].iter().any(|e| word.ends_with(e));
    match word.strip_suffix('s') {
        Some(base) if n > 3 && !keeps_s => base.to_owned(),
        _ => word.to_owned(),
    }
}

/// The fields of a Page that are searched, in the order of [`WEIGHTS`].
const FIELDS: usize = 4;
/// How much a word counts in each field: title, method name, summary, body.
const WEIGHTS: [f64; FIELDS] = [TITLE_WEIGHT, METHOD_WEIGHT, SUMMARY_WEIGHT, BODY_WEIGHT];

/// Per field, a number: a term's frequency, or a length.
type PerField = [f64; FIELDS];

/// The search index of a set of Pages, for BM25F: each field's term frequencies are normalised by that field's
/// length, so a long body does not bury a match in the title.
#[derive(Debug, Clone)]
pub(crate) struct Index {
    /// Each Page's kind and field lengths, in the order the Pages were given.
    docs: Vec<(Kind, PerField)>,
    /// For each term, the Pages holding it with its frequency in each field.
    postings: BTreeMap<String, Vec<(usize, PerField)>>,
    /// The average length of each field.
    average: PerField,
    synonyms: Vec<Synonym>,
}

impl Index {
    /// Index `pages`; results refer to them by position.
    pub fn build(pages: &[&Page], synonyms: Vec<Synonym>) -> Self {
        let mut postings: BTreeMap<String, Vec<(usize, PerField)>> = BTreeMap::new();
        let docs: Vec<(Kind, PerField)> = pages
            .iter()
            .enumerate()
            .map(|(i, page)| {
                let (counts, lengths) = field_terms(page);
                counts
                    .into_iter()
                    .for_each(|(term, tf)| postings.entry(term).or_default().push((i, tf)));
                (page.kind, lengths)
            })
            .collect();
        let average = average_lengths(&docs);
        Self {
            docs,
            postings,
            average,
            synonyms,
        }
    }

    /// The positions of the Pages that match `query`, best first, of `kind` if given, at most `limit`.
    pub fn search(&self, query: &str, kind: Option<Kind>, limit: usize) -> Vec<usize> {
        let mut scores = vec![0.0_f64; self.docs.len()];
        self.query_terms(query)
            .iter()
            .for_each(|(term, weight)| self.score(term, *weight, &mut scores));
        let mut ranked: Vec<(usize, f64)> = scores
            .into_iter()
            .enumerate()
            .filter(|(i, s)| *s > 0.0 && kind.is_none_or(|k| self.docs[*i].0 == k))
            .collect();
        ranked.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
        ranked.into_iter().take(limit).map(|(i, _)| i).collect()
    }

    /// The query's terms with their weights, then the words its synonyms bring in, at a lower weight.
    pub fn query_terms(&self, query: &str) -> Vec<(String, f64)> {
        let mut out: Vec<(String, f64)> = Vec::new();
        let mut add = |term: String, weight: f64| {
            if !out.iter().any(|(t, _)| *t == term) {
                out.push((term, weight));
            }
        };
        terms(query).into_iter().for_each(|t| add(t, 1.0));
        let said = words(query);
        self.synonyms
            .iter()
            .filter(|s| said.windows(s.phrase.len()).any(|w| w == s.phrase.as_slice()))
            .flat_map(|s| s.term.iter().cloned())
            .for_each(|t| add(t, SYNONYM_WEIGHT));
        out
    }

    fn score(&self, term: &str, weight: f64, scores: &mut [f64]) {
        let Some(postings) = self.postings.get(term) else { return };
        let n = count(self.docs.len());
        let df = count(postings.len());
        let idf = ((n - df + 0.5) / (df + 0.5) + 1.0).ln();
        postings.iter().for_each(|(doc, tf)| {
            let tf = self.weighted_frequency(tf, &self.docs[*doc].1);
            scores[*doc] += weight * idf * (tf * (K1 + 1.0)) / (tf + K1);
        });
    }

    /// BM25F's pseudo-frequency: each field's frequency by its weight, normalised by its length.
    fn weighted_frequency(&self, tf: &PerField, lengths: &PerField) -> f64 {
        (0..FIELDS)
            .filter(|f| tf[*f] > 0.0)
            .map(|f| {
                let relative = if self.average[f] > 0.0 { lengths[f] / self.average[f] } else { 1.0 };
                WEIGHTS[f] * tf[f] / (1.0 - B + B * relative)
            })
            .sum()
    }
}

/// The average length of each field over the Pages that have it. Only Reference pages of methods have a method
/// name, so averaging it over every Page would make each method's name look long, and its matches weak.
fn average_lengths(docs: &[(Kind, PerField)]) -> PerField {
    let mut average = [0.0; FIELDS];
    (0..FIELDS).for_each(|f| {
        let present: Vec<f64> = docs.iter().map(|(_, lengths)| lengths[f]).filter(|l| *l > 0.0).collect();
        average[f] = present.iter().sum::<f64>() / count(present.len()).max(1.0);
    });
    average
}

/// A count as `f64`; Manual sizes are far below where that loses precision.
fn count(n: usize) -> f64 {
    u32::try_from(n).map_or(f64::from(u32::MAX), f64::from)
}

/// Each term of a Page with its frequency in each field (title, method name, summary, body), and each field's
/// length in terms.
fn field_terms(page: &Page) -> (BTreeMap<String, PerField>, PerField) {
    let method = if page.kind == Kind::Method { page.title.as_str() } else { "" };
    let fields = [page.title.as_str(), method, page.summary.as_str(), page.body.as_str()];
    let mut counts: BTreeMap<String, PerField> = BTreeMap::new();
    let mut lengths = [0.0; FIELDS];
    fields.into_iter().enumerate().for_each(|(f, text)| {
        terms(text).into_iter().for_each(|t| {
            counts.entry(t).or_insert([0.0; FIELDS])[f] += 1.0;
            lengths[f] += 1.0;
        });
    });
    (counts, lengths)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(id: &str, kind: Kind, title: &str, summary: &str, body: &str) -> Page {
        Page {
            id: id.into(),
            kind,
            title: title.into(),
            summary: summary.into(),
            related: Vec::new(),
            from: None,
            body: body.into(),
            hand_written: None,
        }
    }

    #[test]
    fn words_are_normalised() {
        assert_eq!(words("The Entries of Books, a class"), ["entry", "book", "class"]);
        assert_eq!(
            terms("Use transaction.post or closeBooks."),
            ["use", "transaction", "post", "closebook", "transaction.post", "close", "book"]
        );
    }

    #[test]
    fn ranking_prefers_titles_and_is_stable() {
        let pages = [
            page(
                "method/transaction.post",
                Kind::Method,
                "transaction.post",
                "Post a transaction.",
                "Splits balance.",
            ),
            page(
                "concept/value",
                Kind::Concept,
                "Value and amount",
                "A split's sizes.",
                "A transaction's splits.",
            ),
            page("concept/other", Kind::Concept, "Other", "Nothing.", "Nothing at all."),
        ];
        let refs: Vec<&Page> = pages.iter().collect();
        let index = Index::build(&refs, Vec::new());
        assert_eq!(index.search("transaction", None, 10), [0, 1]);
        assert_eq!(index.search("transaction.post", None, 10), [0, 1]);
        assert_eq!(index.search("transaction", Some(Kind::Concept), 10), [1]);
        assert_eq!(index.search("transaction", None, 1), [0]);
        assert!(index.search("zebra", None, 10).is_empty());
    }

    #[test]
    fn a_long_reference_page_ranks_first_for_its_method_name() {
        let filler = |i: usize| format!("Account {i} lists its splits and their values, in the account's commodity. ");
        let long_body: String = (0..30).map(filler).collect::<String>() + "Open the book, then book the book. A book open.";
        let mut pages = vec![
            page(
                "method/book.open",
                Kind::Method,
                "book.open",
                "Open an XML or SQLite book.",
                &long_body,
            ),
            page(
                "error/book-already-open",
                Kind::Error,
                "book-already-open",
                "A book is already open in this server.",
                "Close the open book first. One server holds one book.",
            ),
        ];
        (0..20).for_each(|i| {
            pages.push(page(
                &format!("concept/c{i}"),
                Kind::Concept,
                &format!("Concept {i}"),
                "A concept.",
                &filler(i),
            ));
        });
        let refs: Vec<&Page> = pages.iter().collect();
        let index = Index::build(&refs, Vec::new());
        assert_eq!(index.search("open a book", None, 2), [0, 1]);
        assert_eq!(index.search("book-already-open", None, 1), [1]);
    }

    #[test]
    fn synonyms_find_the_term() {
        let pages = [page(
            "concept/amount",
            Kind::Concept,
            "Amount",
            "A split's size in its account's commodity.",
            "",
        )];
        let refs: Vec<&Page> = pages.iter().collect();
        let synonyms = vec![Synonym {
            phrase: vec!["quantity".into()],
            term: vec!["amount".into()],
        }];
        let index = Index::build(&refs, synonyms);
        assert_eq!(index.search("quantity", None, 10), [0]);
        assert_eq!(
            index.query_terms("quantity"),
            [("quantity".to_owned(), 1.0), ("amount".to_owned(), 0.5)]
        );
    }
}
