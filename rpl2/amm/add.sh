#!/usr/bin/env bash
# Add liquidity: ./add.sh <RAND> <token units> [share asset — the first deposit only]
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
. "$here/../../scripts/env.sh"
need_rand
id="$(load_id "$here")"
r="$(rand_units "${1:?usage: ./add.sh <RAND> <token units> [share asset]}")"
amount="${2:?usage: ./add.sh <RAND> <token units> [share asset]}"
t="$(mktemp)"; i="$(mktemp)"; trap 'rm -f "$t" "$i"' EXIT
pool="$(cell "$id" "$(word8_hex 1 0 0 0 0 0 0 0)")"
plan "$here" add --token "$(cat "$here/token.txt")" --pool "$pool" --rand "$r" --amount "$amount" ${3:+--lp "$3"} --t "$t" --i "$i" >/dev/null
"$RAND" program invoke "$id" --transition "$t" --inputs-file "$i"
