#!/usr/bin/env bash
# The pool's cell and the vault that holds its reserves.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
. "$here/../../scripts/env.sh"
id="$(load_id "$here")"
"$RAND" program state "$id"
"$RAND" program vault "$id"
