//! A small DOM, and a serialiser that writes it exactly as libxml2 2.9 does for GnuCash.
//!
//! GnuCash builds a libxml2 node per entity and writes it with `xmlElemDump` (`format = 1`), which indents children
//! by two spaces per level unless an element has text content. This module reproduces those bytes so that an
//! unedited book round-trips byte for byte. See `design/notes/xml-format-core.md`.

use std::fmt::Write as _;
use std::io::BufRead;

use quick_xml::Reader;
use quick_xml::events::{BytesStart, Event};

use crate::error::XmlError;
use crate::input::TextStream;

/// An XML node: an element or text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Node {
    /// An element.
    Element(Element),
    /// Character data, unescaped.
    Text(String),
}

/// An XML element with its attributes and children in document order.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Element {
    /// Qualified name, such as `act:name`.
    pub name: String,
    /// Attributes in document order, values unescaped.
    pub attrs: Vec<(String, String)>,
    /// Children in document order.
    pub children: Vec<Node>,
}

impl Element {
    /// An empty element.
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into(), attrs: Vec::new(), children: Vec::new() }
    }

    /// An element holding only text, built like GnuCash's `text_to_dom_tree` (`xmlNodeAddContent`): empty text adds
    /// no child, so it is written `<name/>`.
    pub fn text(name: impl Into<String>, text: impl Into<String>) -> Self {
        let text = text.into();
        let mut el = Self::new(name);
        if !text.is_empty() {
            el.children.push(Node::Text(text));
        }
        el
    }

    /// An element built like libxml2's `xmlNewTextChild`: it always has a text child, so empty text is written
    /// `<name></name>`.
    pub fn text_child(name: impl Into<String>, text: impl Into<String>) -> Self {
        let mut el = Self::new(name);
        el.children.push(Node::Text(text.into()));
        el
    }

    /// Add an attribute (builder style).
    #[must_use]
    pub fn attr(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.attrs.push((name.into(), value.into()));
        self
    }

    /// Add a child element (builder style).
    #[must_use]
    pub fn child(mut self, child: Element) -> Self {
        self.children.push(Node::Element(child));
        self
    }

    /// Add a child element if present (builder style).
    #[must_use]
    pub fn child_opt(self, child: Option<Element>) -> Self {
        match child {
            Some(c) => self.child(c),
            None => self,
        }
    }

    /// Append a child element.
    pub fn push(&mut self, child: Element) {
        self.children.push(Node::Element(child));
    }

    /// The value of an attribute.
    pub fn get_attr(&self, name: &str) -> Option<&str> {
        self.attrs.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str())
    }

    /// Child elements in order.
    pub fn elements(&self) -> impl Iterator<Item = &Element> {
        self.children.iter().filter_map(|n| match n {
            Node::Element(e) => Some(e),
            Node::Text(_) => None,
        })
    }

    /// The first child element with this name.
    pub fn find(&self, name: &str) -> Option<&Element> {
        self.elements().find(|e| e.name == name)
    }

    /// The concatenated text content of this element's direct text children.
    pub fn text_content(&self) -> String {
        self.children
            .iter()
            .filter_map(|n| match n {
                Node::Text(t) => Some(t.as_str()),
                Node::Element(_) => None,
            })
            .collect()
    }

    /// Serialise as `xmlElemDump` does: at indentation `level`, formatted. No trailing newline.
    pub fn dump(&self, level: usize) -> String {
        let mut out = String::new();
        dump_element(&mut out, self, level, true);
        out
    }
}

/// libxml2 caps indentation at 60 characters (`MAX_INDENT`, `xmlsave.c`).
const MAX_INDENT: usize = 60;

fn indent(out: &mut String, level: usize) {
    let width = (level * 2).min(MAX_INDENT);
    out.extend(std::iter::repeat_n(' ', width));
}

/// `xmlNodeDumpOutputInternal` with `format = 1`: children are indented only when none of them is text.
fn dump_element(out: &mut String, el: &Element, level: usize, format: bool) {
    out.push('<');
    out.push_str(&el.name);
    el.attrs.iter().for_each(|(k, v)| {
        out.push(' ');
        out.push_str(k);
        out.push_str("=\"");
        escape_attr(out, v);
        out.push('"');
    });
    if el.children.is_empty() {
        out.push_str("/>");
        return;
    }
    out.push('>');
    let child_format = format && el.children.iter().all(|c| matches!(c, Node::Element(_)));
    if child_format {
        out.push('\n');
    }
    el.children.iter().for_each(|child| {
        match child {
            Node::Element(e) => {
                if child_format {
                    indent(out, level + 1);
                }
                dump_element(out, e, level + 1, child_format);
            }
            Node::Text(t) => escape_text(out, t),
        }
        if child_format {
            out.push('\n');
        }
    });
    if child_format {
        indent(out, level);
    }
    out.push_str("</");
    out.push_str(&el.name);
    out.push('>');
}

/// `xmlEscapeContent` as libxml2 2.9.14 applies it to GnuCash's nodes: markup characters and carriage return
/// escaped, everything else raw UTF-8. Control characters other than tab and newline are not valid XML and are
/// written as `?`, as libxml2 does. Verified against the oracle (`design/notes/xml-format-core.md` §3.4).
fn escape_text(out: &mut String, text: &str) {
    text.chars().for_each(|c| match c {
        '<' => out.push_str("&lt;"),
        '>' => out.push_str("&gt;"),
        '&' => out.push_str("&amp;"),
        '\r' => out.push_str("&#13;"),
        '\n' | '\t' => out.push(c),
        '\u{0}'..='\u{1f}' => out.push('?'),
        _ => out.push(c),
    });
}

/// `xmlAttrSerializeTxtContent` with no document: as text, plus quotes and whitespace controls.
fn escape_attr(out: &mut String, text: &str) {
    text.chars().for_each(|c| match c {
        '<' => out.push_str("&lt;"),
        '>' => out.push_str("&gt;"),
        '&' => out.push_str("&amp;"),
        '"' => out.push_str("&quot;"),
        '\n' => out.push_str("&#10;"),
        '\r' => out.push_str("&#13;"),
        '\t' => out.push_str("&#9;"),
        ' '..='\u{7f}' => out.push(c),
        _ => {
            let _ = write!(out, "&#x{:X};", u32::from(c));
        }
    });
}

/// Parse a whole document and return its root element.
///
/// Whitespace-only text is dropped from elements that also have element children: it is formatting, which the
/// serialiser recreates. Text content of leaf elements is kept exactly, including whitespace.
pub fn parse_document(text: &str) -> Result<Element, XmlError> {
    let mut tokens = Tokens::new(TextStream::new(text.as_bytes())?);
    let mut builder = TreeBuilder::default();
    let mut root = None;
    while let Some(token) = tokens.next()? {
        if let Some(el) = builder.feed(token).map_err(|e| e.at(tokens.position()))? {
            root = Some(el);
        }
    }
    if !builder.is_empty() {
        return Err(Unbalanced::Open.at(tokens.position()));
    }
    root.ok_or(XmlError::Empty)
}

/// One step through a document, as the tree builder takes it. Comments, processing instructions and declarations
/// are skipped, and references are resolved into text.
pub(crate) enum Token {
    /// A start tag.
    Start(Element),
    /// An empty-element tag, `<a/>`.
    Empty(Element),
    /// An end tag.
    End,
    /// Character data, unescaped.
    Text(String),
}

/// The tokens of a document read from a stream of decoded text ([`TextStream`]).
pub(crate) struct Tokens<R: BufRead> {
    reader: Reader<TextStream<R>>,
    buf: Vec<u8>,
    position: u64,
}

impl<R: BufRead> Tokens<R> {
    pub(crate) fn new(text: TextStream<R>) -> Self {
        let mut reader = Reader::from_reader(text);
        let config = reader.config_mut();
        config.trim_text(false);
        config.expand_empty_elements = false;
        Self { reader, buf: Vec::new(), position: 0 }
    }

    /// Byte offset in the decoded text of the token last read.
    pub(crate) fn position(&self) -> u64 {
        self.position
    }

    /// The text stream, to look ahead before the first token or to read on after an error.
    pub(crate) fn text(&mut self) -> &mut TextStream<R> {
        self.reader.get_mut()
    }

    /// The next token, or `None` at the end of the document. An error reading the file comes back as the file's
    /// error, not as what the tokeniser made of it.
    pub(crate) fn next(&mut self) -> Result<Option<Token>, XmlError> {
        loop {
            self.buf.clear();
            self.position = self.reader.buffer_position();
            let pos = self.position;
            let syntax = |message: String| XmlError::Syntax { offset: pos, message };
            let event = match self.reader.read_event_into(&mut self.buf) {
                Ok(event) => event,
                Err(e) => return Err(self.reader.get_mut().take_error().unwrap_or_else(|| syntax(e.to_string()))),
            };
            let token = match event {
                Event::Start(start) => Token::Start(start_element(&start)?),
                Event::Empty(start) => Token::Empty(start_element(&start)?),
                Event::End(_) => Token::End,
                Event::Text(t) => Token::Text(t.decode().map_err(|e| syntax(e.to_string()))?.into_owned()),
                Event::CData(c) => Token::Text(c.decode().map_err(|e| syntax(e.to_string()))?.into_owned()),
                Event::GeneralRef(r) => {
                    let name = r.decode().map_err(|e| syntax(e.to_string()))?;
                    Token::Text(resolve_reference(&name).ok_or_else(|| syntax(format!("unknown entity &{name};")))?)
                }
                Event::Eof => return Ok(None),
                Event::Comment(_) | Event::Decl(_) | Event::PI(_) | Event::DocType(_) => continue,
            };
            return Ok(Some(token));
        }
    }
}

/// Builds elements from tokens, one outermost element at a time.
#[derive(Default)]
pub(crate) struct TreeBuilder {
    stack: Vec<Element>,
}

/// An end tag with no element open, or the document ending with one still open.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Unbalanced {
    /// An end tag with no element open.
    End,
    /// The document ended with an element open.
    Open,
}

impl Unbalanced {
    /// The syntax error at `offset`.
    pub(crate) fn at(self, offset: u64) -> XmlError {
        let message = match self {
            Self::End => "unbalanced end tag",
            Self::Open => "unexpected end of document",
        };
        XmlError::Syntax { offset, message: message.into() }
    }
}

impl TreeBuilder {
    /// No element is being built.
    pub(crate) fn is_empty(&self) -> bool {
        self.stack.is_empty()
    }

    /// Take one token. Returns the outermost element it completes, if any; text outside any element is dropped.
    pub(crate) fn feed(&mut self, token: Token) -> Result<Option<Element>, Unbalanced> {
        match token {
            Token::Start(el) => {
                self.stack.push(el);
                Ok(None)
            }
            Token::Empty(el) => Ok(self.attach(el)),
            Token::End => {
                let mut el = self.stack.pop().ok_or(Unbalanced::End)?;
                // `<a></a>` holds an empty text node in libxml2, unlike `<a/>`; keep the difference.
                if el.children.is_empty() {
                    el.children.push(Node::Text(String::new()));
                }
                drop_formatting(&mut el);
                Ok(self.attach(el))
            }
            Token::Text(text) => {
                self.push_text(text);
                Ok(None)
            }
        }
    }

    fn attach(&mut self, el: Element) -> Option<Element> {
        match self.stack.last_mut() {
            Some(parent) => {
                parent.children.push(Node::Element(el));
                None
            }
            None => Some(el),
        }
    }

    fn push_text(&mut self, text: String) {
        let Some(parent) = self.stack.last_mut() else { return };
        if let Some(Node::Text(last)) = parent.children.last_mut() {
            last.push_str(&text);
        } else {
            parent.children.push(Node::Text(text));
        }
    }
}

fn drop_formatting(el: &mut Element) {
    if el.children.iter().any(|c| matches!(c, Node::Element(_))) {
        el.children.retain(|c| !matches!(c, Node::Text(t) if t.chars().all(char::is_whitespace)));
    }
}

fn start_element(start: &BytesStart<'_>) -> Result<Element, XmlError> {
    let name = String::from_utf8(start.name().as_ref().to_vec()).map_err(|_| XmlError::Encoding)?;
    let attrs = start
        .attributes()
        .map(|a| {
            let a = a.map_err(|e| XmlError::Syntax { offset: 0, message: e.to_string() })?;
            let key = String::from_utf8(a.key.as_ref().to_vec()).map_err(|_| XmlError::Encoding)?;
            let raw = std::str::from_utf8(&a.value).map_err(|_| XmlError::Encoding)?;
            Ok((key, unescape(raw)?))
        })
        .collect::<Result<Vec<_>, XmlError>>()?;
    Ok(Element { name, attrs, children: Vec::new() })
}

/// Unescape entity and character references in attribute text.
fn unescape(raw: &str) -> Result<String, XmlError> {
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        let after = &rest[amp + 1..];
        let semi = after.find(';').ok_or_else(|| XmlError::Syntax { offset: 0, message: format!("bad reference in {raw:?}") })?;
        let resolved = resolve_reference(&after[..semi])
            .ok_or_else(|| XmlError::Syntax { offset: 0, message: format!("unknown entity in {raw:?}") })?;
        out.push_str(&resolved);
        rest = &after[semi + 1..];
    }
    out.push_str(rest);
    Ok(out)
}

/// The five predefined entities and numeric character references.
fn resolve_reference(name: &str) -> Option<String> {
    let c = match name {
        "lt" => '<',
        "gt" => '>',
        "amp" => '&',
        "quot" => '"',
        "apos" => '\'',
        _ => {
            let code = name.strip_prefix("#x").or_else(|| name.strip_prefix("#X")).map_or_else(
                || name.strip_prefix('#').and_then(|d| d.parse::<u32>().ok()),
                |hex| u32::from_str_radix(hex, 16).ok(),
            )?;
            char::from_u32(code)?
        }
    };
    Some(c.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dumps_like_libxml2() {
        let el = Element::new("gnc:account")
            .attr("version", "2.0.0")
            .child(Element::text("act:name", "A & B <é>"))
            .child(Element::text("act:id", "abc").attr("type", "guid"))
            .child(Element::new("act:slots").child(Element::new("slot").child(Element::text("slot:key", "k"))))
            .child(Element::new("act:non-standard-scu"));
        let expected = "<gnc:account version=\"2.0.0\">\n  <act:name>A &amp; B &lt;é&gt;</act:name>\n  <act:id type=\"guid\">abc</act:id>\n  <act:slots>\n    <slot>\n      <slot:key>k</slot:key>\n    </slot>\n  </act:slots>\n  <act:non-standard-scu/>\n</gnc:account>";
        assert_eq!(el.dump(0), expected);
    }

    #[test]
    fn parse_then_dump_is_identity() {
        let text = "<gnc:account version=\"2.0.0\">\n  <act:name>Café &amp; co\ttab&#13;</act:name>\n  <act:description> </act:description>\n  <act:slots>\n    <slot>\n      <slot:key>notes</slot:key>\n      <slot:value type=\"string\">line1\nline2</slot:value>\n    </slot>\n  </act:slots>\n</gnc:account>";
        let el = parse_document(text).unwrap();
        assert_eq!(el.find("act:name").unwrap().text_content(), "Café & co\ttab\r");
        assert_eq!(el.dump(0), text);
    }
}
