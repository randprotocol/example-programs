#!/usr/bin/env bash
# The operator sets the price: ./set-price.sh <price>
# The price is a whole number in whatever unit the operator and the owners agree on (say, cents
# per RAND). The first call creates the oracle; later ones replace the reading. The private
# inputs are [1, secret, old price, new price]; the secret never leaves this machine.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
. "$here/../../scripts/env.sh"
need_rand
id="$(load_id "$here")"
[ -f "$here/operator.secret" ] || die "no operator.secret: only the operator can set the price"
price="${1:?usage: ./set-price.sh <price>}"
t="$(mktemp)"; i="$(mktemp)"; trap 'rm -f "$t" "$i"' EXIT
for attempt in 1 2 3; do
    oracle="$(cell "$id" "$(word8_hex 1 0 0 0 0 0 0 0)")"
    plan "$here" operate --operator "$here/operator.secret" --oracle "$oracle" --price "$price" --t "$t" --i "$i" >/dev/null
    set +e; "$RAND" program invoke "$id" --transition "$t" --inputs-file "$i"; rc=$?; set -e
    [ "$rc" -eq 3 ] && { echo "stale read: the oracle moved; planning again (attempt $attempt)"; continue; }
    exit "$rc"
done
die "the oracle kept moving; try again"
