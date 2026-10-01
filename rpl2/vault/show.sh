#!/usr/bin/env bash
# What the vault holds.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
. "$here/../../scripts/env.sh"
"$RAND" program vault "$(load_id "$here")"
