//! The child reports a report placement option names (`design/notes/report-statements-charts.md` §8.4, §8.5).
//!
//! - **Checked when set:** each must be a template the registry has, or a saved configuration the host keeps, and a
//!   template child's options must be valid options of that template. Never checked when a saved configuration's
//!   stored values apply.
//! - **Run by the host:** a view's script lays out a `grid` of cells naming children by position; the host runs each
//!   child with the view's Book, now, zone, settings and Locale, under the top-level run's deadline, and puts its
//!   result in the cell. A child that cannot run (gone, failing, past a limit, in a cycle) leaves a warning there.

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::fmt;
use std::time::Instant;

use red_ale_core::Guid;
use serde_json::{Map, Value};

use super::registry::{Registry, TemplateInfo};
use super::run::RunContext;
use crate::error::{OptionError, ReportError, ResultError};
use crate::options::{GENERAL, OptionDb, Placement, REPORT_NAME, STYLESHEET, ValueContext};
use crate::result::{Block, EmbeddedResult, GridCell, Link, OptionsSummaryRow, ReportLink, Section, WarningBlock};
use crate::wire::Id;

/// How deep children nest below the top-level run: a view's children are at depth 1, theirs at 2. A child deeper
/// than this is not run (`design/milestone5.md`, "Decided", item 3).
pub const MAX_DEPTH: usize = 4;

/// How many children one top-level run runs, at every depth together.
pub const MAX_CHILD_RUNS: usize = 100;

/// The heading of a child's warning, GnuCash's (`5.17:gnucash/report/reports/standard/view-column.scm:95`).
const REPORT_ERROR: &str = "Report error";

/// The saved report configurations of the host's Configuration (`design/reports.md` §9), which a child may name.
pub trait Configurations {
    /// The template the saved configuration `guid` configures and the option values it stores, by section and name in
    /// wire form; `None` when no saved configuration has that GUID.
    fn configuration(&self, guid: Guid) -> Option<(Guid, &Map<String, Value>)>;
}

/// No saved configurations.
impl Configurations for () {
    fn configuration(&self, _: Guid) -> Option<(Guid, &Map<String, Value>)> {
        None
    }
}

impl fmt::Debug for dyn Configurations + '_ {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Configurations")
    }
}

impl Registry {
    /// Check the children of every report placement option `changes` sets in `db`, whose values are already applied:
    /// each names a template or a saved configuration, and only a template child has options, valid for its template
    /// over its defaults and read against `values`. A template child's options are kept as the values that differ
    /// from its defaults, in wire form, as a saved configuration keeps its own. The first child refused is a
    /// `report-option` error naming the option and the child's position; nothing changes then.
    pub fn check_children(
        &self,
        db: &mut OptionDb,
        changes: &Value,
        ctx: &RunContext<'_>,
        values: &ValueContext<'_>,
    ) -> Result<(), OptionError> {
        db.check_placements(changes, &|child| self.check_child(child, ctx, values))
    }

    fn check_child(&self, child: &Placement, ctx: &RunContext<'_>, values: &ValueContext<'_>) -> Result<Placement, String> {
        if let Some(template) = self.template(child.report) {
            let options = child
                .options
                .as_ref()
                .map(|o| self.child_options(template, o, ctx, values))
                .transpose()?;
            return Ok(Placement {
                options: options.filter(|o| !o.is_empty()),
                ..child.clone()
            });
        }
        match ctx.configurations.configuration(child.report) {
            Some(_) if child.options.is_some() => Err(format!(
                "{} is a saved report configuration, which runs with its own values: options go only with a template's GUID",
                child.report
            )),
            Some(_) => Ok(child.clone()),
            None => Err(format!(
                "no report template or saved report configuration has GUID {}",
                child.report
            )),
        }
    }

    /// A template child's options, checked as `report.run` checks them (its children too), as the values that differ
    /// from the template's defaults.
    fn child_options(
        &self,
        template: &TemplateInfo,
        options: &Map<String, Value>,
        ctx: &RunContext<'_>,
        values: &ValueContext<'_>,
    ) -> Result<Map<String, Value>, String> {
        let changes = Value::Object(options.clone());
        let mut db = self.options(template, ctx).map_err(|e| format!("{}: {e}", template.name))?;
        db.apply(&changes, values)
            .and_then(|()| self.check_children(&mut db, &changes, ctx, values))
            .map_err(|e| format!("{} option {}/{}: {}", template.name, e.section, e.name, e.reason))?;
        Ok(db.changed())
    }
}

/// What a top-level run and every child it runs share.
pub(super) struct Tree {
    /// When the run time limit is passed.
    pub deadline: Instant,
    /// The children run so far.
    runs: Cell<usize>,
    /// The saved configurations being run, outermost first: one met again is a cycle.
    saved: RefCell<Vec<Guid>>,
}

impl Tree {
    /// The tree of a top-level run, of the saved configuration `top` if it is one.
    pub(super) fn new(deadline: Instant, top: Option<Guid>) -> Self {
        Self {
            deadline,
            runs: Cell::new(0),
            saved: RefCell::new(top.into_iter().collect()),
        }
    }
}

/// Where a run is in its tree.
pub(super) struct Embed<'t> {
    /// The tree.
    pub tree: &'t Tree,
    /// 0 for the top-level run.
    pub depth: usize,
}

/// The template a child runs and, for a saved configuration, its GUID and stored values.
type Found<'t, 'c> = (&'t TemplateInfo, Option<(Guid, &'c Map<String, Value>)>);

/// A child's link and its result, or the warning in its place.
type Outcome = (Option<Link>, Result<Box<EmbeddedResult>, WarningBlock>);

fn warning(text: impl Into<String>) -> WarningBlock {
    WarningBlock {
        title: REPORT_ERROR.to_owned(),
        text: text.into(),
        class: None,
    }
}

impl Registry {
    /// Run the children the grid blocks of `sections` place, once each, and put each one's result, or the warning in
    /// its place, and its "Single Report" link in its cells. A cell naming no child of `placements` is `invalid`'s
    /// error; a child past a limit of the engine (`report-limit`) stops the whole run, as the limits bound the
    /// top-level request. Gives the options summary's `Embedded Report` rows, one per child in order, each naming the
    /// child by its "Report name" (`5.17:gnucash/report/html-utilities.scm:339-343`), or by GUID when it did not run.
    pub(super) fn fill_grids(
        &self,
        sections: &mut [Section],
        placements: &[Placement],
        ctx: &RunContext<'_>,
        embed: &Embed<'_>,
        invalid: &dyn Fn(ResultError) -> ReportError,
    ) -> Result<Vec<OptionsSummaryRow>, ReportError> {
        let mut fill = Fill {
            registry: self,
            placements,
            ctx,
            embed,
            invalid,
            ran: BTreeMap::new(),
        };
        sections
            .iter_mut()
            .enumerate()
            .try_for_each(|(i, s)| fill.blocks(&mut s.blocks, &format!("sections[{i}].blocks")))?;
        let name = |i: usize, p: &Placement| match fill.ran.get(&i) {
            Some((_, Ok(child))) => report_name(child),
            _ => p.report.to_string(),
        };
        Ok(placements
            .iter()
            .enumerate()
            .map(|(i, p)| OptionsSummaryRow {
                section: String::new(),
                name: "Embedded Report".to_owned(),
                value: name(i, p),
            })
            .collect())
    }

    /// Run one child, or say why it cannot run.
    fn child(&self, p: &Placement, ctx: &RunContext<'_>, embed: &Embed<'_>) -> Result<Outcome, ReportError> {
        let (template, saved) = match self.resolve(p, ctx) {
            Ok(found) => found,
            Err(text) => return Ok((None, Err(warning(text)))),
        };
        let link = Link::Report(ReportLink {
            template: Id(template.guid.0),
            configuration: saved.map(|(guid, _)| Id(guid)),
            options: p.options.clone().unwrap_or_default(),
        });
        if let Some(text) = refused(saved.map(|(guid, _)| guid), embed) {
            return Ok((Some(link), Err(warning(text))));
        }
        let tree = embed.tree;
        tree.runs.set(tree.runs.get() + 1);
        let stored = saved.map(|(guid, options)| (guid, Value::Object(without_style_sheet(options))));
        let changes = p.options.clone().map(Value::Object);
        if let Some((guid, _)) = stored {
            tree.saved.borrow_mut().push(guid);
        }
        let below = Embed {
            tree,
            depth: embed.depth + 1,
        };
        let child_ctx = RunContext { export: None, ..*ctx };
        let ran = self.run_in(
            template,
            stored.as_ref().map(|(g, v)| (*g, v)),
            changes.as_ref(),
            &child_ctx,
            &below,
        );
        if stored.is_some() {
            tree.saved.borrow_mut().pop();
        }
        match ran {
            Ok(result) => Ok((Some(link), Ok(Box::new(EmbeddedResult::from(result))))),
            Err(e @ (ReportError::Limit(_) | ReportError::Internal(_))) => Err(e),
            Err(e) => Ok((
                Some(link),
                Err(warning(format!("An error occurred while running the report.\n{e}"))),
            )),
        }
    }

    /// The template a child runs and, for a saved configuration, its GUID and stored values; or why there is none.
    fn resolve<'c>(&self, p: &Placement, ctx: &RunContext<'c>) -> Result<Found<'_, 'c>, String> {
        if let Some(template) = self.template(p.report) {
            return Ok((template, None));
        }
        let (template, stored) = ctx.configurations.configuration(p.report).ok_or_else(|| {
            format!(
                "No report template or saved report configuration has GUID {}; it may have been deleted.",
                p.report
            )
        })?;
        let template = self
            .template(template)
            .ok_or_else(|| format!("The template {template} of saved report configuration {} is not loaded.", p.report))?;
        Ok((template, Some((p.report, stored))))
    }
}

/// A saved configuration's values without its style sheet: the view's style sheet renders every child
/// (`5.17:gnucash/report/reports/standard/view-column.scm:84-85`), and one since deleted must not stop the child.
fn without_style_sheet(options: &Map<String, Value>) -> Map<String, Value> {
    let mut options = options.clone();
    if let Some(Value::Object(general)) = options.get_mut(GENERAL) {
        general.remove(STYLESHEET);
    }
    options
}

/// Why a child that exists is not run: too deep, past the count, or a saved configuration already being run further
/// up, a cycle only red-ale's references can form (`design/notes/report-statements-charts.md` §8.5).
fn refused(saved: Option<Guid>, embed: &Embed<'_>) -> Option<String> {
    if embed.depth + 1 > MAX_DEPTH {
        Some(format!(
            "Reports embed other reports at most {MAX_DEPTH} deep; this one is not run."
        ))
    } else if embed.tree.runs.get() >= MAX_CHILD_RUNS {
        Some(format!(
            "A report runs at most {MAX_CHILD_RUNS} embedded reports; this one is not run."
        ))
    } else {
        saved
            .filter(|g| embed.tree.saved.borrow().contains(g))
            .map(|guid| format!("The saved report configuration {guid} embeds itself; it is not run again."))
    }
}

/// A child's "Report name" option, as `gnc:report-name` gives it; its title without one.
fn report_name(child: &EmbeddedResult) -> String {
    child
        .options
        .get(GENERAL)
        .and_then(|g| g.get(REPORT_NAME))
        .and_then(Value::as_str)
        .map_or_else(|| child.title.clone(), str::to_owned)
}

/// The walk over a result's blocks that fills its grid cells.
struct Fill<'f, 'c> {
    registry: &'f Registry,
    placements: &'f [Placement],
    ctx: &'f RunContext<'c>,
    embed: &'f Embed<'f>,
    invalid: &'f dyn Fn(ResultError) -> ReportError,
    /// Each child run so far, by position.
    ran: BTreeMap<usize, Outcome>,
}

impl Fill<'_, '_> {
    fn blocks(&mut self, blocks: &mut [Block], path: &str) -> Result<(), ReportError> {
        blocks.iter_mut().enumerate().try_for_each(|(i, b)| {
            let at = format!("{path}[{i}]");
            match b {
                Block::Group(g) => self.blocks(&mut g.blocks, &format!("{at}.blocks")),
                Block::Grid(g) => g.rows.iter_mut().enumerate().try_for_each(|(r, row)| {
                    row.cells
                        .iter_mut()
                        .enumerate()
                        .try_for_each(|(c, cell)| self.cell(cell, &format!("{at}.rows[{r}].cells[{c}].child")))
                }),
                _ => Ok(()),
            }
        })
    }

    fn cell(&mut self, cell: &mut GridCell, path: &str) -> Result<(), ReportError> {
        let index = usize::try_from(cell.child).unwrap_or(usize::MAX);
        let Some(p) = self.placements.get(index) else {
            let count = self.placements.len();
            return Err((self.invalid)(ResultError::new(
                path,
                format!("the report has {count} children, from 0"),
            )));
        };
        if !self.ran.contains_key(&index) {
            let outcome = self.registry.child(p, self.ctx, self.embed)?;
            self.ran.insert(index, outcome);
        }
        if let Some((link, outcome)) = self.ran.get(&index) {
            cell.link.clone_from(link);
            (cell.result, cell.warning) = match outcome {
                Ok(child) => (Some(child.clone()), None),
                Err(w) => (None, Some(w.clone())),
            };
        }
        Ok(())
    }
}
