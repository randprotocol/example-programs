#!/usr/bin/env bash
# Fire an order whose condition holds: ./fire.sh <secret file> [rand1… recipient]
# Reads the oracle and the order, proves the opening matches the commitment and the price meets
# the trigger, and releases the escrow to the recipient (default: this wallet). If the condition
# does not hold, `plan` stops here: there is nothing to prove, and nothing is sent — nobody can
# tell the attempt was made. If the oracle moves before the fire lands, the chain refuses it as a
# stale read (exit 3) and this script plans again against the new price.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
. "$here/../../scripts/env.sh"
need_rand
id="$(load_id "$here")"
secret="${1:?usage: ./fire.sh <secret file> [recipient]}"
[ -f "$secret" ] || die "no $secret: only the holder of the ticket and the opening can fire"
to="${2:-}"
key="$(plan "$here" key --secret "$secret")"
t="$(mktemp)"; i="$(mktemp)"; trap 'rm -f "$t" "$i"' EXIT
for attempt in 1 2 3; do
    oracle="$(cell "$id" "$(word8_hex 1 0 0 0 0 0 0 0)")"
    order="$(cell "$id" "$key")"
    plan "$here" fire --secret "$secret" --oracle "$oracle" --order "$order" ${to:+--to "$to"} --t "$t" --i "$i" >/dev/null
    set +e; "$RAND" program invoke "$id" --transition "$t" --inputs-file "$i"; rc=$?; set -e
    [ "$rc" -eq 3 ] && { echo "stale read: the oracle or the order moved; planning again (attempt $attempt)"; continue; }
    exit "$rc"
done
die "the cells kept moving; try again"
