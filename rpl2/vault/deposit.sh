#!/usr/bin/env bash
# Pay RAND into the vault: ./deposit.sh <RAND>
# The bundle burns it into the program's vault; the bundle names nobody, so who paid in is private.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
. "$here/../../scripts/env.sh"
need_rand
id="$(load_id "$here")"
units="$(rand_units "${1:?usage: ./deposit.sh <RAND>}")"
t="$(mktemp)"; trap 'rm -f "$t"' EXIT
echo "{ \"deposit\": { \"rand\": \"$units\" } }" > "$t"
"$RAND" program invoke "$id" --transition "$t" --input 1
