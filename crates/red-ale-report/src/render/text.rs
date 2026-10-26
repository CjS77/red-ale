//! Text into markup: escaping, and GnuCash's font descriptions into CSS.

use std::borrow::Cow;

use super::sheet::FONT_FAMILY;

/// `s` with `&`, `<`, `>`, `"` and `'` escaped, and control characters other than tab, newline and carriage return
/// replaced by U+FFFD, so it is safe as HTML or SVG text and inside a quoted attribute.
pub(crate) fn escape(s: &str) -> Cow<'_, str> {
    let needs = |c: char| matches!(c, '&' | '<' | '>' | '"' | '\'') || is_control(c);
    if !s.chars().any(needs) {
        return Cow::Borrowed(s);
    }
    Cow::Owned(s.chars().fold(String::with_capacity(s.len() + 16), |mut out, c| {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c if is_control(c) => out.push('\u{fffd}'),
            c => out.push(c),
        }
        out
    }))
}

fn is_control(c: char) -> bool {
    c.is_control() && !matches!(c, '\t' | '\n' | '\r')
}

/// Escaped text with each line break as `<br>`.
pub(crate) fn escape_lines(s: &str) -> String {
    s.lines().map(|line| escape(line).into_owned()).collect::<Vec<_>>().join("<br>")
}

/// A font option, a Pango description such as `Noto Sans Bold 15`, as CSS declarations, after
/// `font-name-to-style-info` (`5.17:gnucash/report/html-fonts.scm:53-98`): the last word is the size in points; a
/// weight (`bold`, `regular`, `light`, `weight=N`), a style (`italic`, `oblique`) and a stretch (`expanded`,
/// `condensed`) are taken out of the family name. The family keeps only letters, digits, spaces, `-`, `_` and `.`, so
/// no option can write other CSS; a size that is not a number is left out.
pub(crate) fn font_css(description: &str) -> String {
    let (family, size) = match description.rsplit_once(' ') {
        Some((family, size)) if is_size(size) => (family, Some(size)),
        _ => (description, None),
    };
    let (family, weight) = take_weight(family);
    let (family, style) = take_word(&family, &[(" italic", "italic"), (" oblique", "oblique")]);
    let (family, stretch) = take_word(&family, &[(" expanded", "expanded"), (" condensed", "condensed")]);
    let family: String = family.chars().filter(|c| c.is_alphanumeric() || matches!(c, ' ' | '-' | '_' | '.')).collect();
    let family = if family.trim().is_empty() { FONT_FAMILY } else { family.trim() };
    let mut css = format!("font-family:\"{family}\",sans-serif;");
    css.extend(size.map(|s| format!("font-size:{s}pt;")));
    css.extend(style.map(|s| format!("font-style:{s};")));
    css.extend(weight.map(|w| format!("font-weight:{w};")));
    css.extend(stretch.map(|s| format!("font-stretch:{s};")));
    css
}

fn is_size(s: &str) -> bool {
    let digits = s.chars().filter(char::is_ascii_digit).count();
    let dots = s.chars().filter(|c| *c == '.').count();
    (1..=6).contains(&digits) && dots <= 1 && digits + dots == s.chars().count() && s.chars().any(|c| c != '0' && c != '.')
}

/// The first of `words` found in `family`, ignoring ASCII case, removed, with the CSS value it stands for.
fn take_word(family: &str, words: &[(&str, &'static str)]) -> (String, Option<&'static str>) {
    let lower = family.to_ascii_lowercase();
    words
        .iter()
        .find_map(|(word, css)| lower.find(word).map(|at| (format!("{}{}", &family[..at], &family[at + word.len()..]), Some(*css))))
        .unwrap_or_else(|| (family.to_owned(), None))
}

/// The weight: `bold`, `regular`, `light`, then ` weight=N`, checked in GnuCash's order.
fn take_weight(family: &str) -> (String, Option<String>) {
    let (rest, named) = take_word(family, &[(" bold", "bold"), (" regular", "normal"), (" light", "lighter")]);
    if named.is_some() {
        return (rest, named.map(str::to_owned));
    }
    let lower = family.to_ascii_lowercase();
    let numeric = lower.find(" weight=").and_then(|at| {
        let start = at + " weight=".len();
        let digits = family[start..].chars().take_while(char::is_ascii_digit).count();
        (digits > 0).then(|| (format!("{}{}", &family[..at], &family[start + digits..]), family[start..start + digits].to_owned()))
    });
    numeric.map_or_else(|| (family.to_owned(), None), |(rest, w)| (rest, Some(w)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_markup_and_controls() {
        assert_eq!(escape("plain"), "plain");
        assert_eq!(escape("<b a=\"1\" b='2'>&</b>"), "&lt;b a=&quot;1&quot; b=&#39;2&#39;&gt;&amp;&lt;/b&gt;");
        assert_eq!(escape("a\u{0}b\tc"), "a\u{fffd}b\tc");
        assert_eq!(escape_lines("one <\ntwo"), "one &lt;<br>two");
    }

    #[test]
    fn fonts_follow_font_name_to_style_info() {
        assert_eq!(font_css("Noto Sans Bold 15"), "font-family:\"Noto Sans\",sans-serif;font-size:15pt;font-weight:bold;");
        assert_eq!(font_css("Noto Sans Italic 10"), "font-family:\"Noto Sans\",sans-serif;font-size:10pt;font-style:italic;");
        assert_eq!(
            font_css("DejaVu Serif Light Oblique Condensed 9.5"),
            "font-family:\"DejaVu Serif\",sans-serif;font-size:9.5pt;font-style:oblique;font-weight:lighter;font-stretch:condensed;"
        );
        assert_eq!(font_css("Cantarell weight=600 11"), "font-family:\"Cantarell\",sans-serif;font-size:11pt;font-weight:600;");
        assert_eq!(font_css("Arial"), "font-family:\"Arial\",sans-serif;");
    }

    #[test]
    fn font_options_cannot_write_other_css() {
        let css = font_css("x\"; } body { background: url(http://evil) } p { 12");
        assert_eq!(css, "font-family:\"x  body  background urlhttpevil  p\",sans-serif;font-size:12pt;");
        assert_eq!(font_css("Sans 1e9"), "font-family:\"Sans 1e9\",sans-serif;");
        assert_eq!(font_css("\"\" 10"), "font-family:\"Noto Sans\",sans-serif;font-size:10pt;");
    }
}
