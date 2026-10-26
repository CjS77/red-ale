#!/bin/sh
# Runs the parsers of GnuCash 5.17's qif-parse.scm in plain Guile 3 on the inputs of driver.scm and prints their
# results, one tab-separated line per input. The output is
# crates/red-ale-import/tests/data/qif-parse-guile.tsv, which tests/qif_parse_guile.rs compares red-ale's
# translation with.
#
#   GNUCASH_SOURCE=/path/to/gnucash scripts/qif-guile-oracle/run.sh > crates/red-ale-import/tests/data/qif-parse-guile.tsv
#
# prelude.scm stands in for the GnuCash modules qif-parse.scm loads: gnc:warn, G_, the account type constants and
# the two (gnucash string) helpers it calls, copied from string.scm. Lines 56-471 of qif-parse.scm are the parsers,
# without the module header and the print functions, which need GnuCash's engine.
set -eu
: "${GNUCASH_SOURCE:?set GNUCASH_SOURCE to a GnuCash checkout}"
here=$(dirname "$0")
script=$(mktemp)
trap 'rm -f "$script"' EXIT
{
    cat "$here/prelude.scm"
    git -C "$GNUCASH_SOURCE" show 5.17:gnucash/import-export/qif-imp/qif-parse.scm | sed -n 56,471p
    cat "$here/driver.scm"
} > "$script"
LANG=C.UTF-8 guile --no-auto-compile "$script"
