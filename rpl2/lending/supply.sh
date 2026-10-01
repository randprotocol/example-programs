#!/usr/bin/env bash
# Lend: ./supply.sh <RAND> — RAND in, shares minted at the pool's share price. Retries on a stale read.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
. "$here/../../scripts/env.sh"
need_rand
id="$(load_id "$here")"
amount="$(rand_units "${1:?usage: ./supply.sh <RAND>}")"
t="$(mktemp)"; i="$(mktemp)"; trap 'rm -f "$t" "$i"' EXIT
for attempt in 1 2 3; do
    pool="$(cell "$id" "$(word8_hex 2 0 0 0 0 0 0 0)")"
    shares="$(cell "$id" "$(word8_hex 4 0 0 0 0 0 0 0)")"
    plan "$here" supply --public "$here/public.txt" --pool "$pool" --shares "$shares" --amount "$amount" --t "$t" --i "$i" >/dev/null
    set +e; "$RAND" program invoke "$id" --transition "$t" --inputs-file "$i"; rc=$?; set -e
    [ "$rc" -eq 3 ] && { echo "stale read: the pool moved; again (attempt $attempt)"; continue; }
    exit "$rc"
done
die "the pool kept moving; try again"
