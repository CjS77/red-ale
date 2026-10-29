//! The engine of a run (`design/reports.md` §2, "The engine for a run"): `Engine::new_raw()` with Rhai's standard
//! package, `eval` and `sleep` disabled, strict variables and map properties, every limit set, `print` and `debug`
//! going to the report log, red-ale's module resolver, and only the types and functions of the report API.

use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use red_ale_locale::Conventions;
use rhai::packages::{Package, StandardPackage};
use rhai::{Dynamic, Engine};

use super::log::Log;
use super::registry::Sources;
use super::resolver::{Defined, Resolver};
use super::{context, memo, numbers, options, regexp};
use crate::limits::{Limit, Limits};
use crate::number::SizeCap;
use crate::query::{self, Snapshot};
use crate::result::LogLevel;

/// What an engine is built over.
pub(crate) struct Setup {
    /// The Book the run reads.
    pub snap: Rc<Snapshot>,
    /// Every script, for the module resolver.
    pub sources: Arc<Sources>,
    /// The run's limits.
    pub limits: Limits,
    /// The report log.
    pub log: Log,
    /// The Locale's number format, for `m.to_string()`.
    pub conventions: Conventions,
    /// The functions of the scripts the run has compiled, which the resolver checks each module against.
    pub defined: Defined,
    /// When the run time limit is passed: one deadline for a report and the children it embeds.
    pub deadline: Instant,
}

/// The deadline of a run starting now under `limits`.
pub(crate) fn deadline(limits: &Limits) -> Instant {
    let max = Duration::from_secs(limits.get(Limit::Time));
    Instant::now()
        .checked_add(max)
        .unwrap_or_else(|| Instant::now() + Duration::from_secs(u64::from(u32::MAX)))
}

/// How often, in operations, the run time is checked against the host clock.
const CLOCK_EVERY: u64 = 1024;

/// A configured engine, stopping at the setup's deadline.
pub(crate) fn engine(s: &Setup) -> Engine {
    let mut engine = Engine::new_raw();
    engine.register_global_module(StandardPackage::new().as_shared_module());
    // `eval` would compile code at run time.
    engine.disable_symbol("eval");
    // The standard package's `sleep` would block past the run-time limit, which is checked only between operations.
    // Functions registered on the engine are found before the package's.
    engine.register_fn("sleep", |_: rhai::INT| -> Result<(), Box<rhai::EvalAltResult>> {
        Err("sleep is not available to reports".into())
    });
    engine.set_strict_variables(true);
    engine.set_fail_on_invalid_map_property(true);
    limits(&mut engine, &s.limits, s.deadline);
    log(&mut engine, &s.log);
    engine.set_module_resolver(Resolver::new(Arc::clone(&s.sources), s.defined.clone()));
    numbers::register(&mut engine, &s.snap, SizeCap::bits(s.limits.get(Limit::NumberSize)), s.conventions);
    query::register(&mut engine);
    context::register(&mut engine);
    memo::register(&mut engine, s.limits.get_usize(Limit::MapSize));
    regexp::register(&mut engine);
    options::register_builder(&mut engine);
    options::register_reader(&mut engine);
    engine
}

fn limits(engine: &mut Engine, l: &Limits, deadline: Instant) {
    engine.set_max_operations(l.get(Limit::Operations));
    engine.set_max_call_levels(l.get_usize(Limit::CallDepth));
    let depth = l.get_usize(Limit::ExpressionDepth);
    engine.set_max_expr_depths(depth, depth / 2);
    engine.set_max_string_size(l.get_usize(Limit::StringSize));
    engine.set_max_array_size(l.get_usize(Limit::ArraySize));
    engine.set_max_map_size(l.get_usize(Limit::MapSize));
    engine.set_max_modules(l.get_usize(Limit::Modules));
    engine.set_max_variables(l.get_usize(Limit::Variables));
    engine.set_max_functions(l.get_usize(Limit::Functions));
    // The clock only ever ends a run early; it never changes what a run returns.
    engine.on_progress(move |ops| (ops % CLOCK_EVERY == 0 && Instant::now() >= deadline).then_some(Dynamic::UNIT));
}

fn log(engine: &mut Engine, log: &Log) {
    let printed = log.clone();
    engine.on_print(move |text| printed.push(LogLevel::Print, text, None, None));
    let debugged = log.clone();
    engine.on_debug(move |text, source, pos| {
        let line = pos.line().and_then(|l| u32::try_from(l).ok());
        debugged.push(LogLevel::Debug, text, source, line);
    });
}
