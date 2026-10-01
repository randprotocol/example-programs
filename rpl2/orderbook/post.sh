#!/usr/bin/env bash
# Post an order: ./post.sh <give asset> <give units> <want asset> <want units> [name]
#   e.g. ./post.sh 0 10000000000 3 25000000000     10 RAND (asset 0) for 25 units·10⁹ of token 3
# Amounts are base units (1 RAND = 10⁹). The give amount goes into the program's vault as escrow.
# Writes a new ticket, <name>.secret (mode 600) — the only key that closes the order, collecting
# what takers paid and what is left of the escrow: keep it. Prints the order key, for takers.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
. "$here/../../scripts/env.sh"
need_rand
id="$(load_id "$here")"
usage="usage: ./post.sh <give asset> <give units> <want asset> <want units> [name]"
give_asset="${1:?$usage}"; give="${2:?$usage}"; want_asset="${3:?$usage}"; want="${4:?$usage}"
ticket="$here/${5:-order-$(date +%Y%m%d-%H%M%S)}.secret"
new_secret "$ticket"
key="$(plan "$here" key --ticket "$ticket")"
t="$(mktemp)"; i="$(mktemp)"; trap 'rm -f "$t" "$i"' EXIT
order="$(cell "$id" "$key")"
plan "$here" post --ticket "$ticket" --order "$order" --give-asset "$give_asset" --give "$give" \
    --want-asset "$want_asset" --want "$want" --t "$t" --i "$i" >/dev/null
"$RAND" program invoke "$id" --transition "$t" --inputs-file "$i"
echo "order key: $key"
echo "ticket:    $ticket (close it with ./close.sh $ticket)"
