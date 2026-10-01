#!/usr/bin/env bash
# Destroy some of a token this wallet holds; its public supply drops by exactly that much.
#   ./burn.sh <token index or rpl1… id> <amount, display units>
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
. "$here/../../scripts/env.sh"
need_rand
"$RAND" token burn "${1:?usage: ./burn.sh <token> <amount>}" "${2:?}"
