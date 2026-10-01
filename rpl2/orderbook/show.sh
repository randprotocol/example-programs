#!/usr/bin/env bash
# Every order (each cell: key, then [give asset, want asset, give_rem, want_rem, proceeds] as
# little-endian words) and the vault that holds the escrow and the uncollected proceeds.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
. "$here/../../scripts/env.sh"
id="$(load_id "$here")"
"$RAND" program state "$id"
"$RAND" program vault "$id"
