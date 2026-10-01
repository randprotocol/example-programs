#!/usr/bin/env bash
# Take a resting order back, whatever the price: ./cancel.sh <secret file> [rand1… recipient]
# Needs the ticket and the opening (the whole secret file); pays the escrow to the recipient
# (default: this wallet).
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
. "$here/../../scripts/env.sh"
need_rand
id="$(load_id "$here")"
secret="${1:?usage: ./cancel.sh <secret file> [recipient]}"
[ -f "$secret" ] || die "no $secret: only the holder of the ticket and the opening can cancel"
to="${2:-}"
key="$(plan "$here" key --secret "$secret")"
t="$(mktemp)"; i="$(mktemp)"; trap 'rm -f "$t" "$i"' EXIT
order="$(cell "$id" "$key")"
plan "$here" cancel --secret "$secret" --order "$order" ${to:+--to "$to"} --t "$t" --i "$i" >/dev/null
"$RAND" program invoke "$id" --transition "$t" --inputs-file "$i"
