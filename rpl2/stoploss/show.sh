#!/usr/bin/env bash
# The oracle's price, every cell (resting orders show their escrow and a commitment, nothing
# more), and the vault that holds the escrows: ./show.sh [secret file]
# With a secret file: that order, and whether it would fire at the current price.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
. "$here/../../scripts/env.sh"
id="$(load_id "$here")"
oracle="$(cell "$id" "$(word8_hex 1 0 0 0 0 0 0 0)")"
if [ -n "${1:-}" ]; then
    key="$(plan "$here" key --secret "$1")"
    plan "$here" status --oracle "$oracle" --secret "$1" --order "$(cell "$id" "$key")"
else
    plan "$here" status --oracle "$oracle"
fi
"$RAND" program state "$id"
"$RAND" program vault "$id"
