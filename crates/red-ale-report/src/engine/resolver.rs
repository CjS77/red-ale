//! red-ale's module resolver (`design/reports.md` §2, "The module resolver"). It replaces Rhai's file resolver, which
//! would let `import` read any path, and knows only the scripts handed to the engine:
//!
//! - `import "lib/<name>"`, `"standard/<name>"`, `"example/<name>"`: a built-in script;
//! - `import "./<path>"`: a user script, relative to the importing one, from a user script only.
//!
//! Anything else (an absolute path, `..`, a `./` path from a built-in script, a name no script has) is "module not
//! found". Every script is text in memory; this crate reads no file.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use rhai::{Engine, EvalAltResult, Module, ModuleResolver, Position, Scope, Shared};

use super::registry::{BUILTIN_DIRS, Sources, USER_PREFIX};

/// The resolver of one run, caching each module it evaluates.
pub(crate) struct Resolver {
    sources: Arc<Sources>,
    cache: RefCell<BTreeMap<String, Shared<Module>>>,
    loading: RefCell<BTreeSet<String>>,
}

impl Resolver {
    pub(crate) fn new(sources: Arc<Sources>) -> Self {
        Self {
            sources,
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
        Resolver::new(Arc::new(sources))
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
