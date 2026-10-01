#!/usr/bin/env bash
# Remove liquidity: ./remove.sh <share units> — burns them for RAND and the token, in proportion.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
. "$here/../../scripts/env.sh"
need_rand
id="$(load_id "$here")"
read -r token amp < "$here/public.txt"
shares="${1:?usage: ./remove.sh <share units>}"
t="$(mktemp)"; i="$(mktemp)"; trap 'rm -f "$t" "$i"' EXIT
for attempt in 1 2 3; do
    pool="$(cell "$id" "$(word8_hex 1 0 0 0 0 0 0 0)")"
    plan "$here" remove --token "$token" --amp "$amp" --pool "$pool" --shares "$shares" --t "$t" --i "$i" >/dev/null
    set +e; "$RAND" program invoke "$id" --transition "$t" --inputs-file "$i"; rc=$?; set -e
    [ "$rc" -eq 3 ] && { echo "stale read: the pool moved; planning again (attempt $attempt)"; continue; }
    exit "$rc"
done
die "the pool kept moving; try again"
