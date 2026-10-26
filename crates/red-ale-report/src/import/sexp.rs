//! A reader for the data subset of Scheme that GnuCash writes into its saved-report and style-sheet files.
//!
//! It reads text into [`Datum`] values and nothing else: there is no evaluator, no environment and no way to call
//! anything. What it reads is what Guile's `write` prints for the values GnuCash serialises
//! (`5.17:bindings/guile/options.scm:126-128`, `gnc:value->string` is `(format #f "~s" value)`): lists and dotted
//! pairs, strings with Guile's escapes, symbols (plain, and `#{…}#` for those with spaces), numbers, booleans, and
//! `'` for `quote`. Comments (`;` to the end of the line) are skipped. Anything else is a [`ReadError`]: vectors,
//! characters, `#|…|#` comments, quasiquote and the special floats `+inf.0` and `+nan.0`.

use std::fmt;

use num_bigint::BigInt;

use crate::number::{Rational, SizeCap};

/// The deepest nesting read. GnuCash's forms nest about ten deep; the bound keeps hostile input from exhausting the
/// stack.
pub const MAX_DEPTH: usize = 64;

/// The largest decimal exponent read (`1.0e300`): enough for any double, small enough to keep numbers bounded.
const MAX_EXPONENT: u32 = 400;

/// One Scheme datum.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Datum {
    /// `#t` or `#f`.
    Bool(bool),
    /// A number, held exactly as written: `42`, `1.5`, `1.0e-4`, `3/2`.
    Number(Rational),
    /// A string.
    String(String),
    /// A symbol.
    Symbol(String),
    /// A list: its items, and the tail after a dot for an improper list such as `(relative . today)`.
    List(Vec<Datum>, Option<Box<Datum>>),
}

impl Datum {
    /// `(quote d)`, as the reader turns `'d`.
    fn quoted(d: Datum) -> Self {
        Self::List(vec![Self::Symbol("quote".to_owned()), d], None)
    }

    /// The items of a proper list.
    pub fn as_list(&self) -> Option<&[Datum]> {
        match self {
            Self::List(items, None) => Some(items),
            _ => None,
        }
    }

    /// A symbol's name.
    pub fn as_symbol(&self) -> Option<&str> {
        match self {
            Self::Symbol(s) => Some(s),
            _ => None,
        }
    }

    /// A string's text.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(s) => Some(s),
            _ => None,
        }
    }

    /// Whether this is the symbol `name`.
    pub fn is_symbol(&self, name: &str) -> bool {
        self.as_symbol() == Some(name)
    }

    /// The datum `(quote d)` quotes.
    pub fn unquoted(&self) -> Option<&Datum> {
        match self.as_list() {
            Some([quote, d]) if quote.is_symbol("quote") => Some(d),
            _ => None,
        }
    }

    /// A pair `(car . cdr)`: a list of one item with a dotted tail.
    pub fn as_pair(&self) -> Option<(&Datum, &Datum)> {
        match self {
            Self::List(items, Some(tail)) if items.len() == 1 => items.first().map(|car| (car, &**tail)),
            _ => None,
        }
    }
}

impl fmt::Display for Datum {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Bool(b) => f.write_str(if *b { "#t" } else { "#f" }),
            Self::Number(n) => write!(f, "{}", n.to_exact_decimal().unwrap_or_else(|| n.to_string())),
            Self::String(s) => write!(f, "{s:?}"),
            Self::Symbol(s) => f.write_str(s),
            Self::List(items, tail) => {
                let inner: Vec<String> = items.iter().map(ToString::to_string).collect();
                match tail {
                    Some(t) => write!(f, "({} . {t})", inner.join(" ")),
                    None => write!(f, "({})", inner.join(" ")),
                }
            }
        }
    }
}

/// Text that is not in the subset read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadError {
    /// The line, from 1.
    pub line: u32,
    /// What is wrong.
    pub message: String,
}

impl fmt::Display for ReadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}

/// A datum and the line it starts on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Form {
    /// The line, from 1.
    pub line: u32,
    /// The datum.
    pub datum: Datum,
}

/// The top-level forms of a text, up to the first that cannot be read.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Forms {
    /// The forms read, in order.
    pub forms: Vec<Form>,
    /// Why reading stopped before the end, if it did: nothing after it is read.
    pub error: Option<ReadError>,
}

/// Read every top-level form of `text`, stopping at the first that is not in the subset.
pub fn read_all(text: &str) -> Forms {
    let mut reader = Reader { chars: text.chars().collect(), pos: 0, line: 1 };
    let mut out = Forms::default();
    loop {
        match reader.skip_atmosphere() {
            Ok(false) => return out,
            Ok(true) => {}
            Err(e) => return Forms { error: Some(e), ..out },
        }
        let line = reader.line;
        match reader.datum(0) {
            Ok(datum) => out.forms.push(Form { line, datum }),
            Err(e) => return Forms { error: Some(e), ..out },
        }
    }
}

/// Read exactly one datum from `text`.
pub fn read_one(text: &str) -> Result<Datum, ReadError> {
    let Forms { mut forms, error } = read_all(text);
    if let Some(e) = error {
        return Err(e);
    }
    match forms.len() {
        1 => Ok(forms.remove(0).datum),
        n => Err(ReadError { line: 1, message: format!("expected one datum, found {n}") }),
    }
}

struct Reader {
    chars: Vec<char>,
    pos: usize,
    line: u32,
}

type Read<T> = Result<T, ReadError>;

/// Characters that end a symbol or number.
fn is_delimiter(c: char) -> bool {
    c.is_whitespace() || matches!(c, '(' | ')' | '"' | ';' | '\'')
}

impl Reader {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn next(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.pos += 1;
        if c == '\n' {
            self.line += 1;
        }
        Some(c)
    }

    fn error<T>(&self, message: impl Into<String>) -> Read<T> {
        Err(ReadError { line: self.line, message: message.into() })
    }

    /// Skip whitespace and comments; whether anything is left.
    fn skip_atmosphere(&mut self) -> Read<bool> {
        while let Some(c) = self.peek() {
            match c {
                ';' => {
                    while self.peek().is_some_and(|c| c != '\n') {
                        self.next();
                    }
                }
                c if c.is_whitespace() => {
                    self.next();
                }
                '#' if self.chars.get(self.pos + 1) == Some(&'|') => return self.error("block comments #|…|# are not read"),
                _ => return Ok(true),
            }
        }
        Ok(false)
    }

    fn datum(&mut self, depth: usize) -> Read<Datum> {
        if depth > MAX_DEPTH {
            return self.error(format!("nested more than {MAX_DEPTH} deep"));
        }
        match self.peek() {
            None => self.error("unexpected end of text"),
            Some('(') => {
                self.next();
                self.list(depth)
            }
            Some(')') => self.error("unexpected )"),
            Some('"') => {
                self.next();
                self.string().map(Datum::String)
            }
            Some('\'') => {
                self.next();
                self.skip_atmosphere()?;
                self.datum(depth + 1).map(Datum::quoted)
            }
            Some(c @ ('`' | ',' | '[' | ']' | '{' | '}')) => self.error(format!("{c} is not read")),
            Some('#') => self.hash(),
            Some(_) => self.atom(),
        }
    }

    /// The rest of a list after `(`.
    fn list(&mut self, depth: usize) -> Read<Datum> {
        let mut items = Vec::new();
        loop {
            if !self.skip_atmosphere()? {
                return self.error("unclosed (");
            }
            if self.peek() == Some(')') {
                self.next();
                return Ok(Datum::List(items, None));
            }
            if self.at_dot() {
                return self.dotted_tail(items, depth);
            }
            items.push(self.datum(depth + 1)?);
        }
    }

    /// Whether the next token is a lone `.`.
    fn at_dot(&self) -> bool {
        self.peek() == Some('.') && self.chars.get(self.pos + 1).is_none_or(|c| is_delimiter(*c))
    }

    fn dotted_tail(&mut self, items: Vec<Datum>, depth: usize) -> Read<Datum> {
        self.next();
        if items.is_empty() {
            return self.error("a dot needs an item before it");
        }
        if !self.skip_atmosphere()? {
            return self.error("unclosed (");
        }
        let tail = self.datum(depth + 1)?;
        if !self.skip_atmosphere()? || self.next() != Some(')') {
            return self.error("expected ) after the item following a dot");
        }
        Ok(Datum::List(items, Some(Box::new(tail))))
    }

    /// The rest of a string after `"`.
    fn string(&mut self) -> Read<String> {
        let mut out = String::new();
        loop {
            match self.next() {
                None => return self.error("unclosed string"),
                Some('"') => return Ok(out),
                Some('\\') => self.escape(&mut out)?,
                Some(c) => out.push(c),
            }
        }
    }

    /// A string escape after `\`, as Guile writes and reads them: `\n`, `\t`, `\xHH`, `\uHHHH`, `\UHHHHHH`, …
    fn escape(&mut self, out: &mut String) -> Read<()> {
        let simple = |c| match c {
            '"' => Some('"'),
            '\\' => Some('\\'),
            'a' => Some('\u{7}'),
            'b' => Some('\u{8}'),
            't' => Some('\t'),
            'n' => Some('\n'),
            'v' => Some('\u{b}'),
            'f' => Some('\u{c}'),
            'r' => Some('\r'),
            '0' => Some('\0'),
            _ => None,
        };
        match self.next() {
            None => self.error("unclosed string"),
            Some('x') => self.hex_char(2).map(|c| out.push(c)),
            Some('u') => self.hex_char(4).map(|c| out.push(c)),
            Some('U') => self.hex_char(6).map(|c| out.push(c)),
            Some('\n') => {
                while self.peek().is_some_and(|c| c == ' ' || c == '\t') {
                    self.next();
                }
                Ok(())
            }
            Some(c) => simple(c).map(|e| out.push(e)).map_or_else(|| self.error(format!("unknown string escape \\{c}")), Ok),
        }
    }

    fn hex_char(&mut self, digits: usize) -> Read<char> {
        let text: String = (0..digits).filter_map(|_| self.next()).collect();
        u32::from_str_radix(&text, 16)
            .ok()
            .filter(|_| text.chars().count() == digits)
            .and_then(char::from_u32)
            .map_or_else(|| self.error(format!("bad escape \\x, \\u or \\U: {text:?}")), Ok)
    }

    /// `#t`, `#f`, `#true`, `#false`, or a `#{…}#` symbol.
    fn hash(&mut self) -> Read<Datum> {
        if self.chars.get(self.pos + 1) == Some(&'{') {
            self.pos += 2;
            return self.braced_symbol().map(Datum::Symbol);
        }
        let token = self.token();
        match token.as_str() {
            "#t" | "#true" => Ok(Datum::Bool(true)),
            "#f" | "#false" => Ok(Datum::Bool(false)),
            _ => self.error(format!("{token} is not read")),
        }
    }

    /// The rest of a `#{…}#` symbol, with `\xH…;` escapes.
    fn braced_symbol(&mut self) -> Read<String> {
        let mut out = String::new();
        loop {
            match self.next() {
                None => return self.error("unclosed #{"),
                Some('}') if self.peek() == Some('#') => {
                    self.next();
                    return Ok(out);
                }
                Some('\\') if self.peek() == Some('x') => {
                    self.next();
                    let hex: String = std::iter::from_fn(|| self.next()).take_while(|c| *c != ';').collect();
                    let c = u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32);
                    out.push(c.map_or_else(|| self.error(format!("bad symbol escape \\x{hex};")), Ok)?);
                }
                Some(c) => out.push(c),
            }
        }
    }

    fn token(&mut self) -> String {
        std::iter::from_fn(|| self.peek().filter(|c| !is_delimiter(*c)).and_then(|_| self.next())).collect()
    }

    /// A number or a symbol.
    fn atom(&mut self) -> Read<Datum> {
        let token = self.token();
        if token.is_empty() {
            return self.error("expected a datum");
        }
        match number(&token) {
            Some(Ok(n)) => Ok(Datum::Number(n)),
            Some(Err(message)) => self.error(message),
            None => Ok(Datum::Symbol(token)),
        }
    }
}

/// A token read as a number: `None` when it is not one (it is a symbol), an error for a number outside the subset.
fn number(token: &str) -> Option<Result<Rational, String>> {
    let body = token.strip_prefix(['+', '-']).unwrap_or(token);
    if !body.starts_with(|c: char| c.is_ascii_digit() || c == '.') || body == "." || body.starts_with("..") {
        let special = ["inf.0", "nan.0"].contains(&body) && body.len() < token.len();
        return special.then(|| Err(format!("{token} is not read")));
    }
    Some(exact(token).ok_or_else(|| format!("not a number: {token}")))
}

/// `[sign]digits[.digits][e[sign]digits]`, or `n/d`, exactly.
fn exact(token: &str) -> Option<Rational> {
    if token.contains('/') {
        return Rational::parse(token).ok();
    }
    let (mantissa, exponent) = match token.split_once(['e', 'E']) {
        Some((m, e)) => (m, e.parse::<i32>().ok()?),
        None => (token, 0),
    };
    if exponent.unsigned_abs() > MAX_EXPONENT || mantissa.contains(['+', '-']) && !mantissa.starts_with(['+', '-']) {
        return None;
    }
    let m = Rational::parse(mantissa).ok()?;
    let scale = Rational::from_integer(BigInt::from(10).pow(exponent.unsigned_abs()));
    let cap = SizeCap::default();
    if exponent < 0 { m.checked_div(&scale, cap).ok() } else { m.checked_mul(&scale, cap).ok() }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(text: &str) -> Datum {
        read_one(text).unwrap()
    }

    fn sym(s: &str) -> Datum {
        Datum::Symbol(s.to_owned())
    }

    fn num(s: &str) -> Datum {
        Datum::Number(Rational::parse(s).unwrap())
    }

    #[test]
    fn atoms() {
        assert_eq!(read("#t"), Datum::Bool(true));
        assert_eq!(read("#false"), Datum::Bool(false));
        assert_eq!(read("42"), num("42"));
        assert_eq!(read("-0.5"), num("-0.5"));
        assert_eq!(read("1.0e-4"), num("0.0001"));
        assert_eq!(read("1.5e2"), num("150"));
        assert_eq!(read("3/2"), num("1.5"));
        assert_eq!(read("end-prev-fin-year"), sym("end-prev-fin-year"));
        assert_eq!(read("gnc:lookup-option"), sym("gnc:lookup-option"));
        assert_eq!(read("-"), sym("-"));
    }

    #[test]
    fn strings_take_guiles_escapes() {
        // What Guile 3.0's `write` prints for "a\n\t\"\\" BEL ESC é DEL CR NUL, U+2028.
        assert_eq!(read(r#""a\n\t\"\\\a\x1bé\x7f\r\x00 ""#), Datum::String("a\n\t\"\\\u{7}\u{1b}é\u{7f}\r\0\u{2028}".to_owned()));
        assert_eq!(read("\"line\nbreak\""), Datum::String("line\nbreak".to_owned()));
        assert_eq!(read("\"joined \\\n   here\""), Datum::String("joined here".to_owned()));
        assert!(read_one(r#""\q""#).is_err());
        assert!(read_one(r#""\x1""#).is_err());
    }

    #[test]
    fn symbols_with_spaces() {
        assert_eq!(read("#{three plus three}#"), sym("three plus three"));
        assert_eq!(read(r"#{\x7d;#x}#"), sym("}#x"));
    }

    #[test]
    fn lists_pairs_and_quote() {
        assert_eq!(read("'(relative . today)"), Datum::quoted(Datum::List(vec![sym("relative")], Some(Box::new(sym("today"))))));
        assert_eq!(read("(a (b) ())"), Datum::List(vec![sym("a"), Datum::List(vec![sym("b")], None), Datum::List(vec![], None)], None));
        assert_eq!(read("(absolute 12 . 0)"), Datum::List(vec![sym("absolute"), num("12")], Some(Box::new(num("0")))));
        assert_eq!(read("'(0.0 202.0 59.0 255.0)").unquoted().and_then(Datum::as_list).map(<[Datum]>::len), Some(4));
    }

    #[test]
    fn comments_and_lines() {
        let forms = read_all(";; header\n(a)\n; Section: General\n\n(b ; inline\n c)\n");
        assert_eq!(forms.error, None);
        assert_eq!(forms.forms.iter().map(|f| f.line).collect::<Vec<_>>(), [2, 5]);
    }

    #[test]
    fn refuses_what_is_not_data() {
        ["#(1 2)", "#\\a", "`(a ,b)", "#|c|#", "+inf.0", "-nan.0", "(a . b c)", "( . a)", "(a", ")", "\"open", "1.2.3", "1e999"]
            .iter()
            .for_each(|t| assert!(read_one(t).is_err(), "{t} should not read"));
    }

    #[test]
    fn stops_at_the_first_error() {
        let forms = read_all("(a)\n(b\n");
        assert_eq!(forms.forms.len(), 1);
        assert_eq!(forms.error.map(|e| e.line), Some(3));
    }

    #[test]
    fn depth_is_bounded() {
        let deep = format!("{}{}", "(".repeat(MAX_DEPTH + 2), ")".repeat(MAX_DEPTH + 2));
        assert!(read_one(&deep).is_err());
        let fine = format!("{}{}", "(".repeat(10), ")".repeat(10));
        assert!(read_one(&fine).is_ok());
    }

    proptest::proptest! {
        #[test]
        fn never_panics(text in "\\PC{0,200}") {
            let _ = read_all(&text);
        }

        #[test]
        fn never_panics_on_scheme_like_text(text in "[()'\"#;.\\\\ a-z0-9{}|e+\\-/\n]{0,200}") {
            let _ = read_all(&text);
        }
    }
}
