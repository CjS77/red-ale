//! The limits a report run stays within (`design/reports.md` §2, "Limits and their errors"). Passing one stops the run
//! with `report-limit`, naming the limit.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// One limit. Its name is the `data.limit` of a `report-limit` error and its key under `reports.limits` in the
/// Configuration, except [`Limit::Time`], whose key there is `seconds`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Limit {
    /// Rhai operations (`set_max_operations`).
    Operations,
    /// Run time in seconds, checked from Rhai's `on_progress` against the host clock.
    Time,
    /// Function call depth (`set_max_call_levels`).
    CallDepth,
    /// Expression nesting at the top level (`set_max_expr_depths`); half of it inside functions.
    ExpressionDepth,
    /// String length in bytes (`set_max_string_size`).
    StringSize,
    /// Array length (`set_max_array_size`).
    ArraySize,
    /// Map size (`set_max_map_size`).
    MapSize,
    /// Modules loaded (`set_max_modules`).
    Modules,
    /// Variables in scope (`set_max_variables`).
    Variables,
    /// Functions one script defines (`set_max_functions`), checked when the script compiles.
    Functions,
    /// Bits in a Rational's numerator or denominator: red-ale's own check, [`crate::number::SizeCap`].
    NumberSize,
}

const LIMITS: [(Limit, &str, u64); 11] = [
    (Limit::Operations, "operations", 1_000_000_000),
    (Limit::Time, "time", 120),
    (Limit::CallDepth, "call-depth", 64),
    (Limit::ExpressionDepth, "expression-depth", 128),
    (Limit::StringSize, "string-size", 1 << 20),
    (Limit::ArraySize, "array-size", 10_000_000),
    (Limit::MapSize, "map-size", 1_000_000),
    (Limit::Modules, "modules", 64),
    (Limit::Variables, "variables", 10_000),
    (Limit::Functions, "functions", 1_000),
    (Limit::NumberSize, "number-size", crate::number::DEFAULT_MAX_BITS),
];

impl Limit {
    /// Every limit, in the design's order.
    pub fn all() -> impl Iterator<Item = Self> {
        LIMITS.iter().map(|(l, _, _)| *l)
    }

    fn row(self) -> (&'static str, u64) {
        LIMITS
            .iter()
            .find(|(l, _, _)| *l == self)
            .map_or(("operations", 0), |(_, n, d)| (*n, *d))
    }

    /// The name in `data.limit`.
    pub fn as_str(self) -> &'static str {
        self.row().0
    }

    /// The default maximum.
    pub fn default_max(self) -> u64 {
        self.row().1
    }
}

impl fmt::Display for Limit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Limit {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        LIMITS
            .iter()
            .find(|(_, n, _)| *n == s)
            .map(|(l, _, _)| *l)
            .ok_or_else(|| format!("unknown limit {s:?}"))
    }
}

/// The maximum of every limit for one run: the defaults, changed by the Configuration's `reports.limits`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Limits {
    max: [u64; LIMITS.len()],
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max: LIMITS.map(|(_, _, d)| d),
        }
    }
}

impl Limits {
    fn index(limit: Limit) -> usize {
        LIMITS.iter().position(|(l, _, _)| *l == limit).unwrap_or(0)
    }

    /// The maximum of `limit`.
    pub fn get(&self, limit: Limit) -> u64 {
        self.max[Self::index(limit)]
    }

    /// These limits with `limit` set to `max`.
    #[must_use]
    pub fn with(mut self, limit: Limit, max: u64) -> Self {
        self.max[Self::index(limit)] = max;
        self
    }

    /// The maximum of `limit` as a `usize`, saturating.
    pub(crate) fn get_usize(&self, limit: Limit) -> usize {
        usize::try_from(self.get(limit)).unwrap_or(usize::MAX)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limits_start_at_the_defaults() {
        let limits = Limits::default().with(Limit::Operations, 5);
        assert_eq!(limits.get(Limit::Operations), 5);
        assert_eq!(limits.get(Limit::Functions), 1_000);
        Limit::all()
            .filter(|l| *l != Limit::Operations)
            .for_each(|l| assert_eq!(limits.get(l), l.default_max()));
    }

    #[test]
    fn names_and_defaults() {
        assert_eq!(Limit::all().count(), 11);
        Limit::all().for_each(|l| assert_eq!(l.as_str().parse::<Limit>(), Ok(l)));
        Limit::all().for_each(|l| assert_eq!(serde_json::to_value(l).unwrap(), serde_json::json!(l.as_str())));
        assert_eq!(Limit::NumberSize.default_max(), 4096);
        assert_eq!(Limit::StringSize.default_max(), 1_048_576);
    }
}
