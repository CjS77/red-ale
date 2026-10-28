//! The built-in report scripts, compiled into red-ale with `include_str!` (`design/reports.md` §1, §12).
//!
//! They live in `crates/red-ale-report/scripts/`, laid out as GnuCash lays out its reports: `standard/` and `example/`
//! for the reports (`standard/balance-sheet.scm` becomes `standard/balance-sheet.rhai`) and `lib/` for the shared
//! helper modules they import (`import "lib/collectors" as collectors;`). Adding a script is one line in
//! [`BUILTIN`].

/// One `include_str!` per built-in script, by its path below `scripts/`.
macro_rules! scripts {
    ($($path:literal),* $(,)?) => {
        &[$(($path, include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/scripts/", $path)))),*]
    };
}

/// Every built-in script: its path below `scripts/` and its text. Paths start with `lib/`, `standard/` or `example/`
/// and end in `.rhai`. Add a line here for each new file.
const BUILTIN: &[(&str, &str)] = scripts![
    "example/average-balance.rhai",
    "lib/accounts.rhai",
    "lib/acct-table.rhai",
    "lib/balances.rhai",
    "lib/budget.rhai",
    "lib/collectors.rhai",
    "lib/dates.rhai",
    "lib/exchange.rhai",
    "lib/html-utilities.rhai",
    "lib/options.rhai",
    "standard/account-piecharts.rhai",
    "standard/account-summary.rhai",
    "standard/balance-sheet.rhai",
    "standard/budget-balance-sheet.rhai",
    "standard/budget-flow.rhai",
    "standard/budget-income-statement.rhai",
    "standard/budget.rhai",
    "standard/category-barchart.rhai",
    "standard/income-statement.rhai",
    "standard/net-charts.rhai",
    "standard/price-scatter.rhai",
];

/// The GUIDs and names of the GnuCash 5.17 templates red-ale translates (`design/reports.md` §11). A built-in
/// template with one of these GUIDs must have its name, and one with one of these names its GUID, so a GUID or name
/// given to `gnucash-cli --report run` names the same report in red-ale.
pub const GNUCASH_TEMPLATES: [(&str, &str); 24] = [
    ("3298541c236b494998b236dfad6ad752", "Account Summary"),
    ("c4173ac99b2b448289bf4d11c731af13", "Balance Sheet"),
    ("0b81a3bdfd504aff849ec2e8630524bc", "Income Statement"),
    ("8758ba23984c40dea5527f5f0ca2779e", "Profit & Loss"),
    ("d5adcc61c62e4b8684dd8907448d7900", "Average Balance"),
    ("cbba1696c8c24744848062c7f1cf4a72", "Net Worth Barchart"),
    ("80769921e87943adade887b9835a7685", "Income/Expense Chart"),
    ("d8b63264186b11e19038001558291366", "Net Worth Linechart"),
    ("e533c998186b11e1b2e2001558291366", "Income & Expense Linechart"),
    ("44f81bee049b4b3ea908f8dac9a9474e", "Income Over Time"),
    ("b1f15b2052c149df93e698fe85a81ea6", "Expense Over Time"),
    ("e9cf815f79db44bcb637d0295093ae3d", "Assets Over Time"),
    ("faf410e8f8da481fbc09e4763da40bcc", "Liabilities Over Time"),
    ("e1bd09b8a1dd49dd85760db9d82b045c", "Income Accounts"),
    ("9bf1892805cb4336be6320fe48ce5446", "Expense Accounts"),
    ("5c7fd8a1fe9a4cd38884ff54214aa88a", "Assets"),
    ("e9418ff64f2c11e5b61d1c7508d793ed", "Securities"),
    ("3fe6dce77da24c66bdc8f8efdea7f9ac", "Liabilities"),
    ("1d241609fd4644caad765c95be20ff4c", "Price Scatterplot"),
    ("810ed4b25ef0486ea43bbd3dddb32b11", "Budget Report"),
    ("ecc35ea9dbfa4e20ba389fc85d59cb69", "Budget Balance Sheet"),
    ("583c313fcc484efc974c4c844404f454", "Budget Income Statement"),
    ("e5fa5ce805e840ecbeca4dba3fa4ead9", "Budget Profit & Loss"),
    ("e6e34fa3b6e748debde3cb3bc76d3e53", "Budget Flow"),
];

/// A set of built-in scripts: the library compiled into red-ale, or, for tests, any scripts laid out the same way.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Library {
    scripts: Vec<(String, String)>,
}

impl Library {
    /// The scripts compiled into red-ale.
    pub fn builtin() -> Self {
        Self::from_sources(BUILTIN.iter().copied())
    }

    /// A library of the given `(path, text)` scripts, such as `("lib/dates.rhai", "…")`. Paths outside `lib/`,
    /// `standard/` and `example/`, or not ending in `.rhai`, are left out.
    pub fn from_sources<'a>(scripts: impl IntoIterator<Item = (&'a str, &'a str)>) -> Self {
        let valid = |path: &str| {
            let dir = path.split('/').next().unwrap_or("");
            crate::engine::BUILTIN_DIRS.contains(&dir)
                && std::path::Path::new(path).extension().is_some_and(|e| e == "rhai")
                && path.split('/').all(|c| !c.is_empty() && c != "." && c != "..")
        };
        Self {
            scripts: scripts
                .into_iter()
                .filter(|(p, _)| valid(p))
                .map(|(p, t)| (p.to_owned(), t.to_owned()))
                .collect(),
        }
    }

    /// The scripts, by path.
    pub fn scripts(&self) -> impl Iterator<Item = (&str, &str)> {
        self.scripts.iter().map(|(p, t)| (p.as_str(), t.as_str()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::Registry;
    use crate::limits::Limits;

    /// A built-in template with a GnuCash GUID has GnuCash's name for it, and one with a GnuCash name its GUID
    /// (`design/reports.md` §11).
    #[test]
    fn built_in_templates_keep_gnucash_names_and_guids() {
        let registry = Registry::load(&Library::builtin(), &[], Limits::default()).unwrap();
        registry.templates().iter().for_each(|t| {
            let guid = t.guid.0.to_string();
            let by_guid = GNUCASH_TEMPLATES.iter().find(|(g, _)| *g == guid);
            let by_name = GNUCASH_TEMPLATES.iter().find(|(_, n)| *n == t.name);
            assert_eq!(by_guid, by_name, "{} {guid}", t.name);
        });
        let (guids, names) = (GNUCASH_TEMPLATES.map(|(g, _)| g), GNUCASH_TEMPLATES.map(|(_, n)| n));
        let unique = |i: usize| (0..GNUCASH_TEMPLATES.len()).all(|j| j == i || (guids[j] != guids[i] && names[j] != names[i]));
        assert!((0..GNUCASH_TEMPLATES.len()).all(unique));
    }

    #[test]
    fn library_paths_are_gnucash_directories() {
        let paths = ["lib/a.rhai", "standard/b.rhai", "other/c.rhai", "lib/../d.rhai", "lib/e.txt"];
        let lib = Library::from_sources(paths.map(|p| (p, "")));
        assert_eq!(lib.scripts().map(|(p, _)| p).collect::<Vec<_>>(), ["lib/a.rhai", "standard/b.rhai"]);
    }
}
