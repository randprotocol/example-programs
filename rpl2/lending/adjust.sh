#!/usr/bin/env bash
# Borrow against collateral, with the position position.secret owns (made on first use, mode 600):
#   ./adjust.sh [--deposit <C units>] [--withdraw <C units>|max] [--borrow <RAND units>|max] [--repay <RAND units>|all]
# e.g.  ./adjust.sh --deposit 50000000000 --borrow max      open: 50 C in, borrow the most (75 % LTV)
#       ./adjust.sh --repay all --withdraw max              close
# One payout per invoke: borrow or take collateral out, not both. Retries on a stale read.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
. "$here/../../scripts/env.sh"
need_rand
id="$(load_id "$here")"
[ $# -gt 0 ] || die "usage: ./adjust.sh [--deposit u] [--withdraw u|max] [--borrow u|max] [--repay u|all]"
[ -f "$here/position.secret" ] || new_secret "$here/position.secret"
key="$(plan "$here" key --secret "$here/position.secret")"
t="$(mktemp)"; i="$(mktemp)"; trap 'rm -f "$t" "$i"' EXIT
for attempt in 1 2 3; do
    price="$(cell "$id" "$(word8_hex 1 0 0 0 0 0 0 0)")"
    pool="$(cell "$id" "$(word8_hex 2 0 0 0 0 0 0 0)")"
    position="$(cell "$id" "$key")"
    plan "$here" adjust --public "$here/public.txt" --secret "$here/position.secret" \
        --price "$price" --pool "$pool" --position "$position" "$@" --t "$t" --i "$i" >/dev/null
    set +e; "$RAND" program invoke "$id" --transition "$t" --inputs-file "$i"; rc=$?; set -e
    [ "$rc" -eq 3 ] && { echo "stale read: the market moved; again (attempt $attempt)"; continue; }
    exit "$rc"
done
die "the market kept moving; try again"
