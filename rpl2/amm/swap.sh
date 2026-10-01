#!/usr/bin/env bash
# Swap: ./swap.sh rand <RAND> [min token units]    sell RAND for the token
#       ./swap.sh token <token units> [min RAND units]
# If the pool moves before the swap lands, the chain refuses it as a stale read (exit 3) and this
# script quotes again; the minimum protects you from the new price.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
. "$here/../../scripts/env.sh"
need_rand
id="$(load_id "$here")"
side="${1:?usage: ./swap.sh rand|token <amount> [min out]}"
case "$side" in
    rand) amount="$(rand_units "${2:?amount}")" ;;
    token) amount="${2:?amount}" ;;
    *) die "sell rand or token" ;;
esac
t="$(mktemp)"; i="$(mktemp)"; trap 'rm -f "$t" "$i"' EXIT
for attempt in 1 2 3; do
    pool="$(cell "$id" "$(word8_hex 1 0 0 0 0 0 0 0)")"
    plan "$here" swap --token "$(cat "$here/token.txt")" --pool "$pool" --sell "$side" --amount "$amount" --min-out "${3:-1}" --t "$t" --i "$i" >/dev/null
    set +e; "$RAND" program invoke "$id" --transition "$t" --inputs-file "$i"; rc=$?; set -e
    [ "$rc" -eq 3 ] && { echo "stale read: the pool moved; quoting again (attempt $attempt)"; continue; }
    exit "$rc"
done
die "the pool kept moving; try again"
