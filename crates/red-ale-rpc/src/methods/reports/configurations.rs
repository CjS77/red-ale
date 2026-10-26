//! Saved report configurations: a template's options saved under a new name and GUID in red-ale's Configuration,
//! GnuCash's custom reports (`design/reports.md` §9). Each stores only the options that differ from the template's
//! defaults (`5.17:bindings/guile/options.scm:141`), so it follows the template's later defaults for the rest.

use red_ale_service::config::Config;
use red_ale_service::reports::SavedReport;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Map;

use super::{On, ReportInfo, ReportOptions, ReportRef, Reports, name, taken};
use crate::dispatch::Context;
use crate::error::RpcError;

const WHAT: &str = "saved report configuration";

/// Params of `report.saveConfiguration`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct SaveParams {
    /// The new configuration's name, unique among saved configurations.
    pub name: String,
    /// The report whose options to save: a template, or a saved configuration, whose template the new one then
    /// configures.
    pub report: ReportRef,
    /// Changes over the report's options before saving.
    #[serde(default)]
    pub options: Option<ReportOptions>,
}

/// Params of `report.updateConfiguration`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct UpdateParams {
    /// The saved configuration, by GUID or name.
    pub configuration: ReportRef,
    /// The options to set; `null` returns one to the template's default.
    pub options: ReportOptions,
}

/// Params of `report.renameConfiguration`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct RenameParams {
    /// The saved configuration, by GUID or name.
    pub configuration: ReportRef,
    /// Its new name.
    pub name: String,
}

/// Params of `report.deleteConfiguration`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct DeleteParams {
    /// The saved configuration, by GUID or name.
    pub configuration: ReportRef,
}

/// The result of `report.deleteConfiguration`.
#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DeleteResult {
    /// The configuration deleted.
    pub deleted: ReportInfo,
}

/// Save a report's options under a new name. A clash is `conflict`, where GnuCash appends a number
/// (`5.17:gnucash/report/report-core.scm:515-532`), so the caller gets the name it asked for.
pub(super) fn save(ctx: &mut Context, p: SaveParams) -> Result<ReportInfo, RpcError> {
    let reports = Reports::load(ctx)?;
    let name = name(&p.name, WHAT)?;
    if reports.saved.iter().any(|s| s.name == name) {
        return Err(taken(WHAT, &name));
    }
    let (saved, info) = ctx.query_any(|book, env| {
        let (template, base) = reports.resolve(reports.find(&p.report.0)?)?;
        let now = env.now();
        let db = reports.configured(template, base, p.options.as_ref().map(|o| &o.0), &On { book, now, zone: env.zone() })?;
        let saved = SavedReport { guid: env.new_guid(), name, template: template.guid.0, options: db.changed(), other: Map::new() };
        let info = ReportInfo::saved(&saved, Some(template));
        Ok((saved, info))
    })?;
    let mut all = reports.saved;
    all.push(saved);
    store(ctx, reports.config, &all)?;
    Ok(info)
}

/// Set options of a saved configuration, keeping its GUID and name, as GnuCash's "Save Report Configuration" over an
/// existing one does (`5.17:gnucash/report/report-core.scm:705-719`).
pub(super) fn update(ctx: &mut Context, p: UpdateParams) -> Result<ReportInfo, RpcError> {
    let reports = Reports::load(ctx)?;
    let current = reports.configuration(&p.configuration.0)?;
    let template = reports.template_of(current)?;
    let options = ctx.query_any(|book, env| {
        let now = env.now();
        Ok(reports.configured(template, Some(current), Some(&p.options.0), &On { book, now, zone: env.zone() })?.changed())
    })?;
    let updated = SavedReport { options, ..current.clone() };
    let info = ReportInfo::saved(&updated, Some(template));
    let all = replace(&reports.saved, updated);
    store(ctx, reports.config, &all)?;
    Ok(info)
}

/// Rename a saved configuration; a name another one has is `conflict`, as GnuCash's dialog refuses it
/// (`5.17:gnucash/gnome/dialog-custom-report.c:462-494`).
pub(super) fn rename(ctx: &mut Context, p: RenameParams) -> Result<ReportInfo, RpcError> {
    let reports = Reports::load(ctx)?;
    let current = reports.configuration(&p.configuration.0)?;
    let name = name(&p.name, WHAT)?;
    if reports.saved.iter().any(|s| s.name == name && s.guid != current.guid) {
        return Err(taken(WHAT, &name));
    }
    let renamed = SavedReport { name, ..current.clone() };
    let info = ReportInfo::saved(&renamed, reports.registry.template(renamed.template));
    let all = replace(&reports.saved, renamed);
    store(ctx, reports.config, &all)?;
    Ok(info)
}

/// Delete a saved configuration (`gnc:delete-report`, `5.17:gnucash/report/report-core.scm:811-818`).
pub(super) fn delete(ctx: &mut Context, p: DeleteParams) -> Result<DeleteResult, RpcError> {
    let reports = Reports::load(ctx)?;
    let current = reports.configuration(&p.configuration.0)?;
    let deleted = ReportInfo::saved(current, reports.registry.template(current.template));
    let guid = current.guid;
    let all: Vec<SavedReport> = reports.saved.iter().filter(|s| s.guid != guid).cloned().collect();
    store(ctx, reports.config, &all)?;
    Ok(DeleteResult { deleted })
}

/// `saved` with the entry of `changed`'s GUID replaced, in its place.
fn replace(saved: &[SavedReport], changed: SavedReport) -> Vec<SavedReport> {
    saved.iter().map(|s| if s.guid == changed.guid { changed.clone() } else { s.clone() }).collect()
}

/// Write the saved configurations to the Configuration, atomically.
fn store(ctx: &Context, mut config: Config, saved: &[SavedReport]) -> Result<(), RpcError> {
    config.set_saved_reports(saved);
    Ok(ctx.settings.config.save(&config)?)
}
