#!/usr/bin/env bash
# Fill an order: ./fill.sh <order key hex> <pay units>
# Pays <pay units> of the asset the order wants (at most what it still asks) and takes the most
# of its escrow that buys at the maker's price. If another fill lands first, the chain refuses
# this one as a stale read (exit 3) and this script quotes again: the order's remaining price
# never rises, so the new quote is never worse per unit (though there may be less left).
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
. "$here/../../scripts/env.sh"
need_rand
id="$(load_id "$here")"
key="${1:?usage: ./fill.sh <order key hex> <pay units>}"
pay="${2:?usage: ./fill.sh <order key hex> <pay units>}"
t="$(mktemp)"; i="$(mktemp)"; trap 'rm -f "$t" "$i"' EXIT
for attempt in 1 2 3; do
    order="$(cell "$id" "$key")"
    plan "$here" fill --key "$key" --order "$order" --pay "$pay" --t "$t" --i "$i" >/dev/null
    set +e; "$RAND" program invoke "$id" --transition "$t" --inputs-file "$i"; rc=$?; set -e
    [ "$rc" -eq 3 ] && { echo "stale read: the order moved; quoting again (attempt $attempt)"; continue; }
    exit "$rc"
done
die "the order kept moving; try again"
