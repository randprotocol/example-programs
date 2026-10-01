#!/usr/bin/env bash
# Stop lending: ./withdraw.sh <share units> — burns them for RAND at the pool's share price (as much
# as the pool holds in cash: what is lent out comes back as borrowers repay). Retries on a stale read.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
. "$here/../../scripts/env.sh"
need_rand
id="$(load_id "$here")"
burn="${1:?usage: ./withdraw.sh <share units>}"
t="$(mktemp)"; i="$(mktemp)"; trap 'rm -f "$t" "$i"' EXIT
for attempt in 1 2 3; do
    pool="$(cell "$id" "$(word8_hex 2 0 0 0 0 0 0 0)")"
    shares="$(cell "$id" "$(word8_hex 4 0 0 0 0 0 0 0)")"
    plan "$here" withdraw --public "$here/public.txt" --pool "$pool" --shares "$shares" --burn "$burn" --t "$t" --i "$i" >/dev/null
    set +e; "$RAND" program invoke "$id" --transition "$t" --inputs-file "$i"; rc=$?; set -e
    [ "$rc" -eq 3 ] && { echo "stale read: the pool moved; again (attempt $attempt)"; continue; }
    exit "$rc"
done
die "the pool kept moving; try again"
