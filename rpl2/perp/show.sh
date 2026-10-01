#!/usr/bin/env bash
# The market, the open interest and every position (all cells), and the vault behind them.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
. "$here/../../scripts/env.sh"
id="$(load_id "$here")"
"$RAND" program state "$id"
"$RAND" program vault "$id"
