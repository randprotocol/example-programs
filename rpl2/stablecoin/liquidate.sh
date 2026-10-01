#!/usr/bin/env bash
# Liquidate a position below 110 %: ./liquidate.sh <position key, 64 hex>   (./show.sh lists them)
# Burns its whole debt in the stable token (your wallet must hold that much) and pays you all of
# its collateral. Retries on a stale read; if the owner repaid or the price rose, plan refuses.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
. "$here/../../scripts/env.sh"
need_rand
id="$(load_id "$here")"
key="${1:?usage: ./liquidate.sh <position key hex>}"
[ -f "$here/public.txt" ] || die "no public.txt: it is the program's public input (the operator's lock)"
t="$(mktemp)"; i="$(mktemp)"; trap 'rm -f "$t" "$i"' EXIT
for attempt in 1 2 3; do
    config="$(cell "$id" "$(word8_hex 1 0 0 0 0 0 0 0)")"
    position="$(cell "$id" "$key")"
    plan "$here" liquidate --lock "$here/public.txt" --config "$config" --position "$position" --key "$key" \
        --t "$t" --i "$i" >/dev/null
    set +e; "$RAND" program invoke "$id" --transition "$t" --inputs-file "$i"; rc=$?; set -e
    [ "$rc" -eq 3 ] && { echo "stale read: the price or the position changed; planning again (attempt $attempt)"; continue; }
    exit "$rc"
done
die "the cells kept changing; try again"
