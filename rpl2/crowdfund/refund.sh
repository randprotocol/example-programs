#!/usr/bin/env bash
# Take a pledge back: ./refund.sh <receipt units>
# Burns that many receipts from this wallet and pays the same number of RAND units back. Open until
# the creator claims (there is no deadline: see the README). Retries on a stale read (exit 3).
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
. "$here/../../scripts/env.sh"
need_rand
id="$(load_id "$here")"
units="${1:?usage: ./refund.sh <receipt units>}"
t="$(mktemp)"; i="$(mktemp)"; trap 'rm -f "$t" "$i"' EXIT
for attempt in 1 2 3; do
    campaign="$(cell "$id" "$(word8_hex 1 0 0 0 0 0 0 0)")"
    plan "$here" refund --amount "$units" --public "$here/public.txt" --campaign "$campaign" --t "$t" --i "$i" >/dev/null
    set +e; "$RAND" program invoke "$id" --transition "$t" --inputs-file "$i"; rc=$?; set -e
    [ "$rc" -eq 3 ] && { echo "stale read: the campaign moved; planning again (attempt $attempt)"; continue; }
    exit "$rc"
done
die "the campaign kept moving; try again"
