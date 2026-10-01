#!/usr/bin/env bash
# Mint more of a token created by ./create-mintable.sh, signed by authority.key.json.
#   ./mint.sh <token index or rpl1… id> <amount, display units> [rand1… recipient]
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
. "$here/../../scripts/env.sh"
need_rand
asset="${1:?usage: ./mint.sh <token> <amount> [recipient]}"; amount="${2:?}"
to="${3:-$("$RAND" address | tail -1)}"
"$RAND" token mint --asset "$asset" --amount "$amount" --to "$to" --authority-key "$here/authority.key.json"
