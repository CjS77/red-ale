#!/usr/bin/env python3
"""Generate crates/red-ale-locale/src/table.rs from glibc's locale sources.

Usage: scripts/gen-locales.py [/usr/share/i18n/locales] > crates/red-ale-locale/src/table.rs

Reads LC_NUMERIC, LC_MONETARY and LC_TIME of every supported locale, following `copy` directives and resolving
`<Uxxxx>` and escape-character sequences, and writes the values `localeconv`, `nl_langinfo (D_FMT)` and `strftime`
give under that locale, as glibc's `localedef` compiles them. The C locale is not read from a file: glibc builds it
in, and its values are written here as `localeconv` reports them.

Every file the table reads, including those reached through `copy`, must carry glibc's notice that the Free Software
Foundation claims no copyright in the locale data; the script stops if one does not.
"""
import os
import re
import sys
import textwrap
import unicodedata

# The locales red-ale supports: for each language GnuCash 5.17 ships a translation for (`po/*.po`), its main
# territories in glibc, plus common English, French, German, Spanish, Italian and Dutch variants. `@` variants are
# left out.
SUPPORTED = [
    "ar_EG", "ar_SA", "as_IN", "az_AZ", "bg_BG", "brx_IN", "ca_ES", "cs_CZ", "da_DK", "de_AT", "de_CH", "de_DE",
    "doi_IN", "el_GR", "en_AU", "en_CA", "en_GB", "en_IE", "en_IN", "en_NZ", "en_US", "en_ZA", "es_AR", "es_ES",
    "es_MX", "es_NI", "et_EE", "eu_ES", "fa_IR", "fi_FI", "fr_BE", "fr_CA", "fr_CH", "fr_FR", "gu_IN", "he_IL",
    "hi_IN", "hr_HR", "hu_HU", "id_ID", "it_CH", "it_IT", "ja_JP", "kn_IN", "ko_KR", "kok_IN", "ks_IN", "lt_LT",
    "lv_LV", "mai_IN", "mk_MK", "mni_IN", "mr_IN", "nb_NO", "ne_NP", "nl_BE", "nl_NL", "pl_PL", "pt_BR", "pt_PT",
    "ro_RO", "ru_RU", "rw_RW", "sk_SK", "sr_RS", "sv_SE", "ta_IN", "te_IN", "tr_TR", "uk_UA", "ur_PK", "vi_VN",
    "zh_CN", "zh_TW",
]

CATEGORIES = ("LC_NUMERIC", "LC_MONETARY", "LC_TIME")

NOTICE = "The Free Software Foundation does not claim any copyright interest in the locale data contained in this file."

CHAR_MAX = 127

# glibc's built-in C locale, as `localeconv`, `nl_langinfo` and `strftime` report it.
C_LOCALE = {
    "LC_NUMERIC": {"decimal_point": ["."], "thousands_sep": [""], "grouping": [-1]},
    "LC_MONETARY": {
        **{k: [""] for k in ("int_curr_symbol", "currency_symbol", "mon_decimal_point", "mon_thousands_sep",
                             "positive_sign", "negative_sign")},
        "mon_grouping": [-1],
        **{k: [-1] for k in ("int_frac_digits", "frac_digits", "p_cs_precedes", "p_sep_by_space", "n_cs_precedes",
                             "n_sep_by_space", "p_sign_posn", "n_sign_posn")},
    },
    "LC_TIME": {
        "d_fmt": ["%m/%d/%y"],
        "day": ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"],
        "abday": ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"],
        "mon": ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October",
                "November", "December"],
        "abmon": ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"],
    },
}


class LocaleFile:
    """One glibc locale source file: its categories as keyword -> list of values, `copy` unresolved."""

    def __init__(self, path):
        self.path = path
        with open(path, encoding="utf-8") as f:
            text = f.read()
        self.comment = "#"
        self.escape = "\\"
        lines = text.split("\n")
        for line in lines[:10]:
            m = re.match(r"^\s*(comment_char|escape_char)\s+(\S)\s*$", line)
            if m:
                setattr(self, "comment" if m.group(1) == "comment_char" else "escape", m.group(2))
        self.has_notice = NOTICE in " ".join(
            l.strip().lstrip(self.comment).strip() for l in lines if l.strip().startswith(self.comment))
        self.categories = self.parse(self.logical_lines(lines))

    def logical_lines(self, lines):
        out, pending, quoted = [], "", False
        for raw in lines:
            line, quoted = self.strip_comment(raw, quoted)
            line = line.rstrip()
            # A line ending in the escape character continues, even after a comment (`"нд"; %nd /` in uk_UA).
            end = raw.rstrip()
            if end.endswith(self.escape) and not end.endswith(self.escape * 2):
                pending += line[:-1] if line.endswith(self.escape) else line
                continue
            out.append(pending + line)
            pending, quoted = "", False
        return out

    def strip_comment(self, line, quoted):
        """The line up to a comment character outside a string, and whether it ends inside one. glibc's files put
        comments after values too, and continue strings over lines."""
        i = 0
        while i < len(line):
            c = line[i]
            if c == self.escape:
                i += 2
                continue
            if c == '"':
                quoted = not quoted
            elif c == self.comment and not quoted:
                return line[:i], quoted
            i += 1
        return line, quoted

    def parse(self, lines):
        cats, current = {}, None
        for line in lines:
            words = line.split(None, 1)
            if not words:
                continue
            if current is None:
                if words[0] in CATEGORIES:
                    current = words[0]
                    cats[current] = []
                continue
            if words[0] == "END":
                current = None
                continue
            cats[current].append((words[0], self.values(words[1] if len(words) > 1 else "")))
        return cats

    def values(self, text):
        """The `;`-separated strings and numbers of one keyword."""
        values, i = [], 0
        while i < len(text):
            c = text[i]
            if c == self.comment:
                break
            if c in " \t;":
                i += 1
            elif c == '"':
                s, i = self.string(text, i + 1)
                values.append(s)
            elif c == "<":
                s, i = self.symbol(text, i)
                values.append(s)
            else:
                m = re.match(r"-?\d+", text[i:])
                if not m:
                    raise ValueError(f"{self.path}: cannot read {text!r}")
                values.append(int(m.group(0)))
                i += len(m.group(0))
        return values

    def string(self, text, i):
        out = []
        while True:
            if i >= len(text):
                raise ValueError(f"{self.path}: unterminated string in {text!r}")
            c = text[i]
            if c == '"':
                return "".join(out), i + 1
            if c == self.escape:
                out.append(text[i + 1])
                i += 2
            elif c == "<":
                s, i = self.symbol(text, i)
                out.append(s)
            else:
                out.append(c)
                i += 1

    def symbol(self, text, i):
        m = re.match(r"<U([0-9A-Fa-f]{4,8})>", text[i:])
        if not m:
            raise ValueError(f"{self.path}: unknown symbol in {text[i:]!r}")
        return chr(int(m.group(1), 16)), i + len(m.group(0))


class Sources:
    def __init__(self, directory):
        self.directory = directory
        self.files = {}
        self.read = set()

    def file(self, name):
        if name not in self.files:
            self.files[name] = LocaleFile(os.path.join(self.directory, name))
        f = self.files[name]
        if not f.has_notice:
            raise SystemExit(f"{f.path} does not carry the FSF's no-copyright notice; red-ale cannot use it")
        self.read.add(name)
        return f

    def category(self, name, category):
        """A category's keywords, with `copy` followed and later keywords overriding copied ones."""
        entries = self.file(name).categories.get(category)
        if entries is None:
            raise SystemExit(f"{name} has no {category}")
        out = {}
        for key, values in entries:
            if key == "copy":
                out.update(self.category(values[0], category))
            else:
                out[key] = values
        return out


def grouping(values):
    """The grouping string `localedef` compiles: a lone -1 is no grouping (empty), -1 elsewhere is CHAR_MAX."""
    if values == [-1]:
        return []
    out = []
    for v in values:
        if v == 0:
            break
        out.append(CHAR_MAX if v == -1 else v)
    return out


def char(values):
    """A `char` field of `struct lconv`: -1 is CHAR_MAX, which red-ale keeps as `None`."""
    v = values[0]
    return None if v == -1 else v


def rust_str(s):
    """A Rust string literal of exactly these characters. A string glibc does not hold in NFC is written with escapes,
    so nothing normalises it on the way."""
    nfc = unicodedata.is_normalized("NFC", s)
    keep = lambda c: c not in '\\"' and c.isprintable() and (nfc or c.isascii())
    return '"' + "".join(c if keep(c) else f"\\u{{{ord(c):x}}}" for c in s) + '"'


def rust_list(items, fmt=rust_str, indent=12):
    one_line = "&[" + ", ".join(fmt(i) for i in items) + "]"
    if len(one_line) + indent + 8 <= 120:
        return one_line
    pad = " " * (indent + 4)
    return "&[\n" + "".join(f"{pad}{fmt(i)},\n" for i in items) + " " * indent + "]"


def rust_char(v):
    return "None" if v is None else f"Some({v})"


def entry(name, cats):
    num, mon, time = cats["LC_NUMERIC"], cats["LC_MONETARY"], cats["LC_TIME"]
    s = lambda d, k: rust_str(d[k][0])
    d_fmt = time["d_fmt"][0]
    directives = re.findall(r"%([-_0^#]*[EO]?)([A-Za-z%])", d_fmt)
    unsupported = {f + d for f, d in directives if d not in "deymYbBaA%" or f.strip("-EO")}
    if unsupported:
        raise SystemExit(f"{name}: d_fmt {d_fmt!r} uses {sorted(unsupported)}, which red-ale does not print")
    # GnuCash looks for the locale's own currency symbol only when a mnemonic equals `int_curr_symbol` as it is
    # (`5.17:libgnucash/engine/gnc-commodity.cpp:1105-1108`); glibc's always ends in a space, so it never does.
    int_curr = mon["int_curr_symbol"][0]
    if int_curr and not int_curr.endswith(" "):
        raise SystemExit(f"{name}: int_curr_symbol {int_curr!r} has no trailing space")
    for key, n in (("day", 7), ("abday", 7), ("mon", 12), ("abmon", 12)):
        if len(time[key]) != n:
            raise SystemExit(f"{name}: {key} has {len(time[key])} names")
    # ICU maps the POSIX locale C to en_US_POSIX.
    icu = "en-US-posix" if name == "C" else name.replace("_", "-")
    return "\n".join([
        "    LocaleData {",
        f"        name: {rust_str(name)},",
        f"        icu: {rust_str(icu)},",
        "        numeric: Numeric {",
        f"            decimal_point: {s(num, 'decimal_point')},",
        f"            thousands_sep: {s(num, 'thousands_sep')},",
        f"            grouping: {rust_list(grouping(num['grouping']), str)},",
        "        },",
        "        monetary: Monetary {",
        *[f"            {k}: {s(mon, k)}," for k in ("int_curr_symbol", "currency_symbol", "mon_decimal_point",
                                                    "mon_thousands_sep")],
        f"            mon_grouping: {rust_list(grouping(mon['mon_grouping']), str)},",
        *[f"            {k}: {s(mon, k)}," for k in ("positive_sign", "negative_sign")],
        *[f"            {k}: {rust_char(char(mon[k]))}," for k in ("int_frac_digits", "frac_digits", "p_cs_precedes",
                                                                   "p_sep_by_space", "n_cs_precedes", "n_sep_by_space",
                                                                   "p_sign_posn", "n_sign_posn")],
        "        },",
        "        time: Time {",
        f"            d_fmt: {rust_str(d_fmt)},",
        f"            day: {rust_list(time['day']).replace('&', '', 1)},",
        f"            abday: {rust_list(time['abday']).replace('&', '', 1)},",
        f"            mon: {rust_list(time['mon']).replace('&', '', 1)},",
        f"            abmon: {rust_list(time['abmon']).replace('&', '', 1)},",
        "        },",
        "    },",
    ])


def main():
    directory = sys.argv[1] if len(sys.argv) > 1 else "/usr/share/i18n/locales"
    sources = Sources(directory)
    locales = {"C": C_LOCALE}
    locales.update({name: {c: sources.category(name, c) for c in CATEGORIES} for name in SUPPORTED})
    names = sorted(locales)
    out = [
        "//! The Locale table: glibc's LC_NUMERIC, LC_MONETARY and LC_TIME values for each supported locale.",
        "//!",
        "//! Generated by `scripts/gen-locales.py` from glibc's locale sources (`/usr/share/i18n/locales`). Do not edit",
        "//! by hand. Every source file read carries glibc's notice: \"The Free Software Foundation does not claim any",
        "//! copyright interest in the locale data contained in this file.\" Files read:",
        *textwrap.wrap(", ".join(sorted(sources.read)) + ".", 116, initial_indent="//! ", subsequent_indent="//! "),
        "",
        "use crate::data::{LocaleData, Monetary, Numeric, Time};",
        "",
        "/// Every supported locale, sorted by name.",
        f"pub(crate) static LOCALES: [LocaleData; {len(names)}] = [",
        *[entry(n, locales[n]) for n in names],
        "];",
    ]
    print("\n".join(out))


main()
