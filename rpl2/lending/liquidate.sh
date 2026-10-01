#!/usr/bin/env bash
# Liquidate a position past 85 % LTV: ./liquidate.sh <position key, 64 hex> <RAND to repay>
# Repays that much of its debt and takes collateral worth up to 110 % of the debt cleared.
# (./show.sh lists every position's key and value.) Retries on a stale read.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
. "$here/../../scripts/env.sh"
need_rand
id="$(load_id "$here")"
key="${1:?usage: ./liquidate.sh <position key> <RAND>}"
repay="$(rand_units "${2:?usage: ./liquidate.sh <position key> <RAND>}")"
t="$(mktemp)"; i="$(mktemp)"; trap 'rm -f "$t" "$i"' EXIT
for attempt in 1 2 3; do
    price="$(cell "$id" "$(word8_hex 1 0 0 0 0 0 0 0)")"
    pool="$(cell "$id" "$(word8_hex 2 0 0 0 0 0 0 0)")"
    position="$(cell "$id" "$key")"
    plan "$here" liquidate --public "$here/public.txt" --price "$price" --pool "$pool" --key "$key" \
        --position "$position" --repay "$repay" --t "$t" --i "$i" >/dev/null
    set +e; "$RAND" program invoke "$id" --transition "$t" --inputs-file "$i"; rc=$?; set -e
    [ "$rc" -eq 3 ] && { echo "stale read: the market moved; again (attempt $attempt)"; continue; }
    exit "$rc"
done
die "the market kept moving; try again"
