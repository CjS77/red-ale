//! `Regexp` (`design/reports.md` §2, "The engine for a run"): Guile's `make-regexp` and `regexp-exec`, which the
//! Transaction Report engine's account and transaction filters use (`5.17:gnucash/report/trep-engine.scm:2210-2254`,
//! `:2439-2446`).
//!
//! GnuCash compiles POSIX extended expressions; red-ale compiles Rust `regex` expressions with the same options
//! `transaction.find` uses, so one dialect serves both (`design/reports.md` §14).

use regex::{Regex, RegexBuilder};
use rhai::{Dynamic, Engine};

/// A compiled regular expression (Rhai type `Regexp`).
#[derive(Debug, Clone)]
pub struct Regexp(Regex);

pub(super) fn register(engine: &mut Engine) {
    engine.register_type_with_name::<Regexp>("Regexp");
    engine.register_fn("make_regexp", |pattern: &str| make_regexp(pattern, false));
    engine.register_fn("make_regexp", make_regexp);
    engine.register_fn("is_match", |r: &mut Regexp, text: &str| r.0.is_match(text));
    engine.register_fn("to_string", |r: &mut Regexp| format!("Regexp({})", r.0.as_str()));
}

/// `make-regexp`, with `regexp/icase` when `icase`: the expression, or `()` when it does not compile, where Guile
/// raises `regular-expression-syntax` (`trep-engine.scm:2212-2218`), so the caller can say which pattern failed.
fn make_regexp(pattern: &str, icase: bool) -> Dynamic {
    RegexBuilder::new(pattern)
        .case_insensitive(icase)
        .build()
        .map_or(Dynamic::UNIT, |r| Dynamic::from(Regexp(r)))
}
