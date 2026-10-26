//! The string helpers the QIF reader uses.
//!
//! Translated from `5.17:gnucash/import-export/qif-imp/string.scm` (module `(gnucash string)`), which carries no
//! copyright line. GPL-2.0-or-later; this translation is GPL-3.0-or-later. Only the helpers the translated QIF modules
//! call are here.

/// `gnc:string-rcontains`: the byte position of the last of the non-overlapping occurrences of `s2` in `s1`, found
/// from the left. `string.scm:31-36`.
pub fn string_rcontains(s1: &str, s2: &str) -> Option<usize> {
    s1.match_indices(s2).last().map(|(i, _)| i)
}

/// `gnc:substring-count`: the number of non-overlapping occurrences of `s2` in `s1`. `string.scm:49-54`.
pub fn substring_count(s1: &str, s2: &str) -> usize {
    s1.matches(s2).count()
}

/// `gnc:substring-split`: `s1` split at each occurrence of `s2`. `string.scm:67-72`.
pub fn substring_split<'a>(s1: &'a str, s2: &str) -> Vec<&'a str> {
    s1.split(s2).collect()
}

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
    fn string_rcontains_example() {
        // test-qif-imp.scm:20-22
        assert_eq!(string_rcontains("foobarfoobarf", "bar"), Some(9));
        assert_eq!(string_rcontains("foo", "bar"), None);
    }

    #[test]
    fn substring_count_example() {
        // test-qif-imp.scm:24-26
        assert_eq!(substring_count("foobarfoobarfoo", "bar"), 2);
    }

    #[test]
    fn substring_split_example() {
        // test-qif-imp.scm:28-30
        assert_eq!(substring_split("foobarfoobarf", "bar"), ["foo", "foo", "f"]);
    }

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
