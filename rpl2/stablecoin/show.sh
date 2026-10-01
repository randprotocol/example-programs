#!/usr/bin/env bash
# Every cell (the config, key 01000000…; each position, key 02…), the vault that holds the
# collateral, and your own position's key if you have one.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
. "$here/../../scripts/env.sh"
id="$(load_id "$here")"
"$RAND" program state "$id"
"$RAND" program vault "$id"
if [ -f "$here/position.secret" ]; then
    echo "your position: $(plan "$here" key --secret "$here/position.secret")"
fi
