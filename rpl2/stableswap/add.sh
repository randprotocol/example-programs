#!/usr/bin/env bash
# Add liquidity: ./add.sh <RAND> <token units> [share asset — the first deposit only]
# Either side may be 0 once the pool exists (a one-sided add pays the 0.04 % add fee like any
# other). If the pool moves first, the chain refuses the stale read (exit 3) and this re-plans.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
. "$here/../../scripts/env.sh"
need_rand
id="$(load_id "$here")"
read -r token amp < "$here/public.txt"
r="$(rand_units "${1:?usage: ./add.sh <RAND> <token units> [share asset]}")"
amount="${2:?usage: ./add.sh <RAND> <token units> [share asset]}"
t="$(mktemp)"; i="$(mktemp)"; trap 'rm -f "$t" "$i"' EXIT
for attempt in 1 2 3; do
    pool="$(cell "$id" "$(word8_hex 1 0 0 0 0 0 0 0)")"
    plan "$here" add --token "$token" --amp "$amp" --pool "$pool" --rand "$r" --amount "$amount" ${3:+--lp "$3"} --t "$t" --i "$i" >/dev/null
    set +e; "$RAND" program invoke "$id" --transition "$t" --inputs-file "$i"; rc=$?; set -e
    [ "$rc" -eq 3 ] && { echo "stale read: the pool moved; planning again (attempt $attempt)"; continue; }
    exit "$rc"
done
die "the pool kept moving; try again"
