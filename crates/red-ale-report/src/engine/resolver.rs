//! red-ale's module resolver (`design/reports.md` §2, "The module resolver"). It replaces Rhai's file resolver, which
//! would let `import` read any path, and knows only the scripts handed to the engine:
//!
//! - `import "lib/<name>"`, `"standard/<name>"`, `"example/<name>"`: a built-in script;
//! - `import "./<path>"`: a user script, relative to the importing one, from a user script only.
//!
//! Anything else (an absolute path, `..`, a `./` path from a built-in script, a name no script has) is "module not
//! found". Every script is text in memory; this crate reads no file.
//!
//! A module that defines a function with the name and arity of one another script of the run defines is refused:
//! Rhai could call either in place of the other. So is a script that defines a function a call of an engine function
//! would reach (`design/reports.md` §2, "One name, one function").

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;
use std::sync::Arc;

use regex::Regex;
use rhai::{AST, Engine, EvalAltResult, Module, ModuleResolver, Position, Scope, Shared};

use super::registry::{BUILTIN_DIRS, Sources, USER_PREFIX};
use super::setup::engine_functions;

/// A function's name and number of parameters, which is all a Rhai call resolves a script function by.
type Signature = (String, usize);

/// The functions of one script, private ones included, or of the engine.
pub(crate) type Functions = BTreeSet<Signature>;

/// The functions every template script has, which the engine calls through the script's own AST and another script
/// reaches only qualified (`transaction_report::render`), so two template scripts may both define them.
const ENTRY_POINTS: [(&str, usize); 3] = [("templates", 0), ("options", 3), ("render", 4)];

/// The functions each script of a run defines: the script run, then each module as it is imported. Rhai keeps one
/// function resolution cache for a whole run, keyed by name and arity alone, so a function one script resolves can
/// be called in another's place; no two scripts of a run may define the same name and arity.
#[derive(Debug, Clone, Default)]
pub(crate) struct Defined(Rc<RefCell<Vec<(String, Functions)>>>);

impl Defined {
    /// Add the functions of the script the run compiles first, before any import, so with only the engine's to
    /// check them against. The error is the one [`engine_clash`] gives.
    pub(crate) fn start(&self, source: &str, text: &str, ast: &AST) -> Result<(), (String, Position)> {
        let mine = signatures(ast);
        if let Some(clash) = engine_clash(source, text, &mine) {
            return Err(clash);
        }
        self.0.borrow_mut().push((source.to_owned(), mine));
        Ok(())
    }

    /// Add the functions of `source`, compiled to `ast` from `text`. The error names a function a call of an engine
    /// function would reach, or one another script of the run already defines, and that script. A script imported by
    /// itself is the same code, so it is not compared with itself; the resolver refuses that import anyway.
    pub(crate) fn add(&self, source: &str, text: &str, ast: &AST) -> Result<(), Clash> {
        let mine = signatures(ast);
        if let Some((message, at)) = engine_clash(source, text, &mine) {
            return Err(Clash::Engine(message, at));
        }
        let shared = |theirs: &Functions| {
            let templates = is_template(&mine) && is_template(theirs);
            mine.intersection(theirs).find(|s| !(templates && is_entry_point(s))).cloned()
        };
        let clash = self
            .0
            .borrow()
            .iter()
            .filter(|(other, _)| other != source)
            .find_map(|(other, theirs)| shared(theirs).map(|(name, arity)| (other.clone(), name, arity)));
        if let Some((other, name, arity)) = clash {
            return Err(Clash::Scripts(format!(
                "{source} and {other} both define fn {name} with {arity} parameters; Rhai may call either in the \
                 other's place, so one must be renamed"
            )));
        }
        self.0.borrow_mut().push((source.to_owned(), mine));
        Ok(())
    }
}

/// Why a module's functions are refused.
#[derive(Debug)]
pub(crate) enum Clash {
    /// Another script of the run defines a function of the same name and arity: located at the import.
    Scripts(String),
    /// A call of an engine function would reach one of the module's: located at that function.
    Engine(String, Position),
}

/// A function of `mine`, from `source`, that a call of an engine function would reach instead, with the error and
/// where the function is in `text`. Rhai looks a call up among the script functions before the engine's, by name and
/// number of arguments alone, whatever their types (rhai 1.27.0 `src/func/func_call.rs:818-835`), and a module's
/// call finds the importing script's functions too (`src/func/script.rs:377`). A method call `x.f(a)` looks for a
/// script `fn f(a)` reading `x` as `this` before the engine's `f(x, a)` (`src/parser.rs:2127-2136`). So a script
/// `fn f` of n parameters replaces the engine's `f` of n, and of n + 1 called as a method.
fn engine_clash(source: &str, text: &str, mine: &Functions) -> Option<(String, Position)> {
    let engine = engine_functions();
    mine.iter().find_map(|(name, arity)| {
        let replaced = [*arity, arity + 1].into_iter().find(|n| engine.contains(&(name.clone(), *n)))?;
        let how = if replaced == *arity {
            format!("which replaces the report engine's {name}")
        } else {
            format!("which a method call x.{name}(…) reaches in place of the report engine's {name} of {replaced}")
        };
        let message = format!(
            "{source} defines fn {name} with {}, {how} in every script of the run, so it must be renamed",
            parameters(*arity)
        );
        Some((message, position_of(text, name)))
    })
}

fn parameters(n: usize) -> String {
    if n == 1 {
        "1 parameter".to_owned()
    } else {
        format!("{n} parameters")
    }
}

/// Where `fn name` (or `fn Type.name`) is in `text`, at the name, or no position if it is not there. Rhai keeps no
/// position for a script function, so the text is searched; a definition commented out above it would be found
/// first.
fn position_of(text: &str, name: &str) -> Position {
    let pattern = format!(r#"\bfn\s+(?:(?:"[^"]*"|\w+)\s*\.\s*)?({})\s*\("#, regex::escape(name));
    let found = Regex::new(&pattern)
        .ok()
        .and_then(|re| re.captures(text)?.get(1))
        .map(|m| m.start());
    found.map_or(Position::NONE, |at| {
        let before = &text[..at];
        let line = before.matches('\n').count() + 1;
        let column = before.rsplit('\n').next().map_or(0, |l| l.chars().count()) + 1;
        match (u16::try_from(line), u16::try_from(column)) {
            (Ok(line), Ok(column)) => Position::new(line, column),
            _ => Position::NONE,
        }
    })
}

fn signatures(ast: &AST) -> Functions {
    ast.iter_functions().map(|f| (f.name.to_owned(), f.params.len())).collect()
}

fn is_entry_point((name, arity): &Signature) -> bool {
    ENTRY_POINTS.iter().any(|(n, a)| n == name && a == arity)
}

fn is_template(functions: &Functions) -> bool {
    functions.contains(&("templates".to_owned(), 0))
}

/// The resolver of one run, caching each module it evaluates.
pub(crate) struct Resolver {
    sources: Arc<Sources>,
    defined: Defined,
    cache: RefCell<BTreeMap<String, Shared<Module>>>,
    loading: RefCell<BTreeSet<String>>,
}

impl Resolver {
    pub(crate) fn new(sources: Arc<Sources>, defined: Defined) -> Self {
        Self {
            sources,
            defined,
            cache: RefCell::new(BTreeMap::new()),
            loading: RefCell::new(BTreeSet::new()),
        }
    }

    /// The source name `path` names when imported from `importer`, if it names a script there is.
    fn locate(&self, importer: Option<&str>, path: &str) -> Option<String> {
        let path = path.strip_suffix(".rhai").unwrap_or(path);
        let name = if let Some(rest) = path.strip_prefix(USER_PREFIX) {
            let importer = importer.filter(|i| i.starts_with(USER_PREFIX))?;
            let dir = importer.rsplit_once('/').map_or(".", |(dir, _)| dir);
            plain_components(rest).then(|| format!("{dir}/{rest}.rhai"))?
        } else {
            let builtin = BUILTIN_DIRS
                .iter()
                .any(|d| path.strip_prefix(d).is_some_and(|rest| rest.starts_with('/')));
            (builtin && plain_components(path)).then(|| format!("{path}.rhai"))?
        };
        self.sources.contains_key(&name).then_some(name)
    }

    fn evaluate(&self, engine: &Engine, name: &str, pos: Position) -> Result<Shared<Module>, Box<EvalAltResult>> {
        let in_module = |e: Box<EvalAltResult>| -> Box<EvalAltResult> { EvalAltResult::ErrorInModule(name.to_owned(), e, pos).into() };
        let not_found = || EvalAltResult::ErrorModuleNotFound(name.to_owned(), pos);
        let text = self.sources.get(name).map(|s| Arc::clone(&s.text)).ok_or_else(not_found)?;
        let mut ast = engine.compile(&*text).map_err(|e| in_module(e.into()))?;
        ast.set_source(name);
        self.defined.add(name, &text, &ast).map_err(|clash| match clash {
            // Located at the import, in the importing script, rather than inside the module.
            Clash::Scripts(m) => EvalAltResult::ErrorRuntime(m.into(), pos).into(),
            Clash::Engine(m, at) => in_module(EvalAltResult::ErrorRuntime(m.into(), at).into()),
        })?;
        if !self.loading.borrow_mut().insert(name.to_owned()) {
            return Err(in_module(format!("module {name} imports itself").into()));
        }
        let evaluated = Module::eval_ast_as_new(Scope::new(), &ast, engine);
        self.loading.borrow_mut().remove(name);
        let mut module = evaluated.map_err(in_module)?;
        module.set_id(name);
        let module: Shared<Module> = module.into();
        self.cache.borrow_mut().insert(name.to_owned(), Shared::clone(&module));
        Ok(module)
    }
}

/// True when every `/`-separated part of `path` is a plain name: not empty, not `.` or `..`, no backslash.
fn plain_components(path: &str) -> bool {
    !path.is_empty() && path.split('/').all(|c| !c.is_empty() && c != "." && c != ".." && !c.contains('\\'))
}

impl ModuleResolver for Resolver {
    fn resolve(&self, engine: &Engine, source: Option<&str>, path: &str, pos: Position) -> Result<Shared<Module>, Box<EvalAltResult>> {
        let name = self
            .locate(source, path)
            .ok_or_else(|| EvalAltResult::ErrorModuleNotFound(path.to_owned(), pos))?;
        let cached = self.cache.borrow().get(&name).map(Shared::clone);
        match cached {
            Some(module) => Ok(module),
            None => self.evaluate(engine, &name, pos),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::registry::Script;

    fn resolver() -> Resolver {
        let names = [
            "lib/collectors.rhai",
            "standard/balance-sheet.rhai",
            "./mine/report.rhai",
            "./mine/util/helpers.rhai",
            "./top.rhai",
        ];
        let sources = names
            .iter()
            .map(|n| {
                (
                    n.to_string(),
                    Script {
                        text: Arc::from(""),
                        builtin: !n.starts_with("./"),
                    },
                )
            })
            .collect();
        Resolver::new(Arc::new(sources), Defined::default())
    }

    #[test]
    fn imports_resolve_only_to_known_scripts() {
        let r = resolver();
        let user = Some("./mine/report.rhai");
        assert_eq!(r.locate(None, "lib/collectors").as_deref(), Some("lib/collectors.rhai"));
        assert_eq!(r.locate(user, "lib/collectors.rhai").as_deref(), Some("lib/collectors.rhai"));
        assert_eq!(
            r.locate(user, "standard/balance-sheet").as_deref(),
            Some("standard/balance-sheet.rhai")
        );
        assert_eq!(r.locate(user, "./util/helpers").as_deref(), Some("./mine/util/helpers.rhai"));
        assert_eq!(r.locate(Some("./top.rhai"), "./mine/report").as_deref(), Some("./mine/report.rhai"));
        let refused = [
            "../top",
            "./../top",
            "./util/../report",
            "/etc/passwd",
            "lib/../top",
            "mine/report",
            "lib/missing",
            "",
            "./",
            "lib",
        ];
        refused.iter().for_each(|p| assert_eq!(r.locate(user, p), None, "{p}"));
        assert_eq!(
            r.locate(Some("lib/collectors.rhai"), "./mine/report"),
            None,
            "no ./ from a built-in script"
        );
        assert_eq!(r.locate(None, "./top"), None);
    }
}
