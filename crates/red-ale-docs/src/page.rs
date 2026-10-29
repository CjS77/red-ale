//! The Page: one unit of the Manual, with its Page ID and kind.

use serde::{Deserialize, Serialize};

/// What a Page is about. The kind is the first part of its Page ID (`method/transaction.post`), except the Start Page,
/// whose Page ID is `start`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// The Reference page of a JSON-RPC method: `method/<name>`.
    Method,
    /// The Reference page of an error code: `error/<name>`.
    Error,
    /// The Reference page of a report template: `report/<slug of its name>`.
    Report,
    /// A Concept page: `concept/<name>`.
    Concept,
    /// A Guide: `guide/<name>`.
    Guide,
    /// A From GnuCash page, mapping GnuCash's menus and dialogs to methods: `gnucash/<name>`.
    Gnucash,
    /// The Start Page: `start`.
    Start,
    /// The licence, notices and History: `legal/<name>`.
    Legal,
}

impl Kind {
    /// Every kind, in the order indexes list them.
    pub const ALL: [Kind; 8] = [
        Kind::Start,
        Kind::Concept,
        Kind::Guide,
        Kind::Method,
        Kind::Error,
        Kind::Report,
        Kind::Gnucash,
        Kind::Legal,
    ];

    /// The kind's name, as Page IDs and the wire spell it.
    pub fn name(self) -> &'static str {
        match self {
            Kind::Method => "method",
            Kind::Error => "error",
            Kind::Report => "report",
            Kind::Concept => "concept",
            Kind::Guide => "guide",
            Kind::Gnucash => "gnucash",
            Kind::Start => "start",
            Kind::Legal => "legal",
        }
    }

    /// The kind a Page ID belongs to, if it has a known form.
    pub fn of_id(id: &str) -> Option<Kind> {
        if id == "start" {
            return Some(Kind::Start);
        }
        let (prefix, rest) = id.split_once('/')?;
        let kind = Kind::ALL.into_iter().find(|k| *k != Kind::Start && k.name() == prefix)?;
        (!rest.is_empty()).then_some(kind)
    }

    /// The directory of `docs/` that holds this kind's hand-written files: `reference/<method>.md` for methods,
    /// `errors/<name>.md`, `reports/<id>.md`, `concepts/`, `guides/`, `gnucash/`, `legal/`; the Start Page is
    /// `start.md` at the top.
    pub fn directory(self) -> &'static str {
        match self {
            Kind::Method => "reference",
            Kind::Error => "errors",
            Kind::Report => "reports",
            Kind::Concept => "concepts",
            Kind::Guide => "guides",
            Kind::Gnucash => "gnucash",
            Kind::Start => "",
            Kind::Legal => "legal",
        }
    }
}

/// One Page of the Manual.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Page {
    /// The stable Page ID: `method/transaction.post`, `concept/value-and-amount`, `start`.
    pub id: String,
    /// What the Page is about.
    pub kind: Kind,
    /// The title.
    pub title: String,
    /// One line saying what the Page covers.
    pub summary: String,
    /// The Page IDs of related Pages.
    pub related: Vec<String>,
    /// The section of GnuCash's documentation the Page adapts (`guide/ch_txns.docbook`), if it adapts one.
    pub from: Option<String>,
    /// The body in Markdown, with Examples as fenced `example` blocks: the generated reference, if any, and the
    /// hand-written part.
    pub body: String,
    /// The hand-written part of the body, from `docs/`; `None` for a generated Page nobody has written up yet.
    #[serde(skip)]
    pub hand_written: Option<String>,
}

impl Page {
    /// The first paragraph of the hand-written part: what a method is for and when to use it. MCP tool descriptions
    /// are built from it.
    pub fn lead(&self) -> Option<&str> {
        let text = self.hand_written.as_deref()?.trim_start();
        let end = text.find("\n\n").unwrap_or(text.len());
        let lead = text[..end].trim();
        (!lead.is_empty() && !lead.starts_with('#') && !lead.starts_with("```")).then_some(lead)
    }
}

/// The Page ID of a method's Reference page.
pub fn method_page_id(method: &str) -> String {
    format!("method/{method}")
}

/// The Page ID of an error code's Reference page, by the code's stable name.
pub fn error_page_id(name: &str) -> String {
    format!("error/{name}")
}

/// The Page ID of a report template's Reference page: `report/` and its name in lower case, with every run of other
/// characters than letters and digits made one `-` (`Income vs. Day of Week` is `report/income-vs-day-of-week`).
pub fn report_page_id(template_name: &str) -> String {
    format!("report/{}", slug(template_name))
}

/// `text` in lower case with every run of characters other than letters and digits made one `-`, none at the ends.
pub(crate) fn slug(text: &str) -> String {
    let lowered = text.to_lowercase();
    let words: Vec<&str> = lowered.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).collect();
    words.join("-")
}

/// The Page ID a file of `docs/` holds, from its path relative to `docs/`: `reference/book.open.md` is
/// `method/book.open`, `start.md` is `start`.
pub(crate) fn id_for_path(path: &str) -> Option<String> {
    let stem = path.strip_suffix(".md")?;
    if stem == "start" {
        return Some("start".to_owned());
    }
    let (dir, name) = stem.split_once('/')?;
    let kind = Kind::ALL.into_iter().find(|k| *k != Kind::Start && k.directory() == dir)?;
    (!name.is_empty() && !name.contains('/')).then(|| format!("{}/{name}", kind.name()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_ids_name_their_kind() {
        assert_eq!(Kind::of_id("method/book.open"), Some(Kind::Method));
        assert_eq!(Kind::of_id("start"), Some(Kind::Start));
        assert_eq!(Kind::of_id("start/x"), None);
        assert_eq!(Kind::of_id("concept/"), None);
        assert_eq!(Kind::of_id("nothing"), None);
    }

    #[test]
    fn files_map_to_page_ids() {
        assert_eq!(id_for_path("reference/book.open.md").as_deref(), Some("method/book.open"));
        assert_eq!(id_for_path("errors/no-book.md").as_deref(), Some("error/no-book"));
        assert_eq!(id_for_path("start.md").as_deref(), Some("start"));
        assert_eq!(id_for_path("concepts/a/b.md"), None);
        assert_eq!(id_for_path("elsewhere/x.md"), None);
    }

    #[test]
    fn report_ids_are_slugs() {
        assert_eq!(report_page_id("Income vs. Day of Week"), "report/income-vs-day-of-week");
        assert_eq!(
            report_page_id("IFRS weighted-average cost basis report"),
            "report/ifrs-weighted-average-cost-basis-report"
        );
    }

    #[test]
    fn the_lead_is_the_first_hand_written_paragraph() {
        let page = Page {
            id: "method/x".into(),
            kind: Kind::Method,
            title: "x".into(),
            summary: String::new(),
            related: Vec::new(),
            from: None,
            body: String::new(),
            hand_written: Some("\nUse it to do x.\nOn two lines.\n\n## Examples\n".into()),
        };
        assert_eq!(page.lead(), Some("Use it to do x.\nOn two lines."));
    }
}
