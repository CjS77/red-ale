//! Helpers for the engine tests: registries of inline scripts, a run context and fixture books.

#![allow(
    dead_code,
    clippy::missing_panics_doc,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::needless_pass_by_value
)]

use red_ale_core::ops::period::{AccountingPeriod, FiscalYearEnd};
use red_ale_core::{Book, FixedEnv, Time64, TimeZone};
use red_ale_locale::Locale;
use red_ale_report::engine::{Registry, RunContext, ScriptSource};
use red_ale_report::library::Library;
use red_ale_report::limits::Limits;
use red_ale_report::settings::ReportSettings;
use red_ale_report::{ReportError, ReportResult};
use serde_json::{Value, json};

/// The GUID of the test template.
pub const GUID: &str = "00000000000000000000000000000abc";

/// The now the tests run at: 2024-03-15 12:00 UTC, the pinned now of `fixtures/reports/`.
pub const NOW: Time64 = Time64(1_710_504_000);

/// A user script defining one template whose `render` body is `body`.
pub fn template(body: &str) -> String {
    template_with("", body)
}

/// A user script with the functions `prelude` before the test template, whose `render` body is `body`.
pub fn template_with(prelude: &str, body: &str) -> String {
    format!(
        "fn templates() {{ [#{{ guid: \"{GUID}\", name: \"Test\" }}] }}\n\
         fn options(params, o, book) {{ }}\n\
         fn render(params, book, opts, ctx) {{\n{body}\n}}\n{prelude}\n"
    )
}

/// A result with one text block.
pub const EMPTY_RESULT: &str = "#{ title: \"t\", sections: [] }";

/// A registry of the given user scripts (path, text) and no built-in ones.
pub fn registry(scripts: &[(&str, &str)]) -> Registry {
    registry_with(Library::from_sources([]), scripts, Limits::default())
}

pub fn registry_with(library: Library, scripts: &[(&str, &str)], limits: Limits) -> Registry {
    let user: Vec<ScriptSource> = scripts
        .iter()
        .map(|(p, t)| ScriptSource {
            path: (*p).to_owned(),
            text: (*t).to_owned(),
        })
        .collect();
    Registry::load(&library, &user, limits).expect("the library loads")
}

pub fn empty_book() -> Book {
    Book::new_empty(&mut FixedEnv::new(Time64(0), 1))
}

/// A report fixture, read through the XML backend.
pub fn fixture(name: &str) -> Book {
    let path = format!("{}/../../fixtures/reports/{name}", env!("CARGO_MANIFEST_DIR"));
    let bytes = std::fs::read(&path).expect("the fixture exists");
    red_ale_xml::read_book(&bytes).expect("the fixture reads").0
}

/// What a run needs, owned.
pub struct Host {
    pub book: Book,
    pub zone: TimeZone,
    pub settings: ReportSettings,
    pub period: AccountingPeriod,
    pub fiscal_year_end: Option<FiscalYearEnd>,
    pub now: Time64,
    pub locale: Locale,
}

impl Host {
    pub fn new(book: Book) -> Self {
        let (zone, settings, period) = (TimeZone::utc(), ReportSettings::default(), AccountingPeriod::default());
        Self {
            book,
            zone,
            settings,
            period,
            fiscal_year_end: None,
            now: NOW,
            locale: Locale::c(),
        }
    }

    pub fn ctx(&self) -> RunContext<'_> {
        RunContext {
            book: &self.book,
            now: self.now,
            zone: &self.zone,
            settings: &self.settings,
            locale: self.locale,
            accounting_period: &self.period,
            fiscal_year_end: self.fiscal_year_end,
            style_sheets: &["Default", "Easy"],
            export: None,
            configurations: &(),
        }
    }

    /// Run the template `reference` with `options`.
    pub fn run(&self, registry: &Registry, reference: &str, options: &Value) -> Result<ReportResult, ReportError> {
        let t = registry
            .find(reference)
            .unwrap_or_else(|e| panic!("{e:?}: {:?}", registry.problems()));
        registry.run(t, None, Some(options), &self.ctx())
    }
}

/// Run the test template of a script with the render `body` on an empty book.
pub fn render(body: &str) -> Result<ReportResult, ReportError> {
    let script = template(body);
    let reg = registry(&[("test.rhai", &script)]);
    assert!(reg.problems().is_empty(), "{:?}", reg.problems());
    Host::new(empty_book()).run(&reg, GUID, &json!({}))
}

/// Run a render `body` that ends `#{ title: …, sections: [] }`-free: it returns a value, which the test reads from
/// the title. The body's last expression is shown as the title's text.
pub fn eval(expr: &str) -> Result<String, ReportError> {
    render(&format!("let v = {{ {expr} }};\n#{{ title: `${{v}}`, sections: [] }}")).map(|r| r.title)
}

/// The `data.limit` of a `report-limit` error.
pub fn limit_of(e: &ReportError) -> Option<String> {
    match e {
        ReportError::Limit(l) => Some(l.limit.as_str().to_owned()),
        _ => None,
    }
}
