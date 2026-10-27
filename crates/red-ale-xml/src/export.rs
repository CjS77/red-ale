//! Checking an account-tree export ([`crate::book::write_accounts_to`]) by reading it back.
//!
//! The file GnuCash's Export Accounts writes holds no `gnc:book` element (`5.17:libgnucash/backend/xml/io-gncxml-v2.cpp:1340-1374`),
//! so it is not a book [`crate::read_back`] reads. It holds only commodities and accounts, which are small, so it is
//! parsed whole here and each entity compared with the Book it came from.

use std::collections::HashSet;
use std::io::Read;

use red_ale_core::{Book, Guid};

use crate::XmlError;
use crate::dom::{Element, parse_document};
use crate::entities::{commodity_is_written, parse_account, parse_commodity};
use crate::values::Notices;

/// What an account-tree export holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ExportCounts {
    /// Commodity elements.
    pub commodities: usize,
    /// Account elements, the root account's included.
    pub accounts: usize,
}

/// Read an account-tree export back and compare it with `book`: every commodity a full save would write and every
/// account of the tree, each equal to the Book's, and nothing else. Returns the counts, or the first difference.
pub fn check_accounts_export<R: Read>(mut reader: R, book: &Book) -> Result<Result<ExportCounts, String>, XmlError> {
    let mut text = String::new();
    reader.read_to_string(&mut text).map_err(|e| {
        if e.kind() == std::io::ErrorKind::InvalidData {
            XmlError::Encoding
        } else {
            XmlError::Io(e)
        }
    })?;
    let root = parse_document(&text)?;
    if root.name != "gnc-v2" {
        return Ok(Err(format!("the root element is {}, not gnc-v2", root.name)));
    }
    let mut check = Check {
        book,
        notices: Notices::default(),
        counts: ExportCounts::default(),
        seen: HashSet::new(),
    };
    for el in root.elements() {
        if let Some(difference) = check.element(el)? {
            return Ok(Err(difference));
        }
    }
    Ok(check.finish())
}

struct Check<'a> {
    book: &'a Book,
    notices: Notices,
    counts: ExportCounts,
    seen: HashSet<Guid>,
}

impl Check<'_> {
    fn element(&mut self, el: &Element) -> Result<Option<String>, XmlError> {
        match el.name.as_str() {
            "gnc:count-data" => Ok(None),
            "gnc:commodity" => {
                let c = parse_commodity(el, &mut self.notices)?;
                self.counts.commodities += 1;
                Ok((self.book.commodity(&c.id) != Some(&c)).then(|| format!("commodity {} differs after writing", c.id)))
            }
            "gnc:account" => {
                let (a, lots) = parse_account(el, &mut self.notices)?;
                self.counts.accounts += 1;
                self.seen.insert(a.guid);
                let same = self.book.account(a.guid) == Some(&a) && lots.is_empty();
                Ok((!same).then(|| format!("account {} differs after writing", a.guid)))
            }
            other => Ok(Some(format!("unexpected element {other}"))),
        }
    }

    fn finish(self) -> Result<ExportCounts, String> {
        let commodities = self.book.commodities.values().filter(|c| commodity_is_written(c)).count();
        let descendants = self.book.descendants(self.book.root);
        let mut tree = std::iter::once(self.book.root).chain(descendants.iter().map(|a| a.guid));
        if let Some(missing) = tree.find(|g| !self.seen.contains(g)) {
            return Err(format!("account {missing} is missing after writing"));
        }
        if self.counts.commodities != commodities {
            return Err(format!("{} commodities written, {commodities} expected", self.counts.commodities));
        }
        if !self.notices.unrecognised.is_empty() {
            return Err(self.notices.unrecognised.join("; "));
        }
        Ok(self.counts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::book::write_accounts_to;

    fn fixture(name: &str) -> Vec<u8> {
        std::fs::read(format!("{}/../../fixtures/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap()
    }

    /// An export holds the header, the counts, the commodities and the accounts, root first and without lots, and no
    /// book; it reads back as the Book's tree.
    #[test]
    fn exports_read_back_as_the_tree() {
        let (book, residue) = crate::read_book(&fixture("synthetic/business.gnucash")).unwrap();
        assert!(!book.lots.is_empty(), "the fixture has lots to leave out");
        let bytes = write_accounts_to(&book, &residue, Vec::new()).unwrap();
        let text = String::from_utf8(bytes.clone()).unwrap();
        assert!(text.starts_with("<?xml version=\"1.0\" encoding=\"utf-8\" ?>\n<gnc-v2\n     xmlns:gnc="));
        assert!(text.ends_with("</gnc-v2>\n\n"));
        let accounts = 1 + book.descendants(book.root).len();
        assert!(text.contains(&format!("<gnc:count-data cd:type=\"account\">{accounts}</gnc:count-data>\n")));
        assert!(
            !text.contains("<gnc:book") && !text.contains("act:lots") && !text.contains("gnc:transaction") && !text.contains("gnc:pricedb")
        );
        let counts = check_accounts_export(bytes.as_slice(), &book).unwrap().unwrap();
        assert_eq!(counts.accounts, accounts);
        let root_at = text.find(&format!("<act:id type=\"guid\">{}</act:id>", book.root)).unwrap();
        assert!(
            text.find("<gnc:account")
                .is_some_and(|first| first < root_at && root_at < text[first..].find("</gnc:account>").unwrap() + first)
        );
    }

    /// The check notices a file that does not hold the Book's tree.
    #[test]
    fn the_check_finds_differences() {
        let (book, residue) = crate::read_book(&fixture("synthetic/rich.gnucash")).unwrap();
        let text = String::from_utf8(write_accounts_to(&book, &residue, Vec::new()).unwrap()).unwrap();
        let (other, _) = crate::read_book(&fixture("piecash/simple_sample.gnucash")).unwrap();
        assert!(check_accounts_export(text.as_bytes(), &other).unwrap().is_err());
        let cut = text.rfind("<gnc:account").unwrap();
        let truncated = format!("{}</gnc-v2>\n", &text[..cut]);
        assert!(
            check_accounts_export(truncated.as_bytes(), &book)
                .unwrap()
                .unwrap_err()
                .contains("missing")
        );
    }
}
