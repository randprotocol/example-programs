#!/usr/bin/env bash
# Create the market or move its price (the operator only): ./price.sh <RAND per X> [LP asset — creating it]
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
. "$here/../../scripts/env.sh"
need_rand
id="$(load_id "$here")"
[ -f "$here/public.txt" ] || die "no public.txt: run ./operator.sh"
t="$(mktemp)"; i="$(mktemp)"; trap 'rm -f "$t" "$i"' EXIT
price="$(rand_units "${1:?usage: ./price.sh <RAND per X> [LP asset]}")"
[ -f "$here/operator.secret" ] || die "no operator.secret: only the operator sets the price"
# The market and the open interest are shared by everyone: if either moves before this lands, the
# chain refuses it as a stale read (exit 3) and this script plans again from the new cells.
for attempt in 1 2 3; do
    market="$(cell "$id" "$(word8_hex 1 0 0 0 0 0 0 0)")"
    oi="$(cell "$id" "$(word8_hex 2 0 0 0 0 0 0 0)")"
    plan "$here" operate --lock "$here/public.txt" --operator "$here/operator.secret" --market "$market" --price "$price" ${2:+--lp "$2"} --t "$t" --i "$i" >/dev/null
    set +e; "$RAND" program invoke "$id" --transition "$t" --inputs-file "$i"; rc=$?; set -e
    [ "$rc" -eq 3 ] && { echo "stale read: the market moved; planning again (attempt $attempt)"; continue; }
    exit "$rc"
done
die "the market kept moving; try again"
