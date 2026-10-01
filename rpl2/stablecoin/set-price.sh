#!/usr/bin/env bash
# The operator sets the price: ./set-price.sh <stable units per RAND> [stable asset — first time only]
# The price is in the stable token's base units per whole RAND (2000000000 = 2.0 with 9 decimals).
# The first call binds the stable token for good. Needs operator.secret.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
. "$here/../../scripts/env.sh"
need_rand
id="$(load_id "$here")"
[ -f "$here/operator.secret" ] || die "no operator.secret: only the operator sets the price"
price="${1:?usage: ./set-price.sh <stable units per RAND> [stable asset]}"
t="$(mktemp)"; i="$(mktemp)"; trap 'rm -f "$t" "$i"' EXIT
for attempt in 1 2 3; do
    config="$(cell "$id" "$(word8_hex 1 0 0 0 0 0 0 0)")"
    plan "$here" operate --operator "$here/operator.secret" --lock "$here/public.txt" --config "$config" \
        --price "$price" ${2:+--stable "$2"} --t "$t" --i "$i" >/dev/null
    set +e; "$RAND" program invoke "$id" --transition "$t" --inputs-file "$i"; rc=$?; set -e
    [ "$rc" -eq 3 ] && { echo "stale read: the config changed; again (attempt $attempt)"; continue; }
    exit "$rc"
done
die "the config kept changing; try again"
