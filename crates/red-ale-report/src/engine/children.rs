//! The child reports a report placement option names (`design/notes/report-statements-charts.md` §8.4): each must be
//! a template the registry has, or a saved configuration the host keeps, and a template child's options must be valid
//! options of that template. Checked when a caller sets them, never when a saved configuration's stored values apply.

use std::fmt;

use red_ale_core::Guid;
use serde_json::{Map, Value};

use super::registry::{Registry, TemplateInfo};
use super::run::RunContext;
use crate::error::OptionError;
use crate::options::{OptionDb, Placement, ValueContext};

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
            None => Err(format!("no report template or saved report configuration has GUID {}", child.report)),
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
