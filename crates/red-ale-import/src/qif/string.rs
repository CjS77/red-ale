//! The string helpers the QIF reader uses.
//!
//! Translated from `5.17:gnucash/import-export/qif-imp/string.scm` (module `(gnucash string)`), which carries no
//! copyright line. GPL-2.0-or-later; this translation is GPL-3.0-or-later. Only the helpers the translated QIF modules
//! call are here.

/// `gnc:string-replace-char`: every `old` in `s` replaced with `new`. `string.scm:87-88`.
pub fn string_replace_char(s: &str, old: char, new: char) -> String {
    s.chars().map(|c| if c == old { new } else { c }).collect()
}

/// `gnc:string-delete-chars`: `s` without any of the characters in `chars`. `string.scm:100-101`.
pub fn string_delete_chars(s: &str, chars: &str) -> String {
    s.chars().filter(|c| !chars.contains(*c)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn string_replace_char_example() {
        // test-qif-imp.scm:32-34
        assert_eq!(string_replace_char("foo", 'o', 'c'), "fcc");
    }

    #[test]
    fn string_delete_chars_example() {
        // test-qif-imp.scm:36-38
        assert_eq!(string_delete_chars("abcd", "cb"), "ad");
    }
}
