#!/usr/bin/env bash
# The operator: ./operate.sh init <price> <share asset>      set the market up
#               ./operate.sh update <price> [rate]           move the price; accrue interest
# <price> is RAND per 10^9 units of the collateral (RAND per C for a 9-decimal token). [rate] is in
# parts per 10^9 of the index, at most 10000000 (1 %) per update; default 0. Retries on a stale read.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
. "$here/../../scripts/env.sh"
need_rand
id="$(load_id "$here")"
[ -f "$here/operator.secret" ] || die "no operator.secret: only the operator may do this"
mode="${1:?usage: ./operate.sh init <price> <share asset> | update <price> [rate]}"
price="$(rand_units "${2:?price}")"
case "$mode" in
    init) extra=(--share "${3:?share asset}") ;;
    update) extra=(--rate "${3:-0}") ;;
    *) die "init or update" ;;
esac
t="$(mktemp)"; i="$(mktemp)"; trap 'rm -f "$t" "$i"' EXIT
for attempt in 1 2 3; do
    pool="$(cell "$id" "$(word8_hex 2 0 0 0 0 0 0 0)")"
    plan "$here" operate --public "$here/public.txt" --operator "$here/operator.secret" --pool "$pool" \
        --new-price "$price" "${extra[@]}" --t "$t" --i "$i" >/dev/null
    set +e; "$RAND" program invoke "$id" --transition "$t" --inputs-file "$i"; rc=$?; set -e
    [ "$rc" -eq 3 ] && { echo "stale read: the pool moved; again (attempt $attempt)"; continue; }
    exit "$rc"
done
die "the pool kept moving; try again"
