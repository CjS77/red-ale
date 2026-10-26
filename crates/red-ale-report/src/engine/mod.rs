//! The Rhai engine (`design/reports.md` §2): the template registry, the engine each run builds, its limits and
//! module resolver, and running a template into a report result.
//!
//! The service hands in what it read (user scripts as text, the settings, the time zone, now); this module does no
//! I/O. The Rust API the RPC layer calls:
//!
//! - [`Registry::load`] the built-in [`Library`](crate::library::Library) and the user's [`ScriptSource`]s;
//!   [`Registry::problems`] lists user scripts that did not load;
//! - [`Registry::templates`], [`Registry::template`], [`Registry::find`] to list and look templates up;
//! - [`Registry::options`] for a template's [`OptionDb`](crate::options::OptionDb) (`describe`, `params_schema`);
//! - [`Registry::run`] for its [`ReportResult`](crate::result::ReportResult), with a [`RunContext`].
//!
//! Each run builds its own `Engine` over a snapshot of the Book; nothing is shared between runs.

mod context;
mod convert;
mod errors;
mod log;
mod numbers;
mod options;
mod registry;
mod resolver;
mod run;
mod setup;

pub use context::ContextHandle;
pub use options::{OptionBuilder, OptionsHandle};
pub(crate) use registry::BUILTIN_DIRS;
pub use registry::{Lookup, Registry, ScriptSource, TemplateInfo, TemplateKind};
pub use run::RunContext;

