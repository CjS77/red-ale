//! The Locale: a supported locale by name, or the locales a process environment selects, one per category as the C
//! library takes them.

use std::fmt;

use crate::conventions::Conventions;
use crate::data::{LocaleData, Monetary, Numeric, Time};
use crate::date::{self, CivilDate, Weekday};
use crate::icu;
use crate::table::LOCALES;

/// A locale name red-ale does not support.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LocaleError {
    /// The name is not a supported locale.
    #[error("{0:?} is not a supported locale")]
    Unsupported(String),
    /// An environment variable names a locale red-ale does not support.
    #[error("{variable}={name:?} is not a supported locale")]
    UnsupportedInEnvironment {
        /// The variable, `LANG`.
        variable: &'static str,
        /// Its value.
        name: String,
    },
}

/// The parts of a locale that are taken separately: each C library category GnuCash reads, and the locale ICU uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Category {
    /// LC_NUMERIC.
    Numeric,
    /// LC_MONETARY: GnuCash's amounts and the default currency.
    Monetary,
    /// LC_TIME: the `locale` date format and day names.
    Time,
    /// LC_MESSAGES, the category ICU takes its default locale from: the first day of the week and date intervals.
    Messages,
}

impl Category {
    /// Every category.
    pub const ALL: [Category; 4] = [Self::Numeric, Self::Monetary, Self::Time, Self::Messages];

    /// The environment variable that sets this category.
    pub fn variable(self) -> &'static str {
        match self {
            Self::Numeric => "LC_NUMERIC",
            Self::Monetary => "LC_MONETARY",
            Self::Time => "LC_TIME",
            Self::Messages => "LC_MESSAGES",
        }
    }

    fn index(self) -> usize {
        self as usize
    }
}

/// A Locale: the conventions red-ale formats with. Usually one supported locale for every category; from an
/// environment, possibly a different one per category.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Locale {
    parts: [&'static LocaleData; 4],
}

impl fmt::Debug for Locale {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Locale({})", self.name())
    }
}

impl Default for Locale {
    fn default() -> Self {
        Self::c()
    }
}

/// The C locale's entry, which the table always has.
fn c_data() -> &'static LocaleData {
    LOCALES.iter().find(|l| l.name == "C").unwrap_or(&LOCALES[0])
}

/// The table entry for `name`: a glibc name with an optional `.UTF-8` or `.utf8` codeset, or `C`, `POSIX`, `C.UTF-8`.
fn lookup(name: &str) -> Option<&'static LocaleData> {
    let (base, codeset) = name.split_once('.').unwrap_or((name, "utf8"));
    if !codeset.replace('-', "").eq_ignore_ascii_case("utf8") {
        return None;
    }
    let base = if base == "POSIX" { "C" } else { base };
    LOCALES.iter().find(|l| l.name == base)
}

impl Locale {
    /// The C locale: what red-ale printed before locales, and the default.
    pub fn c() -> Self {
        Self { parts: [c_data(); 4] }
    }

    /// A supported locale by name: `fr_FR`, `fr_FR.UTF-8` or `fr_FR.utf8`; `C` and `POSIX` are the C locale.
    pub fn by_name(name: &str) -> Result<Self, LocaleError> {
        lookup(name).map(|l| Self { parts: [l; 4] }).ok_or_else(|| LocaleError::Unsupported(name.to_owned()))
    }

    /// Every supported locale's name, sorted bytewise, `C` among them.
    pub fn supported() -> impl Iterator<Item = &'static str> {
        LOCALES.iter().map(|l| l.name)
    }

    /// The locales a process environment selects, as `setlocale (LC_ALL, "")` reads it: for each category,
    /// `LC_ALL`, else the category's own variable, else `LANG`, else C. Empty values count as unset. `var` returns a
    /// variable's value; this crate reads no environment of its own.
    pub fn from_environment(var: impl Fn(&str) -> Option<String>) -> Result<Self, LocaleError> {
        let set = |name: &'static str| var(name).filter(|v| !v.is_empty()).map(|v| (name, v));
        let mut parts = [c_data(); 4];
        Category::ALL.iter().try_for_each(|c| {
            if let Some((variable, name)) = set("LC_ALL").or_else(|| set(c.variable())).or_else(|| set("LANG")) {
                parts[c.index()] = lookup(&name).ok_or(LocaleError::UnsupportedInEnvironment { variable, name })?;
            }
            Ok(())
        })?;
        Ok(Self { parts })
    }

    /// The locale's name; for an environment that mixes locales, glibc's composite form
    /// `LC_NUMERIC=…;LC_MONETARY=…;LC_TIME=…;LC_MESSAGES=…`.
    pub fn name(&self) -> String {
        if self.parts.iter().all(|p| p.name == self.parts[0].name) {
            return self.parts[0].name.to_owned();
        }
        Category::ALL.iter().map(|c| format!("{}={}", c.variable(), self.category(*c).name)).collect::<Vec<_>>().join(";")
    }

    /// The locale data of one category.
    pub fn category(&self, category: Category) -> &'static LocaleData {
        self.parts[category.index()]
    }

    /// True when `category` is the C locale.
    pub fn is_c(&self, category: Category) -> bool {
        self.category(category).name == "C"
    }

    /// LC_NUMERIC as glibc gives it.
    pub fn numeric(&self) -> &'static Numeric {
        &self.category(Category::Numeric).numeric
    }

    /// LC_MONETARY as glibc gives it.
    pub fn monetary(&self) -> &'static Monetary {
        &self.category(Category::Monetary).monetary
    }

    /// LC_TIME as glibc gives it.
    pub fn time(&self) -> &'static Time {
        &self.category(Category::Time).time
    }

    /// GnuCash's view of the number conventions: `gnc_localeconv`, glibc's values with GnuCash's defaults.
    pub fn conventions(&self) -> Conventions {
        Conventions::new(self.numeric(), self.monetary())
    }

    /// `date` under the locale's `D_FMT`: GnuCash's `locale` date format (`qof_date_format_get_string`,
    /// `5.17:libgnucash/engine/gnc-date.cpp:504-521`).
    pub fn print_date(&self, date: CivilDate) -> String {
        self.format_date(self.time().d_fmt, date)
    }

    /// `date` under a `strftime` format with the locale's names, as GnuCash formats dates.
    pub fn format_date(&self, format: &str, date: CivilDate) -> String {
        date::format(self.time(), format, date)
    }

    /// The full name of a day, `%A`: `lundi` under fr_FR.
    pub fn day_name(&self, weekday: Weekday) -> &'static str {
        self.time().day[weekday.days_from_sunday()]
    }

    /// The abbreviated name of a day, `%a`.
    pub fn abbreviated_day_name(&self, weekday: Weekday) -> &'static str {
        self.time().abday[weekday.days_from_sunday()]
    }

    /// The first day of the week, as GnuCash asks ICU for it (`gnc_start_of_week`,
    /// `5.17:libgnucash/engine/gnc-date.cpp:195-215`). Sunday under C, which ICU reads as en_US_POSIX.
    pub fn first_weekday(&self) -> Weekday {
        icu::first_weekday(self.category(Category::Messages).icu)
    }

    /// ICU's interval form of a date range, `1/1/2026 – 12/31/2026` under en_US
    /// (`5.17:libgnucash/engine/gnc-date.cpp:618-659`); `None` when ICU has no answer.
    pub fn date_interval(&self, from: CivilDate, to: CivilDate) -> Option<String> {
        icu::date_interval(self.category(Category::Messages).icu, from, to)
    }
}

/// The full name of `weekday` under `locale`'s LC_TIME, as `strftime`'s `%A` prints it.
pub fn day_name(locale: &Locale, weekday: Weekday) -> &'static str {
    locale.day_name(weekday)
}
