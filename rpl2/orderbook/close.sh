#!/usr/bin/env bash
# Close an order: ./close.sh <ticket file>
# Deletes the order and pays this wallet what is left of the escrow and what takers paid in —
# a cancel, a claim, or both. Retries if a fill lands first (a stale read, exit 3).
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
. "$here/../../scripts/env.sh"
need_rand
id="$(load_id "$here")"
ticket="${1:?usage: ./close.sh <ticket file>}"
[ -f "$ticket" ] || die "no ticket at $ticket"
key="$(plan "$here" key --ticket "$ticket")"
t="$(mktemp)"; i="$(mktemp)"; trap 'rm -f "$t" "$i"' EXIT
for attempt in 1 2 3; do
    order="$(cell "$id" "$key")"
    plan "$here" close --ticket "$ticket" --order "$order" --t "$t" --i "$i" >/dev/null
    set +e; "$RAND" program invoke "$id" --transition "$t" --inputs-file "$i"; rc=$?; set -e
    [ "$rc" -eq 3 ] && { echo "stale read: a fill landed first; closing again (attempt $attempt)"; continue; }
    [ "$rc" -eq 0 ] && echo "closed; $ticket now names nothing and may be deleted"
    exit "$rc"
done
die "the order kept moving; try again"
