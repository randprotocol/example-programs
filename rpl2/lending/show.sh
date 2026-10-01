#!/usr/bin/env bash
# Every cell (price [1…], pool [2…], positions [3, …], shares [4…]) and the vault holding RAND and C.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
. "$here/../../scripts/env.sh"
id="$(load_id "$here")"
"$RAND" program state "$id"
"$RAND" program vault "$id"
