//! `report.importConfigurations` and `report.importStyleSheets`: GnuCash's saved report configurations and style
//! sheets, from the text of its `saved-reports-2.8` (or `saved-reports-2.4`) and `stylesheets-2.0` files
//! (`design/api/reports.md`; `design/reports.md` §15).
//!
//! The text comes in the params: the server never reads a path a client names. It is read as data by
//! `red_ale_report::import` and never evaluated. Each entry is imported or skipped on its own, with the reason, and
//! everything imported is written to the Configuration in one atomic save; a dry run writes nothing.

use red_ale_core::Guid;
use red_ale_report::error::OptionError;
use red_ale_report::import::{self, Malformed, OptionForm, SavedReportForm, StyleSheetForm, WireOptions};
use red_ale_report::options::{GENERAL, OptionDb, STYLESHEET, ValueContext};
use red_ale_report::render::{PREDEFINED, StyleSheet, StyleSheetTemplate, left_out};
use red_ale_service::reports::{SavedReport, StyleSheetRecord};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::style_sheets::{self as sheets, StyleSheetInfo, record};
use super::{On, ReportInfo, ReportOptions, Reports, configurations};
use crate::dispatch::Context;
use crate::error::RpcError;

/// Params of `report.importConfigurations` and `report.importStyleSheets`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ImportParams {
    /// The file's text: GnuCash's `saved-reports-2.8` (else `saved-reports-2.4`), or `stylesheets-2.0`, from its user
    /// data directory. It is read as data and never run.
    pub text: String,
    /// Report what would be imported without writing the Configuration.
    #[serde(default)]
    pub dry_run: bool,
}

/// Why an entry was not imported.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum SkipReason {
    /// Not a form GnuCash writes, or not one red-ale reads: it was not evaluated.
    Malformed,
    /// Its template (a report's GUID, a style sheet template's name) is not one red-ale has translated.
    Untranslated,
    /// A saved configuration with its GUID, or a style sheet entry with its name, is already in the Configuration or
    /// earlier in the file.
    Exists,
    /// Another saved configuration already has its name.
    Conflict,
    /// An option value red-ale refuses (`report-option`).
    InvalidOption,
    /// A predefined style sheet at its template's defaults: nothing to keep.
    Unchanged,
}

/// An entry that was not imported.
#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SkippedEntry {
    /// The line it starts on, from 1.
    pub line: u32,
    /// Its name, when it could be read.
    pub name: Option<String>,
    /// A saved configuration's GUID, when it could be read.
    pub guid: Option<String>,
    /// Its template: a report template's GUID, or a style sheet template's name, when it could be read.
    pub template: Option<String>,
    /// Why it was skipped.
    pub reason: SkipReason,
    /// What is wrong, for people.
    pub message: String,
}

/// An option an imported entry sets that red-ale did not apply.
#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct IgnoredOption {
    /// The option's section.
    pub section: String,
    /// The option's name.
    pub name: String,
    /// `unknown-option`: the template has no such option, and GnuCash ignores it too. `not-rendered`: kept in the
    /// Configuration, with no effect in red-ale (a style sheet's images).
    pub reason: String,
}

/// A saved report configuration imported.
#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ImportedConfiguration {
    /// The line its entry starts on.
    pub line: u32,
    /// The configuration, with GnuCash's GUID and name.
    pub configuration: ReportInfo,
    /// What is stored: the options that differ from the template's defaults, in wire form.
    pub options: ReportOptions,
    /// Options the entry sets that were left out.
    pub ignored: Vec<IgnoredOption>,
}

/// The result of `report.importConfigurations`.
#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ImportConfigurationsResult {
    /// True when nothing was written.
    pub dry_run: bool,
    /// The configurations imported (or that would be), in file order.
    pub imported: Vec<ImportedConfiguration>,
    /// The entries skipped, in file order.
    pub skipped: Vec<SkippedEntry>,
}

/// A style sheet imported.
#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ImportedStyleSheet {
    /// The line its entry starts on.
    pub line: u32,
    /// The style sheet, with its options.
    pub style_sheet: StyleSheetInfo,
    /// Options the entry sets that red-ale does not render or does not have.
    pub ignored: Vec<IgnoredOption>,
}

/// The result of `report.importStyleSheets`.
#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ImportStyleSheetsResult {
    /// True when nothing was written.
    pub dry_run: bool,
    /// The style sheets imported (or that would be), in file order.
    pub imported: Vec<ImportedStyleSheet>,
    /// The entries skipped, in file order.
    pub skipped: Vec<SkippedEntry>,
}

impl SkippedEntry {
    fn malformed(m: Malformed) -> Self {
        Self {
            line: m.line,
            name: None,
            guid: None,
            template: None,
            reason: SkipReason::Malformed,
            message: m.message,
        }
    }
}

fn ignored(wire: &WireOptions) -> Vec<IgnoredOption> {
    let one = |(section, name, why): &(String, String, import::Ignored)| IgnoredOption {
        section: section.clone(),
        name: name.clone(),
        reason: why.name().to_owned(),
    };
    wire.ignored.iter().map(one).collect()
}

fn option_message(e: &OptionError) -> String {
    format!("option {}/{}: {}", e.section, e.name, e.reason)
}

/// Import saved report configurations. Each keeps GnuCash's GUID and name and stores only the options that differ
/// from its template's defaults, read without a Book, so the result does not depend on the book open.
pub(super) fn configurations(ctx: &mut Context, p: ImportParams) -> Result<ImportConfigurationsResult, RpcError> {
    let lock = ctx.settings.config.lock()?;
    let reports = Reports::load(ctx)?;
    let entries = import::saved_reports(&p.text);
    let mut all = reports.saved.clone();
    let mut out = ImportConfigurationsResult {
        dry_run: p.dry_run,
        imported: Vec::new(),
        skipped: Vec::new(),
    };
    ctx.query_any(|_, env| {
        let on = On {
            book: None,
            now: env.now(),
            zone: env.zone(),
        };
        entries.into_iter().for_each(|entry| {
            match entry
                .map_err(SkippedEntry::malformed)
                .and_then(|e| reports.configuration_entry(e, &all, &on))
            {
                Ok((saved, imported)) => {
                    all.push(saved);
                    out.imported.push(imported);
                }
                Err(skipped) => out.skipped.push(skipped),
            }
        });
        Ok(())
    })?;
    if !p.dry_run && !out.imported.is_empty() {
        configurations::store(ctx, &lock, reports.config, &all)?;
    }
    Ok(out)
}

impl Reports {
    /// One saved report entry as a configuration to add to `taken`, or why it is skipped.
    fn configuration_entry(
        &self,
        e: SavedReportForm,
        taken: &[SavedReport],
        on: &On<'_>,
    ) -> Result<(SavedReport, ImportedConfiguration), SkippedEntry> {
        let skip = |reason, message: String| SkippedEntry {
            line: e.line,
            name: Some(e.name.clone()),
            guid: Some(e.guid.clone()),
            template: Some(e.template.clone()),
            reason,
            message,
        };
        let (Ok(guid), Ok(template_guid)) = (Guid::parse(&e.guid), Guid::parse(&e.template)) else {
            return Err(skip(
                SkipReason::Malformed,
                "'report-guid and 'parent-type must be GUIDs".to_owned(),
            ));
        };
        let Some(template) = self.registry.template(template_guid) else {
            let message = format!("red-ale has no translation of the report template {}", e.template);
            return Err(skip(SkipReason::Untranslated, message));
        };
        let forms = e.options.as_ref().map_err(|m| skip(SkipReason::Malformed, m.clone()))?;
        if taken.iter().any(|s| s.guid == guid) {
            return Err(skip(
                SkipReason::Exists,
                format!("a saved configuration with GUID {} exists", e.guid),
            ));
        }
        if taken.iter().any(|s| s.name == e.name) {
            return Err(skip(
                SkipReason::Conflict,
                format!("a saved configuration is already called {:?}", e.name),
            ));
        }
        let mut sheets = self.sheets.names();
        sheets.extend(named_style_sheet(forms).filter(|n| !sheets.contains(n)));
        let db = self
            .defaults(template, &sheets, on)
            .map_err(|err| skip(SkipReason::InvalidOption, err.message))?;
        let (changed, wire) = configure(db, forms, on).map_err(|err| skip(SkipReason::InvalidOption, option_message(&err)))?;
        let saved = SavedReport {
            guid,
            name: e.name.clone(),
            template: template.guid.0,
            options: changed.clone(),
            other: Map::new(),
        };
        let configuration = ReportInfo::saved(&saved, Some(template));
        let options = ReportOptions(Value::Object(changed));
        let imported = ImportedConfiguration {
            line: e.line,
            configuration,
            options,
            ignored: ignored(&wire),
        };
        Ok((saved, imported))
    }
}

/// The style sheet an entry's General/"Stylesheet" option names, so it is a choice even before that style sheet is
/// imported; it then renders with the base look, as for a deleted one (`design/reports.md` §8).
fn named_style_sheet(forms: &[OptionForm]) -> Option<&str> {
    let form = forms.iter().find(|f| f.section == GENERAL && f.name == STYLESHEET)?;
    form.value.as_symbol().or_else(|| form.value.as_str())
}

/// The options that differ from the defaults once the entry's values are set, and the wire values.
fn configure(mut db: OptionDb, forms: &[OptionForm], on: &On<'_>) -> Result<(Map<String, Value>, WireOptions), OptionError> {
    let wire = import::to_wire(&db, forms, |_, _| false)?;
    db.apply(&Value::Object(wire.options.clone()), &ValueContext { book: None, zone: on.zone })?;
    Ok((db.changed(), wire))
}

/// Import style sheets. An entry for a predefined style sheet holds changes to it; one with a new name is a user
/// style sheet. A name the Configuration already has an entry for is skipped, so edits made in red-ale are kept.
pub(super) fn style_sheets(ctx: &mut Context, p: ImportParams) -> Result<ImportStyleSheetsResult, RpcError> {
    let lock = ctx.settings.config.lock()?;
    let config = ctx.settings.config.load()?;
    let mut records = config.style_sheets()?;
    let mut out = ImportStyleSheetsResult {
        dry_run: p.dry_run,
        imported: Vec::new(),
        skipped: Vec::new(),
    };
    import::style_sheets(&p.text).into_iter().for_each(|entry| {
        match entry.map_err(SkippedEntry::malformed).and_then(|e| style_sheet_entry(e, &records)) {
            Ok((record, imported)) => {
                records.push(record);
                out.imported.push(imported);
            }
            Err(skipped) => out.skipped.push(skipped),
        }
    });
    if !p.dry_run && !out.imported.is_empty() {
        sheets::store(ctx, &lock, config, &records)?;
    }
    Ok(out)
}

/// One style sheet entry as a Configuration record, or why it is skipped.
fn style_sheet_entry(e: StyleSheetForm, records: &[StyleSheetRecord]) -> Result<(StyleSheetRecord, ImportedStyleSheet), SkippedEntry> {
    let skip = |reason, message: String| SkippedEntry {
        line: e.line,
        name: Some(e.name.clone()),
        guid: None,
        template: Some(e.template.clone()),
        reason,
        message,
    };
    let template = StyleSheetTemplate::from_name(&e.template).ok_or_else(|| {
        skip(
            SkipReason::Untranslated,
            format!("red-ale has no style sheet template {:?}", e.template),
        )
    })?;
    let forms = e.options.as_ref().map_err(|m| skip(SkipReason::Malformed, m.clone()))?;
    if records.iter().any(|r| r.name == e.name) {
        return Err(skip(
            SkipReason::Exists,
            format!("the Configuration already has an entry for style sheet {:?}", e.name),
        ));
    }
    let mut sheet = StyleSheet::new(e.name.clone(), template);
    let wire = import::to_wire(sheet.options(), forms, left_out).map_err(|err| skip(SkipReason::InvalidOption, option_message(&err)))?;
    sheet
        .apply(&Value::Object(wire.options.clone()))
        .map_err(|err| skip(SkipReason::InvalidOption, option_message(&err)))?;
    let record = record(&sheet, &wire.kept, Map::new());
    if record.options.is_empty() && PREDEFINED.contains(&(e.name.as_str(), template)) {
        return Err(skip(
            SkipReason::Unchanged,
            "a predefined style sheet at its template's defaults".to_owned(),
        ));
    }
    Ok((
        record,
        ImportedStyleSheet {
            line: e.line,
            style_sheet: StyleSheetInfo::of(&sheet),
            ignored: ignored(&wire),
        },
    ))
}
