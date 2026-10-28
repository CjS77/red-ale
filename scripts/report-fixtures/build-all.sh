#!/bin/sh
# Rebuild every report fixture book with the GnuCash 5.17 oracle and check its balances.
#
# Usage: scripts/report-fixtures/build-all.sh [out-dir]    (default fixtures/reports)
#
# Set RED_ALE_ORACLE_PYTHONPATH to the 5.17 bindings' site-packages if they are not in
# $HOME/.local/opt/gnucash-5.17/lib/python3.12/site-packages.
set -eu
here=$(dirname "$0")
out=${1:-$here/../../fixtures/reports}
PYTHONPATH=${RED_ALE_ORACLE_PYTHONPATH:-$HOME/.local/opt/gnucash-5.17/lib/python3.12/site-packages}
TZ=UTC
export PYTHONPATH TZ
for script in account_summary balsheet_pnl average_balance charts net_barchart category_report budget \
        adjusting_entries cash_flow balsheet_pnl_extras; do
    python3 "$here/$script.py" "$out"
done
