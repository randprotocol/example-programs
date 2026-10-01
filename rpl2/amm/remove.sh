#!/usr/bin/env bash
# Remove liquidity: ./remove.sh <share units> — burns them for RAND and the token, in proportion.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
. "$here/../../scripts/env.sh"
need_rand
id="$(load_id "$here")"
shares="${1:?usage: ./remove.sh <share units>}"
t="$(mktemp)"; i="$(mktemp)"; trap 'rm -f "$t" "$i"' EXIT
pool="$(cell "$id" "$(word8_hex 1 0 0 0 0 0 0 0)")"
plan "$here" remove --token "$(cat "$here/token.txt")" --pool "$pool" --shares "$shares" --t "$t" --i "$i" >/dev/null
"$RAND" program invoke "$id" --transition "$t" --inputs-file "$i"
