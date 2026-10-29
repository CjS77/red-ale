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
//! Rhai could call either in place of the other (`design/reports.md` §2, "One name, one function").

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;
use std::sync::Arc;

use rhai::{AST, Engine, EvalAltResult, Module, ModuleResolver, Position, Scope, Shared};

use super::registry::{BUILTIN_DIRS, Sources, USER_PREFIX};

/// A function's name and number of parameters, which is all a Rhai call resolves a script function by.
type Signature = (String, usize);

/// The functions of one script, private ones included.
type Functions = BTreeSet<Signature>;

/// The functions every template script has, which the engine calls through the script's own AST and another script
/// reaches only qualified (`transaction_report::render`), so two template scripts may both define them.
const ENTRY_POINTS: [(&str, usize); 3] = [("templates", 0), ("options", 3), ("render", 4)];

/// The functions each script of a run defines: the script run, then each module as it is imported. Rhai keeps one
/// function resolution cache for a whole run, keyed by name and arity alone, so a function one script resolves can
/// be called in another's place; no two scripts of a run may define the same name and arity.
#[derive(Debug, Clone, Default)]
pub(crate) struct Defined(Rc<RefCell<Vec<(String, Functions)>>>);

impl Defined {
    /// Add the functions of the script the run compiles first, before any import, so with nothing to check.
    pub(crate) fn start(&self, source: &str, ast: &AST) {
        self.0.borrow_mut().push((source.to_owned(), signatures(ast)));
    }

    /// Add the functions of `source`, compiled to `ast`. The error names a function another script of the run
    /// already defines, and that script. A script imported by itself is the same code, so it is not compared with
    /// itself; the resolver refuses that import anyway.
    pub(crate) fn add(&self, source: &str, ast: &AST) -> Result<(), String> {
        let mine = signatures(ast);
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
            return Err(format!(
                "{source} and {other} both define fn {name} with {arity} parameters; Rhai may call either in the \
                 other's place, so one must be renamed"
            ));
        }
        self.0.borrow_mut().push((source.to_owned(), mine));
        Ok(())
    }
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
        // Located at the import, in the importing script, rather than inside the module.
        self.defined
            .add(name, &ast)
            .map_err(|m| EvalAltResult::ErrorRuntime(m.into(), pos))?;
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
