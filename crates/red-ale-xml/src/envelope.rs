//! The file envelope: XML declaration, the `gnc-v2` root with its namespace declarations, and the book wrapper.
//!
//! GnuCash writes these with `fprintf`, not libxml2. `5.17:libgnucash/backend/xml/io-gncxml-v2.cpp:1251-1339`.

/// Namespaces declared on `<gnc-v2>`, in the order GnuCash 5.17 writes them: the core ones from `write_v2_header`,
/// then the business plugins' in `backend_registry` order.
pub const NAMESPACES: [&str; 29] = [
    "gnc",
    "act",
    "book",
    "cd",
    "cmdty",
    "price",
    "slot",
    "split",
    "sx",
    "trn",
    "ts",
    "fs",
    "bgt",
    "recurrence",
    "lot",
    "addr",
    "billterm",
    "bt-days",
    "bt-prox",
    "cust",
    "employee",
    "entry",
    "invoice",
    "job",
    "order",
    "owner",
    "taxtable",
    "tte",
    "vendor",
];

/// The book element's version attribute (`gnc_v2_book_version_string`).
pub const BOOK_VERSION: &str = "2.0.0";

/// The XML declaration and the opening `<gnc-v2>` tag with its namespace declarations.
pub fn header() -> String {
    let mut out = String::from("<?xml version=\"1.0\" encoding=\"utf-8\" ?>\n<gnc-v2");
    for ns in NAMESPACES {
        out.push_str("\n     xmlns:");
        out.push_str(ns);
        out.push_str("=\"http://www.gnucash.org/XML/");
        out.push_str(ns);
        out.push('"');
    }
    out.push_str(">\n");
    out
}

/// The closing root tag and the trailing blank line.
pub const FOOTER: &str = "</gnc-v2>\n\n";

/// A `gnc:count-data` line, written only for non-zero counts.
pub fn count_line(out: &mut String, kind: &str, count: usize) {
    if count > 0 {
        out.push_str("<gnc:count-data cd:type=\"");
        out.push_str(kind);
        out.push_str("\">");
        out.push_str(&count.to_string());
        out.push_str("</gnc:count-data>\n");
    }
}

#[cfg(test)]
mod tests {
    use std::fmt::Write as _;

    use super::*;
    use crate::dom::{Element, parse_document};

    /// Re-serialises a whole book from its DOM with GnuCash's envelope rules; only for testing the serialiser.
    fn passthrough(root: &Element) -> String {
        let mut out = header();
        root.elements().for_each(|el| {
            if el.name == "gnc:book" {
                let _ = writeln!(out, "<gnc:book version=\"{}\">", el.get_attr("version").unwrap_or(BOOK_VERSION));
                el.elements().for_each(|part| dump_book_part(&mut out, part));
                out.push_str("</gnc:book>\n");
            } else {
                out.push_str(&el.dump(0));
                out.push('\n');
            }
        });
        out.push_str(FOOTER);
        out
    }

    fn dump_book_part(out: &mut String, part: &Element) {
        match part.name.as_str() {
            "gnc:pricedb" => {
                let _ = writeln!(out, "<gnc:pricedb version=\"{}\">", part.get_attr("version").unwrap_or("1"));
                part.elements().for_each(|p| {
                    out.push_str("  ");
                    out.push_str(&p.dump(1));
                    out.push('\n');
                });
                out.push_str("</gnc:pricedb>\n");
            }
            "gnc:template-transactions" => {
                out.push_str("<gnc:template-transactions>\n");
                part.elements().for_each(|p| {
                    out.push_str(&p.dump(0));
                    out.push('\n');
                });
                out.push_str("</gnc:template-transactions>\n");
            }
            _ => {
                out.push_str(&part.dump(0));
                out.push('\n');
            }
        }
    }

    fn fixture(name: &str) -> String {
        let path = format!("{}/../../fixtures/gnucash/{name}", env!("CARGO_MANIFEST_DIR"));
        std::fs::read_to_string(path).unwrap()
    }

    #[test]
    fn dom_passthrough_is_byte_identical_for_gnucash_517_output() {
        for name in ["sample1.gnucash", "bug-746937-sx-split-order.gnucash"] {
            let text = fixture(name);
            let root = parse_document(&text).unwrap();
            let out = passthrough(&root);
            if out != text {
                let line = out.lines().zip(text.lines()).position(|(a, b)| a != b).unwrap_or(0);
                panic!(
                    "{name} differs at line {}: {:?} vs {:?}",
                    line + 1,
                    out.lines().nth(line),
                    text.lines().nth(line)
                );
            }
        }
    }
}
