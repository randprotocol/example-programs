#!/usr/bin/env bash
# A token's public row (name, symbol, decimals, authority, supply, id), or every token, and what
# this wallet holds.
#   ./info.sh [token index or rpl1… id]
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
. "$here/../../scripts/env.sh"
if [ -n "${1:-}" ]; then "$RAND" token info "$1"; else "$RAND" token list; fi
[ -f "$RAND_KEY" ] && "$RAND" asset-balance ${1:+"$1"} || true
